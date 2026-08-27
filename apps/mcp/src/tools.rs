//! The document operations, as plain functions over paths.
//!
//! Deliberately separate from the MCP plumbing in `server.rs`: everything
//! here is testable without a protocol, a transport, or an agent, and none
//! of it knows what MCP is.
//!
//! **Paths in, paths out — never bytes.** An MCP tool result travels back
//! through the model, so returning a PDF as base64 would put the whole
//! document into the agent's context and, for a hosted model, onto someone
//! else's servers. That is the exact thing this product exists not to do.
//! The server reads and writes files locally and hands back a summary; the
//! bytes never leave the machine.

use std::path::Path;

use openpdfedit_batch::PiiPattern;
use openpdfedit_crypt::Permissions;
use openpdfedit_doc::Document;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("couldn't read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("couldn't write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Pdf(String),
    #[error("{0}")]
    Input(String),
}

fn read(path: &Path) -> Result<Vec<u8>, ToolError> {
    std::fs::read(path).map_err(|source| ToolError::Read {
        path: path.display().to_string(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    std::fs::write(path, bytes).map_err(|source| ToolError::Write {
        path: path.display().to_string(),
        source,
    })
}

// --- info ---------------------------------------------------------------

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PdfInfo {
    pub path: String,
    pub pages: u32,
    pub encrypted: bool,
    pub signed: bool,
    pub bytes: u64,
}

/// What an agent needs before deciding what to do.
///
/// Worth its own tool rather than folding into the others: an agent that
/// can ask "how many pages, is it locked" first will stop guessing page
/// indices and stop trying to encrypt something already encrypted.
pub fn info(path: &Path) -> Result<PdfInfo, ToolError> {
    let bytes = read(path)?;
    let encrypted = openpdfedit_crypt::is_encrypted(&bytes);
    // A locked document cannot be parsed for a page count without its
    // password, and saying "0 pages" would be a lie. Report what is known.
    let (pages, signed) = if encrypted {
        (0, false)
    } else {
        let doc = Document::from_bytes(&bytes).map_err(|e| ToolError::Pdf(e.to_string()))?;
        let pages = doc
            .page_count()
            .map_err(|e| ToolError::Pdf(e.to_string()))?;
        (pages, doc.has_signature())
    };
    Ok(PdfInfo {
        path: path.display().to_string(),
        pages,
        encrypted,
        signed,
        bytes: bytes.len() as u64,
    })
}

// --- encrypt / decrypt --------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PermissionChoices {
    pub allow_print: Option<bool>,
    pub allow_modify: Option<bool>,
    pub allow_copy: Option<bool>,
    pub allow_annotate: Option<bool>,
}

impl PermissionChoices {
    /// Everything is allowed unless explicitly turned off — "just put a
    /// password on it" is what almost every request means, and a tool that
    /// silently forbade printing would be surprising.
    fn to_permissions(&self) -> Permissions {
        let base = Permissions::default();
        Permissions {
            print: self.allow_print.unwrap_or(base.print),
            modify: self.allow_modify.unwrap_or(base.modify),
            copy: self.allow_copy.unwrap_or(base.copy),
            annotate: self.allow_annotate.unwrap_or(base.annotate),
            ..base
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct WroteFile {
    pub output_path: String,
    pub bytes: u64,
}

pub fn encrypt(
    input: &Path,
    output: &Path,
    user_password: &str,
    owner_password: Option<&str>,
    permissions: &PermissionChoices,
) -> Result<WroteFile, ToolError> {
    if user_password.is_empty() {
        return Err(ToolError::Input(
            "a user password is required; an empty one would leave the document unprotected".into(),
        ));
    }
    let bytes = read(input)?;
    // Passing the user password as the owner password too is the ordinary
    // case, and is what "put a password on it" means.
    let owner = owner_password.unwrap_or(user_password);
    let encrypted = openpdfedit_crypt::encrypt_document(
        &bytes,
        user_password,
        owner,
        permissions.to_permissions(),
    )
    .map_err(|e| ToolError::Pdf(e.to_string()))?;
    write(output, &encrypted)?;
    Ok(WroteFile {
        output_path: output.display().to_string(),
        bytes: encrypted.len() as u64,
    })
}

pub fn decrypt(input: &Path, output: &Path, password: &str) -> Result<WroteFile, ToolError> {
    let bytes = read(input)?;
    let plain = openpdfedit_crypt::decrypt_document(&bytes, password)
        .map_err(|e| ToolError::Pdf(e.to_string()))?;
    write(output, &plain)?;
    Ok(WroteFile {
        output_path: output.display().to_string(),
        bytes: plain.len() as u64,
    })
}

// --- redact -------------------------------------------------------------

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RedactionSummary {
    pub output_path: String,
    pub pages_processed: u32,
    pub matches_redacted: usize,
    pub patterns: Vec<String>,
}

/// Parse the pattern names an agent supplies.
///
/// Unknown names are refused rather than ignored. A model that asks for
/// `"passport"` and is quietly given nothing would report a document as
/// redacted when it is not, which is the worst possible failure for this
/// particular tool.
pub fn parse_patterns(names: &[String]) -> Result<Vec<PiiPattern>, ToolError> {
    if names.is_empty() {
        return Ok(vec![
            PiiPattern::Email,
            PiiPattern::UsSsn,
            PiiPattern::UsPhone,
            PiiPattern::CardNumber16,
        ]);
    }
    names
        .iter()
        .map(|name| match name.to_ascii_lowercase().as_str() {
            "email" => Ok(PiiPattern::Email),
            "ssn" => Ok(PiiPattern::UsSsn),
            "phone" => Ok(PiiPattern::UsPhone),
            "card" => Ok(PiiPattern::CardNumber16),
            other => Err(ToolError::Input(format!(
                "unknown PII pattern {other:?} — expected email, ssn, phone or card"
            ))),
        })
        .collect()
}

pub fn redact_pii(
    input: &Path,
    output: &Path,
    patterns: &[PiiPattern],
    pattern_names: Vec<String>,
) -> Result<RedactionSummary, ToolError> {
    let mut doc = Document::open(input).map_err(|e| ToolError::Pdf(e.to_string()))?;
    let report = openpdfedit_batch::redact_pii(&mut doc, patterns)
        .map_err(|e| ToolError::Pdf(e.to_string()))?;
    // A full rewrite, never an incremental save: an incremental save keeps
    // the original bytes, so the text this call just removed would still be
    // sitting one revision back in the output and `strings` would find it.
    // See Document::save_full.
    let saved = doc.save_full().map_err(|e| ToolError::Pdf(e.to_string()))?;
    write(output, &saved)?;
    Ok(RedactionSummary {
        output_path: output.display().to_string(),
        pages_processed: report.pages_processed,
        matches_redacted: report.matches_redacted,
        patterns: pattern_names,
    })
}

// --- pages --------------------------------------------------------------

pub fn merge(inputs: &[std::path::PathBuf], output: &Path) -> Result<WroteFile, ToolError> {
    if inputs.len() < 2 {
        return Err(ToolError::Input(
            "merging needs at least two documents".into(),
        ));
    }
    let sources = inputs
        .iter()
        .map(|p| read(p))
        .collect::<Result<Vec<_>, _>>()?;
    let refs: Vec<&[u8]> = sources.iter().map(Vec::as_slice).collect();
    let merged = openpdfedit_pages::merge(&refs).map_err(|e| ToolError::Pdf(e.to_string()))?;
    write(output, &merged)?;
    Ok(WroteFile {
        output_path: output.display().to_string(),
        bytes: merged.len() as u64,
    })
}

pub fn extract_pages(input: &Path, output: &Path, pages: &[u32]) -> Result<WroteFile, ToolError> {
    if pages.is_empty() {
        return Err(ToolError::Input("no pages given to extract".into()));
    }
    let bytes = read(input)?;
    let extracted = openpdfedit_pages::extract_pages(&bytes, pages)
        .map_err(|e| ToolError::Pdf(e.to_string()))?;
    write(output, &extracted)?;
    Ok(WroteFile {
        output_path: output.display().to_string(),
        bytes: extracted.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF carrying `text`, built the same way the CLI's own
    /// tests build theirs.
    fn pdf_with(text: &str) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![72.into(), 720.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Count" => 1, "Kids" => vec![page_id.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn write_pdf(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, pdf_with(text)).unwrap();
        path
    }

    #[test]
    fn info_reports_pages_and_that_nothing_is_locked() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "hello.pdf", "Hello");
        let got = info(&pdf).unwrap();
        assert_eq!(got.pages, 1);
        assert!(!got.encrypted);
        assert!(got.bytes > 0);
    }

    #[test]
    fn encrypt_then_info_says_it_is_locked() {
        // The end-to-end demo, as a test: an agent encrypts a PDF and can
        // then see for itself that it worked.
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "plain.pdf", "Confidential");
        let out = dir.path().join("locked.pdf");

        let wrote = encrypt(&pdf, &out, "hunter2", None, &PermissionChoices::default()).unwrap();
        assert!(wrote.bytes > 0);
        assert!(info(&out).unwrap().encrypted, "the output must be locked");
        assert!(
            !info(&pdf).unwrap().encrypted,
            "the input must be untouched"
        );
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "plain.pdf", "Round trip");
        let locked = dir.path().join("locked.pdf");
        let unlocked = dir.path().join("unlocked.pdf");

        encrypt(&pdf, &locked, "pw", None, &PermissionChoices::default()).unwrap();
        decrypt(&locked, &unlocked, "pw").unwrap();
        assert!(!info(&unlocked).unwrap().encrypted);
        assert_eq!(info(&unlocked).unwrap().pages, 1);
    }

    #[test]
    fn an_empty_password_is_refused_rather_than_producing_an_open_file() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "plain.pdf", "x");
        let out = dir.path().join("out.pdf");
        assert!(matches!(
            encrypt(&pdf, &out, "", None, &PermissionChoices::default()),
            Err(ToolError::Input(_))
        ));
        assert!(!out.exists(), "nothing should have been written");
    }

    #[test]
    fn the_wrong_password_does_not_decrypt() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "plain.pdf", "x");
        let locked = dir.path().join("locked.pdf");
        encrypt(&pdf, &locked, "right", None, &PermissionChoices::default()).unwrap();
        assert!(decrypt(&locked, &dir.path().join("out.pdf"), "wrong").is_err());
    }

    #[test]
    fn an_unknown_pii_pattern_is_refused_not_ignored() {
        // Silently ignoring it would report a document as redacted when it
        // is not — the worst available failure for this tool.
        assert!(matches!(
            parse_patterns(&["passport".to_string()]),
            Err(ToolError::Input(_))
        ));
        assert_eq!(parse_patterns(&[]).unwrap().len(), 4);
        assert_eq!(parse_patterns(&["email".to_string()]).unwrap().len(), 1);
    }

    #[test]
    fn redaction_removes_a_matching_address() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "pii.pdf", "contact ada@example.com now");
        let out = dir.path().join("clean.pdf");
        let summary =
            redact_pii(&pdf, &out, &[PiiPattern::Email], vec!["email".to_string()]).unwrap();
        assert_eq!(summary.matches_redacted, 1);
        assert!(!String::from_utf8_lossy(&std::fs::read(&out).unwrap()).contains("ada@example.com"));
    }

    #[test]
    fn merging_needs_two_documents() {
        let dir = tempfile::tempdir().unwrap();
        let one = write_pdf(dir.path(), "a.pdf", "A");
        assert!(matches!(
            merge(&[one], &dir.path().join("out.pdf")),
            Err(ToolError::Input(_))
        ));
    }

    #[test]
    fn merging_two_documents_gives_two_pages() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_pdf(dir.path(), "a.pdf", "A");
        let b = write_pdf(dir.path(), "b.pdf", "B");
        let out = dir.path().join("merged.pdf");
        merge(&[a, b], &out).unwrap();
        assert_eq!(info(&out).unwrap().pages, 2);
    }

    #[test]
    fn extracting_no_pages_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "a.pdf", "A");
        assert!(matches!(
            extract_pages(&pdf, &dir.path().join("out.pdf"), &[]),
            Err(ToolError::Input(_))
        ));
    }
}
