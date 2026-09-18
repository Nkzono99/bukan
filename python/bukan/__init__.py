"""pip entry point for the bundled native Bukan tools."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys


def main() -> int:
    bundle = Path(__file__).resolve().parent / "_bundle"
    if not (bundle / "bundle.json").is_file():
        print("The Bukan binary bundle is missing. Install a release wheel or build with pip install .", file=sys.stderr)
        return 1
    arguments = sys.argv[1:]
    env = os.environ.copy()
    # uv's pip distribution owns its executable. Its location need not be on
    # the caller's PATH (python -m and venv installs have the same behavior).
    try:
        from uv import find_uv_bin
        uv_directory = str(Path(find_uv_bin()).parent)
    except (ImportError, FileNotFoundError) as error:
        print(f"Bukan's uv dependency is unavailable; reinstall the wheel: {error}", file=sys.stderr)
        return 1
    env["PATH"] = uv_directory + os.pathsep + env.get("PATH", "")
    if arguments and arguments[0] in ("install", "update"):
        if len(arguments) != 1:
            print("Usage: bukan install | bukan update\nBoth prepare the currently installed wheel and preserve existing research.", file=sys.stderr)
            return 2
        command = [sys.executable, "-I", "-B", "-X", "utf8", str(bundle / "install_toolkit.py"), "--bundle", str(bundle)]
    else:
        if arguments in (["--help"], ["-h"], []):
            print("pip commands: install (initial setup), update (activate the installed wheel after pip upgrade).\n", file=sys.stderr)
        binary = bundle / "bin" / ("bukan.exe" if sys.platform == "win32" else "bukan")
        command = [str(binary), *arguments]
    try:
        # No stdout logging: library/research MCP retain their stdio protocol.
        return subprocess.call(command, env=env)
    except OSError as error:
        print(f"Could not start Bukan: {error}", file=sys.stderr)
        return 1
