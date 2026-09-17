use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{detect_paperpile_roots, normalize_library_root};

pub const WORKSPACE_VERSION: u32 = 1;

const WORKSPACE_DIRECTORIES: &[&str] = &[
    "data",
    "notes",
    "queries",
    "candidates",
    "reports",
    "imports",
    "cache",
    ".agents/skills/bukan-paper-review/references",
];
const WORKSPACE_TEMPLATES: &[(&str, &str)] = &[
    (
        "AGENTS.md",
        include_str!("../../../templates/workspace/AGENTS.md"),
    ),
    (
        "taxonomy.toml",
        include_str!("../../../templates/workspace/taxonomy.toml"),
    ),
    (
        ".gitignore",
        include_str!("../../../templates/workspace/.gitignore"),
    ),
    (
        ".agents/skills/bukan-paper-review/SKILL.md",
        include_str!("../../../templates/workspace/.agents/skills/bukan-paper-review/SKILL.md"),
    ),
    (
        ".agents/skills/bukan-paper-review/references/full-review.md",
        include_str!(
            "../../../templates/workspace/.agents/skills/bukan-paper-review/references/full-review.md"
        ),
    ),
    (
        ".agents/skills/bukan-paper-review/references/synthesis.md",
        include_str!(
            "../../../templates/workspace/.agents/skills/bukan-paper-review/references/synthesis.md"
        ),
    ),
    (
        ".agents/skills/bukan-paper-review/references/acquisition.md",
        include_str!(
            "../../../templates/workspace/.agents/skills/bukan-paper-review/references/acquisition.md"
        ),
    ),
    (
        ".agents/skills/bukan-paper-review/references/preview.md",
        include_str!(
            "../../../templates/workspace/.agents/skills/bukan-paper-review/references/preview.md"
        ),
    ),
    (
        ".agents/skills/bukan-paper-review/references/tools.md",
        include_str!(
            "../../../templates/workspace/.agents/skills/bukan-paper-review/references/tools.md"
        ),
    ),
];

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
    pub paperpile_error: Option<String>,
}

pub fn init_workspace(
    root: &Path,
    name: Option<&str>,
    paperpile_path: Option<&str>,
) -> Result<WorkspaceDescriptor, String> {
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(root)
    };
    let destination = crate::storage::resolve_destination(&absolute)?;
    crate::storage::validate_storage_location(&destination)?;
    let root = destination.as_path();
    if root.join("bukan.toml").exists() {
        return Err("このフォルダはすでにBukanワークスペースです".to_string());
    }

    let default_name = absolute
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

    let paperpile = paperpile_path
        .filter(|path| !path.eq_ignore_ascii_case("auto"))
        .and_then(|path| crate::storage::resolve_destination(&root.join(path)).ok());
    // Check every destination before creating anything, including nested links.
    for (relative, directory) in WORKSPACE_DIRECTORIES
        .iter()
        .map(|path| (*path, true))
        .chain(WORKSPACE_TEMPLATES.iter().map(|(path, _)| (*path, false)))
        .chain(std::iter::once(("bukan.toml", false)))
    {
        let path = crate::storage::resolve_destination(&root.join(relative))?;
        if !path.starts_with(root) {
            return Err("初期化するファイルの保存先はワークスペース内にしてください".into());
        }
        crate::storage::validate_storage_location(&path)?;
        if paperpile
            .as_ref()
            .is_some_and(|source| crate::storage::path_is_within(&path, source))
        {
            return Err("Paperpile内にはワークスペースを作成できません".into());
        }
        if path.exists()
            && !(if directory {
                path.is_dir()
            } else {
                path.is_file()
            })
        {
            return Err(format!("{relative} に異なる種類のファイルが存在します"));
        }
    }

    fs::create_dir_all(root)
        .map_err(|error| format!("ワークスペースを作成できませんでした: {error}"))?;
    for directory in WORKSPACE_DIRECTORIES {
        fs::create_dir_all(root.join(directory))
            .map_err(|error| format!("{directory} を作成できませんでした: {error}"))?;
    }
    for (relative, contents) in WORKSPACE_TEMPLATES {
        write_if_absent(&root.join(relative), contents)?;
    }
    write_new(&root.join("bukan.toml"), &config_text)?;

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
    // Saved research remains usable while Drive is disconnected.
    let (paperpile_root, paperpile_error) = match resolve_paperpile_root(&root, &config) {
        Ok(root) => (root, None),
        Err(error) => (None, Some(error)),
    };
    Ok(WorkspaceDescriptor {
        root: root.to_string_lossy().into_owned(),
        name: config.name,
        version: config.version,
        paperpile_root: paperpile_root.map(|path| path.to_string_lossy().into_owned()),
        paperpile_mode: config.paperpile.path,
        paperpile_error,
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
        for (relative, contents) in WORKSPACE_TEMPLATES {
            assert_eq!(fs::read_to_string(root.join(relative)).unwrap(), *contents);
        }
        let (_, config) = load_workspace(&root).expect("load workspace");
        assert!(config.paperpile.read_only);
    }

    #[test]
    fn preserves_custom_instructions_and_research_when_initializing() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        let custom_files = [
            "AGENTS.md",
            ".gitignore",
            "taxonomy.toml",
            ".agents/skills/bukan-paper-review/SKILL.md",
            ".agents/skills/bukan-paper-review/references/full-review.md",
            "notes/custom.md",
        ];
        for relative in custom_files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("custom: {relative}")).unwrap();
        }
        init_workspace(&root, None, None).unwrap();
        for relative in custom_files {
            assert_eq!(
                fs::read_to_string(root.join(relative)).unwrap(),
                format!("custom: {relative}")
            );
        }
        assert!(root
            .join(".agents/skills/bukan-paper-review/references/synthesis.md")
            .is_file());
    }

    #[test]
    fn refuses_to_overwrite_existing_workspace() {
        let temporary = tempfile::tempdir().expect("tempdir");
        init_workspace(temporary.path(), None, None).expect("first init");
        let skill = temporary
            .path()
            .join(".agents/skills/bukan-paper-review/SKILL.md");
        fs::remove_file(&skill).unwrap();
        fs::write(temporary.path().join("AGENTS.md"), "custom instructions").unwrap();
        let config = fs::read(temporary.path().join("bukan.toml")).unwrap();
        describe_workspace(temporary.path()).unwrap();
        let error = init_workspace(temporary.path(), None, None).expect_err("second init fails");
        assert!(error.contains("すでに"));
        assert!(!skill.exists());
        assert_eq!(
            fs::read(temporary.path().join("bukan.toml")).unwrap(),
            config
        );
        assert_eq!(
            fs::read_to_string(temporary.path().join("AGENTS.md")).unwrap(),
            "custom instructions"
        );
    }

    #[cfg(any(unix, windows))]
    fn link_directory(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(target.to_string_lossy().replace('/', "\\"))
                .creation_flags(0x0800_0000)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn refuses_linked_directories_outside_workspace_before_writing() {
        for relative in [
            ".agents",
            ".agents/skills/bukan-paper-review/references",
            "data",
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("workspace");
            let outside = temporary.path().join("outside");
            fs::create_dir_all(&outside).unwrap();
            fs::write(outside.join("keep.md"), "original").unwrap();
            let link = root.join(relative);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            link_directory(&outside, &link);

            assert!(init_workspace(&root, None, None).is_err());
            assert!(!root.join("bukan.toml").exists());
            assert!(!root.join("AGENTS.md").exists());
            assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
            assert_eq!(
                fs::read_to_string(outside.join("keep.md")).unwrap(),
                "original"
            );
        }
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn refuses_nested_links_into_configured_paperpile() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        let source = root.join("synced-library");
        fs::create_dir_all(&source).unwrap();
        let link = root.join(".agents/skills/bukan-paper-review/references");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        link_directory(&source, &link);

        let error = init_workspace(&root, None, Some("synced-library")).unwrap_err();
        assert!(error.contains("Paperpile"));
        assert!(!root.join("bukan.toml").exists());
        assert_eq!(fs::read_dir(&source).unwrap().count(), 0);
    }

    #[test]
    fn rejects_conflicting_skill_path_before_writing() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        fs::create_dir_all(root.join(".agents/skills/bukan-paper-review/SKILL.md")).unwrap();
        assert!(init_workspace(&root, None, None).is_err());
        assert!(!root.join("bukan.toml").exists());
        assert!(!root.join("data").exists());
    }

    #[test]
    fn refuses_to_initialize_inside_source_or_paperpile() {
        let temporary = tempfile::tempdir().unwrap();
        let library = temporary.path().join("Paperpile");
        fs::create_dir_all(library.join("All Papers")).unwrap();
        assert!(init_workspace(&library.join("research"), None, None).is_err());
        assert!(!library.join("research").exists());
        let repo = temporary.path().join("app");
        fs::create_dir_all(repo.join("src-tauri")).unwrap();
        fs::write(repo.join("src-tauri/Cargo.toml"), "[package]").unwrap();
        assert!(init_workspace(&repo.join("research"), None, None).is_err());
        assert!(!repo.join("research").exists());
    }

    #[test]
    fn opens_saved_research_while_paperpile_is_offline() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("research");
        let missing = temporary.path().join("missing-drive/Paperpile");
        let descriptor = init_workspace(&root, None, Some(&missing.to_string_lossy())).unwrap();
        assert!(descriptor.paperpile_root.is_none());
        assert!(descriptor.paperpile_error.is_some());
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
