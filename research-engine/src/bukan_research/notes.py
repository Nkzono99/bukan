"""Versioned Markdown notes. Search/update uses the store; exports never overwrite edits."""
import hashlib

from .models import PaperNote
from .note_assets import bundle_images
from .store import Store
from .exports import export_path, publish_bundle


def get_note(store: Store, record_id: str, revision: int | None = None):
    record = store.get(record_id, revision)
    note = PaperNote.model_validate(record["entity"])
    source = store.get(note.source.id, note.source.revision)["entity"]
    paper = store.get(source["paper"]["id"], source["paper"]["revision"])["entity"]
    reading_status = note.reading_status
    if source["source_type"] != "body" and reading_status == "full_text_reviewed":
        reading_status = "partial"
    lines = [f"# {note.title}", "", f"- Note: `{note.id}@{record['revision']}`",
             f"- Paper: {paper['title']} (`{paper['id']}`)",
             f"- PDF SHA-256: `{source['asset_sha256']}`",
             f"- Source: `{source['uri']}`",
             f"- Reading status: `{reading_status}` (reader-reported; not independently verified)",
             f"- Total PDF file pages: {note.page_count}", "",
             "## Reading coverage", "", "| PDF page | Text | Visuals | Note |", "|---|---|---|---|"]
    coverage = {page.page: page for page in note.coverage}
    for page in range(1, note.page_count + 1):
        item = coverage.get(page)
        comment = item.note.replace("|", "\\|").replace("\n", " ") if item else ""
        lines.append(f"| {page} | {item.text if item else 'unread'} | {item.visuals if item else 'unread'} | {comment} |")
    lines += ["", "## Literature note", "", note.markdown, "", "## Supporting records", ""]
    lines += [f"- `{ref.id}@{ref.revision}`" for ref in note.basis]
    return {"record_id": note.id, "revision": record["revision"],
            "reading_status": reading_status, "source_type": source["source_type"],
            "source_sha256": source["asset_sha256"],
            "markdown": "\n".join(lines).rstrip() + "\n"}


def note_authoring_path(store: Store, record_id: str, revision: int):
    """Return the legacy Markdown base for DB-authored links, without creating it."""
    # Hashing allows all store IDs (including ':' on Windows), with no path traversal.
    key = hashlib.sha256(record_id.encode("utf-8")).hexdigest()
    store_key = hashlib.sha256(store.path.name.encode("utf-8")).hexdigest()
    return export_path(store.path.parent / "paper-notes" / store_key / key / f"r{revision}.md")


def export_note(store: Store, record_id: str, revision: int | None = None):
    """Export an immutable format-2 bundle, preserving legacy snapshots and DB text.

    Authored notes retain paths such as ../../../note-assets/paper/figure.png,
    resolved relative to the legacy rN.md parent. Only the export rewrites them
    to child assets, which standalone Markdown previews can load.
    """
    note = get_note(store, record_id, revision)
    legacy = note_authoring_path(store, record_id, note["revision"])
    legacy_parent = legacy.parent
    if legacy.exists():
        canonical = note["markdown"].replace("\r\n", "\n").replace("\r", "\n")
        if not legacy.is_file() or legacy.read_text(encoding="utf-8") != canonical:
            raise ValueError(f"Legacy export contains local edits; preserve them and update the note as a new revision: {legacy}")
    # Shorter format-2 hashes keep child assets within Windows path limits.
    parent = store.path.parent / "paper-notes" / legacy_parent.parent.name[:16] / legacy_parent.name[:32]
    target = export_path(parent / f"r{note['revision']}" / "index.md")
    content, assets = bundle_images(note["markdown"], legacy_parent, store.path.parent / "note-assets")
    files = {target.parent / "assets" / name: data for name, data in assets.items()}
    publish_bundle(target, content, files)
    return {**note, "markdown": content, "path": str(target), "export_format_version": 2}
