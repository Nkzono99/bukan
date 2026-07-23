# Literature Research Workspace

## Source of truth

- Paperpile is the source of truth for accepted references and PDFs.
- Never edit automatically exported `data/paperpile.bib`.
- Never rename, move, delete, or modify files under the Paperpile sync directory.
- Save candidate references under `candidates/` and approved imports under `imports/`.

## Research rules

- Record the question, search date, databases, queries, and inclusion/exclusion criteria.
- Deduplicate by DOI or other persistent identifier, then normalized title and first author.
- Never invent identifiers, quotations, page numbers, statistical results, or metadata.
- Do not treat citation count as evidence of quality.
- Keep discovered, verified, full-text-reviewed, and import-approved states separate.
- Cite the PDF page or section for important extracted claims.

## Bukan organization

- Use hierarchical `Bukan/` folders for fields, topics, objects, missions, and projects.
- Use flat `bukan:` labels for methods, reference types, workflow status, and keywords.
- Review generated organization suggestions before applying them in Paperpile.

## Bukan and Codex

- Bukan launches Codex with this workspace as its working directory.
- `BUKAN_PAPERPILE_ROOT` identifies the mounted Paperpile library for read-only
  library-wide searches. `BUKAN_PAPERPILE_READ_ONLY=true` is a mandatory boundary,
  not a suggestion.
- When `.bukan/current-context.md` exists, it identifies the paper currently selected
  in the Bukan viewer. Read it when the user's request refers to "this paper" or the
  current paper.
- A PDF path in `.bukan/current-context.md` is a read-only Paperpile source. It may be
  inspected, but must never be modified, moved, renamed, or deleted.
- Save durable paper notes under `notes/`, reproducible searches under `queries/`,
  and synthesized outputs under `reports/`.
- Keep temporary extraction and retrieval artifacts in `cache/`; they must remain
  reproducible from the source PDFs.
- When a search, comparison, or collection task produces a useful paper list, call
  the Bukan MCP `present_paper_list` tool so the user can review it beside the raw
  Codex terminal. This list is temporary and is not a Paperpile write.
