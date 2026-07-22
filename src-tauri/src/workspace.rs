use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{detect_paperpile_roots, normalize_library_root};

pub const WORKSPACE_VERSION: u32 = 1;

const AGENTS_TEMPLATE: &str = include_str!("../../templates/workspace/AGENTS.md");
const TAXONOMY_TEMPLATE: &str = include_str!("../../templates/workspace/taxonomy.toml");
const GITIGNORE_TEMPLATE: &str = include_str!("../../templates/workspace/.gitignore");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub name: String,
    pub paperpile: PaperpileConfig,
    pub bibliography: BibliographyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperpileConfig {
    pub path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibliographyConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDescriptor {
    pub root: String,
    pub name: String,
    pub version: u32,
    pub paperpile_root: Option<String>,
    pub paperpile_mode: String,
}

pub fn init_workspace(
    root: &Path,
    name: Option<&str>,
    paperpile_path: Option<&str>,
) -> Result<WorkspaceDescriptor, String> {
    if root.join("bukan.toml").exists() {
        return Err("このフォルダはすでにBukanワークスペースです".to_string());
    }

    fs::create_dir_all(root)
        .map_err(|error| format!("ワークスペースを作成できませんでした: {error}"))?;
    for directory in [
        "data",
        "notes",
        "queries",
        "candidates",
        "reports",
        "imports",
        "cache",
    ] {
        fs::create_dir_all(root.join(directory))
            .map_err(|error| format!("{directory} を作成できませんでした: {error}"))?;
    }

    let default_name = root
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Literature Workspace".to_string());
    let config = WorkspaceConfig {
        version: WORKSPACE_VERSION,
        name: name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&default_name)
            .to_string(),
        paperpile: PaperpileConfig {
            path: paperpile_path.unwrap_or("auto").to_string(),
            read_only: true,
        },
        bibliography: BibliographyConfig {
            path: "data/paperpile.bib".to_string(),
        },
    };
    let config_text = toml::to_string_pretty(&config)
        .map_err(|error| format!("ワークスペース設定を生成できませんでした: {error}"))?;
    write_new(&root.join("bukan.toml"), &config_text)?;
    write_if_absent(&root.join("taxonomy.toml"), TAXONOMY_TEMPLATE)?;
    write_if_absent(&root.join("AGENTS.md"), AGENTS_TEMPLATE)?;
    write_if_absent(&root.join(".gitignore"), GITIGNORE_TEMPLATE)?;

    describe_workspace(root)
}

pub fn load_workspace(root: &Path) -> Result<(PathBuf, WorkspaceConfig), String> {
    let root = fs::canonicalize(root)
        .map_err(|error| format!("ワークスペースを開けませんでした: {error}"))?;
    let config_path = root.join("bukan.toml");
    let text = fs::read_to_string(&config_path).map_err(|_| {
        "bukan.toml が見つかりません。先に bukan init を実行してください".to_string()
    })?;
    let config: WorkspaceConfig = toml::from_str(&text)
        .map_err(|error| format!("bukan.toml を読み取れませんでした: {error}"))?;
    if config.version > WORKSPACE_VERSION {
        return Err(format!(
            "このワークスペースは新しい形式です (version {})",
            config.version
        ));
    }
    if !config.paperpile.read_only {
        return Err("安全のため paperpile.read_only は true にしてください".to_string());
    }
    Ok((root, config))
}

pub fn describe_workspace(root: &Path) -> Result<WorkspaceDescriptor, String> {
    let (root, config) = load_workspace(root)?;
    let paperpile_root = resolve_paperpile_root(&root, &config)?;
    Ok(WorkspaceDescriptor {
        root: root.to_string_lossy().into_owned(),
        name: config.name,
        version: config.version,
        paperpile_root: paperpile_root.map(|path| path.to_string_lossy().into_owned()),
        paperpile_mode: config.paperpile.path,
    })
}

pub fn resolve_paperpile_root(
    workspace_root: &Path,
    config: &WorkspaceConfig,
) -> Result<Option<PathBuf>, String> {
    if config.paperpile.path.eq_ignore_ascii_case("auto") {
        return Ok(detect_paperpile_roots().into_iter().next());
    }
    let configured = PathBuf::from(&config.paperpile.path);
    let candidate = if configured.is_absolute() {
        configured
    } else {
        workspace_root.join(configured)
    };
    normalize_library_root(&candidate).map(Some)
}

fn write_new(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("{} を作成できませんでした: {error}", path.display()))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("{} を保存できませんでした: {error}", path.display()))
}

fn write_if_absent(path: &Path, contents: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    write_new(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_workspace_without_mixing_application_data() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = temporary.path().join("lunar-research");
        let descriptor = init_workspace(&root, Some("Lunar Research"), Some("auto"))
            .expect("initialize workspace");

        assert_eq!(descriptor.name, "Lunar Research");
        assert!(root.join("bukan.toml").is_file());
        assert!(root.join("taxonomy.toml").is_file());
        assert!(root.join("notes").is_dir());
        assert!(root.join("cache").is_dir());
        let (_, config) = load_workspace(&root).expect("load workspace");
        assert!(config.paperpile.read_only);
    }

    #[test]
    fn refuses_to_overwrite_existing_workspace() {
        let temporary = tempfile::tempdir().expect("tempdir");
        init_workspace(temporary.path(), None, None).expect("first init");
        let error = init_workspace(temporary.path(), None, None).expect_err("second init fails");
        assert!(error.contains("すでに"));
    }

    #[test]
    fn resolves_explicit_paperpile_path() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let workspace_root = temporary.path().join("workspace");
        let paperpile_root = temporary.path().join("Paperpile");
        fs::create_dir_all(paperpile_root.join("All Papers")).expect("Paperpile tree");
        let descriptor = init_workspace(
            &workspace_root,
            None,
            Some(&paperpile_root.to_string_lossy()),
        )
        .expect("initialize workspace");
        let canonical_paperpile = fs::canonicalize(&paperpile_root).expect("canonical Paperpile");
        assert_eq!(
            descriptor.paperpile_root.as_deref(),
            Some(canonical_paperpile.to_string_lossy().as_ref())
        );
    }
}
