use serde::Serialize;
use std::{process::Command, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

#[cfg(target_os = "windows")]
const CREDENTIAL_SERVICE: &str = "jp.bukan.literature";
#[cfg(target_os = "windows")]
const CREDENTIAL_USER: &str = "github-release-token";

#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAuthStatus {
    pub configured: bool,
    pub source: Option<String>,
    pub credential_storage_available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub auth_source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "event",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum UpdateDownloadEvent {
    Started {
        content_length: Option<u64>,
    },
    Progress {
        downloaded: u64,
        content_length: Option<u64>,
    },
    Finished,
}

#[tauri::command]
pub fn update_auth_status() -> UpdateAuthStatus {
    match resolve_github_token() {
        Ok((_, source)) => UpdateAuthStatus {
            configured: true,
            source: Some(source.to_string()),
            credential_storage_available: credential_storage_available(),
        },
        Err(_) => UpdateAuthStatus {
            configured: false,
            source: None,
            credential_storage_available: credential_storage_available(),
        },
    }
}

#[tauri::command]
pub fn save_update_github_token(token: String) -> Result<UpdateAuthStatus, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("GitHub tokenを入力してください".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        credential_entry()?.set_password(token).map_err(|error| {
            format!("Windows Credential Managerへ保存できませんでした: {error}")
        })?;
        Ok(UpdateAuthStatus {
            configured: true,
            source: Some("windows-credential-manager".to_string()),
            credential_storage_available: true,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = token;
        Err("このビルドではGitHub tokenの安全な保存に対応していません".to_string())
    }
}

#[tauri::command]
pub fn clear_update_github_token() -> Result<UpdateAuthStatus, String> {
    #[cfg(target_os = "windows")]
    {
        let entry = credential_entry()?;
        if entry.get_password().is_ok() {
            entry.delete_credential().map_err(|error| {
                format!("Windows Credential Managerから削除できませんでした: {error}")
            })?;
        }
    }
    Ok(update_auth_status())
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> Result<UpdateCheckResult, String> {
    let (token, source) = resolve_github_token()?;
    let update = app
        .updater_builder()
        .header("Authorization", format!("Bearer {token}"))
        .map_err(update_error)?
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(update_error)?
        .check()
        .await
        .map_err(|error| {
            format!(
                "private GitHub Releaseを確認できませんでした。tokenにContents: Read権限があるか確認してください: {error}"
            )
        })?;
    let result = match update.as_ref() {
        Some(update) => UpdateCheckResult {
            current_version: update.current_version.clone(),
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
            published_at: update.date.map(|date| date.to_string()),
            auth_source: source.to_string(),
        },
        None => UpdateCheckResult {
            current_version: app.package_info().version.to_string(),
            available: false,
            version: None,
            notes: None,
            published_at: None,
            auth_source: source.to_string(),
        },
    };
    *pending_update
        .0
        .lock()
        .map_err(|_| "更新状態を保存できませんでした".to_string())? = update;
    Ok(result)
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = pending_update
        .0
        .lock()
        .map_err(|_| "更新状態を読み取れませんでした".to_string())?
        .take()
        .ok_or_else(|| "先に更新を確認してください".to_string())?;

    let progress_app = app.clone();
    let finished_app = app.clone();
    let mut downloaded = 0_u64;
    let mut started = false;
    update
        .download_and_install(
            move |chunk_length, content_length| {
                if !started {
                    started = true;
                    let _ = progress_app.emit(
                        "app-update-progress",
                        UpdateDownloadEvent::Started { content_length },
                    );
                }
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let _ = progress_app.emit(
                    "app-update-progress",
                    UpdateDownloadEvent::Progress {
                        downloaded,
                        content_length,
                    },
                );
            },
            move || {
                let _ = finished_app.emit("app-update-progress", UpdateDownloadEvent::Finished);
            },
        )
        .await
        .map_err(|error| format!("更新をダウンロードまたは検証できませんでした: {error}"))?;
    app.restart();
}

fn resolve_github_token() -> Result<(String, &'static str), String> {
    #[cfg(target_os = "windows")]
    if let Ok(token) = credential_entry().and_then(|entry| {
        entry
            .get_password()
            .map_err(|error| format!("credential read failed: {error}"))
    }) {
        if !token.trim().is_empty() {
            return Ok((token, "windows-credential-manager"));
        }
    }

    for name in ["BUKAN_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = std::env::var(name) {
            if !token.trim().is_empty() {
                return Ok((token, "environment"));
            }
        }
    }

    if let Some(token) = github_cli_token() {
        return Ok((token, "github-cli"));
    }

    Err(
        "private GitHub Releaseへの認証がありません。GitHub tokenをAppへ保存するか、gh auth loginを実行してください"
            .to_string(),
    )
}

fn github_cli_token() -> Option<String> {
    let mut command = Command::new("gh");
    command.args(["auth", "token"]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

#[cfg(target_os = "windows")]
fn credential_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        .map_err(|error| format!("Windows Credential Managerを開けませんでした: {error}"))
}

fn credential_storage_available() -> bool {
    cfg!(target_os = "windows")
}

fn update_error(error: tauri_plugin_updater::Error) -> String {
    format!("Updaterを初期化できませんでした: {error}")
}
