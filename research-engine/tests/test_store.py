import hashlib
from pathlib import Path
import sqlite3

import pytest

from bukan_research.adapters import paper_from_bukan
from bukan_research.models import Claim, Evidence, Paper, Question, Ref, Relation, Source, Topic, Write
from bukan_research.store import Store


def ref(record_id, revision=1):
    return Ref(id=record_id, revision=revision)


def sample():
    text = "A synthetic experiment shows motion under illumination."
    excerpt = "shows motion under illumination"
    start = text.index(excerpt)
    return [
        Paper(id="p", title="Synthetic fixture"),
        Source(id="s", paper=ref("p"), uri="fixture://paper", version="v1", locator="p. 1",
               source_type="body", asset_sha256=hashlib.sha256(b"fixture").hexdigest(), text=text),
        Evidence(id="e", source=ref("s"), start=start, end=start+len(excerpt), excerpt=excerpt),
        Claim(id="c", text="Illuminated grains move.", claim_type="finding",
              conditions={"illumination": "on"}, evidence=[ref("e")]),
        Question(id="q", text="Does motion depend on light?", rationale="The tested condition is limited.",
                 verification_plan="Compare light on/off.", alternatives="A mechanical disturbance.", basis=[ref("c")]),
        Topic(id="t", title="Synthetic topic", scope="Test only", members=[ref("q")]),
    ]


@pytest.fixture
def store(tmp_path):
    store = Store(tmp_path / "store.sqlite")
    store.initialize()
    return store


def test_atomic_forward_references_idempotency_and_reopen(store):
    writes = [Write(entity=entity) for entity in reversed(sample())]
    assert all(r["status"] == "created" for r in store.put(writes))
    assert all(r["status"] == "unchanged" for r in store.put(writes))
    assert store.info()["revisions"] == 6
    reopened = Store(store.path)
    assert len(reopened.trace("t")["nodes"]) == 6
    assert reopened.get("e")["entity"]["excerpt"] == "shows motion under illumination"


@pytest.mark.parametrize("change", ["excerpt", "end", "reference", "kind"])
def test_bad_evidence_or_reference_rolls_back_entire_batch(store, change):
    entities = sample()
    if change == "excerpt":
        entities[2].excerpt = "Invented quotation"
    elif change == "end":
        entities[2].end = 1000000
    elif change == "reference":
        entities[2].source = ref("missing")
    else:
        entities[3].evidence = [ref("p")]
    with pytest.raises(ValueError):
        store.put([Write(entity=entity) for entity in entities])
    assert store.info()["revisions"] == 0


def test_revision_conflict_preserves_manual_edits_and_pinned_history(store):
    entities = sample()
    store.put([Write(entity=e) for e in entities])
    question = entities[4].model_copy(update={"text": "Human corrected question"})
    store.put([Write(entity=question, expected_revision=1)], author="local-user")
    with pytest.raises(ValueError, match="Revision conflict"):
        store.put([Write(entity=entities[4].model_copy(update={"text": "Stale AI answer"}), expected_revision=1)])
    assert store.get("q")["entity"]["text"] == "Human corrected question"
    assert len(store.history("q")) == 2
    exported = store.export()
    assert exported["heads"]["q"] == 2
    assert len(exported["records"]) == 7
    trace = store.trace("t")
    old_question = next(n for n in trace["nodes"] if n["entity"]["id"] == "q")
    assert old_question["revision"] == 1
    assert old_question["latest_revision"] == 2
    assert old_question["entity"]["text"] == entities[4].text


def test_source_and_evidence_are_immutable(store):
    entities = sample()
    store.put([Write(entity=e) for e in entities])
    entities[1].text = "Rewritten source"
    with pytest.raises(ValueError, match="immutable"):
        store.put([Write(entity=entities[1], expected_revision=1)])


def test_extends_needs_precise_change_and_distinct_claims(store):
    entities = sample()
    second = entities[3].model_copy(update={"id": "c2", "text": "Second condition"})
    relation = Relation(id="r", relation_type="extends", source_claim=ref("c2"), target_claim=ref("c"),
                        text="An extension", comparison_basis="Same observable", evidence=[ref("e")])
    with pytest.raises(ValueError, match="retained and changed"):
        store.put([Write(entity=e) for e in [*entities, second, relation]])
    relation.retained, relation.changed = "Same force balance", "Different illumination"
    store.put([Write(entity=e) for e in [*entities, second, relation]])
    assert store.get("r")["entity"]["interpretation"] == "analyst_inference"


def test_pagination_literal_unicode_search_and_bounded_trace(store):
    entities = sample()
    entities[4].text = "ダスト輸送の問い"
    store.put([Write(entity=e) for e in entities])
    first = store.search(limit=2)
    second = store.search(limit=2, offset=first["next_offset"])
    assert {r["entity"]["id"] for r in first["items"]}.isdisjoint(r["entity"]["id"] for r in second["items"])
    assert len(store.search("輸送", "question")["items"]) == 1
    assert store.search("%'")["items"] == []
    assert store.trace("t", limit=2)["truncated"]


def test_store_location_and_format_are_validated(tmp_path):
    with pytest.raises(ValueError, match="Paperpile"):
        Store(tmp_path / "Paperpile" / "store.sqlite")
    custom_library = tmp_path / "custom-library"
    (custom_library / "All Papers").mkdir(parents=True)
    with pytest.raises(ValueError, match="Paperpile"):
        Store(custom_library / "reports" / "store.sqlite")
    repository = tmp_path / "repository"
    (repository / "src-tauri").mkdir(parents=True)
    (repository / "src-tauri/Cargo.toml").write_text("[package]")
    with pytest.raises(ValueError, match="repository"):
        Store(repository / "data/store.sqlite")
    path = tmp_path / "unknown.sqlite"
    with sqlite3.connect(path) as db:
        db.execute("PRAGMA user_version=999")
    with pytest.raises(ValueError, match="Unsupported"):
        Store(path).initialize()
    assert sqlite3.connect(path).execute("PRAGMA user_version").fetchone()[0] == 999


def test_bukan_adapter_keeps_external_id_without_importing_bukan():
    raw = {"id": "p2-test", "title": "Example", "path": "G:/Paperpile/p.pdf", "year": 2000}
    a, b = paper_from_bukan(raw), paper_from_bukan(raw | {"path": "H:/Paperpile/p.pdf"})
    assert a.id == b.id and a.id != raw["id"]
    assert a.external_refs[0].external_id == raw["id"]


def test_export_is_one_snapshot_during_a_concurrent_commit(store, monkeypatch):
    paper = Paper(id="p", title="Original")
    store.put([Write(entity=paper)])
    with sqlite3.connect(store.path) as db:
        db.execute("PRAGMA journal_mode=WAL")
    decode = store._decode
    updated = False

    def update_between_export_queries(row):
        nonlocal updated
        if not updated:
            updated = True
            Store(store.path).put([Write(entity=paper.model_copy(update={"title": "Updated"}), expected_revision=1)])
        return decode(row)

    monkeypatch.setattr(store, "_decode", update_between_export_queries)
    exported = store.export()
    assert exported["heads"] == {"p": 1}
    assert len(exported["records"]) == 1
    assert store.get("p")["revision"] == 2


def test_relation_cannot_use_two_revisions_as_distinct_claims(store):
    entities = sample()
    store.put([Write(entity=e) for e in entities])
    updated = entities[3].model_copy(update={"text": "Corrected claim"})
    store.put([Write(entity=updated, expected_revision=1)])
    relation = Relation(id="self-support", relation_type="supports",
                        source_claim=ref("c",2), target_claim=ref("c",1),
                        text="Self support", comparison_basis="Same claim", evidence=[ref("e")])
    with pytest.raises(ValueError, match="distinct claims"):
        store.put([Write(entity=relation)])
