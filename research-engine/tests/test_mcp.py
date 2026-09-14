import asyncio
import json
import os
import sys
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

from bukan_research.store import Store


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
