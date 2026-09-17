//! Read-only PDF snapshots and page delivery, shared by the CLI and literature MCP.
//! Retrieval never marks a paper as reviewed.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;

const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

pub struct Document {
    snapshot: NamedTempFile,
    pub sha256: String,
    pub page_count: u32,
    pub warnings: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageText {
    pub page: u32,
    pub text: String,
    pub extraction_status: &'static str,
    pub warnings: String,
}

impl Document {
    pub fn open(root: &Path, path: &Path, expected_sha256: Option<&str>) -> Result<Self, String> {
        let root = fs::canonicalize(root).map_err(|e| format!("Library is unavailable: {e}"))?;
        let path = fs::canonicalize(path).map_err(|e| format!("PDF is unavailable: {e}"))?;
        if !path.starts_with(&root) || !crate::is_pdf(&path) || !path.is_file() {
            return Err("Only indexed PDFs within the Paperpile library may be read".into());
        }
        let source = File::open(&path).map_err(|e| format!("Could not open PDF read-only: {e}"))?;
        if source.metadata().map_err(|e| e.to_string())?.len() > MAX_PDF_BYTES {
            return Err("PDF exceeds the 256 MiB reading limit".into());
        }
        // A private temporary copy keeps text, page count and digest on one exact version,
        // and avoids passing Unicode or Drive-specific source paths to Poppler.
        let mut snapshot = tempfile::Builder::new()
            .suffix(".pdf")
            .tempfile()
            .map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut source.take(MAX_PDF_BYTES + 1), &mut snapshot)
            .map_err(|e| format!("Could not retrieve PDF from the mounted Drive: {e}"))?;
        if copied > MAX_PDF_BYTES {
            return Err("PDF exceeds the 256 MiB reading limit".into());
        }
        snapshot.flush().map_err(|e| e.to_string())?;
        let mut input = snapshot.reopen().map_err(|e| e.to_string())?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let size = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
        }
        let sha256 = format!("{:x}", digest.finalize());
        if expected_sha256.is_some_and(|expected| expected != sha256) {
            return Err("PDF version changed. Get its document metadata again before continuing; do not mix page versions".into());
        }
        let (info, warnings) = run_poppler(poppler_command("pdfinfo")?.arg(snapshot.path()))?;
        let page_count = parse_page_count(&String::from_utf8_lossy(&info))?;
        Ok(Self {
            snapshot,
            sha256,
            page_count,
            warnings,
        })
    }

    pub fn read_pages(&self, start: u32, count: u32) -> Result<Vec<PageText>, String> {
        let end = page_range(self.page_count, start, count)?;
        (start..=end).map(|page| {
            let (output, warnings) = run_poppler(poppler_command("pdftotext")?
                .args(["-f", &page.to_string(), "-l", &page.to_string(), "-layout", "-enc", "UTF-8", "-nopgbrk"])
                .arg(self.snapshot.path()).arg("-"))?;
            let text = String::from_utf8(output).map_err(|e| format!("Page {page} is not valid UTF-8: {e}"))?;
            if text.len() > 200_000 {
                return Err(format!("Page {page} exceeds the text limit; inspect its page image. No truncated text was returned"));
            }
            Ok(PageText { page, extraction_status: if text.trim().is_empty() {
                "no_text_inspect_image"
            } else { "text_extracted_not_reviewed" }, text, warnings })
        }).collect()
    }

    pub fn page_image(&self, page: u32) -> Result<(Vec<u8>, String), String> {
        page_range(self.page_count, page, 1)?;
        let (png, warnings) = run_poppler(
            poppler_command("pdftoppm")?
                .args([
                    "-f",
                    &page.to_string(),
                    "-l",
                    &page.to_string(),
                    "-singlefile",
                    "-scale-to",
                    "2000",
                    "-png",
                ])
                .arg(self.snapshot.path()),
        )?;
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("Poppler did not return a PNG page image".into());
        }
        Ok((png, warnings))
    }
}

fn poppler_command(name: &str) -> Result<Command, String> {
    crate::runtime::executable_on_path(name)
        .map(Command::new)
        .ok_or_else(|| format!("Poppler {name} was not found in an absolute PATH directory. Install Poppler and add its binary directory to PATH."))
}

fn parse_page_count(info: &str) -> Result<u32, String> {
    info.lines()
        .find_map(|line| line.strip_prefix("Pages:")?.trim().parse::<u32>().ok())
        .filter(|count| *count > 0)
        .ok_or_else(|| "Could not determine the full PDF page count".into())
}

fn page_range(total: u32, start: u32, count: u32) -> Result<u32, String> {
    if start == 0 || start > total || !(1..=10).contains(&count) {
        return Err(format!(
            "Use PDF file pages 1..={total} and a pageCount of 1..=10"
        ));
    }
    Ok(start.saturating_add(count - 1).min(total))
}

fn run_poppler(command: &mut Command) -> Result<(Vec<u8>, String), String> {
    let stdout = NamedTempFile::new().map_err(|e| e.to_string())?;
    let stderr = NamedTempFile::new().map_err(|e| e.to_string())?;
    command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(stdout.reopen().map_err(|e| e.to_string())?)
        .stderr(stderr.reopen().map_err(|e| e.to_string())?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|e| format!(
        "Could not start Poppler {:?}. Install pdfinfo, pdftotext and pdftoppm on PATH, then restart Bukan: {e}", command.get_program()))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(20))
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(e) => format!("Could not wait for Poppler: {e}"),
                    _ => {
                        "PDF processing timed out after 30 seconds; this page remains unread".into()
                    }
                });
            }
        }
    };
    let warnings = String::from_utf8_lossy(&bounded_read(stderr.path())?)
        .trim()
        .to_string();
    if !status.success() {
        return Err(format!("PDF processing failed ({status}): {warnings}"));
    }
    Ok((bounded_read(stdout.path())?, warnings))
}

fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err("PDF output exceeded 8 MiB; no partial output was returned".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_pdf_page_boundaries() {
        assert_eq!(parse_page_count("Title: example\nPages:  12\n"), Ok(12));
        assert!(parse_page_count("Pages: 0").is_err());
        assert!(parse_page_count("broken").is_err());
        assert_eq!(page_range(12, 11, 5), Ok(12));
        assert!(page_range(12, 0, 1).is_err());
        assert!(page_range(12, 13, 1).is_err());
        assert!(page_range(12, 1, 11).is_err());
    }

    #[test]
    fn rejects_paths_outside_library_before_running_poppler() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::Builder::new().suffix(".pdf").tempfile().unwrap();
        assert!(Document::open(root.path(), outside.path(), None)
            .err()
            .unwrap()
            .contains("within"));
        let not_pdf = root.path().join("secret.txt");
        fs::write(&not_pdf, b"secret").unwrap();
        assert!(Document::open(root.path(), &not_pdf, None).is_err());
    }

    #[test]
    fn rejects_a_changed_version_before_extracting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("paper.pdf");
        fs::write(&path, b"changed source").unwrap();
        assert!(Document::open(root.path(), &path, Some(&"0".repeat(64)))
            .err()
            .unwrap()
            .contains("version changed"));
        assert_eq!(fs::read(&path).unwrap(), b"changed source");
    }

    #[test]
    #[ignore = "requires Poppler (pdfinfo, pdftotext, pdftoppm) on PATH"]
    fn reads_every_fixture_page_and_renders_without_modifying_the_pdf() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("論文.pdf");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>".to_string(),
            page_object(4),
            text_stream("First page"),
            page_object(6),
            text_stream(""),
            page_object(8),
            text_stream("Final appendix"),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut pdf = "%PDF-1.4\n".to_string();
        let mut offsets = vec![0];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 10\n0000000000 65535 f \n");
        for offset in &offsets[1..] {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 10 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        fs::write(&path, &pdf).unwrap();
        let document = Document::open(root.path(), &path, None).unwrap();
        assert_eq!(document.page_count, 3);
        let pages = document.read_pages(1, 10).unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages[0].text.contains("First page"));
        assert_eq!(pages[1].extraction_status, "no_text_inspect_image");
        assert!(pages[2].text.contains("Final appendix"));
        assert!(document.page_image(3).unwrap().0.starts_with(b"\x89PNG"));
        assert!(document.page_image(4).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), pdf);
        assert!(Document::open(root.path(), &path, Some(&document.sha256)).is_ok());
    }

    fn page_object(content: u32) -> String {
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 9 0 R >> >> /Contents {content} 0 R >>")
    }

    fn text_stream(text: &str) -> String {
        let body = format!("BT /F1 12 Tf 50 700 Td ({text}) Tj ET\n");
        format!("<< /Length {} >>\nstream\n{body}endstream", body.len())
    }
}
