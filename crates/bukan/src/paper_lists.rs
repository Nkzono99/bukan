//! Workspace paper lists and their durable Markdown/JSON exports.

use crate::workspace;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_PAPERS: usize = 250;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedPaperList {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub papers: Vec<PresentedPaper>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedPaper {
    #[serde(default)]
    pub paper_id: String,
    pub title: String,
    #[serde(default)]
    pub authors: String,
    pub year: Option<u16>,
    #[serde(default)]
    pub doi: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PresentPaperListInput {
    title: String,
    #[serde(default)]
    description: String,
    papers: Vec<PresentedPaper>,
}

pub(crate) fn validate_list(input: PresentPaperListInput) -> Result<PresentedPaperList, String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err("list title must not be empty".to_string());
    }
    if input.papers.len() > MAX_PAPERS {
        return Err(format!("paper list is limited to {MAX_PAPERS} items"));
    }
    let mut papers = Vec::with_capacity(input.papers.len());
    for mut paper in input.papers {
        paper.title = paper.title.trim().to_string();
        if paper.title.is_empty() {
            return Err("every paper must have a title".to_string());
        }
        paper.doi = paper
            .doi
            .trim()
            .trim_start_matches("https://doi.org/")
            .trim_start_matches("http://doi.org/")
            .to_string();
        papers.push(paper);
    }
    Ok(PresentedPaperList {
        title,
        description: input.description.trim().to_string(),
        papers,
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as u64,
    })
}

pub fn read_presented_list(workspace_root: &Path) -> Result<Option<PresentedPaperList>, String> {
    let path = bridge_path(workspace_root)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("一時文献リストを読み取れませんでした: {error}"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("一時文献リストの形式が不正です: {error}"))
}

pub fn write_presented_list(
    workspace_root: &Path,
    list: &PresentedPaperList,
) -> Result<(), String> {
    let path = bridge_path(workspace_root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("一時文献リストの保存先を作成できませんでした: {error}"))?;
    }
    let bytes = serde_json::to_vec(list)
        .map_err(|error| format!("一時文献リストを変換できませんでした: {error}"))?;
    fs::write(&path, bytes)
        .map_err(|error| format!("一時文献リストを保存できませんでした: {error}"))
}

pub fn clear_presented_list(workspace_root: &Path) -> Result<(), String> {
    let path = bridge_path(workspace_root)?;
    if path.is_file() {
        fs::remove_file(path)
            .map_err(|error| format!("一時文献リストを消去できませんでした: {error}"))?;
    }
    Ok(())
}

pub fn persist_presented_list(workspace_root: &Path, destination: &str) -> Result<PathBuf, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let list = read_presented_list(&workspace_root)?
        .ok_or_else(|| "保存する一時文献リストがありません".to_string())?;
    let (directory, extension, contents) = match destination {
        "reports" => (
            workspace_root.join("reports").join("codex-lists"),
            "md",
            presented_list_markdown(&list).into_bytes(),
        ),
        "candidates" => (
            workspace_root.join("candidates").join("codex-lists"),
            "json",
            serde_json::to_vec_pretty(&list)
                .map_err(|error| format!("候補リストを変換できませんでした: {error}"))?,
        ),
        _ => return Err("保存先は reports または candidates を指定してください".to_string()),
    };
    let directory = crate::storage::validate_workspace_destination(&workspace_root, &directory)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("リストの保存先を作成できませんでした: {error}"))?;
    let slug = filename_slug(&list.title);
    let slug = if slug.is_empty() { "paper-list" } else { &slug };
    let stem = format!("{}-{slug}", list.updated_at);
    for suffix in 0..100 {
        let suffix = if suffix == 0 {
            String::new()
        } else {
            format!("-{suffix}")
        };
        let path = crate::storage::validate_workspace_destination(
            &workspace_root,
            &directory.join(format!("{stem}{suffix}.{extension}")),
        )?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&contents)
                    .map_err(|error| format!("リストを保存できませんでした: {error}"))?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("リストを保存できませんでした: {error}")),
        }
    }
    Err("重複しないリスト名を作成できませんでした".to_string())
}

fn presented_list_markdown(list: &PresentedPaperList) -> String {
    let mut output = format!(
        "# {}\n\n{}\n\n- Generated by: Codex via Bukan MCP\n- Papers: {}\n\n",
        list.title,
        list.description,
        list.papers.len()
    );
    for (index, paper) in list.papers.iter().enumerate() {
        output.push_str(&format!("## {}. {}\n\n", index + 1, paper.title));
        if !paper.paper_id.is_empty() {
            output.push_str(&format!("- Bukan ID: {}\n", paper.paper_id));
        }
        if !paper.authors.is_empty() {
            output.push_str(&format!("- Authors: {}\n", paper.authors));
        }
        if let Some(year) = paper.year {
            output.push_str(&format!("- Year: {year}\n"));
        }
        if !paper.doi.is_empty() {
            output.push_str(&format!("- DOI: {}\n", paper.doi));
        }
        if !paper.url.is_empty() {
            output.push_str(&format!("- URL: {}\n", paper.url));
        }
        if !paper.status.is_empty() {
            output.push_str(&format!("- Status: {}\n", paper.status));
        }
        if !paper.note.is_empty() {
            output.push_str(&format!("\n{}\n", paper.note));
        }
        output.push('\n');
    }
    output
}

fn filename_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_separator = false;
    for character in value.chars() {
        if character.is_alphanumeric() {
            slug.push(character);
            previous_separator = false;
        } else if !previous_separator && !slug.is_empty() {
            slug.push('-');
            previous_separator = true;
        }
        if slug.chars().count() >= 48 {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

pub(crate) fn bridge_path(workspace_root: &Path) -> Result<PathBuf, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    crate::storage::validate_workspace_destination(
        &workspace_root,
        &workspace_root.join(".bukan/paper-list.json"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_temporary_paper_list() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paperpile = temporary.path().join("Paperpile");
        fs::create_dir_all(paperpile.join("All Papers")).expect("Paperpile");
        let workspace_root = temporary.path().join("workspace");
        workspace::init_workspace(
            &workspace_root,
            Some("MCP test"),
            Some(&paperpile.to_string_lossy()),
        )
        .expect("workspace");
        let list = PresentedPaperList {
            title: "Thermal studies".to_string(),
            description: "Selected by Codex".to_string(),
            papers: vec![PresentedPaper {
                paper_id: "p2-local-paper".to_string(),
                title: "Lunar heat flow".to_string(),
                authors: "Sato".to_string(),
                year: Some(2025),
                doi: "10.1234/example".to_string(),
                url: String::new(),
                note: "Relevant method".to_string(),
                status: "candidate".to_string(),
            }],
            updated_at: 1,
        };

        write_presented_list(&workspace_root, &list).expect("write list");
        let restored = read_presented_list(&workspace_root)
            .expect("read list")
            .expect("list");
        assert_eq!(restored.title, "Thermal studies");
        assert_eq!(restored.papers[0].paper_id, "p2-local-paper");
        assert_eq!(restored.papers[0].doi, "10.1234/example");

        let report = persist_presented_list(&workspace_root, "reports").expect("save report");
        let canonical_workspace = fs::canonicalize(&workspace_root).expect("canonical workspace");
        assert!(report.starts_with(canonical_workspace.join("reports").join("codex-lists")));
        assert!(fs::read_to_string(&report)
            .expect("read report")
            .contains("# Thermal studies"));

        let candidates =
            persist_presented_list(&workspace_root, "candidates").expect("save candidates");
        assert!(candidates.starts_with(canonical_workspace.join("candidates").join("codex-lists")));
        assert!(fs::read_to_string(&candidates)
            .expect("read candidates")
            .contains("\"title\": \"Thermal studies\""));

        clear_presented_list(&workspace_root).expect("clear list");
        assert!(read_presented_list(&workspace_root)
            .expect("read cleared list")
            .is_none());
    }
}
