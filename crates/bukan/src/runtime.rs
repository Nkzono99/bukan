//! Packaged research engine with an isolated, per-user Python environment.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, ExitStatus, Output, Stdio},
};

use crate::{settings, storage};

const PYTHON_VERSION: &str = "3.12";
// -I excludes caller modules/user packages; paths are arguments, never Python code.
const ENTRYPOINT: &str =
    "import sys; sys.path.insert(0, sys.argv.pop(1)); from bukan_research.cli import main; main()";
const PAPERPILE_ENTRYPOINT: &str =
    "import sys; sys.path.insert(0, sys.argv.pop(1)); from bukan_research.paperpile import main; main()";
const IMPORT_CHECK: &str = "import sys; sys.path.insert(0, sys.argv[1]); import markdown_it, mcp, pydantic, bukan_research.cli, bukan_research.server";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub available: bool,
    pub ready: bool,
    pub package: Option<PathBuf>,
    pub python: Option<PathBuf>,
    pub detail: String,
}

struct Runtime {
    package: PathBuf,
    root: PathBuf,
    fingerprint: String,
}

impl Runtime {
    fn discover() -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let package = discover_package(&executable)?;
        let fingerprint = package_fingerprint(&package)?;
        let root = settings::cache_dir()?
            .join("research-runtime")
            .join(&fingerprint);
        let root = storage::resolve_destination(&root)?;
        storage::validate_storage_location(&root)?;
        Ok(Self {
            package,
            root,
            fingerprint,
        })
    }

    fn python(&self) -> PathBuf {
        self.root.join(if cfg!(windows) {
            "venv/Scripts/python.exe"
        } else {
            "venv/bin/python"
        })
    }

    fn ready(&self) -> bool {
        self.python().is_file()
            && fs::read_to_string(self.root.join("ready"))
                .is_ok_and(|value| value == self.fingerprint)
    }

    fn require_ready(&self) -> Result<(), String> {
        if self.ready() {
            Ok(())
        } else {
            Err("The research runtime is not prepared. Run bukan setup <workspace> first.".into())
        }
    }

    fn arguments(&self, store: &Path, args: &[String]) -> Vec<String> {
        let mut result = vec![
            "-I".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-c".into(),
            ENTRYPOINT.into(),
            self.package.join("src").to_string_lossy().into_owned(),
            "--store".into(),
            store.to_string_lossy().into_owned(),
        ];
        result.extend_from_slice(args);
        result
    }

    fn check_imports(&self, browser: bool) -> Result<(), String> {
        let imports = if browser {
            format!(
                "{IMPORT_CHECK}; import playwright.sync_api, filelock, bukan_research.paperpile"
            )
        } else {
            IMPORT_CHECK.to_string()
        };
        let output = hidden_command(&self.python())
            .args(["-I", "-B", "-X", "utf8", "-c", &imports])
            .arg(self.package.join("src"))
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("Could not verify the research runtime: {error}"))?;
        checked_output(output, "Research runtime verification").map(|_| ())
    }

    fn prepare(&self, workspace: &Path, browser: bool) -> Result<(), String> {
        let shared = self.root.parent().expect("version directory");
        for path in [
            self.root.clone(),
            self.root.join("venv"),
            self.root.join("ready"),
            shared.join("cache"),
            shared.join("python"),
        ] {
            settings::validate_external_destination(workspace, &path)?;
        }
        if self.ready() && (!browser || self.check_imports(true).is_ok()) {
            return self.check_imports(false);
        }
        let uv = find_uv()?.ok_or_else(|| "uv was not found. Run the Bukan toolkit installer, or install uv from https://docs.astral.sh/uv/getting-started/installation/, then run bukan setup again.".to_string())?;
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("Could not create the runtime directory: {error}"))?;
        let mut command = hidden_command(&uv);
        command.args(["--no-config", "sync"]);
        if browser {
            // Preserve ordinary research support on platforms without Playwright.
            command.args(["--extra", "paperpile"]);
        }
        let output = command
            .args([
                "--locked",
                "--no-dev",
                "--no-install-project",
                "--managed-python",
                "--python",
                PYTHON_VERSION,
                "--project",
            ])
            .arg(&self.package)
            .env("UV_PROJECT_ENVIRONMENT", self.root.join("venv"))
            .env("UV_CACHE_DIR", shared.join("cache"))
            .env("UV_PYTHON_INSTALL_DIR", shared.join("python"))
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("Could not start uv: {error}"))?;
        checked_output(output, "Research runtime setup")?;
        self.check_imports(browser)?;
        fs::write(self.root.join("ready"), &self.fingerprint)
            .map_err(|error| format!("Could not save runtime readiness: {error}"))
    }

    fn initialize_missing(&self, workspace: &Path, store: &Path) -> Result<bool, String> {
        if store.exists() {
            if !store.is_file() {
                return Err(format!("Research store is not a file: {}", store.display()));
            }
            return Ok(false);
        }
        let output = hidden_command(&self.python())
            .args(self.arguments(store, &["init".into()]))
            .current_dir(workspace)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("Could not initialize the research store: {error}"))?;
        checked_output(output, "Research store initialization")?;
        Ok(true)
    }
}

pub fn status() -> RuntimeStatus {
    match Runtime::discover() {
        Ok(runtime) => {
            let ready = runtime.ready();
            let check = if ready {
                runtime.check_imports(false)
            } else {
                Err("Run bukan setup <workspace> to prepare the research runtime.".into())
            };
            RuntimeStatus {
                available: true,
                ready: check.is_ok(),
                python: Some(runtime.python()),
                package: Some(runtime.package),
                detail: check
                    .err()
                    .unwrap_or_else(|| "Research runtime is ready.".into()),
            }
        }
        Err(detail) => RuntimeStatus {
            available: false,
            ready: false,
            package: None,
            python: None,
            detail,
        },
    }
}

/// Only explicit setup prepares dependencies and creates an absent research DB.
pub fn setup(workspace: &Path) -> Result<bool, String> {
    let store = storage::store_path(workspace)?;
    let runtime = Runtime::discover()?;
    runtime.prepare(workspace, false)?;
    runtime.initialize_missing(workspace, &store)
}

/// One dedicated browser profile shared across workspaces, never the user's
/// ordinary Chrome profile or Paperpile's synced source. Python serializes use.
pub fn paperpile(
    workspace: &Path,
    action: &str,
    request: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    if !["login", "status", "import"].contains(&action) {
        return Err("Unknown Paperpile browser action.".into());
    }
    let profile = paperpile_profile(workspace, &settings::data_dir()?)?;
    if action != "login" && !profile.is_dir() {
        return Ok(
            serde_json::json!({"status":"login_required", "detail":"Run: bukan paperpile login"}),
        );
    }
    let runtime = Runtime::discover()?;
    // Browser operations prepare optional dependencies without opening the DB.
    // A toolkit update does not require signing in to an existing profile again.
    runtime.prepare(workspace, true)?;
    let mut child = hidden_command(&runtime.python())
        .args(["-I", "-B", "-X", "utf8", "-c", PAPERPILE_ENTRYPOINT])
        .arg(runtime.package.join("src"))
        .arg(action)
        .arg(profile)
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("Could not start Paperpile browser: {error}"))?;
    if action == "import" {
        let data = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(&data)
            .map_err(|error| format!("Could not send the Paperpile request: {error}"))?;
    } else {
        drop(child.stdin.take());
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    let output = checked_output(
        output,
        "Paperpile browser operation (if import was submitted, its outcome may be unknown)",
    )?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!("Invalid Paperpile browser response; import outcome may be unknown: {error}")
    })
}

fn paperpile_profile(workspace: &Path, data: &Path) -> Result<PathBuf, String> {
    let root = settings::validate_external_destination(workspace, &data.join("paperpile-browser"))?;
    let profile = settings::validate_external_destination(workspace, &root.join("profile"))?;
    let lock = settings::validate_external_destination(workspace, &root.join("browser.lock"))?;
    if !storage::path_is_within(&root, data)
        || profile != root.join("profile")
        || lock != root.join("browser.lock")
    {
        return Err("The dedicated browser profile and lock must remain within Bukan's browser data directory; external links are not supported.".into());
    }
    Ok(profile)
}

/// Inherit all three streams so an MCP session remains a transparent stdio pipe.
pub fn run(workspace: &Path, args: &[String]) -> Result<ExitStatus, String> {
    if args.iter().any(|arg| {
        let option = arg.split('=').next().unwrap_or(arg);
        option.len() > 2 && "--store".starts_with(option)
    }) {
        return Err(
            "The research store is bound to the workspace; --store cannot be overridden.".into(),
        );
    }
    let store = storage::store_path(workspace)?;
    let runtime = Runtime::discover()?;
    runtime.require_ready()?;
    hidden_command(&runtime.python())
        .args(runtime.arguments(&store, args))
        .env(
            "BUKAN_EXECUTABLE",
            std::env::current_exe().map_err(|error| error.to_string())?,
        )
        .env("BUKAN_WORKSPACE", workspace)
        .current_dir(workspace)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("Could not start the research engine: {error}"))
}

fn discover_package(executable: &Path) -> Result<PathBuf, String> {
    let directory = executable
        .parent()
        .ok_or_else(|| "Invalid executable path".to_string())?;
    let mut candidates = vec![directory.join("research-engine")];
    if let Some(parent) = directory.parent() {
        candidates.push(parent.join("research-engine"));
    }
    // Locate development resources from the executable, never the caller's cwd.
    for ancestor in directory.ancestors() {
        if ancestor.join("Cargo.toml").is_file()
            && ancestor.join("crates/bukan/Cargo.toml").is_file()
        {
            candidates.push(ancestor.join("research-engine"));
        }
    }
    candidates.into_iter().find(|path| path.join("pyproject.toml").is_file() && path.join("uv.lock").is_file() && path.join("src/bukan_research/cli.py").is_file())
        .map(|path| fs::canonicalize(path).map_err(|error| error.to_string())).unwrap_or_else(|| Err("Packaged research-engine was not found next to bukan or in its parent directory. Reinstall the complete Bukan bundle.".into()))
}

fn package_fingerprint(package: &Path) -> Result<String, String> {
    let mut files = vec![package.join("pyproject.toml"), package.join("uv.lock")];
    for entry in walkdir::WalkDir::new(package.join("src/bukan_research")) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "py") {
            files.push(entry.into_path());
        }
    }
    files.sort();
    let mut hash = blake3::Hasher::new();
    hash.update(PYTHON_VERSION.as_bytes());
    for file in files {
        hash.update(
            file.strip_prefix(package)
                .unwrap()
                .to_string_lossy()
                .as_bytes(),
        );
        hash.update(&[0]);
        hash.update(
            &fs::read(&file)
                .map_err(|error| format!("Could not read {}: {error}", file.display()))?,
        );
    }
    Ok(hash.finalize().to_hex()[..20].to_string())
}

pub(crate) fn executable_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    std::env::split_paths(&path)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(&filename))
        .find(|candidate| candidate.is_file())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrivateDependencies {
    version: u32,
    uv: PathBuf,
    poppler_bin: PathBuf,
}

fn private_dependencies() -> Result<Option<(PathBuf, PrivateDependencies)>, String> {
    let data = settings::data_dir()?;
    let root = storage::resolve_destination(&data.join("dependencies"))?;
    if !root.starts_with(&data) {
        return Err(
            "The private dependencies directory must remain within the Bukan data directory."
                .into(),
        );
    }
    let pointer = root.join("current.json");
    match fs::symlink_metadata(&pointer) {
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect {}: {error}", pointer.display())),
    }
    let pointer = private_path(&root, Path::new("current.json"))?;
    let text = fs::read_to_string(&pointer)
        .map_err(|error| format!("Could not read {}: {error}", pointer.display()))?;
    let dependencies: PrivateDependencies = serde_json::from_str(&text).map_err(|error| {
        format!(
            "Invalid {}: {error}. Run the Bukan toolkit installer to repair it.",
            pointer.display()
        )
    })?;
    if dependencies.version != 1 {
        return Err(format!(
            "Unsupported private dependency format {}. Update the Bukan toolkit.",
            dependencies.version
        ));
    }
    Ok(Some((root, dependencies)))
}

fn private_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("Private dependency paths must be relative paths without traversal.".into());
    }
    let path = fs::canonicalize(root.join(relative)).map_err(|error| {
        format!("Private dependency {} is unavailable: {error}. Run the Bukan toolkit installer to repair it.", relative.display())
    })?;
    if !path.starts_with(root) {
        return Err(
            "Private dependency links must remain within the dependencies directory.".into(),
        );
    }
    Ok(path)
}

pub(crate) fn find_poppler(name: &str) -> Result<Option<PathBuf>, String> {
    if !["pdfinfo", "pdftotext", "pdftoppm"].contains(&name) {
        return Err("Unknown PDF processing tool".into());
    }
    let Some((root, dependencies)) = private_dependencies()? else {
        return Ok(executable_on_path(name));
    };
    let directory = private_path(&root, &dependencies.poppler_bin)?;
    let relative = directory
        .strip_prefix(&root)
        .expect("checked private path")
        .join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        });
    let tool = private_path(&root, &relative)?;
    if !tool.is_file() {
        return Err(format!(
            "Private Poppler tool is not a file: {}",
            tool.display()
        ));
    }
    Ok(Some(tool))
}

pub(crate) fn find_uv() -> Result<Option<PathBuf>, String> {
    if let Some((root, dependencies)) = private_dependencies()? {
        let tool = private_path(&root, &dependencies.uv)?;
        if !tool.is_file() {
            return Err(format!("Private uv is not a file: {}", tool.display()));
        }
        return Ok(Some(tool));
    }
    Ok(executable_on_path("uv").or_else(|| {
        let filename = if cfg!(windows) { "uv.exe" } else { "uv" };
        let mut candidates = Vec::new();
        for name in ["USERPROFILE", "HOME"] {
            if let Some(home) = std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
            {
                candidates.push(home.join(".local/bin").join(filename));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
        {
            candidates.push(local.join("Microsoft/WinGet/Links").join(filename));
        }
        candidates.into_iter().find(|path| path.is_file())
    }))
}

fn hidden_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

fn checked_output(output: Output, action: &str) -> Result<Output, String> {
    if output.status.success() {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!(
        "{action} failed ({}): {}",
        output.status,
        if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_profile_cannot_link_into_an_ordinary_browser_directory() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        crate::workspace::init_workspace(&workspace, None, None).unwrap();
        let data = storage::resolve_destination(&temporary.path().join("data")).unwrap();
        let expected = data.join("paperpile-browser/profile");
        assert_eq!(paperpile_profile(&workspace, &data).unwrap(), expected);
        assert!(!data.exists());
        fs::create_dir_all(expected.parent().unwrap()).unwrap();
        let normal = temporary.path().join("ordinary-chrome");
        fs::create_dir(&normal).unwrap();
        fs::write(normal.join("sentinel"), b"do not touch").unwrap();
        storage::link_directory(&normal, &expected);
        assert!(paperpile_profile(&workspace, &data).is_err());
        assert_eq!(fs::read(normal.join("sentinel")).unwrap(), b"do not touch");
        assert_eq!(fs::read_dir(&normal).unwrap().count(), 1);
    }

    #[test]
    fn private_dependencies_reject_traversal_absolute_paths_and_links_outside_root() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("dependencies");
        let outside = temporary.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("tool"), "fixture").unwrap();
        let root = fs::canonicalize(root).unwrap();
        crate::storage::link_directory(&outside, &root.join("linked"));
        for path in [
            Path::new(""),
            Path::new("../outside/tool"),
            outside.as_path(),
            Path::new("linked/tool"),
        ] {
            assert!(private_path(&root, path).is_err(), "{}", path.display());
        }
        fs::write(root.join("tool"), "fixture").unwrap();
        assert_eq!(
            private_path(&root, Path::new("tool")).unwrap(),
            root.join("tool")
        );
    }

    #[test]
    fn paths_and_arguments_are_passed_without_shell_interpolation() {
        let runtime = Runtime {
            package: PathBuf::from("研究 O'Brien package"),
            root: PathBuf::from("runtime"),
            fingerprint: "test".into(),
        };
        let store = Path::new("C:\\研究 O'Brien\\$env:HOME `test`\\research.sqlite");
        let args = runtime.arguments(store, &["search".into(), "a query with spaces".into()]);
        assert_eq!(&args[..2], ["-I", "-B"]);
        assert_eq!(args[8], store.to_string_lossy());
        assert_eq!(args[10], "a query with spaces");
        assert!(!ENTRYPOINT.contains("O'Brien"));
    }

    #[test]
    fn setup_does_not_open_or_migrate_an_existing_database() {
        let temporary = tempfile::tempdir().unwrap();
        let store = temporary.path().join("research.sqlite");
        fs::write(&store, b"existing format one database").unwrap();
        let runtime = Runtime {
            package: temporary.path().join("absent"),
            root: temporary.path().join("absent"),
            fingerprint: "test".into(),
        };
        assert!(!runtime
            .initialize_missing(temporary.path(), &store)
            .unwrap());
        assert_eq!(fs::read(store).unwrap(), b"existing format one database");
    }

    #[test]
    fn packaged_and_development_discovery_are_anchored_to_executable() {
        let temporary = tempfile::tempdir().unwrap();
        let package = temporary.path().join("research-engine");
        fs::create_dir_all(package.join("src/bukan_research")).unwrap();
        for file in ["pyproject.toml", "uv.lock", "src/bukan_research/cli.py"] {
            fs::write(package.join(file), "").unwrap();
        }
        let expected = fs::canonicalize(&package).unwrap();
        assert_eq!(
            discover_package(&temporary.path().join("bukan.exe")).unwrap(),
            expected
        );
        assert_eq!(
            discover_package(&temporary.path().join("bin/bukan.exe")).unwrap(),
            expected
        );
        assert!(discover_package(&temporary.path().join("elsewhere/bin/bukan.exe")).is_err());
        fs::create_dir_all(temporary.path().join("crates/bukan")).unwrap();
        fs::write(temporary.path().join("Cargo.toml"), "").unwrap();
        fs::write(temporary.path().join("crates/bukan/Cargo.toml"), "").unwrap();
        assert_eq!(
            discover_package(&temporary.path().join("target/debug/bukan.exe")).unwrap(),
            expected
        );
    }

    #[test]
    #[ignore = "downloads a private managed Python and locked production dependencies"]
    fn prepared_runtime_isolates_cwd_preserves_store_and_serves_mcp_stdio() {
        use std::io::{BufRead, Write};
        let temporary = tempfile::tempdir().unwrap();
        let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../research-engine")
            .canonicalize()
            .unwrap();
        let root = temporary.path().join("研究 O'Brien workspace");
        crate::workspace::init_workspace(&root, None, Some("missing offline library")).unwrap();
        let runtime = Runtime {
            fingerprint: package_fingerprint(&package).unwrap(),
            package,
            root: temporary.path().join("private runtime/version"),
        };
        runtime.prepare(&root, false).unwrap();
        assert!(runtime.ready());
        for module in ["pydantic.py", "mcp.py", "bukan_research.py"] {
            fs::write(
                root.join(module),
                "raise RuntimeError('caller module must never execute')\n",
            )
            .unwrap();
        }
        let store = storage::store_path(&root).unwrap();
        assert!(runtime.initialize_missing(&root, &store).unwrap());
        let original = fs::read(&store).unwrap();
        assert!(!runtime.initialize_missing(&root, &store).unwrap());
        assert_eq!(fs::read(&store).unwrap(), original);
        let output = hidden_command(&runtime.python())
            .args(runtime.arguments(&store, &["search".into(), "a query with spaces".into()]))
            .current_dir(&root)
            .env("PYTHONPATH", &root)
            .output()
            .unwrap();
        checked_output(output, "Isolated search").unwrap();
        let output = hidden_command(&runtime.python())
            .args(runtime.arguments(&store, &["invalid-command".into()]))
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let mut child = hidden_command(&runtime.python())
            .args(runtime.arguments(&store, &["serve".into()]))
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"bukan-test\",\"version\":\"1\"}}}\n").unwrap();
        let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], 1);
        assert!(response["result"]["capabilities"]["tools"].is_object());
        drop(input);
        assert!(child.wait().unwrap().success());
        assert_eq!(fs::read(&store).unwrap(), original);
        assert!(!root.join("__pycache__").exists());
    }
}
