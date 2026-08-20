//! Cross-crate validation: builds a page with real, real-font text
//! ("Hello World"), edits it to a longer replacement ("Goodbye Cruel
//! World"), and confirms — through real PDFium, not this workspace's own
//! reader — that the page still renders and that PDFium's own character
//! extraction finds a character count consistent with the *new* text,
//! not the old one (real evidence the edit reached PDFium's text engine,
//! not just this crate's own content-stream bookkeeping).

use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-textedit -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

fn text_page_pdf_bytes(text: &str, font_size: f64) -> Vec<u8> {
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
            Operation::new("Tf", vec!["F1".into(), font_size.into()]),
            Operation::new("Td", vec![50.0.into(), 400.0.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
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
fn edit_text_run_produces_a_page_pdfium_reads_the_new_text_from() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-textedit-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, text_page_pdf_bytes("Hello World", 24.0))
        .expect("should write temp file");

    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the fixture");
    let before_boxes = engine
        .page_char_boxes(handle, 0)
        .expect("char boxes should succeed");
    assert_eq!(
        before_boxes.len(),
        11,
        "\"Hello World\" is 11 characters (including the space)"
    );
    engine.close(handle);

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    let runs = openpdfedit_textedit::list_text_runs(0, &doc.page_content_bytes(0).unwrap())
        .expect("should find text runs");
    assert_eq!(runs.len(), 1);

    let new_text = "Goodbye Cruel World";
    openpdfedit_textedit::edit_text_run(&mut doc, &runs[0], new_text)
        .expect("edit_text_run should succeed");

    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the edited bytes");

    let edited_handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the edited file");

    let after_boxes = engine
        .page_char_boxes(edited_handle, 0)
        .expect("char boxes should succeed on the edited page");
    assert_eq!(
        after_boxes.len(),
        new_text.chars().count(),
        "PDFium's own text engine must see exactly the new text's character count, \
         not the old text's (11) or a sum of both (30)"
    );

    // Old text must not be independently recoverable either — every
    // extracted character must fall within the page (a weak but real
    // sanity check that nothing wildly off-page/duplicated survived).
    for b in &after_boxes {
        assert!(
            b.left >= -1.0 && b.right <= 613.0,
            "character out of page bounds: {b:?}"
        );
    }

    // The edited page must still render without erroring.
    let tile = engine
        .render_page(edited_handle, 0, 300)
        .expect("edited page should still render");
    assert!(tile.height > 0);

    engine.close(edited_handle);
    let _ = std::fs::remove_file(&tmp_path);
}
