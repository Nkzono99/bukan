import json
import socket
from types import SimpleNamespace

import pytest

from bukan_research import public_access as access
from bukan_research.models import Paper, Write
from bukan_research.store import Store


@pytest.fixture
def store(tmp_path):
    value = Store(tmp_path / "research.sqlite")
    value.initialize()
    return value


def report(**kwargs):
    return access.AccessReport(id="access-test", paper=access.PaperQuery(doi="10.1234/test"), **kwargs)


def candidate(**kwargs):
    return access.PublicCandidate(url="https://example.org/paper.pdf", provider="external-plugin",
        source_url="https://example.org/record", **kwargs)


def crossref():
    return {"DOI": "10.1234/test", "title": ["Dust transport"], "URL": "https://doi.org/10.1234/test",
            "issued": {"date-parts": [[2025]]}, "author": [{"family": "Example"}],
            "license": [{"URL": "https://creativecommons.org/licenses/by/4.0/", "content-version": "vor"}],
            "link": [{"URL": "https://example.org/paper.pdf", "content-version": "vor"}],
            "relation": {"has-preprint": [{"id-type": "doi", "id": "10.1234/preprint"}]}}


def openalex():
    return {"id": "https://openalex.org/W123", "doi": "https://doi.org/10.1234/test", "display_name": "Dust transport",
            "publication_year": 2025, "locations": [
                {"landing_page_url": "https://example.org/publisher", "pdf_url": None, "is_oa": False, "version": "publishedVersion"},
                {"landing_page_url": "https://example.org/repository", "pdf_url": "https://example.org/accepted.pdf",
                 "is_oa": True, "license": None, "version": "acceptedVersion"},
                {"landing_page_url": "https://example.org/preprint", "pdf_url": None,
                 "is_oa": True, "license": "cc-by", "version": "submittedVersion"}]}


def test_doi_discovery_separates_metadata_license_and_actual_access(store, monkeypatch):
    seen = []

    def fetch(url, **kwargs):
        seen.append(url)
        data = {"message": crossref()} if "crossref" in url else openalex()
        return 200, {}, json.dumps(data).encode(), url

    monkeypatch.setattr(access, "fetch", fetch)
    result = access.find_public_versions(store, [access.PaperQuery(doi="https://doi.org/10.1234/TEST")])
    value = result["reports"][0]
    assert len(seen) == 2 and not result["saved"]
    assert access.search_public_access(store)["items"] == []
    assert value["paper"]["doi"] == "10.1234/test"
    copies = value["candidates"]
    assert all(c["check"] is None for c in copies)
    pdf = next(c for c in copies if c["url"] == "https://example.org/paper.pdf")
    assert pdf["free_to_read"] is None and pdf["license_status"] == "reported"
    accepted = next(c for c in copies if c["url"] == "https://example.org/accepted.pdf")
    assert accepted["version"] == "acceptedVersion" and accepted["free_to_read"] is True
    assert accepted["license_status"] == "unknown"
    preprint = next(c for c in copies if c["url"] == "https://doi.org/10.1234/preprint")
    assert preprint["identifiers"]["doi"] == "10.1234/preprint" and preprint["match"] == "external_candidate"


def test_title_matches_stay_provisional_and_provider_failure_keeps_other_candidates(store, monkeypatch):
    def fetch(url, **kwargs):
        if "openalex" in url:
            return 403, {}, b"", url
        assert "Example" in url and "2025" in url
        return 200, {}, json.dumps({"message": {"items": [crossref()]}}).encode(), url

    monkeypatch.setattr(access, "fetch", fetch)
    result = access.find_public_versions(store, [access.PaperQuery(title="Dust transport", authors="Example", year=2025)])
    value = result["reports"][0]
    assert value["candidates"] and not any(c["match"] == "exact_doi" for c in value["candidates"])
    assert value["searches"][1]["outcome"] == "error"
    assert "403" in value["searches"][1]["detail"]


def test_unresolved_library_id_does_not_stop_other_papers(store, monkeypatch):
    monkeypatch.setattr(access, "_native_library", lambda: [])
    monkeypatch.setattr(access, "fetch", lambda url, **kwargs: (404, {}, b"", url))
    result = access.find_public_versions(store, [access.PaperQuery(paper_id="missing"), access.PaperQuery(doi="10.1234/absent")])
    assert result["reports"][0]["searches"][0]["outcome"] == "error"
    assert all(s["outcome"] == "not_found_in_this_search" for s in result["reports"][1]["searches"])


def test_legacy_invalid_doi_does_not_abort_valid_papers_in_same_batch(store, monkeypatch):
    store.put([Write(entity=Paper(id="legacy-paper", title="Legacy metadata", doi="not-a-doi"))])
    monkeypatch.setattr(access, "fetch", lambda url, **kwargs: (404, {}, b"", url))
    result = access.find_public_versions(store, [access.PaperQuery(paper_id="legacy-paper"), access.PaperQuery(doi="10.1234/valid")])
    assert result["reports"][0]["searches"][0]["outcome"] == "error"
    assert all(s["outcome"] == "not_found_in_this_search" for s in result["reports"][1]["searches"])


def test_bukan_ids_reuse_imports_and_native_index_once_without_scanning_pdfs(store, monkeypatch):
    store.put([Write(entity=Paper(id="paper-imported", title="Imported", doi="10.1234/test",
        external_refs=[{"provider": "bukan", "external_id": "p2-imported"}]))])
    calls = []

    def library():
        calls.append(1)
        return [{"id": "p2-native", "legacyId": "legacy-native", "title": "Native", "authors": "Author", "year": 2024}]

    monkeypatch.setattr(access, "_native_library", library)
    values = access.resolve_papers(store, [access.PaperQuery(paper_id="p2-imported"),
        access.PaperQuery(paper_id="p2-native"), access.PaperQuery(paper_id="legacy-native")])
    assert len(calls) == 1
    assert values[0].doi == "10.1234/test" and values[1].title == values[2].title == "Native"


def test_native_bridge_uses_explicit_workspace_and_safe_argv(monkeypatch):
    monkeypatch.setenv("BUKAN_EXECUTABLE", "C:/Tools O'Brien/bukan.exe")
    monkeypatch.setenv("BUKAN_WORKSPACE", "C:/Research 研究/Topic")
    seen = []

    def run(argv, **kwargs):
        seen.append(argv)
        return SimpleNamespace(returncode=0, stdout='{"papers": []}')

    monkeypatch.setattr(access.subprocess, "run", run)
    assert access._native_library() == []
    assert seen == [["C:/Tools O'Brien/bukan.exe", "scan", "C:/Research 研究/Topic", "--json"]]


def test_mixed_batch_never_replaces_curated_metadata_with_native_filename_metadata(store, monkeypatch):
    store.put([Write(entity=Paper(id="curated", title="Corrected title", doi="10.1234/test",
        external_refs=[{"provider": "bukan", "external_id": "p2-curated"}]))])
    query = access.PaperQuery(paper_id="p2-curated")
    alone = access.resolve_papers(store, [query])[0]
    monkeypatch.setattr(access, "_native_library", lambda: [
        {"id": "p2-curated", "title": "Old filename title"}, {"id": "p2-other", "title": "Other"}])
    mixed = access.resolve_papers(store, [query, access.PaperQuery(paper_id="p2-other")])
    assert mixed[0] == alone
    assert mixed[0].title == "Corrected title" and mixed[0].doi == "10.1234/test"


def test_access_checks_do_not_equate_html_200_or_403_with_oa(monkeypatch):
    def fetch(url, **kwargs):
        if url.endswith("timeout"):
            raise TimeoutError
        status = int(url.rsplit("/", 1)[1])
        return status, {"content-type": "text/html"}, b"<html>Login required</html>", url

    monkeypatch.setattr(access, "fetch", fetch)
    result = access.check_public_urls(["https://example.org/200", "https://example.org/403",
                                      "https://example.org/404", "https://example.org/timeout"])
    assert [c["status"] for c in result["checks"]] == ["reachable", "blocked", "not_found", "timeout"]
    assert all(not c["pdf_signature"] for c in result["checks"])
    assert all("free_to_read" not in c for c in result["checks"])


@pytest.mark.parametrize("url", ["file:///tmp/test.pdf", "https://user:password@example.org/", "https://example.org:8080/", "https://example.org/\r\nInjected"])
def test_non_public_url_forms_are_rejected(url):
    assert access.check_public_urls([url])["checks"][0]["status"] == "unsafe_url"


@pytest.mark.parametrize("address", ["127.0.0.1", "10.0.0.5", "169.254.169.254", "::1"])
def test_private_dns_targets_rejected_before_connect(monkeypatch, address):
    monkeypatch.setattr(access.socket, "getaddrinfo", lambda *args, **kwargs: [(socket.AF_INET, socket.SOCK_STREAM, 6, "", (address, 80))])
    monkeypatch.setattr(access.socket, "socket", lambda *args: pytest.fail("must not connect"))
    with pytest.raises(ValueError, match="non-public"):
        access._public_socket("example.org", 80, 1)


def test_socket_connects_to_validated_address_without_resolving_again(monkeypatch):
    connected = []
    monkeypatch.setattr(access.socket, "getaddrinfo", lambda *args, **kwargs: [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("93.184.216.34", 80))])
    sock = SimpleNamespace(settimeout=lambda value: None, connect=connected.append)
    monkeypatch.setattr(access.socket, "socket", lambda *args: sock)
    assert access._public_socket("example.org", 80, 1) is sock
    assert connected == [("93.184.216.34", 80)]


def test_redirect_to_private_network_is_rejected(monkeypatch):
    calls = []

    def connect(host, *args):
        calls.append(host)
        if host == "127.0.0.1":
            raise ValueError("Private target")
        return object()

    response = SimpleNamespace(status=302, getheaders=lambda: [("Location", "http://127.0.0.1/admin")])
    connection = SimpleNamespace(request=lambda *args, **kwargs: None, getresponse=lambda: response, close=lambda: None)
    monkeypatch.setattr(access, "_public_socket", connect)
    monkeypatch.setattr(access.http.client, "HTTPConnection", lambda *args, **kwargs: connection)
    result = access.check_public_urls(["http://example.org/start"])
    assert result["checks"][0]["status"] == "unsafe_url"
    assert calls == ["example.org", "127.0.0.1"]


def test_save_history_conflicts_partial_external_candidates_and_export(store):
    value = report(candidates=[candidate()])
    assert access.save_public_access(store, value)["revision"] == 1
    old = access.get_public_access(store, value.id)
    assert "external-plugin" in old["markdown"] and "not_checked" in old["markdown"]
    value.candidates[0].note = "Corrected after external search"
    assert access.save_public_access(store, value, 1)["revision"] == 2
    assert access.get_public_access(store, value.id, 1)["report"]["candidates"][0]["note"] == ""
    with pytest.raises(ValueError, match="Revision conflict"):
        access.save_public_access(store, value, 1)
    assert access.save_public_access(store, value, 2)["status"] == "unchanged"
    assert access.search_public_access(store, "10.1234/test")["items"][0]["revision"] == 2
    assert len(store.export()["public_access"]["reports"]) == 2
    assert store.info()["counts"] == {}  # No fabricated scientific claims/reading records.


def test_unsaved_searches_are_read_only_and_pagination_uses_current_reports(store):
    before = store.path.read_bytes()
    assert access.search_public_access(store)["items"] == []
    with pytest.raises(ValueError, match="not found"):
        access.get_public_access(store, "missing")
    assert store.path.read_bytes() == before
    for i in range(3):
        value = report()
        value.id = f"access-{i}"
        access.save_public_access(store, value)
    page = access.search_public_access(store, limit=2)
    assert len(page["items"]) == 2 and page["next_offset"] == 2
    assert len(access.search_public_access(store, limit=2, offset=2)["items"]) == 1


def test_checks_cannot_be_attached_to_another_url():
    with pytest.raises(ValueError, match="belong"):
        candidate(check=access.AccessCheck(url="https://example.org/other", status="reachable"))


def test_optional_access_table_does_not_bypass_legacy_store_migration(store):
    with store.connect() as db:
        db.execute("PRAGMA user_version = 1")
    assert access.search_public_access(store)["items"] == []
    with pytest.raises(ValueError, match="read-only"):
        access.save_public_access(store, report())


def test_cli_request_and_validation_share_mcp_operations(store):
    saved = access.handle_request(store, {"operation": "save", "report": report().model_dump()})
    assert saved["revision"] == 1
    assert access.handle_request(store, {"operation": "get", "record_id": "access-test"})["revision"] == 1
    for request in ({"operation": []}, {"operation": "find", "papers": None}, {"operation": "get", "typo": True}):
        with pytest.raises(ValueError):
            access.handle_request(store, request)


def test_markdown_escapes_untrusted_titles_and_urls(store):
    value = report()
    value.paper.title = "<img src=x onerror=alert(1)> | Title"
    value.candidates = [candidate()]
    value.candidates[0].url = "https://example.org/<bad>)"
    access.save_public_access(store, value)
    markdown = access.get_public_access(store, value.id)["markdown"]
    assert "<img" not in markdown and "%3Cbad%3E%29" in markdown
