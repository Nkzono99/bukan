# Bukan plugin

In the source checkout, this directory contains the manifest and setup skill. Build a release with
`scripts/package_release.py` from the repository root before installing it.
Packaging copies the canonical `bukan-paper-review` skill from
`templates/workspace/.agents/skills/` and bundles the CLI and research engine.
Do not maintain a second copy of that review skill here.

An extracted release has this layout:

```text
bukan/
  .codex-plugin/plugin.json
  .mcp.json
  bin/bukan.exe                 # bin/bukan on Linux
  research-engine/
  skills/
  install.cmd                  # Windows entry point
  install.ps1                  # Windows bootstrap
  install_local.py
  bundle.json
```

## Windows: install the toolkit, then the plugin

With Codex installed and the Paperpile sync folder available locally:

1. Run the extracted `bukan/install.cmd`. It launches `install.ps1`, installs
   Bukan under `%LOCALAPPDATA%/bukan/installations/<version>-<bundlehash>/bukan`,
   prepares pinned uv and Poppler under `%LOCALAPPDATA%/bukan/dependencies`, and
   runs `bukan setup`. It uses the bundled executable and does not require a
   preinstalled Python, uv, administrator access, or system PATH changes.
2. Install Bukan from the personal marketplace in Codex, or run the exact
   `codex plugin add bukan@<marketplace-name>` command printed by the installer.
   The first step registers the local marketplace entry while preserving other
   entries; it does not install or enable the plugin.

Initial dependency downloads need network access. Start a new conversation after
plugin installation; restart the host if it has not picked up the new plugin.
The installed `.mcp.json` points both MCP servers at the installed binary by
absolute path. Use the printed executable path for CLI commands; adding its
`bin` directory to your user PATH is optional.

With no existing workspace selection, `setup` creates
`%LOCALAPPDATA%/bukan/workspaces/default`. An explicit workspace argument,
`BUKAN_WORKSPACE`, or a saved Bukan default takes precedence. Invalid existing
selections fail rather than creating a replacement workspace. Existing research
is not moved during installation or upgrades.

Research data stays outside the versioned installation. Settings live in
`%APPDATA%/bukan/settings.toml`; the Python runtime cache is
`%LOCALAPPDATA%/bukan/research-runtime`. Inspect actual paths with
`bukan paths --json`. `BUKAN_DATA_DIR`, `BUKAN_CONFIG_DIR`, and `BUKAN_CACHE_DIR`
accept absolute directory overrides; changing a data root does not migrate
existing workspaces. The research workspace is persistent data, not disposable
runtime cache.

For an upgrade, run the new release's `install.cmd`, then refresh the plugin in
Codex using the instructions printed by the installer. The version-and-hash
directory keeps prior installations separate; research records stay in place.

## Linux and manual installations

The automatic dependency bootstrap above is Windows-only. On Linux, prepare
Python 3.11 or newer, uv, and Poppler, then run:

```sh
python3 /path/to/bukan/install_local.py --bundle /path/to/bukan
```

This manual installer defaults to `~/plugins/bukan`; `--destination` selects a
new directory. Existing destinations are preserved and refused. It does not
prepare dependency tools or register a personal marketplace. Invoke the printed
executable to run `bukan setup`, then use the host's local plugin workflow.
For upgrades, choose a new installation directory. The default managed research
workspace is `$XDG_DATA_HOME/bukan/workspaces/default`, or
`~/.local/share/bukan/workspaces/default` when XDG_DATA_HOME is unset.

## Other MCP hosts and research execution

`bukan mcp-config` exports ordinary MCP configuration for the selected workspace.
Workspace selection follows an explicit argument, then `BUKAN_WORKSPACE`, then
the saved Bukan default; it does not infer a workspace from the current directory.
`setup` creates a managed workspace only when none is selected.

To create a workspace at a chosen location, run `bukan init /absolute/workspace`,
then `bukan setup /absolute/workspace --default`. Use an existing workspace
directly with `setup`. No daemon or model runner is included; the host provides
agent execution. Paperpile remains read-only.
