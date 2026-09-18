---
name: bukan-paper-review
description: Review PDFs and audit their claims in a Bukan research workspace; maintain research wiki syntheses, public-copy links, acquisition backlogs, and note equation or figure previews.
---

# Bukan paper review

Turn source papers into reusable, verifiable notes and current research
interpretations. Follow the workspace's source and revision boundaries and the
user's requested scope. This skill does not apply to Bukan application development
or unrelated Markdown editing.

Use the connected CLI/MCP workspace and its existing records. Read exported notes
and wiki pages in VS Code or the user's chosen Markdown preview, and inspect
original pages in a PDF viewer. No Bukan desktop application is required.

## Choose the needed mode

| Task | Read |
| --- | --- |
| New or substantive paper review; scientific audit of notes or Claims | [Full review](references/full-review.md) |
| Literature survey, cross-paper comparison, research history, wiki integration | [Synthesis](references/synthesis.md) |
| Locate full text, prepare public-copy links, or maintain unacquired references | [Acquisition](references/acquisition.md) |
| Repair or verify equations, figures or portable Markdown | [Preview](references/preview.md) |
| Use Bukan MCP records, persistent review tasks, asset paths or legacy tools | [Tools](references/tools.md) |

Read only the references needed for the current work. A survey calls for full
review when it introduces unread papers; an acquisition task can end at a verified
PDF and updated backlog. A display repair preserves existing reading coverage and
audit history, adding scientific revalidation only when meaning changes or an
error is exposed.
Public-link discovery can finish with dated candidates and unresolved checks;
it does not require full-paper reading or an acquired PDF, even for owned papers.

## Shared completion contract

- Keep synced Paperpile files read-only. Explicit reference registration uses
  the plugin's `bukan-paperpile` skill and Paperpile UI; discovery is not an import request.
  Save generated work in the research workspace and
  preserve human edits, immutable captures and prior revisions.
- Match reusable notes and caches to the exact PDF hash/version. Distinguish
  full-page reading, scientific audit and target-preview verification in records.
  A successful tool response does not establish scientific correctness.
- For substantive reviews, independently audit consequential note assertions
  and **every proposed or revised Claim**, including its conditions and evidence,
  before acceptance into synthesis. Record unresolved issues instead of certifying
  them. A Markdown correction alone does not correct a stored Claim.
- Use the user's chosen worker model and reasoning setting. Coordinate independent
  paper workers within the host's actual free capacity; no fixed worker cap,
  global model change or recursive delegation is implied.
- Carry authorized work through saving, integration and the checks relevant to
  its mode. Persist the exact remaining work when access or resources prevent
  completion. Label a checkpoint as a checkpoint, with a next action.
