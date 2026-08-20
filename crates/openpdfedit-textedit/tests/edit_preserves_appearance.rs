//! Pixel-level proof, through real PDFium, that editing text keeps the
//! text *looking the same* — not merely that the new characters are
//! extractable.
//!
//! The bug this exists for: replacement text was appended with no colour
//! operator, so it always drew in the PDF default, pure black. On a page
//! where a line sits on a coloured banner in white type, editing that
//! line turned it black-on-dark — visually identical to the text having
//! disappeared, which is exactly how it was reported ("after editing
//! those in a colored background, font color will change, and sometimes
//! the entire text will disappear").
//!
//! Extraction-based assertions cannot catch that: the characters are
//! present and correct either way. Only rendering and counting pixels
//! can, which is what these tests do.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Object, Stream};
use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};
use openpdfedit_textedit::{edit_text_run, list_text_runs_in_page, move_text_run};

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

/// A page with a dark navy banner and white text on top of it — the
/// "text with a separate background colour" shape from the report.
fn white_on_dark_banner_pdf() -> Vec<u8> {
    let content = Content {
        operations: vec![
            // The banner: dark navy, x 40..400, y 690..730.
            Operation::new("rg", vec![0.1.into(), 0.1.into(), 0.35.into()]),
            Operation::new("re", vec![40.into(), 690.into(), 360.into(), 40.into()]),
            Operation::new("f", vec![]),
            // The text: white, sitting inside the banner.
            Operation::new("BT", vec![]),
            Operation::new("rg", vec![1.into(), 1.into(), 1.into()]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![50.into(), 702.into()]),
            Operation::new("Tj", vec![Object::string_literal("HEADLINE")]),
            Operation::new("ET", vec![]),
            // A second, ordinary black line below the banner — the one
            // the move check uses, so its new position is visible
            // against the white page rather than white-on-white.
            Operation::new("BT", vec![]),
            Operation::new("rg", vec![0.into(), 0.into(), 0.into()]),
            Operation::new("Tf", vec!["F1".into(), 18.into()]),
            Operation::new("Td", vec![50.into(), 600.into()]),
            Operation::new("Tj", vec![Object::string_literal("BODY LINE")]),
            Operation::new("ET", vec![]),
        ],
    };

    let mut raw = lopdf::Document::with_version("1.5");
    let pages_id = raw.new_object_id();
    let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = raw.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        }}},
    });
    raw.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    raw.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    raw.save_to(&mut bytes).unwrap();
    bytes
}

/// Counts pixels inside a PDF-space rectangle that are "near white" and
/// "near the banner navy", by rendering the page at `tile` scale.
///
/// The distinction is the whole point: white glyph pixels on a navy field
/// are the evidence the text is still legible. If the replacement drew in
/// black, the near-white count collapses to zero while the dark count
/// *rises* — the glyphs are there, just invisible.
struct BannerInk {
    white: usize,
    banner: usize,
}

fn banner_ink(rgba: &[u8], width: u32, height: u32, page_height_pt: f64) -> BannerInk {
    let scale = height as f64 / page_height_pt;
    // The banner in image space (y flipped: PDF y=730 is the top).
    let x0 = (40.0 * scale) as u32;
    let x1 = (400.0 * scale).min(width as f64) as u32;
    let y0 = ((792.0 - 730.0) * scale) as u32;
    let y1 = ((792.0 - 690.0) * scale).min(height as f64) as u32;

    let mut ink = BannerInk {
        white: 0,
        banner: 0,
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * width + x) * 4) as usize;
            let (r, g, b) = (rgba[i] as u32, rgba[i + 1] as u32, rgba[i + 2] as u32);
            if r > 200 && g > 200 && b > 200 {
                ink.white += 1;
            } else if r < 120 && g < 120 && b < 160 {
                ink.banner += 1;
            }
        }
    }
    ink
}

/// Writes a PNG next to the test's temp PDF when
/// `OPENPDFEDIT_DUMP_RENDERS` is set — for eyeballing a failure, not
/// part of the assertion.
fn dump(name: &str, rgba: &[u8], width: u32, height: u32) {
    if std::env::var_os("OPENPDFEDIT_DUMP_RENDERS").is_none() {
        return;
    }
    if let Some(img) = image::RgbaImage::from_raw(width, height, rgba.to_vec()) {
        let path = std::env::temp_dir().join(format!("openpdfedit-{name}.png"));
        let _ = image::DynamicImage::ImageRgba8(img).save(&path);
        eprintln!("wrote {}", path.display());
    }
}

/// Both checks share one test function because a process may only ever
/// construct a single `PdfiumEngine` — PDFium's library initialization is
/// global, and a second instance segfaults. Every other PDFium-backed
/// integration test in this repo follows the same one-test-per-file
/// shape for the same reason.
#[test]
fn editing_and_moving_preserve_how_text_actually_renders() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };
    editing_keeps_the_colour(&engine);
    moving_relocates_the_glyphs(&engine);
}

fn editing_keeps_the_colour(engine: &PdfiumEngine) {
    let dir = std::env::temp_dir();
    let before_path = dir.join(format!(
        "openpdfedit-banner-before-{}.pdf",
        std::process::id()
    ));
    let after_path = dir.join(format!(
        "openpdfedit-banner-after-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&before_path, white_on_dark_banner_pdf()).expect("write");

    let handle = engine.open(&before_path).expect("open");
    let before = engine.render_page(handle, 0, 800).expect("render");
    let before_ink = banner_ink(&before.rgba, before.width, before.height, 792.0);
    dump("banner-before", &before.rgba, before.width, before.height);
    engine.close(handle);

    let mut doc = Document::open(&before_path).expect("doc open");
    let runs = list_text_runs_in_page(&doc, 0).expect("list");
    let run = runs
        .iter()
        .find(|r| r.text.contains("HEADLINE"))
        .expect("the headline run");
    edit_text_run(&mut doc, run, "REPLACED").expect("edit");
    std::fs::write(&after_path, doc.save_incremental().expect("save")).expect("write");

    let handle = engine.open(&after_path).expect("reopen");
    let after = engine.render_page(handle, 0, 800).expect("render");
    let after_ink = banner_ink(&after.rgba, after.width, after.height, 792.0);
    dump("banner-after", &after.rgba, after.width, after.height);
    engine.close(handle);

    assert!(
        before_ink.white > 500,
        "sanity: the original white headline should be plainly visible, got {} px",
        before_ink.white
    );
    // The banner itself must survive (this is the RemovalScope::TextOnly
    // guarantee — an earlier build wiped the background along with the
    // text it was replacing).
    assert!(
        after_ink.banner > before_ink.banner / 2,
        "the coloured background must survive the edit: {} -> {} dark px",
        before_ink.banner,
        after_ink.banner
    );
    // And the replacement must still be *white*. Drawn in the default
    // black it would be invisible against the navy, and this count would
    // fall to roughly zero.
    assert!(
        after_ink.white > before_ink.white / 2,
        "the replacement must keep the original white fill colour, not revert to black: \
         {} -> {} white px inside the banner",
        before_ink.white,
        after_ink.white
    );

    let _ = std::fs::remove_file(&before_path);
    let _ = std::fs::remove_file(&after_path);
}

fn moving_relocates_the_glyphs(engine: &PdfiumEngine) {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("openpdfedit-move-{}.pdf", std::process::id()));
    let moved_path = dir.join(format!("openpdfedit-moved-{}.pdf", std::process::id()));
    std::fs::write(&path, white_on_dark_banner_pdf()).expect("write");

    let mut doc = Document::open(&path).expect("doc open");
    let runs = list_text_runs_in_page(&doc, 0).expect("list");
    let run = runs
        .iter()
        .find(|r| r.text.contains("BODY LINE"))
        .expect("the body run");
    // Right and down, well clear of its original spot.
    move_text_run(&mut doc, run, 180.0, -150.0).expect("move");
    std::fs::write(&moved_path, doc.save_incremental().expect("save")).expect("write");

    let handle = engine.open(&moved_path).expect("open");
    let tile = engine.render_page(handle, 0, 800).expect("render");
    dump("moved", &tile.rgba, tile.width, tile.height);

    // Everything else on the page is untouched: the banner and the
    // headline that sits on it are still exactly where they were.
    let ink = banner_ink(&tile.rgba, tile.width, tile.height, 792.0);
    assert!(
        ink.banner > 1000,
        "moving one run must not disturb the rest of the page, got {} banner px",
        ink.banner
    );
    assert!(
        ink.white > 500,
        "the untouched headline must still be there, got {} white px",
        ink.white
    );

    // PDFium reads the moved line back at its new baseline. Its
    // characters are the only ones on the page near y=450.
    let boxes = engine.page_char_boxes(handle, 0).expect("char boxes");
    let moved: Vec<_> = boxes
        .iter()
        .filter(|b| b.bottom > 440.0 && b.top < 480.0)
        .collect();
    assert_eq!(
        moved.len(),
        "BODY LINE".len(),
        "expected the whole moved line at its new baseline, found {} glyphs",
        moved.len()
    );
    let leftmost = moved.iter().fold(f32::MAX, |acc, b| acc.min(b.left));
    assert!(
        (leftmost - 230.0).abs() < 4.0,
        "moved 180pt right of x=50, so the line should start near x=230; got {leftmost}"
    );
    assert!(
        !boxes
            .iter()
            .any(|b| b.bottom > 590.0 && b.top < 620.0 && b.left < 200.0),
        "no glyphs may be left behind at the original position"
    );
    engine.close(handle);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&moved_path);
}
