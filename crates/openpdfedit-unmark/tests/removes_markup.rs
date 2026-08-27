//! What comes off a page and, more importantly, what does not.
//!
//! The three guards in this crate's module doc are each one honest
//! mistake away from emptying somebody's document, so each gets a test
//! that would catch it being dropped.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Object, Stream};
use openpdfedit_doc::Document;
use openpdfedit_unmark::remove_markup;

/// A 2×2 image, optionally with the alpha channel that marks it out as
/// something drawn over a page rather than the page itself.
fn image(doc: &mut lopdf::Document, masked: bool) -> Object {
    let mut dict = dictionary! {
        "Type" => "XObject", "Subtype" => "Image",
        "Width" => 2, "Height" => 2,
        "BitsPerComponent" => 8, "ColorSpace" => "DeviceRGB",
    };
    if masked {
        let mask = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => 2, "Height" => 2,
                "BitsPerComponent" => 8, "ColorSpace" => "DeviceGray",
            },
            vec![255u8; 4],
        )));
        dict.set("SMask", Object::Reference(mask));
    }
    Object::Reference(doc.add_object(Object::Stream(Stream::new(dict, vec![10u8; 2 * 2 * 3]))))
}

/// One US-Letter page, built in a single document so that everything
/// the page refers to actually lives in the file.
///
/// `build` is handed the document and the resource name the overlay
/// image will have, and returns the page's operators.
fn page_pdf(
    masked: Option<bool>,
    annots: fn(&mut lopdf::Document) -> Vec<Object>,
    build: fn() -> Vec<Operation>,
) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1",
        "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });

    let xobjects = match masked {
        Some(masked) => {
            let overlay = image(&mut doc, masked);
            dictionary! { "Ov1" => overlay }
        }
        None => Dictionary::new(),
    };
    let annotations = annots(&mut doc);

    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: build() }.encode().unwrap(),
    ));
    let mut page = dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font_id },
            "XObject" => xobjects,
        },
    };
    if !annotations.is_empty() {
        page.set("Annots", Object::Array(annotations));
    }
    let page_id = doc.add_object(page);
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

fn no_annotations(_: &mut lopdf::Document) -> Vec<Object> {
    Vec::new()
}

fn say_hello() -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
        Operation::new("Td", vec![50.0.into(), 700.0.into()]),
        Operation::new("Tj", vec![Object::string_literal("THE DOCUMENT")]),
        Operation::new("ET", vec![]),
    ]
}

fn draw_full_page(name: &str) -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![612.into(), 0.into(), 0.into(), 792.into(), 0.into(), 0.into()],
        ),
        Operation::new("Do", vec![name.into()]),
        Operation::new("Q", vec![]),
    ]
}

fn operators(bytes: &[u8]) -> Vec<String> {
    let doc = Document::from_bytes(bytes).expect("should reparse");
    let content = doc.page_content_bytes(0).expect("should read content");
    Content::decode(&content)
        .expect("should decode")
        .operations
        .into_iter()
        .map(|op| op.operator)
        .collect()
}

/// The case the tool exists for: pen strokes exported as one
/// transparent sheet laid over the page, with no annotation anywhere in
/// the file to delete.
#[test]
fn a_flattened_markup_layer_comes_off_and_the_document_stays() {
    let bytes = page_pdf(Some(true), no_annotations, || {
        let mut ops = say_hello();
        ops.extend(draw_full_page("Ov1"));
        ops
    });

    let mut doc = Document::from_bytes(&bytes).expect("should parse");
    let removed = remove_markup(&mut doc).expect("should succeed");
    assert_eq!(removed.layers, 1, "the overlay is the markup");
    assert_eq!(removed.annotations, 0, "there were none to find");

    let saved = doc.save_incremental().expect("should save");
    let ops = operators(&saved);
    assert!(!ops.contains(&"Do".to_string()), "the overlay must be gone: {ops:?}");
    assert!(ops.contains(&"Tj".to_string()), "the document under it must not be: {ops:?}");
}

/// Guard 3. A scan saved as a transparent PNG looks exactly like a
/// markup layer — full page, alpha channel — and is the entire
/// document. Removing it leaves a blank sheet.
#[test]
fn a_transparent_image_that_is_the_whole_page_is_kept() {
    let bytes = page_pdf(Some(true), no_annotations, || draw_full_page("Ov1"));

    let mut doc = Document::from_bytes(&bytes).expect("should parse");
    let removed = remove_markup(&mut doc).expect("should succeed");
    assert_eq!(
        removed.layers, 0,
        "with nothing else on the page, the image is the page"
    );
}

/// Guard 1. Content drawn across a page without transparency is
/// content: a scan, a background, a full-bleed photograph.
#[test]
fn a_full_page_image_without_a_soft_mask_is_kept() {
    let bytes = page_pdf(Some(false), no_annotations, || {
        let mut ops = say_hello();
        ops.extend(draw_full_page("Ov1"));
        ops
    });

    let mut doc = Document::from_bytes(&bytes).expect("should parse");
    let removed = remove_markup(&mut doc).expect("should succeed");
    assert_eq!(removed.layers, 0, "no alpha channel, no reason to call it an overlay");
}

/// Guard 2. A logo, a signature image, a photo with a cut-out
/// background — transparent, small, and part of the document.
#[test]
fn a_small_transparent_image_is_kept() {
    let bytes = page_pdf(Some(true), no_annotations, || {
        let mut ops = say_hello();
        ops.extend(vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![80.into(), 0.into(), 0.into(), 40.into(), 60.into(), 60.into()],
            ),
            Operation::new("Do", vec!["Ov1".into()]),
            Operation::new("Q", vec![]),
        ]);
        ops
    });

    let mut doc = Document::from_bytes(&bytes).expect("should parse");
    let removed = remove_markup(&mut doc).expect("should succeed");
    assert_eq!(removed.layers, 0, "a logo is not a markup layer");
}

/// Markup annotations go; the document's own machinery stays. A link
/// and a form field are in `/Annots` beside the highlights, and neither
/// is something a person drew on the page.
#[test]
fn markup_annotations_go_but_links_and_form_fields_stay() {
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: say_hello() }.encode().unwrap(),
    ));

    let rect = || vec![10.into(), 10.into(), 100.into(), 40.into()];
    let highlight = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Highlight", "Rect" => rect() });
    let ink = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Ink", "Rect" => rect() });
    let note = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "Rect" => rect() });
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => rect() });
    let widget = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "Rect" => rect(), "FT" => "Tx" });

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![
            highlight.into(), ink.into(), note.into(), link.into(), widget.into(),
        ],
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

    let mut doc = Document::from_bytes(&bytes).expect("should parse");
    let removed = remove_markup(&mut doc).expect("should succeed");
    assert_eq!(removed.annotations, 3, "highlight, ink and note");

    let saved = doc.save_incremental().expect("should save");
    let reparsed = Document::from_bytes(&saved).expect("should reparse");
    let left: Vec<Vec<u8>> = reparsed
        .page_annotation_refs(0)
        .expect("should read annots")
        .into_iter()
        .filter_map(|id| {
            reparsed
                .dictionary(id)
                .ok()?
                .get(b"Subtype")
                .ok()?
                .as_name()
                .ok()
                .map(<[u8]>::to_vec)
        })
        .collect();
    assert_eq!(
        left,
        vec![b"Link".to_vec(), b"Widget".to_vec()],
        "a link and a form field are not markup"
    );
}
