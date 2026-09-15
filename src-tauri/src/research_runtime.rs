//! The packaged research engine and its private, writable Python environment.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Mutex,
};
use tauri::{AppHandle, Manager};

use crate::workspace;

const PYTHON_VERSION: &str = "3.12";
// -I removes the working directory and user site packages from the import path.
// The package directory is an argument, never interpolated into Python code.
const ENTRYPOINT: &str =
    "import sys; sys.path.insert(0, sys.argv.pop(1)); from bukan_research.cli import main; main()";
const IMPORT_CHECK: &str = "import sys; sys.path.insert(0, sys.argv[1]); import markdown_it, mcp, pydantic, bukan_research.cli, bukan_research.server";
// Windows PowerShell 5 strips embedded double quotes from native arguments.
// Transport the structured payload as base64 so Codex's TOML override needs only
// simple literal strings, including when paths contain apostrophes or spaces.
const MCP_ENTRYPOINT: &str = "import base64,json,sys; a=json.loads(base64.b64decode(sys.argv[1])); sys.argv=[sys.argv[0]]+a[1:]; exec(a[0])";
static PREPARE_LOCK: Mutex<()> = Mutex::new(());
static INITIALIZE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRuntimeStatus {
    pub available: bool,
    pub ready: bool,
    pub detail: String,
}

struct Runtime {
    package: PathBuf,
    root: PathBuf,
    fingerprint: String,
}

impl Runtime {
    fn discover(app: &AppHandle) -> Result<Self, String> {
        let resources = app
            .path()
            .resource_dir()
            .map_err(|error| format!("研究エンジンの同梱先を取得できませんでした: {error}"))?;
        let package = package_root(&resources)?;
        let fingerprint = package_fingerprint(&package)?;
        let root = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("研究エンジンの保存先を取得できませんでした: {error}"))?
            .join("research-runtime")
            .join(&fingerprint);
        validate_storage_location(&resolve_destination(&root)?)?;
        Ok(Self {
            package,
            root,
            fingerprint,
        })
    }

    fn python(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.root.join("venv/Scripts/python.exe")
        } else {
            self.root.join("venv/bin/python")
        }
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
            Err("研究ホームの「研究環境を準備」から初期設定を完了してください".into())
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

    fn mcp_arguments(&self, store: &Path) -> Vec<String> {
        let direct = self.arguments(store, &["serve".into()]);
        let payload = BASE64.encode(serde_json::to_vec(&direct[5..]).expect("string arguments"));
        let mut result = direct[..5].to_vec();
        result.extend([MCP_ENTRYPOINT.into(), payload]);
        result
    }

    fn execute(
        &self,
        workspace: &Path,
        store: &Path,
        args: &[String],
        stdin: Option<&str>,
    ) -> Result<Value, String> {
        let mut command = hidden_command(&self.python());
        command
            .args(self.arguments(store, args))
            .current_dir(workspace)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("研究エンジンを起動できませんでした: {error}"))?;
        if let Some(input) = stdin {
            let result = child.stdin.take().unwrap().write_all(input.as_bytes());
            if let Err(error) = result {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("研究エンジンへ入力を渡せませんでした: {error}"));
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("研究エンジンの結果を取得できませんでした: {error}"))?;
        checked_output(output, "研究エンジン").and_then(|output| {
            serde_json::from_slice(&output.stdout).map_err(|error| {
                format!("研究エンジンから正しいJSONを受け取れませんでした: {error}")
            })
        })
    }
}

pub fn status(app: &AppHandle) -> Result<ResearchRuntimeStatus, String> {
    match Runtime::discover(app) {
        Ok(runtime) => Ok(ResearchRuntimeStatus {
            available: true,
            ready: runtime.ready(),
            detail: if runtime.ready() {
                "研究エンジンの準備ができています".into()
            } else {
                "初回のみ研究環境を準備します。Pythonと必要なパッケージをダウンロードします".into()
            },
        }),
        Err(detail) => Ok(ResearchRuntimeStatus {
            available: false,
            ready: false,
            detail,
        }),
    }
}

/// Called only by the explicit in-app setup action, never by status polling.
pub fn prepare(app: &AppHandle) -> Result<ResearchRuntimeStatus, String> {
    let _guard = PREPARE_LOCK
        .lock()
        .map_err(|_| "研究環境の準備状態を取得できませんでした".to_string())?;
    let runtime = Runtime::discover(app)?;
    prepare_runtime(&runtime)?;
    status(app)
}

fn prepare_runtime(runtime: &Runtime) -> Result<(), String> {
    if !runtime.ready() {
        let uv = find_uv().map(Ok).unwrap_or_else(install_uv)?;
        fs::create_dir_all(&runtime.root)
            .map_err(|error| format!("研究環境の保存先を作成できませんでした: {error}"))?;
        let shared = runtime.root.parent().expect("runtime version directory");
        let output = hidden_command(&uv)
            .args([
                "sync",
                "--locked",
                "--no-dev",
                "--no-install-project",
                "--managed-python",
                "--python",
                PYTHON_VERSION,
                "--project",
            ])
            .arg(&runtime.package)
            .env("UV_PROJECT_ENVIRONMENT", runtime.root.join("venv"))
            .env("UV_CACHE_DIR", shared.join("cache"))
            .env("UV_PYTHON_INSTALL_DIR", shared.join("python"))
            .current_dir(&runtime.root)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("研究環境の準備を開始できませんでした: {error}"))?;
        checked_output(output, "研究環境の準備")?;
        let output = hidden_command(&runtime.python())
            .args(["-I", "-B", "-X", "utf8", "-c", IMPORT_CHECK])
            .arg(runtime.package.join("src"))
            .current_dir(&runtime.root)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("研究エンジンを検証できませんでした: {error}"))?;
        checked_output(output, "研究エンジンの検証")?;
        fs::write(runtime.root.join("ready"), &runtime.fingerprint)
            .map_err(|error| format!("研究環境の準備完了を保存できませんでした: {error}"))?;
    }
    Ok(())
}

pub fn run(
    app: &AppHandle,
    workspace: &Path,
    args: &[String],
    stdin: Option<&str>,
) -> Result<Value, String> {
    let runtime = Runtime::discover(app)?;
    runtime.require_ready()?;
    let store = store_path(workspace)?;
    let workspace = fs::canonicalize(workspace).map_err(|error| error.to_string())?;
    // Initialize only a new store; existing revisions and notes are retained.
    {
        let _guard = INITIALIZE_LOCK
            .lock()
            .map_err(|_| "研究データの初期化状態を取得できませんでした".to_string())?;
        if !store.exists() {
            let initialized = runtime.execute(&workspace, &store, &["init".into()], None)?;
            if args == ["init"] {
                return Ok(initialized);
            }
        }
    }
    runtime.execute(&workspace, &store, args, stdin)
}

pub fn mcp_config_overrides(app: &AppHandle, workspace: &Path) -> Result<Vec<String>, String> {
    // A configured server must be able to import its dependencies and open its DB.
    run(app, workspace, &["init".into()], None)?;
    let runtime = Runtime::discover(app)?;
    let store = store_path(workspace)?;
    Ok(vec![
        format!(
            "mcp_servers.bukan_research.command={}",
            runtime.python().to_string_lossy()
        ),
        format!(
            "mcp_servers.bukan_research.args=[{}]",
            runtime
                .mcp_arguments(&store)
                .iter()
                .map(|value| format!("'{value}'"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        "mcp_servers.bukan_research.required=true".into(),
        "mcp_servers.bukan_research.startup_timeout_sec=30".into(),
    ])
}

/// Resolve even not-yet-created destinations before allowing research writes.
pub fn store_path(root: &Path) -> Result<PathBuf, String> {
    let (root, config) = workspace::load_workspace(root)?;
    let store = resolve_destination(&root.join("data/research.sqlite"))?;
    if !store.starts_with(&root) {
        return Err("研究データの保存先はワークスペース内にしてください".into());
    }
    validate_storage_location(&store)?;
    if !config.paperpile.path.eq_ignore_ascii_case("auto") {
        let configured = PathBuf::from(&config.paperpile.path);
        let configured = if configured.is_absolute() {
            configured
        } else {
            root.join(configured)
        };
        // Historical research remains usable while a configured Drive is offline.
        // A missing source cannot contain the existing workspace directory.
        if resolve_destination(&configured).is_ok_and(|configured| store.starts_with(configured)) {
            return Err("Paperpile内には研究データを保存できません".into());
        }
    }
    Ok(store)
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
        if parent.join("src-tauri/Cargo.toml").is_file() {
            return Err("研究データはBukanのソースリポジトリの外に保存してください".into());
        }
    }
    Ok(())
}

pub(crate) fn resolve_destination(path: &Path) -> Result<PathBuf, String> {
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

fn package_root(resources: &Path) -> Result<PathBuf, String> {
    let bundled = resources.join("research-engine");
    if bundled.join("pyproject.toml").is_file() {
        return Ok(bundled);
    }
    #[cfg(debug_assertions)]
    {
        let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../research-engine");
        if development.join("pyproject.toml").is_file() {
            return Ok(development);
        }
    }
    Err("研究エンジンの同梱ファイルが見つかりません。Bukanを再インストールしてください".into())
}

fn package_fingerprint(package: &Path) -> Result<String, String> {
    let mut files = vec![package.join("pyproject.toml"), package.join("uv.lock")];
    for entry in walkdir::WalkDir::new(package.join("src/bukan_research")) {
        let entry =
            entry.map_err(|error| format!("研究エンジンを確認できませんでした: {error}"))?;
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
        hash.update(&fs::read(&file).map_err(|error| {
            format!(
                "研究エンジンの {} を読めませんでした: {error}",
                file.display()
            )
        })?);
    }
    Ok(hash.finalize().to_hex()[..20].to_string())
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn find_uv() -> Option<PathBuf> {
    let executable = if cfg!(target_os = "windows") {
        "uv.exe"
    } else {
        "uv"
    };
    executable_on_path(executable).or_else(|| {
        let mut candidates = Vec::new();
        for variable in ["USERPROFILE", "HOME"] {
            if let Some(home) = std::env::var_os(variable) {
                candidates.push(PathBuf::from(home).join(".local/bin").join(executable));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(local)
                    .join("Microsoft/WinGet/Links")
                    .join(executable),
            );
        }
        candidates.into_iter().find(|path| path.is_file())
    })
}

fn install_uv() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let winget = executable_on_path("winget.exe").ok_or_else(|| {
            "研究環境の準備にはWindowsの「アプリ インストーラー」が必要です。Microsoft Storeでインストールしてから再試行してください".to_string()
        })?;
        let output = hidden_command(&winget)
            .args([
                "install",
                "--exact",
                "--id",
                "astral-sh.uv",
                "--source",
                "winget",
                "--scope",
                "user",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity",
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("uvのインストールを開始できませんでした: {error}"))?;
        checked_output(output, "uvのインストール")?;
        find_uv().ok_or_else(|| {
            "uvをインストールしました。Bukanを再起動して研究環境の準備を再試行してください".into()
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("研究環境の準備にはuvが必要です。https://docs.astral.sh/uv/getting-started/installation/ に従ってインストールしてから再試行してください".into())
    }
}

fn hidden_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
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
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    Err(format!("{action}に失敗しました: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_apostrophes_unicode_and_shell_metacharacters_in_mcp_arguments() {
        let path = "C:\\研究 O'Brien\\$env:HOME `test`\\research.sqlite";
        let runtime = Runtime {
            package: PathBuf::from("package's directory"),
            root: PathBuf::from("runtime"),
            fingerprint: "test".into(),
        };
        let arguments = runtime.arguments(Path::new(path), &["serve".into()]);
        assert_eq!(arguments[0], "-I");
        assert_eq!(arguments[5], ENTRYPOINT);
        assert_eq!(arguments[8], path);
        assert_eq!(arguments[9], "serve");
        assert!(!ENTRYPOINT.contains(path));
        let mcp = runtime.mcp_arguments(Path::new(path));
        assert!(mcp.iter().all(|argument| !argument.contains(['\'', '"'])));
        let payload: Vec<String> =
            serde_json::from_slice(&BASE64.decode(&mcp[6]).unwrap()).unwrap();
        assert_eq!(&payload, &arguments[5..]);
    }

    #[test]
    fn resolves_new_store_without_creating_it() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("research");
        workspace::init_workspace(&root, None, Some("auto")).unwrap();
        let store = store_path(&root).unwrap();
        assert_eq!(
            store,
            fs::canonicalize(&root)
                .unwrap()
                .join("data/research.sqlite")
        );
        assert!(!store.exists());
    }

    #[test]
    fn refuses_stores_in_source_repository_and_paperpile() {
        let temporary = tempfile::tempdir().unwrap();
        for (directory, marker) in [
            ("source", "src-tauri/Cargo.toml"),
            ("Paperpile", "All Papers/marker"),
        ] {
            let parent = temporary.path().join(directory);
            let marker = parent.join(marker);
            fs::create_dir_all(marker.parent().unwrap()).unwrap();
            fs::write(marker, "").unwrap();
            let root = parent.join("workspace");
            fs::create_dir_all(root.join("data")).unwrap();
            fs::write(root.join("bukan.toml"), "version=1\nname='test'\n[paperpile]\npath='auto'\nread_only=true\n[bibliography]\npath='data/paperpile.bib'\n").unwrap();
            assert!(store_path(&root).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_data_symlink_outside_the_workspace() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("research");
        workspace::init_workspace(&root, None, Some("auto")).unwrap();
        fs::remove_dir(root.join("data")).unwrap();
        std::os::unix::fs::symlink(temporary.path(), root.join("data")).unwrap();
        assert!(store_path(&root).is_err());
    }

    #[test]
    fn resource_mapping_contains_only_runtime_inputs_and_fingerprint_tracks_code() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let resources = config["bundle"]["resources"].as_object().unwrap();
        assert_eq!(
            resources["../research-engine/pyproject.toml"],
            "research-engine/pyproject.toml"
        );
        assert_eq!(
            resources["../research-engine/uv.lock"],
            "research-engine/uv.lock"
        );
        assert_eq!(
            resources["../research-engine/src/bukan_research/*.py"],
            "research-engine/src/bukan_research/"
        );
        let temporary = tempfile::tempdir().unwrap();
        let package = temporary.path();
        fs::create_dir_all(package.join("src/bukan_research")).unwrap();
        fs::write(package.join("pyproject.toml"), "project").unwrap();
        fs::write(package.join("uv.lock"), "lock").unwrap();
        fs::write(package.join("src/bukan_research/cli.py"), "before").unwrap();
        let before = package_fingerprint(package).unwrap();
        fs::write(package.join("src/bukan_research/cli.py"), "after").unwrap();
        assert_ne!(package_fingerprint(package).unwrap(), before);
    }

    #[test]
    #[ignore = "downloads a private Python runtime and locked dependencies"]
    fn prepares_packaged_engine_and_preserves_store_without_importing_workspace_code() {
        let temporary = tempfile::tempdir().unwrap();
        let package = temporary.path().join("bundled resources/research-engine");
        fs::create_dir_all(package.join("src/bukan_research")).unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../research-engine");
        for name in ["pyproject.toml", "uv.lock"] {
            fs::copy(source.join(name), package.join(name)).unwrap();
        }
        for entry in fs::read_dir(source.join("src/bukan_research")).unwrap() {
            let entry = entry.unwrap();
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "py")
            {
                fs::copy(
                    entry.path(),
                    package.join("src/bukan_research").join(entry.file_name()),
                )
                .unwrap();
            }
        }
        let runtime = Runtime {
            fingerprint: package_fingerprint(&package).unwrap(),
            package,
            root: temporary.path().join("app data/runtime/version"),
        };
        prepare_runtime(&runtime).unwrap();
        assert!(runtime.ready());
        let root = temporary.path().join("研究 O'Brien workspace");
        workspace::init_workspace(&root, None, Some("auto")).unwrap();
        fs::write(
            root.join("pydantic.py"),
            "raise RuntimeError('workspace import executed')",
        )
        .unwrap();
        let store = store_path(&root).unwrap();
        let initialized = runtime
            .execute(&root, &store, &["init".into()], None)
            .unwrap();
        let before = fs::read(&store).unwrap();
        let again = runtime
            .execute(&root, &store, &["init".into()], None)
            .unwrap();
        assert_eq!(initialized, again);
        assert_eq!(fs::read(&store).unwrap(), before);
        assert!(!runtime.package.join(".venv").exists());
        assert!(!runtime
            .package
            .join("src/bukan_research/__pycache__")
            .exists());
        let mut child = hidden_command(&runtime.python())
            .args(runtime.mcp_arguments(&store))
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
        std::io::BufRead::read_line(&mut output, &mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], 1);
        assert!(response["result"]["capabilities"]["tools"].is_object());
        drop(input);
        assert!(child.wait().unwrap().success());
    }
}
