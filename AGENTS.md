# Literature Research Workspace

## Source of truth

- Paperpile is the source of truth for the accepted literature library.
- `data/paperpile.bib` is automatically exported from Paperpile.
- Never edit `data/paperpile.bib`.
- Never rename, move, delete, or modify files under the detected Paperpile directory.
- Write new candidate references to `candidates/`.
- Write references approved for import to `imports/`.

## Search workflow

1. Record the research question, inclusion and exclusion criteria, date range,
   preferred study types, and related terminology.
2. Use OpenAlex for broad discovery, Crossref for bibliographic verification,
   and PubMed/PMC for biomedical topics.
3. Deduplicate against `data/paperpile.bib` by persistent identifier first,
   then normalized title and first author.
4. Never treat citation count as evidence of quality.
5. Keep discovered, verified, full-text-reviewed, and import-approved states separate.
6. Never invent identifiers, quotations, page numbers, or results.
7. Record the search date, databases, actual queries, criteria, and ranking reasons.

## PDF policy

- Paperpile-synced files are read-only.
- Only download open-access PDFs or files the user is authorized to access.
- Save extracted text and analysis under `cache/`, `notes/`, or `reports/`.
- Cite the PDF page or section for important extracted claims.
