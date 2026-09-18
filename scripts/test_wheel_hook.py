"""Wheel build boundaries, without compiling or executing fixture binaries."""

import importlib.util
import json
import os
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import test_packaging


@unittest.skipUnless(importlib.util.find_spec("hatchling"), "Install hatchling to test wheel build hooks")
class WheelHookTests(unittest.TestCase):
    def test_prerelease_wheel_keeps_semver_bundle_and_normalized_distribution_version(self):
        import wheel_hook

        fixture = test_packaging.PackagingTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        fixture.write("crates/bukan/Cargo.toml", '[package]\nversion = "0.2.3-rc.1"\n')
        manifest_path = fixture.root / "plugins/bukan/.codex-plugin/plugin.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["version"] = "0.2.3-rc.1"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        context = SimpleNamespace(root=str(fixture.root), metadata=SimpleNamespace(version="0.2.3rc1"))
        build_data = {"force_include": {}}
        with patch.dict(os.environ, {"BUKAN_BUILD_BINARY": str(fixture.binary)}), \
                patch.object(wheel_hook, "build_target", return_value="x86_64-pc-windows-msvc"), \
                patch.object(wheel_hook.subprocess, "run") as compiler:
            try:
                wheel_hook.CustomBuildHook.initialize(context, "standard", build_data)
                compiler.assert_not_called()
                bundle = Path(next(iter(build_data["force_include"])))
                metadata = json.loads((bundle / "bundle.json").read_text(encoding="utf-8"))
                self.assertEqual(metadata["version"], "0.2.3-rc.1")
                self.assertEqual(context.metadata.version, "0.2.3rc1")
                self.assertEqual(build_data["tag"], "py3-none-win_amd64")
                self.assertFalse(build_data["pure_python"])
            finally:
                if hasattr(context, "_temporary"):
                    context._temporary.cleanup()


if __name__ == "__main__":
    unittest.main()
