"""Toolkit onboarding boundaries; all installs and profiles are disposable fixtures."""

import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import install_toolkit as installer
from install_local import digest, verify_bundle


SCRIPTS = Path(__file__).resolve().parent


class ToolkitInstallerTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "Win32 canonical prefix")
    def test_rust_canonical_path_uses_same_profile_root(self):
        canonical = Path("\\\\?\\" + str(self.data))
        self.assertEqual(installer.ordinary_destination(canonical), self.data)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bukan-windows-install-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.bundle = self.root / "release" / "bukan"
        self.profile = self.root / "profile"
        self.data = self.profile / "AppData" / "Local" / "bukan"
        self.dependencies = self.data / "dependencies"
        self.files = {
            "bin/bukan.exe": "must never execute this fixture",
            ".codex-plugin/plugin.json": json.dumps({"name": "bukan", "version": "0.2.2"}),
            ".mcp.json": json.dumps({"mcpServers": {
                "bukan-library": {"command": "bukan", "args": ["mcp"]},
                "bukan-research": {"command": "bukan", "args": ["research-mcp"]},
            }}),
            "research-engine/pyproject.toml": '[project]\nname = "bukan-research"\n',
            "research-engine/uv.lock": "version = 1\n",
            "research-engine/src/bukan_research/cli.py": "def main(): pass\n",
            "skills/bukan-paper-review/SKILL.md": "# Paper review\n",
            "skills/bukan-setup/SKILL.md": "# Setup\n",
            "install_toolkit.py": "# Never executed by a rejection test.\n",
            "install_local.py": "# Never executed by a rejection test.\n",
            "windows-dependencies.json": "{}",
        }
        for relative, content in self.files.items():
            self.write(self.bundle / relative, content)
        self.refresh_manifest()

    def write(self, path, content):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def refresh_manifest(self):
        metadata = {
            "formatVersion": 1,
            "version": "0.2.2",
            "target": "x86_64-pc-windows-msvc",
            "binary": "bin/bukan.exe",
            "files": {relative: digest(self.bundle / relative) for relative in self.files},
        }
        self.write(self.bundle / "bundle.json", json.dumps(metadata))
        return metadata

    def archive(self, name="poppler", files=None):
        files = files or {
            "poppler/Library/bin/pdfinfo.exe": b"fixture pdfinfo",
            "poppler/Library/bin/pdftotext.exe": b"fixture pdftotext",
            "poppler/Library/bin/pdftoppm.exe": b"fixture pdftoppm",
            "poppler/Library/bin/poppler.dll": b"required shared library",
            "poppler/Library/share/poppler/COPYING": b"license notice",
            "poppler/Library/share/poppler/cMap/Adobe-Japan1": b"required data",
        }
        path = self.root / f"{name}.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for relative, contents in files.items():
                entry = zipfile.ZipInfo(relative)
                # ZipInfo normalizes backslashes on Windows; retain the raw
                # hostile filename to exercise the extraction boundary.
                entry.filename = relative
                archive.writestr(entry, contents)
        return path, {
            "version": "1.0.0",
            "url": f"https://example.invalid/{name}.zip",
            "sha256": digest(path),
            "bin": "poppler/Library/bin" if name == "poppler" else ".",
            "executables": ["pdfinfo.exe", "pdftotext.exe", "pdftoppm.exe"] if name == "poppler" else ["uv.exe"],
            "notices": [],
        }, files

    def snapshot(self, root):
        return {path.relative_to(root).as_posix(): (path.read_bytes(), path.stat().st_mtime_ns)
                for path in root.rglob("*") if path.is_file()}

    def storage_paths(self, selected=None):
        return {"dataDir": str(self.data), "configDir": str(self.profile / "config"),
                "cacheDir": str(self.profile / "cache"),
                "managedWorkspace": str(self.data / "workspaces/default"),
                "selectedWorkspace": str(selected) if selected else None}

    def test_corrupt_bundle_does_not_create_installation(self):
        (self.bundle / "bin/bukan.exe").write_bytes(b"modified after packaging")
        with self.assertRaisesRegex(ValueError, "checksum"):
            installer.install_toolkit(self.bundle, self.data, {})
        self.assertFalse(self.data.exists())

    def test_toolkit_rerun_preserves_source_and_reuses_installation(self):
        original = self.snapshot(self.bundle)
        overrides = {"BUKAN_DATA_DIR": str(self.data), "BUKAN_WORKSPACE": str(self.profile / "existing-research")}
        destination = installer.install_toolkit(self.bundle, self.data, overrides)
        metadata, _, mcp = verify_bundle(destination)
        self.assertEqual(metadata["version"], "0.2.2")
        for server in mcp["mcpServers"].values():
            self.assertEqual(server["command"], str(destination / "bin/bukan.exe"))
            self.assertEqual(server["env"], overrides)
        version = installer.read_json(destination / ".codex-plugin/plugin.json")["version"]
        self.assertTrue(version.startswith("0.2.2+codex."))
        installed = self.snapshot(destination)
        self.assertEqual(installer.install_toolkit(self.bundle, self.data, dict(reversed(list(overrides.items())))), destination)
        self.assertEqual(self.snapshot(destination), installed)
        self.assertEqual(self.snapshot(self.bundle), original)
        updated = installer.install_toolkit(self.bundle, self.data, dict(overrides, BUKAN_WORKSPACE=str(self.profile / "second-research")))
        self.assertNotEqual(updated, destination)
        self.assertNotEqual(installer.read_json(updated / ".codex-plugin/plugin.json")["version"], version)
        self.assertEqual(self.snapshot(destination), installed)

    def test_damaged_existing_toolkit_is_not_overwritten(self):
        destination = installer.install_toolkit(self.bundle, self.data, {})
        binary = destination / "bin/bukan.exe"
        binary.write_bytes(b"unexpected changed executable")
        with self.assertRaisesRegex(ValueError, "checksum"):
            installer.install_toolkit(self.bundle, self.data, {})
        self.assertEqual(binary.read_bytes(), b"unexpected changed executable")

    def test_dependency_preserves_dlls_data_and_notices_and_verifies_reruns(self):
        archive, asset, files = self.archive()
        destination = installer.ensure_dependency(self.dependencies, "poppler", asset, archive)
        for relative, contents in files.items():
            self.assertEqual((destination / relative).read_bytes(), contents)
        original = self.snapshot(destination)
        with patch.object(installer, "download", side_effect=AssertionError("rerun must be offline")):
            self.assertEqual(installer.ensure_dependency(self.dependencies, "poppler", asset), destination)
        self.assertEqual(self.snapshot(destination), original)
        dll = destination / "poppler/Library/bin/poppler.dll"
        dll.write_bytes(b"damaged")
        with self.assertRaisesRegex(ValueError, "damaged"):
            installer.ensure_dependency(self.dependencies, "poppler", asset)
        self.assertEqual(dll.read_bytes(), b"damaged")

    def test_dependency_checksum_failure_does_not_publish_destination(self):
        archive, asset, _ = self.archive()
        archive.write_bytes(archive.read_bytes() + b"changed")
        with self.assertRaisesRegex(ValueError, "checksum"):
            installer.ensure_dependency(self.dependencies, "poppler", asset, archive)
        self.assertFalse((self.dependencies / "poppler" / asset["version"]).exists())

    def test_dependency_missing_executable_does_not_publish_destination(self):
        archive, asset, _ = self.archive(files={"poppler/Library/bin/poppler.dll": b"library only"})
        with self.assertRaisesRegex(ValueError, "Missing bundle file"):
            installer.ensure_dependency(self.dependencies, "poppler", asset, archive)
        self.assertFalse((self.dependencies / "poppler" / asset["version"]).exists())

    def test_dependency_archive_traversal_and_symlink_rejected_before_extraction(self):
        invalid_names = ("../escaped", "/absolute", "C:/absolute", "..\\escaped", "folder/../../escaped", "folder//alias")
        for index, name in enumerate(invalid_names):
            with self.subTest(name=name):
                archive, _, _ = self.archive(name=f"invalid-{index}", files={"ordinary.txt": b"safe", name: b"unsafe"})
                destination = self.root / f"extracted-{index}"
                with self.assertRaisesRegex(ValueError, "Invalid dependency archive path"):
                    installer.extract_archive(archive, destination)
                self.assertFalse(destination.exists())
        archive = self.root / "linked.zip"
        with zipfile.ZipFile(archive, "w") as zipped:
            link = zipfile.ZipInfo("linked")
            link.create_system = 3
            link.external_attr = 0o120777 << 16
            zipped.writestr(link, "../outside")
        with self.assertRaisesRegex(ValueError, "Invalid dependency archive path"):
            installer.extract_archive(archive, self.root / "linked-extracted")
        self.assertFalse((self.root / "linked-extracted").exists())

    def test_download_checks_https_and_digest(self):
        output = self.root / "download.zip"
        with patch.object(installer, "urlopen") as request:
            with self.assertRaisesRegex(ValueError, "HTTPS"):
                installer.download("http://example.invalid/asset", output, "0" * 64)
            request.assert_not_called()
        with patch.object(installer, "urlopen", return_value=io.BytesIO(b"unexpected bytes")):
            with self.assertRaisesRegex(ValueError, "checksum"):
                installer.download("https://example.invalid/asset", output, "0" * 64)

    def test_marketplace_preserves_existing_entries_settings_order_and_backup(self):
        path = self.profile / ".agents/plugins/marketplace.json"
        original = {
            "name": "my-personal", "interface": {"displayName": "My plugins", "custom": "keep"},
            "owner": {"name": "keep owner"},
            "plugins": [
                {"name": "first", "source": {"source": "local", "path": "./plugins/first"}},
                {"name": "bukan", "source": {"source": "local", "path": "./plugins/bukan"},
                 "category": "Science", "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"}, "custom": "keep"},
                {"name": "last", "source": {"source": "local", "path": "./plugins/last"}},
            ],
        }
        self.write(path, json.dumps(original, indent=4))
        original_bytes = path.read_bytes()
        destination = self.data / "installations/fixture/bukan"
        result_path, marketplace = installer.marketplace_entry(self.profile, destination)
        self.assertEqual(result_path, path)
        self.assertEqual(marketplace["name"], original["name"])
        self.assertEqual(marketplace["interface"], original["interface"])
        self.assertEqual(marketplace["owner"], original["owner"])
        self.assertEqual([entry["name"] for entry in marketplace["plugins"]], ["first", "bukan", "last"])
        for index in (0, 2):
            self.assertEqual(marketplace["plugins"][index], original["plugins"][index])
        expected_entry = dict(original["plugins"][1], source={"source": "local", "path": "./AppData/Local/bukan/installations/fixture/bukan"})
        self.assertEqual(marketplace["plugins"][1], expected_entry)
        installer.register_marketplace(path, marketplace)
        backups = list(path.parent.glob("marketplace.json.bukan-backup-*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(backups[0].read_bytes(), original_bytes)
        updated = self.snapshot(path.parent)
        installer.register_marketplace(*installer.marketplace_entry(self.profile, destination))
        self.assertEqual(self.snapshot(path.parent), updated)

    def test_new_marketplace_and_invalid_marketplace_do_not_replace_user_content(self):
        destination = self.data / "installations/fixture/bukan"
        path, marketplace = installer.marketplace_entry(self.profile, destination)
        self.assertFalse(path.exists())
        self.assertEqual(marketplace["name"], "personal")
        self.assertEqual([entry["name"] for entry in marketplace["plugins"]], ["bukan"])
        for invalid in ("not json", json.dumps({"name": "personal", "plugins": ["bad entry"]}),
                        json.dumps({"name": "personal", "plugins": [{"name": "bukan"}, {"name": "bukan"}]})):
            with self.subTest(invalid=invalid):
                self.write(path, invalid)
                with self.assertRaises(ValueError):
                    installer.marketplace_entry(self.profile, destination)
                self.assertEqual(path.read_text(encoding="utf-8"), invalid)
        self.assertFalse(list(path.parent.glob("marketplace.json.bukan-backup-*")))

    def test_paperpile_workspace_and_external_profile_destinations_rejected(self):
        workspace = self.root / "existing-research"
        self.write(workspace / "bukan.toml", "version = 1\n")
        library = self.root / "synced-library"
        (library / "All Papers").mkdir(parents=True)
        for destination in (workspace / "tools", library / "tools", self.root / "Paperpile/tools"):
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                installer.ordinary_destination(destination)
            self.assertFalse(destination.exists())
        with self.assertRaises(ValueError):
            installer.marketplace_entry(self.profile, self.root / "external-tools")
        self.assertFalse((self.profile / ".agents").exists())

    def test_invalid_marketplace_stops_install_before_dependency_changes(self):
        path = self.write(self.profile / ".agents/plugins/marketplace.json", "invalid existing configuration")
        paths = self.storage_paths()
        with patch.object(installer.subprocess, "check_output", return_value=json.dumps(paths)), \
                patch.object(installer, "ensure_dependency") as dependency, \
                patch.object(installer.sys, "platform", "win32"), \
                patch.object(installer.platform, "machine", return_value="AMD64"), \
                patch.object(installer.subprocess, "run") as setup:
            with self.assertRaises(ValueError):
                installer.install(self.bundle, profile=self.profile)
        dependency.assert_not_called()
        setup.assert_not_called()
        self.assertEqual(path.read_text(encoding="utf-8"), "invalid existing configuration")
        self.assertFalse(self.data.exists())

    @unittest.skipUnless(os.name == "nt", "Windows directory junction")
    def test_linked_destinations_and_marketplace_are_not_modified(self):
        outside = self.root / "outside"
        outside.mkdir()
        linked = self.root / "linked"
        subprocess.run(["cmd", "/c", "mklink", "/J", str(linked), str(outside)], check=True, capture_output=True)
        self.addCleanup(linked.rmdir)
        self.write(outside / "keep.txt", "keep")
        original = self.snapshot(outside)
        with self.assertRaisesRegex(ValueError, "Linked"):
            installer.write_json(linked / "new.json", {"must": "not write"})
        with self.assertRaisesRegex(ValueError, "Linked"):
            installer.marketplace_entry(linked, outside / "tools")
        self.assertEqual(self.snapshot(outside), original)

    def test_full_install_uses_saved_workspace_setup_and_captures_only_bukan_overrides(self):
        uv_archive, uv_asset, _ = self.archive("uv", {"uv.exe": b"fixture uv"})
        poppler_archive, poppler_asset, _ = self.archive()
        self.write(self.bundle / "windows-dependencies.json", json.dumps({"uv": uv_asset, "poppler": poppler_asset}))
        self.refresh_manifest()
        existing_workspace = self.profile / "existing-research"
        self.write(existing_workspace / "research.sqlite3", "existing research bytes")
        research_before = self.snapshot(existing_workspace)
        paths = self.storage_paths(existing_workspace)
        overrides = {"BUKAN_DATA_DIR": str(self.data), "BUKAN_CONFIG_DIR": str(self.profile / "config"),
                     "BUKAN_CACHE_DIR": str(self.profile / "cache"), "BUKAN_WORKSPACE": str(existing_workspace)}

        def copy_download(url, destination, expected):
            self.assertEqual(url, poppler_asset["url"])
            self.assertEqual(expected, digest(poppler_archive))
            shutil.copyfile(poppler_archive, destination)

        with patch.dict(os.environ, dict(overrides, UNRELATED_SECRET="must not be captured"), clear=True), \
                patch.object(installer.sys, "platform", "win32"), \
                patch.object(installer.platform, "machine", return_value="AMD64"), \
                patch.object(installer.subprocess, "check_output", return_value=json.dumps(paths)) as command, \
                patch.object(installer.subprocess, "run") as setup, \
                patch.object(installer, "download", side_effect=copy_download), \
                patch("sys.stdout", new_callable=io.StringIO) as output:
            executable = installer.install(self.bundle, uv_archive, profile=self.profile)
        command.assert_called_once_with([str(self.bundle / "bin/bukan.exe"), "paths", "--json"], encoding="utf-8")
        setup.assert_called_once_with([str(executable), "setup"], check=True)
        self.assertIn(str(existing_workspace), output.getvalue())
        self.assertIn("codex plugin add bukan@personal", output.getvalue())
        for server in installer.read_json(executable.parent.parent / ".mcp.json")["mcpServers"].values():
            self.assertEqual(server["env"], overrides)
        pointer = installer.read_json(self.dependencies / "current.json")
        self.assertEqual(pointer["version"], 1)
        self.assertTrue((self.dependencies / pointer["uv"]).is_file())
        self.assertTrue((self.dependencies / pointer["popplerBin"] / "pdftotext.exe").is_file())
        self.assertEqual(self.snapshot(existing_workspace), research_before)
        self.assertFalse((self.data / "workspaces/default").exists())

    def make_linux_bundle(self):
        self.files["bin/bukan"] = self.files.pop("bin/bukan.exe")
        (self.bundle / "bin/bukan.exe").rename(self.bundle / "bin/bukan")
        metadata = self.refresh_manifest()
        metadata.update(target="x86_64-unknown-linux-gnu", binary="bin/bukan")
        self.write(self.bundle / "bundle.json", json.dumps(metadata))

    def test_incompatible_platform_fails_before_running_binary_or_writing(self):
        before = self.snapshot(self.root)
        with patch.object(installer.sys, "platform", "darwin"), \
                patch.object(installer.subprocess, "check_output") as command, \
                patch.object(installer.subprocess, "run") as setup:
            with self.assertRaisesRegex(ValueError, "requires win32 x86_64"):
                installer.install(self.bundle, profile=self.profile)
        command.assert_not_called()
        setup.assert_not_called()
        self.assertEqual(self.snapshot(self.root), before)

    def test_linux_missing_prerequisites_fail_before_running_binary_or_writing(self):
        self.make_linux_bundle()
        before = self.snapshot(self.root)
        for missing, diagnostic in (("pdftoppm", "poppler-utils"), ("uv", "python -m bukan install")):
            with self.subTest(missing=missing), \
                    patch.object(installer.sys, "platform", "linux"), \
                    patch.object(installer.platform, "machine", return_value="x86_64"), \
                    patch.object(installer.shutil, "which", side_effect=lambda tool: None if tool == missing else f"/usr/bin/{tool}"), \
                    patch.object(installer.subprocess, "check_output") as command, \
                    patch.object(installer.subprocess, "run") as setup:
                with self.assertRaisesRegex(ValueError, diagnostic):
                    installer.install(self.bundle, profile=self.profile)
                command.assert_not_called()
                setup.assert_not_called()
                self.assertEqual(self.snapshot(self.root), before)

    def test_linux_install_uses_system_poppler_and_preserves_research(self):
        self.make_linux_bundle()
        self.data = self.profile / ".local/share/bukan"
        existing_workspace = self.profile / "existing-research"
        self.write(existing_workspace / "research.sqlite3", "existing research bytes")
        research_before = self.snapshot(existing_workspace)
        paths = self.storage_paths(existing_workspace)
        with patch.dict(os.environ, {}, clear=True), \
                patch.object(installer.sys, "platform", "linux"), \
                patch.object(installer.platform, "machine", return_value="x86_64"), \
                patch.object(installer.shutil, "which", side_effect=lambda tool: f"/usr/bin/{tool}"), \
                patch.object(installer.subprocess, "check_output", return_value=json.dumps(paths)) as command, \
                patch.object(installer.subprocess, "run") as setup, \
                patch.object(installer, "ensure_dependency") as dependency, \
                patch("sys.stdout", new_callable=io.StringIO) as output:
            executable = installer.install(self.bundle, profile=self.profile)
        self.assertEqual(executable.name, "bukan")
        command.assert_called_once_with([str(self.bundle / "bin/bukan"), "paths", "--json"], encoding="utf-8")
        setup.assert_called_once_with([str(executable), "setup"], check=True)
        dependency.assert_not_called()
        self.assertFalse((self.data / "dependencies").exists())
        self.assertIn(str(existing_workspace), output.getvalue())
        self.assertEqual(self.snapshot(existing_workspace), research_before)
        _, _, mcp = verify_bundle(executable.parent.parent)
        self.assertTrue(all(server["command"] == str(executable) for server in mcp["mcpServers"].values()))
        marketplace = installer.read_json(self.profile / ".agents/plugins/marketplace.json")
        self.assertTrue(marketplace["plugins"][0]["source"]["path"].startswith("./.local/share/bukan/installations/"))

    def test_updated_bundle_switches_marketplace_and_preserves_previous_install_and_research(self):
        self.make_linux_bundle()
        self.data = self.profile / ".local/share/bukan"
        existing_workspace = self.profile / "existing-research"
        self.write(existing_workspace / "research.sqlite3", "existing research bytes")
        research_before = self.snapshot(existing_workspace)
        paths = self.storage_paths(existing_workspace)
        with patch.dict(os.environ, {}, clear=True), \
                patch.object(installer.sys, "platform", "linux"), \
                patch.object(installer.platform, "machine", return_value="x86_64"), \
                patch.object(installer.shutil, "which", side_effect=lambda tool: f"/usr/bin/{tool}"), \
                patch.object(installer.subprocess, "check_output", return_value=json.dumps(paths)), \
                patch.object(installer.subprocess, "run"), \
                patch("sys.stdout", new_callable=io.StringIO):
            first = installer.install(self.bundle, profile=self.profile)
            first_before = self.snapshot(first.parent.parent)
            manifest = self.bundle / ".codex-plugin/plugin.json"
            self.write(manifest, json.dumps({"name": "bukan", "version": "0.2.3"}))
            metadata = installer.read_json(self.bundle / "bundle.json")
            metadata["version"] = "0.2.3"
            metadata["files"][".codex-plugin/plugin.json"] = digest(manifest)
            self.write(self.bundle / "bundle.json", json.dumps(metadata))
            second = installer.install(self.bundle, profile=self.profile)
            marketplace_path = self.profile / ".agents/plugins/marketplace.json"
            marketplace = installer.read_json(marketplace_path)
            self.assertEqual(marketplace["plugins"][0]["source"]["path"],
                             "./" + second.parent.parent.relative_to(self.profile).as_posix())
            marketplace_before = self.snapshot(marketplace_path.parent)
            self.assertEqual(installer.install(self.bundle, profile=self.profile), second)
        self.assertNotEqual(first, second)
        self.assertEqual(self.snapshot(first.parent.parent), first_before)
        self.assertEqual(self.snapshot(existing_workspace), research_before)
        self.assertEqual(self.snapshot(marketplace_path.parent), marketplace_before)
        self.assertEqual(len(list(marketplace_path.parent.glob("marketplace.json.bukan-backup-*"))), 1)

    def test_linux_xdg_storage_roots_are_persisted_for_hosts_without_xdg_environment(self):
        self.make_linux_bundle()
        xdg = {"XDG_DATA_HOME": str(self.profile / "custom-data"),
               "XDG_CONFIG_HOME": str(self.profile / "custom-config"),
               "XDG_CACHE_HOME": str(self.profile / "custom-cache")}
        self.data = Path(xdg["XDG_DATA_HOME"]) / "bukan"
        paths = self.storage_paths(self.profile / "saved-workspace")
        paths.update(configDir=str(Path(xdg["XDG_CONFIG_HOME"]) / "bukan"),
                     cacheDir=str(Path(xdg["XDG_CACHE_HOME"]) / "bukan"))
        with patch.dict(os.environ, xdg, clear=True), \
                patch.object(installer.sys, "platform", "linux"), \
                patch.object(installer.platform, "machine", return_value="x86_64"), \
                patch.object(installer.shutil, "which", side_effect=lambda tool: f"/usr/bin/{tool}"), \
                patch.object(installer.subprocess, "check_output", return_value=json.dumps(paths)), \
                patch.object(installer.subprocess, "run"), \
                patch("sys.stdout", new_callable=io.StringIO):
            executable = installer.install(self.bundle, profile=self.profile)
        expected = {"BUKAN_DATA_DIR": paths["dataDir"], "BUKAN_CONFIG_DIR": paths["configDir"],
                    "BUKAN_CACHE_DIR": paths["cacheDir"]}
        with patch.dict(os.environ, {}, clear=True):
            _, _, mcp = verify_bundle(executable.parent.parent)
            for server in mcp["mcpServers"].values():
                self.assertEqual(server["env"], expected)
                self.assertNotIn("BUKAN_WORKSPACE", server["env"])
            receipt = installer.read_json(executable.parent.parent / "installation.json")
            self.assertEqual(receipt["overrides"], expected)

    @unittest.skipUnless(os.name == "nt" and shutil.which("powershell.exe"), "Windows PowerShell bootstrap")
    def test_powershell_rejects_corrupt_and_traversal_bundles_before_execution(self):
        bootstrap = self.bundle / "install.ps1"
        shutil.copy2(SCRIPTS / "install.ps1", bootstrap)
        metadata = installer.read_json(self.bundle / "bundle.json")
        for kind in ("checksum", "traversal"):
            with self.subTest(kind=kind):
                bad = dict(metadata, files=dict(metadata["files"]))
                if kind == "checksum":
                    bad["files"]["bin/bukan.exe"] = "0" * 64
                    expected = "Bundle checksum mismatch"
                else:
                    bad["files"]["../outside"] = "0" * 64
                    expected = "Invalid bundle path"
                self.write(self.bundle / "bundle.json", json.dumps(bad))
                before = self.snapshot(self.root)
                # Also exercise the double-click entry point with no -Bundle.
                arguments = [] if kind == "checksum" else ["-Bundle", str(self.bundle)]
                result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
                                         str(bootstrap), *arguments],
                                        capture_output=True, text=True, timeout=30)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stdout + result.stderr)
                self.assertNotIn("Downloading", result.stdout + result.stderr)
                self.assertEqual(self.snapshot(self.root), before)


if __name__ == "__main__":
    unittest.main()
