//! Annotation model and PDF appearance-stream generation.
//!
//! Every annotation this crate creates is a real PDF `/Annot` object with
//! its own appearance stream (a Form XObject under `/AP /N`), not an
//! overlay the UI merely draws and forgets — any conforming PDF viewer
//! (Acrobat, Preview, pdf.js) renders it correctly with no knowledge of
//! openpdfedit. That's the pattern pdf.js's own `AnnotationEditorLayer`
//! uses (Apache-2.0 — see `docs/research/04-oss-borrow-map.md`), and it's
//! the only one that survives round-tripping through other tools.
//!
//! Each appearance stream carries its own self-contained `/Resources`
//! (fonts, `ExtGState` for translucency) rather than reaching into the
//! page's shared resources — that keeps annotation creation a pure
//! "add new objects, append one reference" operation with zero risk of
//! colliding with or corrupting whatever resource names the original
//! page already used.
//!
//! Saving goes through `openpdfedit_doc::Document::save_incremental` —
//! this crate never writes bytes directly.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnnotError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("annotation has no geometry (empty quad/stroke list)")]
    EmptyGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub const YELLOW: Color = Color {
        r: 1.0,
        g: 0.92,
        b: 0.23,
    };
    pub const RED: Color = Color {
        r: 0.96,
        g: 0.26,
        b: 0.21,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    fn as_pdf_array(self) -> Vec<Object> {
        vec![self.r.into(), self.g.into(), self.b.into()]
    }
}

/// Axis-aligned rectangle in PDF page-space points (origin bottom-left),
/// matching `/Rect` and Form XObject `/BBox` semantics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    fn as_pdf_array(self) -> Vec<Object> {
        vec![
            self.x0.into(),
            self.y0.into(),
            self.x1.into(),
            self.y1.into(),
        ]
    }

    /// The 8-number `/QuadPoints` entry for this rect, in the PDF-spec
    /// order (ISO 32000-1 §12.5.6.10): top-left, top-right, bottom-left,
    /// bottom-right — note bottom-left before bottom-right, which is the
    /// one non-obvious part of this ordering.
    fn as_quad_points(self) -> [f32; 8] {
        [
            self.x0, self.y1, // top-left
            self.x1, self.y1, // top-right
            self.x0, self.y0, // bottom-left
            self.x1, self.y0, // bottom-right
        ]
    }
}

/// One free-hand stroke: a sequence of points connected by straight
/// segments (the UI is responsible for any smoothing before it gets here).
pub type Stroke = Vec<(f32, f32)>;

#[derive(Debug, Clone)]
pub enum AnnotationKind {
    /// Translucent fill over one or more quads — typically one quad per
    /// line of highlighted text.
    Highlight {
        quads: Vec<Rect>,
    },
    Underline {
        quads: Vec<Rect>,
    },
    StrikeOut {
        quads: Vec<Rect>,
    },
    /// A text box. `rect` on the containing [`NewAnnotation`] is both the
    /// annotation's bounds and the text's wrap width.
    FreeText {
        text: String,
        font_size: f32,
    },
    /// Free-hand drawing; `rect` should be the bounding box of all strokes.
    Ink {
        strokes: Vec<Stroke>,
    },
    /// A rectangle outline (PDF `/Square`). Drawn as a stroke inset by
    /// half the line width so the ink stays inside `rect` — a stroke
    /// centred on the boundary would spill half its width outside the
    /// annotation's own bounds, which readers clip.
    Square {
        /// Border thickness in points.
        line_width: f32,
        /// Interior fill, or `None` for outline only.
        fill: Option<Color>,
    },
    /// An ellipse inscribed in `rect` (PDF `/Circle`), drawn from four
    /// Bézier arcs. Same inset treatment as [`AnnotationKind::Square`].
    Circle {
        line_width: f32,
        fill: Option<Color>,
    },
}

pub struct NewAnnotation {
    pub rect: Rect,
    pub color: Color,
    pub kind: AnnotationKind,
    /// The comment/note text (a highlight's attached comment, a
    /// free-text's body if different from what's drawn, etc.). Stored as
    /// `/Contents`; optional because most markup annotations don't need it.
    pub contents: Option<String>,
    /// Opacity in `0.0..=1.0`, stored as both the annotation's `/CA` and
    /// baked into the appearance stream's `ExtGState`, so it renders
    /// consistently whether a viewer trusts the appearance stream or
    /// re-derives it from the annotation model.
    pub opacity: f32,
}

fn subtype_name(kind: &AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Highlight { .. } => "Highlight",
        AnnotationKind::Underline { .. } => "Underline",
        AnnotationKind::StrikeOut { .. } => "StrikeOut",
        AnnotationKind::FreeText { .. } => "FreeText",
        AnnotationKind::Ink { .. } => "Ink",
        AnnotationKind::Square { .. } => "Square",
        AnnotationKind::Circle { .. } => "Circle",
    }
}

/// Adds `annotation` to `doc` at `page_index` (0-based): builds its
/// appearance stream and dictionary, adds both as new objects, and links
/// the annotation into the page's `/Annots` array. Does **not** save —
/// call [`Document::save_incremental`] when ready to persist (batching
/// several `add_annotation` calls into one save produces one revision
/// instead of one per annotation, which is usually what a UI wants).
pub fn add_annotation(
    doc: &mut Document,
    page_index: u32,
    annotation: NewAnnotation,
) -> Result<ObjectId, AnnotError> {
    let NewAnnotation {
        rect,
        color,
        kind,
        contents,
        opacity,
    } = annotation;

    let (appearance_content, extra_resources) = build_appearance(&kind, rect, color)?;

    let mut resources = dictionary! {
        "ExtGState" => dictionary! {
            "GS0" => dictionary! { "ca" => opacity, "CA" => opacity, "BM" => "Normal" },
        },
    };
    for (key, value) in extra_resources {
        resources.set(key, value);
    }

    let appearance_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => rect.as_pdf_array(),
            "Resources" => resources,
        },
        appearance_content
            .encode()
            .expect("content stream encoding cannot fail for the fixed operator set built here"),
    );
    let appearance_id = doc.add_object(Object::Stream(appearance_stream));

    let mut annot_dict = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype_name(&kind),
        "Rect" => rect.as_pdf_array(),
        "C" => color.as_pdf_array(),
        "CA" => opacity,
        "AP" => dictionary! { "N" => Object::Reference(appearance_id) },
        // Print flag (bit 3) set so the annotation shows up in printed/
        // flattened output, not just on-screen — matches every mainstream
        // editor's default for markup annotations.
        "F" => 4,
    };
    if let Some(text) = contents {
        annot_dict.set("Contents", Object::string_literal(text));
    }
    if let AnnotationKind::Highlight { quads }
    | AnnotationKind::Underline { quads }
    | AnnotationKind::StrikeOut { quads } = &kind
    {
        let flat: Vec<Object> = quads
            .iter()
            .flat_map(|q| q.as_quad_points())
            .map(Object::from)
            .collect();
        annot_dict.set("QuadPoints", flat);
    }

    let annot_id = doc.add_object(Object::Dictionary(annot_dict));
    doc.append_annotation_ref(page_index, annot_id)?;

    Ok(annot_id)
}

/// Removes one annotation, identified by the `id` an earlier
/// [`list_annotations`] call reported for it — see
/// `Document::remove_annotation_ref`'s doc for exactly what "removed"
/// means (unlinked from the page's `/Annots` array; the annotation's own
/// object and appearance stream are left as orphaned, unreferenced
/// objects, not scrubbed from the file).
///
/// A form field's widget needs a second unlink. Its identity and its
/// value live on the document's `/AcroForm`/`/Fields` entry, not on the
/// page — so removing it from `/Annots` alone leaves a field nothing
/// draws and nothing can reach, still holding its value. Creating a
/// field with the same name afterwards then produces, by the spec, *one*
/// field with two widgets: the new one appears pre-filled with the old
/// one's value, and neither can be edited independently. That is not a
/// hypothetical — it is what deleting a text field and drawing another
/// did.
pub fn delete_annotation(
    doc: &mut Document,
    page_index: u32,
    annot_id: ObjectId,
) -> Result<(), AnnotError> {
    doc.remove_annotation_ref(page_index, annot_id)?;
    // Unconditional, and cheap: an ordinary annotation is never in
    // `/Fields`, so this is a no-op for everything that isn't a widget.
    // Asking "is this a widget?" first would mean reading the annotation
    // dict to reach the same answer the array lookup already gives.
    doc.remove_acroform_field(annot_id)?;
    Ok(())
}

/// Clamps a stroke/underline width into a sane on-page range.
fn line_width(px: f32) -> Object {
    px.clamp(1.0, 2.5).into()
}

/// Named resource entries (e.g. `/Font`) an appearance stream needs
/// beyond the always-present `ExtGState`.
type ExtraResources = Vec<(&'static str, Object)>;

/// Builds the appearance stream's content operators, plus any resources
/// beyond the always-present `ExtGState` (namely `/Font` for FreeText).
fn build_appearance(
    kind: &AnnotationKind,
    rect: Rect,
    color: Color,
) -> Result<(Content, ExtraResources), AnnotError> {
    let mut ops = vec![Operation::new("gs", vec!["GS0".into()])];

    match kind {
        AnnotationKind::Highlight { quads } => {
            if quads.is_empty() {
                return Err(AnnotError::EmptyGeometry);
            }
            ops.push(Operation::new("rg", color.as_pdf_array()));
            for quad in quads {
                let [x1, y1, x2, y2, x3, y3, x4, y4] = quad.as_quad_points();
                ops.push(Operation::new("m", vec![x1.into(), y1.into()]));
                ops.push(Operation::new("l", vec![x2.into(), y2.into()]));
                ops.push(Operation::new("l", vec![x4.into(), y4.into()]));
                ops.push(Operation::new("l", vec![x3.into(), y3.into()]));
                ops.push(Operation::new("h", vec![]));
            }
            ops.push(Operation::new("f", vec![]));
        }
        AnnotationKind::Underline { quads } | AnnotationKind::StrikeOut { quads } => {
            if quads.is_empty() {
                return Err(AnnotError::EmptyGeometry);
            }
            let strike = matches!(kind, AnnotationKind::StrikeOut { .. });
            ops.push(Operation::new("RG", color.as_pdf_array()));
            ops.push(Operation::new(
                "w",
                vec![line_width((rect.y1 - rect.y0) * 0.06)],
            ));
            for quad in quads {
                let q = quad.as_quad_points();
                let (start, end) = if strike {
                    // Midline between top and bottom, at each end x.
                    let y = (q[1] + q[5]) / 2.0;
                    ((q[0], y), (q[2], y))
                } else {
                    // A hair above the bottom edge.
                    let y = q[5] + (q[1] - q[5]) * 0.08;
                    ((q[0], y), (q[2], y))
                };
                ops.push(Operation::new("m", vec![start.0.into(), start.1.into()]));
                ops.push(Operation::new("l", vec![end.0.into(), end.1.into()]));
            }
            ops.push(Operation::new("S", vec![]));
        }
        AnnotationKind::FreeText { text, font_size } => {
            ops.push(Operation::new("rg", color.as_pdf_array()));
            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new("Tf", vec!["F1".into(), (*font_size).into()]));
            let leading = font_size * 1.2;
            ops.push(Operation::new("TL", vec![leading.into()]));
            ops.push(Operation::new(
                "Td",
                vec![(rect.x0 + 2.0).into(), (rect.y1 - font_size - 2.0).into()],
            ));
            for (i, line) in wrap_text(text, rect.x1 - rect.x0, *font_size)
                .into_iter()
                .enumerate()
            {
                if i > 0 {
                    ops.push(Operation::new("T*", vec![]));
                }
                ops.push(Operation::new("Tj", vec![Object::string_literal(line)]));
            }
            ops.push(Operation::new("ET", vec![]));
            return Ok((
                Content { operations: ops },
                vec![(
                    "Font",
                    dictionary! {
                        "F1" => dictionary! {
                            "Type" => "Font",
                            "Subtype" => "Type1",
                            "BaseFont" => "Helvetica",
                        },
                    }
                    .into(),
                )],
            ));
        }
        AnnotationKind::Ink { strokes } => {
            if strokes.is_empty() || strokes.iter().all(|s| s.is_empty()) {
                return Err(AnnotError::EmptyGeometry);
            }
            ops.push(Operation::new("RG", color.as_pdf_array()));
            ops.push(Operation::new("w", vec![1.5.into()]));
            ops.push(Operation::new("J", vec![1.into()])); // round line cap
            ops.push(Operation::new("j", vec![1.into()])); // round line join
            for stroke in strokes {
                let mut points = stroke.iter();
                let Some((x0, y0)) = points.next() else {
                    continue;
                };
                ops.push(Operation::new("m", vec![(*x0).into(), (*y0).into()]));
                for (x, y) in points {
                    ops.push(Operation::new("l", vec![(*x).into(), (*y).into()]));
                }
            }
            ops.push(Operation::new("S", vec![]));
        }
        AnnotationKind::Square { line_width, fill } => {
            let w = shape_line_width(*line_width);
            let inset = inset_rect(rect, effective_stroke_width(*fill, w).unwrap_or(0.0) / 2.0)?;
            push_shape_state(&mut ops, color, *fill, w);
            ops.push(Operation::new(
                "re",
                vec![
                    inset.x0.into(),
                    inset.y0.into(),
                    (inset.x1 - inset.x0).into(),
                    (inset.y1 - inset.y0).into(),
                ],
            ));
            ops.push(shape_paint_op(*fill, w));
        }
        AnnotationKind::Circle { line_width, fill } => {
            let w = shape_line_width(*line_width);
            let inset = inset_rect(rect, effective_stroke_width(*fill, w).unwrap_or(0.0) / 2.0)?;
            push_shape_state(&mut ops, color, *fill, w);

            // Four cubic Béziers, the standard circle approximation: a
            // control point at KAPPA of the half-axis puts the maximum
            // radial error around 0.02%, which is invisible at any zoom a
            // reader offers.
            let (cx, cy) = ((inset.x0 + inset.x1) / 2.0, (inset.y0 + inset.y1) / 2.0);
            let (rx, ry) = ((inset.x1 - inset.x0) / 2.0, (inset.y1 - inset.y0) / 2.0);
            let (ox, oy) = (rx * KAPPA, ry * KAPPA);
            ops.push(Operation::new("m", vec![(cx - rx).into(), cy.into()]));
            for [c1x, c1y, c2x, c2y, ex, ey] in [
                [cx - rx, cy + oy, cx - ox, cy + ry, cx, cy + ry],
                [cx + ox, cy + ry, cx + rx, cy + oy, cx + rx, cy],
                [cx + rx, cy - oy, cx + ox, cy - ry, cx, cy - ry],
                [cx - ox, cy - ry, cx - rx, cy - oy, cx - rx, cy],
            ] {
                ops.push(Operation::new(
                    "c",
                    vec![
                        c1x.into(),
                        c1y.into(),
                        c2x.into(),
                        c2y.into(),
                        ex.into(),
                        ey.into(),
                    ],
                ));
            }
            ops.push(Operation::new("h", vec![]));
            ops.push(shape_paint_op(*fill, w));
        }
    }

    Ok((Content { operations: ops }, Vec::new()))
}

/// Bézier control-point offset for approximating a quarter circle:
/// `4/3 * (sqrt(2) - 1)`.
const KAPPA: f32 = 0.552_284_8;

/// Shapes with no border still need a hairline if they have no fill
/// either, or they'd be invisible; and a negative width is meaningless.
fn shape_line_width(requested: f32) -> f32 {
    requested.clamp(0.0, 72.0)
}

/// Shrinks `rect` by `by` on every side.
///
/// A stroke is centred on the path, so half the line width falls outside
/// it; without this inset a thick border spills past the annotation's own
/// `/Rect`, which readers clip — the border then looks thinner on the
/// outside edges than it is.
fn inset_rect(rect: Rect, by: f32) -> Result<Rect, AnnotError> {
    let inset = Rect {
        x0: rect.x0 + by,
        y0: rect.y0 + by,
        x1: rect.x1 - by,
        y1: rect.y1 - by,
    };
    if inset.x1 <= inset.x0 || inset.y1 <= inset.y0 {
        // A border thicker than the box itself has no interior left to
        // draw; better to say so than to emit an inverted rectangle.
        return Err(AnnotError::EmptyGeometry);
    }
    Ok(inset)
}

/// A shape with neither fill nor border would paint nothing, so a
/// zero-width outline-only shape falls back to a hairline rather than
/// being invisible.
fn effective_stroke_width(fill: Option<Color>, width: f32) -> Option<f32> {
    if width > 0.0 {
        Some(width)
    } else if fill.is_none() {
        Some(HAIRLINE_WIDTH)
    } else {
        None
    }
}

/// Thinnest border that still renders on a normal display.
const HAIRLINE_WIDTH: f32 = 0.75;

fn push_shape_state(ops: &mut Vec<Operation>, stroke: Color, fill: Option<Color>, width: f32) {
    if let Some(fill) = fill {
        ops.push(Operation::new("rg", fill.as_pdf_array()));
    }
    // The stroke colour is set whenever the path will actually be
    // stroked — including the hairline fallback, which otherwise
    // inherits the default black and silently ignores the chosen colour.
    if let Some(w) = effective_stroke_width(fill, width) {
        ops.push(Operation::new("RG", stroke.as_pdf_array()));
        ops.push(Operation::new("w", vec![line_width(w)]));
    }
}

/// Which painting operator closes the path: fill, stroke, or both.
fn shape_paint_op(fill: Option<Color>, width: f32) -> Operation {
    match (
        fill.is_some(),
        effective_stroke_width(fill, width).is_some(),
    ) {
        (true, true) => Operation::new("B", vec![]),
        (true, false) => Operation::new("f", vec![]),
        _ => Operation::new("S", vec![]),
    }
}

/// Greedy word-wrap by approximate glyph width (Helvetica at roughly
/// 0.5em average advance — good enough for a first pass; real width
/// metrics land with proper font handling later).
fn wrap_text(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    let avg_char_width = font_size * 0.5;
    let max_chars = ((max_width / avg_char_width).floor() as usize).max(1);

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if candidate_len > max_chars && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Everything a comments panel needs to list one annotation, read back
/// from the document without the caller touching `lopdf` directly.
#[derive(Debug, Clone)]
pub struct AnnotationSummary {
    pub id: ObjectId,
    pub subtype: String,
    pub rect: Rect,
    pub contents: Option<String>,
}

pub fn list_annotations(
    doc: &Document,
    page_index: u32,
) -> Result<Vec<AnnotationSummary>, AnnotError> {
    let refs = doc.page_annotation_refs(page_index)?;
    let mut out = Vec::with_capacity(refs.len());
    for id in refs {
        let dict = doc.dictionary(id)?;
        out.push(summarize(id, dict));
    }
    Ok(out)
}

fn summarize(id: ObjectId, dict: &Dictionary) -> AnnotationSummary {
    let subtype = dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let rect = dict
        .get(b"Rect")
        .ok()
        .and_then(|o| o.as_array().ok())
        .and_then(|arr| {
            let nums: Vec<f32> = arr.iter().filter_map(|o| o.as_float().ok()).collect();
            match nums.as_slice() {
                [x0, y0, x1, y1] => Some(Rect {
                    x0: *x0,
                    y0: *y0,
                    x1: *x1,
                    y1: *y1,
                }),
                _ => None,
            }
        })
        .unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
    let contents = dict
        .get(b"Contents")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|b| String::from_utf8_lossy(b).into_owned());

    AnnotationSummary {
        id,
        subtype,
        rect,
        contents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::Operation as Op;
    use lopdf::{dictionary as dict, Object as O, Stream as S};

    fn minimal_pdf_bytes() -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![Op::new("BT", vec![]), Op::new("ET", vec![])],
        };
        let content_id = doc.add_object(S::new(dict! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dict! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dict! {},
        });
        doc.objects.insert(
            pages_id,
            O::Dictionary(dict! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dict! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn sample_rect() -> Rect {
        Rect {
            x0: 72.0,
            y0: 700.0,
            x1: 300.0,
            y1: 720.0,
        }
    }

    #[test]
    fn highlight_round_trips_with_quad_points_and_appearance_stream() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).unwrap();
        let annot = NewAnnotation {
            rect: sample_rect(),
            color: Color::YELLOW,
            kind: AnnotationKind::Highlight {
                quads: vec![sample_rect()],
            },
            contents: Some("a comment".into()),
            opacity: 0.4,
        };
        let annot_id = add_annotation(&mut doc, 0, annot).expect("add_annotation should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).expect("should reparse");
        let annot_dict = reopened
            .get_dictionary(annot_id)
            .expect("annotation should be a dict");
        assert_eq!(
            annot_dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Highlight"
        );
        assert_eq!(
            annot_dict.get(b"Contents").unwrap().as_str().unwrap(),
            b"a comment"
        );

        let quad_points = annot_dict.get(b"QuadPoints").unwrap().as_array().unwrap();
        assert_eq!(quad_points.len(), 8);

        let ap = annot_dict.get(b"AP").unwrap().as_dict().unwrap();
        let n_ref = ap.get(b"N").unwrap();
        let O::Reference(appearance_id) = n_ref else {
            panic!("AP/N should be a reference")
        };
        let appearance = reopened.get_object(*appearance_id).unwrap();
        let O::Stream(stream) = appearance else {
            panic!("appearance should be a stream")
        };
        assert_eq!(
            stream.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
        assert!(
            !stream.content.is_empty(),
            "appearance stream must have drawing operators"
        );
    }

    #[test]
    fn underline_and_strikeout_produce_line_geometry_not_a_fill() {
        for (kind, expected_subtype) in [
            (
                AnnotationKind::Underline {
                    quads: vec![sample_rect()],
                },
                "Underline",
            ),
            (
                AnnotationKind::StrikeOut {
                    quads: vec![sample_rect()],
                },
                "StrikeOut",
            ),
        ] {
            let mut doc = Document::from_bytes(&minimal_pdf_bytes()).unwrap();
            let annot = NewAnnotation {
                rect: sample_rect(),
                color: Color::RED,
                kind,
                contents: None,
                opacity: 1.0,
            };
            let annot_id = add_annotation(&mut doc, 0, annot).unwrap();
            let saved = doc.save_incremental().unwrap();

            let reopened = lopdf::Document::load_mem(&saved).unwrap();
            let annot_dict = reopened.get_dictionary(annot_id).unwrap();
            assert_eq!(
                annot_dict.get(b"Subtype").unwrap().as_name().unwrap(),
                expected_subtype.as_bytes()
            );
        }
    }

    #[test]
    fn freetext_appearance_embeds_helvetica_and_wrapped_text() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).unwrap();
        let annot = NewAnnotation {
            rect: Rect {
                x0: 72.0,
                y0: 600.0,
                x1: 200.0,
                y1: 680.0,
            },
            color: Color::BLACK,
            kind: AnnotationKind::FreeText {
                text: "This is a fairly long note that should wrap onto more than one line.".into(),
                font_size: 11.0,
            },
            contents: None,
            opacity: 1.0,
        };
        let annot_id = add_annotation(&mut doc, 0, annot).unwrap();
        let saved = doc.save_incremental().unwrap();

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let annot_dict = reopened.get_dictionary(annot_id).unwrap();
        let ap = annot_dict.get(b"AP").unwrap().as_dict().unwrap();
        let O::Reference(appearance_id) = ap.get(b"N").unwrap() else {
            panic!()
        };
        let O::Stream(stream) = reopened.get_object(*appearance_id).unwrap() else {
            panic!()
        };
        let resources = stream.dict.get(b"Resources").unwrap().as_dict().unwrap();
        let fonts = resources.get(b"Font").unwrap().as_dict().unwrap();
        assert!(
            fonts.has(b"F1"),
            "FreeText appearance must embed a font resource"
        );

        let content = Content::decode(&stream.content).expect("appearance content should decode");
        let tj_count = content
            .operations
            .iter()
            .filter(|op| op.operator == "Tj")
            .count();
        assert!(
            tj_count >= 2,
            "a wrapped multi-line note should emit more than one Tj, got {tj_count}"
        );
    }

    #[test]
    fn ink_appearance_draws_every_stroke() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).unwrap();
        let strokes = vec![
            vec![(80.0, 700.0), (100.0, 710.0), (120.0, 700.0)],
            vec![(80.0, 680.0), (120.0, 680.0)],
        ];
        let annot = NewAnnotation {
            rect: Rect {
                x0: 70.0,
                y0: 670.0,
                x1: 130.0,
                y1: 715.0,
            },
            color: Color::BLACK,
            kind: AnnotationKind::Ink {
                strokes: strokes.clone(),
            },
            contents: None,
            opacity: 1.0,
        };
        let annot_id = add_annotation(&mut doc, 0, annot).unwrap();
        let saved = doc.save_incremental().unwrap();

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let annot_dict = reopened.get_dictionary(annot_id).unwrap();
        let ap = annot_dict.get(b"AP").unwrap().as_dict().unwrap();
        let O::Reference(appearance_id) = ap.get(b"N").unwrap() else {
            panic!()
        };
        let O::Stream(stream) = reopened.get_object(*appearance_id).unwrap() else {
            panic!()
        };
        let content = Content::decode(&stream.content).unwrap();
        let move_count = content
            .operations
            .iter()
            .filter(|op| op.operator == "m")
            .count();
        assert_eq!(move_count, strokes.len(), "one moveto per stroke");
    }

    #[test]
    fn empty_highlight_quads_is_a_clean_error_not_a_panic() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).unwrap();
        let annot = NewAnnotation {
            rect: sample_rect(),
            color: Color::YELLOW,
            kind: AnnotationKind::Highlight { quads: vec![] },
            contents: None,
            opacity: 0.4,
        };
        let result = add_annotation(&mut doc, 0, annot);
        assert!(matches!(result, Err(AnnotError::EmptyGeometry)));
    }

    #[test]
    fn list_annotations_reports_everything_added() {
        let mut doc = Document::from_bytes(&minimal_pdf_bytes()).unwrap();
        add_annotation(
            &mut doc,
            0,
            NewAnnotation {
                rect: sample_rect(),
                color: Color::YELLOW,
                kind: AnnotationKind::Highlight {
                    quads: vec![sample_rect()],
                },
                contents: Some("first".into()),
                opacity: 0.4,
            },
        )
        .unwrap();
        add_annotation(
            &mut doc,
            0,
            NewAnnotation {
                rect: sample_rect(),
                color: Color::RED,
                kind: AnnotationKind::Underline {
                    quads: vec![sample_rect()],
                },
                contents: None,
                opacity: 1.0,
            },
        )
        .unwrap();

        let summaries = list_annotations(&doc, 0).expect("listing should succeed");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].subtype, "Highlight");
        assert_eq!(summaries[0].contents.as_deref(), Some("first"));
        assert_eq!(summaries[1].subtype, "Underline");
    }

    #[test]
    fn wrap_text_never_produces_empty_output_for_nonempty_input() {
        let lines = wrap_text("hello world this is a test", 100.0, 12.0);
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|l| !l.trim().is_empty()));
    }

    #[test]
    fn wrap_text_handles_a_single_very_long_word() {
        // Must not infinite-loop or panic when one "word" alone exceeds
        // the wrap width (a URL, a long filename, etc.).
        let long_word = "a".repeat(500);
        let lines = wrap_text(&long_word, 50.0, 12.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], long_word);
    }
}
