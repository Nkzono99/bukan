# Bukan tool and storage reference

Use when interacting with a Bukan workspace. Consult live tool schemas and
descriptions for fields and limits; the generic wiki request needs the bundled
operation reference below. Do not invent fields for audits, queues or scores.
Tool names below omit host-specific prefixes.

## Two MCPs, one research store

The literature MCP uses Bukan's read-only mounted Paperpile index and PDF access.
The research MCP stores analysis in `data/research.sqlite`, also used by
`bukan research <workspace> -- <engine args>`. The host selects models and runs workers; the
engine does not run an LLM, crawler, scheduler or autonomous background research.
Saving a task does not start a worker.

`bukan setup` prepares the research runtime and, only when no workspace is
selected, creates a managed workspace in the OS user data directory. Windows
defaults to `%LOCALAPPDATA%/bukan/workspaces/default`; Linux uses
`$XDG_DATA_HOME/bukan/workspaces/default` or `~/.local/share/bukan/workspaces/default`.
These are persistent research data. `bukan paths --json` reports the data,
configuration, cache, and workspace locations. An absolute `BUKAN_DATA_DIR`
overrides the managed data root without moving existing research.

The pip distribution also exposes these commands through `python -m bukan`.
After installing the OS-specific wheel, `python -m bukan install` prepares the
toolkit, runtime, and personal plugin registration; pip installation alone does
not initialize user data. `python -m bukan update` applies an already upgraded
pip package while preserving the selected workspace. These installation commands
belong to the Python launcher, not the native executable or research MCP.

Workspace selection is explicit path, then `BUKAN_WORKSPACE`, then Bukan's saved
default; the current directory is not a fallback. Invalid configured paths must
be resolved, not replaced with a new empty store. Use
`bukan setup <workspace> --default` to select an existing workspace explicitly.
`bukan mcp-config <workspace> --format toml` prints the library and research MCP
configuration without changing the host's global settings.

Read the requested records and tasks directly through the MCP. Earlier saved
requests under `queries/requests/` remain useful context. Legacy
`.bukan/current-research.md` and `.bukan/current-context.md` files may be retained,
but are not automatically refreshed by the CLI. Resolve a paper through its ID
or metadata rather than assuming a GUI selection. Generated files belong in this
research workspace, outside the application checkout and read-only Paperpile.

## Library and PDF operations

| Tool | Use |
| --- | --- |
| `workspace_context` | Confirm the bound workspace and Paperpile source, including an unavailable source, before research |
| `search_library`, `list_collections` | Search title, authors, year, filename and collection in the read-only index before manually scanning folders |
| `get_paper(paperId)` | Resolve library metadata for the requested paper |
| `get_paper_document(paperId)` | Retrieve the actual PDF's SHA-256 and total PDF file page count |
| `read_paper_pages(paperId, sha256, startPage, pageCount)` | Read pinned page text; start at 1 and follow `nextPage` until null for a full read |
| `read_paper_page_image(paperId, sha256, page)` | Inspect a pinned page image; use higher-resolution local inspection if details cannot be read |
| `present_paper_list` | Save a structured list to workspace `.bukan/paper-list.json`; returns the saved list and path |
| `get_paper_list`, `persist_paper_list(destination)` | Read the saved list or export it to `reports` (Markdown) or `candidates` (JSON) |

`get_current_paper` and `get_current_review` remain compatibility tools for old
context files; they do not track a viewer selection in this workflow. Use explicit
IDs for current research. List saving writes only the research workspace, never
Paperpile. `clear_paper_list` clears the saved current list.

PDF tools use Bukan's installed Poppler or a manually prepared Poppler on PATH.
On Windows, `python -m bukan install` or the optional `install.cmd` prepares
private dependencies. Linux installations require distribution Poppler on PATH.
PDF access can trigger Drive hydration but does not modify
originals. Page batches are 1–10 pages and page images have a maximum dimension of
2,000 pixels. Failed extraction or a changed hash must stay visible; retrieval
does not assess reading completion. An external PDF can be captured as a research
Source with its verified identity/version without a library write.

## Research records and revisions

Public-copy metadata uses five dedicated research tools: `find_public_versions`
(1–20 paper IDs/DOIs/bibliographic queries), `check_public_urls` (1–20 URLs),
`save_public_access` (whole report plus expected revision), `get_public_access`
(current or historical report), and `search_public_access` (query/pagination).
Retrieval returns JSON plus Markdown. These tools do not require PDF reading;
see [acquisition](acquisition.md) for public-link preparation versus acquisition.
Use `save_public_access`, not `put_records`, for this format. CLI clients can send
JSON to `bukan research <workspace> -- public-access` with operation
`find`, `check`, `save`, `get`, or `search` and the corresponding tool arguments.

`search_records(query, kind, limit, offset)` searches current records by literal
substring, including `kind="paper_note"`; paginate as needed. It is not semantic
search. `get_record(record_id, revision)` and `get_history(record_id)` retrieve
fixed/current records and history. `trace_record` follows pinned supporting
references, not general reverse dependencies.

`put_records(writes)` accepts up to 200 complete `Write` objects atomically. Each
has `entity` and `expected_revision` (0 for creation). References are `{id, revision}`.
Use returned revisions, read before updating, and reread/merge on conflicts.
Source captures and Evidence spans are immutable; changed content needs a new ID.

| Entity | Relevant contract |
| --- | --- |
| `source` | `paper`, `uri`, `version`, `locator`, `source_type` (`abstract`, `body`, `metadata`), `asset_sha256`, exact `text` |
| `evidence` | `source`, zero-based `start`, exclusive `end`, exact `excerpt` matching `source.text[start:end]` |
| `claim` | `text`, `claim_type` (`finding`, `method`, `assumption`, `limitation`), nonempty `conditions`, pinned `evidence` |
| `relation` | Distinct `source_claim`/`target_claim`, `comparison_basis`, `evidence`, `interpretation`; types `extends`, `uses_method`, `supports`, `challenges`, `differs_in_conditions`; `extends` requires `retained` and `changed` |
| `paper_note` | `source`, `title`, `markdown`, `page_count`, per-page `coverage`, optional pinned `basis` |
| `question`, `topic` | Reusable questions and search entries with pinned basis/members; preserve existing entries |

Every entity also has `kind` and `id`. These are selected fields, not complete
creation examples. Coverage entries use `page`, `text` (`reviewed`, `unread`,
`failed`), `visuals` (`reviewed`, `not_present`, `unread`, `failed`) and optional
`note`. `full_text_reviewed` is derived from reader-reported coverage; it is not
a writable audit flag. Exact excerpt validation does not prove semantic support.

## Persistent per-paper coordination

Call `plan_paper_reviews(targets, requested_parallelism, available_workers)` with
targets containing `paper_id`, `asset_sha256`, `page_count` and optional `note_id`.
Pass actual currently free worker slots, excluding the coordinator and all other
active work. Zero allows planning without dispatch. The planner reuses fully
covered current body notes for the same paper/hash/page count.

Claim at most returned `dispatch_count` tasks from `ready`: submit the complete
returned `review_task` through `put_records`, use its returned revision, set
`state="running"` and a unique `worker`. Dispatch only after a successful new
claim; an `unchanged` replay does not authorize another worker. Keep the claimed
`note_revision`, PDF identity and owner. Workers return isolated write proposals.

Save the accepted `paper_note` and claimed task's completion in one atomic batch,
along with new supporting records that fit. Use the task's `note_revision` for
the note write and the current task revision for the task write. `result_note`
must pin the resulting current, fully covered body note for the assigned paper,
hash and page count. Supporting records may be prepared in prior batches if
needed; never mark completion before the note and its references are valid.

Task states are `pending`, `running`, `completed`, `needs_followup`. Failure
requires `problem` and retains the assigned worker. On restart inspect the actual
worker and saved outputs; there are no time-based leases. Stop or confirm an
abandoned worker ended before moving it to follow-up. To retry a resolved task,
reset to `pending`, clear `worker`, `problem`, `result_note`, and set the current
`note_revision`. Preserve and merge concurrent human edits before replanning.

## Wiki updates and asset export

The CLI accepts the same operation envelope on stdin through
`bukan research <workspace> -- request`. The lower-level command is
`bukan-research --store PATH request`; `desktop` is a compatibility alias.

`research_wiki_request` exposes `request` as a generic object in its MCP input
schema, not a discriminated per-operation schema. Its tool description summarizes
the API; the table below supplies the accepted operation arguments for standalone
workspaces. Send a version-1 envelope, for example:

```json
{"request":{"version":1,"operation":"wiki-home","limit":50}}
```

Inside `request`, `operation` is required and `version` defaults to 1; no other
version or unknown fields are accepted. Argument names are camelCase. Unless
specified below, optional arguments can be omitted.

| Operation | Required arguments beyond `operation` | Optional arguments |
| --- | --- | --- |
| `wiki-status`, `migrate`, `wiki-refresh` | None | None |
| `wiki-home` | None | `query`, `pageType`, `category`, `offset`, `limit` |
| `summary` | None | `query`, `kind`, `offset`, `limit` |
| `get` | `id` | `revision` (omitted/null reads current) |
| `create-wiki` | `title` | `pageType` (default `concept`), `parentId` |
| `save-wiki` | `id`, `expectedRevision`, `title`, `sections` (objects with `id`, `title`, `markdown`) | `summary` (default empty), `changeReason` |
| `save-note` | `id`, `expectedRevision`, `markdown` | None; edits only `paper_note` body text |
| `create-wiki-task` | `pageId`, `purpose` | `pageRevision`, `sectionId`, `conditions`, `searchThrough` |
| `update-wiki-task` | `id`, `expectedRevision`, `state` | `worker`, `checkpoint`, `reason`, `outcome`, `resultPageRevision` (required for completion) |
| `decide-wiki-candidate` | `id`, `expectedRevision`, `state` | `pageId`, `reason` (required except when pending), `resultPageRevision` (required for integration) |

Search defaults are empty `query`, `offset=0`, `limit=50`; offsets are nonnegative
integers and limits are 1–200. Revisions are positive integers, not strings.
`wiki-status` reads store status, `migrate` initializes or upgrades the store
(backing up format 1 before upgrade), and `wiki-refresh` writes derived work
updates before returning the wiki home. `wiki-home` exposes pages/work/candidates;
`summary` lists records. Page creation accepts `portal`, `theme`, `concept`,
`model`, `method`, `material`, `question`, `comparison`, `history`, `glossary`.

`save-wiki` replaces the title, summary and section list. Send all sections and
the summary to retain them. Matching section IDs retain basis and other metadata,
but changed section titles or Markdown clear inherited audits, reset review status
to `unverified`, and clear the page's `verified_at`. Page title/summary changes
also clear `verified_at`.
Full structured `wiki_page` basis/audit changes use `put_records`. Navigation
points to current page IDs; scientific basis pins revisions and optional sections.

Pin the viewed input with `pageRevision` when creating a task (omitted/null uses
current); `sectionId` must exist in that pinned page. Task states are `pending`,
`running`, `completed`, `needs_followup`; outcomes are `revised`, `unchanged`,
`incomplete`. Running needs a nonempty worker; follow-up needs a reason. Completion
needs a nonempty reason, `revised`/`unchanged` outcome and an explicitly supplied
`resultPageRevision` matching the assigned page's current revision. A `revised`
result must be newer than the task's pinned input. Omitted/null optional update
fields retain their stored values; setting `pending` clears outcome/result.

Candidate states are `pending`, `accepted`, `rejected`, `integrated`. Every
non-pending decision needs a reason. Acceptance/integration needs `pageId` or an
existing destination. Integration additionally requires the explicitly compared
current `resultPageRevision`, whose evidence chain must contain the candidate's
pinned record. Result revisions are applied only for task completion or candidate
integration. Record scientific comparison separately from these structural checks.

Use `get_paper_note`/`export_paper_note` and `export_wiki_page` for current or pinned
revisions. Exports are snapshots. Local edits must be preserved and incorporated
into a new DB revision to be searchable; exporters refuse to overwrite edits.

Paper-note authoring images live under DB-adjacent `note-assets/`. Relative paths
in DB Markdown resolve from the legacy note document directory, so
`../../../note-assets/<paper>/<image>.png` is a supported authoring path. The
exporter returns a format-2 `rN/index.md` bundle with child `assets/` images. Use
the returned path, not a guessed hash directory, and copy its whole directory for
portable delivery. Wiki Markdown resolves from `data/wiki/<page hash>.md`, uses
`../wiki-assets/` for local images, and freezes images in SQLite per revision.
Its exporter also provides child assets. See [preview](preview.md) for actual QA.

## Workspace files and legacy reviews

Keep independent notes in `notes/`, searches in `queries/`, reproducible temporary
artifacts in `cache/`, candidate references in `candidates/` and approved import
files in `imports/`. Wiki records are the current cross-paper interpretation.
Existing `reports/`, `reports/relations/` and versioned
`data/research-history.json` remain useful prior artifacts; the research MCP does
not automatically ingest them or other sidecars.

Legacy living reviews under `reports/reviews/<review-id>/` still use literature
tools `create_review`, `list_reviews`, `get_review`, `get_current_review` and
`update_review`. Preserve history when updating them. Important claims use local
paper IDs and exact locators, for example `[@paper-id, p. 12]`.
`attach_review_figure` copies an image extracted into this workspace into the
review and records provenance; supply `sourcePaperId` and `page` with its caption.
It does not copy anything back into Paperpile. Existing saved lists under
`reports/codex-lists/` or `candidates/codex-lists/` remain workspace artifacts;
new useful search results can be saved directly to workspace reports/candidates.

Use hierarchical `Bukan/` folders for fields/topics/objects/missions/projects and
flat `bukan:` labels for methods/types/status/keywords in organization proposals.
Workspace assignments in `data/collections.json` do not authorize changes to the
Paperpile collection tree. Review organization suggestions before any separately
authorized application to Paperpile.
