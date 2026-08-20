//! Batch operations (PLAN.md M9), built by composing already-shipped
//! library crates rather than new engine code — nothing here needs
//! PDFium (no rendering happens), which is what makes it usable from a
//! plain CLI without the single-process-render-thread machinery the
//! desktop app needs.
//!
//! [`redact_pii`] is the centerpiece: finds text runs matching one or
//! more [`PiiPattern`]s via [`openpdfedit_textedit::list_text_runs`] (M7
//! infrastructure) and redacts each match via
//! [`openpdfedit_redact::redact_page`] (M5's true-removal primitive,
//! reused as-is — not reimplemented).
//!
//! ## Honesty about `PiiPattern`'s scope
//!
//! These are simple, well-known regexes for a handful of common,
//! structurally-recognizable PII shapes (email addresses, US SSNs, US
//! phone numbers, 16-digit card numbers) — **not** a comprehensive PII
//! detector. Regex-based detection has real false positives (e.g. any
//! 9-digit run of digits with dashes in the SSN position) and real false
//! negatives (a name, a street address, an SSN written without dashes,
//! non-US phone/ID formats — none of these are pattern-matchable this
//! way). A production-grade PII scrubber needs NLP/NER-based entity
//! detection on top of this, not instead of it. Treat this as "catch the
//! obvious, structurally-regular cases," not "guarantee no PII remains."

use openpdfedit_doc::{DocError, Document};
use openpdfedit_redact::{Rect, RedactError};
use openpdfedit_textedit::TextEditError;
use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BatchError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error(transparent)]
    Redact(#[from] RedactError),
    #[error(transparent)]
    TextEdit(#[from] TextEditError),
    #[error(transparent)]
    Merge(#[from] openpdfedit_pages::PagesError),
}

/// A recognizable PII shape to search for — see this crate's module doc
/// for what "recognizable" does and doesn't cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiiPattern {
    Email,
    /// US Social Security Number, `NNN-NN-NNNN`.
    UsSsn,
    /// US phone number, with or without a leading `+1`, common
    /// separator styles (`-`, `.`, space, or none).
    UsPhone,
    /// A 16-digit card number in 4-4-4-4 groups (space or dash
    /// separated, or run together).
    CardNumber16,
}

impl PiiPattern {
    fn regex(self) -> Regex {
        let pattern = match self {
            PiiPattern::Email => r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
            PiiPattern::UsSsn => r"\b\d{3}-\d{2}-\d{4}\b",
            PiiPattern::UsPhone => r"(\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b",
            PiiPattern::CardNumber16 => r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b",
        };
        // These are all fixed, hand-written patterns — a compile failure
        // here would be a bug in this crate, not bad user input.
        Regex::new(pattern).expect("built-in PII pattern must compile")
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PiiRedactionReport {
    pub pages_processed: u32,
    pub matches_redacted: usize,
}

/// Scans every page of `doc` for text matching any of `patterns` and
/// redacts each match (true removal, via
/// [`openpdfedit_redact::redact_page`] — see that crate's module doc for
/// exactly what "removal" means and its own known limitations, which
/// apply here too since this is built directly on top of it). Does not
/// save — call [`Document::save_incremental`] afterwards.
pub fn redact_pii(
    doc: &mut Document,
    patterns: &[PiiPattern],
) -> Result<PiiRedactionReport, BatchError> {
    let compiled: Vec<Regex> = patterns.iter().map(|p| p.regex()).collect();
    let page_count = doc.page_count()?;
    let mut report = PiiRedactionReport {
        pages_processed: page_count,
        matches_redacted: 0,
    };

    for page_index in 0..page_count {
        // Re-read fresh each page: a prior page's redaction doesn't
        // affect this one, but re-listing per page (rather than once up
        // front) keeps this correct even if a future caller interleaves
        // other edits between pages.
        let content = doc.page_content_bytes(page_index)?;
        let runs = openpdfedit_textedit::list_text_runs(page_index, &content)?;

        let matched_rects: Vec<Rect> = runs
            .iter()
            .filter(|run| compiled.iter().any(|re| re.is_match(&run.text)))
            .map(|run| run.bbox)
            .collect();

        for rect in matched_rects {
            // Each call re-reads the page's *current* content (already
            // reflecting any earlier redaction on this same page), so
            // sequential single-rect redactions here are correct even
            // when several matches land on one page.
            openpdfedit_redact::redact_page(doc, page_index, rect, [0.0, 0.0, 0.0])?;
            report.matches_redacted += 1;
        }
    }

    Ok(report)
}

/// Merges `source_paths` (whole files, read from disk) into one new
/// document written to `output_path` — a thin wrapper around
/// [`openpdfedit_pages::merge`] for batch/CLI use, where there's no
/// already-open `Document`/engine handle to reuse (unlike the desktop
/// app's `merge_documents_cmd`, which this mirrors).
pub fn merge_files(
    source_paths: &[std::path::PathBuf],
    output_path: &std::path::Path,
) -> Result<(), BatchError> {
    let sources: Vec<Vec<u8>> = source_paths
        .iter()
        .map(std::fs::read)
        .collect::<std::io::Result<_>>()
        .map_err(DocError::Save)?;
    let refs: Vec<&[u8]> = sources.iter().map(Vec::as_slice).collect();
    let merged = openpdfedit_pages::merge(&refs)?;
    std::fs::write(output_path, merged).map_err(DocError::Save)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    fn text_page_pdf_bytes(lines: &[(&str, f64, f64)]) -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });

        let mut operations = Vec::new();
        for &(text, x, y) in lines {
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new("Tf", vec!["F1".into(), 12.0.into()]));
            operations.push(Operation::new("Td", vec![x.into(), y.into()]));
            operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
            operations.push(Operation::new("ET", vec![]));
        }
        let content = Content { operations };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn decoded_tj_strings(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> Vec<String> {
        let page = doc.get_dictionary(page_id).unwrap();
        let content_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = doc.get_object(content_id).unwrap().as_stream().unwrap();
        Content::decode(&stream.content)
            .unwrap()
            .operations
            .iter()
            .filter(|op| op.operator == "Tj")
            .filter_map(|op| op.operands.first())
            .filter_map(|o| o.as_str().ok())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect()
    }

    #[test]
    fn redact_pii_removes_an_email_and_leaves_unrelated_text() {
        let bytes = text_page_pdf_bytes(&[
            ("Contact: jane.doe@example.com", 50.0, 700.0),
            ("Just a normal sentence.", 50.0, 650.0),
        ]);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let report = redact_pii(&mut doc, &[PiiPattern::Email]).expect("should succeed");
        assert_eq!(report.matches_redacted, 1);
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page_id = reopened.get_pages()[&1];
        let strings = decoded_tj_strings(&reopened, page_id);
        assert!(
            !strings.iter().any(|s| s.contains("jane.doe@example.com")),
            "the email-bearing run must be gone: {strings:?}"
        );
        assert!(
            strings.iter().any(|s| s.contains("normal sentence")),
            "unrelated text must survive: {strings:?}"
        );
    }

    #[test]
    fn redact_pii_matches_ssn_pattern() {
        let bytes = text_page_pdf_bytes(&[("SSN: 123-45-6789 on file", 50.0, 700.0)]);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let report = redact_pii(&mut doc, &[PiiPattern::UsSsn]).expect("should succeed");
        assert_eq!(report.matches_redacted, 1);
    }

    #[test]
    fn redact_pii_with_no_matches_redacts_nothing() {
        let bytes = text_page_pdf_bytes(&[("Nothing sensitive here at all.", 50.0, 700.0)]);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let report =
            redact_pii(&mut doc, &[PiiPattern::Email, PiiPattern::UsSsn]).expect("should succeed");
        assert_eq!(report.matches_redacted, 0);
        assert_eq!(report.pages_processed, 1);
    }

    #[test]
    fn redact_pii_handles_multiple_matches_on_one_page() {
        let bytes = text_page_pdf_bytes(&[
            ("Email one: a@example.com", 50.0, 700.0),
            ("Email two: b@example.com", 50.0, 650.0),
            ("Keep this line.", 50.0, 600.0),
        ]);
        let mut doc = Document::from_bytes(&bytes).expect("should parse");

        let report = redact_pii(&mut doc, &[PiiPattern::Email]).expect("should succeed");
        assert_eq!(report.matches_redacted, 2);
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page_id = reopened.get_pages()[&1];
        let strings = decoded_tj_strings(&reopened, page_id);
        assert!(!strings.iter().any(|s| s.contains('@')));
        assert!(strings.iter().any(|s| s.contains("Keep this line")));
    }
}
