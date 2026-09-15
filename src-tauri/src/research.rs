//! Desktop access to the independent research engine and workspace documents.
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use walkdir::WalkDir;

use crate::{research_runtime, workspace};

const MAX_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchFile {
    path: String,
    title: String,
    group: String,
    modified_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchDocument {
    path: String,
    relative_path: String,
    title: String,
    markdown: String,
    sha256: String,
    editable: bool,
}

fn checked_workspace(root: &Path) -> Result<PathBuf, String> {
    let (root, _) = workspace::load_workspace(root)?;
    research_runtime::store_path(&root)?;
    Ok(root)
}

fn is_protected(path: &Path) -> bool {
    path.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("Paperpile")
    }) || path.ancestors().any(|parent| {
        parent.join("All Papers").is_dir() || parent.join("src-tauri/Cargo.toml").is_file()
    })
}

fn checked_file(root: &Path, requested: &Path) -> Result<PathBuf, String> {
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let path = fs::canonicalize(candidate)
        .map_err(|error| format!("ファイルを開けませんでした: {error}"))?;
    if !path.starts_with(root) || !path.is_file() || is_protected(&path) {
        return Err("研究ワークスペース内のファイルを指定してください".into());
    }
    Ok(path)
}

// Managed output paths must never follow directory links into another workspace or library.
fn output_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut result = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err("保存先にはワークスペース内の相対パスを指定してください".into());
        };
        result.push(name);
        if fs::symlink_metadata(&result).is_ok() {
            let resolved = fs::canonicalize(&result).map_err(|error| error.to_string())?;
            if resolved != result || is_protected(&resolved) {
                return Err("研究の保存先にシンボリックリンクは使用できません".into());
            }
        }
    }
    Ok(result)
}

fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_small(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("文書を読み取れませんでした: {e}"))?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("表示できる文書は4 MBまでです".into());
    }
    String::from_utf8(bytes).map_err(|_| "UTF-8の文書を指定してください".to_string())
}

fn document_title(text: &str, path: &Path) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}

fn load_document(root: &Path, requested: &Path) -> Result<ResearchDocument, String> {
    let path = checked_file(root, requested)?;
    if !matches!(extension(&path).as_str(), "md" | "txt" | "bib" | "json") {
        return Err("Markdown・テキスト・BibTeX・JSONを選択してください".into());
    }
    let text = read_small(&path)?;
    let sha256 = digest(text.as_bytes());
    let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
    let editable = extension(&path) == "md"
        && ["notes", "reports", "candidates", "queries"]
            .iter()
            .any(|prefix| relative.starts_with(prefix))
        && !relative.starts_with("reports/reviews");
    Ok(ResearchDocument {
        path: path.to_string_lossy().into_owned(),
        relative_path: relative.to_string_lossy().replace('\\', "/"),
        title: document_title(&text, &path),
        markdown: text,
        sha256,
        editable,
    })
}

fn list_documents(root: &Path) -> (Vec<ResearchFile>, Vec<String>) {
    let mut result = Vec::new();
    let mut warnings = Vec::new();
    for group in ["notes", "reports", "candidates", "queries"] {
        let directory = root.join(group);
        if !directory.is_dir() {
            continue;
        }
        for entry in WalkDir::new(directory)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                !matches!(
                    entry.file_name().to_str(),
                    Some("history" | "figures" | "assets")
                )
            })
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(format!("一部の文書を列挙できませんでした: {error}"));
                    continue;
                }
            };
            if !entry.file_type().is_file()
                || !matches!(extension(entry.path()).as_str(), "md" | "bib")
            {
                continue;
            }
            let path = match checked_file(root, entry.path()) {
                Ok(path) => path,
                Err(error) => {
                    warnings.push(format!("{}: {error}", entry.path().display()));
                    continue;
                }
            };
            let text = match read_small(&path) {
                Ok(text) => text,
                Err(error) => {
                    warnings.push(format!("{}: {error}", entry.path().display()));
                    String::new()
                }
            };
            result.push(ResearchFile {
                path: path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                title: document_title(&text, &path),
                group: group.into(),
                modified_at: fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|t| t.as_secs())
                    .unwrap_or(0),
            });
        }
    }
    result.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.path.cmp(&b.path))
    });
    (result, warnings)
}

#[tauri::command]
pub async fn research_workspace_status(
    app: AppHandle,
    workspace_root: String,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = checked_workspace(Path::new(&workspace_root))?;
        let (_, config) = workspace::load_workspace(&root)?;
        let (documents, warnings) = list_documents(&root);
        Ok(json!({"workspaceRoot": root, "name": config.name,
            "storePath": research_runtime::store_path(&root)?,
            "runtime": research_runtime::status(&app)?, "documents": documents, "warnings": warnings}))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn prepare_research_environment(
    app: AppHandle,
) -> Result<research_runtime::ResearchRuntimeStatus, String> {
    tauri::async_runtime::spawn_blocking(move || research_runtime::prepare(&app))
        .await
        .map_err(|e| e.to_string())?
}

async fn engine_request(
    app: AppHandle,
    workspace_root: String,
    request: Value,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = checked_workspace(Path::new(&workspace_root))?;
        research_runtime::run(&app, &root, &["desktop".into()], Some(&request.to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn research_records(
    app: AppHandle,
    workspace_root: String,
    query: Option<String>,
    kind: Option<String>,
    offset: Option<u32>,
) -> Result<Value, String> {
    engine_request(app, workspace_root, json!({"operation":"summary", "query":query.unwrap_or_default(), "kind":kind, "offset":offset.unwrap_or(0)})).await
}

#[tauri::command]
pub async fn research_record(
    app: AppHandle,
    workspace_root: String,
    id: String,
    revision: Option<u32>,
) -> Result<Value, String> {
    engine_request(
        app,
        workspace_root,
        json!({"operation":"get", "id":id, "revision":revision}),
    )
    .await
}

#[tauri::command]
pub async fn save_research_note(
    app: AppHandle,
    workspace_root: String,
    id: String,
    expected_revision: u32,
    markdown: String,
) -> Result<Value, String> {
    engine_request(app, workspace_root, json!({"operation":"save-note", "id":id, "expectedRevision":expected_revision, "markdown":markdown})).await
}

#[tauri::command]
pub fn read_research_document(
    workspace_root: String,
    path: String,
) -> Result<ResearchDocument, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    load_document(&root, Path::new(&path))
}

fn save_document(
    root: &Path,
    requested: &Path,
    expected: &str,
    markdown: &str,
) -> Result<ResearchDocument, String> {
    let document = load_document(root, requested)?;
    if !document.editable {
        return Err(
            "この文書は直接編集できません。文献ノートは研究記録から開いて改訂してください".into(),
        );
    }
    if document.sha256 != expected {
        return Err(
            "文書が別の操作で更新されています。編集内容を保持して最新版と照合してください".into(),
        );
    }
    if markdown.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("文書は4 MBまでです".into());
    }
    let target = output_path(root, Path::new(&document.relative_path))?;
    let old = fs::read(&target).map_err(|e| e.to_string())?;
    let history = output_path(
        root,
        &PathBuf::from(".bukan/document-history")
            .join(digest(document.relative_path.as_bytes()))
            .join(format!("{}.md", digest(&old))),
    )?;
    fs::create_dir_all(history.parent().unwrap()).map_err(|e| e.to_string())?;
    if !history.exists() {
        fs::write(history, &old).map_err(|e| e.to_string())?;
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(target.parent().unwrap()).map_err(|e| e.to_string())?;
    temporary
        .write_all(markdown.as_bytes())
        .map_err(|e| e.to_string())?;
    if digest(&fs::read(&target).map_err(|e| e.to_string())?) != expected {
        return Err("保存中に文書が更新されました。編集内容を保持して再読込してください".into());
    }
    temporary.persist(&target).map_err(|e| e.to_string())?;
    load_document(root, &target)
}

#[tauri::command]
pub fn save_research_document(
    workspace_root: String,
    path: String,
    expected_sha256: String,
    markdown: String,
) -> Result<ResearchDocument, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    save_document(&root, Path::new(&path), &expected_sha256, &markdown)
}

fn local_link_path(root: &Path, document: &Path, href: &str) -> Result<(PathBuf, String), String> {
    let candidate = if document.is_absolute() {
        document.to_path_buf()
    } else {
        root.join(document)
    };
    let base = if candidate.exists() {
        checked_file(root, &candidate)?
    } else {
        let base = research_runtime::resolve_destination(&candidate)?;
        if !base.starts_with(root.join("data/paper-notes"))
            || extension(&base) != "md"
            || is_protected(&base)
        {
            return Err("画像の参照元には文献ノートの保存先を指定してください".into());
        }
        base
    };
    let base_url = tauri::Url::from_file_path(base)
        .map_err(|_| "文書のパスを解釈できませんでした".to_string())?;
    // URI joining decodes spaces/Unicode and resolves ../ consistently on Windows.
    let url = base_url
        .join(&href.replace('\\', "/"))
        .map_err(|e| e.to_string())?;
    if url.scheme() != "file" {
        return Err("ローカルファイルのリンクを指定してください".into());
    }
    let fragment = url.fragment().unwrap_or_default().to_string();
    let path = url
        .to_file_path()
        .map_err(|_| "ファイルのリンクを解釈できませんでした".to_string())?;
    let path =
        fs::canonicalize(path).map_err(|e| format!("リンク先が見つかりませんでした: {e}"))?;
    Ok((path, fragment))
}

#[tauri::command]
pub fn resolve_research_asset(
    app: AppHandle,
    workspace_root: String,
    document_path: String,
    href: String,
) -> Result<String, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    let (path, _) = local_link_path(&root, Path::new(&document_path), &href)?;
    let path = checked_file(&root, &path)?;
    if !matches!(
        extension(&path).as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg"
    ) {
        return Err("プレビューにはPNG・JPEG・WebP・GIF・SVG画像を使用してください".into());
    }
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn resolve_research_link(
    workspace_root: String,
    document_path: String,
    href: String,
) -> Result<Value, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    if let Ok(url) = tauri::Url::parse(&href) {
        if matches!(url.scheme(), "https" | "http") {
            return Ok(json!({"kind":"external", "path":url.as_str(), "fragment":""}));
        }
    }
    let (path, fragment) = local_link_path(&root, Path::new(&document_path), &href)?;
    if extension(&path) == "pdf" {
        let (_, config) = workspace::load_workspace(&root)?;
        let library = workspace::resolve_paperpile_root(&root, &config)
            .ok()
            .flatten();
        if !(path.starts_with(&root) || library.is_some_and(|library| path.starts_with(library))) {
            return Err("研究ワークスペースまたはPaperpile内のPDFを指定してください".into());
        }
        return Ok(json!({"kind":"pdf", "path":path, "fragment":fragment}));
    }
    let document = load_document(&root, &path)?;
    Ok(json!({"kind":"document", "path":document.path, "fragment":fragment}))
}

#[tauri::command]
pub fn open_research_external(app: AppHandle, url: String) -> Result<(), String> {
    let url = tauri::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("httpまたはhttpsの文献リンクを指定してください".into());
    }
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn checked_pdf(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let (_, config) = workspace::load_workspace(root)?;
    let library = workspace::resolve_paperpile_root(root, &config)
        .ok()
        .flatten();
    if extension(&path) != "pdf"
        || !path.is_file()
        || !(path.starts_with(root) || library.is_some_and(|library| path.starts_with(library)))
    {
        return Err("研究ワークスペースまたはPaperpile内のPDFを指定してください".into());
    }
    Ok(path)
}

#[tauri::command]
pub fn get_research_pdf(
    app: AppHandle,
    workspace_root: String,
    path: String,
) -> Result<crate::PaperRecord, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    let path = checked_pdf(&root, Path::new(&path))?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|e| e.to_string())?;
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (authors, year, title) = crate::parse_paperpile_filename(&file_name);
    Ok(crate::PaperRecord {
        id: format!(
            "research-pdf-{}",
            &digest(path.to_string_lossy().as_bytes())[..20]
        ),
        legacy_id: String::new(),
        identity_source: "research-document".into(),
        title,
        authors,
        year,
        collections: Vec::new(),
        path: path.to_string_lossy().into_owned(),
        relative_path: file_name.clone(),
        file_name,
        size_bytes: fs::metadata(path).map_err(|e| e.to_string())?.len(),
        modified_at: None,
        starred: false,
    })
}

#[tauri::command]
pub fn open_research_pdf(
    app: AppHandle,
    workspace_root: String,
    path: String,
    reveal: bool,
) -> Result<(), String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    let path = checked_pdf(&root, Path::new(&path))?;
    if reveal {
        app.opener()
            .reveal_item_in_dir(path.to_string_lossy().into_owned())
    } else {
        app.opener()
            .open_path(path.to_string_lossy().into_owned(), None::<&str>)
    }
    .map_err(|e| e.to_string())
}

fn save_request(
    root: &Path,
    text: &str,
    record_id: Option<&str>,
    revision: Option<u32>,
    document_path: Option<&str>,
) -> Result<Value, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 50_000 {
        return Err("研究への依頼を1〜50,000バイトで入力してください".into());
    }
    let document = document_path
        .map(|path| checked_file(root, Path::new(path)))
        .transpose()?;
    let request = json!({"formatVersion":1, "text":text, "recordId":record_id, "recordRevision":revision, "documentPath":document});
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let archive = output_path(
        root,
        &PathBuf::from("queries/requests").join(format!("{stamp}.md")),
    )?;
    fs::create_dir_all(archive.parent().unwrap()).map_err(|e| e.to_string())?;
    let context = format!("# Bukanの研究依頼\n\n{text}\n\n## 対象と作業先\n\n- Workspace: `{}`\n- Research store: `{}`\n- Record: {}\n- Document: {}\n\nこの依頼は保存済みです。実行はCodexの対話で依頼された時に開始してください。\nBukan MCPと研究MCPを使い、保存済みの根拠・未処理作業を確認して続行してください。\nAGENTS.mdの全文読解・並列レビュー・引用の外まで探索する手順を守り、結果と残件を同じワークスペースへ保存してください。\nPaperpile原本は読み取り専用です。\n",
        root.display(), research_runtime::store_path(root)?.display(),
        record_id.map(|id| format!("`{id}@{}`", revision.map(|r| r.to_string()).unwrap_or_else(|| "latest".into()))).unwrap_or_else(|| "未指定".into()),
        document.as_ref().map(|p| format!("`{}`", p.display())).unwrap_or_else(|| "未指定".into()));
    fs::write(&archive, &context).map_err(|e| e.to_string())?;
    let current = output_path(root, Path::new(".bukan/current-research.md"))?;
    let current_json = output_path(root, Path::new(".bukan/current-research.json"))?;
    fs::create_dir_all(current.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&current, context).map_err(|e| e.to_string())?;
    fs::write(current_json, request.to_string()).map_err(|e| e.to_string())?;
    Ok(json!({"path":archive,"text":text}))
}

#[tauri::command]
pub fn save_research_request(
    workspace_root: String,
    text: String,
    record_id: Option<String>,
    record_revision: Option<u32>,
    document_path: Option<String>,
) -> Result<Value, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    save_request(
        &root,
        &text,
        record_id.as_deref(),
        record_revision,
        document_path.as_deref(),
    )
}

#[tauri::command]
pub fn read_research_request(workspace_root: String) -> Result<Option<Value>, String> {
    let root = checked_workspace(Path::new(&workspace_root))?;
    let path = output_path(&root, Path::new(".bukan/current-research.json"))?;
    if !path.exists() {
        return Ok(None);
    }
    serde_json::from_str(&read_small(&path)?)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("research");
        workspace::init_workspace(&root, Some("Research"), Some("auto")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        (temporary, root)
    }

    #[test]
    fn document_edits_preserve_history_and_detect_conflicts() {
        let (_temporary, root) = fixture();
        fs::write(root.join("reports/history.md"), "# History\noriginal").unwrap();
        let original = load_document(&root, Path::new("reports/history.md")).unwrap();
        let saved = save_document(
            &root,
            Path::new("reports/history.md"),
            &original.sha256,
            "# History\nnew",
        )
        .unwrap();
        assert!(saved.markdown.contains("new"));
        assert!(save_document(
            &root,
            Path::new("reports/history.md"),
            &original.sha256,
            "lost"
        )
        .is_err());
        let history = root
            .join(".bukan/document-history")
            .join(digest(b"reports/history.md"))
            .join(format!("{}.md", original.sha256));
        assert_eq!(fs::read_to_string(history).unwrap(), "# History\noriginal");
    }

    #[test]
    fn snapshots_are_read_only_and_external_files_are_rejected() {
        let (temporary, root) = fixture();
        fs::write(root.join("data/note.md"), "snapshot").unwrap();
        assert!(
            !load_document(&root, Path::new("data/note.md"))
                .unwrap()
                .editable
        );
        fs::write(temporary.path().join("outside.md"), "private").unwrap();
        assert!(load_document(&root, Path::new("../outside.md")).is_err());
        fs::create_dir_all(root.join("Paperpile/All Papers")).unwrap();
        fs::write(root.join("Paperpile/private.md"), "source").unwrap();
        assert!(load_document(&root, Path::new("Paperpile/private.md")).is_err());
    }

    #[test]
    fn unpreviewable_document_does_not_hide_other_research() {
        let (_temporary, root) = fixture();
        fs::write(root.join("reports/valid.md"), "# Valid report").unwrap();
        fs::write(root.join("reports/binary.md"), [255, 254]).unwrap();
        let oversized = fs::File::create(root.join("reports/large.md")).unwrap();
        oversized.set_len(MAX_DOCUMENT_BYTES + 1).unwrap();
        let (documents, warnings) = list_documents(&root);
        assert_eq!(documents.len(), 3);
        assert_eq!(warnings.len(), 2);
        assert!(documents.iter().any(|doc| doc.title == "Valid report"));
    }

    #[test]
    fn relative_image_links_resolve_unicode_and_spaces() {
        let (_temporary, root) = fixture();
        fs::write(root.join("notes/index.md"), "note").unwrap();
        fs::write(root.join("notes/図 1.png"), "image").unwrap();
        let (image, _) =
            local_link_path(&root, &root.join("notes/index.md"), "%E5%9B%B3%201.png").unwrap();
        assert_eq!(image, root.join("notes/図 1.png"));
        assert!(local_link_path(
            &root,
            &root.join("notes/index.md"),
            "https://example.org/image.png"
        )
        .is_err());
        fs::create_dir_all(root.join("data/note-assets/paper")).unwrap();
        fs::write(root.join("data/note-assets/paper/figure.png"), "image").unwrap();
        let virtual_base = root.join("data/paper-notes/store-hash/note-hash/r2.md");
        let (image, _) = local_link_path(
            &root,
            &virtual_base,
            "../../../note-assets/paper/figure.png",
        )
        .unwrap();
        assert_eq!(image, root.join("data/note-assets/paper/figure.png"));
        assert!(local_link_path(
            &root,
            &root.join("cache/missing.md"),
            "../data/note-assets/paper/figure.png"
        )
        .is_err());
    }

    #[test]
    fn requests_are_saved_in_workspace_and_references_are_checked() {
        let (_temporary, root) = fixture();
        let saved =
            save_request(&root, "ダスト輸送の対立する主張を検証", None, None, None).unwrap();
        assert!(Path::new(saved["path"].as_str().unwrap()).is_file());
        let current = fs::read_to_string(root.join(".bukan/current-research.md")).unwrap();
        assert!(current.contains("data") && current.contains("research.sqlite"));
        assert!(save_request(&root, "check", None, None, Some("../outside.md")).is_err());
    }
}
