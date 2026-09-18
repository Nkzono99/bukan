"""Public-copy discovery and revisioned access metadata, independent of PDF review."""

from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import html
import http.client
import ipaddress
import json
import os
import re
import socket
import ssl
import subprocess
from typing import Literal
from urllib.parse import quote, urlencode, urljoin, urlsplit

from pydantic import Field, field_validator, model_validator

from .models import Identifier, Model
from .store import Store


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def http_url(value: str) -> str:
    parts = urlsplit(value)
    if (parts.scheme not in {"http", "https"} or not parts.hostname or parts.username
            or parts.password or any(ord(c) < 32 for c in value)):
        raise ValueError("Use an HTTP(S) URL without credentials or control characters.")
    if parts.port not in {None, 80, 443}:
        raise ValueError("Public links must use standard HTTP(S) ports.")
    return value


class PaperQuery(Model):
    paper_id: str = Field(default="", max_length=500)
    doi: str = Field(default="", max_length=500)
    title: str = Field(default="", max_length=2000)
    authors: str = Field(default="", max_length=2000)
    year: int | None = Field(default=None, ge=1000, le=3000)

    @field_validator("doi")
    @classmethod
    def normalize_doi(cls, value):
        value = re.sub(r"^(?:https?://(?:dx\.)?doi\.org/|doi:\s*)", "", value.strip(), flags=re.I)
        if value and not re.fullmatch(r"10\.\d{4,9}/\S+", value):
            raise ValueError("Invalid DOI; provide the DOI or a doi.org URL.")
        return value.lower()

    @model_validator(mode="after")
    def identifiable(self):
        if not (self.paper_id.strip() or self.doi or self.title.strip()):
            raise ValueError("Provide paper_id, doi, or title.")
        return self


class AccessCheck(Model):
    url: str
    status: Literal["reachable", "blocked", "not_found", "http_error", "timeout", "network_error", "unsafe_url"]
    checked_at: str = Field(default_factory=now)
    final_url: str = ""
    http_status: int | None = None
    content_type: str = ""
    pdf_signature: bool = False
    detail: str = ""


class PublicCandidate(Model):
    url: str = Field(max_length=8000)
    role: Literal["landing_page", "full_text", "unknown"] = "unknown"
    version: Literal["publishedVersion", "acceptedVersion", "submittedVersion", "unknown"] = "unknown"
    title: str = ""
    authors: str = ""
    year: int | None = None
    identifiers: dict[str, str] = Field(default_factory=dict)
    match: Literal["exact_doi", "bibliographic_candidate", "external_candidate", "confirmed"] = "external_candidate"
    free_to_read: bool | None = None
    license: str = ""
    license_status: Literal["unknown", "reported", "verified"] = "unknown"
    license_source_url: str = ""
    provider: str = Field(min_length=1, max_length=200)
    source_url: str = Field(min_length=1, max_length=8000)
    discovered_at: str = Field(default_factory=now)
    check: AccessCheck | None = None
    note: str = ""

    _url = field_validator("url", "source_url")(http_url)

    @model_validator(mode="after")
    def consistent_check(self):
        if self.check and self.check.url != self.url:
            raise ValueError("The access check must belong to this candidate URL.")
        if self.license_status != "unknown" and not self.license:
            raise ValueError("A reported or verified license needs its name or URL.")
        return self


class SearchAttempt(Model):
    provider: str
    url: str = ""
    outcome: Literal["results", "not_found_in_this_search", "error", "skipped"]
    checked_at: str = Field(default_factory=now)
    detail: str = ""


class AccessReport(Model):
    format_version: Literal[1] = 1
    id: Identifier
    paper: PaperQuery
    candidates: list[PublicCandidate] = Field(default_factory=list, max_length=100)
    searches: list[SearchAttempt] = Field(default_factory=list, max_length=100)
    notes: str = Field(default="", max_length=20000)


def _public_socket(host, port, timeout):
    addresses = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    if not addresses or any(not ipaddress.ip_address(item[4][0]).is_global for item in addresses):
        raise ValueError("Private, loopback and non-public network destinations are not allowed.")
    # Connect to the checked address itself, avoiding a second DNS lookup.
    last_error = None
    for family, kind, protocol, _, address in addresses:
        sock = socket.socket(family, kind, protocol)
        try:
            sock.settimeout(timeout)
            sock.connect(address)
            return sock
        except OSError as error:
            last_error = error
            sock.close()
    raise last_error


def fetch(url: str, *, limit=2_000_000, headers=None) -> tuple[int, dict, bytes, str]:
    """Bounded anonymous GET; no cookies, proxy credentials, or private-network redirects."""
    request_headers = {"User-Agent": "Bukan-public-access/1 (https://github.com/Nkzono99/bukan)",
                       "Accept": "application/json, application/pdf, text/html;q=0.8", **(headers or {})}
    for _ in range(6):
        http_url(url)
        parts = urlsplit(url)
        host = parts.hostname.encode("idna").decode("ascii")
        port = parts.port or (443 if parts.scheme == "https" else 80)
        connection = http.client.HTTPConnection(host, port, timeout=8)
        try:
            sock = _public_socket(host, port, 8)
            connection.sock = sock
            if parts.scheme == "https":
                connection.sock = ssl.create_default_context().wrap_socket(sock, server_hostname=host)
            path = quote(parts.path or "/", safe="/%:@!$&'()*+,;=-._~")
            if parts.query:
                path += "?" + quote(parts.query, safe="%=&+/:;,@!$'()*-._~")
            connection.request("GET", path, headers=request_headers)
            response = connection.getresponse()
            response_headers = {name.lower(): value for name, value in response.getheaders()}
            if response.status in {301, 302, 303, 307, 308} and response_headers.get("location"):
                destination = urljoin(url, response_headers["location"])
                target = urlsplit(destination)
                if (target.scheme, target.netloc) != (parts.scheme, parts.netloc):
                    request_headers.pop("Authorization", None)
                url = destination
                continue
            return response.status, response_headers, response.read(limit + 1), url
        finally:
            connection.close()
    raise ValueError("Too many redirects.")


def check_public_urls(urls: list[str]) -> dict:
    if not 1 <= len(urls) <= 20:
        raise ValueError("Check 1 to 20 URLs per call.")

    def check(url):
        try:
            status, headers, body, final = fetch(url, limit=4096)
            state = ("reachable" if 200 <= status < 300 else "blocked" if status in {401, 403, 429}
                     else "not_found" if status in {404, 410} else "http_error")
            return AccessCheck(url=url, status=state, final_url=final, http_status=status,
                               content_type=headers.get("content-type", ""), pdf_signature=body.lstrip().startswith(b"%PDF-"),
                               detail="HTTP reachability only; not proof of full-text access, matching identity, or license.")
        except (TimeoutError, socket.timeout):
            return AccessCheck(url=url, status="timeout", detail="Request timed out; access rights remain unknown.")
        except ValueError as error:
            return AccessCheck(url=url, status="unsafe_url", detail=str(error))
        except (OSError, http.client.HTTPException):
            return AccessCheck(url=url, status="network_error", detail="Network/TLS failure; access rights remain unknown.")

    with ThreadPoolExecutor(max_workers=4) as pool:
        checks = list(pool.map(check, urls))
    return {"checks": [item.model_dump() for item in checks]}


def _native_library() -> list[dict]:
    executable, workspace = os.getenv("BUKAN_EXECUTABLE"), os.getenv("BUKAN_WORKSPACE")
    if not executable or not workspace:
        raise ValueError("Unresolved paper_id: use the Bukan research launcher, or supply title/DOI from get_paper.")
    result = subprocess.run([executable, "scan", workspace, "--json"], capture_output=True, text=True,
                            encoding="utf-8", timeout=60,
                            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    if result.returncode:
        raise ValueError("The local library could not be read; supply title/DOI or check bukan doctor.")
    return json.loads(result.stdout)["papers"]


def resolve_papers(store: Store, papers: list[PaperQuery]) -> list[PaperQuery | str]:
    """Reuse imported metadata first; delegate native indexing to Rust once per batch."""
    with store.connect() as db:
        imported = [json.loads(row[0]) for row in db.execute(
            "SELECT r.body FROM records r JOIN heads h ON r.id=h.id AND r.revision=h.revision WHERE r.kind='paper'")]
    lookup = {}
    for paper in imported:
        lookup[paper["id"]] = paper
        for ref in paper.get("external_refs", []):
            if ref["provider"] == "bukan":
                lookup[ref["external_id"]] = paper
    need_library = any(p.paper_id not in lookup and not (p.title or p.doi) for p in papers)
    library_error = ""
    if need_library:
        try:
            for paper in _native_library():
                lookup.setdefault(paper["id"], paper)
                if paper.get("legacyId"):
                    lookup.setdefault(paper["legacyId"], paper)
        except (ValueError, OSError, subprocess.TimeoutExpired) as error:
            library_error = str(error)
    resolved = []
    for query in papers:
        metadata = lookup.get(query.paper_id, {})
        values = query.model_dump()
        for field in ("doi", "title", "authors", "year"):
            if not values[field] and metadata.get(field):
                values[field] = metadata[field]
        if not (values["doi"] or values["title"]):
            resolved.append(library_error or f"Paper ID not found: {query.paper_id}")
        else:
            try:
                resolved.append(PaperQuery.model_validate(values))
            except ValueError:
                resolved.append(f"Invalid stored bibliography for {query.paper_id}; search with corrected DOI/title instead.")
    return resolved


def _version(value):
    value = {"vor": "publishedVersion", "am": "acceptedVersion"}.get(value, value)
    return value if value in {"publishedVersion", "acceptedVersion", "submittedVersion"} else "unknown"


def _crossref(work: dict, query: PaperQuery, source: str) -> list[PublicCandidate]:
    doi = work.get("DOI", "").lower()
    title = (work.get("title") or [""])[0]
    authors = "; ".join(" ".join(filter(None, (a.get("given"), a.get("family")))) for a in work.get("author", []))
    dates = (work.get("issued", {}).get("date-parts") or [[]])[0]
    common = dict(title=title, authors=authors, year=dates[0] if dates else None,
                  identifiers={"doi": doi}, match="exact_doi" if query.doi and query.doi == doi else "bibliographic_candidate",
                  provider="crossref", source_url=source)
    links = [(work.get("URL"), "landing_page", "unknown")]
    links.extend((item.get("URL"), "full_text", _version(item.get("content-version"))) for item in work.get("link", []))
    candidates = []
    for url, role, version in links:
        if not url:
            continue
        licenses = [item["URL"] for item in work.get("license", []) if item.get("URL")
                    and _version(item.get("content-version")) == version and version != "unknown"]
        try:
            candidates.append(PublicCandidate(url=url, role=role, version=version, **common,
                              license="; ".join(licenses), license_status="reported" if licenses else "unknown",
                              license_source_url=source if licenses else "",
                              note="Crossref full-text/TDM links and license deposits do not establish free access."))
        except ValueError:
            continue
    for relation, version in (("has-preprint", "submittedVersion"), ("is-preprint-of", "publishedVersion")):
        for item in work.get("relation", {}).get(relation, []):
            if item.get("id-type") == "doi" and item.get("id"):
                candidates.append(PublicCandidate(url="https://doi.org/" + quote(item["id"], safe="/"),
                    role="landing_page", version=version, **dict(common, identifiers={"doi": item["id"]}, match="external_candidate"),
                    note=f"Crossref {relation} relation; identity/version equivalence needs confirmation."))
    return candidates


def _openalex(work: dict, query: PaperQuery, source: str) -> list[PublicCandidate]:
    doi = re.sub(r"^https?://doi.org/", "", work.get("doi") or "", flags=re.I).lower()
    common = dict(title=work.get("display_name") or "", year=work.get("publication_year"),
                  authors="; ".join((a.get("author") or {}).get("display_name", "") for a in work.get("authorships", [])),
                  identifiers={k: v for k, v in {"doi": doi, "openalex": work.get("id")}.items() if v},
                  match="exact_doi" if query.doi and query.doi == doi else "bibliographic_candidate",
                  provider="openalex", source_url=source)
    candidates = []
    for location in work.get("locations") or []:
        for field, role in (("landing_page_url", "landing_page"), ("pdf_url", "full_text")):
            if not location.get(field):
                continue
            try:
                candidates.append(PublicCandidate(url=location[field], role=role, **common,
                    version=_version(location.get("version")), free_to_read=location.get("is_oa"),
                    license=location.get("license") or "", license_status="reported" if location.get("license") else "unknown",
                    license_source_url=source if location.get("license") else "",
                    note="Free-to-read and license fields are provider reports, not a fresh access or legal verification."))
            except ValueError:
                continue
    return candidates


def _search(query: PaperQuery, provider: str) -> tuple[list[PublicCandidate], SearchAttempt]:
    if provider == "crossref":
        url = "https://api.crossref.org/works"
        url += "/" + quote(query.doi, safe="") if query.doi else "?" + urlencode({
            "query.bibliographic": " ".join(str(v) for v in (query.title, query.authors, query.year) if v), "rows": 3})
        headers = {}
    else:
        url = "https://api.openalex.org/works"
        url += "/https://doi.org/" + quote(query.doi, safe="/") if query.doi else "?" + urlencode({
            "search": query.title, "per_page": 3})
        key = os.getenv("BUKAN_OPENALEX_API_KEY")
        headers = {"Authorization": "Bearer " + key} if key else {}
    try:
        status, _, body, _ = fetch(url, headers=headers)
        if status == 404:
            return [], SearchAttempt(provider=provider, url=url, outcome="not_found_in_this_search")
        if status != 200:
            return [], SearchAttempt(provider=provider, url=url, outcome="error", detail=f"HTTP {status}; access rights unknown. Retry later for 429.")
        if len(body) > 2_000_000:
            raise ValueError("Provider response exceeded the size limit.")
        data = json.loads(body)
        if provider == "crossref":
            works = [data["message"]] if query.doi else data["message"]["items"]
            parse = _crossref
        else:
            works = [data] if query.doi else data["results"]
            parse = _openalex
        candidates = [candidate for work in works for candidate in parse(work, query, url)]
        return candidates, SearchAttempt(provider=provider, url=url,
            outcome="results" if candidates else "not_found_in_this_search",
            detail="Metadata candidates only; title matches require identity checking.")
    except (ValueError, KeyError, TypeError, AttributeError, OSError, http.client.HTTPException):
        return [], SearchAttempt(provider=provider, url=url, outcome="error", detail="Provider response/network failure; retry or record external candidates.")


def find_public_versions(store: Store, papers: list[PaperQuery], providers: list[str] | None = None) -> dict:
    if not 1 <= len(papers) <= 20:
        raise ValueError("Search 1 to 20 papers per call.")
    providers = ["crossref", "openalex"] if providers is None else list(dict.fromkeys(providers))
    if not providers or any(p not in {"crossref", "openalex"} for p in providers):
        raise ValueError("Providers must contain crossref and/or openalex.")
    resolved = resolve_papers(store, papers)

    def search(pair):
        original, query = pair
        identity = "id:" + original.paper_id if original.paper_id else "doi:" + original.doi if original.doi else original.model_dump_json()
        report = AccessReport(id="access-" + hashlib.sha256(identity.encode()).hexdigest()[:32],
                              paper=original if isinstance(query, str) else query)
        if isinstance(query, str):
            report.searches.append(SearchAttempt(provider="bukan", outcome="error", detail=query))
        else:
            for provider in providers:
                candidates, attempt = _search(query, provider)
                report.candidates.extend(candidates)
                report.searches.append(attempt)
            unique = {(c.provider, c.url, c.version, json.dumps(c.identifiers, sort_keys=True)): c for c in report.candidates}
            report.candidates = list(unique.values())[:100]
            if len(unique) > 100:
                report.notes = "Candidate limit reached (100); this result is incomplete."
        return report.model_dump()

    with ThreadPoolExecutor(max_workers=4) as pool:
        reports = list(pool.map(search, zip(papers, resolved)))
    return {"reports": reports, "saved": False,
            "notice": "Candidates only; not found means not found in these searches. Save explicitly after merging prior records."}


def _has_reports(db):
    return db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='public_access_reports'").fetchone() is not None


def save_public_access(store: Store, report: AccessReport, expected_revision: int = 0) -> dict:
    if expected_revision < 0:
        raise ValueError("expected_revision must be nonnegative.")
    body = report.model_dump_json()
    # An optional, additive metadata table: old format-2 readers/writers and all
    # scientific entity schemas are unchanged. No PDF or Paperpile writes.
    with store.connect(write=True) as db:
        db.execute("CREATE TABLE IF NOT EXISTS public_access_reports ("
                   "id TEXT NOT NULL, revision INTEGER NOT NULL, body TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY(id, revision))")
        previous = db.execute("SELECT revision,body FROM public_access_reports WHERE id=? ORDER BY revision DESC LIMIT 1", (report.id,)).fetchone()
        revision = previous[0] if previous else 0
        if revision != expected_revision:
            raise ValueError(f"Revision conflict: {report.id}; expected {expected_revision}, current {revision}")
        if previous and previous[1] == body:
            return {"id": report.id, "revision": revision, "status": "unchanged"}
        db.execute("INSERT INTO public_access_reports VALUES (?,?,?,?)", (report.id, revision + 1, body, now()))
    return {"id": report.id, "revision": revision + 1, "status": "saved"}


def _cell(value):
    return html.escape(str(value or "")).replace("|", "&#124;").replace("\n", " ").replace("\r", " ")


def render_reports(items: list[dict]) -> str:
    lines = ["# Public access candidates", "", "Metadata and HTTP reachability do not certify full text, identity, or reuse rights.", ""]
    for item in items:
        report = item["report"]
        lines += ["## " + _cell(report["paper"]["title"] or report["paper"]["doi"] or report["paper"]["paper_id"]), "",
                  f"Record: `{report['id']}` / revision {item['revision']}", "",
                  "| URL | Version / identity | Free-to-read (reported) | License | Check | Source |",
                  "| --- | --- | --- | --- | --- | --- |"]
        for candidate in report["candidates"]:
            url = quote(candidate["url"], safe="/:?=&%+#@;,-._~")
            check = candidate.get("check") or {}
            cells = [f"[link](<{url}>)", _cell(candidate["version"] + " / " + candidate["match"]),
                     "unknown" if candidate["free_to_read"] is None else str(candidate["free_to_read"]).lower(),
                     _cell((candidate["license"] or "unknown") + " / " + candidate["license_status"]),
                     _cell(check.get("status", "not_checked") + " " + check.get("checked_at", "")),
                     _cell(candidate["provider"] + " " + candidate["discovered_at"])]
            lines.append("| " + " | ".join(cells) + " |")
        if not report["candidates"]:
            lines += ["", "No candidates recorded; this does not establish that no public copy exists."]
        lines += ["", "Searches:"] + [f"- {_cell(a['provider'])}: {_cell(a['outcome'])} ({_cell(a['checked_at'])}); {_cell(a['detail'])}" for a in report["searches"]]
        if report["notes"]:
            lines += ["", _cell(report["notes"])]
        lines.append("")
    return "\n".join(lines)


def _decode_report(row):
    return {"report": json.loads(row["body"]), "revision": row["revision"], "created_at": row["created_at"]}


def get_public_access(store: Store, record_id: str, revision: int | None = None) -> dict:
    if revision is not None and revision < 1:
        raise ValueError("revision must be positive.")
    with store.connect() as db:
        row = db.execute("SELECT * FROM public_access_reports WHERE id=? AND (? IS NULL OR revision=?) ORDER BY revision DESC LIMIT 1",
                         (record_id, revision, revision)).fetchone() if _has_reports(db) else None
        if row is None:
            raise ValueError("Public access record not found.")
        result = _decode_report(row)
    return {**result, "markdown": render_reports([result])}


def search_public_access(store: Store, query: str = "", limit: int = 20, offset: int = 0) -> dict:
    if not 1 <= limit <= 100 or offset < 0:
        raise ValueError("limit must be 1..100; offset must be nonnegative.")
    with store.connect() as db:
        rows = db.execute("SELECT r.* FROM public_access_reports r JOIN "
            "(SELECT id,MAX(revision) AS revision FROM public_access_reports GROUP BY id) h USING(id,revision) "
            "WHERE instr(lower(r.body),lower(?))>0 ORDER BY r.id LIMIT ? OFFSET ?",
            (query, limit + 1, offset)).fetchall() if _has_reports(db) else []
    items = [_decode_report(row) for row in rows[:limit]]
    return {"items": items, "next_offset": offset + limit if len(rows) > limit else None, "markdown": render_reports(items)}


def handle_request(store: Store, request: dict) -> dict:
    """CLI transport for the same operations exposed as individual MCP tools."""
    if not isinstance(request, dict):
        raise ValueError("Public-access request must be a JSON object.")
    arguments = dict(request)
    operation = arguments.pop("operation", None)
    functions = {"find": find_public_versions, "check": check_public_urls, "save": save_public_access,
                 "get": get_public_access, "search": search_public_access}
    if not isinstance(operation, str) or operation not in functions:
        raise ValueError("Use operation find, check, save, get, or search.")
    try:
        if "papers" in arguments:
            arguments["papers"] = [PaperQuery.model_validate(paper) for paper in arguments["papers"]]
        if "report" in arguments:
            arguments["report"] = AccessReport.model_validate(arguments["report"])
        return functions[operation](**arguments) if operation == "check" else functions[operation](store, **arguments)
    except TypeError as error:
        raise ValueError(f"Invalid arguments for public-access {operation}: {error}") from None
