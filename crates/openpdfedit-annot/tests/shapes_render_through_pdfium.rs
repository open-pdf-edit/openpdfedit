//! Shape annotations, checked by pixels.
//!
//! A shape whose appearance stream paints nothing, or paints in the
//! wrong colour, still produces a structurally valid PDF — so these
//! assert on what PDFium actually renders: an outline puts ink on its
//! border and leaves the middle alone, a filled shape fills, and the
//! colour that comes out is the colour that went in.

use std::sync::{Arc, OnceLock};

use openpdfedit_annot::{add_annotation, AnnotationKind, Color, NewAnnotation, Rect};
use openpdfedit_doc::Document;
use openpdfedit_engine::{EngineHandle, RenderedTile};

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

/// One engine for the whole binary — PDFium's global init is not safe to
/// run more than once per process.
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

/// A blank Letter page, so any ink in the render is the annotation.
fn blank_page_pdf() -> Vec<u8> {
    use lopdf::{dictionary, Object, Stream};
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
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
}

/// Adds one shape covering the middle of the page, saves, renders.
fn render_shape(engine: &EngineHandle, kind: AnnotationKind, tag: &str) -> Arc<RenderedTile> {
    let mut doc = Document::from_bytes(&blank_page_pdf()).expect("fixture should parse");
    add_annotation(
        &mut doc,
        0,
        NewAnnotation {
            // Middle half of the page, in PDF points.
            rect: Rect {
                x0: 153.0,
                y0: 198.0,
                x1: 459.0,
                y1: 594.0,
            },
            color: Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            },
            kind,
            contents: None,
            opacity: 1.0,
        },
    )
    .expect("shape should add");
    let saved = doc.save_incremental().expect("save should succeed");

    let path = std::env::temp_dir().join(format!(
        "openpdfedit-shape-test-{}-{tag}.pdf",
        std::process::id()
    ));
    std::fs::write(&path, &saved).expect("should write temp file");
    let handle = engine.open(&path).expect("PDFium should open the file");
    let tile = engine
        .render_page(handle, 0, 400)
        .expect("PDFium should render the page");
    engine.close(handle);
    let _ = std::fs::remove_file(&path);
    tile
}

fn pixel_at(tile: &RenderedTile, fx: f32, fy: f32) -> [u8; 3] {
    let x = ((fx * tile.width as f32) as u32).min(tile.width - 1);
    let y = ((fy * tile.height as f32) as u32).min(tile.height - 1);
    let i = ((y * tile.width + x) * 4) as usize;
    [tile.rgba[i], tile.rgba[i + 1], tile.rgba[i + 2]]
}

fn is_reddish(px: [u8; 3]) -> bool {
    px[0] > 150 && px[1] < 110 && px[2] < 110
}

fn is_white(px: [u8; 3]) -> bool {
    px.iter().all(|c| *c > 245)
}

/// Any ink at all in a fractional region.
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
            if !is_white([tile.rgba[i], tile.rgba[i + 1], tile.rgba[i + 2]]) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn a_rectangle_outline_draws_its_border_and_leaves_the_middle_alone() {
    let Some(engine) = shared_engine() else {
        return;
    };
    let tile = render_shape(
        engine,
        AnnotationKind::Square {
            line_width: 4.0,
            fill: None,
        },
        "rect-outline",
    );

    // The rect spans x 0.25..0.75 and (top-left origin) y 0.25..0.75.
    assert!(
        ink_in(&tile, (0.24, 0.24, 0.76, 0.27)) > 0,
        "no ink along the top border"
    );
    assert!(
        ink_in(&tile, (0.24, 0.73, 0.76, 0.76)) > 0,
        "no ink along the bottom border"
    );
    assert!(
        is_white(pixel_at(&tile, 0.5, 0.5)),
        "an outline-only rectangle filled its interior: {:?}",
        pixel_at(&tile, 0.5, 0.5)
    );
    assert!(
        is_white(pixel_at(&tile, 0.1, 0.1)),
        "ink outside the annotation's rect"
    );
}

#[test]
fn a_filled_rectangle_fills_in_the_requested_colour() {
    let Some(engine) = shared_engine() else {
        return;
    };
    let tile = render_shape(
        engine,
        AnnotationKind::Square {
            line_width: 2.0,
            fill: Some(Color {
                r: 0.0,
                g: 0.0,
                b: 1.0,
            }),
        },
        "rect-filled",
    );
    let middle = pixel_at(&tile, 0.5, 0.5);
    assert!(
        middle[2] > 150 && middle[0] < 110,
        "interior should be blue, got {middle:?}"
    );
}

/// The border colour has to be the annotation's colour even when the
/// requested width is zero — that path falls back to a hairline, and an
/// earlier version left the stroke colour unset, so it came out black.
#[test]
fn a_zero_width_outline_still_draws_in_the_annotations_colour() {
    let Some(engine) = shared_engine() else {
        return;
    };
    let tile = render_shape(
        engine,
        AnnotationKind::Square {
            line_width: 0.0,
            fill: None,
        },
        "rect-hairline",
    );
    let border_px: Vec<[u8; 3]> = (0..40)
        .map(|i| pixel_at(&tile, 0.25 + (i as f32) * 0.0125, 0.2505))
        .collect();
    assert!(
        border_px.iter().any(|px| is_reddish(*px)),
        "hairline border wasn't drawn in red: {:?}",
        &border_px[..6]
    );
}

/// An ellipse leaves its corners empty — which is what distinguishes it
/// from the rectangle, and what a wrong Bézier would get wrong.
#[test]
fn a_circle_fills_its_middle_but_not_the_corners_of_its_box() {
    let Some(engine) = shared_engine() else {
        return;
    };
    let tile = render_shape(
        engine,
        AnnotationKind::Circle {
            line_width: 2.0,
            fill: Some(Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            }),
        },
        "circle-filled",
    );
    assert!(
        is_reddish(pixel_at(&tile, 0.5, 0.5)),
        "the ellipse didn't fill its centre: {:?}",
        pixel_at(&tile, 0.5, 0.5)
    );
    // Just inside the bounding box's top-left corner — outside the curve.
    assert!(
        is_white(pixel_at(&tile, 0.27, 0.27)),
        "the ellipse painted into the corner of its box, so it's a rectangle: {:?}",
        pixel_at(&tile, 0.27, 0.27)
    );
}

/// A border thicker than the box has no interior left; better an error
/// than an inverted rectangle.
#[test]
fn a_border_thicker_than_the_shape_is_rejected() {
    let mut doc = Document::from_bytes(&blank_page_pdf()).expect("fixture should parse");
    let result = add_annotation(
        &mut doc,
        0,
        NewAnnotation {
            rect: Rect {
                x0: 100.0,
                y0: 100.0,
                x1: 120.0,
                y1: 120.0,
            },
            color: Color::BLACK,
            kind: AnnotationKind::Square {
                line_width: 60.0,
                fill: None,
            },
            contents: None,
            opacity: 1.0,
        },
    );
    assert!(
        result.is_err(),
        "a 60pt border on a 20pt box should be refused"
    );
}
