use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::LibraryIndex;

#[derive(Debug, Clone, Deserialize)]
pub struct TaxonomyConfig {
    pub version: u32,
    pub folders: FolderConfig,
    pub labels: LabelConfig,
    #[serde(default)]
    pub folder_rules: Vec<MatchRule>,
    #[serde(default)]
    pub label_rules: Vec<LabelRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FolderConfig {
    pub root: String,
    pub axes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelConfig {
    pub prefix: String,
    pub namespaces: Vec<String>,
    pub status: Vec<String>,
    #[serde(rename = "type")]
    pub reference_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatchRule {
    pub path: String,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelRule {
    pub label: String,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationPlan {
    pub taxonomy_version: u32,
    pub folder_root: String,
    pub paper_count: usize,
    pub classified_count: usize,
    pub review_required_count: usize,
    pub assignments: Vec<OrganizationAssignment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationAssignment {
    pub paper_id: String,
    pub title: String,
    pub current_collections: Vec<String>,
    pub suggested_folders: Vec<String>,
    pub suggested_labels: Vec<String>,
    pub evidence: Vec<String>,
    pub review_required: bool,
}

pub fn load_taxonomy(workspace_root: &Path) -> Result<TaxonomyConfig, String> {
    let path = workspace_root.join("taxonomy.toml");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("taxonomy.toml を読み取れませんでした: {error}"))?;
    let taxonomy: TaxonomyConfig = toml::from_str(&text)
        .map_err(|error| format!("taxonomy.toml の形式が不正です: {error}"))?;
    validate_taxonomy(&taxonomy)?;
    Ok(taxonomy)
}

pub fn generate_plan(index: &LibraryIndex, taxonomy: &TaxonomyConfig) -> OrganizationPlan {
    let assignments: Vec<OrganizationAssignment> = index
        .papers
        .iter()
        .map(|paper| {
            let title_haystack = format!("{} {}", paper.title, paper.file_name).to_lowercase();
            let collection_haystack = paper.collections.join(" ").to_lowercase();
            let mut suggested_folders = Vec::new();
            let mut suggested_labels = Vec::new();
            let mut evidence = Vec::new();

            for rule in &taxonomy.folder_rules {
                if let Some((term, source)) =
                    match_rule(&rule.terms, &title_haystack, &collection_haystack)
                {
                    suggested_folders.push(format!("{}/{}", taxonomy.folders.root, rule.path));
                    evidence.push(format!("{source}: {term}"));
                }
            }
            for rule in &taxonomy.label_rules {
                if let Some((term, source)) =
                    match_rule(&rule.terms, &title_haystack, &collection_haystack)
                {
                    suggested_labels.push(format!("{}{}", taxonomy.labels.prefix, rule.label));
                    evidence.push(format!("{source}: {term}"));
                }
            }

            suggested_folders.sort();
            suggested_folders.dedup();
            suggested_labels.sort();
            suggested_labels.dedup();
            evidence.sort();
            evidence.dedup();
            let review_required = suggested_folders.is_empty();
            suggested_labels.push(format!(
                "{}status:{}",
                taxonomy.labels.prefix,
                if review_required {
                    "未整理"
                } else {
                    "要確認"
                }
            ));

            OrganizationAssignment {
                paper_id: paper.id.clone(),
                title: paper.title.clone(),
                current_collections: paper.collections.clone(),
                suggested_folders,
                suggested_labels,
                evidence,
                review_required,
            }
        })
        .collect();

    OrganizationPlan {
        taxonomy_version: taxonomy.version,
        folder_root: taxonomy.folders.root.clone(),
        paper_count: assignments.len(),
        classified_count: assignments
            .iter()
            .filter(|item| !item.suggested_folders.is_empty())
            .count(),
        review_required_count: assignments
            .iter()
            .filter(|item| item.review_required)
            .count(),
        assignments,
    }
}

fn match_rule(
    terms: &[String],
    title_haystack: &str,
    collection_haystack: &str,
) -> Option<(String, &'static str)> {
    for term in terms {
        let normalized = term.to_lowercase();
        if collection_haystack.contains(&normalized) {
            return Some((term.clone(), "existing folder"));
        }
        if title_haystack.contains(&normalized) {
            return Some((term.clone(), "title"));
        }
    }
    None
}

fn validate_taxonomy(taxonomy: &TaxonomyConfig) -> Result<(), String> {
    if taxonomy.version == 0 {
        return Err("taxonomy version must be at least 1".to_string());
    }
    if taxonomy.folders.root.trim().is_empty() || taxonomy.folders.root.contains('/') {
        return Err("folders.root はスラッシュを含まない名前にしてください".to_string());
    }
    for rule in &taxonomy.folder_rules {
        let axis = rule.path.split('/').next().unwrap_or_default();
        if !taxonomy.folders.axes.iter().any(|item| item == axis) {
            return Err(format!("未知のフォルダ軸です: {axis}"));
        }
        if rule.terms.is_empty() {
            return Err(format!("{} に terms がありません", rule.path));
        }
    }
    for rule in &taxonomy.label_rules {
        let namespace = rule.label.split(':').next().unwrap_or_default();
        if !taxonomy
            .labels
            .namespaces
            .iter()
            .any(|item| item == namespace)
        {
            return Err(format!("未知のラベル名前空間です: {namespace}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LibraryStats, PaperRecord};

    fn taxonomy() -> TaxonomyConfig {
        toml::from_str(include_str!("../../templates/workspace/taxonomy.toml"))
            .expect("default taxonomy")
    }

    #[test]
    fn validates_default_taxonomy() {
        validate_taxonomy(&taxonomy()).expect("valid taxonomy");
    }

    #[test]
    fn suggests_faceted_folders_and_labels_with_evidence() {
        let index = LibraryIndex {
            root: "Paperpile".to_string(),
            papers: vec![PaperRecord {
                id: "paper-1".to_string(),
                legacy_id: "legacy-paper-1".to_string(),
                identity_source: "test".to_string(),
                title: "Kaguya observation of lunar surface charging".to_string(),
                authors: None,
                year: Some(2025),
                collections: vec!["かぐや観測解析".to_string()],
                path: "paper.pdf".to_string(),
                relative_path: "paper.pdf".to_string(),
                file_name: "paper.pdf".to_string(),
                size_bytes: 1,
                modified_at: None,
                starred: false,
            }],
            collections: vec!["かぐや観測解析".to_string()],
            stats: LibraryStats {
                paper_count: 1,
                collection_count: 1,
                starred_count: 0,
                total_bytes: 1,
            },
            warnings: vec![],
        };
        let plan = generate_plan(&index, &taxonomy());
        let assignment = &plan.assignments[0];
        assert!(assignment
            .suggested_folders
            .contains(&"Bukan/ミッション/かぐや（SELENE）".to_string()));
        assert!(assignment
            .suggested_folders
            .contains(&"Bukan/トピック/月面帯電".to_string()));
        assert!(assignment
            .suggested_labels
            .contains(&"bukan:method:observation".to_string()));
        assert!(!assignment.review_required);
    }
}
