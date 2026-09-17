use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::workspace;

const COLLECTIONS_VERSION: u32 = 1;
const COLLECTIONS_FILE: &str = "data/collections.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperCollectionSeed {
    pub paper_id: String,
    pub collections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCollection {
    pub path: String,
    pub paper_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCollectionIndex {
    pub version: u32,
    pub initialized_from: String,
    pub collections: Vec<WorkspaceCollection>,
}

pub fn load(workspace_root: &Path) -> Result<WorkspaceCollectionIndex, String> {
    let path = collections_path(workspace_root)?;
    if path.exists() {
        read_index(&path)
    } else {
        Ok(index_from_assignments("workspace", BTreeMap::new()))
    }
}

pub fn load_or_initialize(
    workspace_root: &Path,
    seeds: &[PaperCollectionSeed],
) -> Result<WorkspaceCollectionIndex, String> {
    let path = collections_path(workspace_root)?;
    if path.exists() {
        return read_index(&path);
    }

    let mut assignments = BTreeMap::<String, BTreeSet<String>>::new();
    for seed in seeds {
        validate_paper_id(&seed.paper_id)?;
        for collection in &seed.collections {
            let Some(path) = normalize_collection_path(collection)? else {
                continue;
            };
            assignments
                .entry(path)
                .or_default()
                .insert(seed.paper_id.clone());
        }
    }
    let index = index_from_assignments("paperpile", assignments);
    write_new_index(&path, &index)?;
    read_index(&path)
}

/// Import source assignments once; saved workspace collections remain usable offline.
pub fn initialize_from_paperpile(
    workspace_root: &Path,
) -> Result<WorkspaceCollectionIndex, String> {
    let path = collections_path(workspace_root)?;
    if path.exists() {
        return read_index(&path);
    }
    let (root, config) = workspace::load_workspace(workspace_root)?;
    let library = workspace::resolve_paperpile_root(&root, &config)
        .map_err(|error| format!("Connect the Paperpile library before creating the first workspace collection: {error}"))?
        .ok_or_else(|| "Connect the Paperpile library before creating the first workspace collection.".to_string())?;
    load_or_initialize(&root, &paperpile_seeds(&library)?)
}

fn paperpile_seeds(library: &Path) -> Result<Vec<PaperCollectionSeed>, String> {
    let index = crate::build_index(library)?;
    if !index.warnings.is_empty() {
        return Err(format!(
            "Could not read the complete Paperpile library; no collection import was saved. Restore source access and retry:\n{}",
            index.warnings.join("\n")
        ));
    }
    Ok(index
        .papers
        .into_iter()
        .map(|paper| PaperCollectionSeed {
            paper_id: paper.id,
            collections: paper.collections,
        })
        .collect())
}

pub fn create_collection(
    workspace_root: &Path,
    collection_path: &str,
) -> Result<WorkspaceCollectionIndex, String> {
    let path = collections_path(workspace_root)?;
    let mut index = read_existing_index(&path)?;
    let normalized = normalize_collection_path(collection_path)?
        .ok_or_else(|| "コレクション名を入力してください".to_string())?;
    if !index
        .collections
        .iter()
        .any(|collection| collection.path == normalized)
    {
        index.collections.push(WorkspaceCollection {
            path: normalized,
            paper_ids: Vec::new(),
        });
        sort_index(&mut index);
        write_index(&path, &index)?;
    }
    Ok(index)
}

pub fn set_membership(
    workspace_root: &Path,
    collection_path: &str,
    paper_id: &str,
    member: bool,
) -> Result<WorkspaceCollectionIndex, String> {
    validate_paper_id(paper_id)?;
    let path = collections_path(workspace_root)?;
    let mut index = read_existing_index(&path)?;
    let normalized = normalize_collection_path(collection_path)?
        .ok_or_else(|| "コレクション名を入力してください".to_string())?;
    let collection = index
        .collections
        .iter_mut()
        .find(|collection| collection.path == normalized)
        .ok_or_else(|| format!("Workspaceコレクションが見つかりません: {normalized}"))?;
    if member {
        if !collection.paper_ids.iter().any(|id| id == paper_id) {
            collection.paper_ids.push(paper_id.to_string());
        }
    } else {
        collection.paper_ids.retain(|id| id != paper_id);
    }
    collection.paper_ids.sort();
    write_index(&path, &index)?;
    Ok(index)
}

fn collections_path(workspace_root: &Path) -> Result<PathBuf, String> {
    let (root, _) = workspace::load_workspace(workspace_root)?;
    crate::storage::validate_workspace_destination(&root, &root.join(COLLECTIONS_FILE))
}

fn read_existing_index(path: &Path) -> Result<WorkspaceCollectionIndex, String> {
    if !path.exists() {
        return Err(
            "Workspaceコレクションが未初期化です。先にコレクションを作成してください".to_string(),
        );
    }
    read_index(path)
}

fn read_index(path: &Path) -> Result<WorkspaceCollectionIndex, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Workspaceコレクションを読み取れませんでした: {error}"))?;
    let mut index: WorkspaceCollectionIndex = serde_json::from_str(&text)
        .map_err(|error| format!("Workspaceコレクションの形式が不正です: {error}"))?;
    if index.version > COLLECTIONS_VERSION {
        return Err(format!(
            "Workspaceコレクションは新しい形式です (version {})",
            index.version
        ));
    }
    for collection in &mut index.collections {
        collection.path = normalize_collection_path(&collection.path)?
            .ok_or_else(|| "空のWorkspaceコレクションがあります".to_string())?;
        collection.paper_ids.sort();
        collection.paper_ids.dedup();
        for paper_id in &collection.paper_ids {
            validate_paper_id(paper_id)?;
        }
    }
    sort_index(&mut index);
    Ok(index)
}

fn write_new_index(path: &Path, index: &WorkspaceCollectionIndex) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Workspaceのdataフォルダを作成できませんでした: {error}"))?;
    }
    let contents = serialize_index(index)?;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file
            .write_all(contents.as_bytes())
            .map_err(|error| format!("Workspaceコレクションを保存できませんでした: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another process initialized it first. Its independent state is authoritative.
            read_index(path).map(|_| ())
        }
        Err(error) => Err(format!(
            "Workspaceコレクションを作成できませんでした: {error}"
        )),
    }
}

fn write_index(path: &Path, index: &WorkspaceCollectionIndex) -> Result<(), String> {
    let contents = serialize_index(index)?;
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("Workspaceコレクションを開けませんでした: {error}"))?;
    file.write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Workspaceコレクションを保存できませんでした: {error}"))
}

fn serialize_index(index: &WorkspaceCollectionIndex) -> Result<String, String> {
    serde_json::to_string_pretty(index)
        .map(|text| format!("{text}\n"))
        .map_err(|error| format!("Workspaceコレクションを生成できませんでした: {error}"))
}

fn index_from_assignments(
    initialized_from: &str,
    assignments: BTreeMap<String, BTreeSet<String>>,
) -> WorkspaceCollectionIndex {
    WorkspaceCollectionIndex {
        version: COLLECTIONS_VERSION,
        initialized_from: initialized_from.to_string(),
        collections: assignments
            .into_iter()
            .map(|(path, paper_ids)| WorkspaceCollection {
                path,
                paper_ids: paper_ids.into_iter().collect(),
            })
            .collect(),
    }
}

fn sort_index(index: &mut WorkspaceCollectionIndex) {
    index
        .collections
        .sort_by(|left, right| left.path.cmp(&right.path));
}

fn normalize_collection_path(value: &str) -> Result<Option<String>, String> {
    let mut parts = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("my papers"))
    {
        parts.remove(0);
    }
    if parts.is_empty() {
        return Ok(None);
    }
    if value.len() > 512 || parts.len() > 24 {
        return Err("コレクションのパスが長すぎます".to_string());
    }
    for part in &parts {
        if *part == "." || *part == ".." || part.chars().any(char::is_control) {
            return Err(format!("使用できないコレクション名です: {part}"));
        }
    }
    Ok(Some(parts.join(" / ")))
}

fn validate_paper_id(paper_id: &str) -> Result<(), String> {
    if paper_id.is_empty()
        || paper_id.len() > 160
        || !paper_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_:.".contains(character))
    {
        return Err("文献IDの形式が不正です".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_source_traversal_cannot_be_saved_as_an_empty_import() {
        let temporary = tempfile::tempdir().unwrap();
        // A Drive can disappear after initial path validation but before traversal.
        let source = temporary.path().join("disconnected-source");
        let index = crate::build_index(&source).unwrap();
        assert!(index.papers.is_empty());
        assert!(!index.warnings.is_empty());
        let error = paperpile_seeds(&source).unwrap_err();
        assert!(error.contains("no collection import was saved"));
    }

    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path().join("workspace");
        workspace::init_workspace(&root, None, None).expect("workspace");
        (temporary, root)
    }

    #[test]
    fn listing_absent_collections_does_not_initialize_them() {
        let (_temporary, root) = workspace();
        assert!(load(&root).unwrap().collections.is_empty());
        assert!(!root.join(COLLECTIONS_FILE).exists());
    }

    #[test]
    fn copies_paperpile_collections_only_on_first_initialization() {
        let (_temporary, root) = workspace();
        let first = load_or_initialize(
            &root,
            &[PaperCollectionSeed {
                paper_id: "paper-1".to_string(),
                collections: vec!["My Papers / Biology / Cells".to_string()],
            }],
        )
        .expect("initialize");
        assert_eq!(first.collections[0].path, "Biology / Cells");

        let second = load_or_initialize(
            &root,
            &[PaperCollectionSeed {
                paper_id: "paper-2".to_string(),
                collections: vec!["Physics".to_string()],
            }],
        )
        .expect("load without overwriting");
        assert_eq!(second, first);
    }

    #[test]
    fn creates_nested_collections_and_updates_membership() {
        let (_temporary, root) = workspace();
        load_or_initialize(&root, &[]).expect("initialize");
        let created = create_collection(&root, "Review / Methods").expect("create");
        assert_eq!(created.collections[0].path, "Review / Methods");
        let updated =
            set_membership(&root, "Review / Methods", "paper-1", true).expect("membership");
        assert_eq!(updated.collections[0].paper_ids, vec!["paper-1"]);
        let removed = set_membership(&root, "Review / Methods", "paper-1", false).expect("remove");
        assert!(removed.collections[0].paper_ids.is_empty());
    }

    #[test]
    fn rejects_unsafe_collection_paths() {
        let (_temporary, root) = workspace();
        load_or_initialize(&root, &[]).expect("initialize");
        let error = create_collection(&root, "../escaped").expect_err("unsafe path");
        assert!(error.contains("使用できない"));
    }
}
