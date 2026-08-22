//! Flattening is only correct if the flattened page renders the *same*
//! as the page with the live annotation on it. A structural check can't
//! see the difference between "drawn in the right place" and "drawn 40
//! points to the left at half scale", which is exactly what skipping the
//! §12.5.5 placement algorithm produces.
//!
//! So each test renders twice — before and after — and compares.

use std::sync::{Arc, OnceLock};

use lopdf::{dictionary, Object, Stream};
use openpdfedit_doc::Document;
use openpdfedit_engine::{EngineHandle, RenderedTile};
use openpdfedit_flatten::{flatten, FlattenOptions};

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

fn render(engine: &EngineHandle, pdf: &[u8], tag: &str) -> Arc<RenderedTile> {
    let path = std::env::temp_dir().join(format!(
        "openpdfedit-flatten-test-{}-{tag}.pdf",
        std::process::id()
    ));
    std::fs::write(&path, pdf).expect("should write temp file");
    let handle = engine.open(&path).expect("PDFium should open the file");
    let tile = engine
        .render_page(handle, 0, 300)
        .expect("PDFium should render the page");
    engine.close(handle);
    let _ = std::fs::remove_file(&path);
    tile
}

/// Fraction of pixels that differ by more than a small tolerance.
/// Anti-aliasing means two renders of "the same" thing are never
/// bit-identical, so this measures how much of the page changed rather
/// than whether anything did.
fn difference_fraction(a: &RenderedTile, b: &RenderedTile) -> f32 {
    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
    let differing = a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0.iter())
        .filter(|(pa, pb)| (0..3).any(|i| (pa[i] as i32 - pb[i] as i32).abs() > 12))
        .count();
    differing as f32 / (a.width * a.height) as f32
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

/// One blank Letter page carrying one square annotation whose appearance
/// stream paints a solid block, placed at `rect` with the given `/BBox`
/// and `/Matrix`.
fn page_with_annotation(rect: [f32; 4], bbox: [f32; 4], matrix: Option<[f32; 6]>) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    // The appearance paints its whole /BBox solid red.
    let appearance_content = format!(
        "1 0 0 rg\n{} {} {} {} re\nf\n",
        bbox[0],
        bbox[1],
        bbox[2] - bbox[0],
        bbox[3] - bbox[1]
    );
    let mut appearance_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => bbox.map(Object::Real).to_vec(),
        "Resources" => dictionary! {},
    };
    if let Some(matrix) = matrix {
        appearance_dict.set("Matrix", matrix.map(Object::Real).to_vec());
    }
    let appearance_id = doc.add_object(Stream::new(
        appearance_dict,
        appearance_content.into_bytes(),
    ));

    let annotation_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => rect.map(Object::Real).to_vec(),
        "F" => 4, // Print
        "AP" => dictionary! { "N" => appearance_id },
    });

    let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![annotation_id.into()],
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

#[test]
fn a_flattened_annotation_renders_where_the_live_one_did() {
    let Some(engine) = shared_engine() else {
        return;
    };

    let base = page_with_annotation([100.0, 500.0, 300.0, 600.0], [0.0, 0.0, 200.0, 100.0], None);
    let before = render(engine, &base, "identity-before");
    assert!(
        ink_in(&before, (0.0, 0.0, 1.0, 1.0)) > 0,
        "fixture drew nothing"
    );

    let mut doc = Document::from_bytes(&base).expect("should parse");
    let report = flatten(&mut doc, &FlattenOptions::default()).expect("flatten should succeed");
    assert_eq!(report.flattened, 1);
    assert_eq!(report.skipped, 0);
    let saved = doc.save_incremental().expect("save should succeed");

    let after = render(engine, &saved, "identity-after");
    let difference = difference_fraction(&before, &after);
    assert!(
        difference < 0.001,
        "the flattened page differs from the original on {:.2}% of its pixels",
        difference * 100.0
    );
}

/// The case that separates a real implementation from "draw it at
/// /Rect": a quarter-turn `/Matrix` means the appearance's own width and
/// height are swapped before it's fitted, so a naive placement puts it
/// at the wrong scale.
#[test]
fn a_rotated_appearance_lands_in_the_right_place() {
    let Some(engine) = shared_engine() else {
        return;
    };

    let rotate = [0.0, 1.0, -1.0, 0.0, 0.0, 0.0];
    let base = page_with_annotation(
        [200.0, 300.0, 260.0, 500.0],
        [0.0, 0.0, 200.0, 60.0],
        Some(rotate),
    );
    let before = render(engine, &base, "rotated-before");
    assert!(
        ink_in(&before, (0.0, 0.0, 1.0, 1.0)) > 0,
        "fixture drew nothing"
    );

    let mut doc = Document::from_bytes(&base).expect("should parse");
    flatten(&mut doc, &FlattenOptions::default()).expect("flatten should succeed");
    let saved = doc.save_incremental().expect("save should succeed");

    let after = render(engine, &saved, "rotated-after");
    let difference = difference_fraction(&before, &after);
    assert!(
        difference < 0.001,
        "a rotated appearance moved when flattened: {:.2}% of pixels differ",
        difference * 100.0
    );
}

/// The point of flattening: the markup is now page content, so deleting
/// the annotation afterwards changes nothing.
#[test]
fn flattened_markup_survives_because_it_is_no_longer_an_annotation() {
    let Some(engine) = shared_engine() else {
        return;
    };

    let base = page_with_annotation([80.0, 400.0, 380.0, 550.0], [0.0, 0.0, 300.0, 150.0], None);
    let mut doc = Document::from_bytes(&base).expect("should parse");
    flatten(&mut doc, &FlattenOptions::default()).expect("flatten should succeed");
    let saved = doc.save_incremental().expect("save should succeed");

    // No annotations left to remove or hide.
    let reloaded = Document::from_bytes(&saved).expect("should re-parse");
    assert!(
        reloaded
            .page_annotation_refs(0)
            .expect("annots should read")
            .is_empty(),
        "the annotation is still interactive after flattening"
    );

    // ...and the ink is still on the page.
    let after = render(engine, &saved, "survives");
    assert!(
        ink_in(&after, (0.0, 0.0, 1.0, 1.0)) > 0,
        "flattening removed the annotation and its ink with it"
    );
}

/// Hidden annotations must not be baked in — the `/F` flags say a reader
/// shouldn't paint them, and flattening one makes it visible forever.
#[test]
fn a_hidden_annotation_is_skipped_rather_than_made_permanent() {
    let base = {
        let mut doc = lopdf::Document::load_mem(&page_with_annotation(
            [10.0, 10.0, 50.0, 50.0],
            [0.0, 0.0, 40.0, 40.0],
            None,
        ))
        .expect("fixture should parse");
        let annotation_id = *doc
            .objects
            .iter()
            .find(|(_, obj)| {
                matches!(obj.as_dict(), Ok(d) if matches!(d.get(b"Subtype"), Ok(Object::Name(s)) if s == b"Square"))
            })
            .expect("fixture has a square annotation")
            .0;
        doc.get_object_mut(annotation_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            // Hidden (bit 2) | Print.
            .set("F", 2 | 4);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    };

    let mut doc = Document::from_bytes(&base).expect("should parse");
    let report = flatten(&mut doc, &FlattenOptions::default()).expect("flatten should succeed");
    assert_eq!(report.flattened, 0);
    assert_eq!(report.skipped, 1);
    assert!(
        !doc.page_annotation_refs(0).unwrap().is_empty(),
        "a hidden annotation was removed as though it had been drawn"
    );
}
