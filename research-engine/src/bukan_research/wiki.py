"""Research wiki revisions, immutable image snapshots, and resumable integration work."""

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
from markdown_it import MarkdownIt

from .models import ENTITY, Ref, WikiBasis, WikiCandidate, WikiPage, WikiSection, WikiTask, references
from .note_assets import bundle_images
from .exports import export_path, publish_bundle
from .record_markdown import literal_text, record_markdown
from .store import TransactionStore


def prepare_page(page, old):
    """Edits invalidate inherited audit results; provenance is never silently repinned."""
    page = page.model_copy(deep=True)
    previous = {section.id: section for section in old.sections} if old else {}
    for section in page.sections:
        prior = previous.get(section.id)
        changed = prior and any(getattr(prior, field) != getattr(section, field)
                                for field in ("title", "markdown", "basis", "interpretation"))
        if changed and section.audit == prior.audit:
            section.audit = []
            section.review_status = "unverified"
            page.verified_at = ""
        if section.review_status == "independently_reviewed" and not section.audit:
            raise ValueError("An independently reviewed section needs an audit record.")
    if old and any(getattr(page, field) != getattr(old, field) for field in ("title", "summary")):
        page.verified_at = ""
    return page


def validate_wiki_entity(entity, resolve, db, pending):
    def current(identifier):
        matches = [(revision, item[0]) for (rid, revision), item in pending.items() if rid == identifier]
        if matches:
            return max(matches, key=lambda pair: pair[0])
        row = db.execute("SELECT r.revision,r.body FROM records r JOIN heads h ON r.id=h.id AND r.revision=h.revision WHERE r.id=?", (identifier,)).fetchone()
        if row is None:
            raise ValueError(f"Record not found: {identifier}")
        return row[0], ENTITY.validate_json(row[1])

    def wiki_destination(identifier):
        revision, target = current(identifier)
        if not isinstance(target, WikiPage):
            raise ValueError("Wiki navigation must target wiki pages.")
        return revision, target

    if isinstance(entity, WikiPage):
        next_revision = current(entity.id)[0]
        for section in entity.sections:
            for audit in section.audit:
                if audit.reviewed_revision > next_revision:
                    raise ValueError("An audit cannot verify a future wiki revision.")
                if section.review_status == "independently_reviewed" and audit.reviewed_revision < next_revision:
                    reviewed = resolve(Ref(id=entity.id, revision=audit.reviewed_revision))
                    prior = next((item for item in reviewed.sections if item.id == section.id), None)
                    if prior is None or any(getattr(prior, field) != getattr(section, field)
                                            for field in ("title", "markdown", "basis", "interpretation")):
                        raise ValueError("An audit of an older section cannot verify changed content.")
        for identifier in entity.parent_ids + entity.navigation:
            wiki_destination(identifier)
        for section in entity.sections:
            for basis in section.basis:
                target = resolve(basis)
                if basis.section_id and (not isinstance(target, WikiPage) or
                                         basis.section_id not in {item.id for item in target.sections}):
                    raise ValueError("Pinned wiki section does not exist in the referenced revision.")
                if target.kind == "topic" and basis.relation != "context":
                    raise ValueError("A Topic is contextual provenance, not primary evidence.")
                if isinstance(target, WikiPage):
                    # A wiki interpretation must reach original research records.
                    # Navigation is deliberately absent from this traversal.
                    stack = [(basis, {(entity.id, next_revision, section.id)})]
                    original = False
                    seen = set()
                    while stack:
                        ref, ancestors = stack.pop()
                        item = resolve(ref)
                        if isinstance(item, WikiPage):
                            key = (ref.id, ref.revision, getattr(ref, "section_id", None))
                            if key in ancestors:
                                raise ValueError("Circular wiki evidence dependency; use navigation for related pages.")
                            if key in seen:
                                continue
                            seen.add(key)
                            sections = [s for s in item.sections if not getattr(ref, "section_id", None) or s.id == ref.section_id]
                            stack.extend((child, ancestors | {key}) for s in sections for child in s.basis)
                        elif item.kind in {"source", "evidence", "claim", "relation", "paper_note"}:
                            original = True
                        else:
                            stack.extend((child, ancestors) for child, _ in references(item))
                    if not original:
                        raise ValueError("A wiki interpretation needs underlying original research records; use navigation otherwise.")
    elif isinstance(entity, WikiTask):
        page = resolve(entity.page)
        if entity.section_id and entity.section_id not in {section.id for section in page.sections}:
            raise ValueError("Task section is absent from its pinned page.")
        if entity.result_page:
            if entity.result_page.id != entity.page.id:
                raise ValueError("Task result must revise the assigned page.")
            if entity.outcome == "revised" and entity.result_page.revision <= entity.page.revision:
                raise ValueError("Task revised outcome needs a newer page revision.")
        if entity.state == "completed":
            result = entity.result_page or entity.page
            if current(result.id)[0] != result.revision:
                raise ValueError("Wiki page changed; compare the current revision before completing this task.")
    elif isinstance(entity, WikiCandidate):
        if entity.page_id:
            wiki_destination(entity.page_id)
        for identifier in entity.suggested_page_ids:
            wiki_destination(identifier)
        if entity.result_page and entity.result_page.id != entity.page_id:
            raise ValueError("Candidate result must reference its destination page.")
        if entity.state == "integrated":
            if current(entity.result_page.id)[0] != entity.result_page.revision:
                raise ValueError("Wiki page changed; compare the current revision before integrating this candidate.")
            target = resolve(entity.result_page)
            reachable = _reachable(resolve, [ref for section in target.sections for ref in section.basis])
            if (entity.record.id, entity.record.revision) not in reachable:
                raise ValueError("An integrated candidate must be present in the pinned page's evidence chain.")


def _reachable(resolve, roots, *, sections=False, stop_at_wiki=False):
    pending, seen, records = list(roots), set(), set()
    while pending:
        ref = pending.pop()
        section_id = getattr(ref, "section_id", None)
        key = (ref.id, ref.revision, section_id)
        if key in seen:
            continue
        seen.add(key)
        records.add((ref.id, ref.revision))
        target = resolve(ref)
        if stop_at_wiki and isinstance(target, WikiPage):
            continue
        pending.extend([child for section in target.sections if not section_id or section.id == section_id for child in section.basis]
                       if isinstance(target, WikiPage) else [child for child, _ in references(target)])
    return seen if sections else records


def _heads(db):
    return {row["id"]: {"entity": ENTITY.validate_json(row["body"]), "revision": row["revision"],
                        "created_at": row["created_at"]} for row in db.execute(
        "SELECT r.* FROM records r JOIN heads h ON r.id=h.id AND r.revision=h.revision")}


def _resolver(db):
    cache = {}
    def resolve(ref):
        key = (ref.id, ref.revision)
        if key not in cache:
            row = db.execute("SELECT body FROM records WHERE id=? AND revision=?", key).fetchone()
            if row is None:
                raise ValueError(f"Record not found: {ref.id}@{ref.revision}")
            cache[key] = ENTITY.validate_json(row[0])
        return cache[key]
    return resolve


def freshness(db, page):
    heads = dict(db.execute("SELECT id,revision FROM heads"))
    resolve = _resolver(db)
    sections = []
    for section in page.sections:
        pending = [(ref, []) for ref in section.basis]
        seen, reasons = set(), []
        while pending:
            ref, route = pending.pop()
            section_id = getattr(ref, "section_id", None)
            key = (ref.id, ref.revision, section_id)
            if key in seen:
                continue
            seen.add(key)
            path = route + [{"id": ref.id, "revision": ref.revision}]
            if heads[ref.id] != ref.revision:
                reasons.append({"recordId": ref.id, "pinnedRevision": ref.revision,
                                "currentRevision": heads[ref.id], "path": path,
                                "reason": "A supporting record has a newer revision; compare it before updating the interpretation."})
            target = resolve(ref)
            children = ([child for item in target.sections if not section_id or item.id == section_id for child in item.basis]
                        if isinstance(target, WikiPage) else [child for child, _ in references(target)])
            pending.extend((child, path) for child in children)
        if reasons:
            sections.append({"sectionId": section.id, "reasons": reasons})
    return {"state": "needs_review" if sections else "current", "sections": sections}


def _insert_work(db, entity, now):
    """Insert only new deterministic work IDs inside the originating write transaction."""
    existing = db.execute("SELECT r.revision,r.body FROM records r JOIN heads h ON r.id=h.id AND r.revision=h.revision WHERE r.id=?", (entity.id,)).fetchone()
    revision = 1
    if existing:
        old = ENTITY.validate_json(existing[1])
        if not isinstance(entity, WikiCandidate):
            return
        if old.state == "integrated":
            entity = old.model_copy(update={"state": "pending", "result_page": None,
                                           "suggested_page_ids": entity.suggested_page_ids,
                                           "reason": "The previously integrated record is absent from current wiki evidence; compare the removed basis before deciding again."})
        elif old.state == "pending" and old.suggested_page_ids != entity.suggested_page_ids:
            entity = old.model_copy(update={"suggested_page_ids": entity.suggested_page_ids})
        else:
            return
        revision = existing[0] + 1
    body = json.dumps(entity.model_dump(), ensure_ascii=False, sort_keys=True)
    digest = hashlib.sha256(body.encode()).hexdigest()
    db.execute("INSERT INTO records VALUES (?,?,?,?,?,?,?)", (entity.id, revision, entity.kind, body, digest, "wiki-index", now))
    db.execute("INSERT INTO heads VALUES (?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision", (entity.id, revision))
    for ref, _ in references(entity):
        db.execute("INSERT OR IGNORE INTO links VALUES (?,?,?,?)", (entity.id, revision, ref.id, ref.revision))


def sync_work(db, now=None):
    """Persist both disconnected research candidates and section-specific stale work."""
    heads = _heads(db)
    pages = [record for record in heads.values() if isinstance(record["entity"], WikiPage)]
    if not pages:
        return
    now = now or datetime.now(timezone.utc).isoformat()
    resolve = _resolver(db)
    linked = _reachable(resolve, [ref for record in pages for section in record["entity"].sections for ref in section.basis])
    for record in heads.values():
        candidate = record["entity"]
        if isinstance(candidate, WikiCandidate) and candidate.state == "integrated":
            destination = heads[candidate.page_id]["entity"]
            current_basis = _reachable(resolve, [ref for section in destination.sections for ref in section.basis])
            if (candidate.record.id, candidate.record.revision) not in current_basis:
                _insert_work(db, candidate, now)
    for identifier, record in heads.items():
        entity, revision = record["entity"], record["revision"]
        if entity.kind not in {"paper", "source", "evidence", "claim", "relation", "paper_note"} or (identifier, revision) in linked:
            continue
        text = json.dumps(entity.model_dump(), ensure_ascii=False).casefold()
        suggestions = []
        for page_record in pages:
            page = page_record["entity"]
            terms = [page.title, *page.aliases, *page.categories]
            if any(term.casefold() in text for term in terms if len(term.strip()) >= 3):
                suggestions.append(page.id)
        key = hashlib.sha256(f"{identifier}@{revision}".encode()).hexdigest()[:32]
        _insert_work(db, WikiCandidate(id=f"wiki-candidate-{key}", record=Ref(id=identifier, revision=revision),
                                     suggested_page_ids=suggestions,
                                     reason="Unintegrated research record; suggested matches are lexical candidates, not scientific relevance decisions."), now)
    for record in pages:
        page = record["entity"]
        for section in freshness(db, page)["sections"]:
            signature = json.dumps(section, sort_keys=True)
            key = hashlib.sha256(f"{page.id}@{record['revision']}:{signature}".encode()).hexdigest()[:32]
            _insert_work(db, WikiTask(id=f"wiki-update-{key}", page=Ref(id=page.id, revision=record["revision"]),
                                     section_id=section["sectionId"], purpose="Compare changed supporting records and update or retain this section with a reason.",
                                     checkpoint=signature), now)


def wiki_authoring_path(store, identifier):
    key = hashlib.sha256(identifier.encode()).hexdigest()[:32]
    return export_path(store.path.parent / "wiki" / f"{key}.md")


def page_markdown(page, revision):
    lines = [f"# {literal_text(page.title)}", "", page.summary, "", f"<!-- {page.id}@{revision} -->", ""]
    if page.search_through:
        lines += [f"Search through: {literal_text(page.search_through)}", ""]
    for section in page.sections:
        lines += [f'<a id="{section.id}"></a>', f"## {literal_text(section.title)}", "", section.markdown, ""]
        if section.basis:
            lines += ["### Pinned basis", ""]
            for ref in section.basis:
                fragment = f"#{ref.section_id}" if ref.section_id else ""
                lines.append(f"- [{ref.id}@{ref.revision}](bukan:record:{ref.id}@{ref.revision}{fragment}) ({ref.relation})")
            lines += [""]
    return "\n".join(lines).rstrip() + "\n"


def snapshot_page(store, db, page, revision):
    from .notes import get_note, note_authoring_path

    def bundle(markdown, base, asset_root):
        parser = MarkdownIt("commonmark")
        if any((token.attrGet("src") or "").lower().startswith(("http:", "https:"))
               for block in parser.parse(markdown) for token in block.children or [] if token.type == "image"):
            raise ValueError("Wiki snapshots require local immutable image assets; copy remote figures into wiki-assets before saving.")
        return bundle_images(markdown, base, asset_root)

    assets, note_documents = {}, {}
    previous = None
    if revision > 1:
        previous = WikiPage.model_validate(json.loads(db.execute("SELECT body FROM records WHERE id=? AND revision=?", (page.id, revision - 1)).fetchone()[0]))
        previous_markdown = db.execute("SELECT markdown FROM wiki_snapshots WHERE id=? AND revision=?", (page.id, revision - 1)).fetchone()[0]
        assets.update(dict(db.execute("SELECT name,content FROM wiki_assets WHERE id=? AND revision=?", (page.id, revision - 1))))
        note_documents.update({(row[0], row[1]): row[2] for row in db.execute(
            "SELECT target_id,target_revision,markdown FROM wiki_documents WHERE id=? AND revision=?", (page.id, revision - 1))})
    frozen = page.model_copy(deep=True)
    base, asset_root = wiki_authoring_path(store, page.id).parent, store.path.parent / "wiki-assets"
    if previous and previous.summary == page.summary:
        frozen.summary = _preview_summary(previous_markdown, previous, revision - 1)
    else:
        frozen.summary, images = bundle(page.summary, base, asset_root)
        assets.update(images)
    for section in frozen.sections:
        prior = next((item for item in previous.sections if item.id == section.id), None) if previous else None
        if prior and all(getattr(prior, field) == getattr(section, field) for field in ("title", "markdown", "basis", "interpretation")):
            # An unchanged section keeps its audited image bytes even if an
            # authoring asset was replaced while another section was edited.
            section.markdown = _preview_section(previous_markdown, previous, prior)
        else:
            section.markdown, images = bundle(section.markdown, base, asset_root)
            assets.update(images)
    markdown = page_markdown(frozen, revision)
    db.execute("INSERT INTO wiki_snapshots VALUES (?,?,?)", (page.id, revision, markdown))
    resolve = _resolver(db)
    # Referenced wiki revisions own their own frozen note/image snapshots. Keep
    # those contexts separate when two interpretations adopted the same note.
    related = _reachable(resolve, [ref for section in page.sections for ref in section.basis], stop_at_wiki=True)
    current = TransactionStore(store, db)
    for key in related:
        if resolve(Ref(id=key[0], revision=key[1])).kind == "paper_note" and key not in note_documents:
            note = get_note(current, *key)
            content, images = bundle(note["markdown"], note_authoring_path(store, *key).parent, store.path.parent / "note-assets")
            note_documents[key] = content
            assets.update(images)
    note_documents = {key: content for key, content in note_documents.items() if key in related}
    parser = MarkdownIt("commonmark")
    used_images = {(token.attrGet("src") or "").removeprefix("assets/").split("#", 1)[0]
                   for content in [markdown, *note_documents.values()] for block in parser.parse(content)
                   for token in block.children or [] if token.type == "image"}
    assets = {name: content for name, content in assets.items() if name in used_images}
    db.executemany("INSERT INTO wiki_documents VALUES (?,?,?,?,?)", [(page.id, revision, *key, content) for key, content in note_documents.items()])
    db.executemany("INSERT INTO wiki_assets VALUES (?,?,?,?)", [(page.id, revision, name, content) for name, content in assets.items()])


def _portable_bundle(store, db, identifier, revision):
    """Bundle linked wiki revisions and their exact provenance into local Markdown files."""

    root = (identifier, revision, None)
    root_row = db.execute("SELECT created_at FROM records WHERE id=? AND revision=?", root[:2]).fetchone()
    queue, documents, images, frozen_references = [root], {}, {}, []
    context_cache = {}

    def note_contexts(owner, note_key):
        if owner not in context_cache:
            owner_page = WikiPage.model_validate(store.get(*owner)["entity"])
            reachable = _reachable(_resolver(db), [ref for section in owner_page.sections for ref in section.basis]) | {owner}
            context_cache[owner] = {key for key in reachable if store.get(*key)["entity"]["kind"] == "wiki_page"}
        return sorted(context for context in context_cache[owner] if db.execute(
            "SELECT 1 FROM wiki_documents WHERE id=? AND revision=? AND target_id=? AND target_revision=?", (*context, *note_key)).fetchone())

    def filename(key):
        context = "" if not key[2] or key[2] == root[:2] else "-" + hashlib.sha256(key[2][0].encode()).hexdigest()[:8] + f"-r{key[2][1]}"
        return hashlib.sha256(key[0].encode()).hexdigest()[:24] + f"-r{key[1]}{context}.md"

    def target_of(link, context):
        pinned = re.fullmatch(r"bukan:record:([a-zA-Z0-9_.:-]+)@(\d+)(#[a-zA-Z0-9_.:-]+)?", link)
        current = re.fullmatch(r"bukan:wiki:([a-zA-Z0-9_.:-]+)(#[a-zA-Z0-9_.:-]+)?", link)
        if pinned:
            key, fragment = (pinned[1], int(pinned[2])), pinned[3] or ""
            row = db.execute("SELECT kind FROM records WHERE id=? AND revision=? AND created_at<=?", (*key, root_row[0])).fetchone()
            if not row:
                return None
            if row[0] == "paper_note" and not db.execute("SELECT 1 FROM wiki_documents WHERE id=? AND revision=? AND target_id=? AND target_revision=?", (*context, *key)).fetchone():
                choices = note_contexts(context, key)
                if len(choices) == 1:
                    context = choices[0]
            return (*key, context if row[0] in {"paper_note", "topic"} else None), fragment
        if current:
            # Exports preserve the navigation available when the root revision was
            # saved. Interactive wiki links still open current pages in the app.
            row = db.execute("SELECT revision FROM records WHERE id=? AND kind='wiki_page' AND created_at<=? ORDER BY revision DESC LIMIT 1",
                             (current[1], root_row[0])).fetchone()
            return ((current[1], row[0], None), current[2] or "") if row else None
        return None

    while queue:
        key = queue.pop()
        if key in documents:
            continue
        record = store.get(*key[:2])
        entity = ENTITY.validate_python(record["entity"])
        assets = {}
        frozen_note = False
        if isinstance(entity, WikiPage):
            markdown = db.execute("SELECT markdown FROM wiki_snapshots WHERE id=? AND revision=?", key[:2]).fetchone()[0]
            assets = dict(db.execute("SELECT name,content FROM wiki_assets WHERE id=? AND revision=?", key[:2]))
        elif entity.kind == "paper_note":
            note = db.execute("SELECT markdown FROM wiki_documents WHERE id=? AND revision=? AND target_id=? AND target_revision=?", (*(key[2] or root[:2]), *key[:2])).fetchone()
            if note is None:
                # A historical reading link is not declared evidence. Preserve its
                # stored text without silently fetching mutable image assets.
                markdown = record_markdown(record)
                choices = note_contexts(key[2] or root[:2], key[:2])
                if choices:
                    markdown = "# Adopted note snapshots\n\nThis note was captured in multiple wiki interpretations. Choose the relevant frozen figure context.\n\n" + "\n".join(
                        f"- [{store.get(*context)['entity']['title']}]({filename((*key[:2], context))})" for context in choices) + "\n\n" + markdown
                    queue.extend((*key[:2], context) for context in choices)
            else:
                markdown = note[0]
                frozen_note = True
        else:
            markdown = record_markdown(record)
        refs = references(entity)
        if not isinstance(entity, WikiPage) and refs:
            markdown += "\n## Pinned references\n\n" + "\n".join(
                f"- [{ref.id}@{ref.revision}](bukan:record:{ref.id}@{ref.revision})" for ref, _ in refs) + "\n"
        links = {}
        context = key[:2] if isinstance(entity, WikiPage) else key[2] or root[:2]
        for link in _markdown_links(markdown):
            target = target_of(link, context)
            if target:
                destination, fragment = target
                queue.append(destination)
                if destination == root:
                    links[link] = ("index.md" if key == root else "../index.md") + fragment
                else:
                    links[link] = ("references/" if key == root else "") + filename(destination) + fragment
        for original, destination in links.items():
            # Destinations can use angle brackets, titles and reference definitions.
            markdown = re.sub(r"(?P<prefix>\]\(\s*<?|\]:\s*<?)" + re.escape(original) + r"(?=[>\s)])",
                              lambda match: match["prefix"] + destination, markdown)
            markdown = markdown.replace("<" + original + ">", "[" + original + "](" + destination + ")")
        if key != root:
            markdown = markdown.replace("](assets/", "](../assets/")
        documents[key] = markdown
        if key != root and (entity.kind != "paper_note" or frozen_note):
            context = key[2]
            frozen_references.append({"id": key[0], "revision": key[1], "kind": entity.kind,
                                      "relativePath": f"references/{filename(key)}",
                                      "contextId": context[0] if context else None,
                                      "contextRevision": context[1] if context else None,
                                      "contextTitle": store.get(*context)["entity"]["title"] if context else None})
        for name, content in assets.items():
            if name in images and images[name] != content:
                raise ValueError("Conflicting immutable image names in wiki export.")
            images[name] = content
    return documents[root], {f"references/{filename(key)}": body.encode("utf-8") for key, body in documents.items() if key != root}, images, frozen_references


def export_wiki(store, identifier, revision=None):
    """Export exact stored Markdown and image bytes; never overwrite local additions."""
    record = store.get(identifier, revision)
    page = WikiPage.model_validate(record["entity"])
    revision = record["revision"]
    with store.connect() as db:
        snapshot = db.execute("SELECT markdown FROM wiki_snapshots WHERE id=? AND revision=?", (identifier, revision)).fetchone()
        if snapshot is None:
            raise ValueError("Wiki revision is missing its immutable image snapshot.")
        markdown, documents, assets, frozen_references = _portable_bundle(store, db, identifier, revision)
    key = hashlib.sha256(identifier.encode()).hexdigest()[:32]
    target = export_path(store.path.parent / "wiki" / key / f"r{revision}" / "index.md")
    files = {target.parent / "assets" / name: content for name, content in assets.items()}
    files.update({target.parent / name: content for name, content in documents.items()})
    publish_bundle(target, markdown, files)
    return {"id": identifier, "revision": revision, "markdown": markdown, "previewMarkdown": snapshot[0], "path": str(target),
            "frozenReferences": [{**item, "previewPath": str(target.parent / item["relativePath"])} for item in frozen_references],
            "editPath": str(wiki_authoring_path(store, identifier)), "export_format_version": 2}


def page_summary(record):
    page = record["entity"]
    if isinstance(page, dict):
        page = WikiPage.model_validate(page)
    return {"id": page.id, "revision": record["revision"], "kind": "wiki_page", "title": page.title,
            "summary": page.summary, "pageType": page.page_type, "parentIds": page.parent_ids,
            "categories": page.categories, "aliases": page.aliases, "updatedAt": record["created_at"]}


def _title(entity):
    return next((getattr(entity, name) for name in ("title", "text", "excerpt", "locator") if getattr(entity, name, "")), entity.id)


def _markdown_links(markdown):
    parser = MarkdownIt("commonmark")
    parser.validateLink = lambda url: True
    return [token.attrGet("href") for block in parser.parse(markdown) for token in block.children or [] if token.type == "link_open"]


def _reference_warnings(section, resolve, heads):
    reachable = _reachable(resolve, section.basis)
    scoped = _reachable(resolve, section.basis, sections=True)
    warnings = []
    for link in _markdown_links(section.markdown):
        record = re.fullmatch(r"bukan:record:([a-zA-Z0-9_.:-]+)@(\d+)(?:#([a-zA-Z0-9_.:-]+))?", link)
        navigation = re.fullmatch(r"bukan:wiki:([a-zA-Z0-9_.:-]+)(?:#([a-zA-Z0-9_.:-]+))?", link)
        if record and (record[1], int(record[2])) not in reachable:
            warnings.append({"sectionId": section.id, "reason": f"The inline reference {record[1]}@{record[2]} is not in this section's pinned basis. Check whether it is historical context or an unresolved citation change."})
        elif record and record[3] and (record[1], int(record[2]), None) not in scoped and (record[1], int(record[2]), record[3]) not in scoped:
            warnings.append({"sectionId": section.id, "reason": "The inline wiki section differs from the section fixed in the pinned basis."})
        if navigation:
            target = heads.get(navigation[1])
            if not target or not isinstance(target["entity"], WikiPage) or (navigation[2] and navigation[2] not in {item.id for item in target["entity"].sections}):
                warnings.append({"sectionId": section.id, "reason": "An inline wiki link points to a missing page or section."})
    return warnings


def work_summary(record, resolve):
    entity = record["entity"]
    if isinstance(entity, dict):
        entity = ENTITY.validate_python(entity)
    result = {"id": entity.id, "revision": record["revision"], "state": entity.state, "reason": entity.reason}
    if isinstance(entity, WikiTask):
        result.update(title=entity.purpose, purpose=entity.purpose, pageId=entity.page.id,
                      sectionId=entity.section_id, worker=entity.worker, checkpoint=entity.checkpoint)
    else:
        result.update(title=_title(resolve(entity.record)), pageIds=entity.suggested_page_ids,
                      pageId=entity.page_id, recordId=entity.record.id, recordRevision=entity.record.revision)
    return result


def wiki_home(store, query="", page_type=None, category=None, offset=0, limit=50):
    with store.connect() as db:
        heads = _heads(db)
        resolve = _resolver(db)
        def matches(page):
            if query.casefold() in json.dumps(page.model_dump(), ensure_ascii=False).casefold():
                return True
            return any(query.casefold() in json.dumps(resolve(Ref(id=key[0], revision=key[1])).model_dump(), ensure_ascii=False).casefold()
                       for key in _reachable(resolve, [ref for section in page.sections for ref in section.basis]))
        pages = [record for record in heads.values() if isinstance(record["entity"], WikiPage)]
        pages = [record for record in pages if (not page_type or record["entity"].page_type == page_type)
                 and (not category or category in record["entity"].categories)
                 and matches(record["entity"])]
        pages.sort(key=lambda record: (record["entity"].page_type != "portal", record["entity"].title, record["entity"].id))
        tasks = [work_summary(record, resolve) for record in heads.values() if isinstance(record["entity"], WikiTask)]
        candidates = [work_summary(record, resolve) for record in heads.values() if isinstance(record["entity"], WikiCandidate)]
        return {"pages": [page_summary(record) for record in pages[offset:offset + limit]],
                "hasMore": len(pages) > offset + limit, "nextOffset": offset + limit if len(pages) > offset + limit else None,
                "tasks": tasks, "candidates": candidates, "info": store.info()}


def _preview_section(frozen, page, section):
    prefix = f'<a id="{section.id}"></a>\n## {literal_text(section.title)}\n\n'
    body = frozen.split(prefix, 1)[1]
    for following in page.sections[page.sections.index(section) + 1:]:
        body = body.split(f'<a id="{following.id}"></a>\n## {literal_text(following.title)}\n\n', 1)[0]
    if section.basis:
        body = body.rsplit("\n\n### Pinned basis\n\n", 1)[0]
    return body.rstrip()


def _preview_summary(frozen, page, revision):
    return frozen.split(f"<!-- {page.id}@{revision} -->", 1)[0].removeprefix(f"# {literal_text(page.title)}\n\n").strip()


def wiki_detail(store, identifier, revision=None, *, strict_export=False):
    record = store.get(identifier, revision)
    page = WikiPage.model_validate(record["entity"])
    with store.connect() as db:
        heads = _heads(db)
        resolve = _resolver(db)
        frozen = db.execute("SELECT markdown FROM wiki_snapshots WHERE id=? AND revision=?", (identifier, record["revision"])).fetchone()[0]
        fresh = freshness(db, page)
        navigation = [{"id": target, "title": _title(heads[target]["entity"])} for target in dict.fromkeys(page.parent_ids + page.navigation)]
        backlinks = [{"id": target, "title": item["entity"].title} for target, item in heads.items()
                     if isinstance(item["entity"], WikiPage) and target != identifier and
                     (identifier in item["entity"].parent_ids + item["entity"].navigation or
                      any(ref.id == identifier for section in item["entity"].sections for ref in section.basis))]
        sections, warnings = [], []
        for section in page.sections:
            warnings.extend(_reference_warnings(section, resolve, heads))
            basis = [{"id": ref.id, "revision": ref.revision, "sectionId": ref.section_id, "relation": ref.relation,
                      "title": _title(resolve(ref))} for ref in section.basis]
            sections.append({"id": section.id, "title": section.title, "markdown": section.markdown,
                             "previewMarkdown": _preview_section(frozen, page, section),
                             "basis": basis, "interpretation": section.interpretation, "reviewStatus": section.review_status,
                             "audit": [item.model_dump() for item in section.audit]})
            if section.review_status != "independently_reviewed":
                warnings.append({"sectionId": section.id, "reason": "This section has not passed an independent review of this revision."})
            if not section.basis:
                warnings.append({"sectionId": section.id, "reason": "No pinned basis has been recorded for this section."})
        for section in fresh["sections"]:
            warnings.extend({"sectionId": section["sectionId"], "reason": reason["reason"]} for reason in section["reasons"])
    history = [{"revision": item["revision"], "createdAt": item["created_at"], "author": item["author"],
                "changeReason": item["entity"].get("change_reason", "")} for item in store.history(identifier)]
    result = {"id": identifier, "revision": record["revision"], "kind": "wiki_page", "title": page.title,
              "editable": True, "editPath": str(wiki_authoring_path(store, identifier)),
              "references": [ref.model_dump() for ref, _ in references(page)], "freshness": fresh,
              "wiki": {"summary": page.summary, "pageType": page.page_type, "parentIds": page.parent_ids,
                       "previewSummary": _preview_summary(frozen, page, record["revision"]),
                       "categories": page.categories, "aliases": page.aliases, "sections": sections,
                       "navigation": navigation, "backlinks": backlinks, "history": history,
                       "warnings": warnings, "searchThrough": page.search_through, "verifiedAt": page.verified_at}}
    try:
        preview = export_wiki(store, identifier, record["revision"])
        result.update(markdown=preview["previewMarkdown"], path=preview["path"])
        result["wiki"]["frozenReferences"] = preview["frozenReferences"]
        for section in result["wiki"]["sections"]:
            for basis in section["basis"]:
                if store.get(basis["id"], basis["revision"])["entity"]["kind"] == "paper_note":
                    key = hashlib.sha256(basis["id"].encode()).hexdigest()[:24]
                    path = Path(preview["path"]).parent / "references" / f"{key}-r{basis['revision']}.md"
                    if path.is_file():
                        basis["previewPath"] = str(path)
    except (ValueError, OSError) as error:
        if strict_export:
            raise
        result.update(markdown=page_markdown(page, record["revision"]), previewWarning=str(error))
    return result
