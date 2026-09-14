import argparse
import json
from pathlib import Path

from .models import Write
from .store import Store


def main():
    parser = argparse.ArgumentParser(description="Local research store; keep --store outside Paperpile and the source repository.")
    parser.add_argument("--store", type=Path, required=True)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("init")
    commands.add_parser("info")
    commands.add_parser("export")
    commands.add_parser("serve")
    note = commands.add_parser("export-note")
    note.add_argument("id")
    note.add_argument("--revision", type=int)
    put = commands.add_parser("put")
    put.add_argument("file", type=Path)
    put.add_argument("--author", default="cli-user")
    get = commands.add_parser("get")
    get.add_argument("id")
    get.add_argument("--revision", type=int)
    search = commands.add_parser("search")
    search.add_argument("query", nargs="?", default="")
    search.add_argument("--kind")
    search.add_argument("--offset", type=int, default=0)
    trace = commands.add_parser("trace")
    trace.add_argument("id")
    trace.add_argument("--revision", type=int)
    args = parser.parse_args()
    try:
        store = Store(args.store)
        match args.command:
            case "init":
                result = store.initialize()
            case "info":
                result = store.info()
            case "export":
                result = store.export()
            case "serve":
                from .server import create_server
                store.info()
                create_server(store).run(transport="stdio")
                return
            case "export-note":
                from .notes import export_note
                result = export_note(store, args.id, args.revision)
            case "put":
                writes = [Write.model_validate(item) for item in json.loads(args.file.read_text(encoding="utf-8-sig"))]
                result = store.put(writes, args.author)
            case "get":
                result = store.get(args.id, args.revision)
            case "search":
                result = store.search(args.query, args.kind, offset=args.offset)
            case "trace":
                result = store.trace(args.id, args.revision)
        print(json.dumps(result, ensure_ascii=False, indent=2))
    except (ValueError, OSError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
