# Surveys, research history and wiki integration

Use for substantive literature synthesis or an update to an existing research
interpretation. Maintain a wiki whose combined coverage and detail can support a
large field review: a concise overview leads to question pages, condition-level
comparisons, derivations, disagreements and original evidence. Retain reusable
paper notes and structured Claims; a new standalone report is optional.

## Start from the question and the current evidence

Read the requested wiki pages/sections, pinned evidence, unresolved tasks and
unintegrated candidates. Record scope, research/search dates, databases, queries,
inclusion/exclusion criteria and the deliverable. Infer routine details from the
request; do not introduce a permission pause for already authorized research.
Search existing notes before repeating analysis, then verify relevant source
pages and exact versions. Use [full review](full-review.md) for newly used papers,
including papers required to establish a cited claim's original basis.

Distinguish a bounded comparison from an exhaustive survey. For substantive
surveys, the user's standard is comprehensive prior-work analysis, not a convenient
paper count, famous-paper selection, a single citation hop or a fluent overview.
Persist discovery, acquisition, full-reading, Claim normalization, relation-audit
and synthesis work separately, so a finished stage cannot conceal another backlog.

## Expand cited and non-citing work

Repeatedly inspect references and later citing papers from adopted papers, keeping
each expansion's target count, progress and selection decisions. Independently
search non-citing work using alternative terminology, mechanisms, equations,
methods, data, observables and neighboring fields. Citation absence does not imply
absence of a scientific relation; a citation does not establish support, influence
or model inheritance. Do not use citation count as evidence of quality.

Deduplicate by persistent identifier, then normalized title and authors; distinguish
different versions of the same work. Log search cells and results, exclusions,
unexpanded papers, unfinished result pages, access failures and unchecked original
sources. Keep useful missing papers in the [acquisition backlog](acquisition.md).
Refill workers within actual free capacity while the coordinator continues
independent integration. A resource checkpoint is not a survey stopping criterion.

## Compare results under their actual conditions

Normalize claims at the level needed to test the research question. Preserve
definitions, variable/equation mappings, units, assumptions, initial/boundary
conditions, measured versus inferred quantities, uncertainty and exceptions.
Separate distinct experiments or models in one paper. Record conversions and
original values. A missing report, an unchecked quantity and a measured zero are
different states.

Prioritize consequential dependencies, conflicts and bridges between fields.
Maintain detailed relation dossiers with both fixed source/Claim revisions,
comparison question, matched and differing conditions, what is retained/changed,
supporting and opposing evidence, unresolved alternatives and proposed tests.
Keep unexamined relations visible; do not replace meaningful analysis with shallow
entries for every paper pair. Shared data or apparatus can make apparent
replications dependent. Examine differing conditions before claiming contradiction.

Independently audit consequential relations and apparent refutations against
both original sources after the per-paper audits. Record comparison evidence
and resolution. Use `author_explicit` only for a relation established by the
authors' actual wording; use `analyst_inference` for the reviewer's comparison.
Keep a simple citation in the discovery ledger when no supported Relation type
fits. The supported types are listed in [tools](tools.md).

For research histories, organize dated contributions around persistent questions:
what was addressed, what was established under which conditions, what remained,
and how later work treated it. Separate publication, preprint, observation and
correction dates. Publication order alone does not establish influence. Describe
evidence modes and conditional progress without invented percentages. A review
gap is not automatically a field-wide open problem, priority claim or absence of
follow-up; check later and independently discovered work before concluding.

## Integrate into the same living interpretation

Update stable wiki pages/sections with fixed evidence revisions and a reason for
the change. Preserve human additions, prior interpretations and original notes.
Workers return isolated proposals; one coordinator integrates a shared page.
Independent pages can have separate owners. Use current revisions and merge
conflicts rather than refreshing a revision number merely to overwrite edits.

Keep current interpretation, author statements and human research proposals
distinct. Summaries must retain important caveats and point to detailed
comparisons. A wiki paragraph or report is a derived view, not new primary evidence.
Use supported wiki basis references for scientific dependence and navigation
links for browsing. On a corrected Claim or new source version, inspect affected
comparisons and dependent summaries. Previously unlinked papers also need a
relevance decision; string matching is only a candidate generator.

The engine can find stale wiki basis revisions and their dependent sections;
the coordinator assesses the scientific effect, including replacements of
immutable Sources and previously unlinked evidence. Preserve accepted corrections,
research constraints and next actions from conversation in durable records.
Do not delete intermediate evidence just because the wiki summarizes it.

## Durable queues and honest stopping

Use `review_task` for paper ownership and reading, and `wiki_task`/`wiki_candidate`
for page updates and unintegrated inputs. Other queues can live in versioned
workspace sidecars, for example `data/review-coverage.json` and
`candidates/research-frontier.json`; relation dossiers can live in
`reports/relations/`. These are host-maintained files, not additional MCP states
or a built-in scheduler.

Record stable work IDs, purpose, target, priority/reason, state, actual owner,
attempts, pinned inputs/hashes, dependencies, outputs, completion condition and
next action. Keep search-cell denominators, exclusions, unread/unacquired papers,
unchecked relations and access gaps in the coverage ledger. Version new sidecar
formats (`format_version: 1`) and preserve compatibility or increment the format
on breaking changes. A single coordinator checks the current file hash before
replacing a sidecar; DB revision protection does not apply to ordinary JSON files.

On resume, check actual workers and saved outputs before reassigning work. Elapsed
time alone does not mean a worker is gone. On input changes, identify affected
downstream work through pinned references/dependencies, retain historical results
and revalidate current conclusions. `trace_record` follows support forward;
it does not perform a general reverse scientific-dependency search.

A scoped search can reach operational saturation only after all declared search
cells and adopted citation expansions are processed, candidates have reasoned
decisions, relevant original-source dependencies are checked, and broader searches
produce no additional relevant candidates. Record unresolved access constraints
and their impact; unavailable search routes or adopted but unexpanded papers
prevent declaring that search complete. Saturation under stated methods is not
proof of total recall.

Finish an additional survey by recording each affected page/section as revised,
unchanged after comparison, or unfinished with a reason and next action. Persist
accepted result revisions and [preview checks](preview.md). Outstanding required
discovery, reading, audit or integration keeps the overall survey incomplete.
A time, token or capacity boundary yields a resumable checkpoint, even when a
batch of papers has been fully reviewed.
