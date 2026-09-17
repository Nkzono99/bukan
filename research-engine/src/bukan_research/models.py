"""Small, versioned research vocabulary; no transport or storage dependencies."""

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, model_validator

Identifier = Annotated[str, Field(pattern=r"^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,159}$")]
Text = Annotated[str, Field(min_length=1, max_length=20000)]


class Model(BaseModel):
    model_config = ConfigDict(extra="forbid")


class Ref(Model):
    id: Identifier
    revision: int = Field(ge=1)


class ExternalRef(Model):
    provider: Text
    external_id: Text
    uri: str = ""


class Paper(Model):
    kind: Literal["paper"] = "paper"
    id: Identifier
    title: Text
    authors: str = ""
    year: int | None = None
    doi: str = ""
    external_refs: list[ExternalRef] = Field(default_factory=list)


class Source(Model):
    kind: Literal["source"] = "source"
    id: Identifier
    paper: Ref
    uri: Text
    version: Text
    locator: Text
    source_type: Literal["abstract", "body", "metadata"]
    asset_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    text: str = Field(min_length=1, max_length=100000)


class Evidence(Model):
    kind: Literal["evidence"] = "evidence"
    id: Identifier
    source: Ref
    start: int = Field(ge=0)
    end: int = Field(gt=0)
    excerpt: Text


class Claim(Model):
    kind: Literal["claim"] = "claim"
    id: Identifier
    text: Text
    claim_type: Literal["finding", "method", "assumption", "limitation"]
    conditions: dict[str, str] = Field(min_length=1)
    evidence: list[Ref] = Field(min_length=1, max_length=30)


class Relation(Model):
    kind: Literal["relation"] = "relation"
    id: Identifier
    relation_type: Literal["extends", "uses_method", "supports", "challenges", "differs_in_conditions"]
    source_claim: Ref
    target_claim: Ref
    text: Text
    retained: str = ""
    changed: str = ""
    comparison_basis: Text
    evidence: list[Ref] = Field(min_length=1, max_length=30)
    interpretation: Literal["analyst_inference", "author_explicit"] = "analyst_inference"


class Question(Model):
    kind: Literal["question"] = "question"
    id: Identifier
    text: Text
    rationale: Text
    verification_plan: Text
    alternatives: Text
    state: Literal["open", "revised", "deferred", "rejected"] = "open"
    basis: list[Ref] = Field(min_length=1, max_length=50)


class Topic(Model):
    kind: Literal["topic"] = "topic"
    id: Identifier
    title: Text
    scope: Text
    members: list[Ref] = Field(min_length=1, max_length=200)


class PageReview(Model):
    page: int = Field(ge=1)
    text: Literal["reviewed", "unread", "failed"] = "unread"
    visuals: Literal["reviewed", "not_present", "unread", "failed"] = "unread"
    note: str = ""


class PaperNote(Model):
    kind: Literal["paper_note"] = "paper_note"
    id: Identifier
    source: Ref
    title: Text
    markdown: str = Field(min_length=1, max_length=100000)
    page_count: int = Field(ge=1, le=10000)
    coverage: list[PageReview] = Field(default_factory=list, max_length=10000)
    basis: list[Ref] = Field(default_factory=list, max_length=200)

    @model_validator(mode="after")
    def check_pages(self):
        pages = [item.page for item in self.coverage]
        if len(set(pages)) != len(pages) or any(page > self.page_count for page in pages):
            raise ValueError("Coverage must use unique PDF file pages within page_count.")
        return self

    @property
    def reading_status(self) -> str:
        if len(self.coverage) == self.page_count and all(
            p.text == "reviewed" and p.visuals in {"reviewed", "not_present"}
            for p in self.coverage
        ):
            return "full_text_reviewed"
        return "partial" if any(p.text == "reviewed" or p.visuals == "reviewed" for p in self.coverage) else "screening"


class ReviewTask(Model):
    kind: Literal["review_task"] = "review_task"
    id: Identifier
    paper: Ref
    asset_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    page_count: int = Field(ge=1, le=10000)
    note_id: Identifier
    note_revision: int = Field(default=0, ge=0)
    state: Literal["pending", "running", "completed", "needs_followup"] = "pending"
    worker: str = ""
    problem: str = ""
    result_note: Ref | None = None

    @model_validator(mode="after")
    def check_state(self):
        if self.state == "running" and not self.worker.strip():
            raise ValueError("A running review task needs a worker.")
        if self.state == "completed" and self.result_note is None:
            raise ValueError("A completed review task needs a result_note.")
        if self.state == "needs_followup" and not self.problem.strip():
            raise ValueError("A task needing followup must explain the problem.")
        if self.state != "completed" and self.result_note is not None:
            raise ValueError("Only a completed review task may have a result_note.")
        if self.state == "pending" and (self.worker or self.problem):
            raise ValueError("A pending review task must clear its worker and problem.")
        return self


class WikiBasis(Ref):
    section_id: Identifier | None = None
    relation: Literal["supports", "challenges", "context"] = "supports"


class WikiAudit(Model):
    reviewer: Text
    reviewed_revision: int = Field(ge=1)
    reviewed_at: Text
    findings: str = ""


class WikiSection(Model):
    id: Identifier
    title: Text
    markdown: str = Field(default="", max_length=200000)
    basis: list[WikiBasis] = Field(default_factory=list, max_length=500)
    interpretation: Literal["analyst_inference", "author_explicit", "human_proposal"] = "analyst_inference"
    review_status: Literal["unverified", "migration_unverified", "independently_reviewed", "issues_open"] = "unverified"
    audit: list[WikiAudit] = Field(default_factory=list)
    source_files: list[str] = Field(default_factory=list)


class WikiPage(Model):
    kind: Literal["wiki_page"] = "wiki_page"
    id: Identifier
    title: Text
    summary: str = Field(default="", max_length=20000)
    page_type: Literal["portal", "theme", "concept", "model", "method", "material", "question", "comparison", "history", "glossary"] = "concept"
    aliases: list[str] = Field(default_factory=list)
    categories: list[str] = Field(default_factory=list)
    parent_ids: list[Identifier] = Field(default_factory=list)
    navigation: list[Identifier] = Field(default_factory=list)
    sections: list[WikiSection] = Field(default_factory=list, max_length=200)
    change_reason: str = ""
    search_through: str = ""
    verified_at: str = ""
    source_files: list[str] = Field(default_factory=list)

    @model_validator(mode="after")
    def unique_sections(self):
        if len({section.id for section in self.sections}) != len(self.sections):
            raise ValueError("Wiki section IDs must be unique within a page.")
        return self


class WikiTask(Model):
    kind: Literal["wiki_task"] = "wiki_task"
    id: Identifier
    page: Ref
    section_id: Identifier | None = None
    purpose: Text
    conditions: str = ""
    search_through: str = ""
    state: Literal["pending", "running", "completed", "needs_followup"] = "pending"
    worker: str = ""
    checkpoint: str = ""
    reason: str = ""
    outcome: Literal["revised", "unchanged", "incomplete"] | None = None
    result_page: Ref | None = None
    candidates: list[Ref] = Field(default_factory=list)

    @model_validator(mode="after")
    def completion(self):
        if self.state == "running" and not self.worker.strip():
            raise ValueError("A running wiki task needs a worker.")
        if self.state == "completed" and (self.outcome not in {"revised", "unchanged"} or not self.reason.strip()):
            raise ValueError("A completed wiki task needs a revised/unchanged outcome and a reason.")
        if self.outcome == "revised" and self.result_page is None:
            raise ValueError("A revised wiki task needs a pinned result_page.")
        if self.state == "needs_followup" and not self.reason.strip():
            raise ValueError("An unfinished wiki task needs a reason.")
        return self


class WikiCandidate(Model):
    kind: Literal["wiki_candidate"] = "wiki_candidate"
    id: Identifier
    record: Ref
    page_id: Identifier | None = None
    state: Literal["pending", "accepted", "rejected", "integrated"] = "pending"
    reason: str = ""
    suggested_page_ids: list[Identifier] = Field(default_factory=list)
    result_page: Ref | None = None

    @model_validator(mode="after")
    def decision(self):
        if self.state != "pending" and not self.reason.strip():
            raise ValueError("Candidate decisions need a reason.")
        if self.state in {"accepted", "integrated"} and not self.page_id:
            raise ValueError("Accepted candidates need a destination wiki page.")
        if self.state == "integrated" and self.result_page is None:
            raise ValueError("Integrated candidates need a pinned result_page.")
        return self


Entity = Annotated[Paper | Source | Evidence | Claim | Relation | Question | Topic | PaperNote | ReviewTask | WikiPage | WikiTask | WikiCandidate,
                   Field(discriminator="kind")]
ENTITY = TypeAdapter(Entity)


class Write(Model):
    entity: Entity
    expected_revision: int = Field(default=0, ge=0)


def references(entity: Entity) -> list[tuple[Ref, set[str]]]:
    """The same declared references drive integrity checks and graph traversal."""
    match entity:
        case Source():
            return [(entity.paper, {"paper"})]
        case Evidence():
            return [(entity.source, {"source"})]
        case Claim():
            return [(ref, {"evidence"}) for ref in entity.evidence]
        case Relation():
            return [(entity.source_claim, {"claim"}), (entity.target_claim, {"claim"})] + [
                (ref, {"evidence"}) for ref in entity.evidence]
        case Question():
            return [(ref, {"claim", "relation"}) for ref in entity.basis]
        case Topic():
            return [(ref, {"paper", "claim", "relation", "question", "paper_note"}) for ref in entity.members]
        case PaperNote():
            return [(entity.source, {"source"})] + [
                (ref, {"evidence", "claim", "relation", "question"}) for ref in entity.basis]
        case ReviewTask():
            return [(entity.paper, {"paper"})] + (
                [(entity.result_note, {"paper_note"})] if entity.result_note else [])
        case WikiPage():
            return [(ref, {"paper", "source", "evidence", "claim", "relation", "question", "topic", "paper_note", "wiki_page"})
                    for section in entity.sections for ref in section.basis]
        case WikiTask():
            return [(entity.page, {"wiki_page"})] + ([(entity.result_page, {"wiki_page"})] if entity.result_page else []) + [
                (ref, {"wiki_candidate"}) for ref in entity.candidates]
        case WikiCandidate():
            return [(entity.record, {"paper", "source", "evidence", "claim", "relation", "paper_note"})] + (
                [(entity.result_page, {"wiki_page"})] if entity.result_page else [])
        case _:
            return []
