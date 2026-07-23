use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Emitter};

use crate::{is_pdf, workspace};

const DEFAULT_COLS: u16 = 110;
const DEFAULT_ROWS: u16 = 32;

pub struct CodexTerminalState {
    session: Mutex<Option<CodexSession>>,
    next_session_id: AtomicU64,
}

struct CodexSession {
    id: u64,
    workspace_root: PathBuf,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Debug, Clone)]
enum CodexCommand {
    Direct(PathBuf),
    CommandScript(PathBuf),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRuntimeStatus {
    pub available: bool,
    pub version: Option<String>,
    pub command: Option<String>,
    pub running: bool,
    pub session_id: Option<u64>,
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexTerminalOutput {
    session_id: u64,
    data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexTerminalExit {
    session_id: u64,
}

impl Default for CodexTerminalState {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            next_session_id: AtomicU64::new(1),
        }
    }
}

impl Drop for CodexTerminalState {
    fn drop(&mut self) {
        if let Ok(session) = self.session.get_mut() {
            if let Some(session) = session.as_mut() {
                let _ = session.child.kill();
                let _ = session.child.wait();
            }
        }
    }
}

impl CodexTerminalState {
    pub fn status(&self) -> Result<CodexRuntimeStatus, String> {
        let command = find_codex_command();
        let version = command
            .as_ref()
            .and_then(|command| codex_version(command).ok());
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Codexターミナルの状態を読み取れませんでした".to_string())?;
        clear_finished_session(&mut session);

        Ok(CodexRuntimeStatus {
            available: command.is_some(),
            version,
            command: command.as_ref().map(command_display),
            running: session.is_some(),
            session_id: session.as_ref().map(|session| session.id),
            workspace_root: session
                .as_ref()
                .map(|session| session.workspace_root.to_string_lossy().into_owned()),
        })
    }

    pub fn start(
        &self,
        app: AppHandle,
        workspace_root: &Path,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> Result<CodexRuntimeStatus, String> {
        let (workspace_root, config) = workspace::load_workspace(workspace_root)?;
        let paperpile_root = workspace::resolve_paperpile_root(&workspace_root, &config)?;
        let command = find_codex_command().ok_or_else(|| {
            "Codex CLIが見つかりません。Codexをインストールしてから再試行してください".to_string()
        })?;
        let mcp_overrides = mcp_config_overrides(&workspace_root)?;
        let size = PtySize {
            rows: rows.unwrap_or(DEFAULT_ROWS).clamp(8, 500),
            cols: cols.unwrap_or(DEFAULT_COLS).clamp(20, 500),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|error| format!("Codex用ターミナルを作成できませんでした: {error}"))?;
        let mut command_builder = command_builder(&command, &workspace_root, &mcp_overrides);
        command_builder.cwd(&workspace_root);
        command_builder.env("TERM", "xterm-256color");
        command_builder.env("COLORTERM", "truecolor");
        command_builder.env("BUKAN_WORKSPACE", &workspace_root);
        command_builder.env(
            "BUKAN_CONTEXT_FILE",
            workspace_root.join(".bukan").join("current-context.md"),
        );
        if let Some(paperpile_root) = paperpile_root {
            command_builder.env("BUKAN_PAPERPILE_ROOT", paperpile_root);
            command_builder.env("BUKAN_PAPERPILE_READ_ONLY", "true");
        }

        let child = pair
            .slave
            .spawn_command(command_builder)
            .map_err(|error| format!("Codexを起動できませんでした: {error}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Codexの出力を読み取れませんでした: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Codexの入力を準備できませんでした: {error}"))?;
        let session_id = self.next_session_id.fetch_add(1, Ordering::Relaxed);

        let mut session_guard = self
            .session
            .lock()
            .map_err(|_| "Codexターミナルの状態を更新できませんでした".to_string())?;
        stop_session(session_guard.as_mut());
        *session_guard = Some(CodexSession {
            id: session_id,
            workspace_root: workspace_root.clone(),
            master: pair.master,
            writer,
            child,
        });
        drop(session_guard);

        std::thread::spawn(move || {
            let mut buffer = [0_u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(length) => {
                        let data = String::from_utf8_lossy(&buffer[..length]).into_owned();
                        let _ = app.emit(
                            "codex-terminal-output",
                            CodexTerminalOutput { session_id, data },
                        );
                    }
                }
            }
            let _ = app.emit("codex-terminal-exit", CodexTerminalExit { session_id });
        });

        Ok(CodexRuntimeStatus {
            available: true,
            version: codex_version(&command).ok(),
            command: Some(command_display(&command)),
            running: true,
            session_id: Some(session_id),
            workspace_root: Some(workspace_root.to_string_lossy().into_owned()),
        })
    }

    pub fn write(&self, data: &str) -> Result<(), String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Codexターミナルの状態を読み取れませんでした".to_string())?;
        let session = session
            .as_mut()
            .ok_or_else(|| "Codexは起動していません".to_string())?;
        session
            .writer
            .write_all(data.as_bytes())
            .and_then(|_| session.writer.flush())
            .map_err(|error| format!("Codexへ入力を送信できませんでした: {error}"))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let session = self
            .session
            .lock()
            .map_err(|_| "Codexターミナルの状態を読み取れませんでした".to_string())?;
        let session = session
            .as_ref()
            .ok_or_else(|| "Codexは起動していません".to_string())?;
        session
            .master
            .resize(PtySize {
                rows: rows.clamp(8, 500),
                cols: cols.clamp(20, 500),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Codexターミナルをリサイズできませんでした: {error}"))
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "Codexターミナルの状態を更新できませんでした".to_string())?;
        stop_session(session.as_mut());
        *session = None;
        Ok(())
    }
}

fn clear_finished_session(session: &mut Option<CodexSession>) {
    let finished = session
        .as_mut()
        .and_then(|session| session.child.try_wait().ok())
        .flatten()
        .is_some();
    if finished {
        *session = None;
    }
}

fn stop_session(session: Option<&mut CodexSession>) {
    if let Some(session) = session {
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
}

fn find_codex_command() -> Option<CodexCommand> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("where.exe").arg("codex").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let paths: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect();
        if let Some(path) = paths.iter().find(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        }) {
            return Some(CodexCommand::Direct(path.clone()));
        }
        paths
            .into_iter()
            .find(|path| {
                path.extension().is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
                })
            })
            .map(CodexCommand::CommandScript)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("sh")
            .args(["-lc", "command -v codex"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!path.is_empty()).then(|| CodexCommand::Direct(PathBuf::from(path)))
    }
}

fn command_builder(
    command: &CodexCommand,
    workspace_root: &Path,
    config_overrides: &[String],
) -> CommandBuilder {
    match command {
        CodexCommand::Direct(path) => {
            let mut builder = CommandBuilder::new(path);
            builder.arg("-C");
            builder.arg(workspace_root);
            builder.arg("--sandbox");
            builder.arg("workspace-write");
            builder.arg("--ask-for-approval");
            builder.arg("on-request");
            for config_override in config_overrides {
                builder.arg("--config");
                builder.arg(config_override);
            }
            builder
        }
        CodexCommand::CommandScript(path) => {
            #[cfg(target_os = "windows")]
            {
                let mut builder = CommandBuilder::new("cmd.exe");
                let script = windows_command_string(path, workspace_root, config_overrides);
                builder.args(["/D", "/S", "/C", script.as_str()]);
                builder
            }
            #[cfg(not(target_os = "windows"))]
            {
                let mut builder = CommandBuilder::new(path);
                builder.arg("-C");
                builder.arg(workspace_root);
                builder.arg("--sandbox");
                builder.arg("workspace-write");
                builder.arg("--ask-for-approval");
                builder.arg("on-request");
                for config_override in config_overrides {
                    builder.arg("--config");
                    builder.arg(config_override);
                }
                builder
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_command_string(
    command: &Path,
    workspace_root: &Path,
    config_overrides: &[String],
) -> String {
    fn escape_percent(value: &str) -> String {
        value.replace('%', "%%")
    }
    let mut value = format!(
        "\"{}\" -C \"{}\" --sandbox workspace-write --ask-for-approval on-request",
        escape_percent(&command.to_string_lossy()),
        escape_percent(&workspace_root.to_string_lossy())
    );
    for config_override in config_overrides {
        value.push_str(" --config \"");
        value.push_str(&escape_percent(config_override));
        value.push('"');
    }
    value
}

fn mcp_config_overrides(workspace_root: &Path) -> Result<Vec<String>, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Bukan MCPの実行ファイルを特定できませんでした: {error}"))?;
    let executable = executable.to_string_lossy();
    let workspace_root = workspace_root.to_string_lossy();
    if executable.contains('\'') || workspace_root.contains('\'') {
        return Err(
            "BukanまたはWorkspaceのパスにアポストロフィがあるためMCPを設定できません".to_string(),
        );
    }
    Ok(vec![
        format!("mcp_servers.bukan.command='{executable}'"),
        "mcp_servers.bukan.args=['--bukan-mcp-stdio']".to_string(),
        format!("mcp_servers.bukan.env.BUKAN_WORKSPACE='{workspace_root}'"),
        "mcp_servers.bukan.required=true".to_string(),
    ])
}

fn command_display(command: &CodexCommand) -> String {
    match command {
        CodexCommand::Direct(path) | CodexCommand::CommandScript(path) => {
            path.to_string_lossy().into_owned()
        }
    }
}

fn codex_version(command: &CodexCommand) -> Result<String, String> {
    let output = match command {
        CodexCommand::Direct(path) => Command::new(path).arg("--version").output(),
        CodexCommand::CommandScript(path) => {
            #[cfg(target_os = "windows")]
            {
                Command::new("cmd.exe")
                    .args([
                        "/D",
                        "/S",
                        "/C",
                        &format!(
                            "\"{}\" --version",
                            path.to_string_lossy().replace('%', "%%")
                        ),
                    ])
                    .output()
            }
            #[cfg(not(target_os = "windows"))]
            {
                Command::new(path).arg("--version").output()
            }
        }
    }
    .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn write_current_context(
    workspace_root: &Path,
    paper_path: &Path,
    paper_id: &str,
    title: &str,
    authors: Option<&str>,
    year: Option<u16>,
    collections: &[String],
) -> Result<PathBuf, String> {
    let (workspace_root, config) = workspace::load_workspace(workspace_root)?;
    let paperpile_root = workspace::resolve_paperpile_root(&workspace_root, &config)?
        .ok_or_else(|| "Paperpileライブラリが見つかりません".to_string())?;
    let paper_path =
        fs::canonicalize(paper_path).map_err(|_| "選択されたPDFが見つかりません".to_string())?;
    if !paper_path.starts_with(&paperpile_root) || !is_pdf(&paper_path) {
        return Err("Paperpileライブラリ内のPDFだけをCodexコンテキストに設定できます".to_string());
    }

    let managed_dir = workspace_root.join(".bukan");
    fs::create_dir_all(&managed_dir)
        .map_err(|error| format!("Codexコンテキスト用フォルダを作成できませんでした: {error}"))?;
    let context_path = managed_dir.join("current-context.md");
    let relative_path = paper_path
        .strip_prefix(&paperpile_root)
        .unwrap_or(&paper_path);
    let metadata = [
        authors
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        year.map(|value| value.to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let collection_text = if collections.is_empty() {
        "未分類".to_string()
    } else {
        collections.join(" · ")
    };
    let contents = format!(
        "# Bukan current paper context\n\n\
         This file is managed by Bukan. The Paperpile source is read-only.\n\n\
         - Paper ID: `{}`\n\
         - Title: {}\n\
         - Metadata: {}\n\
         - Collections: {}\n\
         - Paperpile relative path: `{}`\n\
         - PDF path: `{}`\n\n\
         When answering questions about this paper, inspect the PDF and cite its page or section. \
         Never modify, move, rename, or delete the source PDF.\n",
        markdown_inline(paper_id),
        markdown_inline(title),
        markdown_inline(if metadata.is_empty() {
            "書誌情報なし"
        } else {
            &metadata
        }),
        markdown_inline(&collection_text),
        markdown_inline(&relative_path.to_string_lossy()),
        markdown_inline(&paper_path.to_string_lossy()),
    );
    fs::write(&context_path, contents)
        .map_err(|error| format!("Codexコンテキストを保存できませんでした: {error}"))?;
    Ok(context_path)
}

pub fn clear_current_context(workspace_root: &Path) -> Result<(), String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let context_path = workspace_root.join(".bukan").join("current-context.md");
    if context_path.is_file() {
        fs::remove_file(&context_path)
            .map_err(|error| format!("Codexコンテキストを解除できませんでした: {error}"))?;
    }
    Ok(())
}

fn markdown_inline(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_context_only_for_paperpile_pdf() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paperpile = temporary.path().join("Paperpile");
        let all_papers = paperpile.join("All Papers");
        fs::create_dir_all(&all_papers).expect("Paperpile tree");
        let pdf = all_papers.join("Sato 2025 - Lunar plasma.pdf");
        fs::write(&pdf, b"%PDF-test").expect("paper");
        let workspace_root = temporary.path().join("workspace");
        workspace::init_workspace(
            &workspace_root,
            Some("Lunar"),
            Some(&paperpile.to_string_lossy()),
        )
        .expect("workspace");

        let path = write_current_context(
            &workspace_root,
            &pdf,
            "paper-1",
            "Lunar plasma",
            Some("Sato"),
            Some(2025),
            &["My Papers / Plasma".to_string()],
        )
        .expect("context");
        let contents = fs::read_to_string(path).expect("context contents");

        assert!(contents.contains("Lunar plasma"));
        assert!(contents.contains("read-only"));
        assert!(contents.contains("Sato 2025 - Lunar plasma.pdf"));
    }

    #[test]
    fn rejects_context_outside_paperpile() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paperpile = temporary.path().join("Paperpile");
        fs::create_dir_all(paperpile.join("All Papers")).expect("Paperpile tree");
        let outside_pdf = temporary.path().join("outside.pdf");
        fs::write(&outside_pdf, b"%PDF-test").expect("outside paper");
        let workspace_root = temporary.path().join("workspace");
        workspace::init_workspace(&workspace_root, None, Some(&paperpile.to_string_lossy()))
            .expect("workspace");

        let error = write_current_context(
            &workspace_root,
            &outside_pdf,
            "outside",
            "Outside",
            None,
            None,
            &[],
        )
        .expect_err("outside PDF must fail");

        assert!(error.contains("Paperpileライブラリ内"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn quotes_windows_codex_command_paths() {
        let command = windows_command_string(
            Path::new(r"C:\Users\Test User\AppData\Roaming\npm\codex.cmd"),
            Path::new(r"G:\マイドライブ\Bukan Workspaces\Lunar 100%"),
            &[
                "mcp_servers.bukan.command='C:\\Program Files\\Bukan\\bukan.exe'".to_string(),
                "mcp_servers.bukan.args=['--bukan-mcp-stdio']".to_string(),
            ],
        );
        assert!(command.contains("\"C:\\Users\\Test User"));
        assert!(command.contains("Lunar 100%%\""));
        assert!(command.contains("--sandbox workspace-write"));
        assert!(command.contains("mcp_servers.bukan.command="));
    }
}
