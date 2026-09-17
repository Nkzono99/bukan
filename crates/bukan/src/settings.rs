//! Bukan's own workspace binding; never edits an AI client's configuration.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{storage, workspace};

pub struct WorkspaceBinding {
    pub root: PathBuf,
    pub source: &'static str,
}

pub fn bind_workspace(explicit: Option<&Path>) -> Result<WorkspaceBinding, String> {
    configured_workspace(explicit)?.ok_or_else(|| {
        "No workspace is configured. Run bukan setup to create the managed workspace, or pass an existing workspace path.".to_string()
    })
}

pub fn configured_workspace(explicit: Option<&Path>) -> Result<Option<WorkspaceBinding>, String> {
    workspace_selection(explicit)?
        .map(|(path, source)| {
            let (root, _) = workspace::load_workspace(&path)?;
            Ok(WorkspaceBinding { root, source })
        })
        .transpose()
}

fn workspace_selection(explicit: Option<&Path>) -> Result<Option<(PathBuf, &'static str)>, String> {
    let selected = if let Some(path) = explicit {
        (path.to_path_buf(), "argument")
    } else if let Some(path) = std::env::var_os("BUKAN_WORKSPACE") {
        (
            absolute_setting(PathBuf::from(path), "BUKAN_WORKSPACE")?,
            "BUKAN_WORKSPACE",
        )
    } else {
        let Some(path) = read_default()? else {
            return Ok(None);
        };
        (path, "saved default")
    };
    Ok(Some(selected))
}

/// Only explicit setup may initialize a managed workspace. Invalid configured
/// bindings remain errors so an offline drive never silently creates a new DB.
pub fn setup_workspace(explicit: Option<&Path>) -> Result<WorkspaceBinding, String> {
    let (path, source) = match workspace_selection(explicit)? {
        Some(selected) => selected,
        None => {
            let path = managed_workspace()?;
            if !path
                .join("bukan.toml")
                .try_exists()
                .map_err(|error| error.to_string())?
            {
                workspace::init_workspace(&path, Some("Bukan Research"), None)?;
            }
            (path, "managed default")
        }
    };
    let (root, _) = workspace::load_workspace(&path)?;
    Ok(WorkspaceBinding { root, source })
}

/// Durable user data is separate from replaceable toolkit/plugin installations.
pub fn data_dir() -> Result<PathBuf, String> {
    user_directory(
        "BUKAN_DATA_DIR",
        "LOCALAPPDATA",
        "XDG_DATA_HOME",
        ".local/share",
    )
}

pub fn managed_workspace() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("workspaces/default"))
}

pub fn config_dir() -> Result<PathBuf, String> {
    user_directory("BUKAN_CONFIG_DIR", "APPDATA", "XDG_CONFIG_HOME", ".config")
}

pub fn cache_dir() -> Result<PathBuf, String> {
    user_directory(
        "BUKAN_CACHE_DIR",
        "LOCALAPPDATA",
        "XDG_CACHE_HOME",
        ".cache",
    )
}

fn user_directory(
    override_name: &str,
    windows_name: &str,
    unix_name: &str,
    fallback: &str,
) -> Result<PathBuf, String> {
    let path = if let Some(path) = std::env::var_os(override_name) {
        absolute_setting(PathBuf::from(path), override_name)?
    } else {
        let variable = if cfg!(windows) {
            windows_name
        } else {
            unix_name
        };
        let base = if let Some(path) = std::env::var_os(variable) {
            absolute_setting(PathBuf::from(path), variable)?
        } else if cfg!(windows) {
            return Err(format!(
                "{variable} is not set; set {override_name} to an absolute directory."
            ));
        } else {
            let home = std::env::var_os("HOME").ok_or_else(|| {
                format!("HOME is not set; set {override_name} to an absolute directory.")
            })?;
            absolute_setting(PathBuf::from(home), "HOME")?.join(fallback)
        };
        base.join("bukan")
    };
    let resolved = storage::resolve_destination(&path)?;
    storage::validate_storage_location(&resolved)?;
    Ok(resolved)
}

fn absolute_setting(path: PathBuf, name: &str) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(format!(
            "{name} must be an absolute path (received {}).",
            path.display()
        ))
    }
}

fn read_settings(directory: &Path) -> Result<(PathBuf, toml::Table), String> {
    let path = storage::resolve_destination(&directory.join("settings.toml"))?;
    storage::validate_storage_location(&path)?;
    let settings = match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| format!("Could not parse {}: {error}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(error) => return Err(format!("Could not read {}: {error}", path.display())),
    };
    Ok((path, settings))
}

/// Check readability and syntax without interpreting a binding that an explicit
/// argument or environment setting may override.
pub(crate) fn validate_settings() -> Result<(), String> {
    read_settings(&config_dir()?).map(|_| ())
}

pub fn read_default() -> Result<Option<PathBuf>, String> {
    let (_, settings) = read_settings(&config_dir()?)?;
    settings
        .get("default_workspace")
        .map(|value| {
            let value = value.as_str().ok_or_else(|| {
                "settings.toml default_workspace must be a path string.".to_string()
            })?;
            absolute_setting(PathBuf::from(value), "default_workspace")
        })
        .transpose()
}

/// Check external runtime/config destinations against this workspace's source too.
pub(crate) fn validate_external_destination(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let destination = storage::resolve_destination(path)?;
    storage::validate_storage_location(&destination)?;
    let (root, config) = workspace::load_workspace(root)?;
    if !config.paperpile.path.eq_ignore_ascii_case("auto") {
        let source = root.join(&config.paperpile.path);
        if storage::resolve_destination(&source)
            .is_ok_and(|source| storage::path_is_within(&destination, &source))
        {
            return Err("Bukan settings and runtime files must be outside the configured Paperpile library.".into());
        }
    }
    Ok(destination)
}

/// `setup` records the first default; replacing an existing one is explicit.
pub fn save_default(root: &Path, replace: bool) -> Result<bool, String> {
    save_default_in(root, replace, &config_dir()?)
}

fn save_default_in(root: &Path, replace: bool, directory: &Path) -> Result<bool, String> {
    let (root, _) = workspace::load_workspace(root)?;
    let (path, mut settings) = read_settings(directory)?;
    if settings.contains_key("default_workspace") && !replace {
        return Ok(false);
    }
    let path = validate_external_destination(&root, &path)?;
    settings.insert(
        "default_workspace".into(),
        toml::Value::String(root.to_string_lossy().into_owned()),
    );
    let contents = toml::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "Invalid settings path".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create settings directory: {error}"))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(contents.as_bytes())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| error.to_string())?;
    temporary
        .persist(&path)
        .map_err(|error| format!("Could not save {}: {error}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_default_is_saved_and_switching_requires_replace() {
        let temporary = tempfile::tempdir().unwrap();
        let first = temporary.path().join("first research");
        let second = temporary.path().join("second research");
        workspace::init_workspace(&first, None, None).unwrap();
        workspace::init_workspace(&second, None, None).unwrap();
        let directory = temporary.path().join("config");
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("settings.toml"),
            "theme = 'custom'\n[personal]\nvalue = 42\n",
        )
        .unwrap();
        assert!(save_default_in(&first, false, &directory).unwrap());
        let original = fs::read(directory.join("settings.toml")).unwrap();
        assert!(!save_default_in(&second, false, &directory).unwrap());
        assert_eq!(fs::read(directory.join("settings.toml")).unwrap(), original);
        assert!(save_default_in(&second, true, &directory).unwrap());
        let (_, saved) = read_settings(&directory).unwrap();
        assert_eq!(
            saved["default_workspace"].as_str().unwrap(),
            fs::canonicalize(second).unwrap().to_string_lossy()
        );
        assert_eq!(saved["theme"].as_str(), Some("custom"));
        assert_eq!(saved["personal"]["value"].as_integer(), Some(42));
    }

    #[test]
    fn external_locations_refuse_configured_library_even_with_an_unusual_name() {
        let temporary = tempfile::tempdir().unwrap();
        let library = temporary.path().join("sources");
        fs::create_dir(&library).unwrap();
        let root = temporary.path().join("research");
        workspace::init_workspace(&root, None, Some(library.to_str().unwrap())).unwrap();
        assert!(save_default_in(&root, true, &library.join("settings")).is_err());
        assert!(!library.join("settings").exists());
    }

    #[cfg(windows)]
    #[test]
    fn missing_configured_source_is_protected_regardless_of_case() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("research");
        let source = temporary.path().join("OfflineSource");
        workspace::init_workspace(&root, None, Some(source.to_str().unwrap())).unwrap();
        let settings = temporary.path().join("offlinesource/settings");
        assert!(save_default_in(&root, true, &settings).is_err());
        assert!(!source.exists());
    }
}
