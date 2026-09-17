# Full-text acquisition and access backlog

Use to find the exact full text of a needed paper or maintain useful references
that remain unavailable. Acquisition, reading and import/purchase decisions are
separate. A PDF link is not a retrieved PDF and a downloaded PDF is not a review.

## Resolve full text in order

1. Search mounted Google Drive/Paperpile through Bukan's read-only library index.
   Use title/authors as well as persistent identifiers; DOI metadata may be absent.
   Confirm the actual local PDF, hash, page count and version with
   `get_paper_document`. Index or Drive hydration failures do not establish
   non-ownership. An unavailable library means the local check is blocked.
2. Check the publisher's full text and available access routes when local retrieval
   does not yield the source. Record what was actually attempted and observed.
3. Check legitimate preprints, accepted manuscripts, author copies and institutional
   repositories. Verify title, authors, identifiers and version. A full alternative
   manuscript may support review; a conference abstract or slide deck does not
   replace the full article.
4. If full text remains unavailable, retain the needed paper in the access backlog
   with its relevance, attempts and next action. Continue other permitted work.

Keep explicit paywalls separate from login requirements, bot blocking, broken
links and untested routes. Record URLs and dates. An unsuccessful preprint search
means "not found in this search", not that a preprint does not exist. Do not invent
access rights, purchase status, prices or metadata.

If using an alternative manuscript, record which methods, figures, conclusions,
supplements and corrections were compared with the publication. When the published
version is unavailable, equivalence remains unchecked. Retain that limitation in
the review and affected synthesis. Save downloaded copies outside Paperpile.

## Keep one useful candidate record

Maintain matching keys in these workspace files:

| File | Content |
| --- | --- |
| `candidates/literature-access.bib` | Verified bibliographic fields for unacquired papers and candidate alternative versions |
| `candidates/literature-access.md` | Readable list explaining why each paper matters, priority and next action |
| `candidates/literature-access.json` | Dated attempts, observed access state, alternative versions and reading/acquisition state |

Deduplicate by DOI or other persistent identifier, then title/authors/year. The
JSON is a host-maintained sidecar, not an automatically ingested MCP record. Keep
`format_version: 1`, a research date and history; version incompatible changes.
Useful fields include the shared key, verified bibliography, relevance to a
question, priority, attempted URLs/dates/results, alternative identity/version
checks, acquisition state, reading state and next action. Do not put these fields
into a strict MCP entity unless its current schema supports them.

When acquired, update the same candidate with its actual source/hash/version,
remove stale purchase needs, retain access-attempt history and queue any requested
[full review](full-review.md). A candidate list does not authorize an import or
purchase. Never edit automatically exported `data/paperpile.bib` or synced
Paperpile files. Acquisition-only completion is a verified source or a documented
unresolved access record; it makes no claim about full-paper reading.
