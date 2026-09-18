# Bukan Application Repository

This repository contains the Bukan CLI, MCP servers, research engine, Codex plugin,
shared indexing core, and workspace templates. It does not contain a user's
literature notes or Paperpile data.

## Development rules

- Keep Paperpile synced-file access read-only. Never rename, move, delete, or modify synced files. Explicitly requested reference registration goes through Paperpile's UI (or a documented API), separately from the index and PDF readers.
- Put reusable workspace defaults under `templates/workspace/`.
- Keep user-generated indexes, notes, reports, and assignments outside this repository.
- Changes to workspace formats must remain backward-compatible or increment the format version.
- Core detection and indexing behavior belongs in Rust and should be shared by CLI and MCP.
- Add tests for path validation, workspace initialization, and classification behavior.

The `templates/workspace/AGENTS.md` file contains instructions copied into newly
initialized research workspaces. Do not mix those instructions with this file.

For research instructions, bundled skills, or research task packets, use
[the harness guide](docs/research-harness.md) to find the owning files and checks.
