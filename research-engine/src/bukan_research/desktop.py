"""Version-1 JSON bridge for the desktop research workspace.

The GUI uses the same Store operations as MCP. Only note body text is editable;
source references, reading coverage and provenance stay pinned to their revisions.
"""

from contextlib import contextmanager
import html
import json
import re
import uuid
from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

from .models import ENTITY, Identifier, PaperNote, WikiPage, WikiSection, WikiTask, WikiCandidate, Ref, Write, references
from .notes import export_note, get_note, note_authoring_path
from .store import Store

SQLITE_MAX_INTEGER = 2**63 - 1


class Request(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    version: Literal[1] = 1


class SummaryRequest(Request):
    operation: Literal["summary"]
    query: str = ""
    kind: str | None = None
    offset: int = Field(default=0, ge=0, le=SQLITE_MAX_INTEGER)
    limit: int = Field(default=50, ge=1, le=200)


class GetRequest(Request):
    operation: Literal["get"]
    id: Identifier
    revision: int | None = Field(default=None, ge=1, le=SQLITE_MAX_INTEGER)


class SaveNoteRequest(Request):
    operation: Literal["save-note"]
    id: Identifier
    expectedRevision: int = Field(ge=1, le=SQLITE_MAX_INTEGER)
    markdown: str = Field(min_length=1, max_length=100000)


class WikiStatusRequest(Request):
    operation: Literal["wiki-status", "migrate", "wiki-refresh"]


class WikiHomeRequest(Request):
    operation: Literal["wiki-home"]
    query: str = ""
    pageType: str | None = None
    category: str | None = None
    offset: int = Field(default=0, ge=0, le=SQLITE_MAX_INTEGER)
    limit: int = Field(default=50, ge=1, le=200)


class EditSection(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    id: Identifier
    title: str = Field(min_length=1, max_length=20000)
    markdown: str = Field(max_length=200000)


class SaveWikiRequest(Request):
    operation: Literal["save-wiki"]
    id: Identifier
    expectedRevision: int = Field(ge=1, le=SQLITE_MAX_INTEGER)
    title: str = Field(min_length=1, max_length=20000)
    summary: str = Field(default="", max_length=20000)
    sections: list[EditSection] = Field(max_length=200)
    changeReason: str = "Desktop edit"


class CreateWikiRequest(Request):
    operation: Literal["create-wiki"]
    title: str = Field(min_length=1, max_length=20000)
    pageType: str = "concept"
    parentId: Identifier | None = None


class CreateWikiTaskRequest(Request):
    operation: Literal["create-wiki-task"]
    pageId: Identifier
    pageRevision: int | None = Field(default=None, ge=1, le=SQLITE_MAX_INTEGER)
    sectionId: Identifier | None = None
    purpose: str = Field(min_length=1, max_length=20000)
    conditions: str = ""
    searchThrough: str = ""


class UpdateWikiTaskRequest(Request):
    operation: Literal["update-wiki-task"]
    id: Identifier
    expectedRevision: int = Field(ge=1, le=SQLITE_MAX_INTEGER)
    state: Literal["pending", "running", "completed", "needs_followup"]
    worker: str | None = None
    checkpoint: str | None = None
    reason: str | None = None
    outcome: Literal["revised", "unchanged", "incomplete"] | None = None
    resultPageRevision: int | None = Field(default=None, ge=1, le=SQLITE_MAX_INTEGER)


class DecideWikiCandidateRequest(Request):
    operation: Literal["decide-wiki-candidate"]
    id: Identifier
    expectedRevision: int = Field(ge=1, le=SQLITE_MAX_INTEGER)
    state: Literal["pending", "accepted", "rejected", "integrated"]
    pageId: Identifier | None = None
    reason: str = ""
    resultPageRevision: int | None = Field(default=None, ge=1, le=SQLITE_MAX_INTEGER)


REQUEST = TypeAdapter(Annotated[SummaryRequest | GetRequest | SaveNoteRequest | WikiStatusRequest | WikiHomeRequest | SaveWikiRequest | CreateWikiRequest | CreateWikiTaskRequest | UpdateWikiTaskRequest | DecideWikiCandidateRequest,
                                Field(discriminator="operation")])


class _TransactionStore(Store):
    """Reuse Store validation within one outer transaction, including exports."""

    def __init__(self, store, db):
        self.path = store.path
        self._db = db

    @contextmanager
    def connect(self, *, write=False):
        yield self._db


def handle_request(store: Store, payload, *, author="desktop-user"):
    request = REQUEST.validate_python(payload)
    if isinstance(request, WikiStatusRequest) and request.operation in {"wiki-status", "migrate"}:
        return {"version": 1, **(store.migrate() if request.operation == "migrate" else store.status())}
    mutations = (SaveNoteRequest, SaveWikiRequest, CreateWikiRequest, CreateWikiTaskRequest, UpdateWikiTaskRequest, DecideWikiCandidateRequest, WikiStatusRequest)
    with store.connect(write=isinstance(request, mutations)) as db:
        current = _TransactionStore(store, db)
        match request:
            case WikiHomeRequest():
                from .wiki import wiki_home
                result = wiki_home(current, request.query, request.pageType, request.category, request.offset, request.limit)
            case WikiStatusRequest():
                from .wiki import sync_work, wiki_home
                sync_work(db)
                result = wiki_home(current)
            case CreateWikiRequest():
                from .wiki import wiki_detail
                page = WikiPage(id=f"wiki-{uuid.uuid4().hex}", title=request.title, page_type=request.pageType,
                                parent_ids=[request.parentId] if request.parentId else [],
                                change_reason="Created in desktop")
                current.put([Write(entity=page)], author=author)
                result = wiki_detail(current, page.id, strict_export=True)
            case SaveWikiRequest():
                from .wiki import wiki_detail
                record = current.get(request.id)
                page = WikiPage.model_validate(record["entity"])
                prior = {section.id: section for section in page.sections}
                sections = [WikiSection.model_validate({**(prior[section.id].model_dump() if section.id in prior else {}),
                                                       **section.model_dump()}) for section in request.sections]
                page = WikiPage.model_validate({**page.model_dump(), "title": request.title, "summary": request.summary,
                                               "sections": sections, "change_reason": request.changeReason})
                saved = current.put([Write(entity=page, expected_revision=request.expectedRevision)], author=author)[0]
                result = wiki_detail(current, page.id, saved["revision"], strict_export=True)
            case CreateWikiTaskRequest():
                page = current.get(request.pageId, request.pageRevision)
                task = WikiTask(id=f"wiki-task-{uuid.uuid4().hex}", page=Ref(id=request.pageId, revision=page["revision"]),
                                section_id=request.sectionId, purpose=request.purpose, conditions=request.conditions,
                                search_through=request.searchThrough)
                current.put([Write(entity=task)], author=author)
                result = {"id": task.id, "task": current.get(task.id)}
            case UpdateWikiTaskRequest():
                record = current.get(request.id)
                task = WikiTask.model_validate(record["entity"])
                changes = request.model_dump(include={"state", "worker", "checkpoint", "reason", "outcome"}, exclude_none=True)
                if request.state == "completed":
                    if request.resultPageRevision is None:
                        raise ValueError("Completing a wiki task requires the explicitly compared resultPageRevision.")
                    changes["result_page"] = {"id": task.page.id, "revision": request.resultPageRevision}
                if request.state == "pending":
                    changes.update(outcome=None, result_page=None)
                task = WikiTask.model_validate({**task.model_dump(), **changes})
                current.put([Write(entity=task, expected_revision=request.expectedRevision)], author=author)
                result = {"task": current.get(task.id)}
            case DecideWikiCandidateRequest():
                record = current.get(request.id)
                candidate = WikiCandidate.model_validate(record["entity"])
                page_id = request.pageId or candidate.page_id
                if request.state == "integrated" and request.resultPageRevision is None:
                    raise ValueError("Integrating a candidate requires the explicitly compared resultPageRevision.")
                candidate = WikiCandidate.model_validate({**candidate.model_dump(), "state": request.state,
                    "page_id": page_id, "reason": request.reason,
                    "result_page": {"id": page_id, "revision": request.resultPageRevision} if request.state == "integrated" and page_id else None})
                current.put([Write(entity=candidate, expected_revision=request.expectedRevision)], author=author)
                result = {"candidate": current.get(candidate.id)}
            case SummaryRequest():
                found = current.search(request.query, request.kind, request.limit, request.offset)
                result = {"info": current.info(),
                          "records": [_summary(current, record) for record in found["items"]],
                          "hasMore": found["next_offset"] is not None,
                          "nextOffset": found["next_offset"]}
            case GetRequest():
                result = _get_record(current, request.id, request.revision)
            case SaveNoteRequest():
                record = current.get(request.id)
                if record["revision"] != request.expectedRevision:
                    raise ValueError(f"Revision conflict: {request.id}; expected "
                                     f"{request.expectedRevision}, current {record['revision']}")
                if record["entity"]["kind"] != "paper_note":
                    raise ValueError("Only paper_note body text can be edited in the desktop viewer.")
                note = PaperNote.model_validate({**record["entity"], "markdown": request.markdown})
                saved = current.put([Write(entity=note, expected_revision=request.expectedRevision)],
                                    author=author)[0]
                # Export before committing: bad images or edited target files must
                # not publish a DB revision which the app then reports as failed.
                result = _get_record(current, note.id, saved["revision"], strict_export=True)
    return {"version": 1, **result}


def _title(entity):
    text = next((entity[key] for key in ("title", "text", "excerpt", "locator", "note_id")
                 if entity.get(key)), entity["id"])
    return " ".join(text.split())[:180]


def _summary(store, record):
    entity = record["entity"]
    if entity["kind"] == "wiki_page":
        from .wiki import page_summary
        return page_summary(record)
    text = next((entity[key] for key in ("markdown", "scope", "rationale", "text", "excerpt",
                                        "problem", "authors", "uri") if entity.get(key)), "")
    result = {"id": entity["id"], "revision": record["revision"], "kind": entity["kind"],
              "title": _title(entity), "summary": " ".join(text.split())[:300],
              "updatedAt": record["created_at"]}
    if "state" in entity:
        result["state"] = entity["state"]
    if entity["kind"] == "paper_note":
        note = PaperNote.model_validate(entity)
        status = note.reading_status
        source = store.get(note.source.id, note.source.revision)["entity"]
        result["readingStatus"] = "partial" if (
            status == "full_text_reviewed" and source["source_type"] != "body") else status
    return result


def _get_record(store, record_id, revision=None, *, strict_export=False):
    record = store.get(record_id, revision)
    entity = ENTITY.validate_python(record["entity"])
    if isinstance(entity, WikiPage):
        from .wiki import wiki_detail
        return wiki_detail(store, record_id, revision, strict_export=strict_export)
    result = {"id": entity.id, "revision": record["revision"], "kind": entity.kind,
              "title": _title(record["entity"]), "editable": isinstance(entity, PaperNote),
              "references": [ref.model_dump() for ref, _ in references(entity)]}
    if isinstance(entity, PaperNote):
        # Keep authoring paths (../../../note-assets/...) in the editor. The
        # portable preview's assets/... paths are never written back to the DB.
        result["editText"] = entity.markdown
        result["editSemantics"] = "paper_note.markdown; source, coverage and basis are preserved"
        try:
            result["editPath"] = str(note_authoring_path(store, record_id, record["revision"]))
            preview = export_note(store, record_id, record["revision"])
            result["path"] = preview["path"]
        except (ValueError, OSError) as error:
            if strict_export:
                raise
            # A hand-edited export must not make its underlying note inaccessible.
            # The client displays this warning and does not resolve local images
            # without a validated preview path.
            preview = get_note(store, record_id, record["revision"])
            result["previewWarning"] = str(error)
        result["markdown"] = preview["markdown"]
        result["readingStatus"] = preview["reading_status"]
    else:
        result["markdown"] = _record_markdown(record)
    return result


def _literal(text):
    """Display stored prose as text, without creating links or embedded HTML."""
    return re.sub(r"([\\`*_{}\[\]()#+.!|>~-])", r"\\\1", html.escape(str(text), quote=False))


def _record_markdown(record):
    entity = record["entity"]
    lines = [f"# {_literal(_title(entity))}", "",
             f"- Record: `{entity['id']}@{record['revision']}`",
             f"- Kind: `{entity['kind']}`",
             f"- Updated: {_literal(record['created_at'])}",
             f"- Author: {_literal(record['author'])}",
             "- Scientific interpretation: not independently verified by the store", ""]
    for key, value in entity.items():
        if key in {"id", "kind"} or value is None or value == "" or value == []:
            continue
        lines += [f"## {_literal(key.replace('_', ' ').capitalize())}", ""]
        if isinstance(value, (dict, list)) or key in {"text", "excerpt"} and entity["kind"] in {"source", "evidence"}:
            text = json.dumps(value, ensure_ascii=False, indent=2) if isinstance(value, (dict, list)) else value
            # Longer fences preserve even captured source containing Markdown.
            fence = "`" * max(3, max((len(run) + 1 for run in re.findall(r"`+", text)), default=0))
            lines += [fence, text, fence, ""]
        else:
            lines += [_literal(value), ""]
    return "\n".join(lines).rstrip() + "\n"
