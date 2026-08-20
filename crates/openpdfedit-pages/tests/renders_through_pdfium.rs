//! Cross-crate validation: `merge` and `extract_pages` produce a fresh,
//! full-rewrite document, which carries more structural risk than an
//! incremental update (renumbering every object, rebuilding the page
//! tree from scratch). Confirms real PDFium — not just our own
//! lopdf-based reader — accepts and renders the output.

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

fn n_page_pdf_bytes(n: usize) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_ids: Vec<_> = (0..n)
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
            "Count" => n as i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn render_every_page(engine: &PdfiumEngine, bytes: &[u8], tag: &str, expected_pages: u32) {
    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-pages-pdfium-{tag}-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, bytes).expect("should write temp file");

    let handle = engine
        .open(&tmp_path)
        .unwrap_or_else(|e| panic!("PDFium should open {tag} output: {e}"));
    let page_count = engine
        .page_count(handle)
        .expect("page count should succeed");
    assert_eq!(page_count, expected_pages);

    for page_index in 0..page_count {
        let tile = engine
            .render_page(handle, page_index, 150)
            .unwrap_or_else(|e| panic!("PDFium should render {tag} page {page_index}: {e}"));
        assert!(tile.height > 0);
    }

    engine.close(handle);
    let _ = std::fs::remove_file(&tmp_path);
}

#[test]
fn pdfium_renders_merged_and_extracted_output() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let a = n_page_pdf_bytes(2);
    let b = n_page_pdf_bytes(3);

    let merged = openpdfedit_pages::merge(&[&a, &b]).expect("merge should succeed");
    render_every_page(&engine, &merged, "merged", 5);

    let extracted =
        openpdfedit_pages::extract_pages(&merged, &[4, 1, 0]).expect("extract should succeed");
    render_every_page(&engine, &extracted, "extracted", 3);
}
