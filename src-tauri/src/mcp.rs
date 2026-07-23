use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::workspace;

const MAX_PAPERS: usize = 250;
const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedPaperList {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub papers: Vec<PresentedPaper>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedPaper {
    pub title: String,
    #[serde(default)]
    pub authors: String,
    pub year: Option<u16>,
    #[serde(default)]
    pub doi: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresentPaperListInput {
    title: String,
    #[serde(default)]
    description: String,
    papers: Vec<PresentedPaper>,
}

pub fn run_stdio_from_environment() -> Result<(), String> {
    let workspace_root = std::env::var_os("BUKAN_WORKSPACE")
        .map(PathBuf::from)
        .ok_or_else(|| "BUKAN_WORKSPACE is not set".to_string())?;
    let (workspace_root, _) = workspace::load_workspace(&workspace_root)?;
    run_stdio(&workspace_root)
}

pub fn run_stdio(workspace_root: &Path) -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("could not read MCP request: {error}"))?;
        if line.len() > MAX_MESSAGE_BYTES {
            return Err("MCP request exceeded the size limit".to_string());
        }
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                write_response(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("Parse error: {error}") }
                    }),
                )?;
                continue;
            }
        };
        if let Some(response) = handle_request(workspace_root, &request) {
            write_response(&mut stdout, &response)?;
        }
    }
    Ok(())
}

fn handle_request(workspace_root: &Path, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str)?;
    let result = match method {
        "initialize" => {
            let protocol_version = request
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-06-18");
            Ok(json!({
                "protocolVersion": protocol_version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "bukan", "version": env!("CARGO_PKG_VERSION") }
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(
            workspace_root,
            request.get("params").unwrap_or(&Value::Null),
        ),
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }));
        }
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": message }],
                "isError": true
            }
        }),
    })
}

fn call_tool(workspace_root: &Path, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tool name is required".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "present_paper_list" => {
            let input: PresentPaperListInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid paper list: {error}"))?;
            let list = validate_list(input)?;
            write_presented_list(workspace_root, &list)?;
            Ok(tool_text(format!(
                "Bukan GUIに「{}」を{}件で表示しました。",
                list.title,
                list.papers.len()
            )))
        }
        "clear_paper_list" => {
            clear_presented_list(workspace_root)?;
            Ok(tool_text("Bukan GUIの一時文献リストを消去しました。"))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "present_paper_list",
            "title": "Present a paper list in Bukan",
            "description": "Show a temporary, structured literature list in the Bukan GUI. Use this after searching, collecting, comparing, or selecting papers when the user would benefit from reviewing the list beside the raw Codex terminal.",
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
            "description": "Clear the temporary paper list currently shown in the Bukan GUI.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }
        }),
    ]
}

fn validate_list(input: PresentPaperListInput) -> Result<PresentedPaperList, String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err("list title must not be empty".to_string());
    }
    if input.papers.len() > MAX_PAPERS {
        return Err(format!("paper list is limited to {MAX_PAPERS} items"));
    }
    let mut papers = Vec::with_capacity(input.papers.len());
    for mut paper in input.papers {
        paper.title = paper.title.trim().to_string();
        if paper.title.is_empty() {
            return Err("every paper must have a title".to_string());
        }
        paper.doi = paper
            .doi
            .trim()
            .trim_start_matches("https://doi.org/")
            .trim_start_matches("http://doi.org/")
            .to_string();
        papers.push(paper);
    }
    Ok(PresentedPaperList {
        title,
        description: input.description.trim().to_string(),
        papers,
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64,
    })
}

fn tool_text(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": false
    })
}

fn write_response(stdout: &mut impl Write, response: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *stdout, response)
        .map_err(|error| format!("could not encode MCP response: {error}"))?;
    stdout
        .write_all(b"\n")
        .and_then(|_| stdout.flush())
        .map_err(|error| format!("could not write MCP response: {error}"))
}

pub fn read_presented_list(workspace_root: &Path) -> Result<Option<PresentedPaperList>, String> {
    let path = bridge_path(workspace_root)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("一時文献リストを読み取れませんでした: {error}"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("一時文献リストの形式が不正です: {error}"))
}

pub fn write_presented_list(
    workspace_root: &Path,
    list: &PresentedPaperList,
) -> Result<(), String> {
    let path = bridge_path(workspace_root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("一時文献リストの保存先を作成できませんでした: {error}"))?;
    }
    let bytes = serde_json::to_vec(list)
        .map_err(|error| format!("一時文献リストを変換できませんでした: {error}"))?;
    fs::write(&path, bytes)
        .map_err(|error| format!("一時文献リストを保存できませんでした: {error}"))
}

pub fn clear_presented_list(workspace_root: &Path) -> Result<(), String> {
    let path = bridge_path(workspace_root)?;
    if path.is_file() {
        fs::remove_file(path)
            .map_err(|error| format!("一時文献リストを消去できませんでした: {error}"))?;
    }
    Ok(())
}

fn bridge_path(workspace_root: &Path) -> Result<PathBuf, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let local_root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Bukan")
        .join("bridge");
    let workspace_id = &blake3::hash(workspace_root.to_string_lossy().as_bytes()).to_hex()[..20];
    Ok(local_root.join(format!("{workspace_id}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_temporary_paper_list() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paperpile = temporary.path().join("Paperpile");
        fs::create_dir_all(paperpile.join("All Papers")).expect("Paperpile");
        let workspace_root = temporary.path().join("workspace");
        workspace::init_workspace(
            &workspace_root,
            Some("MCP test"),
            Some(&paperpile.to_string_lossy()),
        )
        .expect("workspace");
        let list = PresentedPaperList {
            title: "Thermal studies".to_string(),
            description: "Selected by Codex".to_string(),
            papers: vec![PresentedPaper {
                title: "Lunar heat flow".to_string(),
                authors: "Sato".to_string(),
                year: Some(2025),
                doi: "10.1234/example".to_string(),
                url: String::new(),
                note: "Relevant method".to_string(),
                status: "candidate".to_string(),
            }],
            updated_at: 1,
        };

        write_presented_list(&workspace_root, &list).expect("write list");
        let restored = read_presented_list(&workspace_root)
            .expect("read list")
            .expect("list");
        assert_eq!(restored.title, "Thermal studies");
        assert_eq!(restored.papers[0].doi, "10.1234/example");
        clear_presented_list(&workspace_root).expect("clear list");
        assert!(read_presented_list(&workspace_root)
            .expect("read cleared list")
            .is_none());
    }

    #[test]
    fn exposes_the_gui_presentation_tool() {
        let tools = tool_definitions();
        assert_eq!(tools[0]["name"], "present_paper_list");
        assert_eq!(tools[1]["name"], "clear_paper_list");
    }
}
