import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import pytest

from bukan_research.desktop import handle_request
from bukan_research.models import (
    Claim, Evidence, PageReview, Paper, PaperNote, Question, Ref, Relation,
    ReviewTask, Source, Topic, Write,
)
from bukan_research.store import Store


def ref(record_id):
    return Ref(id=record_id, revision=1)


@pytest.fixture
def store(tmp_path):
    store = Store(tmp_path / "research.sqlite")
    store.initialize()
    text = "All source text.\n```\n![untrusted](file:///secret.png)\n```\n最後の頁。"
    source = Source(id="s", paper=ref("p"), uri="fixture://p.pdf", version="v1",
                    locator="PDF pp. 1–2", source_type="body", asset_sha256="a" * 64, text=text)
    entities = [
        Paper(id="p", title="ダスト輸送の文献", authors="Example et al.", year=2025),
        source,
        Evidence(id="e", source=ref("s"), start=0, end=len(text), excerpt=text),
        Claim(id="c", text="Grains move.", claim_type="finding",
              conditions={"illumination": "on"}, evidence=[ref("e")]),
        Claim(id="c2", text="Grains move under another condition.", claim_type="finding",
              conditions={"illumination": "off"}, evidence=[ref("e")]),
        Relation(id="r", source_claim=ref("c2"), target_claim=ref("c"), relation_type="challenges",
                 text="Conflicting observations", comparison_basis="Compare light conditions",
                 evidence=[ref("e")]),
        Question(id="q", text="Does illumination cause motion?", rationale="Two conditions",
                 alternatives="Mechanical disturbance", verification_plan="Controlled experiment",
                 basis=[ref("r")]),
        PaperNote(id="n", source=ref("s"), title="文献ノート", markdown="元の解析。",
                  page_count=2, coverage=[PageReview(page=p, text="reviewed", visuals="not_present")
                                          for p in (1, 2)], basis=[ref("c")]),
        Topic(id="t", title="ダスト輸送", scope="Research scope", members=[ref("q"), ref("n")]),
        ReviewTask(id="task", paper=ref("p"), asset_sha256="a" * 64, page_count=2,
                   note_id="n", note_revision=1),
    ]
    store.put([Write(entity=entity) for entity in entities])
    return store


def request(store, operation, **fields):
    return handle_request(store, {"version": 1, "operation": operation, **fields})


def test_summary_search_pagination_and_read_only_database(store):
    store.put([Write(entity=Paper(id=f"extra-{i:02}", title=f"Additional paper {i}"))
               for i in range(55)])
    before = store.path.read_bytes()
    first = request(store, "summary", kind="paper")
    assert first["version"] == 1
    assert first["info"]["counts"]["paper"] == 56
    assert len(first["records"]) == 50
    assert first["hasMore"] and first["nextOffset"] == 50
    second = request(store, "summary", kind="paper", offset=first["nextOffset"])
    assert len(second["records"]) == 6
    assert not second["hasMore"] and second["nextOffset"] is None
    assert not {r["id"] for r in first["records"]} & {r["id"] for r in second["records"]}
    # A term occurring only in structured conditions is also searchable.
    assert [r["id"] for r in request(store, "summary", query="illumination", kind="claim")["records"]] == ["c", "c2"]
    note = request(store, "summary", kind="paper_note")["records"][0]
    assert note["title"] == "文献ノート" and note["readingStatus"] == "full_text_reviewed"
    assert note["updatedAt"] == store.get("n")["created_at"]
    assert request(store, "summary", kind="review_task")["records"][0]["state"] == "pending"
    assert store.path.read_bytes() == before


def test_get_traverses_pinned_references_and_preserves_full_source(store):
    pending = [("t", 1)]
    seen = {}
    while pending:
        record_id, revision = pending.pop()
        if record_id in seen:
            continue
        result = request(store, "get", id=record_id, revision=revision)
        seen[record_id] = result
        pending.extend((ref["id"], ref["revision"]) for ref in result["references"])
    assert set(seen) == {"t", "q", "r", "c", "c2", "e", "s", "p", "n"}
    assert store.get("s")["entity"]["text"] in seen["s"]["markdown"]
    assert "````\nAll source text." in seen["s"]["markdown"]
    assert '"illumination": "on"' in seen["c"]["markdown"]
    assert "Controlled experiment" in seen["q"]["markdown"]
    assert "Compare light conditions" in seen["r"]["markdown"]
    assert seen["n"]["editable"] and not seen["c"]["editable"]
    paper = Paper.model_validate(store.get("p")["entity"])
    paper.title = "New title"
    store.put([Write(entity=paper, expected_revision=1)])
    assert request(store, "get", id="p", revision=1)["title"] == "ダスト輸送の文献"
    assert request(store, "get", id="p")["title"] == "New title"


def test_note_edit_uses_authoring_paths_preserves_provenance_and_updates_search(store):
    assets = store.path.parent / "note-assets" / "paper"
    assets.mkdir(parents=True)
    picture = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=")
    (assets / "figure.png").write_bytes(picture)
    before = store.get("n")["entity"]
    original_preview = request(store, "get", id="n")
    text = "更新した知見。\n\n$$\nF = ma\n$$\n\n![図1](../../../note-assets/paper/figure.png)\n"
    saved = request(store, "save-note", id="n", expectedRevision=1, markdown=text)
    assert saved["revision"] == 2 and saved["editText"] == text
    assert "![図1](assets/" in saved["markdown"]
    path = Path(saved["path"])
    edit_path = Path(saved["editPath"])
    assert edit_path == (store.path.parent / "paper-notes"
                         / hashlib.sha256(store.path.name.encode("utf-8")).hexdigest()
                         / hashlib.sha256(b"n").hexdigest() / "r2.md")
    assert not edit_path.exists()
    assert (edit_path.parent / "../../../note-assets/paper/figure.png").resolve() == assets / "figure.png"
    assert (edit_path.parent / "../../../note-assets/paper/figure.png").resolve().read_bytes() == picture
    assert Path(original_preview["editPath"]).name == "r1.md"
    assert request(store, "get", id="n", revision=1)["editPath"] == original_preview["editPath"]
    assert request(store, "get", id="n")["editPath"] == saved["editPath"]
    assert path.parent.name == "r2"
    assert path.read_text(encoding="utf-8") == saved["markdown"]
    assert next((path.parent / "assets").iterdir()).read_bytes() == picture
    assert Path(original_preview["path"]).exists()
    assert store.get("n", 1)["entity"] == before
    assert store.get("n")["entity"] == {**before, "markdown": text}
    assert request(store, "summary", query="更新した知見", kind="paper_note")["records"][0]["revision"] == 2
    assert request(store, "get", id="n")["editText"] == text
    assert request(store, "save-note", id="n", expectedRevision=2, markdown=text)["revision"] == 2


def test_note_conflict_also_rejects_identical_stale_body(store):
    text = "Another editor saved this."
    request(store, "save-note", id="n", expectedRevision=1, markdown=text)
    for stale_text in ("Stale correction", text):
        with pytest.raises(ValueError, match="Revision conflict"):
            request(store, "save-note", id="n", expectedRevision=1, markdown=stale_text)
    assert store.get("n")["entity"]["markdown"] == text
    assert len(store.history("n")) == 2
    with pytest.raises(ValueError, match="Only paper_note"):
        request(store, "save-note", id="c", expectedRevision=1, markdown="Changed claim")


def test_note_export_validation_failure_rolls_back_database(store):
    before = store.export()
    with pytest.raises(ValueError, match="missing"):
        request(store, "save-note", id="n", expectedRevision=1,
                markdown="![Missing](../../../note-assets/paper/missing.png)")
    assert store.export() == before
    existing = Path(request(store, "get", id="n")["path"])
    target = existing.parent.parent / "r2" / "index.md"
    target.parent.mkdir()
    target.write_text("Unpublished manual content", encoding="utf-8")
    with pytest.raises(ValueError, match="local edits"):
        request(store, "save-note", id="n", expectedRevision=1, markdown="New body")
    assert store.export() == before
    assert target.read_text(encoding="utf-8") == "Unpublished manual content"


def test_locally_edited_preview_remains_repairable_without_overwrite(store):
    original = request(store, "get", id="n")
    path = Path(original["path"])
    path.write_text("Manual export additions", encoding="utf-8")
    result = request(store, "get", id="n")
    assert result["editable"] and result["editText"] == "元の解析。"
    assert result["editPath"] == original["editPath"]
    assert "local edits" in result["previewWarning"] and "path" not in result
    saved = request(store, "save-note", id="n", expectedRevision=1,
                    markdown=result["editText"] + "\nManual export additions")
    assert saved["revision"] == 2
    assert path.read_text(encoding="utf-8") == "Manual export additions"


def test_non_body_note_cannot_report_full_reading_in_summary(store):
    source = Source.model_validate(store.get("s")["entity"])
    source.id, source.source_type = "abstract", "abstract"
    note = PaperNote.model_validate(store.get("n")["entity"])
    note.source = ref("abstract")
    store.put([Write(entity=source), Write(entity=note, expected_revision=1)])
    assert request(store, "summary", kind="paper_note")["records"][0]["readingStatus"] == "partial"


@pytest.mark.parametrize("payload", [[], {"operation": "unknown"}, {"operation": "get"},
                                   {"operation": "summary", "version": 2},
                                   {"operation": "summary", "offset": -1},
                                   {"operation": "summary", "offset": "1"},
                                   {"operation": "save-note", "id": "n", "expectedRevision": 1, "markdown": ""}])
def test_malformed_requests_are_validation_errors(store, payload):
    with pytest.raises(ValueError):
        handle_request(store, payload)


@pytest.mark.parametrize("payload, success", [('invalid JSON', False),
                                              ('{"operation":"get"}', False),
                                              pytest.param('[' * 100000 + '0' + ']' * 100000, False, id="nested-json"),
                                              (json.dumps({"operation": "summary", "offset": 2**80}), False),
                                              (json.dumps({"operation": "get", "id": "n", "revision": 2**80}), False),
                                              ('{"operation":"summary","query":"元の解析"}', True)])
def test_cli_json_stdio_contract_and_errors_without_traceback(store, payload, success):
    completed = subprocess.run([sys.executable, "-m", "bukan_research.cli", "--store", str(store.path), "desktop"],
                               input=payload, encoding="utf-8", capture_output=True,
                               env={**os.environ, "PYTHONUTF8": "1"}, check=False)
    assert (completed.returncode == 0) == success
    if success:
        assert json.loads(completed.stdout)["records"][0]["id"] == "n"
        assert not completed.stderr
    else:
        assert completed.stderr and "Traceback" not in completed.stderr and not completed.stdout
