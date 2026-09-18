---
name: bukan-setup
description: Set up or diagnose a local Bukan CLI installation, choose a research workspace, and connect the library and research MCP servers to an agent host.
---

# Bukan setup

Use `python -m bukan`, the pip-installed `bukan` command, or the installed native
executable. `install` and `update` are Python-side commands; other arguments go
to the bundled native CLI.
Read `bukan --help` and the relevant subcommand help before changing settings.
Keep synced Paperpile files read-only and place research workspaces outside the application
installation and Paperpile directories.

1. Inspect `bukan paths --json` and the current workspace selection. An explicit
   workspace argument overrides `BUKAN_WORKSPACE`, which overrides the saved
   default. Preserve an existing selection unless the user asks to change it;
   an invalid configured path is a problem to resolve, not a reason to create a
   replacement research store.
2. For a standard installation, use 64-bit Python 3.11+ to pip-install the
   matching Windows/Linux x86_64 wheel, then run `python -m bukan install`.
   PyPI publication has not happened; use the wheel path, not `pip install bukan`.
   Wheel users do not need Rust; source installation with `pip install .` does.
   Pip installation itself does not initialize user data. The explicit `install`
   command copies the toolkit into the user data directory, runs `bukan setup`,
   and registers the personal marketplace. Windows prepares private uv and
   Poppler; Linux uses pip's uv dependency and requires distribution Poppler
   on PATH. Install the registered plugin through Codex or the exact printed
   `codex plugin add ...` command. Windows users without Python can use the
   extracted `install.cmd` bootstrap instead.
3. `bukan setup` with no existing selection creates a managed workspace under
   the OS user data directory: `%LOCALAPPDATA%/bukan/workspaces/default` on
   Windows, `$XDG_DATA_HOME/bukan/workspaces/default` or
   `~/.local/share/bukan/workspaces/default` on Linux. These are persistent
   research data, separate from the installation and runtime cache. A custom
   absolute `BUKAN_DATA_DIR` changes the managed data root; it does not move
   existing workspaces. For a user-chosen new location, use `bukan init PATH`
   then `bukan setup PATH --default`. For an existing workspace, skip `init`
   and preserve customized instructions and records.
4. Inspect `bukan research --help` for direct engine operations and
   `bukan doctor` for diagnosis. The research engine uses a separate writable
   cache; do not create environments inside the plugin or Paperpile. Preparing
   dependencies may need network access.
5. The plugin starts `bukan mcp` for the library and `bukan research-mcp` for
   research records over stdio. Use `bukan mcp-config` when configuring another
   MCP host. Inspect the exported configuration before applying it to that host.
6. Verify both servers through the host and start a new conversation after
   installing or refreshing a plugin. A process starting is not evidence that
   all tools work; check the intended read operation against the chosen workspace.

For updates, install the new wheel with pip, then run `python -m bukan update`
(or repeat `install`) and refresh the Codex plugin as instructed. `update` uses
the already installed Python package; it does not fetch a newer release. Both
commands preserve existing research. Removing the pip package or its virtual
environment does not delete the copied toolkit, plugin registration, or data.

Use the bundled `bukan-paper-review` skill for scientific reading, audits,
synthesis, acquisition, and previews. Bukan stores records and performs
deterministic checks; the agent host runs models and coordinates workers.

For user-requested Paperpile registration, use `bukan-paperpile`. It requires
installed Google Chrome and one-time `bukan paperpile login`; the user signs in
in the dedicated window and closes it. `paperpile_browser_status` checks access.
Login is optional for read-only PDF research and never reuses normal Chrome's
profile. See `bukan paperpile --help` for the login/status commands.
