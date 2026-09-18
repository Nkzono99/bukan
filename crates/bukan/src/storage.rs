use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::workspace;

pub(crate) fn store_path(root: &Path) -> Result<PathBuf, String> {
    validate_workspace_destination(root, &root.join("data/research.sqlite"))
}

/// Validate the resolved destination, including existing symlinks and junctions,
/// before any workspace writer creates directories or modifies files.
pub(crate) fn validate_workspace_destination(
    workspace_root: &Path,
    path: &Path,
) -> Result<PathBuf, String> {
    let (root, config) = workspace::load_workspace(workspace_root)?;
    let path = resolve_destination(path)?;
    if !path.starts_with(&root) {
        return Err("研究データの保存先はワークスペース内にしてください".into());
    }
    validate_storage_location(&path)?;
    if !config.paperpile.path.eq_ignore_ascii_case("auto") {
        // An offline source is allowed, but never a destination under that source.
        if resolve_destination(&root.join(&config.paperpile.path))
            .is_ok_and(|source| path_is_within(&path, &source))
        {
            return Err("Paperpile内には研究データを保存できません".into());
        }
    }
    Ok(path)
}

/// Conservatively detect overlap with a protected source. Missing Windows path
/// components cannot be canonicalized but may still refer to the same directory
/// under a different case. Workspace containment uses resolved, exact paths.
pub(crate) fn path_is_within(path: &Path, root: &Path) -> bool {
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
        let mut components = path.components();
        root.components().all(|expected| {
            components.next().is_some_and(|actual| {
                let actual: Vec<u16> = actual.as_os_str().encode_wide().collect();
                let expected: Vec<u16> = expected.as_os_str().encode_wide().collect();
                // Both buffers remain valid for the explicit UTF-16 lengths.
                unsafe {
                    CompareStringOrdinal(
                        actual.as_ptr(),
                        actual.len() as i32,
                        expected.as_ptr(),
                        expected.len() as i32,
                        1,
                    ) == CSTR_EQUAL
                }
            })
        })
    }
}

pub(crate) fn validate_storage_location(path: &Path) -> Result<(), String> {
    for parent in path.ancestors() {
        if parent
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("paperpile"))
            || parent.join("All Papers").is_dir()
        {
            return Err("Paperpile内には研究データを保存できません".into());
        }
        if parent.join("crates/bukan/Cargo.toml").is_file()
            || parent.join("src-tauri/Cargo.toml").is_file()
        {
            return Err("研究データはBukanのソースリポジトリの外に保存してください".into());
        }
        let manifest = parent.join(".codex-plugin/plugin.json");
        if fs::read_to_string(manifest)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("name")
                    .and_then(|name| name.as_str())
                    .map(str::to_owned)
            })
            .is_some_and(|name| name.eq_ignore_ascii_case("bukan"))
        {
            return Err("研究データはBukanプラグインの外に保存してください".into());
        }
    }
    Ok(())
}

pub(crate) fn resolve_destination(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        let absolute = std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path);
        return resolve_destination(&absolute);
    }
    match fs::symlink_metadata(path) {
        Ok(_) => {
            fs::canonicalize(path).map_err(|error| format!("保存先を解決できませんでした: {error}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| "保存先が無効です".to_string())?;
            let name = path
                .file_name()
                .ok_or_else(|| "保存先が無効です".to_string())?;
            Ok(resolve_destination(parent)?.join(name))
        }
        Err(error) => Err(format!("保存先を確認できませんでした: {error}")),
    }
}

#[cfg(test)]
pub(crate) fn link_directory(target: &Path, link: &Path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_new_store_without_creating_it() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        workspace::init_workspace(&root, None, Some("missing-source")).unwrap();
        let path = store_path(&root).unwrap();
        assert_eq!(
            path,
            fs::canonicalize(&root)
                .unwrap()
                .join("data/research.sqlite")
        );
        assert!(!path.exists());
    }

    #[test]
    fn rejects_bukan_sources_plugins_and_paperpile_before_initialization() {
        let temporary = tempfile::tempdir().unwrap();
        for (name, marker, contents) in [
            ("source", "crates/bukan/Cargo.toml", "[package]"),
            ("legacy-source", "src-tauri/Cargo.toml", "[package]"),
            ("plugin", ".codex-plugin/plugin.json", r#"{"name":"bukan"}"#),
            ("Paperpile", "All Papers/marker", ""),
        ] {
            let parent = temporary.path().join(name);
            let marker = parent.join(marker);
            fs::create_dir_all(marker.parent().unwrap()).unwrap();
            fs::write(marker, contents).unwrap();
            let root = parent.join("workspace");
            assert!(
                workspace::init_workspace(&root, None, Some("missing-source")).is_err(),
                "{name}"
            );
            assert!(!root.exists());
            // Existing user workspaces moved under these locations cannot be written either.
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("bukan.toml"), "version=1\nname='test'\n[paperpile]\npath='auto'\nread_only=true\n[bibliography]\npath='data/paperpile.bib'\n").unwrap();
            assert!(store_path(&root).is_err(), "{name}");
        }
    }

    #[test]
    fn unrelated_repositories_and_plugins_are_not_bukan_sources() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(
            temporary.path().join("Cargo.toml"),
            "[package]\nname='other'\n",
        )
        .unwrap();
        fs::create_dir(temporary.path().join(".codex-plugin")).unwrap();
        fs::write(
            temporary.path().join(".codex-plugin/plugin.json"),
            r#"{"name":"other"}"#,
        )
        .unwrap();
        let root = temporary.path().join("workspace");
        workspace::init_workspace(&root, None, Some("missing-source")).unwrap();
        assert!(store_path(&root).is_ok());
    }

    #[test]
    fn configured_source_inside_workspace_remains_read_only() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        workspace::init_workspace(&root, None, Some("synced-source")).unwrap();
        assert!(
            validate_workspace_destination(&root, &root.join("synced-source/notes.md")).is_err()
        );
        assert!(!root.join("synced-source").exists());
    }

    #[test]
    fn rejects_parent_traversal_in_existing_and_missing_paths() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        workspace::init_workspace(&root, None, Some("missing-source")).unwrap();
        for relative in [
            "../outside.txt",
            "new/../../outside.txt",
            "data/../../outside.txt",
        ] {
            assert!(validate_workspace_destination(&root, &root.join(relative)).is_err());
        }
        assert!(!temporary.path().join("outside.txt").exists());
    }

    #[test]
    #[cfg(windows)]
    fn protects_missing_sources_with_case_variants() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        workspace::init_workspace(&root, None, Some(".BUKAN")).unwrap();
        let list = crate::paper_lists::PresentedPaperList {
            title: "Fixture".into(),
            description: String::new(),
            papers: Vec::new(),
            updated_at: 0,
        };
        assert!(crate::paper_lists::write_presented_list(&root, &list).is_err());
        assert!(!root.join(".BUKAN").exists());
        let other = temporary.path().join("other-workspace");
        assert!(workspace::init_workspace(&other, None, Some("NOTES")).is_err());
        assert!(!other.exists());
        let source = resolve_destination(&root.join("ÉTUDE")).unwrap();
        let candidate = resolve_destination(&root.join("étude/output.json")).unwrap();
        assert!(path_is_within(&candidate, &source));
        assert!(!path_is_within(
            &root.join("étude-other/output.json"),
            &source
        ));
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn resolves_links_before_accepting_a_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("workspace");
        let outside = temporary.path().join("outside");
        fs::create_dir(&outside).unwrap();
        workspace::init_workspace(&root, None, Some("missing-source")).unwrap();
        fs::remove_dir(root.join("data")).unwrap();
        link_directory(&outside, &root.join("data"));
        assert!(store_path(&root).is_err());
        assert!(crate::workspace_collections::load_or_initialize(&root, &[]).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    }
}
