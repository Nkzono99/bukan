"""Publish immutable Markdown bundles without overwriting local edits."""

from .store import validate_store_path


def export_path(path):
    target = validate_store_path(path)
    if target != path:
        raise ValueError(f"Export path must not contain symlinks: {path}")
    return target


def _check_export(path, content):
    if not path.is_file() or path.read_bytes() != content:
        raise ValueError(f"Export contains local edits; preserve them and update the note as a new revision: {path}")


def publish_bundle(target, markdown, files):
    """Preflight every file, then publish assets/references before the root document."""
    files = {export_path(path): content for path, content in files.items()}
    files[export_path(target)] = markdown.encode("utf-8")
    published = target.exists()
    for path, content in files.items():
        if path.exists():
            _check_export(path, content)
        elif published:
            raise ValueError(f"Export contains local edits (missing file): {path}")
    for path, content in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        try:
            with path.open("xb") as output:
                output.write(content)
        except FileExistsError:
            _check_export(path, content)
