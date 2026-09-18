#!/usr/bin/env python3
"""Package a built Bukan CLI with its plugin and research engine (Python 3.11+)."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tomllib
import zipfile

from install_local import digest, read_mcp_config, validate_destination


ROOT = Path(__file__).resolve().parents[1]
WINDOWS_INSTALL_FILES = (
    "install.cmd", "install.ps1", "windows-dependencies.json",
)
COMMON_INSTALL_FILES = ("install_local.py", "install_toolkit.py")


def source_files(root: Path, suffix: str) -> list[Path]:
    """Read only ordinary, contained source files, never linked directories."""
    result = []
    for path in sorted(root.iterdir()):
        if path.name.startswith(".") or path.name == "__pycache__":
            continue
        if path.resolve() != path.absolute():
            raise ValueError(f"Linked package input is not supported: {path}")
        if path.is_dir():
            result.extend(source_files(path, suffix))
        elif path.suffix == suffix:
            result.append(path)
    return result


def bundle_inputs(binary: Path, target: str, version: str | None,
                  root: Path) -> tuple[dict[str, Path], str, str]:
    # Fail before creating output when the release binary or any source is missing.
    binary = binary.expanduser().resolve(strict=True)
    if not binary.is_file():
        raise ValueError(f"Not a binary file: {binary}")
    if not re.fullmatch(r"[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)+", target):
        raise ValueError("Use a Rust target triple without path separators.")
    root = root.resolve()
    cargo = tomllib.loads((root / "crates/bukan/Cargo.toml").read_text(encoding="utf-8"))
    current_version = cargo["package"]["version"]
    plugin_root = root / "plugins/bukan"
    manifest = json.loads((plugin_root / ".codex-plugin/plugin.json").read_text(encoding="utf-8"))
    if manifest.get("name") != "bukan" or manifest.get("version") != current_version:
        raise ValueError("The Bukan plugin name/version must match the CLI package.")
    if version is not None and version != current_version:
        raise ValueError(f"Requested version {version} does not match CLI {current_version}.")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?", current_version):
        raise ValueError("The package version must be a safe semantic version.")
    executable = "bukan.exe" if "windows" in target.split("-") else "bukan"
    if binary.name != executable:
        raise ValueError(f"Target {target} requires a binary named {executable}.")
    engine = root / "research-engine"
    review = root / "templates/workspace/.agents/skills/bukan-paper-review"
    setup = plugin_root / "skills"
    inputs = {
        f"bin/{executable}": binary,
        ".codex-plugin/plugin.json": plugin_root / ".codex-plugin/plugin.json",
        ".mcp.json": plugin_root / ".mcp.json",
        "README.md": plugin_root / "README.md",
        "research-engine/pyproject.toml": engine / "pyproject.toml",
        "research-engine/uv.lock": engine / "uv.lock",
    }
    inputs.update({name: root / "scripts" / name for name in COMMON_INSTALL_FILES})
    if executable == "bukan.exe":
        inputs.update({name: root / "scripts" / name for name in WINDOWS_INSTALL_FILES})
    for folder, suffix, prefix in ((engine / "src", ".py", "research-engine/src"),
                                    (review, ".md", "skills/bukan-paper-review"),
                                    (setup, ".md", "skills")):
        if folder.resolve() != folder.absolute():
            raise ValueError(f"Linked package input is not supported: {folder}")
        for source in source_files(folder, suffix):
            relative = f"{prefix}/{source.relative_to(folder).as_posix()}"
            if relative in inputs:
                raise ValueError(f"Duplicated canonical package input: {relative}")
            inputs[relative] = source
    required = {"research-engine/src/bukan_research/cli.py",
                "skills/bukan-paper-review/SKILL.md", "skills/bukan-setup/SKILL.md"}
    if not required.issubset(inputs):
        raise ValueError("Required engine or skill files are missing.")
    for source in inputs.values():
        if not source.is_file() or source.resolve() != source.absolute():
            raise ValueError(f"Missing or linked package input: {source}")
    read_mcp_config(inputs[".mcp.json"])
    return inputs, current_version, executable


def output_destination(path: Path, root: Path) -> Path:
    destination = validate_destination(path)
    for source in ("research-engine", "plugins/bukan", "templates/workspace/.agents/skills/bukan-paper-review"):
        if destination.is_relative_to((root / source).resolve()):
            raise ValueError("The output directory must be outside package sources.")
    return destination


def write_bundle(bundle: Path, inputs: dict[str, Path], target: str, version: str, executable: str) -> Path:
    bundle.mkdir(parents=True)
    for relative, source in sorted(inputs.items()):
        destination = bundle / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    (bundle / f"bin/{executable}").chmod(0o755)
    metadata = {"formatVersion": 1, "version": version, "target": target,
                "binary": f"bin/{executable}",
                "files": {relative: digest(bundle / relative) for relative in sorted(inputs)}}
    (bundle / "bundle.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return bundle


def create_bundle(binary: Path, target: str, bundle: Path,
                  version: str | None = None, *, root: Path = ROOT) -> Path:
    """Create the verified payload shared by release archives and Python wheels."""
    inputs, current_version, executable = bundle_inputs(binary, target, version, root)
    if os.path.lexists(bundle):
        raise FileExistsError(f"Bundle output already exists: {bundle}")
    bundle = output_destination(bundle, root)
    return write_bundle(bundle, inputs, target, current_version, executable)


def package_release(binary: Path, target: str, output_dir: Path,
                    version: str | None = None, *, root: Path = ROOT) -> tuple[Path, Path]:
    inputs, current_version, executable = bundle_inputs(binary, target, version, root)
    output_dir = output_destination(output_dir, root)
    release_name = f"bukan-{current_version}-{target}"
    release_dir = output_dir / release_name
    bundle = release_dir / "bukan"
    archive = output_dir / f"{release_name}.zip"
    if os.path.lexists(release_dir) or os.path.lexists(archive):
        raise FileExistsError(f"Release output already exists: {release_name}")
    write_bundle(bundle, inputs, target, current_version, executable)
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as output:
        for source in sorted(bundle.rglob("*")):
            if source.is_file():
                info = zipfile.ZipInfo.from_file(source, source.relative_to(release_dir).as_posix())
                # Preserve executable permissions even when building on Windows.
                info.create_system = 3
                mode = 0o755 if source == bundle / f"bin/{executable}" else 0o644
                info.external_attr = (stat.S_IFREG | mode) << 16
                output.writestr(info, source.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)
    return bundle, archive


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--target", required=True, help="Rust target triple")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "dist")
    parser.add_argument("--version", help="Require this CLI/plugin version")
    args = parser.parse_args()
    try:
        bundle, archive = package_release(args.binary, args.target, args.output_dir, args.version)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Packaging failed: {error}\n")
    print(f"Plugin bundle: {bundle}")
    print(f"Release archive: {archive}")


if __name__ == "__main__":
    main()
