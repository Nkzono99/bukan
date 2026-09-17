# Bukan research instructions and harness

Bukan separates persistent research facts and deterministic record checks from
the agent's scientific work. Workspace instructions provide the source boundaries
and route a task to the relevant research mode. The host coordinates reading,
audits and integration; the app and MCPs make the resulting work inspectable.

This organization follows OpenAI's recommendation to keep skill descriptions
specific, disclose task guidance progressively, remove unnecessary document loads
and approval pauses, and state completion clearly. Its model-specific guidance
supports adapting task packets rather than making every model follow one detailed
recipe. These are instruction-design choices, not evidence that scientific audits
can be removed. [Rethinking skills and prompts for GPT-6 Astra, September 11, 2026](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra)

## Instruction ownership

| Location | Owns |
| --- | --- |
| Repository [AGENTS.md](../AGENTS.md) | Application-development boundaries; it is not the research workspace policy |
| [Workspace AGENTS.md](../templates/workspace/AGENTS.md) | Always-applicable source, revision and completion boundaries; contextual entry points |
| [bukan-paper-review/SKILL.md](../templates/workspace/.agents/skills/bukan-paper-review/SKILL.md) | Concise capability description and mode selection |
| Skill references | Self-contained [full review](../templates/workspace/.agents/skills/bukan-paper-review/references/full-review.md), [synthesis](../templates/workspace/.agents/skills/bukan-paper-review/references/synthesis.md), [acquisition](../templates/workspace/.agents/skills/bukan-paper-review/references/acquisition.md), [preview](../templates/workspace/.agents/skills/bukan-paper-review/references/preview.md) and [tool contracts](../templates/workspace/.agents/skills/bukan-paper-review/references/tools.md) |
| Saved request and worker packet | The user's specific question, selected records/revisions, model choice, output and completion scope |
| Runtime schemas and tests | Supported arguments, structural validation, storage and path behavior |

The template skill is the version-controlled canonical copy. New-workspace
initialization bundles its six files through the shared Rust initializer. Existing
custom files are preserved individually and missing files get bundled defaults.
Output paths are preflighted, including nested links/junctions, before writing.
An already initialized workspace remains unchanged; opening it does not upgrade
instructions. Updating a deployed workspace or personal skill therefore requires
an intentional comparison/merge that preserves local research preferences. Do not
assume which of two installed same-name skills wins discovery.

Maintain general workflow changes here, in the template skill, rather than only
patching a personal installation. Keep workspace-specific topic choices, target
preview preferences and active requests in that workspace. Update the tool
reference when contracts change, and update the bundled file list if the skill
layout changes. Existing detailed project docs remain implementation/design
references; a research workspace does not need the application checkout to use
the skill. Avoid duplicating new mandatory rules across every document.

## What the current harness actually checks

| Layer | Implemented check or behavior | Remaining agent judgment |
| --- | --- | --- |
| Rust library/PDF tools | Read-only paths, actual PDF hash/page count, pinned page retrieval and explicit extraction failures | Whether the pages were read and figures/equations understood |
| Research store | Typed entities, fixed references, revision conflicts, immutable Sources/Evidence, exact excerpt spans and atomic writes | Whether an excerpt supports the full result, conditions and caveats |
| Review planning | Matching paper/hash/page count, current complete coverage declarations, claimed task ownership and completion tied to the current note | Scientific correctness, audit independence and whether every accepted Claim was checked |
| Wiki records | Pinned basis, saved revisions, stale-basis/dependent-section work and unintegrated candidates | Scientific relevance, effect of a correction, completeness of non-citing discovery and current interpretation |
| Export | Versioned documents and assets, portable bundles, preservation of local edits | Correct equations, readable crops and successful rendering in the user's actual preview |
| Host coordination | Persistent tasks expose work that a running host can resume | Dispatch, actual worker capacity, resource checkpoints and exhaustive survey decisions |

The engine runs no LLM, crawler or background scheduler. `trace_record` follows
supporting references; it is not a general reverse scientific-dependency search.
Wiki candidate matching is lexical, not a semantic relevance judgment. Sidecar
search/acquisition ledgers are maintained by the host and are not automatically
ingested or protected by DB revision checks.

Reading, scientific audit and preview QA remain separate. The research store's
`full_text_reviewed` status records coverage declarations. A completed task or
an exact Evidence substring cannot certify a correct conclusion. An independently
audited note must also identify the final proposed/stored Claims checked: a fix
to Markdown can otherwise leave a contradictory Claim searchable in the DB.

## Task packets and acceptance

Give a paper worker its question, exact source/hash/page count, existing note and
revision, relevant mode reference, isolated output location and intended result.
The current user-selected draft route is Luna max with a stronger independent
scientific reviewer; model selection belongs to the host assignment and does not
change global configuration. Other user selections remain valid. A capable model
needs concise outcomes and boundaries; targeted help can explain fragile source
navigation or schemas without imposing a universal itinerary.

The scientific review covers consequential note assertions and all proposed or
revised Claims with their conditions and evidence. Short calculations, powers,
measurement-versus-inference distinctions and source ambiguity are checked where
relevant. Repeat affected checks after corrections and tie the audit to the
accepted versions. Full reading does not require the auditor to extract every
page again; cached exact-version material can support the independent audit.

Display-only repair loads the preview mode and the records/assets it needs. It
preserves valid reading coverage and rechecks the saved target preview. A
substantive survey additionally expands adopted references, later citations and
independently found non-citing work, with reasoned selection and persistent
pending queues. Completion includes integrating affected wiki sections or a
reasoned unchanged result. A completed small batch, exhausted budget or unread
access backlog is reported as a scoped checkpoint, never field-wide completion.

## Validate behavior with realistic work

Use `skill-creator/scripts/quick_validate.py` for skill syntax and check relative
links. These checks do not establish good research behavior. Forward tests should
use fresh agents, isolated workspace artifacts and raw source material, without
an answer key or the suspected failure in the worker prompt.

| Scenario | Observable acceptance |
| --- | --- |
| Repair one equation and broken image in an already read note | Correct saved output and relocated portable preview; coverage and immutable evidence preserved; no unsolicited full-review restart |
| Audit a draft note plus proposed Claims against a PDF | Checks both artifacts, discovers unsupported conditions/calculations and inadequate excerpts, and records remaining uncertainty |
| Resume work while a human edits the destination note | Inspects live workers/current revisions, preserves and merges edits, avoids duplicate dispatch |
| Continue a survey with an acquired batch and unexpanded references | Integrates accepted evidence, expands cited and non-citing work, and keeps unfinished search/audit work visible |
| Locate a needed paper after a Drive hydration failure | Distinguishes an unavailable local check from non-ownership, follows legitimate alternatives, updates matching Bib/Markdown/JSON candidates |

For target GUI QA, open the final Bukan page and requested VS Code Markdown
preview. Inspect every embedded image and all new/changed equations, follow
representative pinned evidence links, and verify a copied portable bundle. Syntax
tests or a substitute renderer do not establish this result. Record the checked
application/artifact revision and leave inaccessible targets explicitly unverified.
Keep forward-test observations separate from model-wide accuracy, price or speed
claims; a small trial cannot establish those comparisons.
