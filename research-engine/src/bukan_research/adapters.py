"""Translate exported library records; Rust remains responsible for library scanning."""

from uuid import NAMESPACE_URL, uuid5

from .models import ExternalRef, Paper


def paper_from_bukan(record: dict) -> Paper:
    external_id = record["id"]
    return Paper(
        id="paper-" + uuid5(NAMESPACE_URL, "bukan:" + external_id).hex,
        title=record["title"], authors=record.get("authors") or "", year=record.get("year"),
        external_refs=[ExternalRef(provider="bukan", external_id=external_id,
                                   uri=record.get("path", ""))],
    )
