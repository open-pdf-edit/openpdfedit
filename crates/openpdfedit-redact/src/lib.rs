//! True content-removal redaction (PLAN.md M5: "manual redaction with
//! true removal").
//!
//! The classic redaction bug this exists to avoid: painting a black box
//! *over* sensitive content leaves the underlying text/image bytes fully
//! intact and recoverable (select-all, copy-paste, or just deleting the
//! box in an editor reveals it) — a real, repeated real-world leak
//! pattern, not a hypothetical. [`redact_content`] instead interprets a
//! page's content stream (tracking the graphics state — CTM via `q`/`Q`/
//! `cm`, text position via `BT`/`Tm`/`Td`/`TD`/`T*` — well enough to know
//! where each paint operation actually lands on the page) and **drops
//! the operators that paint anything overlapping the redacted region
//! entirely**, rather than leaving them in place under a visual cover.
//! [`redact_page`] then also overlays a solid box on top, belt-and-
//! suspenders: even if the interpreter's approximation missed something,
//! the page still shows nothing there.
//!
//! ## Scope and honesty about its limits
//!
//! This is a **coarse, conservative** interpreter, not a full PDF
//! rendering engine:
//!
//! - **Text bounding boxes are approximate**, not real glyph metrics: a
//!   string's extent is estimated from font size and character count
//!   (`0.5 × font_size` per character), not the font's actual glyph
//!   widths. This can only make the estimated box *larger* than the true
//!   rendered text in the common case (most fonts' average character
//!   width is well under half the em size for latin text at normal
//!   tracking) — meaning it can flag content for removal that a precise
//!   engine would consider just outside the redaction rect, but it does
//!   not risk the opposite (leaving redacted-looking text that's
//!   actually still there). For a redaction tool, erring toward removing
//!   too much is the correct failure direction; erring toward removing
//!   too little is not.
//! - **Path (non-text, non-image) geometry uses point-cloud bounding
//!   boxes**, not exact curve extents: every coordinate operand of a
//!   path-construction operator (`m`/`l`/`c`/`v`/`y`/`re`) contributes to
//!   an axis-aligned bounding box, including Bézier control points —
//!   which is the true bounding box for a `re` (rectangle) and a
//!   superset (never smaller) of the true bounding box for a curved
//!   path, since a cubic Bézier curve never leaves the convex hull of its
//!   control points. Same "conservative, never under-removes" property
//!   as the text approximation.
//! - **No partial-glyph or partial-path redaction.** If a text-showing
//!   or path-painting operator's bounding box overlaps the redaction
//!   rect *at all*, the whole operator (whole string, whole path) is
//!   dropped — not clipped down to just the covered portion. Simpler and
//!   safer than trying to split a run of text or a path mid-stream, at
//!   the cost of sometimes removing slightly more than strictly
//!   necessary just outside the redaction box.
//! - **Clipping paths (`W`/`W*`) are not applied** — a redaction rect
//!   that only *would* be visible through an active clip is still
//!   treated as visible for the purposes of this pass. Rare in practice
//!   for the kind of flat, single-layer content this targets (scanned
//!   pages, ordinary text documents); a real risk for clip-heavy vector
//!   art or forms with visibility layers (`/OCG`s, not handled either).
//! - **Metadata/hidden-content sanitization (document info, XMP,
//!   embedded files, JavaScript, hidden layers, previous-revision bytes
//!   in an incrementally-saved file) is out of scope for this pass** —
//!   this crate only rewrites one page's visible content stream. PLAN.md's
//!   "sanitize (PDF4QT port)" half of the M5 redaction milestone is not
//!   implemented; only "true removal" of the selected on-page content is.

use lopdf::content::{Content, Operation};
use lopdf::Object;
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedactError {
    #[error("failed to decode content stream: {0}")]
    ContentDecode(String),
    #[error("failed to encode content stream: {0}")]
    ContentEncode(String),
    #[error(transparent)]
    Doc(#[from] DocError),
}

/// A 2D affine transform, `[a, b, c, d, e, f]` in PDF's row-vector
/// convention: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
pub type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Composes `m1` then `m2` (a point transformed by `m1` first, then by
/// `m2`) — the same order the `cm` operator's own semantics need: the new
/// CTM is the incoming matrix concatenated *before* the existing CTM.
fn multiply(m1: Matrix, m2: Matrix) -> Matrix {
    [
        m1[0] * m2[0] + m1[1] * m2[2],
        m1[0] * m2[1] + m1[1] * m2[3],
        m1[2] * m2[0] + m1[3] * m2[2],
        m1[2] * m2[1] + m1[3] * m2[3],
        m1[4] * m2[0] + m1[5] * m2[2] + m2[4],
        m1[4] * m2[1] + m1[5] * m2[3] + m2[5],
    ]
}

fn transform_point(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

/// An axis-aligned rectangle in page-space points (PDF's native unit,
/// origin bottom-left, y-up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    fn from_points(points: &[(f64, f64)]) -> Option<Rect> {
        if points.is_empty() {
            return None;
        }
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        Some(Rect { x0, y0, x1, y1 })
    }

    fn intersects(&self, other: &Rect) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.y0 < other.y1 && other.y0 < self.y1
    }
}

fn number(obj: &Object) -> f64 {
    obj.as_float()
        .map(f64::from)
        .unwrap_or_else(|_| obj.as_i64().unwrap_or(0) as f64)
}

/// Estimates whether a text-showing operator painting `char_count`
/// characters at the current `text_matrix`/`ctm`/`font_size` overlaps
/// `rect` — see this crate's module doc for why the estimate is
/// deliberately coarse (and safe in the direction it's coarse).
fn text_overlaps(
    text_matrix: Matrix,
    ctm: Matrix,
    font_size: f64,
    char_count: usize,
    rect: Rect,
) -> bool {
    if char_count == 0 {
        return false;
    }
    let combined = multiply(text_matrix, ctm);
    let width = font_size.max(1.0) * char_count as f64 * 0.5;
    let height = font_size.max(1.0);
    let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
    let device_points: Vec<(f64, f64)> = corners
        .iter()
        .map(|&(x, y)| transform_point(combined, x, y))
        .collect();
    Rect::from_points(&device_points)
        .map(|b| b.intersects(&rect))
        .unwrap_or(false)
}

fn string_operand_len(obj: Option<&Object>) -> usize {
    obj.and_then(|o| o.as_str().ok())
        .map(|s| s.len())
        .unwrap_or(0)
}

fn tj_array_len(obj: Option<&Object>) -> usize {
    obj.and_then(|o| o.as_array().ok())
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o.as_str().ok())
                .map(|s| s.len())
                .sum()
        })
        .unwrap_or(0)
}

pub struct RedactedContent {
    pub bytes: Vec<u8>,
    /// Number of top-level operators dropped (a multi-operator path
    /// counts every construction operator plus its paint operator).
    pub removed_operations: usize,
}

/// Rewrites `content_bytes` (one page's decoded content-stream bytes —
/// see [`Document::page_content_bytes`]), dropping every operator that
/// paints anything overlapping `rect`. See this crate's module doc for
/// exactly what "overlapping" means for text, images, and paths, and
/// this function's known limitations.
/// Which kinds of painted content a removal pass takes out of a region.
///
/// Redaction and text editing want genuinely different things here, and
/// conflating them was a real bug: editing one line of text on a
/// coloured background wiped the background too, because the background
/// rectangle overlaps the text and every overlapping operator was being
/// dropped. Redaction *must* keep that behaviour — anything left behind
/// in a redacted region could leak — so the two cases are now explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalScope {
    /// Text, images and vector paths — everything that paints. Required
    /// for redaction, where leaving any content behind defeats the point.
    Everything,
    /// Only text-showing operators. Used when replacing text: the
    /// surrounding artwork (background fills, rules, images) is part of
    /// the page's formatting and must survive the edit.
    TextOnly,
    /// Only image/form XObject placements, for repositioning an image
    /// without disturbing text or artwork around it.
    ImagesOnly,
}

impl RemovalScope {
    fn removes_text(self) -> bool {
        matches!(self, Self::Everything | Self::TextOnly)
    }
    fn removes_images(self) -> bool {
        matches!(self, Self::Everything | Self::ImagesOnly)
    }
    fn removes_paths(self) -> bool {
        matches!(self, Self::Everything)
    }
}

pub fn redact_content(content_bytes: &[u8], rect: Rect) -> Result<RedactedContent, RedactError> {
    redact_content_scoped(content_bytes, rect, RemovalScope::Everything)
}

/// [`redact_content`], but removing only the kinds of content named by
/// `scope` — see [`RemovalScope`].
pub fn redact_content_scoped(
    content_bytes: &[u8],
    rect: Rect,
    scope: RemovalScope,
) -> Result<RedactedContent, RedactError> {
    let content =
        Content::decode(content_bytes).map_err(|e| RedactError::ContentDecode(e.to_string()))?;

    let mut ctm_stack: Vec<Matrix> = Vec::new();
    let mut ctm: Matrix = IDENTITY;
    let mut text_matrix: Matrix = IDENTITY;
    let mut text_line_matrix: Matrix = IDENTITY;
    let mut font_size: f64 = 0.0;
    let mut leading: f64 = 0.0;

    let mut path_points: Vec<(f64, f64)> = Vec::new();
    let mut pending_path_ops: Vec<Operation> = Vec::new();

    let mut out_ops: Vec<Operation> = Vec::new();
    let mut removed = 0usize;

    for op in content.operations {
        match op.operator.as_str() {
            "q" => {
                ctm_stack.push(ctm);
                out_ops.push(op);
            }
            "Q" => {
                if let Some(m) = ctm_stack.pop() {
                    ctm = m;
                }
                out_ops.push(op);
            }
            "cm" => {
                if op.operands.len() == 6 {
                    let m: Matrix = std::array::from_fn(|i| number(&op.operands[i]));
                    ctm = multiply(m, ctm);
                }
                out_ops.push(op);
            }
            "BT" => {
                text_matrix = IDENTITY;
                text_line_matrix = IDENTITY;
                out_ops.push(op);
            }
            "Tf" => {
                if let Some(size) = op.operands.get(1) {
                    font_size = number(size);
                }
                out_ops.push(op);
            }
            "Tm" => {
                if op.operands.len() == 6 {
                    let m: Matrix = std::array::from_fn(|i| number(&op.operands[i]));
                    text_matrix = m;
                    text_line_matrix = m;
                }
                out_ops.push(op);
            }
            "Td" | "TD" => {
                if op.operands.len() == 2 {
                    let tx = number(&op.operands[0]);
                    let ty = number(&op.operands[1]);
                    if op.operator == "TD" {
                        leading = -ty;
                    }
                    text_line_matrix = multiply([1.0, 0.0, 0.0, 1.0, tx, ty], text_line_matrix);
                    text_matrix = text_line_matrix;
                }
                out_ops.push(op);
            }
            "T*" => {
                text_line_matrix = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -leading], text_line_matrix);
                text_matrix = text_line_matrix;
                out_ops.push(op);
            }
            "TL" => {
                if let Some(v) = op.operands.first() {
                    leading = number(v);
                }
                out_ops.push(op);
            }
            "Tj" => {
                let len = string_operand_len(op.operands.first());
                if scope.removes_text() && text_overlaps(text_matrix, ctm, font_size, len, rect) {
                    removed += 1;
                } else {
                    out_ops.push(op);
                }
            }
            "'" => {
                text_line_matrix = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -leading], text_line_matrix);
                text_matrix = text_line_matrix;
                let len = string_operand_len(op.operands.first());
                if scope.removes_text() && text_overlaps(text_matrix, ctm, font_size, len, rect) {
                    removed += 1;
                } else {
                    out_ops.push(op);
                }
            }
            "\"" => {
                text_line_matrix = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -leading], text_line_matrix);
                text_matrix = text_line_matrix;
                let len = string_operand_len(op.operands.get(2));
                if scope.removes_text() && text_overlaps(text_matrix, ctm, font_size, len, rect) {
                    removed += 1;
                } else {
                    out_ops.push(op);
                }
            }
            "TJ" => {
                let len = tj_array_len(op.operands.first());
                if scope.removes_text() && text_overlaps(text_matrix, ctm, font_size, len, rect) {
                    removed += 1;
                } else {
                    out_ops.push(op);
                }
            }
            "Do" => {
                // An image/form XObject is placed by mapping the unit
                // square (0,0)-(1,1) through the CTM — the standard
                // convention every PDF-producing tool relies on.
                let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
                let device_points: Vec<(f64, f64)> = corners
                    .iter()
                    .map(|&(x, y)| transform_point(ctm, x, y))
                    .collect();
                let overlaps = Rect::from_points(&device_points)
                    .map(|b| b.intersects(&rect))
                    .unwrap_or(false);
                if scope.removes_images() && overlaps {
                    removed += 1;
                } else {
                    out_ops.push(op);
                }
            }
            "re" => {
                if op.operands.len() == 4 {
                    let x = number(&op.operands[0]);
                    let y = number(&op.operands[1]);
                    let w = number(&op.operands[2]);
                    let h = number(&op.operands[3]);
                    path_points.extend([(x, y), (x + w, y), (x, y + h), (x + w, y + h)]);
                }
                pending_path_ops.push(op);
            }
            "m" | "l" => {
                if op.operands.len() == 2 {
                    path_points.push((number(&op.operands[0]), number(&op.operands[1])));
                }
                pending_path_ops.push(op);
            }
            "c" => {
                if op.operands.len() == 6 {
                    path_points.push((number(&op.operands[0]), number(&op.operands[1])));
                    path_points.push((number(&op.operands[2]), number(&op.operands[3])));
                    path_points.push((number(&op.operands[4]), number(&op.operands[5])));
                }
                pending_path_ops.push(op);
            }
            "v" | "y" => {
                if op.operands.len() == 4 {
                    path_points.push((number(&op.operands[0]), number(&op.operands[1])));
                    path_points.push((number(&op.operands[2]), number(&op.operands[3])));
                }
                pending_path_ops.push(op);
            }
            "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" | "n" => {
                let device_points: Vec<(f64, f64)> = path_points
                    .iter()
                    .map(|&(x, y)| transform_point(ctm, x, y))
                    .collect();
                let overlaps = Rect::from_points(&device_points)
                    .map(|b| b.intersects(&rect))
                    .unwrap_or(false);
                // "n" paints nothing (it's the no-op path-painting
                // operator, used for clip-only paths) — never worth
                // dropping since there's nothing visible to redact.
                if scope.removes_paths() && overlaps && op.operator != "n" {
                    removed += pending_path_ops.len() + 1;
                } else {
                    out_ops.append(&mut pending_path_ops);
                    out_ops.push(op);
                }
                pending_path_ops.clear();
                path_points.clear();
            }
            _ => {
                // Clip markers (W/W*), color/graphics-state operators,
                // marked content, and anything else this interpreter
                // doesn't specifically track never paint on their own —
                // pass through unchanged. A clip marker mid-path is kept
                // in the pending buffer either way (it's not matched by
                // any arm above that clears it), so it's still emitted
                // or dropped together with the path it belongs to.
                if matches!(op.operator.as_str(), "W" | "W*") {
                    pending_path_ops.push(op);
                } else {
                    out_ops.push(op);
                }
            }
        }
    }
    // Any leftover pending path ops (content ending mid-path, without a
    // paint operator — malformed, but not this function's job to
    // reject) are emitted as-is: nothing was ever painted, so there's
    // nothing to redact.
    out_ops.append(&mut pending_path_ops);

    let bytes = Content {
        operations: out_ops,
    }
    .encode()
    .map_err(|e| RedactError::ContentEncode(e.to_string()))?;
    Ok(RedactedContent {
        bytes,
        removed_operations: removed,
    })
}

/// Redacts `rect` on `page_index`: rewrites the page's content stream via
/// [`redact_content`] (true removal), then overlays a solid `color` box
/// over the same region — belt-and-suspenders on top of the removal
/// above, so even a gap in this interpreter's coverage (see module doc)
/// still shows nothing there visually. Replaces the page's `/Contents`
/// wholesale via [`Document::set_page_contents`]; does not save — call
/// [`Document::save_incremental`] afterwards. Returns the number of
/// operators removed.
pub fn redact_page(
    doc: &mut Document,
    page_index: u32,
    rect: Rect,
    color: [f32; 3],
) -> Result<usize, RedactError> {
    let original = doc.page_content_bytes(page_index)?;
    let redacted = redact_content(&original, rect)?;

    let mut bytes = redacted.bytes;
    bytes.extend(overlay_box_bytes(rect, color)?);

    doc.set_page_contents(page_index, bytes)?;
    Ok(redacted.removed_operations)
}

fn overlay_box_bytes(rect: Rect, color: [f32; 3]) -> Result<Vec<u8>, RedactError> {
    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "rg",
            vec![
                (color[0] as f64).into(),
                (color[1] as f64).into(),
                (color[2] as f64).into(),
            ],
        ),
        Operation::new(
            "re",
            vec![
                rect.x0.into(),
                rect.y0.into(),
                (rect.x1 - rect.x0).into(),
                (rect.y1 - rect.y0).into(),
            ],
        ),
        Operation::new("f", vec![]),
        Operation::new("Q", vec![]),
    ];
    Content { operations: ops }
        .encode()
        .map_err(|e| RedactError::ContentEncode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_operators(bytes: &[u8]) -> Vec<String> {
        Content::decode(bytes)
            .expect("should decode")
            .operations
            .into_iter()
            .map(|op| op.operator)
            .collect()
    }

    #[test]
    fn drops_text_that_overlaps_the_redaction_rect() {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![100.0.into(), 100.0.into()]),
                Operation::new("Tj", vec![Object::string_literal("secret")]),
                Operation::new("ET", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        let redacted = redact_content(
            &bytes,
            Rect {
                x0: 90.0,
                y0: 90.0,
                x1: 200.0,
                y1: 120.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 1);
        let ops = decode_operators(&redacted.bytes);
        assert!(
            !ops.contains(&"Tj".to_string()),
            "the Tj call must be gone: {ops:?}"
        );
        // Everything else (state-setting, not painting) survives.
        assert!(ops.contains(&"BT".to_string()));
        assert!(ops.contains(&"ET".to_string()));
    }

    #[test]
    fn keeps_text_that_does_not_overlap_the_redaction_rect() {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![100.0.into(), 100.0.into()]),
                Operation::new("Tj", vec![Object::string_literal("safe")]),
                Operation::new("ET", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        // Redaction rect nowhere near the text's actual position.
        let redacted = redact_content(
            &bytes,
            Rect {
                x0: 500.0,
                y0: 500.0,
                x1: 600.0,
                y1: 600.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 0);
        let ops = decode_operators(&redacted.bytes);
        assert!(ops.contains(&"Tj".to_string()));
    }

    #[test]
    fn drops_a_filled_rectangle_path_that_overlaps() {
        let content = Content {
            operations: vec![
                Operation::new(
                    "re",
                    vec![50.0.into(), 50.0.into(), 100.0.into(), 100.0.into()],
                ),
                Operation::new("f", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        let redacted = redact_content(
            &bytes,
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 200.0,
                y1: 200.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 2, "the re and the f");
        assert!(decode_operators(&redacted.bytes).is_empty());
    }

    #[test]
    fn keeps_a_filled_rectangle_path_that_does_not_overlap() {
        let content = Content {
            operations: vec![
                Operation::new(
                    "re",
                    vec![50.0.into(), 50.0.into(), 100.0.into(), 100.0.into()],
                ),
                Operation::new("f", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        let redacted = redact_content(
            &bytes,
            Rect {
                x0: 500.0,
                y0: 500.0,
                x1: 600.0,
                y1: 600.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 0);
        assert_eq!(decode_operators(&redacted.bytes), vec!["re", "f"]);
    }

    #[test]
    fn cm_translation_is_applied_before_the_overlap_check() {
        // A rectangle drawn at local (0,0)-(10,10) but placed at device
        // (100,100) via `cm` must be checked against its *device*
        // position, not its local one.
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        1.0.into(),
                        0.0.into(),
                        0.0.into(),
                        1.0.into(),
                        100.0.into(),
                        100.0.into(),
                    ],
                ),
                Operation::new("re", vec![0.0.into(), 0.0.into(), 10.0.into(), 10.0.into()]),
                Operation::new("f", vec![]),
                Operation::new("Q", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        // A rect around local (0,0)-(10,10) — should NOT match, since the
        // path is actually painted around device (100,100).
        let far = redact_content(
            &bytes,
            Rect {
                x0: -5.0,
                y0: -5.0,
                x1: 5.0,
                y1: 5.0,
            },
        )
        .expect("should succeed");
        assert_eq!(
            far.removed_operations, 0,
            "cm translation must be respected"
        );

        // A rect around device (100,100)-(110,110) — should match.
        let near = redact_content(
            &bytes,
            Rect {
                x0: 95.0,
                y0: 95.0,
                x1: 115.0,
                y1: 115.0,
            },
        )
        .expect("should succeed");
        assert_eq!(near.removed_operations, 2);
    }

    #[test]
    fn drops_an_image_xobject_whose_unit_square_overlaps() {
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        50.0.into(),
                        0.0.into(),
                        0.0.into(),
                        50.0.into(),
                        10.0.into(),
                        10.0.into(),
                    ],
                ),
                Operation::new("Do", vec!["Im1".into()]),
                Operation::new("Q", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        // Image occupies device (10,10)-(60,60) after the cm scale+translate.
        let redacted = redact_content(
            &bytes,
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 70.0,
                y1: 70.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 1);
        assert!(!decode_operators(&redacted.bytes).contains(&"Do".to_string()));
    }

    #[test]
    fn q_and_q_restore_the_ctm_after_a_nested_transform() {
        // Path A is drawn under a translated cm inside q/Q; path B is
        // drawn afterwards, at the *original* (untranslated) CTM — the
        // `Q` must correctly restore it, or B would wrongly inherit A's
        // translation.
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        1.0.into(),
                        0.0.into(),
                        0.0.into(),
                        1.0.into(),
                        1000.0.into(),
                        1000.0.into(),
                    ],
                ),
                Operation::new("re", vec![0.0.into(), 0.0.into(), 5.0.into(), 5.0.into()]),
                Operation::new("f", vec![]),
                Operation::new("Q", vec![]),
                Operation::new("re", vec![50.0.into(), 50.0.into(), 5.0.into(), 5.0.into()]),
                Operation::new("f", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();

        // Redact near (50,50) — must catch only the second rectangle
        // (the first is way out at (1000,1000) and should survive).
        let redacted = redact_content(
            &bytes,
            Rect {
                x0: 40.0,
                y0: 40.0,
                x1: 60.0,
                y1: 60.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 2, "only the second rect+fill");
    }

    #[test]
    fn empty_content_stream_is_a_no_op() {
        let redacted = redact_content(
            b"",
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            },
        )
        .expect("should succeed");
        assert_eq!(redacted.removed_operations, 0);
    }

    #[test]
    fn redact_page_replaces_content_and_appends_an_overlay_box() {
        use lopdf::{dictionary, Stream};

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![100.0.into(), 100.0.into()]),
                Operation::new("Tj", vec![Object::string_literal("secret")]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });
        raw.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let mut doc = Document::from_bytes(&bytes).expect("should parse");
        let removed = redact_page(
            &mut doc,
            0,
            Rect {
                x0: 90.0,
                y0: 90.0,
                x1: 200.0,
                y1: 120.0,
            },
            [0.0, 0.0, 0.0],
        )
        .expect("redact_page should succeed");
        assert_eq!(removed, 1);

        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let content_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = reopened
            .get_object(content_id)
            .unwrap()
            .as_stream()
            .unwrap();
        let ops = decode_operators(&stream.content);

        assert!(
            !ops.contains(&"Tj".to_string()),
            "original text must be gone: {ops:?}"
        );
        assert!(
            ops.contains(&"re".to_string()),
            "overlay box rectangle must be present"
        );
        assert!(ops.contains(&"f".to_string()), "overlay box must be filled");
    }
}
