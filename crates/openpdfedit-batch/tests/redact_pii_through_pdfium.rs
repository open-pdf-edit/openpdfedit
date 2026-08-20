//! Cross-crate validation: a page with a real email address (real
//! Helvetica text, next to unrelated text) has PII redaction run against
//! it, and PDFium's own character extraction confirms the email is
//! genuinely gone — not just covered — while the unrelated text
//! survives. Mirrors `openpdfedit-redact`'s own PDFium cross-validation
//! test, applied through this crate's PII-search layer instead of a
//! caller-supplied rectangle.

use openpdfedit_batch::{redact_pii, PiiPattern};
use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-batch -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

fn two_line_pdf_bytes(line1: &str, line2: &str) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 18.0.into()]),
            Operation::new("Td", vec![50.0.into(), 700.0.into()]),
            Operation::new("Tj", vec![Object::string_literal(line1)]),
            Operation::new("ET", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 18.0.into()]),
            Operation::new("Td", vec![50.0.into(), 600.0.into()]),
            Operation::new("Tj", vec![Object::string_literal(line2)]),
            Operation::new("ET", vec![]),
        ],
    };
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

#[test]
fn redact_pii_truly_removes_the_matched_email_through_real_pdfium() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let line1 = "Contact us at hello@example.com";
    let line2 = "This line has nothing sensitive.";

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-batch-pii-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, two_line_pdf_bytes(line1, line2)).expect("should write temp file");

    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the fixture");
    let before_boxes = engine
        .page_char_boxes(handle, 0)
        .expect("char boxes should succeed");
    // Not an exact character-count match against the source strings —
    // PDFium's own char enumeration doesn't necessarily map 1:1 onto
    // Rust's `.chars().count()` (e.g. how whitespace is represented) —
    // just confirms both lines produced real, non-trivial extractable
    // text to compare against after redaction.
    assert!(
        before_boxes.len() > 40,
        "expected real text from both lines, got {}",
        before_boxes.len()
    );
    let before_in_top_band = before_boxes.iter().filter(|b| b.top > 690.0).count();
    assert!(
        before_in_top_band > 0,
        "the redacted line must have real characters before redaction"
    );
    engine.close(handle);

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    let report = redact_pii(&mut doc, &[PiiPattern::Email]).expect("redact_pii should succeed");
    assert_eq!(report.matches_redacted, 1);

    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the redacted bytes");

    let redacted_handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the redacted file");
    let after_boxes = engine
        .page_char_boxes(redacted_handle, 0)
        .expect("char boxes should succeed on the redacted page");

    // The core "true removal" proof: no character remains in the
    // redacted line's vertical band...
    let leaked_in_top_band = after_boxes.iter().filter(|b| b.top > 690.0).count();
    assert_eq!(
        leaked_in_top_band, 0,
        "no characters should remain in the redacted line's position"
    );
    // ...while the unrelated line survives untouched.
    assert!(
        after_boxes.len() > 20,
        "the unrelated line's text should still be extractable, got {} chars",
        after_boxes.len()
    );

    engine.close(redacted_handle);
    let _ = std::fs::remove_file(&tmp_path);
}
