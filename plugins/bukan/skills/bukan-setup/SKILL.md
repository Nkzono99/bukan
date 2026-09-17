---
name: bukan-setup
description: Set up or diagnose a local Bukan CLI installation, choose a research workspace, and connect the library and research MCP servers to an agent host.
---

# Bukan setup

Use the installed Bukan executable, or `bukan` when its `bin` directory is on PATH.
Read `bukan --help` and the relevant subcommand help before changing settings.
Keep Paperpile read-only and place research workspaces outside the application
installation and Paperpile directories.

1. Inspect `bukan doctor` and the current workspace selection. An explicit
   workspace argument overrides `BUKAN_WORKSPACE`, which overrides the saved
   default. Preserve an existing selection unless the user asks to change it.
2. For a new workspace, run `bukan init /absolute/workspace`. Then run
   `bukan setup /absolute/workspace --default` to prepare research dependencies
   and select the default when that change is in scope. For an existing
   workspace, skip `init` and preserve customized instructions and records.
3. Inspect `bukan research --help` for direct engine operations and
   `bukan doctor` for diagnosis. The research engine uses a separate writable
   cache; do not create environments inside the plugin or Paperpile. Preparing
   dependencies may need network access.
4. The plugin starts `bukan mcp` for the library and `bukan research-mcp` for
   research records over stdio. Use `bukan mcp-config` when configuring another
   MCP host. Inspect the exported configuration before applying it to that host.
5. Verify both servers through the host and start a new conversation after
   installing or refreshing a plugin. A process starting is not evidence that
   all tools work; check the intended read operation against the chosen workspace.

Use the bundled `bukan-paper-review` skill for scientific reading, audits,
synthesis, acquisition, and previews. Bukan stores records and performs
deterministic checks; the agent host runs models and coordinates workers.
