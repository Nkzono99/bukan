import asyncio
import json
import os
import sys
import threading
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

from bukan_research.store import Store
import pytest


def test_real_stdio_server_registers_reopens_and_rejects_bad_reference(tmp_path):
    path = tmp_path / "research.sqlite"
    Store(path).initialize()

    async def run():
        params = StdioServerParameters(command=sys.executable,
            args=["-m", "bukan_research.cli", "--store", str(path), "serve"],
            env={**os.environ, "PYTHONUTF8": "1"})
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as client:
                await client.initialize()
                tools = await client.list_tools()
                assert {"put_records", "trace_record", "search_records"} <= {t.name for t in tools.tools}
                result = await client.call_tool("put_records", {"writes": [{"entity": {
                    "kind": "paper", "id": "p", "title": "ダスト輸送"}}]})
                assert not result.isError
                result = await client.call_tool("search_records", {"query": "輸送"})
                assert not result.isError
                data = json.loads(result.content[0].text)
                assert data["items"][0]["entity"]["title"] == "ダスト輸送"
                bad = await client.call_tool("put_records", {"writes": [{"entity": {
                    "kind": "topic", "id": "t", "title": "Test", "scope": "Test",
                    "members": [{"id": "missing", "revision": 1}]}}]})
                assert bad.isError
                note_result = await client.call_tool("put_records", {"writes": [
                    {"entity": {"kind": "source", "id": "s", "paper": {"id": "p", "revision": 1},
                     "uri": "fixture://p.pdf", "version": "v1", "locator": "PDF p. 1", "source_type": "body",
                     "asset_sha256": "a" * 64, "text": "Full page fixture"}},
                    {"entity": {"kind": "paper_note", "id": "n", "source": {"id": "s", "revision": 1},
                     "title": "文献ノート", "markdown": "輸送条件を検討する。", "page_count": 2}}
                ]})
                assert not note_result.isError
                note_result = await client.call_tool("get_paper_note", {"record_id": "n"})
                assert not note_result.isError
                assert json.loads(note_result.content[0].text)["reading_status"] == "screening"
                exported = await client.call_tool("export_paper_note", {"record_id": "n"})
                assert not exported.isError
                bundle = json.loads(exported.content[0].text)
                assert bundle["export_format_version"] == 2
                assert Path(bundle["path"]).name == "index.md"
                assert Path(bundle["path"]).parent.name == "r1"

    asyncio.run(run())
    assert Store(path).info()["counts"] == {"paper": 1, "source": 1, "paper_note": 1}


def test_wiki_stdio_api_preserves_author_and_revision_contract(tmp_path):
    path = tmp_path / "research.sqlite"
    Store(path).initialize()

    async def run():
        params = StdioServerParameters(command=sys.executable,
            args=["-m", "bukan_research.cli", "--store", str(path), "serve"],
            env={**os.environ, "PYTHONUTF8": "1"})
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as client:
                await client.initialize()
                names = {tool.name for tool in (await client.list_tools()).tools}
                assert {"research_wiki_request", "export_wiki_page"} <= names
                created = await client.call_tool("research_wiki_request", {"request": {
                    "operation": "create-wiki", "title": "照射と離脱", "pageType": "concept"}})
                assert not created.isError
                data = json.loads(created.content[0].text)
                identifier = data["id"]
                assert Store(path).get(identifier)["author"] == "ai-client"
                updated = await client.call_tool("research_wiki_request", {"request": {
                    "operation": "save-wiki", "id": identifier, "expectedRevision": 1,
                    "title": "照射と離脱", "summary": "条件を限定して検証する。", "sections": [
                        {"id": "conditions", "title": "対象条件", "markdown": "接触状態を未確認として残す。"}]}})
                assert not updated.isError
                exported = await client.call_tool("export_wiki_page", {"record_id": identifier, "revision": 1})
                assert not exported.isError
                assert json.loads(exported.content[0].text)["revision"] == 1
                assert len(Store(path).history(identifier)) == 2

    asyncio.run(run())


def test_public_access_stdio_saves_partial_external_report_and_preserves_history(tmp_path):
    path = tmp_path / "research.sqlite"
    Store(path).initialize()

    async def run():
        params = StdioServerParameters(command=sys.executable,
            args=["-m", "bukan_research.cli", "--store", str(path), "serve"],
            env={**os.environ, "PYTHONUTF8": "1"})
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as client:
                await client.initialize()
                names = {tool.name for tool in (await client.list_tools()).tools}
                assert {"find_public_versions", "check_public_urls", "save_public_access",
                        "get_public_access", "search_public_access"} <= names
                report = {"id": "external-access", "paper": {"title": "Dust transport"}, "candidates": [{
                    "url": "https://example.org/manuscript", "provider": "browser-search",
                    "source_url": "https://example.org/catalog", "version": "acceptedVersion"}]}
                saved = await client.call_tool("save_public_access", {"report": report})
                assert not saved.isError
                assert json.loads(saved.content[0].text)["revision"] == 1
                report["notes"] = "Identity not yet confirmed"
                saved = await client.call_tool("save_public_access", {"report": report, "expected_revision": 1})
                assert not saved.isError and json.loads(saved.content[0].text)["revision"] == 2
                conflict = await client.call_tool("save_public_access", {"report": report, "expected_revision": 1})
                assert conflict.isError
                old = await client.call_tool("get_public_access", {"record_id": "external-access", "revision": 1})
                assert not old.isError and json.loads(old.content[0].text)["report"]["notes"] == ""
                found = await client.call_tool("search_public_access", {"query": "Dust"})
                assert not found.isError
                data = json.loads(found.content[0].text)
                assert data["items"][0]["revision"] == 2 and "not_checked" in data["markdown"]
                invalid = await client.call_tool("check_public_urls", {"urls": ["http://127.0.0.1/private"]})
                assert not invalid.isError
                assert json.loads(invalid.content[0].text)["checks"][0]["status"] == "unsafe_url"
                empty = await client.call_tool("find_public_versions", {"papers": []})
                assert empty.isError

    asyncio.run(run())
    assert Store(path).info()["counts"] == {}


@pytest.mark.parametrize("name,arguments", [
    ("find_public_versions", {"papers": [{"doi": "10.1234/test"}]}),
    ("check_public_urls", {"urls": ["https://example.org/paper"]}),
])
def test_network_tools_leave_mcp_responsive(tmp_path, monkeypatch, name, arguments):
    from bukan_research import public_access
    from bukan_research.server import create_server

    store = Store(tmp_path / "research.sqlite")
    store.initialize()
    started, release = threading.Event(), threading.Event()

    def slow(*args):
        started.set()
        release.wait(2)
        return {"done": True}

    monkeypatch.setattr(public_access, name, slow)
    server = create_server(store)

    async def run():
        task = asyncio.create_task(server.call_tool(name, arguments))
        try:
            assert await asyncio.to_thread(started.wait, 1)
            assert not task.done(), "Network work blocked the event loop until completion"
            await asyncio.wait_for(server.call_tool("store_info", {}), timeout=0.5)
        finally:
            release.set()
            await task

    asyncio.run(run())
