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
  install_local.py
  bundle.json
```

Run `python /path/to/bukan/install_local.py --bundle /path/to/bukan` with Python
3.11 or newer. It installs into `~/plugins/bukan`; use `--destination` to choose
another new directory. Existing destinations are preserved and refused. For an
upgrade, choose a new destination or explicitly archive the old installation
first. Research workspaces and the runtime cache live outside this directory.

The installer sets both MCP commands in the installed `.mcp.json` to the absolute
path of the installed binary. It does not change your shell PATH or host settings.
The portable release config uses `bukan` on PATH until installed. To use the CLI
by name, add the installed `bin` directory to PATH yourself, or invoke the printed
absolute executable path. For a new workspace, run `bukan init /path/to/workspace`,
then `bukan setup /path/to/workspace --default` to prepare research dependencies
and select the default. Use an existing workspace directly with `setup`.

Install the resulting plugin folder through the host's local plugin workflow.
When using a generic MCP host instead, `bukan mcp-config` exports ordinary MCP
configuration for the selected workspace. Workspace selection follows an explicit
argument, then `BUKAN_WORKSPACE`, then the saved Bukan default. No daemon or model
runner is included; the host provides agent execution.
