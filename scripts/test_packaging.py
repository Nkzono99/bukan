"""Release and installation boundaries; no live research data is used."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile

from install_local import digest, install_bundle
from package_release import COMMON_INSTALL_FILES, ROOT, WINDOWS_INSTALL_FILES, create_bundle, package_release


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bukan-packaging-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "repository"
        self.output = Path(self.temporary.name) / "release"
        self.destination = Path(self.temporary.name) / "installed" / "bukan"
        for relative in ("plugins/bukan", "templates/workspace/.agents/skills/bukan-paper-review"):
            shutil.copytree(ROOT / relative, self.root / relative)
        self.version = json.loads((self.root / "plugins/bukan/.codex-plugin/plugin.json").read_text(encoding="utf-8"))["version"]
        self.write("crates/bukan/Cargo.toml", f'[package]\nversion = "{self.version}"\n')
        self.write("research-engine/pyproject.toml", '[project]\nname = "bukan-research"\n')
        self.write("research-engine/uv.lock", "version = 1\n")
        self.write("research-engine/src/bukan_research/cli.py", "def main(): pass\n")
        for name in COMMON_INSTALL_FILES + WINDOWS_INSTALL_FILES:
            self.write(f"scripts/{name}", (ROOT / "scripts" / name).read_text(encoding="utf-8"))
        self.binary = self.write("target/release/bukan.exe", "placeholder executable\n")

    def write(self, relative, contents):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")
        return path

    def package(self, *, binary=None, target="x86_64-pc-windows-msvc", output=None, version=None):
        return package_release(binary or self.binary, target, output or self.output,
                               version, root=self.root)

    def test_bundle_contains_canonical_skill_and_excludes_local_data(self):
        self.write("research-engine/.venv/secrets.py", "excluded")
        self.write("research-engine/src/bukan_research/__pycache__/cached.pyc", "excluded")
        self.write("research-engine/src/bukan_research/settings.toml", "secret")
        self.write("research-engine/research.sqlite3", "excluded")
        self.write("plugins/bukan/.env", "secret")
        self.write("Paperpile/paper.pdf", "private source")
        bundle, archive = self.package()
        metadata = json.loads((bundle / "bundle.json").read_text(encoding="utf-8"))
        for name in COMMON_INSTALL_FILES + WINDOWS_INSTALL_FILES:
            self.assertEqual((bundle / name).read_bytes(), (self.root / "scripts" / name).read_bytes())
            self.assertEqual(metadata["files"][name], digest(bundle / name))
        source = self.root / "templates/workspace/.agents/skills/bukan-paper-review"
        for path in source.rglob("*.md"):
            self.assertEqual(path.read_bytes(), (bundle / "skills/bukan-paper-review" / path.relative_to(source)).read_bytes())
        with zipfile.ZipFile(archive) as zipped:
            self.assertTrue(all(name.startswith("bukan/") for name in zipped.namelist()))
            self.assertEqual(zipped.getinfo("bukan/bin/bukan.exe").external_attr >> 16 & 0o777, 0o755)
            names = "\n".join(zipped.namelist())
            for excluded in (".venv", "__pycache__", "settings.toml", "sqlite3", ".env", "Paperpile"):
                self.assertNotIn(excluded, names)

    def test_missing_binary_does_not_create_output(self):
        with self.assertRaises(FileNotFoundError):
            self.package(binary=self.root / "missing.exe")
        self.assertFalse(self.output.exists())

    def test_standalone_payload_matches_zip_bundle(self):
        bundle, _ = self.package()
        payload = create_bundle(self.binary, "x86_64-pc-windows-msvc", self.output / "wheel-payload",
                                self.version, root=self.root)
        originals = {path.relative_to(bundle): path.read_bytes() for path in bundle.rglob("*") if path.is_file()}
        self.assertEqual({path.relative_to(payload): path.read_bytes() for path in payload.rglob("*") if path.is_file()}, originals)
        with self.assertRaises(FileExistsError):
            create_bundle(self.binary, "x86_64-pc-windows-msvc", payload, root=self.root)

    def test_linux_musl_bundle_uses_shared_installer(self):
        binary = self.write("target/release/bukan", "placeholder static executable")
        payload = create_bundle(binary, "x86_64-unknown-linux-musl", self.output / "wheel-payload", root=self.root)
        metadata = json.loads((payload / "bundle.json").read_text(encoding="utf-8"))
        self.assertEqual(metadata["target"], "x86_64-unknown-linux-musl")
        self.assertEqual(metadata["binary"], "bin/bukan")
        self.assertTrue((payload / "install_toolkit.py").is_file())
        self.assertFalse((payload / "install.ps1").exists())

    def test_missing_windows_installer_does_not_create_output(self):
        (self.root / "scripts/install.ps1").unlink()
        with self.assertRaisesRegex(ValueError, "Missing or linked package input"):
            self.package()
        self.assertFalse(self.output.exists())

    def test_version_and_target_checks_before_writes(self):
        for options in ({"version": "9.0.0"}, {"target": "../../escape"},
                        {"target": "x86_64-unknown-linux-gnu"}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                self.package(**options)
        self.assertFalse(self.output.exists())

    def test_missing_skill_and_plugin_version_mismatch(self):
        manifest_path = self.root / "plugins/bukan/.codex-plugin/plugin.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["version"] = "0.0.0"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "name/version"):
            self.package()
        manifest["version"] = self.version
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        (self.root / "templates/workspace/.agents/skills/bukan-paper-review/SKILL.md").unlink()
        with self.assertRaisesRegex(ValueError, "skill files"):
            self.package()
        self.assertFalse(self.output.exists())

    def test_outputs_outside_paperpile_and_research_workspace(self):
        workspace = Path(self.temporary.name) / "research"
        workspace.mkdir()
        (workspace / "bukan.toml").write_text("version = 1\n", encoding="utf-8")
        for destination in (Path(self.temporary.name) / "Paperpile" / "release", workspace / "release"):
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                self.package(output=destination)
            self.assertFalse(destination.exists())

    def test_existing_release_preserved(self):
        bundle, archive = self.package()
        original = digest(archive)
        (bundle / "custom.txt").write_text("keep", encoding="utf-8")
        with self.assertRaises(FileExistsError):
            self.package()
        self.assertEqual(digest(archive), original)
        self.assertEqual((bundle / "custom.txt").read_text(), "keep")

    def test_install_rewrites_only_installed_mcp_and_preserves_existing_destination(self):
        bundle, _ = self.package()
        original = (bundle / ".mcp.json").read_bytes()
        executable = install_bundle(bundle, self.destination)
        servers = json.loads((self.destination / ".mcp.json").read_text())["mcpServers"]
        self.assertEqual({server["command"] for server in servers.values()}, {str(executable)})
        self.assertEqual([server["args"] for server in servers.values()], [["mcp"], ["research-mcp"]])
        metadata = json.loads((self.destination / "bundle.json").read_text())
        self.assertEqual(metadata["files"][".mcp.json"], digest(self.destination / ".mcp.json"))
        self.assertEqual((bundle / ".mcp.json").read_bytes(), original)
        (self.destination / "custom.txt").write_text("keep", encoding="utf-8")
        with self.assertRaises(FileExistsError):
            install_bundle(bundle, self.destination)
        self.assertEqual((self.destination / "custom.txt").read_text(), "keep")

    def test_unix_executable_name_and_archive_permissions(self):
        binary = self.write("target/release/bukan", "placeholder unix executable")
        bundle, archive = self.package(binary=binary, target="x86_64-unknown-linux-gnu")
        metadata = json.loads((bundle / "bundle.json").read_text(encoding="utf-8"))
        for name in COMMON_INSTALL_FILES:
            self.assertEqual((bundle / name).read_bytes(), (self.root / "scripts" / name).read_bytes())
            self.assertEqual(metadata["files"][name], digest(bundle / name))
        for name in WINDOWS_INSTALL_FILES:
            self.assertFalse((bundle / name).exists())
            self.assertNotIn(name, metadata["files"])
        executable = install_bundle(bundle, self.destination)
        self.assertEqual(executable.name, "bukan")
        with zipfile.ZipFile(archive) as zipped:
            self.assertEqual(zipped.getinfo("bukan/bin/bukan").external_attr >> 16 & 0o777, 0o755)
            for name in WINDOWS_INSTALL_FILES:
                self.assertNotIn(f"bukan/{name}", zipped.namelist())
        servers = json.loads((self.destination / ".mcp.json").read_text())["mcpServers"]
        self.assertTrue(all(server["command"] == str(executable) for server in servers.values()))

    def test_modified_bundle_rejected_before_install(self):
        bundle, _ = self.package()
        (bundle / "bin/bukan.exe").write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "checksum"):
            install_bundle(bundle, self.destination)
        self.assertFalse(self.destination.exists())

    def test_manifest_path_escape_rejected_before_install(self):
        bundle, _ = self.package()
        path = bundle / "bundle.json"
        metadata = json.loads(path.read_text())
        for relative in ("../escaped", "C:/escaped", "..\\escaped", "/escaped"):
            bad = dict(metadata, files=dict(metadata["files"], **{relative: "irrelevant"}))
            path.write_text(json.dumps(bad), encoding="utf-8")
            with self.subTest(relative=relative), self.assertRaisesRegex(ValueError, "Invalid bundle path"):
                install_bundle(bundle, self.destination)
        self.assertFalse(self.destination.exists())

    def test_install_rejects_source_and_research_destinations(self):
        bundle, _ = self.package()
        workspace = Path(self.temporary.name) / "research"
        workspace.mkdir()
        (workspace / "bukan.toml").write_text("version = 1\n", encoding="utf-8")
        for destination in (bundle / "installed", workspace / "installed", workspace.parent / "Paperpile" / "installed"):
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                install_bundle(bundle, destination)
            self.assertFalse(destination.exists())

    def test_renamed_paperpile_root_rejected_before_writes(self):
        paperpile = Path(self.temporary.name) / "synced-library"
        (paperpile / "All Papers").mkdir(parents=True)
        with self.assertRaisesRegex(ValueError, "outside Paperpile"):
            self.package(output=paperpile / "release")
        bundle, _ = self.package()
        with self.assertRaisesRegex(ValueError, "outside Paperpile"):
            install_bundle(bundle, paperpile / "installed")
        self.assertEqual(list(paperpile.iterdir()), [paperpile / "All Papers"])

    def test_malformed_mcp_config_rejected_before_packaging_or_installing(self):
        bundle, _ = self.package()
        config_path = self.root / "plugins/bukan/.mcp.json"
        config = json.loads(config_path.read_text())
        config["mcpServers"]["bukan-library"] = None
        contents = json.dumps(config)
        config_path.write_text(contents, encoding="utf-8")
        other_output = self.output / "invalid"
        with self.assertRaisesRegex(ValueError, "Invalid Bukan MCP"):
            self.package(output=other_output)
        self.assertFalse(other_output.exists())
        (bundle / ".mcp.json").write_text(contents, encoding="utf-8")
        metadata = json.loads((bundle / "bundle.json").read_text())
        metadata["files"][".mcp.json"] = digest(bundle / ".mcp.json")
        (bundle / "bundle.json").write_text(json.dumps(metadata), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Invalid Bukan MCP"):
            install_bundle(bundle, self.destination)
        self.assertFalse(self.destination.exists())

    def test_linked_source_rejected(self):
        linked = self.root / "research-engine/src/bukan_research/linked.py"
        outside = Path(self.temporary.name) / "outside.py"
        outside.write_text("private", encoding="utf-8")
        try:
            linked.symlink_to(outside)
        except OSError:
            self.skipTest("File symlinks unavailable on this host")
        with self.assertRaisesRegex(ValueError, "Linked package input"):
            self.package()
        self.assertFalse(self.output.exists())

    @unittest.skipUnless(os.name == "nt", "Windows junction regression")
    def test_junction_source_and_paperpile_output_rejected(self):
        outside = Path(self.temporary.name) / "Paperpile"
        outside.mkdir()
        (outside / "private.py").write_text("private", encoding="utf-8")
        linked = self.root / "research-engine/src/bukan_research/linked"
        subprocess.run(["cmd", "/c", "mklink", "/J", str(linked), str(outside)],
                       check=True, capture_output=True)
        with self.assertRaisesRegex(ValueError, "Linked package input"):
            self.package()
        linked.rmdir()  # Remove the junction itself; its target remains untouched.
        output_link = Path(self.temporary.name) / "output-link"
        subprocess.run(["cmd", "/c", "mklink", "/J", str(output_link), str(outside)],
                       check=True, capture_output=True)
        with self.assertRaisesRegex(ValueError, "outside Paperpile"):
            self.package(output=output_link / "release")
        self.assertEqual((outside / "private.py").read_text(), "private")
        self.assertFalse((outside / "release").exists())

    @unittest.skipUnless(os.name == "nt", "Windows junction regression")
    def test_dangling_junction_destinations_are_preserved(self):
        bundle, _ = self.package()
        self.destination.parent.mkdir(parents=True)
        missing = Path(self.temporary.name) / "missing-target"
        subprocess.run(["cmd", "/c", "mklink", "/J", str(self.destination), str(missing)],
                       check=True, capture_output=True)
        with self.assertRaises(FileExistsError):
            install_bundle(bundle, self.destination)
        other_output = self.output / "other"
        other_output.mkdir()
        release = other_output / f"bukan-{self.version}-x86_64-pc-windows-msvc"
        subprocess.run(["cmd", "/c", "mklink", "/J", str(release), str(missing)],
                       check=True, capture_output=True)
        with self.assertRaises(FileExistsError):
            self.package(output=other_output)
        self.assertFalse(missing.exists())
        self.assertTrue(os.path.lexists(self.destination))
        self.assertTrue(os.path.lexists(release))


if __name__ == "__main__":
    unittest.main()
