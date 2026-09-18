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
        .env("BUKAN_DATA_DIR", temporary.join("app data"))
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
fn paths_reports_managed_data_without_creating_directories() {
    let temporary = tempfile::tempdir().unwrap();
    let output = checked(
        command(temporary.path())
            .args(["paths", "--json"])
            .output()
            .unwrap(),
    );
    let paths: Value = serde_json::from_slice(&output.stdout).unwrap();
    let data = fs::canonicalize(temporary.path()).unwrap().join("app data");
    assert_eq!(paths["dataDir"], data.to_string_lossy().as_ref());
    assert_eq!(
        paths["managedWorkspace"],
        data.join("workspaces/default").to_string_lossy().as_ref()
    );
    assert!(paths["defaultWorkspace"].is_null());
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    let output = command(temporary.path())
        .env("BUKAN_DATA_DIR", "relative")
        .args(["paths", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("BUKAN_DATA_DIR must be an absolute path")
    );
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[test]
fn default_data_location_uses_the_platform_user_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let platform_root = temporary.path().join("OS user data");
    let output = checked(
        command(temporary.path())
            .env_remove("BUKAN_DATA_DIR")
            .env(
                if cfg!(windows) {
                    "LOCALAPPDATA"
                } else {
                    "XDG_DATA_HOME"
                },
                &platform_root,
            )
            .args(["paths", "--json"])
            .output()
            .unwrap(),
    );
    let paths: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = fs::canonicalize(temporary.path())
        .unwrap()
        .join("OS user data/bukan");
    assert_eq!(paths["dataDir"], expected.to_string_lossy().as_ref());
    assert!(!platform_root.exists());
}

#[test]
fn paperpile_login_refuses_profile_under_configured_source_before_setup() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source-files");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("paper.pdf"), b"unchanged source").unwrap();
    let root = temporary.path().join("workspace");
    checked(
        command(temporary.path())
            .arg("init")
            .arg(&root)
            .arg("--paperpile")
            .arg(&source)
            .output()
            .unwrap(),
    );
    let output = command(temporary.path())
        .env("BUKAN_DATA_DIR", &source)
        .args(["paperpile", "login"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("outside the configured Paperpile library"));
    assert_eq!(
        fs::read(source.join("paper.pdf")).unwrap(),
        b"unchanged source"
    );
    assert_eq!(fs::read_dir(&source).unwrap().count(), 1);
    assert!(!temporary.path().join("runtime cache").exists());
}

#[test]
fn data_directory_refuses_source_repositories_plugins_and_paperpile() {
    let temporary = tempfile::tempdir().unwrap();
    for (name, marker, contents) in [
        ("source", "crates/bukan/Cargo.toml", "[package]"),
        ("plugin", ".codex-plugin/plugin.json", r#"{"name":"bukan"}"#),
        ("library", "All Papers/marker", ""),
    ] {
        let root = temporary.path().join(name);
        let marker = root.join(marker);
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(marker, contents).unwrap();
        let data = root.join("data");
        let output = command(temporary.path())
            .env("BUKAN_DATA_DIR", &data)
            .args(["paths", "--json"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name}");
        assert!(!data.exists());
    }
}

#[test]
fn setup_creates_only_unbound_managed_workspace_and_can_resume_before_dependencies() {
    let temporary = tempfile::tempdir().unwrap();
    // Stop before runtime downloads; this specifically tests setup's workspace phase.
    let setup = || {
        command(temporary.path())
            .env("BUKAN_CACHE_DIR", "relative")
            .arg("setup")
            .output()
            .unwrap()
    };
    let output = setup();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("BUKAN_CACHE_DIR must be an absolute path"));
    let root = temporary.path().join("app data/workspaces/default");
    let before = fs::read(root.join("bukan.toml")).unwrap();
    let (_, config) = bukan_lib::workspace::load_workspace(&root).unwrap();
    assert_eq!(config.paperpile.path, "auto");
    assert!(config.paperpile.read_only);
    fs::write(root.join("notes/keep.md"), "Existing research").unwrap();
    assert!(!setup().status.success());
    assert_eq!(fs::read(root.join("bukan.toml")).unwrap(), before);
    assert_eq!(
        fs::read(root.join("notes/keep.md")).unwrap(),
        b"Existing research"
    );
    assert!(!root.join("data/research.sqlite").exists());
    assert!(!temporary.path().join("config").exists());
}

#[test]
fn setup_never_falls_back_from_missing_or_invalid_configured_workspaces() {
    for binding in ["argument", "environment", "saved", "invalid-settings"] {
        let temporary = tempfile::tempdir().unwrap();
        let missing = temporary.path().join("offline existing workspace");
        let mut setup = command(temporary.path());
        setup.env("BUKAN_CACHE_DIR", "relative").arg("setup");
        match binding {
            "argument" => {
                setup.arg(&missing);
            }
            "environment" => {
                setup.env("BUKAN_WORKSPACE", &missing);
            }
            "saved" => default_config(temporary.path(), &missing),
            _ => {
                fs::create_dir(temporary.path().join("config")).unwrap();
                fs::write(
                    temporary.path().join("config/settings.toml"),
                    "invalid TOML [",
                )
                .unwrap();
            }
        }
        assert!(!setup.output().unwrap().status.success(), "{binding}");
        assert!(!missing.exists());
        assert!(!temporary.path().join("app data").exists());
        assert!(!temporary.path().join("runtime cache").exists());
    }
}

#[test]
fn setup_reuses_saved_external_workspace_without_moving_it() {
    let temporary = tempfile::tempdir().unwrap();
    let root = init(temporary.path(), "existing external research");
    default_config(temporary.path(), &root);
    fs::write(
        root.join("data/research.sqlite"),
        b"Existing database untouched",
    )
    .unwrap();
    let settings = fs::read(temporary.path().join("config/settings.toml")).unwrap();
    let output = command(temporary.path())
        .env("BUKAN_CACHE_DIR", "relative")
        .arg("setup")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("saved default"));
    assert_eq!(
        fs::read(root.join("data/research.sqlite")).unwrap(),
        b"Existing database untouched"
    );
    assert_eq!(
        fs::read(temporary.path().join("config/settings.toml")).unwrap(),
        settings
    );
    assert!(!temporary.path().join("app data").exists());
}

#[test]
fn setup_explicit_and_environment_bindings_override_invalid_saved_values() {
    for saved in ["123", "'relative path'"] {
        for binding in ["argument", "environment"] {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = init(temporary.path(), "existing workspace");
            fs::create_dir(temporary.path().join("config")).unwrap();
            fs::write(
                temporary.path().join("config/settings.toml"),
                format!("default_workspace = {saved}\n"),
            )
            .unwrap();
            let mut setup = command(temporary.path());
            setup
                .env("BUKAN_CACHE_DIR", "relative")
                .arg("setup")
                .arg("--default");
            if binding == "argument" {
                setup.arg(&workspace);
            } else {
                setup.env("BUKAN_WORKSPACE", &workspace);
            }
            let output = setup.output().unwrap();
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("BUKAN_CACHE_DIR must be an absolute path"),
                "{saved} {binding}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!temporary.path().join("app data").exists());
        }
    }
}

#[test]
fn paths_rejects_invalid_bindings_and_custom_paperpile_destinations_before_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("existing workspace");
    let source = temporary.path().join("custom synced library");
    fs::create_dir(&source).unwrap();
    bukan_lib::workspace::init_workspace(&workspace, None, Some(source.to_str().unwrap())).unwrap();
    for destination in ["BUKAN_DATA_DIR", "BUKAN_CONFIG_DIR", "BUKAN_CACHE_DIR"] {
        let output = command(temporary.path())
            .env("BUKAN_WORKSPACE", &workspace)
            .env(destination, source.join("storage"))
            .args(["paths", "--json"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{destination}");
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("outside the configured Paperpile library"));
        assert!(!source.join("storage").exists());
    }
    let output = command(temporary.path())
        .env(
            "BUKAN_WORKSPACE",
            temporary.path().join("missing workspace"),
        )
        .args(["paths", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    default_config(
        temporary.path(),
        &temporary.path().join("missing saved workspace"),
    );
    let output = command(temporary.path())
        .args(["paths", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!temporary.path().join("app data").exists());
}

#[test]
fn paths_identifies_existing_binding_without_initializing_managed_workspace() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = init(temporary.path(), "existing workspace");
    default_config(temporary.path(), &workspace);
    let output = checked(
        command(temporary.path())
            .args(["paths", "--json"])
            .output()
            .unwrap(),
    );
    let paths: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        paths["defaultWorkspace"],
        workspace.to_string_lossy().as_ref()
    );
    assert_eq!(
        paths["selectedWorkspace"],
        workspace.to_string_lossy().as_ref()
    );
    assert_eq!(paths["workspaceSource"], "saved default");
    assert!(!temporary.path().join("app data").exists());
}

#[test]
fn paths_environment_binding_overrides_invalid_saved_values_but_reports_them() {
    for saved in ["123", "'relative path'"] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = init(temporary.path(), "existing workspace");
        fs::create_dir(temporary.path().join("config")).unwrap();
        let contents = format!("default_workspace = {saved}\n");
        fs::write(temporary.path().join("config/settings.toml"), &contents).unwrap();
        let output = checked(
            command(temporary.path())
                .env("BUKAN_WORKSPACE", &workspace)
                .args(["paths", "--json"])
                .output()
                .unwrap(),
        );
        let paths: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            paths["selectedWorkspace"],
            workspace.to_string_lossy().as_ref()
        );
        assert_eq!(paths["workspaceSource"], "BUKAN_WORKSPACE");
        assert!(paths["defaultWorkspace"].is_null());
        assert!(paths["defaultWorkspaceError"].is_string());
        assert_eq!(
            fs::read_to_string(temporary.path().join("config/settings.toml")).unwrap(),
            contents
        );
        assert!(!temporary.path().join("app data").exists());
        // Without the overriding environment binding the invalid saved value is selected and fails.
        assert!(!command(temporary.path())
            .args(["paths", "--json"])
            .output()
            .unwrap()
            .status
            .success());
    }
}

fn dependency_fixture(temporary: &Path) -> std::path::PathBuf {
    let root = temporary.join("app data/dependencies");
    fs::create_dir_all(root.join("uv/test")).unwrap();
    fs::create_dir_all(root.join("poppler/test/bin")).unwrap();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    fs::write(
        root.join(format!("uv/test/uv{suffix}")),
        "private uv fixture",
    )
    .unwrap();
    for name in ["pdfinfo", "pdftotext", "pdftoppm"] {
        fs::write(
            root.join(format!("poppler/test/bin/{name}{suffix}")),
            "private Poppler fixture",
        )
        .unwrap();
    }
    fs::write(
        root.join("current.json"),
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "uv": format!("uv/test/uv{suffix}"), "popplerBin": "poppler/test/bin"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::canonicalize(root).unwrap()
}

#[test]
fn doctor_uses_private_dependencies_even_without_path() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = init(temporary.path(), "workspace");
    let dependencies = dependency_fixture(temporary.path());
    let output = checked(
        command(temporary.path())
            .env("PATH", "")
            .arg("doctor")
            .arg(&workspace)
            .arg("--json")
            .output()
            .unwrap(),
    );
    let doctor: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(doctor["dependencyErrors"].as_array().unwrap().is_empty());
    for path in
        std::iter::once(&doctor["uv"]).chain(doctor["poppler"].as_object().unwrap().values())
    {
        assert!(Path::new(path.as_str().unwrap()).starts_with(&dependencies));
    }
    assert!(!workspace.join("data/research.sqlite").exists());
}

#[test]
fn absent_private_pointer_preserves_absolute_path_tool_discovery() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = init(temporary.path(), "workspace");
    let tools = temporary.path().join("existing tools");
    fs::create_dir(&tools).unwrap();
    for name in ["uv", "pdfinfo", "pdftotext", "pdftoppm"] {
        fs::write(
            tools.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_string()
            }),
            "existing tool fixture",
        )
        .unwrap();
    }
    let output = checked(
        command(temporary.path())
            .env("PATH", &tools)
            .arg("doctor")
            .arg(&workspace)
            .arg("--json")
            .output()
            .unwrap(),
    );
    let doctor: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(doctor["dependencyErrors"].as_array().unwrap().is_empty());
    for path in
        std::iter::once(&doctor["uv"]).chain(doctor["poppler"].as_object().unwrap().values())
    {
        assert!(Path::new(path.as_str().unwrap()).starts_with(&tools));
    }
    assert!(!temporary.path().join("app data").exists());
}

#[test]
fn invalid_private_dependency_paths_are_reported_instead_of_using_path_fallback() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = init(temporary.path(), "workspace");
    let dependencies = dependency_fixture(temporary.path());
    let absolute = temporary
        .path()
        .join("outside")
        .to_string_lossy()
        .into_owned();
    for invalid in ["../outside", "missing", absolute.as_str()] {
        fs::write(
            dependencies.join("current.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 1, "uv": invalid, "popplerBin": invalid
            }))
            .unwrap(),
        )
        .unwrap();
        let output = checked(
            command(temporary.path())
                .arg("doctor")
                .arg(&workspace)
                .arg("--json")
                .output()
                .unwrap(),
        );
        let doctor: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            doctor["dependencyErrors"].as_array().unwrap().len(),
            4,
            "{invalid}"
        );
        assert!(doctor["uv"].is_null());
        assert!(doctor["poppler"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_null));
    }
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
