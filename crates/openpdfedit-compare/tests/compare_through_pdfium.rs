//! Cross-crate validation: renders two real PDFs through real PDFium and
//! confirms `compare_pixels` reports zero differing pixels for identical
//! documents, and a non-zero differing-pixel count whose bounding box
//! lands in the region that actually changed for documents that differ.
//! Single test function in its own process, so constructing
//! `PdfiumEngine::new` directly (rather than going through the
//! `EngineHandle` thread wrapper) is safe — see openpdfedit-engine's
//! module doc on why two `PdfiumEngine`s in one process is unsafe, which
//! doesn't apply here since there's only ever one.

use openpdfedit_compare::compare_pixels;
use openpdfedit_engine::{Engine, PdfiumEngine};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-compare -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

/// A one-page PDF with a solid black filled rectangle at a fixed
/// position (`x_offset` shifts it horizontally), on an otherwise blank
/// white page — simple, high-contrast shapes so a pixel diff has an
/// unambiguous "changed" region to find.
fn rect_page_pdf_bytes(x_offset: f64) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let content = Content {
        operations: vec![
            Operation::new(
                "re",
                vec![
                    (100.0 + x_offset).into(),
                    100.0.into(),
                    50.0.into(),
                    50.0.into(),
                ],
            ),
            Operation::new("f", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn compare_pixels_finds_no_diff_for_identical_documents_and_a_real_diff_for_a_moved_rect() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_dir = std::env::temp_dir();
    let path_a = tmp_dir.join(format!("openpdfedit-compare-a-{}.pdf", std::process::id()));
    let path_b_same = tmp_dir.join(format!(
        "openpdfedit-compare-b-same-{}.pdf",
        std::process::id()
    ));
    let path_b_moved = tmp_dir.join(format!(
        "openpdfedit-compare-b-moved-{}.pdf",
        std::process::id()
    ));

    std::fs::write(&path_a, rect_page_pdf_bytes(0.0)).unwrap();
    std::fs::write(&path_b_same, rect_page_pdf_bytes(0.0)).unwrap();
    std::fs::write(&path_b_moved, rect_page_pdf_bytes(80.0)).unwrap();

    let handle_a = engine.open(&path_a).expect("should open a");
    let handle_b_same = engine.open(&path_b_same).expect("should open b_same");
    let handle_b_moved = engine.open(&path_b_moved).expect("should open b_moved");

    let identical_report =
        compare_pixels(&engine, handle_a, handle_b_same, 300).expect("compare should succeed");
    assert_eq!(identical_report.pages.len(), 1);
    assert_eq!(identical_report.pages[0].differing_pixels, 0);
    assert!(identical_report.pages[0].bbox.is_none());
    assert!(identical_report.pages[0].total_pixels > 0);

    let moved_report =
        compare_pixels(&engine, handle_a, handle_b_moved, 300).expect("compare should succeed");
    assert_eq!(moved_report.pages.len(), 1);
    let page_diff = &moved_report.pages[0];
    assert!(
        page_diff.differing_pixels > 0,
        "moving the rectangle should produce a real pixel difference"
    );
    let bbox = page_diff
        .bbox
        .expect("a non-zero diff must have a bounding box");
    // The rect moved 80pt right on a 300pt-wide page rendered at 300px —
    // roughly a 1:1 point-to-pixel scale, so the changed region should be
    // in the right two-thirds of the page, not clustered at the left edge
    // where nothing changed.
    assert!(
        bbox.left > 90,
        "the changed region should start around where the rect used to end, got left={}",
        bbox.left
    );

    engine.close(handle_a);
    engine.close(handle_b_same);
    engine.close(handle_b_moved);
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b_same);
    let _ = std::fs::remove_file(&path_b_moved);
}
