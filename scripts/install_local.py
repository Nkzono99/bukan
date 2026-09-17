#!/usr/bin/env python3
"""Install an extracted Bukan release without changing PATH or host settings."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate_destination(path: Path) -> Path:
    """Resolve links before checking known read-only/research destinations."""
    destination = path.expanduser().resolve()
    ancestors = (destination, *destination.parents)
    if any(parent.name.casefold() == "paperpile" or (parent / "All Papers").is_dir()
           for parent in ancestors):
        raise ValueError("The destination must be outside Paperpile.")
    if any((parent / "bukan.toml").exists() for parent in ancestors):
        raise ValueError("The destination must be outside research workspaces.")
    return destination


def bundle_path(root: Path, relative: str) -> Path:
    path = PurePosixPath(relative)
    if (not relative or path.is_absolute() or ".." in path.parts
            or "\\" in relative or ":" in relative or path.as_posix() != relative):
        raise ValueError(f"Invalid bundle path: {relative!r}")
    source = root.joinpath(*path.parts)
    if source.resolve() != source.absolute():
        raise ValueError(f"Bundle links are not supported: {relative}")
    if not source.is_file():
        raise ValueError(f"Missing bundle file: {relative}")
    return source


def read_mcp_config(path: Path) -> dict:
    config = json.loads(path.read_text(encoding="utf-8"))
    servers = config.get("mcpServers") if isinstance(config, dict) else None
    commands = {"bukan-library": "mcp", "bukan-research": "research-mcp"}
    if not isinstance(servers, dict) or set(servers) != set(commands):
        raise ValueError("The bundle must contain the two Bukan MCP servers.")
    for name, command in commands.items():
        server = servers[name]
        if (not isinstance(server, dict) or server.get("args") != [command]
                or not isinstance(server.get("command"), str) or not server["command"].strip()):
            raise ValueError(f"Invalid Bukan MCP server: {name}")
    return config


def install_bundle(bundle: Path, destination: Path) -> Path:
    bundle = bundle.expanduser().resolve()
    metadata = json.loads(bundle_path(bundle, "bundle.json").read_text(encoding="utf-8"))
    if metadata.get("formatVersion") != 1:
        raise ValueError("Unsupported Bukan bundle format.")
    binary = metadata.get("binary")
    if binary not in ("bin/bukan", "bin/bukan.exe"):
        raise ValueError("Missing or invalid Bukan binary path.")
    files = metadata.get("files")
    required = {binary, ".codex-plugin/plugin.json", ".mcp.json",
                "research-engine/pyproject.toml", "research-engine/uv.lock",
                "research-engine/src/bukan_research/cli.py",
                "skills/bukan-paper-review/SKILL.md", "skills/bukan-setup/SKILL.md"}
    if not isinstance(files, dict) or not required.issubset(files):
        raise ValueError("The bundle is incomplete; build it with package_release.py.")
    sources = {relative: bundle_path(bundle, relative) for relative in files}
    for relative, source in sources.items():
        if digest(source) != files[relative]:
            raise ValueError(f"Bundle checksum mismatch: {relative}")
    mcp = read_mcp_config(sources[".mcp.json"])
    # Resolve before writing; never merge into an existing/custom installation.
    if os.path.lexists(destination.expanduser()):
        raise FileExistsError(f"Installation destination already exists: {destination}")
    destination = validate_destination(destination)
    if destination.is_relative_to(bundle):
        raise ValueError("The installation must be outside the source bundle.")
    destination.mkdir(parents=True, exist_ok=False)
    for relative, source in sources.items():
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    executable = destination / binary
    executable.chmod(0o755)
    for server in mcp["mcpServers"].values():
        server["command"] = str(executable)
    (destination / ".mcp.json").write_text(json.dumps(mcp, indent=2) + "\n", encoding="utf-8")
    metadata["files"][".mcp.json"] = digest(destination / ".mcp.json")
    (destination / "bundle.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return executable


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True, help="Extracted release's bukan directory")
    parser.add_argument("--destination", type=Path, default=Path.home() / "plugins" / "bukan")
    args = parser.parse_args()
    try:
        executable = install_bundle(args.bundle, args.destination)
    except (OSError, ValueError) as error:
        parser.exit(1, f"Install failed: {error}\n")
    print(f"Installed executable: {executable}")
    print(f"Plugin folder: {executable.parent.parent}")
    print(f"Optional PATH entry: {executable.parent}")
    print('For a new workspace: run the executable with init "/absolute/workspace"')
    print('Then run it with setup "/absolute/workspace" --default')
    print("Finally install the plugin in your agent host.")
    print("PATH, host configuration, and research data were not changed.")


if __name__ == "__main__":
    main()
