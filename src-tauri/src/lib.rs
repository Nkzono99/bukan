use regex::Regex;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use walkdir::{DirEntry, WalkDir};

pub mod mcp;
pub mod organization;
pub mod terminal;
pub mod workspace;

#[derive(Default)]
struct AppState {
    active_root: Mutex<Option<PathBuf>>,
    terminal: terminal::CodexTerminalState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLocation {
    pub path: String,
    pub drive: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperRecord {
    pub id: String,
    pub legacy_id: String,
    pub identity_source: String,
    pub title: String,
    pub authors: Option<String>,
    pub year: Option<u16>,
    pub collections: Vec<String>,
    pub path: String,
    pub relative_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at: Option<u64>,
    pub starred: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub paper_count: usize,
    pub collection_count: usize,
    pub starred_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryIndex {
    pub root: String,
    pub papers: Vec<PaperRecord>,
    pub collections: Vec<String>,
    pub stats: LibraryStats,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChangeToken {
    pub token: String,
    pub paper_count: usize,
    pub checked_at: u64,
}

#[derive(Debug)]
struct PaperBuilder {
    id: String,
    identity_source: String,
    path: PathBuf,
    relative_path: String,
    file_name: String,
    size_bytes: u64,
    modified_at: Option<u64>,
    collections: BTreeSet<String>,
    starred: bool,
}

#[tauri::command]
fn detect_libraries() -> Vec<LibraryLocation> {
    detect_paperpile_roots()
        .into_iter()
        .map(|path| {
            let drive = drive_label(&path);
            LibraryLocation {
                display_name: format!("Google Drive · Paperpile ({drive})"),
                path: path.to_string_lossy().into_owned(),
                drive,
            }
        })
        .collect()
}

#[tauri::command]
fn initialize_workspace(
    root: String,
    name: Option<String>,
    paperpile_path: Option<String>,
) -> Result<workspace::WorkspaceDescriptor, String> {
    workspace::init_workspace(Path::new(&root), name.as_deref(), paperpile_path.as_deref())
}

#[tauri::command]
fn open_workspace(root: String) -> Result<workspace::WorkspaceDescriptor, String> {
    workspace::describe_workspace(Path::new(&root))
}

#[tauri::command]
fn suggest_organization(workspace_root: String) -> Result<organization::OrganizationPlan, String> {
    let descriptor = workspace::describe_workspace(Path::new(&workspace_root))?;
    let paperpile_root = descriptor
        .paperpile_root
        .ok_or_else(|| "Paperpileライブラリが見つかりません".to_string())?;
    let index = build_index(Path::new(&paperpile_root))?;
    let taxonomy = organization::load_taxonomy(Path::new(&descriptor.root))?;
    Ok(organization::generate_plan(&index, &taxonomy))
}

#[tauri::command]
fn scan_library(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> Result<LibraryIndex, String> {
    let root = normalize_library_root(Path::new(&root))?;
    app.asset_protocol_scope()
        .allow_directory(&root, true)
        .map_err(|error| format!("PDF表示の権限を設定できませんでした: {error}"))?;

    *state
        .active_root
        .lock()
        .map_err(|_| "ライブラリ状態を更新できませんでした".to_string())? = Some(root.clone());

    build_index(&root)
}

#[tauri::command]
fn open_paper(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let path = authorized_paper_path(&state, &path)?;
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| format!("PDFを開けませんでした: {error}"))
}

#[tauri::command]
fn reveal_paper(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let path = authorized_paper_path(&state, &path)?;
    app.opener()
        .reveal_item_in_dir(path.to_string_lossy().into_owned())
        .map_err(|error| format!("エクスプローラーを開けませんでした: {error}"))
}

#[tauri::command]
fn open_workspace_in_vscode(app: AppHandle, workspace_root: String) -> Result<(), String> {
    let (root, _) = workspace::load_workspace(Path::new(&workspace_root))?;
    let url = vscode_workspace_url(&root)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| format!("VS Codeでワークスペースを開けませんでした: {error}"))
}

#[tauri::command]
fn codex_runtime_status(
    state: State<'_, AppState>,
) -> Result<terminal::CodexRuntimeStatus, String> {
    state.terminal.status()
}

#[tauri::command]
fn start_codex_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_root: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<terminal::CodexRuntimeStatus, String> {
    state
        .terminal
        .start(app, Path::new(&workspace_root), cols, rows)
}

#[tauri::command]
fn write_codex_terminal(state: State<'_, AppState>, data: String) -> Result<(), String> {
    state.terminal.write(&data)
}

#[tauri::command]
fn resize_codex_terminal(state: State<'_, AppState>, cols: u16, rows: u16) -> Result<(), String> {
    state.terminal.resize(cols, rows)
}

#[tauri::command]
fn stop_codex_terminal(state: State<'_, AppState>) -> Result<(), String> {
    state.terminal.stop()
}

#[tauri::command]
fn set_codex_paper_context(
    workspace_root: String,
    paper_path: String,
    paper_id: String,
    title: String,
    authors: Option<String>,
    year: Option<u16>,
    collections: Vec<String>,
) -> Result<String, String> {
    terminal::write_current_context(
        Path::new(&workspace_root),
        Path::new(&paper_path),
        &paper_id,
        &title,
        authors.as_deref(),
        year,
        &collections,
    )
    .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn clear_codex_paper_context(workspace_root: String) -> Result<(), String> {
    terminal::clear_current_context(Path::new(&workspace_root))
}

#[tauri::command]
fn get_codex_paper_list(workspace_root: String) -> Result<Option<mcp::PresentedPaperList>, String> {
    mcp::read_presented_list(Path::new(&workspace_root))
}

#[tauri::command]
fn clear_codex_paper_list(workspace_root: String) -> Result<(), String> {
    mcp::clear_presented_list(Path::new(&workspace_root))
}

#[tauri::command]
fn persist_codex_paper_list(workspace_root: String, destination: String) -> Result<String, String> {
    mcp::persist_presented_list(Path::new(&workspace_root), &destination)
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn library_change_token(root: String) -> Result<LibraryChangeToken, String> {
    let root = normalize_library_root(Path::new(&root))?;
    compute_library_change_token(&root)
}

fn vscode_workspace_url(root: &Path) -> Result<String, String> {
    let file_url = tauri::Url::from_directory_path(root)
        .map_err(|_| "ワークスペースのVS Code URLを作成できませんでした".to_string())?;
    let suffix = file_url
        .as_str()
        .strip_prefix("file://")
        .ok_or_else(|| "ワークスペースのfile URLを作成できませんでした".to_string())?;
    let suffix = if suffix.starts_with('/') {
        suffix.to_string()
    } else {
        format!("/{suffix}")
    };
    Ok(format!("vscode://file{suffix}"))
}

fn authorized_paper_path(state: &State<'_, AppState>, requested: &str) -> Result<PathBuf, String> {
    let root = state
        .active_root
        .lock()
        .map_err(|_| "ライブラリ状態を読み取れませんでした".to_string())?
        .clone()
        .ok_or_else(|| "先にPaperpileライブラリを読み込んでください".to_string())?;
    let path =
        fs::canonicalize(requested).map_err(|_| "指定されたPDFが見つかりません".to_string())?;
    if !path.starts_with(&root) || !is_pdf(&path) {
        return Err("Paperpileライブラリ外のファイルは開けません".to_string());
    }
    Ok(path)
}

pub fn detect_paperpile_roots() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            let drive_path = Path::new(&drive);
            if !drive_path.is_dir() {
                continue;
            }
            for relative in [
                "マイドライブ\\Paperpile",
                "My Drive\\Paperpile",
                "Google Drive\\Paperpile",
                "Paperpile",
            ] {
                let candidate = drive_path.join(relative);
                if is_paperpile_root(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let volumes = Path::new("/Volumes");
        if let Ok(entries) = fs::read_dir(volumes) {
            for entry in entries.flatten() {
                for relative in ["My Drive/Paperpile", "マイドライブ/Paperpile", "Paperpile"]
                {
                    let candidate = entry.path().join(relative);
                    if is_paperpile_root(&candidate) {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            for relative in [
                "Google Drive/My Drive/Paperpile",
                "GoogleDrive/My Drive/Paperpile",
                "マイドライブ/Paperpile",
            ] {
                let candidate = home.join(relative);
                if is_paperpile_root(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn drive_label(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        path.components()
            .next()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_else(|| "Drive".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.components()
            .next_back()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_else(|| "Drive".to_string())
    }
}

fn is_paperpile_root(path: &Path) -> bool {
    path.is_dir() && path.join("All Papers").is_dir()
}

pub fn normalize_library_root(path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.file_name().is_some_and(|name| name == "All Papers") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    if !is_paperpile_root(candidate) {
        return Err("「All Papers」を含むPaperpileフォルダを選択してください".to_string());
    }
    fs::canonicalize(candidate)
        .map_err(|error| format!("Paperpileフォルダを読み取れませんでした: {error}"))
}

pub fn build_index(root: &Path) -> Result<LibraryIndex, String> {
    let all_papers = root.join("All Papers");
    let starred_papers = root.join("Starred Papers");
    let starred_keys = if starred_papers.is_dir() {
        scan_keys(&starred_papers)
    } else {
        HashSet::new()
    };

    let mut merged: BTreeMap<String, PaperBuilder> = BTreeMap::new();
    let mut warnings = Vec::new();

    for entry in walker(&all_papers)
        .filter_map(Result::ok)
        .filter(is_pdf_entry)
    {
        let path = entry.into_path();
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let relative = path.strip_prefix(&all_papers).unwrap_or(&path);
        let collection = collection_name(relative);
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let key = paper_metadata_fingerprint(&file_name, metadata.len());
        let identity_source = "paperpile-metadata".to_string();
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64);
        let relative_path = relative.to_string_lossy().into_owned();
        let is_starred = starred_keys.contains(&key);
        let stable_id = format!("p2-{}", &blake3::hash(key.as_bytes()).to_hex()[..20]);

        let paper = merged.entry(key).or_insert_with(|| PaperBuilder {
            id: stable_id,
            identity_source,
            path: path.clone(),
            relative_path,
            file_name,
            size_bytes: metadata.len(),
            modified_at,
            collections: BTreeSet::new(),
            starred: is_starred,
        });
        if !collection.is_empty() {
            paper.collections.insert(collection);
        }
        paper.starred |= is_starred;
    }

    let mut papers: Vec<PaperRecord> = merged
        .into_values()
        .map(|paper| {
            let (authors, year, title) = parse_paperpile_filename(&paper.file_name);
            let legacy_id = blake3::hash(paper.relative_path.as_bytes()).to_hex()[..16].to_string();
            PaperRecord {
                id: paper.id,
                legacy_id,
                identity_source: paper.identity_source,
                title,
                authors,
                year,
                collections: paper.collections.into_iter().collect(),
                path: paper.path.to_string_lossy().into_owned(),
                relative_path: paper.relative_path,
                file_name: paper.file_name,
                size_bytes: paper.size_bytes,
                modified_at: paper.modified_at,
                starred: paper.starred,
            }
        })
        .collect();

    papers.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
    });

    let collections: Vec<String> = papers
        .iter()
        .flat_map(|paper| paper.collections.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let stats = LibraryStats {
        paper_count: papers.len(),
        collection_count: collections.len(),
        starred_count: papers.iter().filter(|paper| paper.starred).count(),
        total_bytes: papers.iter().map(|paper| paper.size_bytes).sum(),
    };

    Ok(LibraryIndex {
        root: root.to_string_lossy().into_owned(),
        papers,
        collections,
        stats,
        warnings,
    })
}

fn walker(root: &Path) -> impl Iterator<Item = walkdir::Result<DirEntry>> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
}

fn is_pdf_entry(entry: &DirEntry) -> bool {
    entry.file_type().is_file() && is_pdf(entry.path())
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn scan_keys(root: &Path) -> HashSet<String> {
    walker(root)
        .filter_map(Result::ok)
        .filter(is_pdf_entry)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy();
            Some(paper_metadata_fingerprint(&name, metadata.len()))
        })
        .collect()
}

fn paper_metadata_fingerprint(file_name: &str, size_bytes: u64) -> String {
    let (authors, year, title) = parse_paperpile_filename(file_name);
    let normalized = format!(
        "{}\0{}\0{}\0{size_bytes}",
        authors.unwrap_or_default().to_lowercase(),
        year.map(|value| value.to_string()).unwrap_or_default(),
        title.to_lowercase()
    );
    blake3::hash(normalized.as_bytes()).to_hex().to_string()
}

fn compute_library_change_token(root: &Path) -> Result<LibraryChangeToken, String> {
    let all_papers = root.join("All Papers");
    let mut records = Vec::new();
    for entry in walker(&all_papers)
        .filter_map(Result::ok)
        .filter(is_pdf_entry)
    {
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        let relative = entry
            .path()
            .strip_prefix(&all_papers)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .into_owned();
        records.push((relative, metadata.len(), modified));
    }
    records.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = blake3::Hasher::new();
    for (relative, size, modified) in &records {
        hasher.update(relative.as_bytes());
        hasher.update(&size.to_le_bytes());
        hasher.update(&modified.to_le_bytes());
    }
    Ok(LibraryChangeToken {
        token: hasher.finalize().to_hex().to_string(),
        paper_count: records.len(),
        checked_at: std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64,
    })
}

fn collection_name(relative_path: &Path) -> String {
    relative_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| {
            parent
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default()
}

fn parse_paperpile_filename(file_name: &str) -> (Option<String>, Option<u16>, String) {
    let stem = file_name
        .strip_suffix(".pdf")
        .or_else(|| file_name.strip_suffix(".PDF"))
        .unwrap_or(file_name)
        .trim();
    let pattern =
        Regex::new(r"^(?P<authors>.+?)\s+(?P<year>(?:18|19|20)\d{2})[a-z]?\s+-\s+(?P<title>.+)$")
            .expect("valid filename regex");
    if let Some(captures) = pattern.captures(stem) {
        let authors = captures
            .name("authors")
            .map(|value| value.as_str().trim().to_string());
        let year = captures
            .name("year")
            .and_then(|value| value.as_str().parse().ok());
        let title = captures
            .name("title")
            .map(|value| clean_title(value.as_str()))
            .unwrap_or_else(|| clean_title(stem));
        return (authors, year, title);
    }

    let title = stem
        .split_once(" - ")
        .map(|(_, right)| clean_title(right))
        .unwrap_or_else(|| clean_title(stem));
    (None, None, title)
}

fn clean_title(value: &str) -> String {
    value
        .strip_suffix(".pdf")
        .or_else(|| value.strip_suffix(".PDF"))
        .unwrap_or(value)
        .trim()
        .to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            detect_libraries,
            initialize_workspace,
            open_workspace,
            suggest_organization,
            scan_library,
            open_paper,
            reveal_paper,
            open_workspace_in_vscode,
            codex_runtime_status,
            start_codex_terminal,
            write_codex_terminal,
            resize_codex_terminal,
            stop_codex_terminal,
            set_codex_paper_context,
            clear_codex_paper_context,
            get_codex_paper_list,
            clear_codex_paper_list,
            persist_codex_paper_list,
            library_change_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bukan");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_paperpile_filename() {
        let (authors, year, title) =
            parse_paperpile_filename("Alemi 2020 - The Amazing Journey of Reason.pdf");
        assert_eq!(authors.as_deref(), Some("Alemi"));
        assert_eq!(year, Some(2020));
        assert_eq!(title, "The Amazing Journey of Reason");
    }

    #[test]
    fn uses_second_half_for_legacy_duplicate_filename() {
        let (_, year, title) = parse_paperpile_filename("12901.pdf - Lunar dust.pdf");
        assert_eq!(year, None);
        assert_eq!(title, "Lunar dust");
    }

    #[test]
    fn indexes_and_merges_duplicate_collection_entries() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path();
        let collection_a = root.join("All Papers").join("Methods");
        let collection_b = root.join("All Papers").join("Favorite");
        let starred = root.join("Starred Papers");
        fs::create_dir_all(&collection_a).expect("collection a");
        fs::create_dir_all(&collection_b).expect("collection b");
        fs::create_dir_all(&starred).expect("starred");
        let name = "Sato 2025 - Lunar plasma.pdf";
        fs::write(collection_a.join(name), b"%PDF-test").expect("paper a");
        fs::write(collection_b.join(name), b"%PDF-test").expect("paper b");
        fs::write(starred.join(name), b"%PDF-test").expect("starred paper");

        let index = build_index(root).expect("index");
        assert_eq!(index.papers.len(), 1);
        assert_eq!(index.papers[0].collections, vec!["Favorite", "Methods"]);
        assert!(index.papers[0].starred);
        assert_eq!(index.stats.starred_count, 1);
    }

    #[test]
    fn keeps_stable_id_when_a_paper_moves_between_collections() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path();
        let original_directory = root.join("All Papers").join("Old collection");
        let moved_directory = root.join("All Papers").join("New collection");
        fs::create_dir_all(&original_directory).expect("original collection");
        fs::create_dir_all(&moved_directory).expect("new collection");
        let original = original_directory.join("Sato 2025 - Lunar plasma.pdf");
        let moved = moved_directory.join("Sato 2025 - Lunar plasma.pdf");
        fs::write(&original, b"%PDF-stable-identity-test").expect("paper");

        let before = build_index(root).expect("index before rename");
        fs::rename(&original, &moved).expect("move paper");
        let after = build_index(root).expect("index after rename");

        assert_eq!(before.papers.len(), 1);
        assert_eq!(before.papers[0].id, after.papers[0].id);
        assert_ne!(before.papers[0].legacy_id, after.papers[0].legacy_id);
        assert_eq!(after.papers[0].identity_source, "paperpile-metadata");
    }

    #[test]
    fn library_change_token_detects_metadata_changes() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path();
        let all_papers = root.join("All Papers");
        fs::create_dir_all(&all_papers).expect("all papers");
        fs::write(all_papers.join("One.pdf"), b"%PDF-one").expect("first paper");

        let before = compute_library_change_token(root).expect("token before");
        fs::write(all_papers.join("Two.pdf"), b"%PDF-two").expect("second paper");
        let after_add = compute_library_change_token(root).expect("token after add");
        fs::rename(all_papers.join("Two.pdf"), all_papers.join("Renamed.pdf"))
            .expect("rename paper");
        let after_rename = compute_library_change_token(root).expect("token after rename");

        assert_eq!(before.paper_count, 1);
        assert_eq!(after_add.paper_count, 2);
        assert_ne!(before.token, after_add.token);
        assert_ne!(after_add.token, after_rename.token);
    }

    #[test]
    fn creates_encoded_vscode_workspace_url() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path().join("研究 workspace");
        fs::create_dir_all(&root).expect("workspace directory");

        let url = vscode_workspace_url(&root).expect("VS Code URL");

        assert!(url.starts_with("vscode://file/"));
        assert!(url.contains("%20"));
        assert!(!url.contains(' '));
    }

    #[test]
    #[ignore = "requires a mounted Paperpile library"]
    fn reports_mounted_paperpile_library() {
        let roots = detect_paperpile_roots();
        assert!(
            !roots.is_empty(),
            "no mounted Paperpile library was detected"
        );
        for root in roots {
            let index = build_index(&root).expect("mounted Paperpile index");
            println!(
                "{}: {} papers, {} collections, {} starred, {} warnings",
                index.root,
                index.stats.paper_count,
                index.stats.collection_count,
                index.stats.starred_count,
                index.warnings.len()
            );
        }
    }
}
