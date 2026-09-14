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


Entity = Annotated[Paper | Source | Evidence | Claim | Relation | Question | Topic | PaperNote | ReviewTask,
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
        case _:
            return []
