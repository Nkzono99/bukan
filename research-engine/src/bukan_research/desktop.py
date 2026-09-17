"""Compatibility entry point for the former desktop JSON bridge.

New clients should use :mod:`bukan_research.workspace_api`.
"""

from .store import Store
from .workspace_api import handle_request as _handle_request


def handle_request(store: Store, payload, *, author="desktop-user"):
    return _handle_request(store, payload, author=author)
