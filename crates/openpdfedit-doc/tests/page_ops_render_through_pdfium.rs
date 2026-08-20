//! Cross-crate validation for page operations, mirroring
//! `openpdfedit-annot`'s PDFium test: an lopdf-valid incremental update
//! is not automatically a *real-world*-valid PDF. Page-tree restructuring
//! (delete, reorder — which flattens the tree) touches more fundamental
//! structure than an annotation append does, so it's worth confirming
//! PDFium (the strictest, Chrome-grade parser available here) is still
//! happy with the result, not just our own reader.

use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

fn three_page_pdf_bytes() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_ids: Vec<_> = (0..3)
        .map(|i| {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tj", vec![Object::string_literal(format!("page {i}"))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            })
        })
        .collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|&id| id.into()).collect::<Vec<Object>>(),
            "Count" => 3,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn pdfium_renders_a_document_after_reorder_delete_rotate_and_crop() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let mut doc = Document::from_bytes(&three_page_pdf_bytes()).expect("base doc should parse");

    doc.reorder_pages(&[2, 0, 1])
        .expect("reorder should succeed");
    doc.rotate_page(0, 90).expect("rotate should succeed");
    doc.set_crop_box(1, [10.0, 10.0, 400.0, 500.0])
        .expect("crop should succeed");
    let saved = doc.save_incremental().expect("save should succeed");
    let mut doc = Document::from_bytes(&saved).expect("re-load should succeed for the next edit");
    doc.delete_page(2).expect("delete should succeed");
    let saved = doc.save_incremental().expect("second save should succeed");

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-page-ops-pdfium-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, &saved).expect("should write temp file");

    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the restructured file");
    let page_count = engine
        .page_count(handle)
        .expect("page count should succeed");
    assert_eq!(page_count, 2, "started with 3, reordered, then deleted one");

    for page_index in 0..page_count {
        let tile = engine
            .render_page(handle, page_index, 200)
            .expect("PDFium should render every remaining page");
        assert!(tile.height > 0);
        assert_eq!(tile.rgba.len(), (tile.width * tile.height * 4) as usize);
    }

    engine.close(handle);
    let _ = std::fs::remove_file(&tmp_path);
}
