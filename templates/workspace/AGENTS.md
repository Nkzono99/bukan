# Literature Research Workspace

This is a research workspace. Keep generated research data here, outside the
Bukan application repository. Use the user's requested scope and model settings;
do not change global model configuration for a research assignment.

## Sources and durable knowledge

- Paperpile is the source of truth for accepted references and PDFs. Never edit
  `data/paperpile.bib` or rename, move, delete, or modify synced Paperpile files.
  `BUKAN_PAPERPILE_READ_ONLY=true` is a mandatory boundary.
- Put candidate references in `candidates/` and approved import files in `imports/`.
  A candidate or workspace collection does not authorize a Paperpile write,
  import, or purchase.
- The app and research MCP share `data/research.sqlite` (`BUKAN_RESEARCH_STORE`).
  Reuse it for app requests. Wiki pages hold current interpretations; paper notes,
  Claims, exact Evidence, original PDFs and human notes remain reusable evidence.
  Generated prose is not independent primary evidence.
- Preserve human edits, stable IDs and prior revisions. Read current records
  before updates and use `expected_revision`. Source captures and Evidence are
  immutable; corrections require new records and review of affected conclusions.
- Never invent metadata, quotations, numbers, locators or verification results.
  Keep discovery, acquisition, reading, scientific audit and preview QA distinct.
  Full review means every PDF page, including references and appendices, with
  visual inspection of figures, tables and equations. Coverage alone is not a
  scientific audit. Record exact PDF versions and unresolved work honestly.
- Use standalone `$$` display blocks for authored equations. Embed important
  figure previews as Markdown images with source locators; portable exports must
  carry relative image assets below the document directory. Verify the saved
  artifact in its target preview; an unavailable preview remains unverified.

## Read the context needed for this task

- For paper reviews, scientific audits, surveys, acquisition backlogs, or research
  note display repairs, use [.agents/skills/bukan-paper-review/SKILL.md](.agents/skills/bukan-paper-review/SKILL.md)
  and its relevant mode reference. Formatting-only work does not restart a review.
- For a saved app request, read `.bukan/current-research.md` (`BUKAN_RESEARCH_FILE`)
  and the records/documents it references. Durable request copies are under
  `queries/requests/`. Inspect existing tasks and workers before resuming.
- For "this paper", read `.bukan/current-context.md` when present or use
  `get_current_paper`. Its PDF path is a read-only source.
- Use Bukan's library tools before manually scanning Paperpile. The skill's
  [tool reference](.agents/skills/bukan-paper-review/references/tools.md) explains
  the two MCPs, current record contracts, workspace paths and legacy reviews.

Complete the authorized task through saved results and relevant verification.
For additional surveys, integrate the affected wiki interpretations or record
why they remain unchanged. Persist remaining queues and restart conditions at
resource checkpoints; a finished paper batch does not complete a field survey.
