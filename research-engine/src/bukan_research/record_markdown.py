"""Render stored records as Markdown without interpreting source prose as markup."""

import html
import json
import re


def record_title(entity):
    text = next((entity[key] for key in ("title", "text", "excerpt", "locator", "note_id")
                 if entity.get(key)), entity["id"])
    return " ".join(text.split())[:180]


def literal_text(text):
    """Display stored prose as text, without creating links or embedded HTML."""
    return re.sub(r"([\\`*_{}\[\]()#+.!|>~-])", r"\\\1", html.escape(str(text), quote=False))


def record_markdown(record):
    entity = record["entity"]
    lines = [f"# {literal_text(record_title(entity))}", "",
             f"- Record: `{entity['id']}@{record['revision']}`",
             f"- Kind: `{entity['kind']}`",
             f"- Updated: {literal_text(record['created_at'])}",
             f"- Author: {literal_text(record['author'])}",
             "- Scientific interpretation: not independently verified by the store", ""]
    for key, value in entity.items():
        if key in {"id", "kind"} or value is None or value == "" or value == []:
            continue
        lines += [f"## {literal_text(key.replace('_', ' ').capitalize())}", ""]
        if isinstance(value, (dict, list)) or key in {"text", "excerpt"} and entity["kind"] in {"source", "evidence"}:
            text = json.dumps(value, ensure_ascii=False, indent=2) if isinstance(value, (dict, list)) else value
            # Longer fences preserve even captured source containing Markdown.
            fence = "`" * max(3, max((len(run) + 1 for run in re.findall(r"`+", text)), default=0))
            lines += [fence, text, fence, ""]
        else:
            lines += [literal_text(value), ""]
    return "\n".join(lines).rstrip() + "\n"
