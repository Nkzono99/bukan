"""pip entry-point boundaries; subprocesses and uv are isolated test fixtures."""

from contextlib import redirect_stderr, redirect_stdout
import io
import os
from pathlib import Path
import runpy
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
import bukan as launcher


class PipLauncherTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bukan-pip-launcher-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "日本語 profile with spaces"
        self.package = self.root / "site-packages" / "bukan"
        self.bundle = self.package / "_bundle"
        self.bundle.mkdir(parents=True)
        (self.bundle / "bundle.json").write_text("{}", encoding="utf-8")
        self.uv_binary = self.root / "uv tools" / "uv.exe"
        self.uv = types.ModuleType("uv")
        self.uv.find_uv_bin = Mock(return_value=str(self.uv_binary))
        self.addCleanup(patch.stopall)
        patch.object(launcher, "__file__", str(self.package / "__init__.py")).start()
        patch.dict(sys.modules, {"uv": self.uv}).start()
        self.child = patch.object(launcher.subprocess, "call", return_value=0).start()

    def invoke(self, *arguments):
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(sys, "argv", ["bukan", *arguments]), redirect_stdout(stdout), redirect_stderr(stderr):
            result = launcher.main()
        return result, stdout.getvalue(), stderr.getvalue()

    def test_install_and_update_prepare_the_same_bundled_toolkit(self):
        commands = []
        for verb in ("install", "update"):
            with self.subTest(verb=verb):
                self.child.reset_mock()
                self.child.return_value = 17
                self.assertEqual(self.invoke(verb), (17, "", ""))
                self.child.assert_called_once()
                command = self.child.call_args.args[0]
                commands.append(command)
                self.assertEqual(command, [
                    sys.executable, "-I", "-B", "-X", "utf8",
                    str(self.bundle / "install_toolkit.py"), "--bundle", str(self.bundle),
                ])
                self.assertEqual(set(self.child.call_args.kwargs), {"env"})
        self.assertEqual(commands[0], commands[1])

    def test_setup_aliases_reject_extra_arguments_without_launching(self):
        for verb in ("install", "update"):
            with self.subTest(verb=verb):
                result, stdout, stderr = self.invoke(verb, "--bundle", "untrusted location")
                self.assertEqual(result, 2)
                self.assertEqual(stdout, "")
                self.assertIn("Usage:", stderr)
        self.child.assert_not_called()

    def test_forwards_native_arguments_without_parsing_or_shell_expansion(self):
        argument_lists = [
            ["mcp", str(self.root / "研究 workspace")],
            ["research-mcp"],
            ["research", str(self.root / "研究 workspace"), "--", "request", "--store", "file with spaces.sqlite3"],
            ["research", "--", "--store=unchanged.sqlite3", "$(no-shell)", "; echo untouched"],
        ]
        for platform, binary in (("win32", "bukan.exe"), ("linux", "bukan")):
            for arguments in argument_lists:
                with self.subTest(platform=platform, arguments=arguments), patch.object(sys, "platform", platform):
                    self.child.reset_mock()
                    self.assertEqual(self.invoke(*arguments), (0, "", ""))
                    self.child.assert_called_once()
                    self.assertEqual(self.child.call_args.args[0], [str(self.bundle / "bin" / binary), *arguments])
                    # Inherited stdio keeps MCP messages intact; list argv avoids a shell.
                    self.assertEqual(set(self.child.call_args.kwargs), {"env"})

    def test_mcp_stdout_contains_only_the_child_protocol(self):
        protocol = '{"jsonrpc":"2.0","id":1,"result":{}}\n'

        def emit_protocol(*args, **kwargs):
            sys.stdout.write(protocol)
            return 0

        self.child.side_effect = emit_protocol
        for command in ("mcp", "research-mcp"):
            with self.subTest(command=command):
                self.assertEqual(self.invoke(command), (0, protocol, ""))

    def test_uv_path_is_prepended_only_in_child_environment(self):
        original_path = str(self.root / "unrelated tools")
        with patch.dict(os.environ, {"PATH": original_path, "BUKAN_LAUNCHER_TEST": "preserve"}):
            original_environment = os.environ.copy()
            self.assertEqual(self.invoke("doctor", "--json"), (0, "", ""))
            child_environment = self.child.call_args.kwargs["env"]
            self.assertEqual(child_environment, {
                **original_environment,
                "PATH": str(self.uv_binary.parent) + os.pathsep + original_path,
            })
            self.assertEqual(dict(os.environ), original_environment)

    def test_child_failure_status_is_preserved(self):
        self.child.return_value = 23
        self.assertEqual(self.invoke("research-mcp"), (23, "", ""))

    def test_start_failure_is_reported_on_stderr(self):
        self.child.side_effect = OSError("fixture executable is unavailable")
        result, stdout, stderr = self.invoke("mcp")
        self.assertEqual(result, 1)
        self.assertEqual(stdout, "")
        self.assertIn("Could not start Bukan", stderr)
        self.assertIn("fixture executable is unavailable", stderr)

    def test_missing_bundle_never_executes_or_finds_uv(self):
        (self.bundle / "bundle.json").unlink()
        result, stdout, stderr = self.invoke("install")
        self.assertEqual(result, 1)
        self.assertEqual(stdout, "")
        self.assertIn("bundle is missing", stderr)
        self.uv.find_uv_bin.assert_not_called()
        self.child.assert_not_called()

    def test_missing_uv_package_has_actionable_diagnostic(self):
        with patch.dict(sys.modules, {"uv": None}):
            result, stdout, stderr = self.invoke("mcp")
        self.assertEqual(result, 1)
        self.assertEqual(stdout, "")
        self.assertIn("uv dependency is unavailable", stderr)
        self.assertIn("reinstall", stderr)
        self.child.assert_not_called()

    def test_missing_uv_executable_never_executes(self):
        self.uv.find_uv_bin.side_effect = FileNotFoundError("fixture uv binary is missing")
        result, stdout, stderr = self.invoke("update")
        self.assertEqual(result, 1)
        self.assertEqual(stdout, "")
        self.assertIn("fixture uv binary is missing", stderr)
        self.child.assert_not_called()

    def test_help_guidance_does_not_write_to_stdout(self):
        for arguments in ([], ["--help"], ["-h"]):
            with self.subTest(arguments=arguments):
                self.child.reset_mock()
                result, stdout, stderr = self.invoke(*arguments)
                self.assertEqual((result, stdout), (0, ""))
                self.assertIn("pip commands:", stderr)
                self.child.assert_called_once()
                self.assertEqual(self.child.call_args.args[0][1:], arguments)

    def test_python_module_entrypoint_preserves_exit_status(self):
        with patch.object(launcher, "main", return_value=19) as main:
            with self.assertRaises(SystemExit) as raised:
                runpy.run_module("bukan", run_name="__main__")
        self.assertEqual(raised.exception.code, 19)
        main.assert_called_once_with()
        self.child.assert_not_called()


if __name__ == "__main__":
    unittest.main()
