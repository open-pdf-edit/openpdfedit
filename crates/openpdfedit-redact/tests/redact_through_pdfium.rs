//! Cross-crate validation: builds a page with real, real-font, real-
//! position text ("SECRET DATA" next to "PUBLIC TEXT"), redacts a rect
//! covering only the first, and confirms — through real PDFium, not this
//! workspace's own reader — that (a) the redacted region no longer has
//! any extractable characters (the actual "true removal" proof: a
//! naive black-box-only redaction would still have them), (b) the
//! untouched text elsewhere on the page is unaffected, and (c) the
//! rendered pixels in the redacted region are solid black (the visual
//! side of the promise).

use openpdfedit_doc::Document;
use openpdfedit_engine::EngineHandle;
use openpdfedit_redact::Rect;

/// One engine for the whole test binary.
///
/// PDFium's global init is not safe to run concurrently across
/// independent bindings: a second engine in the same process takes the
/// whole run down with a segfault, which is what happened the moment a
/// second test here spawned its own. See `PdfiumEngine::new`'s doc —
/// one engine per process, shared.
fn shared_engine() -> Option<&'static EngineHandle> {
    static ENGINE: std::sync::OnceLock<Option<EngineHandle>> = std::sync::OnceLock::new();
    ENGINE
        .get_or_init(|| EngineHandle::spawn(dev_vendor_lib_dir()).ok())
        .as_ref()
}

fn dev_vendor_lib_dir() -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/openpdfedit-redact -> workspace root
    let workspace_root = manifest_dir.parent()?.parent()?;
    let dir = workspace_root.join(if cfg!(windows) {
        ".vendor/pdfium/bin"
    } else {
        ".vendor/pdfium/lib"
    });
    dir.exists().then_some(dir)
}

/// A one-page PDF with two separate text runs at known positions: a
/// "secret" line near the bottom-left (which will be redacted) and a
/// "public" line near the top-right (which must survive untouched).
fn two_text_runs_pdf_bytes() -> Vec<u8> {
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
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![50.0.into(), 50.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("SECRET DATA")]),
            Operation::new("ET", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![50.0.into(), 700.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("PUBLIC TEXT")]),
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
fn redact_page_truly_removes_text_and_leaves_the_rest_intact() {
    let Some(engine) = shared_engine() else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-redact-test-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, two_text_runs_pdf_bytes()).expect("should write temp file");

    // Baseline: PDFium finds real characters from *both* text runs.
    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should open the fixture");
    let before_boxes = engine
        .page_char_boxes(handle, 0)
        .expect("char boxes should succeed");
    assert!(
        before_boxes.len() >= 20,
        "both text runs (SECRET DATA + PUBLIC TEXT, ~23 chars) should be extractable before redaction, got {}",
        before_boxes.len()
    );
    engine.close(handle);

    // Redact a rect covering only the "SECRET DATA" line (bottom-left).
    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    let removed = openpdfedit_redact::redact_page(
        &mut doc,
        0,
        Rect {
            x0: 40.0,
            y0: 40.0,
            x1: 300.0,
            y1: 80.0,
        },
        [0.0, 0.0, 0.0],
    )
    .expect("redact_page should succeed");
    assert_eq!(removed, 1, "exactly the SECRET DATA Tj call");

    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the redacted bytes");

    let redacted_handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the redacted file");

    let after_boxes = engine
        .page_char_boxes(redacted_handle, 0)
        .expect("char boxes should succeed on the redacted page");

    // The true-removal proof: no character box may fall within (or even
    // meaningfully overlap) the redacted rectangle.
    let leaked: Vec<_> = after_boxes
        .iter()
        .filter(|b| b.left < 300.0 && b.right > 40.0 && b.bottom < 80.0 && b.top > 40.0)
        .collect();
    assert!(
        leaked.is_empty(),
        "no character from the redacted region may remain extractable, found: {leaked:?}"
    );

    // The untouched "PUBLIC TEXT" line must still be fully present —
    // redaction must not have collaterally deleted unrelated content.
    let public_region_boxes = after_boxes.iter().filter(|b| b.top > 690.0).count();
    assert!(
        public_region_boxes >= 10,
        "PUBLIC TEXT (11 chars) should be untouched, found {public_region_boxes} chars near it"
    );

    // Visual side of the promise: the redacted region renders as solid
    // black, not just "no text" but "nothing visible there at all."
    let tile = engine
        .render_page(redacted_handle, 0, 612)
        .expect("should render at 1:1 scale (612px wide == 612pt page)");
    // Page-space (100, 60) — well inside the redaction rect — maps to
    // pixel space by flipping y (PDF is bottom-up, image is top-down).
    let px = 100u32;
    let py = tile.height.saturating_sub(60);
    let idx = ((py * tile.width + px) * 4) as usize;
    assert!(
        idx + 3 < tile.rgba.len(),
        "sample pixel must be within the rendered tile"
    );
    let (r, g, b) = (tile.rgba[idx], tile.rgba[idx + 1], tile.rgba[idx + 2]);
    assert!(
        r < 20 && g < 20 && b < 20,
        "pixel inside the redacted region should be solid black, got rgb({r},{g},{b})"
    );

    engine.close(redacted_handle);
    let _ = std::fs::remove_file(&tmp_path);
}

/// A page whose text is inside a form XObject rather than in the page's
/// own content stream — an ordinary shape for anything produced by a
/// layout tool, a form filler, or a PDF/A converter.
///
/// The form is scaled by its `/Matrix` and again by the CTM, which is
/// what made the old unit-square placement test miss it: the square it
/// checked sat near the origin, two points across, while the form
/// itself covered the page.
fn text_inside_a_scaled_form() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1",
        "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });

    // Drawn in a half-size space, so the form's own `/Matrix` and the
    // page's `cm` both have to be applied to find it.
    let form_content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![50.0.into(), 50.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("SECRET DATA")]),
            Operation::new("ET", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![50.0.into(), 700.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("PUBLIC TEXT")]),
            Operation::new("ET", vec![]),
        ],
    };
    let form_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Matrix" => vec![0.5.into(), 0.into(), 0.into(), 0.5.into(), 0.into(), 0.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        },
        form_content.encode().unwrap(),
    ));

    let page_content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![2.into(), 0.into(), 0.into(), 2.into(), 0.into(), 0.into()],
            ),
            Operation::new("Do", vec!["Fm1".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        page_content.encode().unwrap(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "XObject" => dictionary! { "Fm1" => form_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// The leak this exists to close.
///
/// A form XObject was placed by the unit-square rule that belongs to
/// images, so its real position — `/BBox` through `/Matrix` through the
/// CTM — was never computed and the overlap test always said no.
/// Redaction dropped nothing and painted a black box, which is the
/// cover-up-not-removal failure this whole crate is written against,
/// reached by a route nobody had checked. PDFium, which does not care
/// what is drawn on top, read the text straight back out.
#[test]
fn text_inside_a_form_xobject_is_removed_not_covered() {
    let Some(engine) = shared_engine() else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-redact-form-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, text_inside_a_scaled_form()).expect("should write temp file");

    let handle = engine.open(&tmp_path).expect("PDFium should open the fixture");
    let before: String = engine
        .page_chars(handle, 0)
        .expect("chars should succeed")
        .iter()
        .collect();
    assert!(
        before.contains("SECRET DATA") && before.contains("PUBLIC TEXT"),
        "both runs must be readable before redaction, got {before:?}"
    );
    engine.close(handle);

    // The form's text sits at (50,50) in form space; `/Matrix` halves
    // it and the page's `cm` doubles it, so it lands back at (50,50) on
    // the page — while the unit square the old code tested is a 2-point
    // speck at the origin.
    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    let removed = openpdfedit_redact::redact_page(
        &mut doc,
        0,
        Rect { x0: 40.0, y0: 40.0, x1: 300.0, y1: 90.0 },
        [1.0, 1.0, 1.0],
    )
    .expect("redact_page should succeed");
    assert!(removed > 0, "a redaction that removes nothing is a black box, not a redaction");

    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the redacted bytes");

    let redacted = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the redacted file");
    let after: String = engine
        .page_chars(redacted, 0)
        .expect("chars should succeed")
        .iter()
        .collect();
    engine.close(redacted);

    assert!(
        !after.contains("SECRET"),
        "the redacted text is still extractable from inside the form: {after:?}"
    );
    assert!(
        after.contains("PUBLIC TEXT"),
        "the rest of the form must survive — redaction is not deletion: {after:?}"
    );
}

/// A scan: one image, the size of the page, and nothing else on it.
const SCAN_PIXELS: u32 = 100;

fn scanned_page_pdf_bytes() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let mut samples = Vec::new();
    for _ in 0..(SCAN_PIXELS * SCAN_PIXELS) {
        samples.extend_from_slice(&[200u8, 30, 30]);
    }
    let mut image = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => SCAN_PIXELS, "Height" => SCAN_PIXELS,
            "BitsPerComponent" => 8, "ColorSpace" => "DeviceRGB",
        },
        samples,
    );
    image.compress().expect("fixture should compress");
    let image_id = doc.add_object(Object::Stream(image));

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![612.into(), 0.into(), 0.into(), 792.into(), 0.into(), 0.into()],
            ),
            Operation::new("Do", vec!["Im1".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// The page's one image, decoded, read back out of a saved file.
fn scan_samples(bytes: &[u8]) -> Vec<u8> {
    let doc = Document::from_bytes(bytes).expect("should reparse");
    let resources = doc.page_resources(0).expect("page should have resources");
    let xobjects = match doc.resolve(resources.get(b"XObject").expect("XObject resources")) {
        lopdf::Object::Dictionary(d) => d.clone(),
        other => panic!("expected an XObject dictionary, got {other:?}"),
    };
    let id = xobjects
        .get(b"Im1")
        .expect("Im1 must still be there")
        .as_reference()
        .expect("Im1 must be a reference");
    doc.decoded_stream(id).expect("image should decode")
}

fn sample_at(samples: &[u8], x: usize, y: usize) -> [u8; 3] {
    let i = (y * SCAN_PIXELS as usize + x) * 3;
    [samples[i], samples[i + 1], samples[i + 2]]
}

/// Redacting a line of a scan used to delete the scan.
///
/// A page image is one operator painting one indivisible blob, so the
/// only removal available was dropping the whole `Do` — and since a
/// scan's image covers the page, every redaction overlapped it. Hiding
/// one address blanked the document. The same rule is what wiped every
/// pen mark from a page whose markup had been flattened into a
/// transparent overlay image: one box drawn anywhere, and the entire
/// overlay went with it.
///
/// So the pixels are cleared instead, which has to be true removal and
/// not a crop: the bytes that carried the redacted content have to be
/// gone from the file, not merely undrawn.
#[test]
fn redacting_part_of_a_scan_clears_those_pixels_and_keeps_the_rest() {
    let Some(engine) = shared_engine() else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-redact-scan-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, scanned_page_pdf_bytes()).expect("should write temp file");

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    openpdfedit_redact::redact_page(
        &mut doc,
        0,
        Rect { x0: 100.0, y0: 600.0, x1: 300.0, y1: 700.0 },
        [1.0, 1.0, 1.0],
    )
    .expect("redact_page should succeed");
    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the redacted bytes");

    // True removal, checked in the image's own bytes rather than in
    // what a renderer chooses to show.
    let samples = scan_samples(&saved);
    assert_eq!(
        sample_at(&samples, 30, 18),
        [255, 255, 255],
        "the redacted pixels must be gone from the image data, not covered up"
    );
    assert_eq!(
        sample_at(&samples, 80, 80),
        [200, 30, 30],
        "the rest of the scan must survive — this is the whole point"
    );

    // And the page still shows a page.
    let handle = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the redacted file");
    let tile = engine.render_page(handle, 0, 300).expect("should render");
    engine.close(handle);

    let px = |x: u32, y: u32| {
        let i = ((y * tile.width + x) * 4) as usize;
        [tile.rgba[i], tile.rgba[i + 1], tile.rgba[i + 2]]
    };
    let corner = px(tile.width - 10, tile.height - 10);
    assert!(
        corner[0] > 150 && corner[1] < 100,
        "the far corner of the scan must still be the scan, not a blank page: {corner:?}"
    );
}

/// A form inside a form, which is what a stamp, a letterhead or a
/// placed-PDF asset looks like once a layout tool has been through it.
///
/// The recursion has to carry the redaction rect down through both
/// `/Matrix` transforms and copy both objects on the way back up. Doing
/// only the outer one leaves the text exactly where it was.
fn text_inside_a_nested_form() -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1",
        "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });

    let inner_content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![100.0.into(), 100.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("SECRET DATA")]),
            Operation::new("ET", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
            Operation::new("Td", vec![100.0.into(), 1400.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("PUBLIC TEXT")]),
            Operation::new("ET", vec![]),
        ],
    };
    let inner_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 1224.into(), 1584.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        },
        inner_content.encode().unwrap(),
    ));

    // The outer form halves the inner one; the page's own `cm` doubles
    // the outer. "SECRET DATA" therefore starts at (100, 100) on the
    // page after two transforms neither of which is the identity.
    let outer_content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![0.5.into(), 0.into(), 0.into(), 0.5.into(), 0.into(), 0.into()],
            ),
            Operation::new("Do", vec!["Inner".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let outer_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "XObject" => dictionary! { "Inner" => inner_id } },
        },
        outer_content.encode().unwrap(),
    ));

    let page_content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![2.into(), 0.into(), 0.into(), 2.into(), 0.into(), 0.into()],
            ),
            Operation::new("Do", vec!["Outer".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        page_content.encode().unwrap(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "XObject" => dictionary! { "Outer" => outer_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn redaction_follows_a_form_inside_a_form() {
    let Some(engine) = shared_engine() else {
        eprintln!("skipping: PDFium not available (run scripts/fetch-pdfium.sh)");
        return;
    };

    let tmp_path = std::env::temp_dir().join(format!(
        "openpdfedit-redact-nested-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&tmp_path, text_inside_a_nested_form()).expect("should write temp file");

    let handle = engine.open(&tmp_path).expect("PDFium should open the fixture");
    let before: String = engine
        .page_chars(handle, 0)
        .expect("chars should succeed")
        .iter()
        .collect();
    assert!(
        before.contains("SECRET DATA") && before.contains("PUBLIC TEXT"),
        "both runs must be readable before redaction, got {before:?}"
    );
    engine.close(handle);

    let mut doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
    openpdfedit_redact::redact_page(
        &mut doc,
        0,
        Rect { x0: 90.0, y0: 90.0, x1: 350.0, y1: 140.0 },
        [1.0, 1.0, 1.0],
    )
    .expect("redact_page should succeed");
    let saved = doc.save_incremental().expect("save should succeed");
    std::fs::write(&tmp_path, &saved).expect("should overwrite with the redacted bytes");

    let redacted = engine
        .open(&tmp_path)
        .expect("PDFium should reopen the redacted file");
    let after: String = engine
        .page_chars(redacted, 0)
        .expect("chars should succeed")
        .iter()
        .collect();
    engine.close(redacted);

    assert!(
        !after.contains("SECRET"),
        "two forms deep is still on the page: {after:?}"
    );
    assert!(
        after.contains("PUBLIC TEXT"),
        "and the rest of it must survive: {after:?}"
    );
}
