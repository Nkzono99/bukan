#!/usr/bin/env python3
"""Finish Windows setup using the Python obtained by install.ps1."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile
from urllib.request import urlopen
import zipfile

# -I excludes arbitrary caller modules. Only the verified release is importable.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from install_local import bundle_path, digest, install_bundle, validate_destination, verify_bundle


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def ordinary_destination(path: Path) -> Path:
    # Rust canonical Windows paths use the extended Win32 prefix. Normalize it
    # before comparing with Path.home(), which has the ordinary drive spelling.
    value = str(path)
    if os.name == "nt" and value.startswith("\\\\?\\"):
        path = Path("\\\\" + value[8:] if value.startswith("\\\\?\\UNC\\") else value[4:])
    path = path.absolute()
    if path.resolve() != path:
        raise ValueError(f"Linked installer destinations are not supported: {path}")
    return validate_destination(path)


def write_json(path: Path, value: dict) -> None:
    ordinary_destination(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        stream.write((json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def download(url: str, destination: Path, expected: str) -> None:
    if not url.startswith("https://"):
        raise ValueError("Dependency downloads must use HTTPS.")
    with urlopen(url, timeout=120) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output)
    if digest(destination) != expected:
        raise ValueError(f"Dependency checksum mismatch: {url}")


def extract_archive(archive: Path, destination: Path) -> None:
    with zipfile.ZipFile(archive) as zipped:
        for item in zipped.infolist():
            relative = item.filename.rstrip("/")
            path = PurePosixPath(relative)
            if (not relative or path.is_absolute() or ".." in path.parts
                    or "\\" in relative or ":" in relative or path.as_posix() != relative
                    or (item.external_attr >> 16) & 0o170000 == 0o120000):
                raise ValueError(f"Invalid dependency archive path: {item.filename}")
        zipped.extractall(destination)


def ensure_dependency(root: Path, name: str, asset: dict, archive: Path | None = None) -> Path:
    if not re.fullmatch(r"[A-Za-z0-9.-]+", asset["version"]) or asset["version"] in (".", ".."):
        raise ValueError("Invalid dependency version.")
    destination = ordinary_destination(root / name / asset["version"])
    if destination.exists():
        receipt = read_json(destination / "bukan-dependency.json")
        if receipt["sha256"] != asset["sha256"]:
            raise ValueError(f"Dependency version has a different checksum: {destination}")
        for relative, expected in receipt["files"].items():
            if digest(bundle_path(destination, relative)) != expected:
                raise ValueError(f"Installed dependency is damaged: {destination / relative}")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".install-", dir=destination.parent) as staging:
            stage = Path(staging)
            if archive is None:
                archive = stage / "download.zip"
                print(f"Downloading {name} {asset['version']}...", flush=True)
                download(asset["url"], archive, asset["sha256"])
            elif digest(archive) != asset["sha256"]:
                raise ValueError(f"Dependency checksum mismatch: {name}")
            extracted = stage / "contents"
            extract_archive(archive, extracted)
            for notice in asset["notices"]:
                if Path(notice["name"]).name != notice["name"]:
                    raise ValueError("Invalid license filename.")
                download(notice["url"], extracted / notice["name"], notice["sha256"])
            for executable in asset["executables"]:
                bundle_path(extracted, str(PurePosixPath(asset["bin"]) / executable))
            receipt = {"sha256": asset["sha256"], "files": {
                file.relative_to(extracted).as_posix(): digest(file)
                for file in sorted(extracted.rglob("*")) if file.is_file()}}
            write_json(extracted / "bukan-dependency.json", receipt)
            extracted.rename(destination)
    for executable in asset["executables"]:
        bundle_path(destination, str(PurePosixPath(asset["bin"]) / executable))
    return destination


def marketplace_entry(profile: Path, destination: Path) -> tuple[Path, dict]:
    """Personal sources resolve relative to the profile, not .agents/plugins."""
    relative = destination.relative_to(profile.resolve()).as_posix()
    path = ordinary_destination(profile / ".agents/plugins/marketplace.json")
    if path.exists():
        marketplace = read_json(path)
    else:
        marketplace = {"name": "personal", "interface": {"displayName": "Personal"}, "plugins": []}
    if (not isinstance(marketplace, dict)
            or not re.fullmatch(r"[A-Za-z0-9_-]+", marketplace.get("name", ""))
            or not isinstance(marketplace.get("plugins"), list)):
        raise ValueError(f"Invalid personal marketplace: {path}")
    entries = marketplace["plugins"]
    if any(not isinstance(entry, dict) for entry in entries):
        raise ValueError(f"Invalid personal marketplace entries: {path}")
    matches = [entry for entry in entries if entry.get("name") == "bukan"]
    if len(matches) > 1:
        raise ValueError(f"Duplicate Bukan marketplace entries: {path}")
    entry = matches[0] if matches else {"name": "bukan"}
    entry["source"] = {"source": "local", "path": f"./{relative}"}
    entry.setdefault("policy", {"installation": "AVAILABLE", "authentication": "ON_INSTALL"})
    entry.setdefault("category", "Productivity")
    if not matches:
        entries.append(entry)
    return path, marketplace


def register_marketplace(path: Path, marketplace: dict) -> None:
    if path.exists():
        if read_json(path) == marketplace:
            return
        stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S-%f")
        backup = path.with_name(f"marketplace.json.bukan-backup-{stamp}")
        shutil.copy2(path, backup)
    write_json(path, marketplace)


def windows_bundle(bundle: Path) -> dict:
    metadata, _, _ = verify_bundle(bundle)
    if (metadata.get("target") != "x86_64-pc-windows-msvc"
            or metadata.get("binary") != "bin/bukan.exe"
            or not re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?", metadata.get("version", ""))):
        raise ValueError("Use the Bukan Windows x64 release bundle.")
    return metadata


def install_toolkit(bundle: Path, data: Path, overrides: dict[str, str]) -> Path:
    metadata = windows_bundle(bundle)
    fingerprint = hashlib.sha256((digest(bundle / "bundle.json") + json.dumps(overrides, sort_keys=True)).encode()).hexdigest()[:16]
    destination = ordinary_destination(data / "installations" / f"{metadata['version']}-{fingerprint}" / "bukan")
    if destination.exists():
        verify_bundle(destination)
        if read_json(destination / "installation.json") != {"source": digest(bundle / "bundle.json"), "overrides": overrides}:
            raise ValueError(f"Existing installation has different inputs: {destination}")
        return destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".install-", dir=destination.parent) as staging:
        stage = Path(staging) / "bukan"
        install_bundle(bundle, stage)
        mcp = read_json(stage / ".mcp.json")
        for server in mcp["mcpServers"].values():
            server["command"] = str(destination / metadata["binary"])
            if overrides:
                server["env"] = dict(server.get("env", {}), **overrides)
        write_json(stage / ".mcp.json", mcp)
        manifest_path = stage / ".codex-plugin/plugin.json"
        manifest = read_json(manifest_path)
        if manifest.get("name") != "bukan":
            raise ValueError("Invalid plugin name.")
        # Each build/configuration has its own cache entry; identical reruns reuse it.
        manifest["version"] = f"{metadata['version'].split('+')[0]}+codex.{fingerprint}"
        write_json(manifest_path, manifest)
        for relative in (".mcp.json", ".codex-plugin/plugin.json"):
            metadata["files"][relative] = digest(stage / relative)
        write_json(stage / "bundle.json", metadata)
        write_json(stage / "installation.json", {"source": digest(bundle / "bundle.json"), "overrides": overrides})
        stage.rename(destination)
    return destination


def install(bundle: Path, uv_archive: Path | None = None, *, profile: Path | None = None) -> Path:
    bundle = bundle.resolve()
    metadata = windows_bundle(bundle)
    binary = bundle / metadata["binary"]
    paths = json.loads(subprocess.check_output([str(binary), "paths", "--json"], encoding="utf-8"))
    data = ordinary_destination(Path(paths["dataDir"]))
    profile = (profile or Path.home()).resolve()
    if not data.is_relative_to(profile):
        raise ValueError("The Windows plugin installer requires BUKAN_DATA_DIR inside your user profile. Use manual CLI installation for an external data root.")
    overrides = {name: os.environ[name] for name in
                 ("BUKAN_DATA_DIR", "BUKAN_CONFIG_DIR", "BUKAN_CACHE_DIR", "BUKAN_WORKSPACE") if name in os.environ}
    # Validate marketplace before downloading or changing the active runtime.
    marketplace_entry(profile, data / "installations")
    assets = read_json(bundle / "windows-dependencies.json")
    dependencies = ordinary_destination(data / "dependencies")
    uv = ensure_dependency(dependencies, "uv", assets["uv"], uv_archive)
    poppler = ensure_dependency(dependencies, "poppler", assets["poppler"])
    write_json(dependencies / "current.json", {"version": 1,
               "uv": (uv / "uv.exe").relative_to(dependencies).as_posix(),
               "popplerBin": (poppler / assets["poppler"]["bin"]).relative_to(dependencies).as_posix()})
    destination = install_toolkit(bundle, data, overrides)
    executable = destination / "bin/bukan.exe"
    subprocess.run([str(executable), "setup"], check=True)
    path, marketplace = marketplace_entry(profile, destination)
    register_marketplace(path, marketplace)
    print(f"\nBukan tools installed: {executable}")
    print(f"Research workspace: {paths.get('selectedWorkspace') or paths['managedWorkspace']}")
    print(f"\nStep 2: In Codex, install (or reinstall) Bukan from {marketplace['name']}.")
    print(f"CLI equivalent: codex plugin add bukan@{marketplace['name']}")
    print("Then start a new Codex conversation. No PATH or administrator changes are needed.")
    return executable


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--uv-archive", type=Path)
    args = parser.parse_args()
    try:
        if sys.platform != "win32":
            raise ValueError("Use install_local.py on Linux.")
        install(args.bundle, args.uv_archive)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Installation failed: {error}\n")


if __name__ == "__main__":
    main()
