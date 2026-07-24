use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::workspace;

pub const REVIEW_FORMAT_VERSION: u32 = 1;
const MAX_ARTICLE_BYTES: usize = 4 * 1024 * 1024;
const MAX_THEME_CHARS: usize = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCitation {
    pub paper_id: String,
    pub title: String,
    #[serde(default)]
    pub authors: String,
    pub year: Option<u16>,
    #[serde(default)]
    pub locator: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFigure {
    pub file: String,
    pub caption: String,
    pub source_paper_id: String,
    pub page: Option<u32>,
    pub added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewMetadata {
    format_version: u32,
    id: String,
    theme: String,
    title: String,
    revision: u32,
    created_at: u64,
    updated_at: u64,
    #[serde(default)]
    citations: Vec<ReviewCitation>,
    #[serde(default)]
    figures: Vec<ReviewFigure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSummary {
    pub id: String,
    pub theme: String,
    pub title: String,
    pub revision: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub citation_count: usize,
    pub figure_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDocument {
    pub id: String,
    pub theme: String,
    pub title: String,
    pub revision: u32,
    pub created_at: u64,
    pub updated_at: u64,
    pub citations: Vec<ReviewCitation>,
    pub figures: Vec<ReviewFigure>,
    pub article: String,
    pub directory: String,
}

pub fn create_review(
    workspace_root: &Path,
    theme: &str,
    title: Option<&str>,
) -> Result<ReviewDocument, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let theme = validate_theme(theme)?;
    let title = title
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&theme)
        .to_string();
    let reviews_root = reviews_root(&workspace_root);
    fs::create_dir_all(&reviews_root)
        .map_err(|error| format!("レビュー保存先を作成できませんでした: {error}"))?;
    let id = unique_review_id(&reviews_root, &theme);
    let directory = reviews_root.join(&id);
    fs::create_dir_all(directory.join("figures"))
        .and_then(|_| fs::create_dir_all(directory.join("history")))
        .map_err(|error| format!("レビュープロジェクトを作成できませんでした: {error}"))?;
    let now = unix_timestamp();
    let metadata = ReviewMetadata {
        format_version: REVIEW_FORMAT_VERSION,
        id,
        theme: theme.clone(),
        title: title.clone(),
        revision: 1,
        created_at: now,
        updated_at: now,
        citations: Vec::new(),
        figures: Vec::new(),
    };
    let article = initial_article(&title, &theme);
    write_metadata(&directory, &metadata)?;
    write_article(&directory, &article)?;
    load_review(&workspace_root, &metadata.id)
}

pub fn list_reviews(workspace_root: &Path) -> Result<Vec<ReviewSummary>, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let root = reviews_root(&workspace_root);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut summaries = Vec::new();
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("レビュー一覧を読み取れませんでした: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("レビュー項目を読み取れませんでした: {error}"))?;
        if !entry.path().is_dir() {
            continue;
        }
        let metadata = match read_metadata(&entry.path()) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        summaries.push(summary(&metadata));
    }
    summaries.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(summaries)
}

pub fn load_review(workspace_root: &Path, review_id: &str) -> Result<ReviewDocument, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let directory = review_directory(&workspace_root, review_id)?;
    let metadata = read_metadata(&directory)?;
    let article = fs::read_to_string(directory.join("article.md"))
        .map_err(|error| format!("レビュー本文を読み取れませんでした: {error}"))?;
    Ok(ReviewDocument {
        id: metadata.id,
        theme: metadata.theme,
        title: metadata.title,
        revision: metadata.revision,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        citations: metadata.citations,
        figures: metadata.figures,
        article,
        directory: directory.to_string_lossy().into_owned(),
    })
}

pub fn update_review(
    workspace_root: &Path,
    review_id: &str,
    article: &str,
    citations: Option<Vec<ReviewCitation>>,
) -> Result<ReviewDocument, String> {
    if article.len() > MAX_ARTICLE_BYTES {
        return Err("レビュー本文が大きすぎます".to_string());
    }
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let directory = review_directory(&workspace_root, review_id)?;
    let mut metadata = read_metadata(&directory)?;
    snapshot_current_article(&directory, metadata.revision)?;
    metadata.revision = metadata.revision.saturating_add(1);
    metadata.updated_at = unix_timestamp();
    if let Some(citations) = citations {
        metadata.citations = citations;
    }
    write_article(&directory, article)?;
    write_metadata(&directory, &metadata)?;
    load_review(&workspace_root, review_id)
}

pub fn attach_figure(
    workspace_root: &Path,
    review_id: &str,
    source_path: &Path,
    caption: &str,
    source_paper_id: &str,
    page: Option<u32>,
) -> Result<ReviewDocument, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let source_path = fs::canonicalize(source_path)
        .map_err(|error| format!("画像を開けませんでした: {error}"))?;
    if !source_path.starts_with(&workspace_root) {
        return Err("添付画像はBukan作業領域内で抽出したファイルを指定してください".to_string());
    }
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "画像の拡張子を確認できません".to_string())?;
    if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "svg") {
        return Err("添付できる画像はPNG、JPEG、WebP、SVGです".to_string());
    }
    let directory = review_directory(&workspace_root, review_id)?;
    let mut metadata = read_metadata(&directory)?;
    let base_name = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(slugify)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "figure".to_string());
    let file = unique_figure_name(&directory.join("figures"), &base_name, &extension);
    fs::copy(&source_path, directory.join("figures").join(&file))
        .map_err(|error| format!("レビューへ画像を複製できませんでした: {error}"))?;
    snapshot_current_article(&directory, metadata.revision)?;
    metadata.revision = metadata.revision.saturating_add(1);
    metadata.updated_at = unix_timestamp();
    metadata.figures.push(ReviewFigure {
        file,
        caption: caption.trim().to_string(),
        source_paper_id: source_paper_id.trim().to_string(),
        page,
        added_at: metadata.updated_at,
    });
    write_metadata(&directory, &metadata)?;
    load_review(&workspace_root, review_id)
}

pub fn set_current_review(workspace_root: &Path, review_id: &str) -> Result<PathBuf, String> {
    let (workspace_root, _) = workspace::load_workspace(workspace_root)?;
    let review = load_review(&workspace_root, review_id)?;
    let context_directory = workspace_root.join(".bukan");
    fs::create_dir_all(&context_directory)
        .map_err(|error| format!("レビューコンテキストを作成できませんでした: {error}"))?;
    let path = context_directory.join("current-review.md");
    let article_path = Path::new(&review.directory).join("article.md");
    let contents = format!(
        "# Current Bukan review\n\n\
         - Review ID: `{}`\n\
         - Theme: `{}`\n\
         - Revision: `{}`\n\
         - Article: `{}`\n\n\
         Use the Bukan MCP `get_review` and `update_review` tools to inspect and revise this living review. \
         Preserve source locators for important claims and never modify Paperpile PDFs.\n",
        review.id,
        review.theme.replace('`', "'"),
        review.revision,
        article_path.to_string_lossy().replace('`', "'"),
    );
    fs::write(&path, contents)
        .map_err(|error| format!("レビューコンテキストを保存できませんでした: {error}"))?;
    Ok(path)
}

fn validate_theme(theme: &str) -> Result<String, String> {
    let theme = theme.trim();
    if theme.is_empty() {
        return Err("レビューのテーマを入力してください".to_string());
    }
    if theme.chars().count() > MAX_THEME_CHARS {
        return Err(format!("テーマは{MAX_THEME_CHARS}文字以内にしてください"));
    }
    Ok(theme.to_string())
}

fn review_directory(workspace_root: &Path, review_id: &str) -> Result<PathBuf, String> {
    if review_id.is_empty()
        || review_id.len() > 160
        || !review_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err("レビューIDが不正です".to_string());
    }
    let directory = reviews_root(workspace_root).join(review_id);
    if !directory.join("review.json").is_file() {
        return Err(format!("レビューが見つかりません: {review_id}"));
    }
    Ok(directory)
}

fn reviews_root(workspace_root: &Path) -> PathBuf {
    workspace_root.join("reports").join("reviews")
}

fn read_metadata(directory: &Path) -> Result<ReviewMetadata, String> {
    let text = fs::read_to_string(directory.join("review.json"))
        .map_err(|error| format!("レビューメタデータを読み取れませんでした: {error}"))?;
    let metadata: ReviewMetadata = serde_json::from_str(&text)
        .map_err(|error| format!("レビューメタデータの形式が不正です: {error}"))?;
    if metadata.format_version > REVIEW_FORMAT_VERSION {
        return Err(format!(
            "このレビューは新しい形式です (version {})",
            metadata.format_version
        ));
    }
    Ok(metadata)
}

fn write_metadata(directory: &Path, metadata: &ReviewMetadata) -> Result<(), String> {
    let text = serde_json::to_string_pretty(metadata)
        .map_err(|error| format!("レビューメタデータを変換できませんでした: {error}"))?;
    fs::write(directory.join("review.json"), format!("{text}\n"))
        .map_err(|error| format!("レビューメタデータを保存できませんでした: {error}"))
}

fn write_article(directory: &Path, article: &str) -> Result<(), String> {
    fs::write(directory.join("article.md"), article)
        .map_err(|error| format!("レビュー本文を保存できませんでした: {error}"))
}

fn snapshot_current_article(directory: &Path, revision: u32) -> Result<(), String> {
    let current = fs::read(directory.join("article.md"))
        .map_err(|error| format!("現在のレビュー本文を読み取れませんでした: {error}"))?;
    let history = directory.join("history");
    fs::create_dir_all(&history)
        .map_err(|error| format!("レビュー履歴を作成できませんでした: {error}"))?;
    fs::write(history.join(format!("revision-{revision:04}.md")), current)
        .map_err(|error| format!("レビュー履歴を保存できませんでした: {error}"))
}

fn summary(metadata: &ReviewMetadata) -> ReviewSummary {
    ReviewSummary {
        id: metadata.id.clone(),
        theme: metadata.theme.clone(),
        title: metadata.title.clone(),
        revision: metadata.revision,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        citation_count: metadata.citations.len(),
        figure_count: metadata.figures.len(),
    }
}

fn initial_article(title: &str, theme: &str) -> String {
    format!(
        "# {title}\n\n\
         > Theme: {theme}\n\n\
         ## Scope\n\n\
         このレビューの問い、対象範囲、除外条件を記述します。\n\n\
         ## Evidence synthesis\n\n\
         Codex terminalから文献を検索し、重要な主張には `[@paper-id, p. 12]` の形式で出典位置を付けて更新します。\n\n\
         ## Open questions\n\n\
         - 未解決の論点\n\n\
         ## References\n\n\
         引用情報はBukanの構造化メタデータとして蓄積されます。\n"
    )
}

fn unique_review_id(reviews_root: &Path, theme: &str) -> String {
    let date_prefix = unix_timestamp();
    let slug = slugify(theme);
    let base = format!(
        "{date_prefix}-{}",
        if slug.is_empty() { "review" } else { &slug }
    );
    unique_name(reviews_root, &base, "")
}

fn unique_figure_name(directory: &Path, base: &str, extension: &str) -> String {
    unique_name(directory, base, &format!(".{extension}"))
}

fn unique_name(directory: &Path, base: &str, suffix: &str) -> String {
    let first = format!("{base}{suffix}");
    if !directory.join(&first).exists() {
        return first;
    }
    for index in 2..10_000 {
        let candidate = format!("{base}-{index}{suffix}");
        if !directory.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}{suffix}", unix_timestamp())
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !slug.is_empty() {
            slug.push('-');
            previous_separator = true;
        }
        if slug.len() >= 64 {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workspace() -> tempfile::TempDir {
        let temporary = tempfile::tempdir().expect("tempdir");
        let paperpile = temporary.path().join("Paperpile");
        fs::create_dir_all(paperpile.join("All Papers")).expect("paperpile");
        workspace::init_workspace(
            &temporary.path().join("workspace"),
            Some("Review tests"),
            Some(&paperpile.to_string_lossy()),
        )
        .expect("workspace");
        temporary
    }

    #[test]
    fn creates_updates_and_lists_a_review_with_history() {
        let temporary = test_workspace();
        let root = temporary.path().join("workspace");
        let created = create_review(&root, "Lunar dust transport", None).expect("create");
        assert!(created.article.contains("# Lunar dust transport"));

        let updated =
            update_review(&root, &created.id, "# Revised\n", Some(Vec::new())).expect("update");
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.article, "# Revised\n");
        assert!(Path::new(&updated.directory)
            .join("history")
            .join("revision-0001.md")
            .is_file());

        let summaries = list_reviews(&root).expect("list");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].revision, 2);
    }

    #[test]
    fn rejects_review_path_traversal_and_images_outside_workspace() {
        let temporary = test_workspace();
        let root = temporary.path().join("workspace");
        let review = create_review(&root, "Safe review", None).expect("create");
        assert!(load_review(&root, "../escape").is_err());

        let outside = temporary.path().join("outside.png");
        fs::write(&outside, b"png").expect("outside image");
        assert!(attach_figure(&root, &review.id, &outside, "caption", "paper-1", Some(1)).is_err());
    }

    #[test]
    fn copies_an_extracted_figure_and_records_its_provenance() {
        let temporary = test_workspace();
        let root = temporary.path().join("workspace");
        let review = create_review(&root, "Illustrated review", None).expect("create");
        let extracted = root.join("cache").join("figures").join("source.png");
        fs::create_dir_all(extracted.parent().expect("figure parent")).expect("figure directory");
        fs::write(&extracted, b"png").expect("extracted image");

        let updated = attach_figure(
            &root,
            &review.id,
            &extracted,
            "Experimental arrangement",
            "p2-paper",
            Some(7),
        )
        .expect("attach figure");

        assert_eq!(updated.revision, 2);
        assert_eq!(updated.figures.len(), 1);
        assert_eq!(updated.figures[0].source_paper_id, "p2-paper");
        assert_eq!(updated.figures[0].page, Some(7));
        assert!(Path::new(&updated.directory)
            .join("figures")
            .join(&updated.figures[0].file)
            .is_file());
    }
}
