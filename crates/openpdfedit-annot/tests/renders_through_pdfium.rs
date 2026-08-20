//! Cross-crate validation: an lopdf-valid incremental update is not
//! automatically a *real-world*-valid PDF. This test writes an annotated
//! document to disk and opens/renders it through the actual PDFium
//! engine (the same one the desktop app uses, and the strictest,
//! Chrome-grade parser available to this project) — the property under
//! test is that a real-world viewer accepts the file at all, not just
//! that our own writer's output re-parses with our own reader.

use openpdfedit_annot::{add_annotation, AnnotationKind, Color, NewAnnotation, Rect};
use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    // Mirrors openpdfedit-engine's own dev-lookup, but from this crate's
    // manifest dir (crates/openpdfedit-annot -> workspace root).
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

fn minimal_pdf_bytes() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content = Content {
        operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
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
fn pdfium_renders_a_document_with_every_annotation_kind() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("base doc should parse");

    let rect = Rect {
        x0: 72.0,
        y0: 650.0,
        x1: 300.0,
        y1: 700.0,
    };
    add_annotation(
        &mut doc,
        0,
        NewAnnotation {
            rect,
            color: Color::YELLOW,
            kind: AnnotationKind::Highlight { quads: vec![rect] },
            contents: Some("highlighted".into()),
            opacity: 0.4,
        },
    )
    .expect("highlight should add");
    add_annotation(
        &mut doc,
        0,
        NewAnnotation {
            rect,
            color: Color::RED,
            kind: AnnotationKind::Underline { quads: vec![rect] },
            contents: None,
            opacity: 1.0,
        },
    )
    .expect("underline should add");
    add_annotation(
        &mut doc,
        0,
        NewAnnotation {
            rect: Rect {
                x0: 72.0,
                y0: 500.0,
                x1: 250.0,
                y1: 560.0,
            },
            color: Color::BLACK,
            kind: AnnotationKind::FreeText {
                text: "A note rendered through real PDFium.".into(),
                font_size: 12.0,
            },
            contents: None,
            opacity: 1.0,
        },
    )
    .expect("freetext should add");
    add_annotation(
        &mut doc,
        0,
        NewAnnotation {
            rect: Rect {
                x0: 72.0,
                y0: 400.0,
                x1: 200.0,
                y1: 450.0,
            },
            color: Color::BLACK,
            kind: AnnotationKind::Ink {
                strokes: vec![vec![(80.0, 410.0), (120.0, 440.0), (160.0, 410.0)]],
            },
            contents: None,
            opacity: 1.0,
        },
    )
    .expect("ink should add");

    let saved = doc
        .save_incremental()
        .expect("incremental save should succeed");

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-annot-pdfium-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, &saved).expect("should write temp file");

    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the annotated file");
    let page_count = engine
        .page_count(handle)
        .expect("page count should succeed");
    assert_eq!(page_count, 1);

    let tile = engine
        .render_page(handle, 0, 300)
        .expect("PDFium should render the annotated page");
    assert!(tile.height > 0);
    assert_eq!(tile.rgba.len(), (tile.width * tile.height * 4) as usize);

    engine.close(handle);
    let _ = std::fs::remove_file(&tmp_path);
}
