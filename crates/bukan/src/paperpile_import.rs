//! Prepare reference data for Paperpile's supported Paste UI. No remote writes.

use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportFormat {
    Identifiers,
    Bibtex,
    Ris,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareImport {
    pub format: ImportFormat,
    pub text: String,
    #[serde(default = "default_destination")]
    pub destination: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterImport {
    pub format: ImportFormat,
    pub text: String,
    #[serde(default = "default_destination")]
    pub destination: String,
    pub expected_count: Option<usize>,
    #[serde(default)]
    pub preview_only: bool,
    #[serde(default = "default_postprocess")]
    pub postprocess: bool,
}

fn default_postprocess() -> bool {
    true
}

/// Resolve the expected count before any browser or network activity. Requiring
/// it for opaque bibliography formats prevents unnoticed partial imports.
pub fn prepare_registration(input: RegisterImport) -> Result<Value, String> {
    if input.destination.trim() != "My Library" {
        return Err("Automated registration currently supports My Library only; no folder or shared-library fallback is performed.".into());
    }
    let mut result = prepare(PrepareImport {
        format: input.format,
        text: input.text,
        destination: input.destination,
    })?;
    let parsed_count = result["referenceCount"].as_u64().map(|n| n as usize);
    let expected = match (parsed_count, input.expected_count) {
        (Some(count), Some(expected)) if count != expected => {
            return Err("expectedCount must match the number of distinct DOI/URL inputs.".into())
        }
        (Some(count), _) | (None, Some(count)) => count,
        _ => {
            return Err(
                "BibTeX/RIS registration requires expectedCount (number of distinct references)."
                    .into(),
            )
        }
    };
    if !(1..=100).contains(&expected) {
        return Err("expectedCount must be between 1 and 100.".into());
    }
    result["referenceCount"] = json!(expected);
    result["previewOnly"] = json!(input.preview_only);
    result["postprocess"] = json!(input.postprocess);
    Ok(result)
}

fn default_destination() -> String {
    "My Library".into()
}

pub fn prepare(input: PrepareImport) -> Result<Value, String> {
    let text = input.text.trim();
    if text.is_empty() || input.text.len() > 64 * 1024 || text.contains('\0') {
        return Err("Reference text must be nonempty, at most 64 KiB, and contain no NUL.".into());
    }
    let destination = input.destination.trim();
    if destination.is_empty()
        || destination.chars().count() > 500
        || destination.chars().any(char::is_control)
    {
        return Err("Destination must be a nonempty, single-line library/folder/label name (max 500 characters).".into());
    }
    let (format, paste_text, count, removed) = match input.format {
        ImportFormat::Identifiers => {
            let doi_pattern = Regex::new(r"(?i)^10\.[0-9]{4,9}/\S+$").unwrap();
            let url_pattern = Regex::new(r"(?i)^https?://[^\s/?#]+(?:[/?#]\S*)?$").unwrap();
            let lines: Vec<_> = text
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if lines.len() > 100 {
                return Err("Use at most 100 DOI/URL lines per import batch.".into());
            }
            let mut seen = HashSet::new();
            let mut items = Vec::new();
            for (index, line) in lines.iter().enumerate() {
                if line.chars().any(char::is_control) {
                    return Err(format!("Line {} contains a control character.", index + 1));
                }
                let lowercase = line.to_ascii_lowercase();
                let doi = [
                    "https://doi.org/",
                    "http://doi.org/",
                    "https://dx.doi.org/",
                    "http://dx.doi.org/",
                    "doi:",
                ]
                .iter()
                .find_map(|prefix| lowercase.strip_prefix(prefix))
                .unwrap_or(&lowercase)
                .trim();
                // Do not turn encoded URL paths or query/fragment components
                // into DOI characters. Let Paperpile resolve those URLs as-is.
                let decorated_url = (lowercase.starts_with("https://")
                    || lowercase.starts_with("http://"))
                    && line.contains(['%', '?', '#']);
                let item = if doi_pattern.is_match(doi) && !decorated_url {
                    doi.to_string()
                } else if url_pattern.is_match(line) {
                    // URL paths and query values can be case-sensitive.
                    line.to_string()
                } else {
                    return Err(format!(
                        "Line {} must be a DOI or HTTP(S) URL. Use bibtex/ris for citation data.",
                        index + 1
                    ));
                };
                if seen.insert(item.clone()) {
                    items.push(item);
                }
            }
            let count = items.len();
            (
                "identifiers",
                items.join("\n"),
                Some(count),
                lines.len() - count,
            )
        }
        // Let Paperpile parse the source format and preview the actual records.
        ImportFormat::Bibtex => ("bibtex", text.to_string(), None, 0),
        ImportFormat::Ris => ("ris", text.to_string(), None, 0),
    };
    Ok(json!({
        "status": "prepared",
        "registrationPerformed": false,
        "paperpileUrl": "https://paperpile.com/app",
        "format": format,
        "pasteText": paste_text,
        "requestedDestination": destination,
        "referenceCount": count,
        "removedInputDuplicates": removed,
        "libraryDuplicatesChecked": false,
        "nextAction": "For a user-requested My Library import, call paperpile_import_references after the one-time bukan paperpile login setup. Supply expectedCount for BibTeX/RIS. This preparation tool itself performs no registration.",
        "warnings": [
            "Paperpile must resolve identifiers or parse BibTeX/RIS; preparation does not validate bibliographic identity or prove access to a PDF.",
            "A synced-PDF search cannot establish that a reference is absent from Paperpile. Check duplicates in the live library.",
            "Paperpile registration, PDF acquisition, Google Drive synchronization and Bukan indexing are separate outcomes."
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(format: &str, text: &str) -> PrepareImport {
        serde_json::from_value(json!({"format": format, "text": text})).unwrap()
    }

    #[test]
    fn normalizes_dois_but_preserves_url_case_and_order() {
        let result = prepare(input("identifiers", " DOI: 10.1002/2016GL069491\r\nhttps://doi.org/10.1002/2016gl069491\nhttps://arxiv.org/abs/ABC?key=X\nhttps://arxiv.org/abs/abc?key=x")).unwrap();
        assert_eq!(result["referenceCount"], 3);
        assert_eq!(result["removedInputDuplicates"], 1);
        assert_eq!(result["pasteText"], "10.1002/2016gl069491\nhttps://arxiv.org/abs/ABC?key=X\nhttps://arxiv.org/abs/abc?key=x");
        assert_eq!(result["registrationPerformed"], false);
        assert_eq!(result["libraryDuplicatesChecked"], false);
        assert_eq!(result["requestedDestination"], "My Library");
    }

    #[test]
    fn preserves_encoded_or_decorated_doi_resolver_urls() {
        let text = "https://doi.org/10.1002/2016GL069491?utm_source=ABC\nhttps://doi.org/10.1234/a#Section\nhttps://doi.org/10.1234/a%2FB";
        let result = prepare(input("identifiers", text)).unwrap();
        assert_eq!(result["pasteText"], text);
        assert_eq!(result["referenceCount"], 3);
    }

    #[test]
    fn preserves_bibliographic_payload_without_claiming_it_was_parsed() {
        for (format, text) in [
            (
                "bibtex",
                "@article{x, title={日本語 {Study}}, doi={10.1234/Example}}",
            ),
            ("ris", "TY  - JOUR\r\nTI  - 日本語\r\nER  -"),
        ] {
            let result = prepare(input(format, text)).unwrap();
            assert_eq!(result["pasteText"], text);
            assert!(result["referenceCount"].is_null());
        }
    }

    #[test]
    fn rejects_wrong_formats_and_oversized_batches() {
        for text in [
            "",
            "\n ",
            "file:///private.pdf",
            "javascript:alert(1)",
            "some article title",
            "10.1234/x\0",
            "https://example.org/a\tb",
        ] {
            assert!(prepare(input("identifiers", text)).is_err(), "{text:?}");
        }
        assert!(prepare(input("identifiers", &"10.1234/example\n".repeat(101))).is_err());
        assert!(prepare(input("ris", &"x".repeat(65537))).is_err());
        assert!(
            serde_json::from_value::<PrepareImport>(json!({"format":"pdf", "text":"x"})).is_err()
        );
    }

    #[test]
    fn destination_is_only_an_intent_not_a_path_to_write() {
        let mut request = input("identifiers", "10.1234/example");
        request.destination = "My Library / Dust research".into();
        let result = prepare(request).unwrap();
        assert_eq!(result["requestedDestination"], "My Library / Dust research");
        assert_eq!(result["status"], "prepared");
        for destination in ["", "a\nb", "a\0b"] {
            let mut request = input("identifiers", "10.1234/example");
            request.destination = destination.into();
            assert!(prepare(request).is_err());
        }
    }

    #[test]
    fn registration_requires_an_exact_count_and_supported_destination() {
        let parse = |value| serde_json::from_value::<RegisterImport>(value).unwrap();
        assert_eq!(
            prepare_registration(parse(
                json!({"format":"identifiers", "text":"10.1234/a\n10.1234/A"})
            ))
            .unwrap()["referenceCount"],
            1
        );
        for extra in [
            json!({"expectedCount": 2}),
            json!({"destination": "Shared"}),
        ] {
            let mut input = json!({"format":"identifiers", "text":"10.1234/a"});
            input
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(prepare_registration(parse(input)).is_err());
        }
        assert!(
            prepare_registration(parse(json!({"format":"bibtex", "text":"@article{x}"}))).is_err()
        );
        assert!(prepare_registration(parse(
            json!({"format":"ris", "text":"TY - JOUR", "expectedCount":0})
        ))
        .is_err());
        assert!(prepare_registration(parse(
            json!({"format":"bibtex", "text":"@article{x}", "expectedCount":1})
        ))
        .is_ok());
    }

    #[test]
    fn registration_defaults_to_followup_and_accepts_explicit_opt_out() {
        for (extra, expected) in [(json!({}), true), (json!({"postprocess": false}), false)] {
            let mut value = json!({"format": "identifiers", "text": "10.1234/a"});
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let result = prepare_registration(serde_json::from_value(value).unwrap()).unwrap();
            assert_eq!(result["postprocess"], expected);
        }
        assert!(serde_json::from_value::<RegisterImport>(json!({
            "format": "identifiers", "text": "10.1234/a", "postprocess": "true"
        }))
        .is_err());
    }
}
