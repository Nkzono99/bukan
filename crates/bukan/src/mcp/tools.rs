//! MCP argument handling; persistence and indexing remain in their domain modules.

use crate::paper_lists::{
    bridge_path, clear_presented_list, persist_presented_list, read_presented_list, validate_list,
    write_presented_list, PresentPaperListInput, MAX_PAPERS,
};
use crate::{build_index, paper_document::Document, reviews, workspace, LibraryIndex, PaperRecord};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchLibraryInput {
    #[serde(default)]
    query: String,
    #[serde(default)]
    collection: String,
    year_from: Option<u16>,
    year_to: Option<u16>,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadPagesInput {
    paper_id: String,
    sha256: String,
    start_page: u32,
    #[serde(default = "default_page_count")]
    page_count: u32,
}

fn default_page_count() -> u32 {
    3
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PageImageInput {
    paper_id: String,
    sha256: String,
    page: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateReviewInput {
    theme: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewIdInput {
    review_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateReviewInput {
    review_id: String,
    markdown: String,
    citations: Option<Vec<ReviewCitationInput>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewCitationInput {
    paper_id: String,
    #[serde(default)]
    locator: String,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachReviewFigureInput {
    review_id: String,
    image_path: String,
    caption: String,
    source_paper_id: String,
    page: Option<u32>,
}

fn default_search_limit() -> usize {
    50
}

pub(super) fn call_tool(workspace_root: &Path, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tool name is required".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "prepare_paperpile_import" => {
            let input = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid Paperpile import: {error}"))?;
            tool_json(crate::paperpile_import::prepare(input)?)
        }
        "paperpile_import_references" => {
            let input = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid Paperpile registration: {error}"))?;
            let request = crate::paperpile_import::prepare_registration(input)?;
            tool_json(crate::runtime::paperpile(
                workspace_root,
                "import",
                &request,
            )?)
        }
        "paperpile_browser_status" => {
            if arguments != json!({}) {
                return Err("paperpile_browser_status takes no arguments.".into());
            }
            tool_json(crate::runtime::paperpile(
                workspace_root,
                "status",
                &json!({}),
            )?)
        }
        "workspace_context" => tool_json(workspace::describe_workspace(workspace_root)?),
        "get_paper_list" => tool_json(read_presented_list(workspace_root)?),
        "persist_paper_list" => {
            let destination = arguments
                .get("destination")
                .and_then(Value::as_str)
                .ok_or_else(|| "destination is required".to_string())?;
            let path = persist_presented_list(workspace_root, destination)?;
            tool_json(json!({ "path": path }))
        }
        "present_paper_list" => {
            let input: PresentPaperListInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid paper list: {error}"))?;
            let list = validate_list(input)?;
            write_presented_list(workspace_root, &list)?;
            tool_json(json!({ "path": bridge_path(workspace_root)?, "list": list }))
        }
        "clear_paper_list" => {
            clear_presented_list(workspace_root)?;
            Ok(tool_text("保存された文献リストを消去しました。"))
        }
        "search_library" => {
            let input: SearchLibraryInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid library search: {error}"))?;
            let index = workspace_index(workspace_root)?;
            let papers = search_index(&index, &input);
            Ok(tool_json(json!({
                "query": input.query,
                "collection": input.collection,
                "count": papers.len(),
                "papers": papers,
            }))?)
        }
        "list_collections" => {
            let index = workspace_index(workspace_root)?;
            let collections = index
                .collections
                .iter()
                .map(|collection| {
                    let count = index
                        .papers
                        .iter()
                        .filter(|paper| paper.collections.iter().any(|item| item == collection))
                        .count();
                    json!({ "path": collection, "paperCount": count })
                })
                .collect::<Vec<_>>();
            Ok(tool_json(json!({ "collections": collections }))?)
        }
        "get_paper" => {
            let id = arguments
                .get("paperId")
                .and_then(Value::as_str)
                .ok_or_else(|| "paperId is required".to_string())?;
            let index = workspace_index(workspace_root)?;
            let paper = index
                .papers
                .iter()
                .find(|paper| paper.id == id || paper.legacy_id == id)
                .ok_or_else(|| format!("paper not found: {id}"))?;
            Ok(tool_json(paper)?)
        }
        "get_paper_document" => {
            let id = arguments
                .get("paperId")
                .and_then(Value::as_str)
                .ok_or_else(|| "paperId is required".to_string())?;
            let (paper, document) = open_document(workspace_root, id, None)?;
            tool_json(json!({
                "paperId": paper.id, "sourcePath": paper.path,
                "accessMethod": "mounted-library-read-only",
                "sha256": document.sha256, "pageCount": document.page_count,
                "pageNumbering": "1-based PDF file pages, including covers and appendices",
                "warnings": document.warnings, "reviewStatus": "not_assessed_by_retrieval",
            }))
        }
        "read_paper_pages" => {
            let input: ReadPagesInput =
                serde_json::from_value(arguments).map_err(|e| e.to_string())?;
            let (paper, document) =
                open_document(workspace_root, &input.paper_id, Some(&input.sha256))?;
            let pages = document.read_pages(input.start_page, input.page_count)?;
            let end = pages.last().expect("validated nonempty page range").page;
            tool_json(json!({
                "paperId": paper.id, "sha256": document.sha256, "pageCount": document.page_count,
                "pages": pages, "nextPage": if end < document.page_count { Some(end + 1) } else { None },
                "warnings": document.warnings, "reviewStatus": "not_assessed_by_retrieval",
            }))
        }
        "read_paper_page_image" => {
            let input: PageImageInput =
                serde_json::from_value(arguments).map_err(|e| e.to_string())?;
            let (paper, document) =
                open_document(workspace_root, &input.paper_id, Some(&input.sha256))?;
            let (png, warnings) = document.page_image(input.page)?;
            Ok(json!({ "content": [
                { "type": "text", "text": serde_json::to_string(&json!({
                    "paperId": paper.id, "sha256": document.sha256,
                    "page": input.page, "pageCount": document.page_count,
                    "warnings": warnings, "documentWarnings": document.warnings,
                    "reviewStatus": "not_assessed_by_retrieval", "maxDimensionPixels": 2000,
                })).map_err(|e| e.to_string())? },
                { "type": "image", "mimeType": "image/png", "data": STANDARD.encode(png) }
            ] }))
        }
        "get_current_paper" => {
            let context_path = workspace_root.join(".bukan").join("current-context.md");
            if !context_path.is_file() {
                return Err("このワークスペースには current-context.md がありません。search_library と get_paper で論文を選んでください".to_string());
            }
            let context = fs::read_to_string(context_path)
                .map_err(|error| format!("could not read current context: {error}"))?;
            Ok(tool_text(context))
        }
        "create_review" => {
            let input: CreateReviewInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid review: {error}"))?;
            let title = (!input.title.trim().is_empty()).then_some(input.title.as_str());
            let review = reviews::create_review(workspace_root, &input.theme, title)?;
            Ok(tool_json(&review)?)
        }
        "list_reviews" => Ok(tool_json(&reviews::list_reviews(workspace_root)?)?),
        "get_review" => {
            let input: ReviewIdInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid review ID: {error}"))?;
            Ok(tool_json(&reviews::load_review(
                workspace_root,
                &input.review_id,
            )?)?)
        }
        "get_current_review" => {
            let context_path = workspace_root.join(".bukan").join("current-review.md");
            if !context_path.is_file() {
                return Err("Bukanで現在のレビューが選択されていません".to_string());
            }
            let context = fs::read_to_string(context_path)
                .map_err(|error| format!("could not read current review: {error}"))?;
            Ok(tool_text(context))
        }
        "update_review" => {
            let input: UpdateReviewInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid review update: {error}"))?;
            let citations = input
                .citations
                .map(|citations| review_citations(workspace_root, citations))
                .transpose()?;
            let review = reviews::update_review(
                workspace_root,
                &input.review_id,
                &input.markdown,
                citations,
            )?;
            Ok(tool_json(&review)?)
        }
        "attach_review_figure" => {
            let input: AttachReviewFigureInput = serde_json::from_value(arguments)
                .map_err(|error| format!("invalid review figure: {error}"))?;
            let index = workspace_index(workspace_root)?;
            if !index.papers.iter().any(|paper| {
                paper.id == input.source_paper_id || paper.legacy_id == input.source_paper_id
            }) {
                return Err(format!(
                    "source paper was not found: {}",
                    input.source_paper_id
                ));
            }
            let review = reviews::attach_figure(
                workspace_root,
                &input.review_id,
                Path::new(&input.image_path),
                &input.caption,
                &input.source_paper_id,
                input.page,
            )?;
            Ok(tool_json(&review)?)
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn review_citations(
    workspace_root: &Path,
    inputs: Vec<ReviewCitationInput>,
) -> Result<Vec<reviews::ReviewCitation>, String> {
    if inputs.len() > MAX_PAPERS {
        return Err(format!(
            "review citations are limited to {MAX_PAPERS} items"
        ));
    }
    let index = workspace_index(workspace_root)?;
    inputs
        .into_iter()
        .map(|input| {
            let paper = index
                .papers
                .iter()
                .find(|paper| paper.id == input.paper_id || paper.legacy_id == input.paper_id)
                .ok_or_else(|| format!("cited paper was not found: {}", input.paper_id))?;
            Ok(reviews::ReviewCitation {
                paper_id: paper.id.clone(),
                title: paper.title.clone(),
                authors: paper.authors.clone().unwrap_or_default(),
                year: paper.year,
                locator: input.locator.trim().to_string(),
                note: input.note.trim().to_string(),
            })
        })
        .collect()
}

fn workspace_index(workspace_root: &Path) -> Result<LibraryIndex, String> {
    let (workspace_root, config) = workspace::load_workspace(workspace_root)?;
    let paperpile_root = workspace::resolve_paperpile_root(&workspace_root, &config)?
        .ok_or_else(|| "Paperpile library was not detected".to_string())?;
    build_index(&paperpile_root)
}

fn open_document(
    workspace_root: &Path,
    id: &str,
    expected: Option<&str>,
) -> Result<(PaperRecord, Document), String> {
    let index = workspace_index(workspace_root)?;
    let paper = index
        .papers
        .into_iter()
        .find(|paper| paper.id == id || paper.legacy_id == id)
        .ok_or_else(|| format!("paper not found: {id}"))?;
    let document = Document::open(Path::new(&index.root), Path::new(&paper.path), expected)?;
    Ok((paper, document))
}

fn search_index<'a>(index: &'a LibraryIndex, input: &SearchLibraryInput) -> Vec<&'a PaperRecord> {
    let terms = input
        .query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let collection = input.collection.trim().to_lowercase();
    index
        .papers
        .iter()
        .filter(|paper| {
            if let Some(year_from) = input.year_from {
                if paper.year.unwrap_or(0) < year_from {
                    return false;
                }
            }
            if let Some(year_to) = input.year_to {
                if paper.year.unwrap_or(u16::MAX) > year_to {
                    return false;
                }
            }
            if !collection.is_empty()
                && !paper
                    .collections
                    .iter()
                    .any(|item| item.to_lowercase().contains(&collection))
            {
                return false;
            }
            let haystack = format!(
                "{} {} {} {} {}",
                paper.title,
                paper.authors.as_deref().unwrap_or_default(),
                paper.year.map(|year| year.to_string()).unwrap_or_default(),
                paper.file_name,
                paper.collections.join(" ")
            )
            .to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .take(input.limit.clamp(1, 200))
        .collect()
}

fn tool_text(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": false
    })
}

fn tool_json(value: impl Serialize) -> Result<Value, String> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("could not encode tool result: {error}"))?;
    Ok(tool_text(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LibraryStats;

    #[test]
    fn searches_the_shared_library_index() {
        let index = LibraryIndex {
            root: "Paperpile".to_string(),
            papers: vec![
                paper(
                    "p2-moon",
                    "Lunar dust transport",
                    "Sato",
                    2025,
                    "Moon / Dust",
                ),
                paper(
                    "p2-mars",
                    "Martian atmospheric escape",
                    "Chen",
                    2022,
                    "Mars",
                ),
            ],
            collections: vec!["Mars".to_string(), "Moon / Dust".to_string()],
            stats: LibraryStats {
                paper_count: 2,
                collection_count: 2,
                starred_count: 0,
                total_bytes: 2,
            },
            warnings: Vec::new(),
        };
        let input = SearchLibraryInput {
            query: "lunar sato".to_string(),
            collection: "dust".to_string(),
            year_from: Some(2024),
            year_to: None,
            limit: 50,
        };

        let matches = search_index(&index, &input);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "p2-moon");
    }

    fn paper(id: &str, title: &str, authors: &str, year: u16, collection: &str) -> PaperRecord {
        PaperRecord {
            id: id.to_string(),
            legacy_id: format!("legacy-{id}"),
            identity_source: "test".to_string(),
            title: title.to_string(),
            authors: Some(authors.to_string()),
            year: Some(year),
            collections: vec![collection.to_string()],
            path: format!("{id}.pdf"),
            relative_path: format!("{id}.pdf"),
            file_name: format!("{title}.pdf"),
            size_bytes: 1,
            modified_at: None,
            starred: false,
        }
    }
}
