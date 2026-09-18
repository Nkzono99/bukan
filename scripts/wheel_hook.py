"""Build a platform wheel from the same verified payload as the release ZIP."""
from __future__ import annotations

import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

sys.path.insert(0, str(Path(__file__).resolve().parent))
from package_release import create_bundle


TARGET_TAGS = {
    "x86_64-pc-windows-msvc": "py3-none-win_amd64",
    "x86_64-unknown-linux-gnu": "py3-none-linux_x86_64",
    # The musl target's executable is statically linked, so it runs on either
    # libc. Release CI builds/tests it explicitly; no CPython ABI is involved.
    "x86_64-unknown-linux-musl": "py3-none-manylinux_2_17_x86_64.musllinux_1_2_x86_64",
}


def build_target() -> str:
    if platform.machine().lower() not in ("amd64", "x86_64"):
        raise ValueError("Bukan currently distributes x86_64 wheels only.")
    defaults = {"win32": "x86_64-pc-windows-msvc", "linux": "x86_64-unknown-linux-gnu"}
    if sys.platform not in defaults:
        raise ValueError("Bukan wheels currently support Windows and Linux.")
    target = os.environ.get("BUKAN_BUILD_TARGET", defaults[sys.platform])
    if target not in TARGET_TAGS or ("windows" in target) != (sys.platform == "win32"):
        raise ValueError(f"Unsupported native wheel target on this host: {target}")
    return target


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version: str, build_data: dict) -> None:
        if version == "editable":
            raise ValueError("Bukan packages a native bundle; use pip install . instead of an editable install.")
        root = Path(self.root)
        target = build_target()
        binary_setting = os.environ.get("BUKAN_BUILD_BINARY")
        if binary_setting:
            binary = Path(binary_setting).resolve(strict=True)
        else:
            subprocess.run(["cargo", "build", "--release", "--locked", "--package", "bukan", "--target", target], cwd=root, check=True)
            binary = root / "target" / target / "release" / ("bukan.exe" if sys.platform == "win32" else "bukan")
        self._temporary = tempfile.TemporaryDirectory(prefix="bukan-wheel-")
        try:
            # Cargo/plugin versions retain SemVer spelling (e.g. 1.0.0-rc.1);
            # Hatch separately normalizes the wheel metadata to PEP 440.
            bundle = create_bundle(binary, target, Path(self._temporary.name) / "bundle", root=root)
            build_data["force_include"][str(bundle)] = "bukan/_bundle"
            build_data["pure_python"] = False
            build_data["tag"] = TARGET_TAGS[target]
        except BaseException:
            self._temporary.cleanup()
            raise

    def finalize(self, version: str, build_data: dict, artifact_path: str) -> None:
        self._temporary.cleanup()
