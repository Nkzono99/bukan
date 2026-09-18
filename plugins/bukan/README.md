# Bukan plugin

In the source checkout, this directory contains the manifest and setup skill.
The pip wheel and release ZIP contain the compiled CLI and research engine.
Packaging copies the canonical `bukan-paper-review` skill from
`templates/workspace/.agents/skills/` and bundles the CLI and research engine.
Do not maintain a second copy of that review skill here.

The `bukan-paperpile` skill uses `paperpile_import_references` to register
user-selected references in My Library and verify presence through Paperpile's
Paste UI. Install Google Chrome and run `python -m bukan paperpile login` once;
sign in and close the dedicated window. Subsequent imports run within the MCP,
without browser-control tools in the host. Login state stays in Bukan's user-data
directory. Install Paperpile's extension in the dedicated Chrome to run the
default Auto update and PDF search after import; inspect the separate
`postprocessing` results. Use `postprocess: false` for registration only. Login
state stays separate from ordinary Chrome. Folder imports and PDF uploads are
not yet supported. See [registration](https://github.com/Nkzono99/bukan/blob/main/docs/paperpile-registration.md).
Direct access to synced Paperpile files remains read-only.

An extracted release has this layout:

```text
bukan/
  .codex-plugin/plugin.json
  .mcp.json
  bin/bukan.exe                 # bin/bukan on Linux
  research-engine/
  skills/
  install.cmd                  # Optional Windows entry point without Python
  install.ps1                  # Windows bootstrap
  install_local.py
  bundle.json
```

## Install with pip, initialize, then add the plugin

Use 64-bit Python 3.11 or newer, Codex, and a locally available Paperpile sync
folder. Choose the wheel for Windows or Linux x86_64. This is a Windows example:

```sh
python -m pip install /path/to/bukan-0.3.1-py3-none-win_amd64.whl
python -m bukan install
```

The package is not published to PyPI yet; install the wheel by path. Wheel users
do not need Rust. Source installation with `python -m pip install .` builds the
native CLI and requires a Rust toolchain. Building or installing the Python
package does not initialize AppData or register a plugin. The explicit `install`
command copies the toolkit into the OS user data directory, prepares the
research runtime through `bukan setup`, and registers the personal marketplace
entry while preserving other entries.

Then install Bukan from the personal marketplace in Codex, or run the exact
`codex plugin add bukan@<marketplace-name>` command printed by the installer.
The initialization command does not install or enable the plugin.

On Windows, the toolkit goes into
`%LOCALAPPDATA%/bukan/installations/<version>-<bundlehash>/bukan`; pinned uv and
Poppler go into `%LOCALAPPDATA%/bukan/dependencies`. On Linux, uv is a pip
dependency; install your distribution's Poppler package first so `pdfinfo`,
`pdftotext`, and `pdftoppm` are on PATH. Initialization checks these before
writing installation or workspace files. Linux uses `$XDG_DATA_HOME/bukan` or
`~/.local/share/bukan` for persistent data. Use a virtual environment if your
distribution disallows changes to system Python.

Initial dependency downloads need network access. Start a new conversation after
plugin installation; restart the host if it has not picked up the new plugin.
The installed `.mcp.json` points both MCP servers at the installed binary by
absolute path. CLI commands are available as `bukan` or `python -m bukan` in the
pip environment. They forward to the bundled native executable, except for the
Python-side `install` and `update` commands. The printed installed executable
path also works for native commands; no system PATH change is needed.

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

For an upgrade, install the new wheel with `python -m pip install --upgrade`
followed by its path, then run `python -m bukan update`. `update` activates the
currently installed Python package; it does not search for or download a newer
Bukan release. Running `install` again has the same effect. Refresh the plugin
in Codex using the printed instructions. The version-and-hash directory keeps
prior installations separate; research records stay in place.

The copied toolkit and research data are independent of the pip environment.
`pip uninstall bukan` does not remove them, their installed dependencies, or the
registered plugin. The MCPs continue working while the prepared research runtime
is present. To rebuild a cleared runtime cache on Linux, reinstall the wheel
with pip and run `python -m bukan install` so uv is available again.

## Build a wheel from source

From the repository root, run `python -m pip install build`, then
`python -m build --wheel`. This builds the native Rust CLI and packages it with
the engine and skills without initializing user data. The default target is
Windows MSVC or Linux GNU. Release CI explicitly builds the static Linux musl
target; local GNU wheels depend on their build environment.

`BUKAN_BUILD_TARGET` selects a supported native target and wheel tag.
`BUKAN_BUILD_BINARY` can supply an already built executable for that target,
skipping the Rust build. The target must match the executable; changing its tag
alone does not make a binary portable.

## Windows without Python and manual installations

Windows users without Python can still run `bukan/install.cmd` from an extracted
release ZIP, then add the plugin in Codex. This bootstrap prepares Python, uv,
and Poppler without administrator access or system PATH changes. Run the new
release's `install.cmd` for an upgrade, then refresh the plugin as instructed.

For manually managed dependencies and host registration, prepare Python 3.11 or
newer, uv, and Poppler, then run:

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
