//! Flattening: turning annotations and filled form fields into ordinary
//! page content.
//!
//! What it's for: a marked-up contract sent back to the other side
//! should show the highlights and the signature to whoever opens it,
//! in any reader, without them being movable, hideable, or deletable
//! with a click. Filled form values likewise — a flattened form is a
//! document, not a form someone can retype.
//!
//! ## How an appearance stream becomes page content
//!
//! Every drawable annotation carries its own rendering as a Form
//! XObject under `/AP` `/N` — this is what a reader actually paints, and
//! reusing it is what makes a flattened highlight look identical to the
//! live one rather than being redrawn from the annotation's parameters
//! and coming out subtly different.
//!
//! Placing it correctly is the whole difficulty, and it is *not* "draw
//! it at `/Rect`". PDF 32000-1 §12.5.5 specifies an algorithm: transform
//! the appearance's `/BBox` by its `/Matrix`, take the bounding box of
//! that result, and compute the matrix that maps that box onto the
//! annotation's `/Rect`. Skipping it puts every rotated or
//! non-unit-matrix appearance in the wrong place and at the wrong scale
//! — and freehand ink and stamps are exactly the annotations that carry
//! interesting matrices.
//!
//! ## What is deliberately left alone
//!
//! - **Links.** They have no visible appearance and flattening one just
//!   destroys a working link.
//! - **Popups.** They are the note window for a parent markup
//!   annotation, never painted on the page. Flattening them would stamp
//!   a note window into the document; they're removed with their parent
//!   instead.
//! - **Hidden and NoView annotations.** The `/F` flags say a reader
//!   shouldn't paint them; baking them in makes them visible forever.
//! - **Anything with no `/AP` `/N`.** Without an appearance there is
//!   nothing to draw, and inventing one would mean re-implementing every
//!   annotation type's rendering.

use lopdf::{Object, ObjectId};
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlattenError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("failed to encode the flattened content: {0}")]
    ContentEncode(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlattenOptions {
    /// Bake markup — highlights, notes, ink, stamps, drawn signatures —
    /// into the page.
    pub annotations: bool,
    /// Bake filled form fields in, and remove the interactive form. A
    /// flattened form can be read but not refilled.
    pub form_fields: bool,
}

impl Default for FlattenOptions {
    fn default() -> Self {
        Self {
            annotations: true,
            form_fields: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlattenReport {
    /// Annotations drawn into the page and removed.
    pub flattened: usize,
    /// Annotations left interactive — a link, or something with no
    /// appearance to draw. See this module's header.
    pub skipped: usize,
    /// Popup windows removed alongside their parent markup annotation.
    pub popups_removed: usize,
}

/// One annotation that passed every check and is about to be drawn.
///
/// Gathered before any mutation begins: `add_page_overlay` and
/// `remove_annotation_ref` both rewrite the page dictionary, so deciding
/// and acting in one pass would have each annotation's decision made
/// against a page the previous one had already changed.
struct PendingFlatten {
    annotation_id: ObjectId,
    appearance_id: ObjectId,
    /// The annotation's `/Rect` — where it goes.
    rect: [f32; 4],
    /// The appearance stream's `/BBox` — its own coordinate extent.
    bbox: [f32; 4],
    /// The appearance stream's `/Matrix`.
    matrix: [f32; 6],
}

/// `/F` annotation flags (PDF 32000-1 table 165), as bit positions.
const FLAG_HIDDEN: i64 = 1 << 1;
const FLAG_NOVIEW: i64 = 1 << 5;

/// Flattens the whole document, returning what happened.
pub fn flatten(
    doc: &mut Document,
    options: &FlattenOptions,
) -> Result<FlattenReport, FlattenError> {
    let mut report = FlattenReport::default();
    if !options.annotations && !options.form_fields {
        return Ok(report);
    }

    for page_index in 0..doc.page_count()? {
        flatten_page(doc, page_index, options, &mut report)?;
    }

    // Only once every page's widgets are gone: an `/AcroForm` describing
    // fields whose widgets no longer exist makes some readers draw empty
    // field boxes over the values just baked in.
    if options.form_fields && report.flattened > 0 {
        doc.remove_acroform();
    }
    Ok(report)
}

fn flatten_page(
    doc: &mut Document,
    page_index: u32,
    options: &FlattenOptions,
    report: &mut FlattenReport,
) -> Result<(), FlattenError> {
    let annotation_ids = doc.page_annotation_refs(page_index)?;
    if annotation_ids.is_empty() {
        return Ok(());
    }

    // Decided up front, against the document as it stands, so the
    // decisions can't be affected by the mutations made below.
    let mut to_draw: Vec<PendingFlatten> = Vec::new();
    let mut to_remove: Vec<ObjectId> = Vec::new();

    for annotation_id in &annotation_ids {
        let Ok(annotation) = doc.dictionary(*annotation_id) else {
            report.skipped += 1;
            continue;
        };

        let subtype = annotation
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            .unwrap_or(b"");
        let is_widget = subtype == b"Widget";

        // A Popup is removed with its parent rather than drawn; counted
        // separately because it isn't a markup annotation that was
        // "skipped", it's UI chrome that shouldn't have been in the
        // output either way.
        if subtype == b"Popup" {
            if options.annotations {
                to_remove.push(*annotation_id);
                report.popups_removed += 1;
            }
            continue;
        }
        // Links carry no appearance; flattening one only destroys it.
        if subtype == b"Link" {
            report.skipped += 1;
            continue;
        }
        if is_widget && !options.form_fields {
            continue;
        }
        if !is_widget && !options.annotations {
            continue;
        }

        let flags = annotation
            .get(b"F")
            .ok()
            .and_then(|f| f.as_i64().ok())
            .unwrap_or(0);
        if flags & (FLAG_HIDDEN | FLAG_NOVIEW) != 0 {
            report.skipped += 1;
            continue;
        }

        let Some(rect) = rect_from(doc, annotation.get(b"Rect").ok()) else {
            report.skipped += 1;
            continue;
        };
        let Some(appearance_id) = normal_appearance(doc, *annotation_id) else {
            report.skipped += 1;
            continue;
        };
        let Ok(appearance) = doc.dictionary_or_stream_dict(appearance_id) else {
            report.skipped += 1;
            continue;
        };
        let Some(bbox) = rect_from(doc, appearance.get(b"BBox").ok()) else {
            // A Form XObject without a /BBox is malformed; drawing it
            // would mean guessing its extent.
            report.skipped += 1;
            continue;
        };
        let matrix = matrix_from(doc, appearance.get(b"Matrix").ok());

        to_draw.push(PendingFlatten {
            annotation_id: *annotation_id,
            appearance_id,
            rect,
            bbox,
            matrix,
        });
    }

    for pending in to_draw {
        let PendingFlatten {
            annotation_id,
            appearance_id,
            rect,
            bbox,
            matrix,
        } = pending;
        // `merge_page_resource` returns the name actually used, picking a
        // numbered variant if `OPEFlat` is already taken by a different
        // object — so the `Do` below always names the right XObject.
        let resource_name =
            doc.merge_page_resource(page_index, "XObject", "OPEFlat", appearance_id)?;
        let content = draw_xobject(&resource_name, place_appearance(rect, bbox, matrix))?;
        // Appends *after* the page's own content (so the markup lands
        // where the reader was painting it a moment ago) and brackets
        // that content in `q`/`Q` — which matters because plenty of real
        // files leave a `q` unclosed, and an appended stream would then
        // render under a stray clip path, i.e. invisibly.
        doc.wrap_and_append_page_content(page_index, &content)?;
        doc.remove_annotation_ref(page_index, annotation_id)?;
        report.flattened += 1;
    }

    for popup_id in to_remove {
        // A popup whose parent was already unlinked may no longer be in
        // `/Annots`; that's success, not an error.
        let _ = doc.remove_annotation_ref(page_index, popup_id);
    }

    Ok(())
}

/// The `/AP` `/N` appearance stream's object id.
///
/// `/N` is either the stream itself or, for an annotation with several
/// appearance states (a checkbox's on and off, a radio button's each
/// value), a dictionary keyed by state name. In that case `/AS` names
/// which one is current — and picking the wrong one flattens a ticked
/// checkbox as unticked, which is a silent, plausible-looking data loss.
fn normal_appearance(doc: &Document, annotation_id: ObjectId) -> Option<ObjectId> {
    let annotation = doc.dictionary(annotation_id).ok()?;
    let appearances = doc.resolve(annotation.get(b"AP").ok()?).as_dict().ok()?;
    let normal = appearances.get(b"N").ok()?;

    match normal {
        Object::Reference(id) => {
            // Could be the stream directly, or a state dictionary stored
            // indirectly.
            match doc.dictionary_or_stream_dict(*id) {
                Ok(_) if is_form_xobject(doc, *id) => Some(*id),
                _ => state_appearance(doc, annotation, *id),
            }
        }
        Object::Dictionary(_) => {
            let state = annotation.get(b"AS").ok()?.as_name().ok()?;
            let states = doc.resolve(normal).as_dict().ok()?;
            states.get(state).and_then(Object::as_reference).ok()
        }
        _ => None,
    }
}

fn state_appearance(
    doc: &Document,
    annotation: &lopdf::Dictionary,
    states_id: ObjectId,
) -> Option<ObjectId> {
    let state = annotation.get(b"AS").ok()?.as_name().ok()?;
    let states = doc.dictionary(states_id).ok()?;
    states.get(state).and_then(Object::as_reference).ok()
}

/// A Form XObject declares `/Subtype /Form`. Used to tell an appearance
/// stream apart from a dictionary of appearance states, which is the one
/// ambiguity in `/AP` `/N`.
fn is_form_xobject(doc: &Document, id: ObjectId) -> bool {
    doc.dictionary_or_stream_dict(id)
        .ok()
        .and_then(|dict| dict.get(b"Subtype").ok()?.as_name().ok())
        .is_some_and(|subtype| subtype == b"Form")
}

fn rect_from(doc: &Document, object: Option<&Object>) -> Option<[f32; 4]> {
    let array = doc.resolve(object?).as_array().ok()?;
    if array.len() != 4 {
        return None;
    }
    let mut values = [0.0f32; 4];
    for (slot, item) in values.iter_mut().zip(array) {
        *slot = doc.resolve(item).as_float().ok()?;
    }
    let [x0, y0, x1, y1] = values;
    // The spec only requires the corners to be opposite, not ordered.
    Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
}

fn matrix_from(doc: &Document, object: Option<&Object>) -> [f32; 6] {
    const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let Some(object) = object else {
        return IDENTITY;
    };
    let Ok(array) = doc.resolve(object).as_array() else {
        return IDENTITY;
    };
    if array.len() != 6 {
        return IDENTITY;
    }
    let mut values = IDENTITY;
    for (slot, item) in values.iter_mut().zip(array) {
        match doc.resolve(item).as_float() {
            Ok(value) => *slot = value,
            Err(_) => return IDENTITY,
        }
    }
    values
}

/// The matrix that places an appearance stream inside an annotation's
/// rectangle, per PDF 32000-1 §12.5.5.
///
/// Returned as `[a, b, c, d, e, f]`, ready for a `cm` operator. The
/// appearance's own `/Matrix` is *not* included: the reader applies that
/// when it executes the Form XObject, so including it here would apply
/// it twice.
pub fn place_appearance(rect: [f32; 4], bbox: [f32; 4], matrix: [f32; 6]) -> [f32; 6] {
    // Step 1: transform the four corners of /BBox by /Matrix and take
    // the bounding box of the result. Corners, not just two of them —
    // a rotation moves all four differently.
    let [a, b, c, d, e, f] = matrix;
    let corners = [
        (bbox[0], bbox[1]),
        (bbox[2], bbox[1]),
        (bbox[2], bbox[3]),
        (bbox[0], bbox[3]),
    ];
    let transformed: Vec<(f32, f32)> = corners
        .iter()
        .map(|(x, y)| (a * x + c * y + e, b * x + d * y + f))
        .collect();

    let min_x = transformed
        .iter()
        .map(|p| p.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = transformed
        .iter()
        .map(|p| p.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = transformed
        .iter()
        .map(|p| p.1)
        .fold(f32::INFINITY, f32::min);
    let max_y = transformed
        .iter()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max);

    // Step 2: the scale that maps that box onto /Rect. A degenerate box
    // (zero width or height, which malformed files do produce) would
    // divide by zero; the spec's own wording is to use 1 there.
    let transformed_width = max_x - min_x;
    let transformed_height = max_y - min_y;
    let scale_x = if transformed_width.abs() < f32::EPSILON {
        1.0
    } else {
        (rect[2] - rect[0]) / transformed_width
    };
    let scale_y = if transformed_height.abs() < f32::EPSILON {
        1.0
    } else {
        (rect[3] - rect[1]) / transformed_height
    };

    [
        scale_x,
        0.0,
        0.0,
        scale_y,
        rect[0] - min_x * scale_x,
        rect[1] - min_y * scale_y,
    ]
}

fn draw_xobject(resource_name: &str, matrix: [f32; 6]) -> Result<Vec<u8>, FlattenError> {
    use lopdf::content::{Content, Operation};

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix.map(Object::Real).to_vec()),
            Operation::new("Do", vec![Object::Name(resource_name.into())]),
            Operation::new("Q", vec![]),
        ],
    };
    content
        .encode()
        .map_err(|e| FlattenError::ContentEncode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: [f32; 6], expected: [f32; 6]) {
        for (a, e) in actual.iter().zip(expected.iter()) {
            assert!(
                (a - e).abs() < 0.001,
                "expected {expected:?}, got {actual:?}"
            );
        }
    }

    const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    #[test]
    fn an_identity_appearance_at_the_origin_is_translated_to_the_rect() {
        let matrix = place_appearance(
            [100.0, 200.0, 150.0, 220.0],
            [0.0, 0.0, 50.0, 20.0],
            IDENTITY,
        );
        approx(matrix, [1.0, 0.0, 0.0, 1.0, 100.0, 200.0]);
    }

    #[test]
    fn a_bbox_larger_than_the_rect_is_scaled_down() {
        let matrix = place_appearance([0.0, 0.0, 50.0, 50.0], [0.0, 0.0, 100.0, 200.0], IDENTITY);
        approx(matrix, [0.5, 0.0, 0.0, 0.25, 0.0, 0.0]);
    }

    /// A `/BBox` that doesn't start at the origin has to be shifted, not
    /// just scaled — otherwise the appearance lands offset by wherever
    /// its box happened to begin.
    #[test]
    fn a_bbox_offset_from_the_origin_is_shifted_into_place() {
        let matrix = place_appearance(
            [10.0, 10.0, 30.0, 30.0],
            [100.0, 100.0, 120.0, 120.0],
            IDENTITY,
        );
        approx(matrix, [1.0, 0.0, 0.0, 1.0, -90.0, -90.0]);
    }

    /// The case the naive "draw it at /Rect" approach gets wrong: a
    /// quarter-turn `/Matrix` swaps the appearance's width and height,
    /// so the scale has to be computed against the *transformed* box.
    #[test]
    fn a_rotated_matrix_is_measured_after_rotation() {
        // 90 degrees counter-clockwise: (x, y) -> (-y, x).
        let rotate = [0.0, 1.0, -1.0, 0.0, 0.0, 0.0];
        // A 100x20 box becomes 20 wide and 100 tall once rotated.
        let matrix = place_appearance([0.0, 0.0, 40.0, 200.0], [0.0, 0.0, 100.0, 20.0], rotate);
        // 40 / 20 = 2 across, 200 / 100 = 2 up.
        approx(matrix, [2.0, 0.0, 0.0, 2.0, 40.0, 0.0]);
    }

    /// A malformed appearance with a zero-width box must not produce
    /// infinities that poison every coordinate downstream.
    #[test]
    fn a_degenerate_bbox_does_not_divide_by_zero() {
        let matrix = place_appearance([0.0, 0.0, 10.0, 10.0], [5.0, 0.0, 5.0, 10.0], IDENTITY);
        assert!(matrix.iter().all(|v| v.is_finite()), "{matrix:?}");
    }

    #[test]
    fn default_options_flatten_markup_but_not_form_fields() {
        let options = FlattenOptions::default();
        assert!(options.annotations);
        assert!(
            !options.form_fields,
            "flattening a form by default would silently make it unfillable"
        );
    }
}
