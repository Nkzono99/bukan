# Equations, embedded figures and target-preview QA

Use for final research outputs and display repairs. Repair the affected saved
artifact in context. Preserve existing reading/audit coverage; repeat scientific
checks only when a repair changes meaning or reveals a content error. Check
immutable source evidence without rewriting it for display convenience.

## Author portable research Markdown

Use display equations with `$$` alone on both delimiter lines and blank lines
around the block. Preserve symbols, powers, subscripts, units, assumptions and
meaning against the original expression. Do not use `\(...\)`, `\[...\]` or code
backticks as equation delimiters. Use inline `$...$` only if explicitly supported
and verified in the target preview; simple variables can be prose. A blanket
regex rewrite cannot establish that an equation was preserved.

Embed important figure/table previews with inline Markdown image syntax, outside
Markdown tables. For example, a portable document can contain:

```markdown
![Figure 2: measured response and its uncertainty](assets/paper/figure-2.png)

Source: paper title, PDF version/hash, PDF file p. 4, Fig. 2.
```

Retain axes, units, legends and relevant captions; inspect every crop, including
panel context. Store provenance with paper identity, PDF version/hash, page and
figure/table number. Promote generated assets from scratch to durable storage
before saving the note. Keep assets referenced by earlier revisions.

For ordinary Markdown, put preview copies below the document's directory and use
relative paths. For research DB notes/wiki pages, use their supported authoring
asset paths and exporter from [tools](tools.md); the portable export rewrites
images to child assets. Do not rewrite DB Markdown to a guessed export path or
manually replace an immutable export snapshot. The note exporter supports inline
Markdown images outside tables; reference-style, table-local and HTML images are
rejected. An unsupported form needs a deliberate authored-note revision.

## Verify the final saved artifact

Open the final saved note or export in the user's target application. For Bukan
wiki work, check the relevant app view; for delivered Markdown, check VS Code's
standard preview when that is the requested target. A diagnostic HTML renderer
can investigate a problem but cannot certify a different application.

Check that every embedded image loads with nonzero intrinsic dimensions, then
visually inspect readability, cropping, captions and correspondence to the source.
Inspect all equations in a new output and every changed equation in a repair for
typesetting, symbols, parser errors and clipping. File existence, image counts,
successful export and valid Markdown syntax are preliminary checks only.

If rendering fails, inspect the resolved image URL, loading/network error and,
where relevant, allowed local roots and content security policy. Fix the
demonstrated cause and reopen the saved result. Keep preview security enabled.
For a portable delivery, copy the document and assets together to another location
and inspect that relocated document in the target preview.

Record the final note/page revision or document hash, PDF hash, application,
checked scope, observed result and unresolved issues. If the target cannot be
opened, mark preview QA unverified and state the pending check. Keep scientific
audit and reading status separate. Update current editable records through the
normal revision mechanism, preserve historical/evaluation artifacts, and identify
the current corrected output. A display repair is complete when the saved result
passes its affected target checks; an unavailable target leaves that part pending.
