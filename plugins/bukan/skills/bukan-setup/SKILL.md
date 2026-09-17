---
name: bukan-setup
description: Set up or diagnose a local Bukan CLI installation, choose a research workspace, and connect the library and research MCP servers to an agent host.
---

# Bukan setup

Use the installed Bukan executable, or `bukan` when its `bin` directory is on PATH.
Read `bukan --help` and the relevant subcommand help before changing settings.
Keep Paperpile read-only and place research workspaces outside the application
installation and Paperpile directories.

1. Inspect `bukan paths --json` and the current workspace selection. An explicit
   workspace argument overrides `BUKAN_WORKSPACE`, which overrides the saved
   default. Preserve an existing selection unless the user asks to change it;
   an invalid configured path is a problem to resolve, not a reason to create a
   replacement research store.
2. For the standard Windows installation, run the extracted `install.cmd`, then
   install the registered Bukan plugin in Codex using the UI or the exact
   `codex plugin add ...` command printed by the installer. The installer
   prepares private uv, Poppler, and Python dependencies and runs `bukan setup`;
   preinstalled Python and global PATH changes are unnecessary. It registers a
   local marketplace entry but leaves plugin installation to the second step.
   Linux/manual installations still require dependency tools and host registration.
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

Use the bundled `bukan-paper-review` skill for scientific reading, audits,
synthesis, acquisition, and previews. Bukan stores records and performs
deterministic checks; the agent host runs models and coordinates workers.
