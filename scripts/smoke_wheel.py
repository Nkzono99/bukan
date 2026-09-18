#!/usr/bin/env python3
"""Install a real wheel into a disposable profile and exercise its MCP entry points."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import venv


def mcp_probe(binary: Path, command: str, env: dict) -> None:
    process = subprocess.Popen([str(binary), command], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
    messages: queue.Queue[str] = queue.Queue()

    def read() -> None:
        for line in process.stdout:
            messages.put(line)
        messages.put("")

    threading.Thread(target=read, daemon=True).start()

    def send(message: dict) -> dict | None:
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()
        if "id" not in message:
            return None
        response = json.loads(messages.get(timeout=60))
        assert response.get("id") == message["id"] and "result" in response, response
        return response["result"]

    try:
        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "bukan-wheel-smoke", "version": "1"}}})
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        listing = send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        assert listing["tools"], listing
        process.stdin.close()
        process.wait(timeout=15)
        assert process.returncode == 0, process.stderr.read()
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdout.close()
        process.stderr.close()


def smoke(wheel: Path) -> None:
    # The child process alone sees the synthetic user profile. Never touch the
    # caller's actual default workspace, marketplace, or Python installation.
    with tempfile.TemporaryDirectory(prefix="bukan-pip-smoke-") as temporary:
        root = Path(temporary)
        profile = root / "User 研究 O'Brien"
        profile.mkdir()
        environment = root / "venv"
        venv.EnvBuilder(with_pip=True).create(environment)
        scripts = environment / ("Scripts" if os.name == "nt" else "bin")
        python = scripts / ("python.exe" if os.name == "nt" else "python")
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("BUKAN_", "UV_", "PYTHON"))}
        env.update({"HOME": str(profile), "USERPROFILE": str(profile),
                    "APPDATA": str(profile / "AppData/Roaming"),
                    "LOCALAPPDATA": str(profile / "AppData/Local"),
                    "BUKAN_DATA_DIR": str(profile / "data"),
                    "BUKAN_CONFIG_DIR": str(profile / "config"),
                    "BUKAN_CACHE_DIR": str(profile / "cache")})
        if os.name == "nt":
            env["PATH"] = os.environ["SystemRoot"] + r"\System32;" + os.environ["SystemRoot"]
        else:
            env["PATH"] = "/usr/bin:/bin"

        def run(*arguments: str) -> str:
            return subprocess.check_output([str(python), *arguments], env=env, cwd=root, encoding="utf-8", timeout=360)

        print(run("-m", "pip", "install", "--disable-pip-version-check", str(wheel)), flush=True)
        run("-m", "bukan", "--help")
        paths = json.loads(run("-m", "bukan", "paths", "--json"))
        assert paths["selectedWorkspace"] is None
        assert not (profile / "data").exists() and not (profile / "config").exists()
        assert not (profile / ".agents").exists(), "pip/read-only commands must not initialize the profile"
        market = profile / ".agents/plugins/marketplace.json"
        market.parent.mkdir(parents=True)
        market.write_text(json.dumps({"name": "local-test", "plugins": [{"name": "keep", "source": {
            "source": "local", "path": "./plugins/keep"}}]}), encoding="utf-8")
        print(run("-m", "bukan", "install"), flush=True)
        marketplace = json.loads(market.read_text(encoding="utf-8"))
        assert marketplace["plugins"][0]["name"] == "keep"
        installed = profile / marketplace["plugins"][1]["source"]["path"]
        binary = installed / "bin" / ("bukan.exe" if os.name == "nt" else "bukan")
        config = json.loads((installed / ".mcp.json").read_text(encoding="utf-8"))["mcpServers"]
        assert all(Path(server["command"]) == binary for server in config.values())
        workspace = profile / "data/workspaces/default"
        store = workspace / "data/research.sqlite"
        before = store.read_bytes()
        registry_before = market.read_bytes()
        print(run("-m", "bukan", "update"), flush=True)
        assert store.read_bytes() == before and market.read_bytes() == registry_before
        # Exercise the generated console entry point too, independent of PATH.
        version = subprocess.check_output([str(scripts / ("bukan.exe" if os.name == "nt" else "bukan")), "--version"], env=env, encoding="utf-8")
        assert version.startswith("bukan "), version
        run("-m", "pip", "uninstall", "-y", "bukan")
        assert store.read_bytes() == before
        # AppData/XDG copies must survive removal of the pip environment package.
        for server in config.values():
            host_env = {key: value for key, value in env.items() if not key.startswith(("BUKAN_", "XDG_"))}
            host_env.update(server["env"])
            mcp_probe(Path(server["command"]), server["args"][0], host_env)
        assert store.read_bytes() == before
        print("PASS: pip has no setup side effects; install/update preserve research and registry; both MCPs survive pip uninstall.", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wheel", type=Path)
    args = parser.parse_args()
    if args.wheel.is_dir():
        wheels = list(args.wheel.glob("*.whl"))
        if len(wheels) != 1:
            parser.error("The directory must contain exactly one wheel.")
        args.wheel = wheels[0]
    smoke(args.wheel.resolve(strict=True))
