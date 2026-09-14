import hashlib
from pathlib import Path

import pytest

from bukan_research.models import Paper, Source, PaperNote, PageReview, Ref, Write
from bukan_research.notes import get_note, export_note
from bukan_research.store import Store


def test_notes_search_update_and_exports_preserve_human_edits(tmp_path):
    store = Store(tmp_path / "research.sqlite")
    store.initialize()
    source = Source(id="s", paper=Ref(id="p", revision=1), uri="fixture://paper.pdf",
                    version="v1", locator="PDF page 1", source_type="body", text="Source",
                    asset_sha256="a" * 64)
    note = PaperNote(id="note:one", source=Ref(id="s", revision=1), title="文献の知見",
                     markdown="負の初期電荷の条件を確認する。原文 p. 1。", page_count=2,
                     coverage=[PageReview(page=1, text="reviewed", visuals="reviewed")])
    store.put([Write(entity=e) for e in [Paper(id="p", title="Sample paper"), source, note]])
    assert get_note(store, note.id)["reading_status"] == "partial"
    assert store.search("初期電荷", "paper_note")["items"][0]["entity"]["id"] == note.id
    exported = export_note(store, note.id)
    path = Path(exported["path"])
    assert exported["export_format_version"] == 2
    assert path.name == "index.md"
    assert path.parent.name == "r1"
    assert "| 2 | unread | unread |" in path.read_text(encoding="utf-8")
    assert export_note(store, note.id)["path"] == str(path)
    path.write_text("Human additions", encoding="utf-8")
    with pytest.raises(ValueError, match="local edits"):
        export_note(store, note.id)
    note.markdown += "\nHuman additions"
    note.coverage.append(PageReview(page=2, text="reviewed", visuals="not_present"))
    store.put([Write(entity=note, expected_revision=1)], author="local-user")
    latest = export_note(store, note.id)
    assert latest["reading_status"] == "full_text_reviewed"
    assert latest["path"] != str(path)
    assert path.read_text(encoding="utf-8") == "Human additions"
    assert get_note(store, note.id, 1)["reading_status"] == "partial"
    assert any(n["entity"]["id"] == "s" for n in store.trace(note.id)["nodes"])
    with pytest.raises(ValueError, match="Revision conflict"):
        store.put([Write(entity=note.model_copy(update={"markdown": "stale"}), expected_revision=1)])


def test_full_reading_requires_every_page_and_visual_review():
    note = PaperNote(id="n", source=Ref(id="s", revision=1), title="Note", markdown="Screening",
                     page_count=2)
    assert note.reading_status == "screening"
    note.coverage = [PageReview(page=page, text="reviewed") for page in [1, 2]]
    assert note.reading_status == "partial"
    note.coverage[0].visuals = "reviewed"
    note.coverage[1].visuals = "failed"
    assert note.reading_status == "partial"
    note.coverage[1].visuals = "not_present"
    assert note.reading_status == "full_text_reviewed"


@pytest.mark.parametrize("source_type", ["abstract", "metadata"])
def test_non_body_source_cannot_report_full_text_review(tmp_path, source_type):
    store = Store(tmp_path / "research.sqlite")
    store.initialize()
    source = Source(id="s", paper=Ref(id="p", revision=1), uri="fixture://paper.pdf",
                    version="v1", locator="PDF page 1", source_type=source_type,
                    text="Screening capture", asset_sha256="a" * 64)
    note = PaperNote(id="n", source=Ref(id="s", revision=1), title="Note",
                     markdown="Only a screening source is registered.", page_count=1,
                     coverage=[PageReview(page=1, text="reviewed", visuals="not_present")])
    store.put([Write(entity=e) for e in [Paper(id="p", title="Paper"), source, note]])
    result = export_note(store, note.id)
    assert result["reading_status"] == "partial"
    assert result["source_type"] == source_type
    assert "Reading status: `partial`" in Path(result["path"]).read_text(encoding="utf-8")


def test_exports_from_neighboring_stores_do_not_collide(tmp_path):
    paths = []
    for name in ["first", "second"]:
        store = Store(tmp_path / f"{name}.sqlite")
        store.initialize()
        source = Source(id="s", paper=Ref(id="p", revision=1), uri="fixture://p.pdf",
                        version="v1", locator="p. 1", source_type="body", text="Source", asset_sha256="a"*64)
        note = PaperNote(id="n", source=Ref(id="s", revision=1), title="Note",
                         markdown=name, page_count=1)
        store.put([Write(entity=e) for e in [Paper(id="p", title="Paper"), source, note]])
        paths.append(Path(export_note(store, "n")["path"]))
    assert paths[0] != paths[1]
    assert "first" in paths[0].read_text(encoding="utf-8")
    assert "second" in paths[1].read_text(encoding="utf-8")


@pytest.mark.parametrize("pages", [[1, 1], [1, 3]])
def test_invalid_coverage_cannot_be_recorded(pages):
    with pytest.raises(ValueError, match="unique PDF"):
        PaperNote(id="n", source=Ref(id="s", revision=1), title="N", markdown="N", page_count=2,
                  coverage=[PageReview(page=page, text="reviewed", visuals="reviewed") for page in pages])


def _store_with_markdown(tmp_path, markdown):
    store = Store(tmp_path / "research.sqlite")
    store.initialize()
    source = Source(id="s", paper=Ref(id="p", revision=1), uri="fixture://paper.pdf",
                    version="v1", locator="PDF page 1", source_type="body", text="Source",
                    asset_sha256="a" * 64)
    note = PaperNote(id="n", source=Ref(id="s", revision=1), title="Note",
                     markdown=markdown, page_count=1)
    store.put([Write(entity=e) for e in [Paper(id="p", title="Paper"), source, note]])
    return store


def _image(tmp_path, filename="figure.png"):
    image = tmp_path / "note-assets" / "paper" / filename
    image.parent.mkdir(parents=True, exist_ok=True)
    image.write_bytes(b"\x89PNG\r\n\x1a\n\x00fixture image bytes\xff")
    return image


def _legacy_path(store):
    return (store.path.parent / "paper-notes" /
            hashlib.sha256(store.path.name.encode()).hexdigest() /
            hashlib.sha256(b"n").hexdigest() / "r1.md")


def test_export_bundles_identical_images_and_preserves_db_and_legacy_snapshots(tmp_path):
    source = _image(tmp_path)
    original = "Text.\n\n![図 1](../../../note-assets/paper/figure.png \"Caption\")\n"
    store = _store_with_markdown(tmp_path, original)
    legacy = _legacy_path(store)
    legacy.parent.mkdir(parents=True)
    legacy.write_bytes(get_note(store, "n")["markdown"].replace("\n", "\r\n").encode())
    preserved = {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in [source, legacy, store.path]}

    exported = export_note(store, "n")
    target = Path(exported["path"])
    filename = hashlib.sha256(source.read_bytes()).hexdigest()[:32] + ".png"
    bundled = target.parent / "assets" / filename
    assert bundled.is_relative_to(target.parent)
    assert bundled.read_bytes() == source.read_bytes()
    assert f'![図 1](assets/{filename} "Caption")' in exported["markdown"]
    assert exported["markdown"].encode() == target.read_bytes()
    assert store.get("n")["entity"]["markdown"] == original
    assert export_note(store, "n") == exported
    for path, before in preserved.items():
        assert (path.read_bytes(), path.stat().st_mtime_ns) == before


def test_export_rejects_same_revision_legacy_edits_but_preserves_older_ones(tmp_path):
    _image(tmp_path)
    store = _store_with_markdown(tmp_path, "![caption](../../../note-assets/paper/figure.png)")
    legacy = _legacy_path(store)
    legacy.parent.mkdir(parents=True)
    legacy.write_bytes(b"Legacy snapshot with human edits\r\n")
    before = store.path.read_bytes()
    with pytest.raises(ValueError, match="local edits"):
        export_note(store, "n")
    assert list((tmp_path / "paper-notes").rglob("index.md")) == []
    assert store.path.read_bytes() == before

    note = PaperNote.model_validate(store.get("n")["entity"])
    note.markdown += "\nNew revision"
    store.put([Write(entity=note, expected_revision=1)])
    assert Path(export_note(store, "n")["path"]).parent.name == "r2"
    assert legacy.read_bytes() == b"Legacy snapshot with human edits\r\n"


@pytest.mark.parametrize("embedding", [
    '![caption](../../../note-assets/paper/figure%20one.PNG "title")',
    '![caption](<../../../note-assets/paper/figure one.PNG> \'title\')',
    '> ![caption](../../../note-assets/paper/figure%20one.PNG)',
    '- ![caption](../../../note-assets/paper/figure%20one.PNG)',
    '[![caption](../../../note-assets/paper/figure%20one.PNG)](https://example.com)',
    '> Paragraph\n> ![caption](../../../note-assets/paper/figure%20one.PNG)',
])
def test_export_preserves_markdown_structure_and_accepts_encoded_spaces(tmp_path, embedding):
    source = _image(tmp_path, "figure one.PNG")
    store = _store_with_markdown(tmp_path, embedding)
    exported = export_note(store, "n")
    filename = hashlib.sha256(source.read_bytes()).hexdigest()[:32] + ".png"
    expected = embedding.replace('<../../../note-assets/paper/figure one.PNG>', f"assets/{filename}")
    expected = expected.replace('../../../note-assets/paper/figure%20one.PNG', f"assets/{filename}")
    assert expected in exported["markdown"]
    assert len(list((Path(exported["path"]).parent / "assets").iterdir())) == 1


def test_export_only_rewrites_real_images_and_leaves_remote_images_external(tmp_path):
    source = _image(tmp_path)
    local = "![caption](../../../note-assets/paper/figure.png)"
    untouched = (f"A path in prose: ../../../note-assets/paper/figure.png\n\n"
                 f"`{local}`\n\n```markdown\n{local}\n```\n\n"
                 f"    {local}\n\n\\{local}\n\n"
                 f"![Remote](https://example.com/image.png \"Remote title\")\n\n"
                 f"<!-- <img src=\"../../../note-assets/paper/figure.png\"> -->")
    store = _store_with_markdown(tmp_path, untouched + f"\n\n`{local}` and {local}\n")
    exported = export_note(store, "n")
    assert untouched in exported["markdown"]
    filename = hashlib.sha256(source.read_bytes()).hexdigest()[:32] + ".png"
    assert f"`{local}` and ![caption](assets/{filename})" in exported["markdown"]


@pytest.mark.parametrize("newline", ["\n", "\r\n", "\r"])
def test_unicode_line_separators_do_not_shift_image_rewrites_into_code(tmp_path, newline):
    source = _image(tmp_path)
    local = "![caption](../../../note-assets/paper/figure.png)"
    markdown = newline.join(["```text", "x\u2028x\u2028x\u2028x", local, "```", "", local, ""])
    store = _store_with_markdown(tmp_path, markdown)
    canonical = get_note(store, "n")["markdown"]
    filename = hashlib.sha256(source.read_bytes()).hexdigest()[:32] + ".png"
    start = canonical.rfind(local)
    expected = canonical[:start] + f"![caption](assets/{filename})" + canonical[start + len(local):]
    result = export_note(store, "n")
    assert result["markdown"] == expected
    assert Path(result["path"]).read_bytes() == expected.encode()


@pytest.mark.parametrize("prefix", ["- first\n\t", "> - first\n>\t", "- first\n  second\n\t"])
def test_tab_indentation_is_preserved_while_local_images_are_bundled(tmp_path, prefix):
    source = _image(tmp_path)
    local = "![caption](../../../note-assets/paper/figure.png)"
    store = _store_with_markdown(tmp_path, prefix + f"`{local}` and {local}")
    canonical = get_note(store, "n")["markdown"]
    filename = hashlib.sha256(source.read_bytes()).hexdigest()[:32] + ".png"
    expected = canonical.replace(f"and {local}", f"and ![caption](assets/{filename})")
    assert export_note(store, "n")["markdown"] == expected


@pytest.mark.parametrize("edit", ["change", "delete", "source_change"])
def test_export_rejects_modified_or_missing_bundled_assets(tmp_path, edit):
    source = _image(tmp_path)
    store = _store_with_markdown(tmp_path, "![caption](../../../note-assets/paper/figure.png)")
    target = Path(export_note(store, "n")["path"])
    bundled = next((target.parent / "assets").iterdir())
    if edit == "change":
        bundled.write_bytes(b"Human image changes")
    elif edit == "delete":
        bundled.unlink()
    else:
        source.write_bytes(b"New source image")
    before = {path: path.read_bytes() for path in target.parent.rglob("*") if path.is_file()}
    with pytest.raises(ValueError, match="local edits"):
        export_note(store, "n")
    assert {path: path.read_bytes() for path in target.parent.rglob("*") if path.is_file()} == before


@pytest.mark.parametrize("image_url, error", [
    ("../../../note-assets/paper/missing.png", "missing"),
    ("../../../outside.png", "inside.*note-assets"),
    ("../../../note-assets/%2e%2e/outside.png", "inside.*note-assets"),
    ("file:///outside.png", "Unsupported local image URL"),
    ("//server/share/image.png", "Unsupported local image URL"),
    ("../../../note-assets/Paperpile/figure.png", "outside Paperpile"),
])
def test_invalid_source_prevents_publishing_any_bundle(tmp_path, image_url, error):
    _image(tmp_path)
    (tmp_path / "outside.png").write_bytes(b"Outside image")
    store = _store_with_markdown(tmp_path, "![Valid](../../../note-assets/paper/figure.png)\n\n"
                                 f"![Invalid]({image_url})")
    before = store.path.read_bytes()
    with pytest.raises(ValueError, match=error):
        export_note(store, "n")
    assert not (tmp_path / "paper-notes").exists()
    assert store.path.read_bytes() == before


@pytest.mark.parametrize("markdown, error", [
    ("![caption][figure]\n\n[figure]: ../../../note-assets/paper/figure.png", "Reference-style"),
    ('<img src="../../../note-assets/paper/figure.png">', "HTML images"),
])
def test_unsupported_local_image_syntax_fails_explicitly(tmp_path, markdown, error):
    _image(tmp_path)
    store = _store_with_markdown(tmp_path, markdown)
    with pytest.raises(ValueError, match=error):
        export_note(store, "n")
    assert not (tmp_path / "paper-notes").exists()


@pytest.mark.parametrize("table", [
    "| Figure |\n| --- |\n| {image} |",
    "| A | B | C |\n| --- | --- | --- |\n| ` | {image} | ` |",
    "| A | B |\n| --- | --- |\n| `{image}` | {image} |",
    "| A | B |\n| --- | --- |\n| {image} | {image} |",
])
def test_local_images_in_gfm_tables_fail_before_any_files_are_published(tmp_path, table):
    _image(tmp_path)
    local = "![caption](../../../note-assets/paper/figure.png)"
    markdown = local + "\n\n" + table.format(image=local)
    store = _store_with_markdown(tmp_path, markdown)
    with pytest.raises(ValueError, match="inside Markdown tables.*below the table"):
        export_note(store, "n")
    assert not (tmp_path / "paper-notes").exists()
    assert store.get("n")["entity"]["markdown"] == markdown


def test_gfm_tables_with_prose_code_and_remote_images_remain_unchanged(tmp_path):
    source = _image(tmp_path)
    local = "![caption](../../../note-assets/paper/figure.png)"
    table = ("| Prose | Code | Remote |\n| --- | --- | --- |\n"
             f"| ../../../note-assets/paper/figure.png | `{local}` | "
             "![Remote](https://example.com/image.png) |")
    store = _store_with_markdown(tmp_path, table + "\n\n" + local)
    exported = export_note(store, "n")
    filename = hashlib.sha256(source.read_bytes()).hexdigest()[:32] + ".png"
    assert table + f"\n\n![caption](assets/{filename})" in exported["markdown"]
    assert len(list((Path(exported["path"]).parent / "assets").iterdir())) == 1


def _symlink(link, target):
    try:
        link.symlink_to(target, target_is_directory=target.is_dir())
    except OSError as exc:
        pytest.skip(f"Symlink creation is unavailable: {exc}")


def test_image_symlink_cannot_escape_note_assets(tmp_path):
    outside = tmp_path / "outside.png"
    outside.write_bytes(b"Outside image")
    source = _image(tmp_path)
    source.unlink()
    _symlink(source, outside)
    store = _store_with_markdown(tmp_path, "![caption](../../../note-assets/paper/figure.png)")
    with pytest.raises(ValueError, match="inside.*note-assets"):
        export_note(store, "n")
    assert not (tmp_path / "paper-notes").exists()
    assert outside.read_bytes() == b"Outside image"


def test_export_rejects_symlinked_note_asset_root(tmp_path):
    outside = tmp_path / "other-assets"
    outside.mkdir()
    (outside / "figure.png").write_bytes(b"Outside image")
    _symlink(tmp_path / "note-assets", outside)
    store = _store_with_markdown(tmp_path, "![caption](../../../note-assets/figure.png)")
    with pytest.raises(ValueError, match="note-assets.*symlink"):
        export_note(store, "n")
    assert not (tmp_path / "paper-notes").exists()


@pytest.mark.parametrize("redirect", ["markdown", "assets", "legacy"])
def test_export_rejects_symlinked_output_paths(tmp_path, redirect):
    _image(tmp_path)
    store = _store_with_markdown(tmp_path, "![caption](../../../note-assets/paper/figure.png)")
    target = Path(export_note(store, "n")["path"])
    if redirect == "markdown":
        original = target
    elif redirect == "assets":
        original = next((target.parent / "assets").iterdir())
    else:
        original = _legacy_path(store)
        original.parent.mkdir(parents=True)
        original.write_text(get_note(store, "n")["markdown"], encoding="utf-8")
    outside = tmp_path / "human-file"
    outside.write_bytes(original.read_bytes())
    original.unlink()
    _symlink(original, outside)
    with pytest.raises(ValueError, match="symlink"):
        export_note(store, "n")
    assert outside.read_bytes() == original.read_bytes()
