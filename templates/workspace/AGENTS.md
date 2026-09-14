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
- Read every page of each paper used for research synthesis, including methods,
  results, limitations, references and appendices. Abstracts and selected pages
  are for screening only; never present them as a completed literature review.
- Inspect figures, tables and equations visually. Record unread pages, extraction
  failures and inaccessible supplements explicitly. Text extraction, downloading
  and rendering do not by themselves count as reading or understanding.
- Keep a per-paper Markdown note with the PDF hash/version, total PDF page count,
  per-page text/visual reading coverage, methods, findings, assumptions, limitations,
  research insights, open questions and precise supporting page locators.
- Update those notes as understanding develops; preserve earlier interpretations
  and human additions. Search the notes before repeating analysis, then return to
  the cited original pages for verification. A note is not a replacement for the PDF.
- With the research MCP, store notes as `paper_note` through `put_records` with
  `expected_revision`, search them with `search_records(kind="paper_note")`, and
  read/export them with `get_paper_note` / `export_paper_note`. Exported Markdown
  is a revision snapshot; local edits must be incorporated into a new DB revision
  to participate in MCP searches. Do not silently overwrite or omit those edits.
- Delegate each paper's full review to a separate subagent. Use the host's actual
  free worker slots, refill them as papers finish, and keep other independent work
  within the same limit. Do not assume a requested parallelism changes host limits.
- Call `plan_paper_reviews` with the PDF hash and total pages to reuse completed
  notes and persist remaining work. Claim tasks with `expected_revision` before
  dispatch; save the worker's records and task completion in one atomic batch.
  On restart, inspect existing tasks and worker status before assigning work again.
- Reuse extraction and page-image caches for the same PDF hash. Give workers
  separate scratch folders and record IDs; the coordinator integrates outputs and
  owns shared Questions, Topics and cross-paper relations. Record failed work as
  `needs_followup` and continue other papers. Preserve concurrent human edits.
- Separate full-page coverage, scientific audit, and preview QA. A draft worker
  such as Luna max may read each paper; a stronger reviewer checks consequential
  claims, numerical reasoning, conditions, exceptions, and locators against the
  exact PDF before synthesis. Expand the audit when errors or importance warrant
  it. Complete coverage alone does not establish scientific correctness.
- Put authored equations in display blocks with `$$` alone on both delimiter
  lines and blank lines around the block. Do not use `\(...\)`, `\[...\]`, or
  backticks as equation delimiters. Preserve meaning and source uncertainties;
  never rewrite immutable Source text or exact Evidence excerpts for formatting.
- Embed important figure/table previews with `![description](image-path)`, not
  just links. Preserve axes, units, legends and relevant captions; visually inspect
  each crop and cite the PDF page, figure/table number and source version.
  Promote previews from scratch folders to durable research assets before saving
  the note. Bundle preview copies below the exported document's directory using
  relative paths, so individually opened documents can load them in VS Code.
  Open the final saved document in the target preview and inspect every figure
  and changed equation. File existence is not a rendering check. Keep preview
  security enabled, and report an unavailable preview as unverified. For portable
  delivery, copy the document and assets together and check the relocated preview.

## Research history and acquisition needs

- For substantive research synthesis, prioritize comprehensive coverage and
  detailed prior-work analysis at a scale impractical for manual review. Do not
  stop at a convenient paper count, one citation hop, or an overview. Expand
  references and later citing papers, and independently discover non-citing
  related work through alternative terminology, methods, equations and data.
- Persist separate discovery, acquisition, full-reading, claim-normalization,
  relation-analysis and audit queues. Keep a versioned coverage ledger with
  search cells, known candidate denominators, exclusions, pending work and access
  gaps. Refill available worker slots continuously; reuse exact-version caches.
  Search saturation is scoped evidence, not proof of total recall; a resource
  checkpoint does not mark the research complete.
- Compare consequential claims and equations under matched definitions, units,
  assumptions and initial/boundary conditions. Record measured versus inferred
  quantities, uncertainty and exceptions. Keep detailed relation dossiers with
  fixed evidence, retained/changed conditions, contradictions and proposed tests.
  Independently audit important links and flag dependent records when a new
  paper or correction changes their basis. Citation absence does not mean no
  relationship; a citation alone does not establish support or inheritance.
- Search for full text in order: mounted Google Drive/Paperpile via Bukan's
  read-only index, publisher, legitimate preprint/accepted/author manuscript,
  then an unacquired record. Search title/authors when DOI is not indexed.
  Confirm the actual local PDF/version. Hydration/index errors or an unavailable
  library do not establish non-ownership; retain the observed status honestly.
- Organize research histories around dated questions, contributions, conditions,
  remaining limits, and later treatment. Pin note/Source revisions and PDF page
  evidence. Separate a paper's explicit continuity from an analyst's comparison;
  publication order alone does not prove influence. Distinguish review gaps from
  field-wide open problems and check later work before claiming novelty.
- Keep useful unacquired references in `candidates/literature-access.bib` with a
  readable `.md` list and `.json` status record. Record verified metadata, why the
  paper matters, priority, attempted URLs and dates, legal alternative versions,
  and the next action. Never edit automatically exported `data/paperpile.bib`.
- Distinguish explicit paywalls from bot blocking, login, broken links, and routes
  not yet tested. Full preprints or accepted manuscripts may substitute after
  identity/version checks; mark unchecked differences from the publication and
  missing supplements. Conference abstracts do not replace full articles.
  Acquisition, full reading, and import/purchase decisions remain separate.
- Store the history in `reports/` and a versioned `data/research-history.json`.
  These files are not automatically ingested by the research MCP. Keep a short
  search entry in a Topic with pinned members; update candidates when acquired.

## Bukan organization

- Use hierarchical `Bukan/` folders for fields, topics, objects, missions, and projects.
- Use flat `bukan:` labels for methods, reference types, workflow status, and keywords.
- Review generated organization suggestions before applying them in Paperpile.
- Workspace-only collection assignments live in `data/collections.json`. They may be
  edited through Bukan, but must never be treated as authorization to modify the
  corresponding Paperpile collection tree.

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
- Prefer the Bukan MCP `search_library`, `list_collections`, `get_paper`, and
  `get_current_paper` tools over scanning the Paperpile directory manually. These
  tools expose the same read-only index used by the Viewer.
- For full reading, call `get_paper_document`, retain its SHA-256, then call
  `read_paper_pages` starting at PDF file page 1 and follow `nextPage` to the end.
  Use `read_paper_page_image` for visual inspection. These tools read the mounted
  Google Drive / Paperpile PDFs without modifying them. If the PDF hash changes,
  begin a separate version of the reading record rather than mixing pages.
- The user may explicitly save a presented list from the Viewer. Saved reports go
  under `reports/codex-lists/`; candidate sets go under `candidates/codex-lists/`.
  Neither destination is inside Paperpile.
- Living literature reviews are stored under `reports/reviews/<review-id>/`.
  Use the Bukan MCP `create_review`, `list_reviews`, `get_review`,
  `get_current_review`, and `update_review` tools to maintain them.
- Important review claims must cite a local paper ID and an exact PDF page,
  section, figure, or table locator. Never invent a quotation or locator.
- To add a figure, first extract it into this workspace (normally under `cache/`),
  then call `attach_review_figure` with its local library paper ID and PDF page.
  Bukan copies the image into the review and records provenance; never write an
  extracted image back into the Paperpile directory.
