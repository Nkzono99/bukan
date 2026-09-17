from concurrent.futures import ThreadPoolExecutor
import pytest

from bukan_research.models import Paper, PaperNote, PageReview, Ref, ReviewTask, Source, Write
from bukan_research.review_workflow import ReviewTarget, plan_reviews
from bukan_research.store import Store


@pytest.fixture
def store(tmp_path):
    store = Store(tmp_path / "research.sqlite")
    store.initialize()
    store.put([Write(entity=Paper(id=paper, title=paper)) for paper in ["p", "other"]])
    return store


def target(paper="p", asset="a", pages=2, note_id=None):
    return ReviewTarget(paper_id=paper, asset_sha256=asset * 64, page_count=pages, note_id=note_id)


def plan(store, targets=None, requested=30, available=30):
    return plan_reviews(store, targets or [target()], requested, available)


def claim(store, row, worker="worker-one"):
    task = ReviewTask.model_validate(row["entity"]).model_copy(update={"state": "running", "worker": worker})
    store.put([Write(entity=task, expected_revision=row["revision"])])
    return store.get(task.id)


def completion(row, *, changes=None, source_changes=None):
    task = ReviewTask.model_validate(row["entity"])
    source = Source(id="source-" + task.id, paper=task.paper, uri="fixture://paper.pdf", version="fixture",
                    locator="PDF pages 1-2", source_type="body", asset_sha256=task.asset_sha256, text="Source")
    if source_changes:
        source = source.model_copy(update=source_changes)
    note = PaperNote(id=task.note_id, source=Ref(id=source.id, revision=1), title="Review", markdown="Durable note",
                     page_count=task.page_count, coverage=[PageReview(page=page, text="reviewed", visuals="reviewed")
                                                          for page in range(1, task.page_count + 1)])
    if changes:
        note = note.model_copy(update=changes)
    done = task.model_copy(update={"state": "completed", "result_note": Ref(id=note.id, revision=task.note_revision + 1)})
    return [Write(entity=done, expected_revision=row["revision"]), Write(entity=source),
            Write(entity=note, expected_revision=task.note_revision)]


def test_durable_idempotent_planning_and_actual_free_capacity(store):
    targets = [target(), target(), target("other")]
    first = plan(store, targets, requested=30, available=1)
    assert first["dispatch_count"] == 1 and len(first["ready"]) == 2
    reopened = plan(Store(store.path), targets, requested=30, available=0)
    assert reopened["dispatch_count"] == 0
    assert reopened["ready"] == first["ready"]
    assert store.info()["counts"]["review_task"] == 2
    assert store.info()["schema_version"] == 2
    assert plan(store, targets, requested=100, available=1)["dispatch_count"] == 1


@pytest.mark.parametrize("targets", [
    [target(), target(asset="b")], [target(), target(pages=3)],
    [target(note_id="shared"), target("other", note_id="shared")],
])
def test_conflicting_targets_do_not_create_tasks(store, targets):
    with pytest.raises(ValueError, match="Conflicting review targets"):
        plan(store, targets)
    assert "review_task" not in store.info()["counts"]


def test_full_note_is_reused_only_for_exact_pdf_and_page_count(store):
    row = claim(store, plan(store)["ready"][0])
    store.put(completion(row))
    reused = plan(store)
    assert reused["ready"] == [] and reused["reused"] == [{"paper_id": "p", "note": {"id": "note-p", "revision": 1}}]
    assert len(plan(store, [target(asset="b")])["ready"]) == 1
    assert len(plan(store, [target(pages=3)])["ready"]) == 1


def test_partial_new_note_reopens_completed_task_and_preserves_history(store):
    row = claim(store, plan(store)["ready"][0])
    store.put(completion(row))
    note = PaperNote.model_validate(store.get("note-p")["entity"])
    note = note.model_copy(update={"markdown": "Human additions; page 2 needs another look", "coverage": note.coverage[:1]})
    store.put([Write(entity=note, expected_revision=1)])
    ready = plan(store)["ready"][0]
    assert ready["entity"]["id"] == row["entity"]["id"]
    assert ready["entity"]["note_revision"] == 2
    assert ready["entity"]["worker"] == "" and ready["entity"]["result_note"] is None
    assert len(store.history(ready["entity"]["id"])) == 4
    assert store.get("note-p")["entity"]["markdown"].startswith("Human additions")


def test_compare_and_swap_allows_only_one_concurrent_worker(store):
    row = plan(store)["ready"][0]

    def try_claim(worker):
        try:
            claim(Store(store.path), row, worker)
            return True
        except ValueError as error:
            assert "Revision conflict" in str(error)
            return False

    with ThreadPoolExecutor(max_workers=2) as pool:
        assert sorted(pool.map(try_claim, ["worker-one", "worker-two"])) == [False, True]
    assert len(plan(store)["running"]) == 1


def test_separate_plans_cannot_assign_same_note_to_two_running_workers(store):
    first = plan(store)["ready"][0]
    second = plan(store, [target(asset="b")])["ready"][0]
    claim(store, first)
    with pytest.raises(ValueError, match="already owns"):
        claim(store, second, "worker-two")
    assert store.get(second["entity"]["id"])["entity"]["state"] == "pending"


@pytest.mark.parametrize("changes,source_changes", [
    ({"coverage": [PageReview(page=1, text="reviewed", visuals="reviewed")]}, None),
    ({"page_count": 3}, None), (None, {"asset_sha256": "b" * 64}),
    (None, {"paper": Ref(id="other", revision=1)}), (None, {"source_type": "abstract"}),
])
def test_invalid_completion_rolls_back_note_sources_and_task(store, changes, source_changes):
    row = claim(store, plan(store)["ready"][0])
    before = store.info()
    with pytest.raises(ValueError, match="assigned paper"):
        store.put(completion(row, changes=changes, source_changes=source_changes))
    assert store.info() == before
    assert store.get(row["entity"]["id"])["entity"]["state"] == "running"


def test_stale_worker_cannot_overwrite_new_note_or_complete_from_old_revision(store):
    row = claim(store, plan(store)["ready"][0])
    result = completion(row)
    store.put(result)
    original_note = result[-1].entity
    store.put([Write(entity=original_note.model_copy(update={"markdown": "Human update", "coverage": []}), expected_revision=1)])
    row = claim(store, plan(store)["ready"][0], "new-worker")
    task = ReviewTask.model_validate(row["entity"])
    old_result = task.model_copy(update={"state": "completed", "result_note": Ref(id=task.note_id, revision=1)})
    with pytest.raises(ValueError, match="current fully read"):
        store.put([Write(entity=old_result, expected_revision=row["revision"])])
    note = PaperNote.model_validate(store.get(task.note_id)["entity"])
    store.put([Write(entity=note.model_copy(update={"markdown": "Newer human update"}), expected_revision=2)])
    result = completion(row, changes={"markdown": "Stale worker result"})
    result[-1].expected_revision = 3  # Even a refreshed write CAS cannot conceal an outdated task baseline.
    with pytest.raises(ValueError, match="Review note changed"):
        store.put(result)
    assert store.get(task.note_id)["entity"]["markdown"] == "Newer human update"


def test_identical_completion_replay_cannot_bypass_current_note_validation(store):
    row = claim(store, plan(store)["ready"][0])
    result = completion(row)
    store.put(result)
    assert all(item["status"] == "unchanged" for item in store.put(result))
    note = result[-1].entity
    store.put([Write(entity=note.model_copy(update={"markdown": "Human update"}), expected_revision=1)])
    stale_note = note.model_copy(update={"markdown": "Stale result using refreshed CAS"})
    with pytest.raises(ValueError, match="current fully read"):
        store.put([result[0], Write(entity=stale_note, expected_revision=2)])
    assert store.get(note.id)["entity"]["markdown"] == "Human update"
    assert store.get(note.id)["revision"] == 2


def test_claim_cannot_include_a_change_to_its_starting_note(store):
    row = plan(store)["ready"][0]
    result = completion(row)
    task = ReviewTask.model_validate(row["entity"]).model_copy(update={"state": "running", "worker": "worker"})
    with pytest.raises(ValueError, match="Review note changed"):
        store.put([Write(entity=task, expected_revision=1), *result[1:]])
    assert "paper_note" not in store.info()["counts"]


def test_failure_does_not_block_other_papers_and_requires_explicit_retry(store):
    rows = plan(store, [target(), target("other")])["ready"]
    running = claim(store, rows[0])
    failed = ReviewTask.model_validate(running["entity"]).model_copy(update={"state": "needs_followup", "problem": "Page 2 unreadable"})
    store.put([Write(entity=failed, expected_revision=running["revision"])])
    replanned = plan(store, [target(), target("other")])
    assert replanned["dispatch_count"] == 1 and len(replanned["needs_followup"]) == 1
    retry = failed.model_copy(update={"state": "pending", "worker": "", "problem": ""})
    store.put([Write(entity=retry, expected_revision=3)])
    assert plan(store, [target(), target("other")])["dispatch_count"] == 2


def test_task_assignment_and_claim_baseline_cannot_be_rewritten(store):
    row = plan(store)["ready"][0]
    task = ReviewTask.model_validate(row["entity"])
    with pytest.raises(ValueError, match="immutable"):
        store.put([Write(entity=task.model_copy(update={"asset_sha256": "b" * 64}), expected_revision=1)])
    row = claim(store, row)
    task = ReviewTask.model_validate(row["entity"])
    with pytest.raises(ValueError, match="original note revision"):
        store.put([Write(entity=task.model_copy(update={"note_revision": 1}), expected_revision=2)])


def test_replanning_refreshes_pending_task_after_a_note_edit(store):
    row = plan(store)["ready"][0]
    store.put(completion(row, changes={"coverage": [], "markdown": "Human draft"})[1:])
    with pytest.raises(ValueError, match="Review note changed"):
        claim(store, row)
    refreshed = plan(store)["ready"][0]
    assert refreshed["entity"]["note_revision"] == 1
    assert refreshed["revision"] == 2
    assert claim(store, refreshed)["entity"]["state"] == "running"


def test_planning_snapshot_excludes_captured_text_and_unrequested_history(store, monkeypatch):
    row = claim(store, plan(store)["ready"][0])
    store.put(completion(row, changes={"markdown": "Private long note"}))
    snapshot = store.review_snapshot({"p", "note-p", row["entity"]["id"]})
    assert set(snapshot) == {"p", "note-p", row["entity"]["id"]}
    assert "markdown" not in snapshot["note-p"]["entity"]
    assert snapshot["note-p"]["source_hash"] == "a" * 64

    def no_full_export():
        pytest.fail("Planning must not export all historical captured text")

    monkeypatch.setattr(store, "export", no_full_export)
    assert len(plan(store)["reused"]) == 1


def test_long_paper_id_has_valid_stable_note_identifier(store):
    paper_id = "p" * 160
    store.put([Write(entity=Paper(id=paper_id, title="Long identifier"))])
    first = plan(store, [target(paper=paper_id)])["ready"][0]
    assert len(first["entity"]["note_id"]) <= 160
    assert plan(store, [target(paper=paper_id)])["ready"] == [first]
