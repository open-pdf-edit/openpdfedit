//! Pixel-level proof, through real PDFium, that a created checkbox
//! *looks* like a checkbox: an empty box when off, and a visibly ticked
//! box when its value is set.
//!
//! The bug this exists for: `create_field` gave every field a single
//! `/AP` `/N` stream. For a text field that is correct, but a checkbox's
//! `/AP` `/N` must be a *dictionary of appearance states* keyed by the
//! same names `/AS` and `/V` use. With one stream there is no "checked"
//! appearance to select, so the widget rendered as a static rectangle no
//! matter what its value was — reported as "a box did pop but its not a
//! checkbox".
//!
//! `created_field_through_pdfium.rs` already proves the field is
//! *structurally* recognized and fillable. That test passed the whole
//! time this bug existed, because the defect was purely visual. Hence
//! this one, which renders and counts ink.

use openpdfedit_doc::Document;
use openpdfedit_engine::{Engine, PdfiumEngine};
use openpdfedit_forms::{create_field, normalize_button_states, NewField, NewFieldKind};
use std::collections::HashMap;

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

fn minimal_pdf_bytes() -> Vec<u8> {
    use lopdf::{dictionary, Object};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
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

/// The checkbox's placement, in PDF points. Deliberately drawn as a wide
/// rectangle so the squaring behavior is exercised too.
const DRAG: [f32; 4] = [60.0, 600.0, 260.0, 640.0];

/// Counts dark pixels strictly *inside* the checkbox's square, excluding
/// a 3pt margin so the box's own border stroke doesn't dominate the
/// count — what's being measured is the tick, not the frame.
fn tick_ink(rgba: &[u8], width: u32, height: u32) -> usize {
    let scale = height as f64 / 792.0;
    // square_rect anchors at the drag's top-left and takes the smaller
    // side, so the field is (60, 600)..(100, 640).
    let margin = 4.0;
    let x0 = ((60.0 + margin) * scale) as u32;
    let x1 = ((100.0 - margin) * scale) as u32;
    let y0 = ((792.0 - 640.0 + margin) * scale) as u32;
    let y1 = ((792.0 - 600.0 - margin) * scale) as u32;

    let mut dark = 0;
    for y in y0..y1.min(height) {
        for x in x0..x1.min(width) {
            let i = ((y * width + x) * 4) as usize;
            if rgba[i] < 128 && rgba[i + 1] < 128 && rgba[i + 2] < 128 {
                dark += 1;
            }
        }
    }
    dark
}

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

#[test]
fn a_created_checkbox_renders_empty_when_off_and_ticked_when_checked() {
    let Ok(engine) = PdfiumEngine::new(dev_vendor_lib_dir().as_deref()) else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let off_path = dir.join(format!("openpdfedit-checkbox-off-{pid}.pdf"));
    let on_path = dir.join(format!("openpdfedit-checkbox-on-{pid}.pdf"));
    std::fs::write(&off_path, minimal_pdf_bytes()).expect("write");

    let mut doc = Document::open(&off_path).expect("doc open");
    create_field(
        &mut doc,
        NewField {
            page_index: 0,
            name: "agree".into(),
            rect: DRAG,
            kind: NewFieldKind::Checkbox,
        },
    )
    .expect("create checkbox");
    std::fs::write(&off_path, doc.save_incremental().expect("save")).expect("write");

    // --- unchecked ---
    let handle = engine.open(&off_path).expect("open");
    let off = engine.render_page(handle, 0, 800).expect("render");
    let off_ink = tick_ink(&off.rgba, off.width, off.height);
    dump("checkbox-off", &off.rgba, off.width, off.height);

    // --- check it through the same PDFium fill path the app uses ---
    let mut values = HashMap::new();
    values.insert("agree".to_string(), "true".to_string());
    engine.fill_form_fields(handle, &values).expect("fill");
    engine.save_document(handle, &on_path).expect("save");
    engine.close(handle);

    // The same repair the app applies after every PDFium form save: turn
    // the string `/AS (/Yes)` PDFium writes into the name `/AS /Yes` that
    // actually selects an appearance state. Without this the assertion
    // below fails with zero tick pixels — which is precisely the bug.
    let mut saved = lopdf::Document::load(&on_path).expect("reload");
    let repaired = normalize_button_states(&mut saved);
    assert_eq!(
        repaired, 2,
        "expected PDFium's string-valued /AS and /V to need repair"
    );
    saved.save(&on_path).expect("rewrite");

    let handle = engine.open(&on_path).expect("reopen");
    let on = engine.render_page(handle, 0, 800).expect("render");
    let on_ink = tick_ink(&on.rgba, on.width, on.height);
    dump("checkbox-on", &on.rgba, on.width, on.height);
    engine.close(handle);

    // An unchecked box's interior is empty (a few stray antialiased
    // border pixels are fine, a tick is not).
    assert!(
        off_ink < 40,
        "an unchecked checkbox should have an empty interior, found {off_ink} dark px"
    );
    // A checked one draws a tick. This is the assertion that fails
    // against the old single-`/AP`-stream implementation: with no
    // "checked" appearance state to select, both renders were identical.
    assert!(
        on_ink > 150,
        "a checked checkbox must render a visible tick; found only {on_ink} dark px \
         inside the box (unchecked had {off_ink})"
    );

    let _ = std::fs::remove_file(&off_path);
    let _ = std::fs::remove_file(&on_path);
}
