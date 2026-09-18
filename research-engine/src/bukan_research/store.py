"""Append-only SQLite revisions and exact source-span checks, with atomic batches."""

from collections import deque
from contextlib import closing, contextmanager
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sqlite3

from .models import ENTITY, Evidence, Relation, ReviewTask, Source, WikiPage, WikiTask, WikiCandidate, Write, references

FORMAT_VERSION = 2


def _install_writer_guards(db):
    # Version-1 clients may have passed their format check before migration took
    # the writer lock. Triggers also block these already-open legacy connections.
    for table in ("records", "heads", "links", "wiki_snapshots", "wiki_assets", "wiki_documents"):
        for operation in ("INSERT", "UPDATE", "DELETE"):
            db.execute(f"CREATE TRIGGER {table}_v2_{operation.lower()} BEFORE {operation} ON {table} "
                       "BEGIN SELECT CASE WHEN bukan_writer_version() != 2 THEN RAISE(ABORT, 'Unsupported research writer format') END; END")


def validate_store_path(path: Path) -> Path:
    path = path.expanduser().resolve()
    if any(part.casefold() == "paperpile" for part in path.parts) or any(
        (parent / "All Papers").is_dir() for parent in path.parents
    ):
        raise ValueError("The research store must be outside Paperpile.")
    for parent in (path, *path.parents):
        if any((parent / marker).is_file() for marker in ("crates/bukan/Cargo.toml", "src-tauri/Cargo.toml")):
            raise ValueError("Research data must be outside the Bukan source repository.")
        manifest = parent / ".codex-plugin" / "plugin.json"
        if manifest.is_file():
            try:
                metadata = json.loads(manifest.read_text(encoding="utf-8-sig"))
            except (OSError, ValueError):
                continue
            name = metadata.get("name") if isinstance(metadata, dict) else None
            if isinstance(name, str) and name.lower() == "bukan":
                raise ValueError("Research data must be outside the Bukan plugin distribution.")
    return path


class Store:
    def __init__(self, path: Path):
        self.path = validate_store_path(path)

    @contextmanager
    def connect(self, *, write=False):
        if not self.path.is_file():
            raise ValueError("Store not initialized; run bukan-research --store PATH init.")
        db = sqlite3.connect(self.path, timeout=10)
        db.row_factory = sqlite3.Row
        db.create_function("bukan_writer_version", 0, lambda: FORMAT_VERSION)
        db.execute("PRAGMA foreign_keys = ON")
        try:
            version = db.execute("PRAGMA user_version").fetchone()[0]
            if version not in {1, FORMAT_VERSION}:
                raise ValueError("Unsupported research store format.")
            if write and version != FORMAT_VERSION:
                raise ValueError("Research store format 1 is read-only; run migrate to back up and upgrade to format 2.")
            with db:
                db.execute("BEGIN IMMEDIATE" if write else "BEGIN")
                yield db
        finally:
            db.close()

    def initialize(self):
        if self.path.exists():
            return self.info()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        # Exclusive creation avoids silently adopting/overwriting an existing DB.
        with self.path.open("xb"):
            pass
        db = sqlite3.connect(self.path)
        try:
            db.executescript("""
                BEGIN;
                CREATE TABLE records (
                    id TEXT NOT NULL, revision INTEGER NOT NULL, kind TEXT NOT NULL,
                    body TEXT NOT NULL, digest TEXT NOT NULL, author TEXT NOT NULL,
                    created_at TEXT NOT NULL, PRIMARY KEY(id, revision)
                );
                CREATE TABLE heads (
                    id TEXT PRIMARY KEY, revision INTEGER NOT NULL,
                    FOREIGN KEY(id, revision) REFERENCES records(id, revision)
                );
                CREATE TABLE links (
                    id TEXT NOT NULL, revision INTEGER NOT NULL,
                    target_id TEXT NOT NULL, target_revision INTEGER NOT NULL,
                    PRIMARY KEY(id, revision, target_id, target_revision),
                    FOREIGN KEY(id, revision) REFERENCES records(id, revision),
                    FOREIGN KEY(target_id, target_revision) REFERENCES records(id, revision)
                );
                CREATE INDEX links_target ON links(target_id, target_revision);
                CREATE TABLE wiki_snapshots (id TEXT NOT NULL, revision INTEGER NOT NULL, markdown TEXT NOT NULL,
                    PRIMARY KEY(id,revision), FOREIGN KEY(id,revision) REFERENCES records(id,revision));
                CREATE TABLE wiki_assets (id TEXT NOT NULL, revision INTEGER NOT NULL, name TEXT NOT NULL, content BLOB NOT NULL,
                    PRIMARY KEY(id,revision,name), FOREIGN KEY(id,revision) REFERENCES records(id,revision));
                CREATE TABLE wiki_documents (id TEXT NOT NULL, revision INTEGER NOT NULL, target_id TEXT NOT NULL,
                    target_revision INTEGER NOT NULL, markdown TEXT NOT NULL, PRIMARY KEY(id,revision,target_id,target_revision),
                    FOREIGN KEY(id,revision) REFERENCES records(id,revision));
                PRAGMA user_version = 2;
                COMMIT;
            """)
            _install_writer_guards(db)
            db.commit()
        finally:
            db.close()
        return self.info()

    def status(self):
        if not self.path.is_file():
            return {"formatVersion": None, "needsMigration": False, "initialized": False}
        with closing(sqlite3.connect(f"{self.path.as_uri()}?mode=ro", uri=True)) as db:
            version = db.execute("PRAGMA user_version").fetchone()[0]
        if version not in {1, FORMAT_VERSION}:
            raise ValueError("Unsupported research store format.")
        return {"formatVersion": version, "needsMigration": version == 1, "initialized": True}

    def migrate(self):
        """Back up a consistent SQLite snapshot before the explicit format upgrade."""
        status = self.status()
        if not status["initialized"]:
            self.initialize()
            return self.status()
        if not status["needsMigration"]:
            return status
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        backup = validate_store_path(self.path.parent / "backups" / f"{self.path.stem}-v1-{stamp}.sqlite")
        backup.parent.mkdir(parents=True, exist_ok=True)
        with backup.open("xb"):
            pass
        with closing(sqlite3.connect(self.path, timeout=10)) as db, db:
            # Hold a reserved writer lock while a separate reader takes the backup.
            # The backup API includes committed WAL pages and excludes partial writes.
            db.execute("BEGIN IMMEDIATE")
            if db.execute("PRAGMA user_version").fetchone()[0] != 1:
                raise ValueError("Store format changed while migrating; reopen its status.")
            with closing(sqlite3.connect(self.path)) as source, closing(sqlite3.connect(backup)) as target:
                source.backup(target)
                if target.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                    raise ValueError("Migration backup integrity check failed.")
            db.execute("PRAGMA user_version = 2")
            db.execute("CREATE TABLE wiki_snapshots (id TEXT NOT NULL, revision INTEGER NOT NULL, markdown TEXT NOT NULL, PRIMARY KEY(id,revision), FOREIGN KEY(id,revision) REFERENCES records(id,revision))")
            db.execute("CREATE TABLE wiki_assets (id TEXT NOT NULL, revision INTEGER NOT NULL, name TEXT NOT NULL, content BLOB NOT NULL, PRIMARY KEY(id,revision,name), FOREIGN KEY(id,revision) REFERENCES records(id,revision))")
            db.execute("CREATE TABLE wiki_documents (id TEXT NOT NULL, revision INTEGER NOT NULL, target_id TEXT NOT NULL, target_revision INTEGER NOT NULL, markdown TEXT NOT NULL, PRIMARY KEY(id,revision,target_id,target_revision), FOREIGN KEY(id,revision) REFERENCES records(id,revision))")
            _install_writer_guards(db)
        return {**self.status(), "backupPath": str(backup)}

    @staticmethod
    def _get(db, record_id, revision=None):
        row = db.execute(
            "SELECT r.* FROM records r JOIN heads h ON h.id=r.id "
            "WHERE r.id=? AND r.revision=COALESCE(?,h.revision)",
            (record_id, revision),
        ).fetchone()
        if row is None:
            raise ValueError(f"Record not found: {record_id}@{revision or 'latest'}")
        return row

    @staticmethod
    def _decode(row):
        return {"entity": json.loads(row["body"]), "revision": row["revision"],
                "author": row["author"], "created_at": row["created_at"],
                "sha256": row["digest"], "semantic_review": "not_verified_by_store"}

    def put(self, writes: list[Write], author="ai-client"):
        if not 1 <= len(writes) <= 200:
            raise ValueError("A batch must contain 1 to 200 records.")
        if len({item.entity.id for item in writes}) != len(writes):
            raise ValueError("Duplicate IDs within a batch.")
        with self.connect(write=True) as db:
            pending, previous, unchanged_tasks, results = {}, {}, [], []
            for item in writes:
                entity = item.entity
                body = json.dumps(entity.model_dump(), ensure_ascii=False, sort_keys=True)
                digest = hashlib.sha256(body.encode("utf-8")).hexdigest()
                head = db.execute("SELECT revision FROM heads WHERE id=?", (entity.id,)).fetchone()
                revision = head[0] if head else 0
                old = self._get(db, entity.id) if head else None
                if isinstance(entity, (WikiPage, WikiTask, WikiCandidate)) and item.expected_revision != revision:
                    raise ValueError(f"Revision conflict: {entity.id}; expected {item.expected_revision}, current {revision}")
                if isinstance(entity, WikiPage):
                    from .wiki import prepare_page
                    entity = prepare_page(entity, ENTITY.validate_json(old["body"]) if old else None)
                    body = json.dumps(entity.model_dump(), ensure_ascii=False, sort_keys=True)
                    digest = hashlib.sha256(body.encode("utf-8")).hexdigest()
                if old and old["digest"] == digest:
                    if isinstance(entity, ReviewTask):
                        unchanged_tasks.append(entity)
                    results.append({"id": entity.id, "revision": revision, "status": "unchanged"})
                    continue
                if item.expected_revision != revision:
                    raise ValueError(f"Revision conflict: {entity.id}; expected {item.expected_revision}, current {revision}")
                if old and (old["kind"] != entity.kind or entity.kind in {"source", "evidence"}):
                    raise ValueError("Kinds, source captures and evidence spans are immutable; use a new ID.")
                revision += 1
                pending[(entity.id, revision)] = (entity, body, digest)
                if isinstance(entity, ReviewTask):
                    previous[entity.id] = ENTITY.validate_json(old["body"]) if old else None
                results.append({"id": entity.id, "revision": revision, "status": "created" if not old else "updated"})

            def resolve(ref):
                key = (ref.id, ref.revision)
                return pending[key][0] if key in pending else ENTITY.validate_json(
                    self._get(db, ref.id, ref.revision)["body"])

            for entity, _, _ in pending.values():
                for ref, kinds in references(entity):
                    if resolve(ref).kind not in kinds:
                        raise ValueError(f"Wrong reference kind: {entity.id} -> {ref.id}; expected {sorted(kinds)}")
                if isinstance(entity, Evidence):
                    source = resolve(entity.source)
                    if not entity.start < entity.end <= len(source.text) or source.text[entity.start:entity.end] != entity.excerpt:
                        raise ValueError(f"Excerpt does not match captured source: {entity.id}")
                if isinstance(entity, Relation):
                    if entity.source_claim.id == entity.target_claim.id:
                        raise ValueError("A relation needs two distinct claims.")
                    if entity.relation_type == "extends" and not (entity.retained.strip() and entity.changed.strip()):
                        raise ValueError("An extends relation must state what was retained and changed.")
                if isinstance(entity, ReviewTask):
                    self._validate_review_task(db, entity, previous[entity.id], pending, resolve)
                if isinstance(entity, (WikiPage, WikiTask, WikiCandidate)):
                    from .wiki import validate_wiki_entity
                    validate_wiki_entity(entity, resolve, db, pending)
            for task in unchanged_tasks:
                self._validate_review_task(db, task, task, pending, resolve, replay=True)

            now = datetime.now(timezone.utc).isoformat()
            for (record_id, revision), (entity, body, digest) in pending.items():
                db.execute("INSERT INTO records VALUES (?,?,?,?,?,?,?)",
                           (record_id, revision, entity.kind, body, digest, author, now))
                db.execute("INSERT INTO heads VALUES (?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision",
                           (record_id, revision))
            for (record_id, revision), (entity, _, _) in pending.items():
                for ref, _ in references(entity):
                    db.execute("INSERT OR IGNORE INTO links VALUES (?,?,?,?)",
                               (record_id, revision, ref.id, ref.revision))
                if isinstance(entity, WikiPage):
                    from .wiki import snapshot_page
                    snapshot_page(self, db, entity, revision)
            if any(entity.kind in {"paper", "source", "evidence", "claim", "relation", "question", "topic", "paper_note", "wiki_page"}
                   for entity, _, _ in pending.values()):
                from .wiki import sync_work
                sync_work(db, now)
            return results

    def _validate_review_task(self, db, task, old, pending, resolve, *, replay=False):
        if old is None:
            if task.state != "pending":
                raise ValueError("Create a pending review task before claiming it.")
        elif not replay:
            if any(getattr(old, field) != getattr(task, field) for field in
                   ("paper", "asset_sha256", "page_count", "note_id")):
                raise ValueError("A review task's paper, PDF hash, page count and note ID are immutable.")
            allowed = {"pending": {"pending", "running", "needs_followup"},
                       "running": {"running", "completed", "needs_followup"},
                       "completed": {"pending"}, "needs_followup": {"pending"}}
            if task.state not in allowed[old.state]:
                raise ValueError(f"Invalid review task transition: {old.state} -> {task.state}")
            if task.state != "pending" and task.note_revision != old.note_revision:
                raise ValueError("A claimed task must keep its original note revision.")
            if old.state == "running" and task.worker != old.worker:
                raise ValueError("A claimed task must keep its assigned worker.")

        head = db.execute("SELECT revision FROM heads WHERE id=?", (task.note_id,)).fetchone()
        note_revision = head[0] if head else 0
        next_note = pending.get((task.note_id, note_revision + 1))
        resulting_revision = note_revision + int(next_note is not None)
        baseline = note_revision if task.state == "completed" else resulting_revision
        if (task.state in {"pending", "running", "completed"} and task.note_revision != baseline
                and not (replay and task.state == "completed")):
            raise ValueError("Review note changed; preserve the new note and replan before claiming or completing.")
        if (head or next_note) and task.state != "needs_followup":
            note = next_note[0] if next_note else ENTITY.validate_json(self._get(db, task.note_id)["body"])
            if note.kind != "paper_note" or resolve(note.source).paper.id != task.paper.id:
                raise ValueError("Review destination must be a note for the assigned paper.")

        if task.state == "running":
            # Serialize claims for different PDF versions that share one destination note.
            active = db.execute(
                "SELECT r.id FROM records r JOIN heads h ON r.id=h.id AND r.revision=h.revision "
                "WHERE r.kind='review_task' AND json_extract(r.body,'$.note_id')=? "
                "AND json_extract(r.body,'$.state')='running'", (task.note_id,)).fetchall()
            updated = {entity.id: entity for entity, _, _ in pending.values()
                       if isinstance(entity, ReviewTask)}
            other_ids = {row[0] for row in active if row[0] not in updated}
            other_ids.update(entity.id for entity in updated.values()
                             if entity.note_id == task.note_id and entity.state == "running")
            if other_ids - {task.id}:
                raise ValueError("Another worker already owns this review note.")

        if task.state == "completed":
            note = resolve(task.result_note)
            source = resolve(note.source)
            if (note.id != task.note_id or task.result_note.revision != resulting_revision
                    or source.paper.id != task.paper.id or source.asset_sha256 != task.asset_sha256
                    or source.source_type != "body" or note.page_count != task.page_count
                    or note.reading_status != "full_text_reviewed"):
                raise ValueError("Completed review must reference the current fully read body note for the assigned paper, PDF hash and page count.")

    def review_snapshot(self, record_ids):
        """Read only requested heads and pinned source metadata in one consistent snapshot."""
        placeholders = ",".join("?" for _ in record_ids)
        with self.connect() as db:
            rows = db.execute(
                "SELECT r.id,r.revision,json_remove(r.body,'$.markdown','$.text') AS body,"
                "json_extract(s.body,'$.paper.id') AS source_paper,"
                "json_extract(s.body,'$.asset_sha256') AS source_hash,"
                "json_extract(s.body,'$.source_type') AS source_type "
                "FROM heads h JOIN records r ON r.id=h.id AND r.revision=h.revision "
                "LEFT JOIN records s ON r.kind='paper_note' AND s.id=json_extract(r.body,'$.source.id') "
                "AND s.revision=json_extract(r.body,'$.source.revision') "
                f"WHERE r.id IN ({placeholders})", list(record_ids)).fetchall()
            return {row["id"]: {"entity": json.loads(row["body"]), "revision": row["revision"],
                                "source_paper": row["source_paper"], "source_hash": row["source_hash"],
                                "source_type": row["source_type"]} for row in rows}

    def get(self, record_id, revision=None):
        with self.connect() as db:
            return self._decode(self._get(db, record_id, revision))

    def search(self, query="", kind=None, limit=50, offset=0):
        if not 1 <= limit <= 200 or offset < 0:
            raise ValueError("limit must be 1..200 and offset must be nonnegative.")
        with self.connect() as db:
            rows = db.execute(
                "SELECT r.* FROM records r JOIN heads h ON r.id=h.id AND r.revision=h.revision "
                "WHERE (? IS NULL OR r.kind=?) AND instr(lower(r.body),lower(?))>0 "
                "ORDER BY r.kind,r.id LIMIT ? OFFSET ?", (kind, kind, query, limit + 1, offset)
            ).fetchall()
            return {"items": [self._decode(row) for row in rows[:limit]],
                    "next_offset": offset + limit if len(rows) > limit else None}

    def history(self, record_id):
        with self.connect() as db:
            rows = db.execute("SELECT * FROM records WHERE id=? ORDER BY revision", (record_id,)).fetchall()
            return [self._decode(row) for row in rows]

    def trace(self, record_id, revision=None, depth=5, limit=100):
        """Follow pinned provenance outwards, never replacing it with newer revisions."""
        if not 0 <= depth <= 8 or not 1 <= limit <= 200:
            raise ValueError("depth must be 0..8 and limit must be 1..200.")
        with self.connect() as db:
            first = self._get(db, record_id, revision)
            queue = deque([(first["id"], first["revision"], 0)])
            seen, nodes, edges = set(), [], []
            truncated = False
            while queue:
                current_id, current_revision, distance = queue.popleft()
                key = (current_id, current_revision)
                if key in seen:
                    continue
                if len(nodes) >= limit:
                    truncated = True
                    break
                seen.add(key)
                node = self._decode(self._get(db, *key))
                node["latest_revision"] = self._get(db, current_id)["revision"]
                nodes.append(node)
                links = db.execute("SELECT target_id,target_revision FROM links WHERE id=? AND revision=?", key).fetchall()
                if distance == depth:
                    truncated |= bool(links)
                    continue
                for link in links:
                    edges.append({"from": {"id": current_id, "revision": current_revision},
                                  "to": {"id": link[0], "revision": link[1]}})
                    queue.append((link[0], link[1], distance + 1))
            return {"nodes": nodes, "edges": edges, "truncated": truncated}

    def info(self):
        with self.connect() as db:
            counts = dict(db.execute("SELECT r.kind,count(*) FROM records r JOIN heads h "
                                     "ON r.id=h.id AND r.revision=h.revision GROUP BY r.kind").fetchall())
            return {"schema_version": db.execute("PRAGMA user_version").fetchone()[0], "counts": counts,
                    "revisions": db.execute("SELECT count(*) FROM records").fetchone()[0]}

    def export(self):
        with self.connect() as db:
            result = {"schema_version": db.execute("PRAGMA user_version").fetchone()[0],
                    "records": [self._decode(row) for row in db.execute(
                        "SELECT * FROM records ORDER BY id,revision")],
                    "heads": dict(db.execute("SELECT id,revision FROM heads").fetchall())}
            if db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='public_access_reports'").fetchone():
                result["public_access"] = {"format_version": 1, "reports": [
                    {"report": json.loads(row["body"]), "revision": row["revision"], "created_at": row["created_at"]}
                    for row in db.execute("SELECT * FROM public_access_reports ORDER BY id,revision")]}
            return result


class TransactionStore(Store):
    """Borrow an open Store connection; its caller owns commit, rollback and close.

    Nested Store operations see uncommitted revisions without opening another
    connection or committing the surrounding transaction.
    """

    def __init__(self, store, db):
        self.path = store.path
        self._db = db

    @contextmanager
    def connect(self, *, write=False):
        yield self._db
