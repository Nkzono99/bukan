use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn command(temporary: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bukan"));
    command
        .current_dir(temporary)
        .env_remove("BUKAN_WORKSPACE")
        .env("BUKAN_CONFIG_DIR", temporary.join("config"))
        .env("BUKAN_CACHE_DIR", temporary.join("runtime cache"));
    command
}

fn checked(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init(temporary: &Path, name: &str) -> std::path::PathBuf {
    let root = temporary.join(name);
    checked(command(temporary).arg("init").arg(&root).output().unwrap());
    fs::canonicalize(root).unwrap()
}

fn default_config(temporary: &Path, root: &Path) {
    fs::create_dir_all(temporary.join("config")).unwrap();
    let text = toml::to_string(&serde_json::json!({"default_workspace": root})).unwrap();
    fs::write(temporary.join("config/settings.toml"), text).unwrap();
}

fn configured_root(output: Output) -> String {
    let parsed: Value = serde_json::from_slice(&checked(output).stdout).unwrap();
    assert_eq!(
        parsed["mcpServers"]["bukan_research"]["args"][0],
        "research-mcp"
    );
    parsed["mcpServers"]["bukan"]["args"][1]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn workspace_binding_precedence_is_explicit_environment_saved_and_never_cwd() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = init(root, "研究 O'Brien first workspace");
    let second = init(root, "second workspace");
    let unbound = command(root)
        .current_dir(&first)
        .arg("mcp-config")
        .output()
        .unwrap();
    assert!(!unbound.status.success());
    assert!(String::from_utf8_lossy(&unbound.stderr).contains("No workspace is configured"));
    default_config(root, &first);
    assert_eq!(
        configured_root(
            command(root)
                .current_dir(&second)
                .arg("mcp-config")
                .output()
                .unwrap()
        ),
        first.to_string_lossy()
    );
    assert_eq!(
        configured_root(
            command(root)
                .env("BUKAN_WORKSPACE", &second)
                .arg("mcp-config")
                .output()
                .unwrap()
        ),
        second.to_string_lossy()
    );
    assert_eq!(
        configured_root(
            command(root)
                .env("BUKAN_WORKSPACE", "invalid relative path")
                .arg("mcp-config")
                .arg(&first)
                .output()
                .unwrap()
        ),
        first.to_string_lossy()
    );
    let invalid = command(root)
        .env("BUKAN_WORKSPACE", "relative workspace")
        .arg("mcp-config")
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("must be an absolute path"));
    assert!(!first.join("data/research.sqlite").exists());
    assert!(!second.join("data/research.sqlite").exists());
    assert!(!root.join("runtime cache").exists());
}

#[test]
fn doctor_reports_offline_library_without_initializing_anything() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    checked(
        command(temporary.path())
            .arg("init")
            .arg(&root)
            .args(["--paperpile", "missing offline Drive"])
            .output()
            .unwrap(),
    );
    let config_before = fs::read(root.join("bukan.toml")).unwrap();
    let result = checked(
        command(temporary.path())
            .arg("doctor")
            .arg(&root)
            .arg("--json")
            .output()
            .unwrap(),
    );
    let parsed: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(parsed["healthy"], false);
    assert!(parsed["workspace"]["paperpileRoot"].is_null());
    assert!(parsed["workspace"]["paperpileError"].is_string());
    assert_eq!(parsed["researchStore"]["initialized"], false);
    assert_eq!(fs::read(root.join("bukan.toml")).unwrap(), config_before);
    assert!(!root.join("data/research.sqlite").exists());
    assert!(!temporary.path().join("runtime cache").exists());
    assert!(!temporary.path().join("config").exists());
}

#[test]
fn collection_listing_is_read_only_and_changes_stay_in_workspace() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    let library = temporary.path().join("Paperpile");
    fs::create_dir_all(library.join("All Papers")).unwrap();
    bukan_lib::workspace::init_workspace(&root, None, Some(library.to_str().unwrap())).unwrap();
    let output = checked(
        command(temporary.path())
            .args(["collection", "list"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["collections"], serde_json::json!([]));
    assert!(!root.join("data/collections.json").exists());
    checked(
        command(temporary.path())
            .args(["collection", "create", "Group / Topic"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    checked(
        command(temporary.path())
            .args(["collection", "add", "Group / Topic", "paper-1"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let before: Value =
        serde_json::from_slice(&fs::read(root.join("data/collections.json")).unwrap()).unwrap();
    assert_eq!(
        before["collections"][0]["paperIds"],
        serde_json::json!(["paper-1"])
    );
    checked(
        command(temporary.path())
            .args(["collection", "remove", "Group / Topic", "paper-1"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let after: Value =
        serde_json::from_slice(&fs::read(root.join("data/collections.json")).unwrap()).unwrap();
    assert_eq!(after["collections"][0]["paperIds"], serde_json::json!([]));
}

#[test]
fn first_collection_creation_imports_source_assignments_without_changing_paperpile() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    let library = temporary.path().join("Paperpile");
    let source = library.join("All Papers/Existing Collection/Author 2024 - Example.pdf");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"%PDF-synthetic fixture").unwrap();
    let source_metadata = fs::metadata(&source).unwrap();
    let library_before = bukan_lib::build_index(&library).unwrap();
    let source_id = library_before.papers[0].id.clone();
    bukan_lib::workspace::init_workspace(&root, None, Some(library.to_str().unwrap())).unwrap();
    let output = checked(
        command(temporary.path())
            .args(["collection", "create", "New Collection"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["initializedFrom"], "paperpile");
    assert_eq!(
        parsed["collections"],
        serde_json::json!([
            {"path": "Existing Collection", "paperIds": [source_id]},
            {"path": "New Collection", "paperIds": []},
        ])
    );
    assert_eq!(fs::read(&source).unwrap(), b"%PDF-synthetic fixture");
    assert_eq!(
        fs::metadata(&source).unwrap().modified().unwrap(),
        source_metadata.modified().unwrap()
    );
    assert_eq!(
        serde_json::to_value(bukan_lib::build_index(&library).unwrap()).unwrap(),
        serde_json::to_value(library_before).unwrap()
    );

    // Disconnect only the test workspace's configuration. Existing assignments survive.
    let (_, mut config) = bukan_lib::workspace::load_workspace(&root).unwrap();
    config.paperpile.path = "offline source".into();
    fs::write(root.join("bukan.toml"), toml::to_string(&config).unwrap()).unwrap();
    let output = checked(
        command(temporary.path())
            .args(["collection", "create", "Offline Collection"])
            .arg(&root)
            .output()
            .unwrap(),
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        parsed["collections"][0]["paperIds"],
        serde_json::json!([source_id])
    );
    assert_eq!(parsed["collections"].as_array().unwrap().len(), 3);
    assert_eq!(fs::read(&source).unwrap(), b"%PDF-synthetic fixture");
}

#[test]
fn first_collection_creation_requires_source_access_without_saving_empty_provenance() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    bukan_lib::workspace::init_workspace(&root, None, Some("offline source")).unwrap();
    let output = command(temporary.path())
        .args(["collection", "create", "New Collection"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("Connect the Paperpile library before creating the first workspace collection"));
    assert!(!root.join("data/collections.json").exists());
}

#[test]
fn research_cannot_override_the_bound_store_or_initialize_implicitly() {
    let temporary = tempfile::tempdir().unwrap();
    let root = init(temporary.path(), "workspace");
    for option in [
        "--store",
        "--s",
        "--st",
        "--sto",
        "--stor",
        "--sto=elsewhere.sqlite",
    ] {
        let result = command(temporary.path())
            .arg("research")
            .arg(&root)
            .args(["--", option, "elsewhere.sqlite", "init"])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("--store cannot be overridden"),
            "{option}"
        );
    }
    let result = command(temporary.path())
        .arg("research")
        .arg(&root)
        .args(["--", "info"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("bukan setup"));
    assert!(!root.join("data/research.sqlite").exists());
    assert!(!temporary.path().join("elsewhere.sqlite").exists());
}
