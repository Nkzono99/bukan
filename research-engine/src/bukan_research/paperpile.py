"""Paperpile's public Paste UI, isolated from both the research DB and synced PDFs.

Only the native Bukan bridge supplies paths and validated import payloads. Login
uses ordinary Chrome; automation subsequently reuses that dedicated profile.
No private APIs, copied cookies, browser extensions or OS clipboard are needed.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

from filelock import FileLock, Timeout as LockTimeout
from playwright.sync_api import Error as BrowserError, sync_playwright

APP_URL = "https://paperpile.com/app"
REFERENCE_COUNT = re.compile(r"^(\d+) references?$", re.MULTILINE)
DUPLICATES = re.compile(r"Skip (\d+) duplicates?", re.IGNORECASE)


def chrome_profile_path(profile: Path) -> str:
    """Keep native path validation intact; pass Chrome a Win32 path spelling.

    Rust canonicalization produces verbatim Windows paths. Chrome starts with
    those paths, but its IndexedDB backing store fails to open (including short
    paths). Removing the namespace prefix names the same existing directory.
    """
    path = str(profile)
    if sys.platform == "win32":
        if path.startswith("\\\\?\\UNC\\"):
            return "\\\\" + path[8:]
        if re.match(r"^\\\\\?\\[A-Za-z]:\\", path):
            return path[4:]
    return path


def chrome_executable() -> Path | None:
    candidates = []
    if sys.platform == "win32":
        for name in ("PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"):
            if os.environ.get(name):
                candidates.append(Path(os.environ[name]) / "Google/Chrome/Application/chrome.exe")
    elif sys.platform == "darwin":
        candidates.append(Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"))
    else:
        executable = shutil.which("google-chrome") or shutil.which("google-chrome-stable")
        if executable:
            candidates.append(Path(executable))
    return next((path for path in candidates if path.is_file()), None)


class PaperpileUI:
    def __init__(self, page, *, timeout_ms: int = 30_000):
        self.page = page
        self.timeout_ms = timeout_ms

    def ready(self) -> bool:
        try:
            # Paperpile renders the toolbar underneath its loading overlay.
            # Visible buttons alone do not establish a usable/authenticated UI.
            self.page.get_by_role("button", name="Add", exact=True).click(
                trial=True, timeout=self.timeout_ms
            )
            return self.page.get_by_role("button", name="My Library", exact=True).is_visible()
        except BrowserError:
            return False

    def preview(self, text: str) -> dict:
        self.page.get_by_role("button", name="Add", exact=True).click()
        self.page.get_by_role("menuitem", name=re.compile(r"^Paste…")).click()
        dialog = self.page.get_by_role("dialog")
        textbox = dialog.get_by_role("textbox")
        textbox.wait_for(state="visible")
        # Paperpile listens for paste; fill() alone intentionally does not parse.
        textbox.evaluate("""(element, text) => {
            const data = new DataTransfer();
            data.setData('text/plain', text);
            element.dispatchEvent(new ClipboardEvent('paste', {
                clipboardData: data, bubbles: true, cancelable: true
            }));
        }""", text)
        dialog.get_by_role("button", name="Select all", exact=True).wait_for(
            state="visible", timeout=self.timeout_ms
        )
        skip = dialog.get_by_role("checkbox", name=re.compile(r"^Skip .*duplicates?", re.I))
        if skip.count():
            skip.check()
        dialog.get_by_role("button", name="Select all", exact=True).click()
        content = dialog.inner_text()
        count = REFERENCE_COUNT.search(content)
        if not count:
            raise ValueError("Unrecognized Paperpile reference count; nothing submitted.")
        duplicates = DUPLICATES.search(content)
        result = {
            "newCount": int(count[1]),
            "duplicateCount": int(duplicates[1]) if duplicates else 0,
            "allDuplicates": "All pasted references are duplicates" in content,
        }
        # Initial support is deliberately scoped to personal My Library. Never
        # silently fall back from a requested shared library or folder.
        destination = dialog.get_by_role("combobox")
        if destination.inner_text().strip() != "My Library":
            raise ValueError("The import destination is not My Library; nothing submitted.")
        if result["duplicateCount"] and not skip.is_checked():
            raise ValueError("Paperpile duplicate skipping could not be enabled.")
        return result

    def cancel(self) -> None:
        self.page.get_by_role("dialog").get_by_role("button", name="Cancel", exact=True).click()
        self.page.get_by_role("dialog").wait_for(state="hidden")

    def import_references(self, request: dict) -> dict:
        """No retry after submission. A second *preview* verifies live presence."""
        submitted = False
        before = None
        try:
            before = self.preview(request["pasteText"])
            expected = request["referenceCount"]
            if before["newCount"] + before["duplicateCount"] != expected:
                self.cancel()
                return {
                    "status": "preview_mismatch", "submissionAttempted": False,
                    "preview": before, "expectedCount": expected,
                    "detail": "Paperpile did not resolve exactly the requested number of references. Nothing imported.",
                }
            if before["allDuplicates"] and before["newCount"] == 0:
                self.cancel()
                return {"status": "already_present", "submissionAttempted": False,
                        "presentCount": expected, "preview": before,
                        "serverSyncVerified": False, "verificationScope": "dedicated-browser"}
            if before["newCount"] == 0:
                raise ValueError("No importable references in the preview.")
            if request.get("previewOnly", False):
                self.cancel()
                return {"status": "preview_only", "submissionAttempted": False,
                        "preview": before, "serverSyncVerified": False}
            # A timeout at click can occur after the remote mutation, so mark the
            # outcome uncertain before issuing it, not after click() returns.
            submitted = True
            self.page.get_by_role("dialog").get_by_role("button", name="Import", exact=True).click()
            self.page.get_by_role("dialog").wait_for(state="hidden", timeout=self.timeout_ms)
            # Reload checks persisted browser-library state. Paperpile is local
            # first: this does NOT prove background synchronization to its server.
            # No second Import is ever issued.
            self.page.reload(wait_until="domcontentloaded")
            if not self.ready():
                raise ValueError("Could not reopen the library after submission.")
            after = self.preview(request["pasteText"])
            self.cancel()
            if not (after["allDuplicates"] and after["newCount"] == 0
                    and after["duplicateCount"] == expected):
                return {"status": "unknown", "submissionAttempted": True,
                        "preview": before, "verification": after,
                        "detail": "Import was submitted, but full library presence was not verified. Do not report success."}
            return {"status": "present_in_browser", "submissionAttempted": True,
                    "presentCount": expected, "newlyPresentCount": before["newCount"],
                    "alreadyPresentCount": before["duplicateCount"],
                    "preview": before, "verification": after,
                    "serverSyncVerified": False, "verificationScope": "dedicated-browser",
                    "detail": "All references are present after reloading this browser. Paperpile syncs its local database in the background; server persistence has NOT been verified."}
        except (BrowserError, ValueError):
            # Raw browser exceptions can include page details. Keep protocol
            # output small and never expose authentication pages or tokens.
            return {"status": "unknown" if submitted else "ui_unavailable",
                    "submissionAttempted": submitted, "preview": before,
                    "detail": "Could not verify the Paperpile UI. Recheck live duplicates before retrying; an uncertain submission may have committed." if submitted
                    else "Paperpile's Paste UI could not be verified. Nothing submitted; check login or UI changes."}


def operate(action: str, profile: Path, request: dict) -> dict:
    chrome = chrome_executable()
    if not chrome:
        return {"status": "chrome_missing", "detail": "Install Google Chrome, then run: bukan paperpile login"}
    if action != "login" and not profile.is_dir():
        return {"status": "login_required", "detail": "Run: bukan paperpile login"}
    profile.parent.mkdir(parents=True, exist_ok=True)
    try:
        with FileLock(str(profile.parent / "browser.lock"), timeout=0):
            if action == "login":
                # Ordinary Chrome supports interactive Google login without
                # copying an existing browser profile or extracting credentials.
                print("Sign in to Paperpile in the dedicated Chrome window, then close that window. Your normal Chrome is separate.", file=sys.stderr, flush=True)
                process = subprocess.Popen([
                    str(chrome), f"--user-data-dir={chrome_profile_path(profile)}", "--no-first-run",
                    "--no-default-browser-check", APP_URL,
                ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    process.wait(timeout=900)
                except subprocess.TimeoutExpired:
                    return {"status": "login_window_open", "detail": "Close the dedicated Chrome window, then run bukan paperpile status. No browser was forcibly closed."}
            with sync_playwright() as playwright:
                context = playwright.chromium.launch_persistent_context(
                    chrome_profile_path(profile), executable_path=str(chrome), headless=True,
                    chromium_sandbox=True, timeout=20_000,
                    # Match ordinary Chrome's OS credential store used at login.
                    ignore_default_args=["--password-store=basic", "--use-mock-keychain"],
                    viewport={"width": 1400, "height": 1000},
                )
                try:
                    context.set_default_timeout(10_000)
                    page = context.pages[0] if context.pages else context.new_page()
                    page.goto(APP_URL, wait_until="domcontentloaded", timeout=30_000)
                    ui = PaperpileUI(page)
                    if not ui.ready():
                        return {"status": "login_required", "detail": "Library not ready. Check network and sign in using: bukan paperpile login"}
                    if action in ("login", "status"):
                        return {"status": "ready", "detail": "The dedicated browser can access Paperpile My Library."}
                    result = ui.import_references(request)
                    result["destination"] = "My Library"
                    result["serverSyncVerified"] = False
                    result["pdfSyncVerified"] = False
                    return result
                finally:
                    try:
                        context.close()
                    except BrowserError:
                        pass  # Closing a dead browser must not replace a verified/unknown result.
    except LockTimeout:
        return {"status": "busy", "detail": "Another Paperpile operation or login owns the dedicated browser. Finish it before retrying."}
    except (BrowserError, OSError):
        return {"status": "browser_unavailable", "detail": "Close the dedicated Chrome window and retry. If needed, run bukan paperpile login. The normal Chrome profile is never used."}


def main() -> None:
    action, raw_profile = sys.argv[1:]
    if action not in ("login", "status", "import"):
        raise ValueError("Unknown browser action")
    request = json.load(sys.stdin) if action == "import" else {}
    print(json.dumps(operate(action, Path(raw_profile), request), ensure_ascii=False))


if __name__ == "__main__":
    main()
