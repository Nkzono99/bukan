use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf, process::Command, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Nkzono99/bukan/releases/latest";
const GITHUB_API_VERSION: &str = "2022-11-28";

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
    pub detail: Option<String>,
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

#[derive(Debug)]
struct GithubToken {
    value: String,
    source: &'static str,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    url: String,
    browser_download_url: String,
}

#[tauri::command]
pub fn update_auth_status() -> UpdateAuthStatus {
    match github_token_candidates().into_iter().next() {
        Some(token) => UpdateAuthStatus {
            configured: true,
            source: Some(token.source.to_string()),
            credential_storage_available: credential_storage_available(),
            detail: None,
        },
        None => UpdateAuthStatus {
            configured: false,
            source: None,
            credential_storage_available: credential_storage_available(),
            detail: Some(missing_github_auth_message()),
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
            detail: None,
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
    let candidates = github_token_candidates();
    if candidates.is_empty() {
        return Err(missing_github_auth_message());
    }

    let mut failures = Vec::new();
    let mut authenticated_update = None;
    for candidate in candidates {
        match check_with_github_token(&app, &candidate).await {
            Ok(update) => {
                authenticated_update = Some((update, candidate.source));
                break;
            }
            Err(error) => {
                failures.push(format!("{}: {error}", auth_source_label(candidate.source)))
            }
        }
    }
    let (update, source) = authenticated_update.ok_or_else(|| {
        format!(
            "private GitHub Releaseを確認できませんでした。{}",
            failures.join(" / ")
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

async fn check_with_github_token(
    app: &AppHandle,
    token: &GithubToken,
) -> Result<Option<Update>, String> {
    let release = fetch_latest_release(&token.value).await?;
    let metadata = release
        .assets
        .iter()
        .find(|asset| asset.name == "latest.json")
        .ok_or_else(|| "最新Releaseにlatest.jsonがありません".to_string())?;
    let endpoint = metadata
        .url
        .parse()
        .map_err(|error| format!("latest.jsonのAPI URLが不正です: {error}"))?;
    let mut update = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(update_error)?
        .header("Authorization", format!("Bearer {}", token.value))
        .map_err(update_error)?
        .header("Accept", "application/octet-stream")
        .map_err(update_error)?
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .map_err(update_error)?
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(update_error)?
        .check()
        .await
        .map_err(|error| format!("更新メタデータを読み取れませんでした: {error}"))?;

    if let Some(update) = update.as_mut() {
        let installer = release_asset_for_download(&release, update.download_url.as_str())
            .ok_or_else(|| "更新インストーラーがGitHub Release assetsにありません".to_string())?;
        update.download_url = installer
            .url
            .parse()
            .map_err(|error| format!("インストーラーのAPI URLが不正です: {error}"))?;
    }
    Ok(update)
}

async fn fetch_latest_release(token: &str) -> Result<GithubRelease, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("Bukan updater")
        .build()
        .map_err(|error| format!("GitHub接続を準備できませんでした: {error}"))?;
    let response = client
        .get(GITHUB_LATEST_RELEASE_API)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .send()
        .await
        .map_err(|error| format!("GitHub APIへ接続できませんでした: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => "tokenが無効または期限切れです".to_string(),
            403 => "tokenにこのprivateリポジトリのContents: Read権限がありません".to_string(),
            404 => {
                "privateリポジトリまたは公開済みReleaseへアクセスできません。tokenの対象リポジトリを確認してください"
                    .to_string()
            }
            code => format!("GitHub APIがHTTP {code}を返しました"),
        });
    }
    response
        .json()
        .await
        .map_err(|error| format!("GitHub Release情報を読み取れませんでした: {error}"))
}

fn release_asset_for_download<'a>(
    release: &'a GithubRelease,
    download_url: &str,
) -> Option<&'a GithubReleaseAsset> {
    let file_name = download_url.rsplit('/').next()?;
    release.assets.iter().find(|asset| {
        asset.url == download_url
            || asset.browser_download_url == download_url
            || asset.name == file_name
    })
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

fn github_token_candidates() -> Vec<GithubToken> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    #[cfg(target_os = "windows")]
    if let Ok(token) = credential_entry().and_then(|entry| {
        entry
            .get_password()
            .map_err(|error| format!("credential read failed: {error}"))
    }) {
        push_token_candidate(
            &mut candidates,
            &mut seen,
            token,
            "windows-credential-manager",
        );
    }

    for name in ["BUKAN_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(token) = std::env::var(name) {
            push_token_candidate(&mut candidates, &mut seen, token, "environment");
        }
    }

    if let Some(token) = github_cli_token() {
        push_token_candidate(&mut candidates, &mut seen, token, "github-cli");
    }

    candidates
}

fn push_token_candidate(
    candidates: &mut Vec<GithubToken>,
    seen: &mut HashSet<String>,
    token: String,
    source: &'static str,
) {
    let token = token.trim().to_string();
    if !token.is_empty() && seen.insert(token.clone()) {
        candidates.push(GithubToken {
            value: token,
            source,
        });
    }
}

fn github_cli_token() -> Option<String> {
    for executable in github_cli_candidates() {
        let mut command = Command::new(executable);
        command.args(["auth", "token", "--hostname", "github.com"]);
        hide_command_window(&mut command);
        let output = match command.output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }
        let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }
    None
}

fn github_cli_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("gh")];
    #[cfg(target_os = "windows")]
    {
        for variable in ["ProgramFiles", "ProgramW6432"] {
            if let Some(root) = std::env::var_os(variable) {
                candidates.push(PathBuf::from(root).join("GitHub CLI").join("gh.exe"));
            }
        }
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(root)
                    .join("Programs")
                    .join("GitHub CLI")
                    .join("gh.exe"),
            );
        }
    }
    candidates
}

fn github_cli_is_installed() -> bool {
    github_cli_candidates().into_iter().any(|executable| {
        let mut command = Command::new(executable);
        command.arg("--version");
        hide_command_window(&mut command);
        command
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

fn missing_github_auth_message() -> String {
    if github_cli_is_installed() {
        "GitHub CLIの認証が無効または期限切れです。ターミナルで `gh auth login -h github.com` を実行するか、GitHub tokenをAppへ保存してください"
            .to_string()
    } else {
        "private GitHub Releaseへの認証がありません。GitHub tokenをAppへ保存するか、gh auth loginを実行してください"
            .to_string()
    }
}

fn auth_source_label(source: &str) -> &str {
    match source {
        "windows-credential-manager" => "Windows Credential Manager",
        "environment" => "環境変数",
        "github-cli" => "GitHub CLI",
        _ => source,
    }
}

fn hide_command_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_candidates_are_deduplicated_without_exposing_values() {
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        push_token_candidate(
            &mut candidates,
            &mut seen,
            " same ".to_string(),
            "environment",
        );
        push_token_candidate(&mut candidates, &mut seen, "same".to_string(), "github-cli");
        push_token_candidate(&mut candidates, &mut seen, " ".to_string(), "environment");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, "environment");
    }

    #[test]
    fn matches_private_release_installer_by_github_asset_api_url() {
        let release = GithubRelease {
            assets: vec![GithubReleaseAsset {
                name: "Bukan_0.2.1_x64-setup.exe".to_string(),
                url: "https://api.github.com/repos/Nkzono99/bukan/releases/assets/488100746"
                    .to_string(),
                browser_download_url:
                    "https://github.com/Nkzono99/bukan/releases/download/v0.2.1/Bukan_0.2.1_x64-setup.exe"
                        .to_string(),
            }],
        };

        let matched = release_asset_for_download(
            &release,
            "https://api.github.com/repos/Nkzono99/bukan/releases/assets/488100746",
        )
        .expect("API asset URL should match");
        assert_eq!(matched.name, "Bukan_0.2.1_x64-setup.exe");

        let legacy_matched = release_asset_for_download(
            &release,
            "https://github.com/Nkzono99/bukan/releases/download/v0.2.1/Bukan_0.2.1_x64-setup.exe",
        )
        .expect("legacy clients should match the browser download URL");
        assert_eq!(legacy_matched.name, "Bukan_0.2.1_x64-setup.exe");
    }
}
