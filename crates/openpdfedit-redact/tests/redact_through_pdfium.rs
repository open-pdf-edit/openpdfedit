//! Cross-crate validation: builds a page with real, real-font, real-
//! position text ("SECRET DATA" next to "PUBLIC TEXT"), redacts a rect
//! covering only the first, and confirms — through real PDFium, not this
//! workspace's own reader — that (a) the redacted region no longer has
//! any extractable characters (the actual "true removal" proof: a
//! naive black-box-only redaction would still have them), (b) the
//! untouched text elsewhere on the page is unaffected, and (c) the
//! rendered pixels in the redacted region are solid black (the visual
//! side of the promise).

use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};
use openpdfedit_redact::Rect;

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-redact -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

/// A one-page PDF with two separate text runs at known positions: a
/// "secret" line near the bottom-left (which will be redacted) and a
/// "public" line near the top-right (which must survive untouched).
fn two_text_runs_pdf_bytes() -> Vec<u8> {
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
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![50.0.into(), 50.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("SECRET DATA")]),
            Operation::new("ET", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![50.0.into(), 700.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("PUBLIC TEXT")]),
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
fn redact_page_truly_removes_text_and_leaves_the_rest_intact() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-redact-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, two_text_runs_pdf_bytes()).expect("should write temp file");

    // Baseline: PDFium finds real characters from *both* text runs.
    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the fixture");
    let before_boxes = engine
        .page_char_boxes(handle, 0)
        .expect("char boxes should succeed");
    assert!(
        before_boxes.len() >= 20,
        "both text runs (SECRET DATA + PUBLIC TEXT, ~23 chars) should be extractable before redaction, got {}",
        before_boxes.len()
    );
    engine.close(handle);

    // Redact a rect covering only the "SECRET DATA" line (bottom-left).
    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    let removed = openpdfedit_redact::redact_page(
        &mut doc,
        0,
        Rect {
            x0: 40.0,
            y0: 40.0,
            x1: 300.0,
            y1: 80.0,
        },
        [0.0, 0.0, 0.0],
    )
    .expect("redact_page should succeed");
    assert_eq!(removed, 1, "exactly the SECRET DATA Tj call");

    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the redacted bytes");

    let redacted_handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the redacted file");

    let after_boxes = engine
        .page_char_boxes(redacted_handle, 0)
        .expect("char boxes should succeed on the redacted page");

    // The true-removal proof: no character box may fall within (or even
    // meaningfully overlap) the redacted rectangle.
    let leaked: Vec<_> = after_boxes
        .iter()
        .filter(|b| b.left < 300.0 && b.right > 40.0 && b.bottom < 80.0 && b.top > 40.0)
        .collect();
    assert!(
        leaked.is_empty(),
        "no character from the redacted region may remain extractable, found: {leaked:?}"
    );

    // The untouched "PUBLIC TEXT" line must still be fully present —
    // redaction must not have collaterally deleted unrelated content.
    let public_region_boxes = after_boxes.iter().filter(|b| b.top > 690.0).count();
    assert!(
        public_region_boxes >= 10,
        "PUBLIC TEXT (11 chars) should be untouched, found {public_region_boxes} chars near it"
    );

    // Visual side of the promise: the redacted region renders as solid
    // black, not just "no text" but "nothing visible there at all."
    let tile = engine
        .render_page(redacted_handle, 0, 612)
        .expect("should render at 1:1 scale (612px wide == 612pt page)");
    // Page-space (100, 60) — well inside the redaction rect — maps to
    // pixel space by flipping y (PDF is bottom-up, image is top-down).
    let px = 100u32;
    let py = tile.height.saturating_sub(60);
    let idx = ((py * tile.width + px) * 4) as usize;
    assert!(
        idx + 3 < tile.rgba.len(),
        "sample pixel must be within the rendered tile"
    );
    let (r, g, b) = (tile.rgba[idx], tile.rgba[idx + 1], tile.rgba[idx + 2]);
    assert!(
        r < 20 && g < 20 && b < 20,
        "pixel inside the redacted region should be solid black, got rgb({r},{g},{b})"
    );

    engine.close(redacted_handle);
    let _ = std::fs::remove_file(&tmp_path);
}
