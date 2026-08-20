//! AcroForm field **creation** — the opposite direction from
//! `openpdfedit-engine`'s PDFium-backed form **fill** (see that crate's
//! module doc): fill mutates an *already-open* PDFium document in place;
//! this crate adds a brand-new field to the document's object graph via
//! plain `lopdf` operations, no PDFium involvement needed for the
//! creation itself.
//!
//! [`create_field`] builds a Widget annotation dictionary (the field
//! *is* its own annotation for a simple, non-grouped field — no
//! `/Kids`/parent-field indirection, matching the simplest, most common
//! real-world case) with the requested `/FT` (field type), registers it
//! on the target page's `/Annots` (via
//! `openpdfedit_doc::Document::append_annotation_ref` — the exact same
//! call `openpdfedit-annot` uses) and in the document's `/AcroForm`/
//! `/Fields` array (via the new
//! `Document::ensure_acroform_and_append_field`), and — for `Text`
//! fields, which need somewhere to look up a font for their `/DA` — also
//! merges a `/DR` (default resources) Helvetica font and top-level `/DA`
//! into `/AcroForm` if one isn't already there (via
//! `Document::merge_acroform_entries`), so the field's default appearance
//! string has something to resolve.
//!
//! No `/AP` appearance stream is built — the same simplification
//! `openpdfedit-engine`'s form-fill tests already rely on for checkboxes
//! (no `/AP`, just `/AS`), and here relying on PDFium's (or any other
//! conforming viewer's) live form-fill rendering to paint the field from
//! its `/DA`/`/V`/`/AS` state rather than a static appearance.
//!
//! ## Scope
//!
//! Two field kinds: `Text` and `Checkbox` — the two PLAN.md's own M4
//! fill-side acceptance criterion ("IRS/EU standard forms fill
//! correctly") already covers as reliable (see
//! `PdfiumEngine::fill_form_fields`'s doc — RadioButton fill is
//! best-effort there, and creating a *group* of radio widgets sharing
//! one parent field correctly is meaningfully more structure than a
//! single Text/Checkbox widget; ComboBox/ListBox aren't fillable via
//! this dependency version at all). Not implemented: radio groups,
//! combo/list boxes, and any auto-detect-fields-from-existing-content
//! tooling (PLAN.md's "form-field creation palette with auto-detect" —
//! this crate only creates a field where the caller explicitly places
//! one).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormsError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("field name must not be empty")]
    EmptyFieldName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewFieldKind {
    Text,
    Checkbox,
}

#[derive(Debug, Clone)]
pub struct NewField {
    pub page_index: u32,
    /// The field's `/T` (must be non-empty and, by convention, unique
    /// within the document — not enforced here, since AcroForm allows
    /// deliberately non-unique names for grouped fields, which this
    /// crate doesn't create).
    pub name: String,
    /// `[x0, y0, x1, y1]` in PDF page-space points.
    pub rect: [f32; 4],
    pub kind: NewFieldKind,
}

/// Creates `field` and registers it on its page and in the document's
/// `/AcroForm`. Does not save — call [`Document::save_incremental`]
/// afterwards, same as every other `openpdfedit-doc` mutation. Returns
/// the new field's object id.
pub fn create_field(doc: &mut Document, field: NewField) -> Result<ObjectId, FormsError> {
    if field.name.trim().is_empty() {
        return Err(FormsError::EmptyFieldName);
    }

    let rect = match field.kind {
        NewFieldKind::Text => field.rect,
        // A checkbox drawn from a freehand drag comes out whatever shape
        // the user's gesture was — typically a wide, short rectangle,
        // which reads as a text box, not a checkbox (reported: "a box
        // did pop but its not a checkbox"). Squaring it to the smaller
        // dimension, anchored at the drag's top-left, is what every
        // other PDF editor does and is what makes the control
        // recognisable.
        NewFieldKind::Checkbox => square_rect(field.rect),
    };
    let rect_array: Vec<Object> = rect.iter().map(|&v| (v as f64).into()).collect();
    let width = (rect[2] - rect[0]).abs().max(1.0) as f64;
    let height = (rect[3] - rect[1]).abs().max(1.0) as f64;

    let mut widget = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "T" => Object::string_literal(field.name.as_str()),
        "Rect" => rect_array,
        // Bit 3 (value 4): Print — the field shows up when the page is
        // printed, standard practice for a field meant to be filled in,
        // not an interactive-only artifact.
        "F" => 4,
        // Border/background hints, for viewers that regenerate
        // appearances themselves rather than using the streams below.
        "MK" => dictionary! {
            "BC" => vec![0.35.into(), 0.35.into(), 0.35.into()],
            "BG" => vec![0.95.into(), 0.95.into(), 0.95.into()],
        },
        // A 1pt solid border, so a viewer that synthesizes its own
        // appearance still draws the box.
        "BS" => dictionary! { "W" => 1, "S" => "S" },
    };

    // A widget with no `/AP` normal-appearance stream is drawn by
    // *nothing* — PDFium included. Viewers are only obliged to synthesize
    // an appearance from `/DA`/`/MK` when the document sets
    // `/AcroForm /NeedAppearances true`, and even then support varies.
    // Without this a newly added field was genuinely invisible on the
    // page (reported: "after adding, it does not appear on the pdf"),
    // even though it existed and was fillable.
    match field.kind {
        NewFieldKind::Text => {
            widget.set("FT", Object::Name(b"Tx".to_vec()));
            widget.set("V", Object::string_literal(""));
            widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
            ensure_default_resources(doc)?;

            let normal = appearance_stream(doc, width, height, checkbox_box_ops(width, height))?;
            widget.set("AP", dictionary! { "N" => Object::Reference(normal) });
        }
        NewFieldKind::Checkbox => {
            widget.set("FT", Object::Name(b"Btn".to_vec()));
            // The field starts unchecked. `/AS` selects which of the
            // `/AP` `/N` states below is actually drawn.
            widget.set("V", Object::Name(b"Off".to_vec()));
            widget.set("AS", Object::Name(b"Off".to_vec()));
            // A checkbox's caption font. `/ZaDb` (ZapfDingbats) with
            // caption `4` is the conventional check mark, and is what
            // viewers that regenerate appearances will look for.
            widget.set("DA", Object::string_literal("/ZaDb 0 Tf 0 g"));
            if let Some(mk) = widget
                .get_mut(b"MK")
                .ok()
                .and_then(|o| o.as_dict_mut().ok())
            {
                mk.set("CA", Object::string_literal("4"));
            }
            ensure_checkbox_resources(doc)?;

            // The real fix for "it's not a checkbox": a checkbox's `/AP`
            // `/N` is a *dictionary of appearance states*, not a single
            // stream. One entry per state, keyed by the same names `/AS`
            // and `/V` use. With only one stream (what this used to
            // emit) the widget can never show a tick no matter what its
            // value is — it is a static box that happens to be a form
            // field, which is exactly what was reported.
            let off = appearance_stream(doc, width, height, checkbox_box_ops(width, height))?;
            let on = appearance_stream(doc, width, height, checkbox_checked_ops(width, height))?;
            widget.set(
                "AP",
                dictionary! {
                    "N" => dictionary! {
                        "Off" => Object::Reference(off),
                        CHECKED_STATE => Object::Reference(on),
                    },
                },
            );
        }
    }

    let field_id = doc.add_object(Object::Dictionary(widget));
    // The text-field appearance stream draws the *box*, not its contents
    // — it is generated once, at creation, and knows nothing about
    // values typed later. Without this flag a filled field kept
    // rendering as an empty box (reported: "text field remains empty
    // after entering"), because viewers were free to keep using that
    // stale appearance. `NeedAppearances` tells any conforming viewer to
    // regenerate each field's appearance from its current `/V` and
    // `/DA`. Checkboxes don't need it — they ship both states above —
    // but it is a document-level flag and is harmless for them.
    doc.merge_acroform_entries(lopdf::dictionary! { "NeedAppearances" => true })?;
    doc.append_annotation_ref(field.page_index, field_id)?;
    doc.ensure_acroform_and_append_field(field_id)?;

    Ok(field_id)
}

/// The `/AS`//`/V` name for a checked checkbox. `Yes` is the conventional
/// choice and the one PDFium's own form layer defaults to.
const CHECKED_STATE: &str = "Yes";

/// Repairs checkbox/radio `/AS` and `/V` entries that were written as
/// *strings* where the spec requires *names*, returning how many entries
/// were corrected.
///
/// Run this over a file after saving through PDFium's writer. When
/// appearance streams are in use, `pdfium-render`'s
/// `PdfFormCheckboxField::set_checked` sets the value with
/// `FPDFAnnot_SetStringValue("AS", "/Yes")` — which emits the string
/// object `(/Yes)`, not the name `/Yes`. `/AS` selects an entry from the
/// widget's `/AP`/`/N` state dictionary *by name*, and a string matches
/// no key there, so the checkbox renders as though it were still off no
/// matter how many times it is ticked. Verified by rendering: the tick
/// was absent and the saved file contained `/AS(/Yes)/V(/Yes)`.
///
/// Only values that begin with `/` are rewritten — that leading slash is
/// the tell that a name was intended — so a text field whose value
/// legitimately starts with a slash is never touched, because this only
/// inspects button fields.
pub fn normalize_button_states(doc: &mut lopdf::Document) -> usize {
    fn name_from_slashed_string(object: &Object) -> Option<Object> {
        let Object::String(bytes, _) = object else {
            return None;
        };
        let rest = bytes.strip_prefix(b"/")?;
        (!rest.is_empty()).then(|| Object::Name(rest.to_vec()))
    }

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut repaired = 0;
    for id in ids {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
            continue;
        };
        // Button fields only: `/FT` may live on the widget itself or,
        // for a grouped control, on a parent this widget points at. The
        // `/AS` key alone is also a reliable tell, since only widgets
        // with appearance states have one.
        let is_button = dict
            .get(b"FT")
            .ok()
            .and_then(|o| o.as_name().ok())
            .is_some_and(|n| n == b"Btn")
            || dict.has(b"AS");
        if !is_button {
            continue;
        }

        let fixes: Vec<(&[u8], Object)> = [b"AS".as_slice(), b"V".as_slice()]
            .into_iter()
            .filter_map(|key| {
                let name = name_from_slashed_string(dict.get(key).ok()?)?;
                Some((key, name))
            })
            .collect();
        if fixes.is_empty() {
            continue;
        }
        if let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&id) {
            for (key, name) in fixes {
                dict.set(key.to_vec(), name);
                repaired += 1;
            }
        }
    }
    repaired
}

/// Shrinks `rect` to a square anchored at its top-left corner.
fn square_rect(rect: [f32; 4]) -> [f32; 4] {
    let x0 = rect[0].min(rect[2]);
    let y1 = rect[1].max(rect[3]);
    let side = (rect[2] - rect[0]).abs().min((rect[3] - rect[1]).abs());
    // A drag too small to have a meaningful size still has to produce a
    // clickable control, so floor it at a sensible default checkbox size.
    let side = side.max(12.0);
    [x0, y1 - side, x0 + side, y1]
}

/// The empty box every field state starts from: a light fill plus a
/// visible border, in the appearance stream's own 0-based coordinates.
fn checkbox_box_ops(width: f64, height: f64) -> Vec<Operation> {
    vec![
        // Very light fill so the field reads as an input area without
        // obscuring anything beneath it.
        Operation::new("g", vec![0.95.into()]),
        Operation::new("re", vec![0.into(), 0.into(), width.into(), height.into()]),
        Operation::new("f", vec![]),
        Operation::new("G", vec![0.35.into()]),
        Operation::new("w", vec![1.into()]),
        Operation::new(
            "re",
            vec![
                0.5.into(),
                0.5.into(),
                (width - 1.0).into(),
                (height - 1.0).into(),
            ],
        ),
        Operation::new("S", vec![]),
    ]
}

/// The box plus a tick. Drawn as two stroked line segments rather than
/// as ZapfDingbats character `4`, so the appearance needs no font
/// resource at all and cannot depend on a viewer having that font.
fn checkbox_checked_ops(width: f64, height: f64) -> Vec<Operation> {
    let mut ops = checkbox_box_ops(width, height);
    let inset = (width.min(height) * 0.22).max(1.5);
    let stroke = (width.min(height) * 0.14).max(1.0);
    ops.extend([
        Operation::new("G", vec![0.into()]),
        Operation::new("w", vec![stroke.into()]),
        // Round caps/joins keep the tick from looking like two
        // disconnected sticks at small sizes.
        Operation::new("J", vec![1.into()]),
        Operation::new("j", vec![1.into()]),
        Operation::new("m", vec![inset.into(), (height * 0.55).into()]),
        Operation::new("l", vec![(width * 0.42).into(), inset.into()]),
        Operation::new("l", vec![(width - inset).into(), (height - inset).into()]),
        Operation::new("S", vec![]),
    ]);
    ops
}

/// Wraps `operations` in a Form XObject sized `width`x`height` and adds
/// it to `doc`, returning its object id.
fn appearance_stream(
    doc: &mut Document,
    width: f64,
    height: f64,
    operations: Vec<Operation>,
) -> Result<ObjectId, FormsError> {
    let encoded = Content { operations }
        .encode()
        .map_err(|e| FormsError::Doc(DocError::Save(std::io::Error::other(e.to_string()))))?;
    Ok(doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), width.into(), height.into()],
            "Resources" => dictionary! {},
        },
        encoded,
    ))))
}

/// Makes a text field's `/DA` (`/Helv 0 Tf 0 g`) resolvable: registers
/// `/Helv` in `/AcroForm`/`/DR`/`/Font` and sets a document-level `/DA`
/// for viewers that fall back to it.
///
/// The font registration goes through
/// [`Document::ensure_acroform_font`], which merges into the nested
/// `/Font` dictionary rather than replacing `/DR` wholesale — see that
/// method's doc for why replacing it would break both mixed field types
/// and any pre-existing form in the document.
fn ensure_default_resources(doc: &mut Document) -> Result<(), DocError> {
    doc.ensure_acroform_font("Helv", "Helvetica")?;
    let mut entries = Dictionary::new();
    entries.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    doc.merge_acroform_entries(entries)?;
    Ok(())
}

/// Registers `/ZaDb` (ZapfDingbats) in `/AcroForm`/`/DR` so a checkbox's
/// `/DA` has a font to resolve. The tick this crate draws is vector
/// strokes, not a ZapfDingbats glyph, so this only matters for viewers
/// that discard the supplied `/AP` and synthesize their own appearance
/// from `/MK`/`/CA` — but those viewers exist, and a `/DA` naming a font
/// that `/DR` doesn't define is exactly how a checkbox ends up drawing
/// nothing.
///
/// ZapfDingbats is one of the 14 standard fonts, so no font program is
/// embedded or needed.
fn ensure_checkbox_resources(doc: &mut Document) -> Result<(), DocError> {
    doc.ensure_acroform_font("ZaDb", "ZapfDingbats")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object as O};

    fn minimal_pdf_bytes() -> Vec<u8> {
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
            O::Dictionary(dictionary! {
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
    fn create_text_field_registers_it_on_the_page_and_in_acroform() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");

        let field_id = create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "full_name".into(),
                rect: [50.0, 700.0, 250.0, 720.0],
                kind: NewFieldKind::Text,
            },
        )
        .expect("create_field should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots, &vec![O::Reference(field_id)]);

        let field = reopened.get_dictionary(field_id).unwrap();
        assert_eq!(field.get(b"FT").unwrap().as_name().unwrap(), b"Tx");
        assert_eq!(field.get(b"T").unwrap().as_str().unwrap(), b"full_name");

        let root_id = reopened
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let catalog = reopened.get_dictionary(root_id).unwrap();
        let acroform_id = catalog.get(b"AcroForm").unwrap().as_reference().unwrap();
        let acroform = reopened.get_dictionary(acroform_id).unwrap();
        let fields = acroform.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields, &vec![O::Reference(field_id)]);

        // Default resources for the field's /DA to resolve /Helv from.
        let dr = acroform.get(b"DR").unwrap().as_dict().unwrap();
        assert!(dr.get(b"Font").unwrap().as_dict().unwrap().has(b"Helv"));
    }

    #[test]
    fn create_checkbox_field_registers_it() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");

        let field_id = create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "agree".into(),
                rect: [50.0, 650.0, 65.0, 665.0],
                kind: NewFieldKind::Checkbox,
            },
        )
        .expect("create_field should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let field = reopened.get_dictionary(field_id).unwrap();
        assert_eq!(field.get(b"FT").unwrap().as_name().unwrap(), b"Btn");
        assert_eq!(field.get(b"AS").unwrap().as_name().unwrap(), b"Off");
    }

    /// The regression test for "a box did pop but its not a checkbox": a
    /// checkbox's `/AP` `/N` must be a *dictionary of appearance states*
    /// with both an off and an on entry. With a single stream (the old
    /// behavior) the widget can never draw a tick regardless of its
    /// value — it renders as a static box that happens to be a field.
    #[test]
    fn checkbox_has_both_on_and_off_appearance_states() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        let field_id = create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "agree".into(),
                rect: [50.0, 650.0, 70.0, 670.0],
                kind: NewFieldKind::Checkbox,
            },
        )
        .expect("create_field should succeed");
        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();

        let field = reopened.get_dictionary(field_id).unwrap();
        let ap = field.get(b"AP").unwrap().as_dict().unwrap();
        let normal = ap.get(b"N").unwrap().as_dict().unwrap();
        assert!(normal.has(b"Off"), "missing the unchecked appearance state");
        assert!(
            normal.has(CHECKED_STATE.as_bytes()),
            "missing the checked appearance state — this is what makes it a checkbox"
        );
        assert_eq!(
            field.get(b"AS").unwrap().as_name().unwrap(),
            b"Off",
            "a new checkbox starts unchecked"
        );

        // The on state must actually paint something the off state
        // doesn't, or both would render identically.
        let stream_len = |key: &[u8]| {
            let id = normal.get(key).unwrap().as_reference().unwrap();
            reopened
                .get_object(id)
                .unwrap()
                .as_stream()
                .unwrap()
                .content
                .len()
        };
        assert!(
            stream_len(CHECKED_STATE.as_bytes()) > stream_len(b"Off"),
            "the checked state must draw a tick on top of the box"
        );
    }

    /// A freehand drag produces whatever rectangle the gesture traced; a
    /// checkbox has to come out square or it reads as a text box.
    #[test]
    fn checkbox_rect_is_squared_from_the_drag() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        let field_id = create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "agree".into(),
                // A wide, short drag — the shape people actually make.
                rect: [50.0, 650.0, 250.0, 668.0],
                kind: NewFieldKind::Checkbox,
            },
        )
        .expect("create_field should succeed");

        let saved = doc.save_incremental().expect("save");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let rect = reopened
            .get_dictionary(field_id)
            .unwrap()
            .get(b"Rect")
            .unwrap()
            .as_array()
            .unwrap();
        let v: Vec<f64> = rect.iter().map(|o| o.as_float().unwrap() as f64).collect();
        let (w, h) = (v[2] - v[0], v[3] - v[1]);
        assert!(
            (w - h).abs() < 0.01,
            "expected a square checkbox, got {w}x{h}"
        );
        assert!(
            (h - 18.0).abs() < 0.01,
            "should take the smaller side, got {h}"
        );
    }

    /// Adding a checkbox after a text field must not drop the text
    /// field's `/Helv` from `/AcroForm`/`/DR`/`/Font` — a whole-key
    /// `/DR` overwrite would, leaving the text field's `/DA` naming a
    /// font the document no longer defines.
    #[test]
    fn mixed_field_types_keep_each_others_default_resources() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "name".into(),
                rect: [50.0, 700.0, 250.0, 720.0],
                kind: NewFieldKind::Text,
            },
        )
        .expect("text field");
        create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "agree".into(),
                rect: [50.0, 650.0, 68.0, 668.0],
                kind: NewFieldKind::Checkbox,
            },
        )
        .expect("checkbox");

        let saved = doc.save_incremental().expect("save");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let root_id = reopened
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        let acroform_id = reopened
            .get_dictionary(root_id)
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_reference()
            .unwrap();
        let acroform = reopened.get_dictionary(acroform_id).unwrap();
        let dr = acroform.get(b"DR").unwrap().as_dict().unwrap();
        let fonts = dr.get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.has(b"Helv"), "the text field's font was clobbered");
        assert!(fonts.has(b"ZaDb"), "the checkbox's font is missing");
    }

    #[test]
    fn empty_field_name_is_rejected() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        let result = create_field(
            &mut doc,
            NewField {
                page_index: 0,
                name: "  ".into(),
                rect: [0.0, 0.0, 10.0, 10.0],
                kind: NewFieldKind::Text,
            },
        );
        assert!(matches!(result, Err(FormsError::EmptyFieldName)));
    }

    #[test]
    fn out_of_range_page_index_returns_error_not_panic() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).expect("should parse");
        let result = create_field(
            &mut doc,
            NewField {
                page_index: 9,
                name: "x".into(),
                rect: [0.0, 0.0, 10.0, 10.0],
                kind: NewFieldKind::Text,
            },
        );
        assert!(result.is_err());
    }
}
