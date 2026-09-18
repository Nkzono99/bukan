//! Public MCP tool descriptions and argument schemas.

use crate::paper_lists::MAX_PAPERS;
use serde_json::{json, Value};

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "paperpile_browser_status",
            "description": "Check live Paperpile access through Bukan's dedicated Chrome profile. No reference writes. Initial login: bukan paperpile login. Does not use the MCP host's browser or ordinary Chrome profile. May prepare optional browser dependencies and update browser session/cache files.",
            "annotations": { "readOnlyHint": true, "openWorldHint": true },
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "paperpile_import_references",
            "title": "Register references in Paperpile",
            "description": "Register user-selected DOI/URL lines or BibTeX/RIS in My Library through dedicated Chrome, verify browser-local presence, then by default run Paperpile Auto update and Find PDFs online for uniquely matched references, including already-present ones. Paperpile extension must be installed in the dedicated profile for follow-up. Inspect per-reference postprocessing metadata/PDF results: registration success does not prove follow-up completion. postprocess=false registers only; previewOnly never writes. Server and Drive sync remain unverified. Requires one-time bukan paperpile login. Keeps duplicate skipping for Import; preserves existing PDFs; never edits synced files. BibTeX/RIS requires expectedCount; parsed-count mismatch aborts. Unknown outcome may have committed. Max 64 KiB, 100 references; small batches recommended.",
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": true },
            "inputSchema": {
                "type": "object", "additionalProperties": false, "required": ["format", "text"],
                "properties": {
                    "format": { "type": "string", "enum": ["identifiers", "bibtex", "ris"] },
                    "text": { "type": "string", "minLength": 1, "maxLength": 65536 },
                    "destination": { "type": "string", "enum": ["My Library"], "default": "My Library" },
                    "expectedCount": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Required for BibTeX/RIS: number of distinct requested references. For DOI/URL inputs, must match normalized unique input count if supplied." },
                    "previewOnly": { "type": "boolean", "default": false, "description": "Parse and check live duplicates, then cancel. Never imports, updates metadata or searches PDFs." },
                    "postprocess": { "type": "boolean", "default": true, "description": "After verified presence, run Auto update and PDF search for the requested references. Set false for registration only. Requires Paperpile extension in the dedicated Chrome profile." }
                }
            }
        }),
        json!({
            "name": "prepare_paperpile_import",
            "title": "Prepare references for Paperpile registration",
            "description": "Optionally prepare DOI/URL lines or BibTeX/RIS for Paperpile. DOES NOT register papers, contact Paperpile, save files or launch a browser. paperpile_import_references completes a user-requested My Library import through the dedicated browser. Deduplicates input DOI/URL lines only; actual library duplicates are checked by Paperpile. Text limited to 64 KiB; identifiers to 100 lines.",
            "annotations": { "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false },
            "inputSchema": {
                "type": "object", "additionalProperties": false,
                "required": ["format", "text"],
                "properties": {
                    "format": { "type": "string", "enum": ["identifiers", "bibtex", "ris"] },
                    "text": { "type": "string", "minLength": 1, "maxLength": 65536 },
                    "destination": { "type": "string", "minLength": 1, "maxLength": 500, "default": "My Library", "description": "Intended library/folder/label to verify in the browser; this tool does not create or select it." }
                }
            }
        }),
        json!({
            "name": "workspace_context",
            "description": "Identify this server's bound workspace, format version and read-only Paperpile source, including an unavailable source. Research outputs belong to this workspace.",
            "annotations": { "readOnlyHint": true },
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "get_paper_list",
            "description": "Read the structured paper list saved in this workspace, or null if none exists.",
            "annotations": { "readOnlyHint": true },
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "persist_paper_list",
            "description": "Export the saved paper list to a Markdown report or candidate JSON file in the bound workspace. Returns the new file path.",
            "inputSchema": { "type": "object", "additionalProperties": false,
                "required": ["destination"], "properties": {
                    "destination": { "type": "string", "enum": ["reports", "candidates"] }
                }
            }
        }),
        json!({
            "name": "present_paper_list",
            "title": "Save a paper list in the workspace",
            "description": "Save a structured literature list to .bukan/paper-list.json in the bound workspace. Returns the saved list and path. Read it with get_paper_list or export with persist_paper_list.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "papers"],
                "properties": {
                    "title": { "type": "string", "minLength": 1, "maxLength": 200 },
                    "description": { "type": "string", "maxLength": 2000 },
                    "papers": {
                        "type": "array",
                        "maxItems": MAX_PAPERS,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["title"],
                            "properties": {
                                "paperId": { "type": "string", "maxLength": 200, "description": "Stable or legacy Bukan paper ID when this item already exists in the local library." },
                                "title": { "type": "string", "minLength": 1, "maxLength": 500 },
                                "authors": { "type": "string", "maxLength": 500 },
                                "year": { "type": ["integer", "null"], "minimum": 1500, "maximum": 2200 },
                                "doi": { "type": "string", "maxLength": 300 },
                                "url": { "type": "string", "maxLength": 2000 },
                                "note": { "type": "string", "maxLength": 4000 },
                                "status": { "type": "string", "maxLength": 100 }
                            }
                        }
                    }
                }
            }
        }),
        json!({
            "name": "clear_paper_list",
            "title": "Clear the Bukan paper list",
            "description": "Remove the saved structured paper list from the bound workspace.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }
        }),
        json!({
            "name": "search_library",
            "title": "Search the Paperpile library",
            "description": "Search the mounted Paperpile library read-only by title, author, year, filename, or collection. Returns stable Bukan paper IDs and source paths. Use this before scanning the filesystem manually.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": { "type": "string", "maxLength": 500 },
                    "collection": { "type": "string", "maxLength": 500 },
                    "yearFrom": { "type": ["integer", "null"], "minimum": 1500, "maximum": 2200 },
                    "yearTo": { "type": ["integer", "null"], "minimum": 1500, "maximum": 2200 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 50 }
                }
            }
        }),
        json!({
            "name": "list_collections",
            "title": "List Paperpile collections",
            "description": "List the current read-only Paperpile collection tree paths and paper counts.",
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "get_paper",
            "title": "Get paper metadata",
            "description": "Get metadata and the read-only PDF path for a paper by stable or legacy Bukan paper ID.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["paperId"],
                "properties": { "paperId": { "type": "string", "minLength": 1, "maxLength": 200 } }
            }
        }),
        json!({
            "name": "get_current_paper",
            "title": "Get the current Bukan paper",
            "description": "Read an existing .bukan/current-context.md compatibility file. If absent, choose a paper with search_library and get_paper.",
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "get_paper_document",
            "description": "Retrieve the exact PDF version (SHA-256) and total page count from the mounted Paperpile / Google Drive folder, read-only. Requires Poppler on PATH. This may download an on-demand Drive file. Follow with all pages and visual inspection before research synthesis.",
            "annotations": { "readOnlyHint": true },
            "inputSchema": { "type": "object", "additionalProperties": false,
                "required": ["paperId"], "properties": { "paperId": { "type": "string" } } }
        }),
        json!({
            "name": "read_paper_pages",
            "description": "Read unabridged page text from one pinned PDF version. Start at page 1 and continue nextPage until null; file pages include covers and appendices. Empty or garbled text requires page image inspection or OCR. Text extraction alone does not establish full review.",
            "annotations": { "readOnlyHint": true },
            "inputSchema": { "type": "object", "additionalProperties": false,
                "required": ["paperId", "sha256", "startPage"],
                "properties": { "paperId": { "type": "string" },
                    "sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "startPage": { "type": "integer", "minimum": 1 },
                    "pageCount": { "type": "integer", "minimum": 1, "maximum": 10, "default": 3 } } }
        }),
        json!({
            "name": "read_paper_page_image",
            "description": "Return a PNG of an entire PDF page from the specified SHA-256 version for reading figures, tables and equations. Maximum dimension 2000 pixels; unreadable details require higher-resolution inspection by the client. Rendering is not reading completion.",
            "annotations": { "readOnlyHint": true },
            "inputSchema": { "type": "object", "additionalProperties": false,
                "required": ["paperId", "sha256", "page"],
                "properties": { "paperId": { "type": "string" },
                    "sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "page": { "type": "integer", "minimum": 1 } } }
        }),
        json!({
            "name": "create_review",
            "title": "Create a living research review",
            "description": "Create a durable, theme-based review article project in reports/reviews. Use it when the user wants an accumulating literature review that can be revised over time.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["theme"],
                "properties": {
                    "theme": { "type": "string", "minLength": 1, "maxLength": 300 },
                    "title": { "type": "string", "maxLength": 300 }
                }
            }
        }),
        json!({
            "name": "list_reviews",
            "title": "List living research reviews",
            "description": "List the durable review projects currently accumulated in Bukan.",
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "get_review",
            "title": "Read a research review",
            "description": "Read a review's current Markdown, structured citations, figures, and revision metadata.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["reviewId"],
                "properties": {
                    "reviewId": { "type": "string", "minLength": 1, "maxLength": 160 }
                }
            }
        }),
        json!({
            "name": "get_current_review",
            "title": "Get the current research review",
            "description": "Read the living review currently selected in the Bukan App, including its review ID and article path.",
            "inputSchema": { "type": "object", "additionalProperties": false, "properties": {} }
        }),
        json!({
            "name": "update_review",
            "title": "Update a research review",
            "description": "Replace the current review Markdown and optionally its structured citations. Bukan preserves the previous revision in history. Important claims must use citation markers such as [@paper-id, p. 12], and citations must identify local library papers.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["reviewId", "markdown"],
                "properties": {
                    "reviewId": { "type": "string", "minLength": 1, "maxLength": 160 },
                    "markdown": { "type": "string", "maxLength": 2000000 },
                    "citations": {
                        "type": "array",
                        "maxItems": MAX_PAPERS,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["paperId"],
                            "properties": {
                                "paperId": { "type": "string", "minLength": 1, "maxLength": 200 },
                                "locator": { "type": "string", "maxLength": 300, "description": "PDF page, section, figure, or table supporting the cited claim." },
                                "note": { "type": "string", "maxLength": 2000 }
                            }
                        }
                    }
                }
            }
        }),
        json!({
            "name": "attach_review_figure",
            "title": "Attach an extracted figure to a review",
            "description": "Copy an image previously extracted into the Bukan workspace into a review's figures directory and record its source paper, page, and caption. The Paperpile PDF remains read-only.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["reviewId", "imagePath", "caption", "sourcePaperId"],
                "properties": {
                    "reviewId": { "type": "string", "minLength": 1, "maxLength": 160 },
                    "imagePath": { "type": "string", "minLength": 1, "maxLength": 2000 },
                    "caption": { "type": "string", "maxLength": 2000 },
                    "sourcePaperId": { "type": "string", "minLength": 1, "maxLength": 200 },
                    "page": { "type": ["integer", "null"], "minimum": 1, "maximum": 100000 }
                }
            }
        }),
    ]
}
