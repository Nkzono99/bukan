from contextlib import closing
from pathlib import Path
import sqlite3

import pytest

from bukan_research.desktop import handle_request
from bukan_research.models import Claim, PaperNote, Ref, WikiAudit, WikiBasis, WikiCandidate, WikiPage, WikiSection, WikiTask, Write
from bukan_research.store import Store
from bukan_research.wiki import export_wiki, wiki_detail, wiki_home
from test_store import sample


@pytest.fixture
def store(tmp_path):
    store = Store(tmp_path / "data" / "research.sqlite")
    store.initialize()
    store.put([Write(entity=entity) for entity in sample()])
    return store


def page(identifier="wiki-test", basis="c", **fields):
    return WikiPage(id=identifier, title="Dust detachment", aliases=["離脱"], categories=["Dust"],
                    sections=[WikiSection(id="s1", title="Conditions", markdown="Pinned explanation.",
                                          basis=[WikiBasis(id=basis, revision=1)] if basis else [])], **fields)


def put(store, entity, revision=0):
    return store.put([Write(entity=entity, expected_revision=revision)])[0]


def test_migration_backups_history_and_blocks_old_writes(store):
    original = store.export()
    with sqlite3.connect(store.path) as db:
        for (name,) in db.execute("SELECT name FROM sqlite_master WHERE type='trigger'").fetchall():
            db.execute(f'DROP TRIGGER "{name}"')
        db.execute("DROP TABLE wiki_assets")
        db.execute("DROP TABLE wiki_snapshots")
        db.execute("DROP TABLE wiki_documents")
        db.execute("PRAGMA user_version=1")
    assert store.status()["needsMigration"]
    assert store.get("c")["revision"] == 1
    with pytest.raises(ValueError, match="read-only"):
        put(store, page())
    migrated = store.migrate()
    backup = Store(Path(migrated["backupPath"]))
    assert backup.export()["records"] == original["records"]
    assert backup.status()["formatVersion"] == 1
    assert store.export()["records"] == original["records"]
    assert store.status()["formatVersion"] == 2
    assert "backupPath" not in store.migrate()
    # Format-1 clients check user_version before writing; the new marker rejects them.
    assert sqlite3.connect(store.path).execute("PRAGMA user_version").fetchone()[0] != 1


def test_pinned_images_and_history_survive_asset_changes(store):
    assets = store.path.parent / "wiki-assets"
    assets.mkdir()
    (assets / "figure.png").write_bytes(b"original-image")
    entity = page()
    entity.sections[0].markdown = "$$\nF=ma\n$$\n\n![Figure](../wiki-assets/figure.png)"
    put(store, entity)
    (assets / "figure.png").write_bytes(b"new-image")
    original = export_wiki(store, entity.id, 1)
    assert next((Path(original["path"]).parent / "assets").iterdir()).read_bytes() == b"original-image"
    entity.sections[0].markdown += "\n\nChanged explanation."
    put(store, entity, 1)
    revised = export_wiki(store, entity.id, 2)
    assert next((Path(revised["path"]).parent / "assets").iterdir()).read_bytes() == b"new-image"
    assert export_wiki(store, entity.id, 1) == original
    Path(original["path"]).write_text("Human export correction", encoding="utf-8")
    detail = wiki_detail(store, entity.id, 1)
    assert "local edits" in detail["previewWarning"]
    assert "path" not in detail


def test_save_preserves_basis_resets_audit_and_conflicts(store):
    entity = page()
    entity.sections[0].audit = [WikiAudit(reviewer="auditor", reviewed_revision=1, reviewed_at="2026-09-15")]
    entity.sections[0].review_status = "independently_reviewed"
    put(store, entity)
    request = {"operation": "save-wiki", "id": entity.id, "expectedRevision": 1,
               "title": entity.title, "summary": "Updated summary", "sections": [
                   {"id": "s1", "title": "Conditions", "markdown": "Human correction."}]}
    saved = handle_request(store, request)
    assert saved["revision"] == 2
    updated = WikiPage.model_validate(store.get(entity.id)["entity"])
    assert updated.sections[0].basis == entity.sections[0].basis
    assert not updated.sections[0].audit
    assert updated.sections[0].review_status == "unverified"
    with pytest.raises(ValueError, match="Revision conflict"):
        handle_request(store, request)
    assert store.get(entity.id, 1)["entity"]["sections"][0]["audit"]
    assert wiki_home(store, "離脱")["pages"][0]["id"] == entity.id


def test_transitive_staleness_excludes_navigation_and_persists_tasks(store):
    first = page("wiki-first")
    second = page("wiki-second", "wiki-first")
    nav = page("wiki-nav", None, navigation=["wiki-first"])
    store.put([Write(entity=entity) for entity in (first, second, nav)])
    claim = Claim.model_validate(store.get("c")["entity"])
    claim.text = "Revised condition."
    put(store, claim, 1)
    first_detail = wiki_detail(store, first.id)
    second_detail = wiki_detail(store, second.id)
    assert first_detail["freshness"]["state"] == "needs_review"
    assert len(second_detail["freshness"]["sections"][0]["reasons"][0]["path"]) == 2
    assert wiki_detail(store, nav.id)["freshness"]["state"] == "current"
    tasks = wiki_home(Store(store.path))["tasks"]
    assert {task["pageId"] for task in tasks} == {first.id, second.id}
    assert all(task["sectionId"] == "s1" for task in tasks)
    assert store.trace(second.id)["nodes"][2]["revision"] == 1


@pytest.mark.parametrize("basis,field", [("q", "text"), ("t", "scope")])
def test_question_and_topic_changes_immediately_persist_review_tasks(store, basis, field):
    entity = page(basis=basis)
    if basis == "t":
        entity.sections[0].basis[0].relation = "context"
    put(store, entity)
    from bukan_research.models import ENTITY
    changed = ENTITY.validate_python(store.get(basis)["entity"])
    setattr(changed, field, "Updated scope or question")
    put(store, changed, 1)
    assert any(task["pageId"] == entity.id for task in wiki_home(store)["tasks"])


def test_cycles_and_wiki_only_evidence_are_rejected(store):
    empty = page("wiki-empty", None)
    put(store, empty)
    with pytest.raises(ValueError, match="underlying original"):
        put(store, page("wiki-derived", "wiki-empty"))
    first, second = page("wiki-a", "wiki-b"), page("wiki-b", "wiki-a")
    with pytest.raises(ValueError, match="Circular"):
        store.put([Write(entity=entity) for entity in (first, second)])


def test_historical_interpretation_chain_is_not_a_circular_reference(store):
    first = page("wiki-first")
    put(store, first)
    put(store, page("wiki-second", "wiki-first"))
    first.sections[0].basis = [WikiBasis(id="wiki-second", revision=1)]
    put(store, first, 1)
    assert store.get(first.id)["revision"] == 2
    assert any(item["entity"]["id"] == "c" for item in store.trace(first.id)["nodes"])


def test_interpretation_can_reach_original_evidence_through_question(store):
    put(store, page("wiki-first", "q"))
    put(store, page("wiki-second", "wiki-first"))
    assert any(item["entity"]["kind"] == "source" for item in store.trace("wiki-second")["nodes"])


def test_new_unlinked_source_surfaces_and_candidate_needs_actual_integration(store):
    put(store, page())
    source = sample()[1].model_copy(update={"id": "corrected-source", "version": "correction"})
    put(store, source)
    candidates = wiki_home(Store(store.path))["candidates"]
    candidate_summary = next(candidate for candidate in candidates if candidate["recordId"] == source.id)
    candidate = WikiCandidate.model_validate(store.get(candidate_summary["id"])["entity"])
    candidate.state, candidate.page_id, candidate.reason = "integrated", "wiki-test", "Compared new source."
    candidate.result_page = Ref(id="wiki-test", revision=1)
    with pytest.raises(ValueError, match="evidence chain"):
        put(store, candidate, 1)
    candidate.state, candidate.result_page = "accepted", None
    put(store, candidate, 1)
    assert Store(store.path).get(candidate.id)["entity"]["state"] == "accepted"


def test_section_pins_do_not_integrate_unrelated_section_evidence(store):
    other = sample()[3].model_copy(update={"id": "other-claim"})
    put(store, other)
    first = page("wiki-first")
    first.sections.append(WikiSection(id="other-section", title="Other condition", basis=[WikiBasis(id="other-claim", revision=1)]))
    put(store, first)
    second = page("wiki-second", None)
    second.sections[0].basis = [WikiBasis(id="wiki-first", revision=1, section_id="s1")]
    put(store, second)
    candidate = WikiCandidate(id="manual-candidate", record=Ref(id="other-claim", revision=1), page_id=second.id,
                              state="integrated", reason="Unrelated section should not count", result_page=Ref(id=second.id, revision=1))
    with pytest.raises(ValueError, match="evidence chain"):
        put(store, candidate)


def test_wiki_task_resumes_and_cannot_finish_with_unrecorded_outcome(store):
    put(store, page())
    created = handle_request(store, {"operation": "create-wiki-task", "pageId": "wiki-test", "sectionId": "s1", "purpose": "Check counterevidence"})
    task = WikiTask.model_validate(Store(store.path).get(created["id"])["entity"])
    task.state, task.worker, task.checkpoint = "running", "reviewer", "Retrieved PDF; comparison remains."
    put(store, task, 1)
    task.state, task.outcome, task.reason = "completed", "unchanged", "Compared conditions and retained the existing scope."
    put(store, task, 2)
    assert Store(store.path).get(task.id)["entity"]["checkpoint"] == "Retrieved PDF; comparison remains."
    assert len(store.history(task.id)) == 3


def test_invalid_image_save_rolls_back_page_and_work(store):
    before = store.export()
    entity = page()
    entity.sections[0].markdown = "![Missing](../wiki-assets/missing.png)"
    with pytest.raises(ValueError, match="missing"):
        put(store, entity)
    assert store.export() == before


def test_already_open_legacy_writer_is_blocked_after_migration(store):
    with closing(sqlite3.connect(store.path)) as db, db:
        for (name,) in db.execute("SELECT name FROM sqlite_master WHERE type='trigger'").fetchall():
            db.execute(f'DROP TRIGGER "{name}"')
        for name in ("wiki_assets", "wiki_snapshots", "wiki_documents"):
            db.execute(f"DROP TABLE {name}")
        db.execute("PRAGMA user_version=1")
    with closing(sqlite3.connect(store.path)) as legacy:
        assert legacy.execute("PRAGMA user_version").fetchone()[0] == 1
        store.migrate()
        with pytest.raises(sqlite3.OperationalError, match="bukan_writer_version"):
            legacy.execute("INSERT INTO records VALUES ('legacy',1,'paper','{}','','legacy','today')")


def test_audit_target_and_unchanged_frozen_images(store):
    entity = page()
    entity.sections[0].audit = [WikiAudit(reviewer="reviewer", reviewed_revision=999, reviewed_at="2026-09-15")]
    entity.sections[0].review_status = "independently_reviewed"
    with pytest.raises(ValueError, match="future"):
        put(store, entity)
    entity.sections[0].audit[0].reviewed_revision = 1
    assets = store.path.parent / "wiki-assets"
    assets.mkdir()
    image = assets / "figure.png"
    image.write_bytes(b"audited")
    entity.sections[0].markdown = "![Audited figure](../wiki-assets/figure.png)"
    put(store, entity)
    image.write_bytes(b"unreviewed replacement")
    entity.summary = "A summary-only revision"
    put(store, entity, 1)
    preview = wiki_detail(store, entity.id)
    assert preview["wiki"]["sections"][0]["reviewStatus"] == "independently_reviewed"
    assert next((Path(preview["path"]).parent / "assets").iterdir()).read_bytes() == b"audited"
    entity.sections[0].markdown += "\nChanged result."
    entity.sections[0].audit[0].findings = "Modified old audit"
    with pytest.raises(ValueError, match="older section"):
        put(store, entity, 2)


def test_pinned_note_figures_are_frozen_with_wiki_and_exposed_as_read_only_basis(store):
    assets = store.path.parent / "note-assets"
    assets.mkdir()
    figure = assets / "figure.png"
    figure.write_bytes(b"note original")
    note = PaperNote(id="note", source=Ref(id="s", revision=1), title="Original note", page_count=1,
                     markdown="![Original figure](../../../note-assets/figure.png)")
    put(store, note)
    put(store, page(basis="note"))
    figure.unlink()
    detail = wiki_detail(store, "wiki-test")
    basis = detail["wiki"]["sections"][0]["basis"][0]
    assert Path(basis["previewPath"]).is_file()
    assert "![Original figure](../assets/" in Path(basis["previewPath"]).read_text(encoding="utf-8")
    assert next((Path(detail["path"]).parent / "assets").iterdir()).read_bytes() == b"note original"
    frozen_ref = next(item for item in detail["wiki"]["frozenReferences"] if item["id"] == note.id)
    assert frozen_ref["previewPath"] == basis["previewPath"]
    assert frozen_ref["contextId"] == "wiki-test"


def test_remote_images_are_not_silently_mutable_wiki_assets(store):
    entity = page()
    entity.sections[0].markdown = "![Remote](https://example.test/figure.png)"
    with pytest.raises(ValueError, match="local immutable"):
        put(store, entity)


def test_candidates_refresh_suggestions_and_reopen_removed_integrations(store):
    put(store, page(basis=None))
    candidate_id = next(item["id"] for item in wiki_home(store)["candidates"] if item["recordId"] == "c")
    more = page("wiki-more", None)
    more.title = "Illuminated grains"
    put(store, more)
    candidate = WikiCandidate.model_validate(store.get(candidate_id)["entity"])
    assert more.id in candidate.suggested_page_ids
    changed = page()
    put(store, changed, 1)
    candidate.state, candidate.page_id, candidate.reason = "integrated", changed.id, "Integrated conditions."
    candidate.result_page = Ref(id=changed.id, revision=2)
    put(store, candidate, store.get(candidate_id)["revision"])
    changed.sections[0].basis = []
    put(store, changed, 2)
    reopened = store.get(candidate_id)
    assert reopened["entity"]["state"] == "pending"
    assert any(item["entity"]["state"] == "integrated" for item in store.history(candidate_id))
    claim = Claim.model_validate(store.get("c")["entity"])
    claim.text = "Completely different latest title"
    put(store, claim, 1)
    old = next(item for item in wiki_home(store)["candidates"] if item["id"] == candidate_id)
    assert old["title"] == "Illuminated grains move."
    assert old["recordRevision"] == 1


def test_removed_integration_is_specific_to_destination_and_historical_candidate(store):
    put(store, page(basis=None))
    candidate_id = next(item["id"] for item in wiki_home(store)["candidates"] if item["recordId"] == "c")
    first = page()
    put(store, first, 1)
    put(store, page("wiki-other"))
    candidate = WikiCandidate.model_validate(store.get(candidate_id)["entity"])
    candidate.state, candidate.page_id, candidate.reason = "integrated", first.id, "Integrated here."
    candidate.result_page = Ref(id=first.id, revision=2)
    put(store, candidate, store.get(candidate_id)["revision"])
    claim = sample()[3].model_copy(update={"text": "Current claim changed"})
    put(store, claim, 1)
    first.sections[0].basis = []
    put(store, first, 2)
    assert store.get(candidate_id)["entity"]["state"] == "pending"
    assert store.get("wiki-other")["entity"]["sections"][0]["basis"][0]["id"] == "c"


def test_claim_content_search_and_inline_reference_correspondence(store):
    first = page("wiki-first")
    first.sections.append(WikiSection(id="other", title="Other", basis=[WikiBasis(id="c", revision=1)]))
    put(store, first)
    assert wiki_home(store, "Illuminated grains")["pages"][0]["id"] == first.id
    second = page("wiki-second", None)
    second.sections[0].basis = [WikiBasis(id=first.id, revision=1, section_id="s1")]
    second.sections[0].markdown = "[Different section](bukan:record:wiki-first@1#other) and [Different revision](bukan:record:c@2)"
    put(store, second)
    warnings = wiki_detail(store, second.id)["wiki"]["warnings"]
    assert any("inline wiki section differs" in item["reason"] for item in warnings)
    assert any("c@2" in item["reason"] for item in warnings)


def test_portable_links_with_titles_reference_definitions_and_pinned_history(store):
    entity = page()
    entity.sections[0].markdown = '[Claim](bukan:record:c@1 "Pinned claim")\n\n[Claim again][claim]\n\n[claim]: <bukan:record:c@1> "Title"'
    put(store, entity)
    exported = export_wiki(store, entity.id)
    assert "bukan:record:c@1" not in exported["markdown"]
    assert "references/" in exported["markdown"]
    assert list((Path(exported["path"]).parent / "references").glob("*.md"))
    assert "bukan:record:c@1" in exported["previewMarkdown"]


def test_task_targets_and_completions_use_explicit_compared_revisions(store):
    entity = page()
    put(store, entity)
    entity.summary = "new head"
    put(store, entity, 1)
    created = handle_request(store, {"operation": "create-wiki-task", "pageId": entity.id,
                                    "pageRevision": 1, "sectionId": "s1", "purpose": "Recheck old interpretation"}, author="ai-client")
    task = store.get(created["id"])
    assert task["author"] == "ai-client"
    assert task["entity"]["page"]["revision"] == 1
    request = {"operation": "update-wiki-task", "id": created["id"], "expectedRevision": 1,
               "state": "completed", "outcome": "unchanged", "reason": "Compared current head"}
    with pytest.raises(ValueError, match="resultPageRevision"):
        handle_request(store, request)
    with pytest.raises(ValueError, match="page changed"):
        handle_request(store, {**request, "resultPageRevision": 1})
    handle_request(store, {**request, "resultPageRevision": 2})
    assert store.get(created["id"])["entity"]["result_page"]["revision"] == 2


def test_repointed_wiki_basis_keeps_each_interpretations_frozen_note_images(store):
    import re
    assets = store.path.parent / "note-assets"
    assets.mkdir()
    figure = assets / "figure.png"
    figure.write_bytes(b"original")
    note = PaperNote(id="note", source=Ref(id="s", revision=1), title="Same note", page_count=1,
                     markdown="![Figure](../../../note-assets/figure.png)")
    put(store, note)
    first = page("wiki-a", "note")
    put(store, first)
    figure.write_bytes(b"changed")
    second = page("wiki-b", "note")
    put(store, second)
    first.sections[0].basis = [WikiBasis(id=second.id, revision=1)]
    first.sections[0].markdown = "[Adopted note](bukan:record:note@1)"
    put(store, first, 1)
    exported = export_wiki(store, first.id, 2)
    root = Path(exported["path"]).parent
    note_files = [path for path in (root / "references").glob("*.md") if "![Figure]" in path.read_text(encoding="utf-8")]
    assert len(note_files) == 1
    image = re.search(r"!\[Figure\]\(([^)]+)\)", note_files[0].read_text(encoding="utf-8"))[1]
    assert (note_files[0].parent / image).read_bytes() == b"changed"
    frozen_ref = next(item for item in exported["frozenReferences"] if item["id"] == "note")
    assert frozen_ref["contextId"] == second.id
    assert Path(frozen_ref["previewPath"]) == note_files[0]
    assert f"[Adopted note]({frozen_ref['relativePath']})" in exported["markdown"]
    old = export_wiki(store, first.id, 1)
    assert next((Path(old["path"]).parent / "assets").iterdir()).read_bytes() == b"original"


def test_unresolved_export_link_stays_fixed_when_target_is_later_registered(store):
    entity = page()
    entity.sections[0].markdown = "[Future](bukan:record:future@1)\n\n<bukan:record:c@1>"
    put(store, entity)
    original = export_wiki(store, entity.id)
    assert "<bukan:record:c@1>" not in original["markdown"]
    assert "[bukan:record:c@1](references/" in original["markdown"]
    later = sample()[3].model_copy(update={"id": "future"})
    put(store, later)
    assert export_wiki(store, entity.id) == original
