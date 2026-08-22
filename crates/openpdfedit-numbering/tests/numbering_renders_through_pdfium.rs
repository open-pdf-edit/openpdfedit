//! Numbering that produces a structurally valid file showing nothing is
//! the failure that matters, and only rendering catches it — a content
//! stream naming a font resource the page doesn't have compiles, saves,
//! and draws a blank page. (That is not hypothetical: it is exactly what
//! this crate did when it was first wired onto `ensure_page_font`, whose
//! returned resource name differs from the one the stream was
//! hardcoding.)

use std::sync::{Arc, OnceLock};

use lopdf::{dictionary, Object, Stream};
use openpdfedit_doc::Document;
use openpdfedit_engine::{EngineHandle, RenderedTile};
use openpdfedit_numbering::{add_numbering, bates_style, Anchor, NumberStyle, Numbering};

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

fn shared_engine() -> Option<&'static EngineHandle> {
    static ENGINE: OnceLock<Option<EngineHandle>> = OnceLock::new();
    ENGINE
        .get_or_init(|| match EngineHandle::spawn(dev_vendor_lib_dir()) {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("skipping: PDFium not available ({e}) — run scripts/fetch-pdfium.sh");
                None
            }
        })
        .as_ref()
}

/// Blank Letter pages, so anything non-white afterwards is the label.
fn blank_pages_pdf(count: u32) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_ids: Vec<_> = (0..count)
        .map(|_| {
            let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
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
            "Count" => count,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn render(engine: &EngineHandle, pdf: &[u8], page: u32, tag: &str) -> Arc<RenderedTile> {
    let path = std::env::temp_dir().join(format!(
        "openpdfedit-numbering-test-{}-{tag}.pdf",
        std::process::id()
    ));
    std::fs::write(&path, pdf).expect("should write temp file");
    let handle = engine.open(&path).expect("PDFium should open the file");
    let tile = engine
        .render_page(handle, page, 400)
        .expect("PDFium should render the page");
    engine.close(handle);
    let _ = std::fs::remove_file(&path);
    tile
}

/// Ink in a fractional region, top-left origin.
fn ink_in(tile: &RenderedTile, region: (f32, f32, f32, f32)) -> usize {
    let (fx0, fy0, fx1, fy1) = region;
    let x0 = (fx0 * tile.width as f32) as u32;
    let x1 = (fx1 * tile.width as f32) as u32;
    let y0 = (fy0 * tile.height as f32) as u32;
    let y1 = (fy1 * tile.height as f32) as u32;
    let mut count = 0;
    for y in y0..y1.min(tile.height) {
        for x in x0..x1.min(tile.width) {
            let i = ((y * tile.width + x) * 4) as usize;
            if tile.rgba[i] < 245 || tile.rgba[i + 1] < 245 || tile.rgba[i + 2] < 245 {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn bates_numbering_renders_in_the_bottom_right_of_every_page() {
    let Some(engine) = shared_engine() else {
        return;
    };

    let mut doc = Document::from_bytes(&blank_pages_pdf(3)).expect("fixture should parse");
    let numbering = Numbering {
        prefix: "ACME-".to_string(),
        suffix: String::new(),
        start_at: 41,
        digits: 6,
    };
    assert_eq!(
        add_numbering(&mut doc, &[0, 1, 2], &numbering, &bates_style())
            .expect("numbering should succeed"),
        3
    );
    let saved = doc.save_incremental().expect("save should succeed");

    for page in 0..3 {
        let tile = render(engine, &saved, page, &format!("bates{page}"));
        assert!(
            ink_in(&tile, (0.55, 0.85, 1.0, 1.0)) > 0,
            "page {page} has no Bates number in the bottom-right corner — \
             a label drawn with an unregistered font resource renders blank"
        );
        assert_eq!(
            ink_in(&tile, (0.0, 0.0, 0.45, 0.5)),
            0,
            "page {page} has ink in the top-left, where nothing was numbered"
        );
    }
}

#[test]
fn a_page_number_can_sit_bottom_centre_instead() {
    let Some(engine) = shared_engine() else {
        return;
    };

    let mut doc = Document::from_bytes(&blank_pages_pdf(2)).expect("fixture should parse");
    let style = NumberStyle {
        anchor: Anchor::BottomCenter,
        font_size: 12.0,
        ..NumberStyle::default()
    };
    add_numbering(&mut doc, &[0, 1], &Numbering::default(), &style)
        .expect("numbering should succeed");
    let saved = doc.save_incremental().expect("save should succeed");

    let tile = render(engine, &saved, 0, "centre");
    assert!(
        ink_in(&tile, (0.35, 0.85, 0.65, 1.0)) > 0,
        "nothing at bottom centre"
    );
    assert_eq!(
        ink_in(&tile, (0.8, 0.85, 1.0, 1.0)),
        0,
        "ink in the corner too"
    );
}

/// A page that already registers its own Helvetica must not have that
/// entry clobbered, and the label must still find the font it names.
#[test]
fn numbering_a_page_that_already_has_fonts_still_renders() {
    let Some(engine) = shared_engine() else {
        return;
    };

    let base = {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1",
            "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 24 Tf 72 700 Td (Existing text) Tj ET\n".to_vec(),
        ));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
            ),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    };

    let before = render(engine, &base, 0, "fonts-before");
    let existing_text_ink = ink_in(&before, (0.0, 0.0, 1.0, 0.3));
    assert!(existing_text_ink > 0, "fixture should draw its own text");

    let mut doc = Document::from_bytes(&base).expect("fixture should parse");
    add_numbering(&mut doc, &[0], &Numbering::default(), &bates_style())
        .expect("numbering should succeed");
    let saved = doc.save_incremental().expect("save should succeed");

    let after = render(engine, &saved, 0, "fonts-after");
    assert!(
        ink_in(&after, (0.55, 0.85, 1.0, 1.0)) > 0,
        "the label didn't render on a page that already had fonts"
    );
    assert_eq!(
        ink_in(&after, (0.0, 0.0, 1.0, 0.3)),
        existing_text_ink,
        "numbering changed the page's own text"
    );
}
