//! Packaged research engine with an isolated, per-user Python environment.
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output, Stdio},
};

use crate::{settings, storage};

const PYTHON_VERSION: &str = "3.12";
// -I excludes caller modules/user packages; paths are arguments, never Python code.
const ENTRYPOINT: &str =
    "import sys; sys.path.insert(0, sys.argv.pop(1)); from bukan_research.cli import main; main()";
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

    fn check_imports(&self) -> Result<(), String> {
        let output = hidden_command(&self.python())
            .args(["-I", "-B", "-X", "utf8", "-c", IMPORT_CHECK])
            .arg(self.package.join("src"))
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("Could not verify the research runtime: {error}"))?;
        checked_output(output, "Research runtime verification").map(|_| ())
    }

    fn prepare(&self, workspace: &Path) -> Result<(), String> {
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
        if self.ready() {
            return self.check_imports();
        }
        let uv = find_uv().ok_or_else(|| "uv was not found. Install uv from https://docs.astral.sh/uv/getting-started/installation/ and run bukan setup <workspace> again.".to_string())?;
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("Could not create the runtime directory: {error}"))?;
        let output = hidden_command(&uv)
            .args([
                "--no-config",
                "sync",
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
        self.check_imports()?;
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
                runtime.check_imports()
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
    runtime.prepare(workspace)?;
    runtime.initialize_missing(workspace, &store)
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

pub(crate) fn find_uv() -> Option<PathBuf> {
    executable_on_path("uv").or_else(|| {
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
    })
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
        runtime.prepare(&root).unwrap();
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
