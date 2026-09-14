"""Bundle local Markdown images without reformatting the surrounding source."""

import hashlib
from html.parser import HTMLParser
from pathlib import Path
import re
from urllib.parse import quote, unquote, urlsplit

from markdown_it import MarkdownIt
from markdown_it.rules_inline import image as parse_image

from .store import validate_store_path


class _HTMLImages(HTMLParser):
    def handle_starttag(self, tag, attrs):
        if tag == "img":
            raise ValueError("HTML images cannot be bundled; use inline Markdown images.")


def _image_with_position(state, silent):
    start = state.pos
    if not parse_image(state, silent):
        return False
    if not silent:
        token = state.tokens[-1]
        label_end = state.md.helpers.parseLinkLabel(state, start + 1, False)
        pos = label_end + 1
        if state.src[pos:pos + 1] == "(":
            pos += 1
            while state.src[pos:pos + 1] in (" ", "\t", "\n"):
                pos += 1
            destination = state.md.helpers.parseLinkDestination(state.src, pos, state.posMax)
            token.meta["destination"] = (pos, destination.pos)
    return True


def _source_position(token, position, lines, line_offsets):
    """Map an inline parser position past Markdown list/quote indentation."""
    consumed = 0
    for line_number, inline_line in enumerate(token.content.split("\n"), token.map[0]):
        if line_number >= token.map[1]:
            break
        if position <= consumed + len(inline_line):
            # The block parser can partially expand a tab in list indentation.
            # Match only this source line, then account for its parsed prefix.
            unindented = inline_line.lstrip(" \t")
            column = lines[line_number].replace("\0", "\ufffd").find(unindented)
            relative = position - consumed - (len(inline_line) - len(unindented))
            if column >= 0 and relative >= 0:
                return line_offsets[line_number] + column + relative
            break
        consumed += len(inline_line) + 1
    raise ValueError("Cannot locate image in Markdown source; use a simple inline image.")


def bundle_images(markdown: str, legacy_parent: Path, asset_root: Path):
    """Resolve DB image paths from the legacy rN.md directory, never from rN/."""
    parser = MarkdownIt("commonmark").enable("table")
    # Parse even unsupported URL schemes so they fail explicitly below.
    parser.validateLink = lambda url: True
    parser.inline.ruler.at("image", _image_with_position)
    tokens = parser.parse(markdown)
    # CommonMark normalizes only CRLF, CR and LF, not Unicode line separators.
    lines = re.split(r"\r\n?|\n", markdown)
    line_offsets = [0, *(newline.end() for newline in re.finditer(r"\r\n?|\n", markdown))]
    assets = {}
    replacements = []
    in_table = False
    for block in tokens:
        if block.type == "table_open":
            in_table = True
        elif block.type == "table_close":
            in_table = False
        if block.type == "html_block":
            _HTMLImages().feed(block.content)
        if block.type != "inline":
            continue
        for token in block.children or []:
            if token.type == "html_inline":
                _HTMLImages().feed(token.content)
            if token.type != "image":
                continue
            url = urlsplit(token.attrGet("src") or "")
            if url.scheme.lower() in ("http", "https"):
                continue  # External images stay external; exporting never fetches them.
            if in_table:
                raise ValueError("Local images inside Markdown tables cannot be bundled; embed the image below the table.")
            if url.scheme or url.netloc or url.query or not url.path:
                raise ValueError("Unsupported local image URL; use a relative note-assets path.")
            if "destination" not in token.meta:
                raise ValueError("Reference-style local images cannot be bundled; use inline Markdown images.")
            if validate_store_path(asset_root) != asset_root:
                raise ValueError("The note-assets directory must not be a symlink.")
            source = validate_store_path(legacy_parent / unquote(url.path))
            if not source.is_relative_to(asset_root):
                raise ValueError(f"Local image must be inside the store's note-assets directory: {source}")
            if not source.is_file():
                raise ValueError(f"Local image is missing: {source}")
            content = source.read_bytes()
            filename = hashlib.sha256(content).hexdigest()[:32] + source.suffix.lower()
            assets[filename] = content
            destination = "assets/" + quote(filename)
            if url.fragment:
                destination += "#" + url.fragment
            start, end = token.meta["destination"]
            replacements.append((_source_position(block, start, lines, line_offsets),
                                 _source_position(block, end, lines, line_offsets), destination))
    for start, end, destination in sorted(replacements, reverse=True):
        markdown = markdown[:start] + destination + markdown[end:]
    return markdown, assets
