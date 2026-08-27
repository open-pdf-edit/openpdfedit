//! Taking a person's markings off a document.
//!
//! The counterpart to [`openpdfedit_flatten`](../openpdfedit_flatten):
//! that bakes markup into the page so it can never be removed, this
//! removes it. What both need first is an answer to the same question —
//! which things on this page did somebody add, and which are the
//! document?
//!
//! ## Markup arrives in two quite different forms
//!
//! **As annotations.** Highlights, notes, ink, stamps: separate objects
//! listed in the page's `/Annots`, which is why a reader lets you click
//! and delete them one at a time. Removing these is bookkeeping.
//!
//! **Already flattened into the page.** Every app that exports
//! annotated PDFs — tablet note-takers, scanner apps, "print to PDF"
//! from a marker tool — writes the pen strokes as one transparent image
//! laid over the whole page and no annotations at all. Open one of
//! those and there is nothing to click: the marks are as much a part of
//! the page as the text under them, and no amount of annotation
//! handling will touch them.
//!
//! The second kind is the one people actually have and cannot get rid
//! of, so this handles both.
//!
//! ## Telling an overlay from the document
//!
//! Dropping the wrong image here empties the page, so the test is
//! narrow. An image is treated as a markup layer only when all three
//! hold:
//!
//! 1. **It has a soft mask.** `/SMask` is a real alpha channel. A page
//!    of content does not need one; a layer drawn *over* content is
//!    exactly what does, because everywhere the pen did not go has to
//!    stay see-through.
//! 2. **It covers the page.** Markup spans the sheet. A photograph
//!    placed in a document does not, so it is left alone.
//! 3. **The page paints something else too.** This is the guard against
//!    the honest mistake: a scan saved as a transparent PNG is a
//!    full-page image with an alpha channel and no markup anywhere near
//!    it. If the candidate is the only thing on the page, the page *is*
//!    the candidate, and it stays.
//!
//! Anything narrower and real markup layers are missed; anything looser
//! and a document gets blanked. What is left over — pen strokes drawn
//! as ordinary vector paths, indistinguishable from a diagram — is not
//! detectable at all, and this does not guess at it.
//!
//! ## What is not markup
//!
//! Links and form-field widgets live in `/Annots` alongside the
//! highlights, and neither is something a person drew. Removing a link
//! breaks navigation; removing a widget destroys the form. Both stay.
//!
//! Hidden markup does not get the same courtesy it gets from
//! flattening, which skips it so that baking it in cannot make it
//! visible. Here the concern runs the other way: a hidden note is still
//! somebody's note, and still readable by anything that ignores the
//! flag, so it goes with the rest.
//!
//! ## Removed, not destroyed
//!
//! This takes markup off the document; it does not promise the file
//! stops carrying it. Like every ordinary edit here the result is saved
//! as an incremental revision, so the previous one — annotations and
//! all — is still in the bytes, which is also what makes the change
//! undoable. That is the right trade for tidying a document.
//!
//! It is the wrong trade when the markup is the sensitive part and the
//! file is going to someone else. Redaction is the tool for that: it
//! rewrites the file rather than appending to it, precisely so nothing
//! survives a revision back.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, ObjectId};
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnmarkError {
    #[error("failed to decode content stream: {0}")]
    ContentDecode(String),
    #[error("failed to encode content stream: {0}")]
    ContentEncode(String),
    #[error(transparent)]
    Doc(#[from] DocError),
}

/// What came off.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Removed {
    /// Annotations deleted — highlights, notes, ink, stamps, and the
    /// popup windows belonging to them.
    pub annotations: usize,
    /// Flattened markup layers dropped from page content.
    pub layers: usize,
}

impl Removed {
    pub fn is_empty(self) -> bool {
        self.annotations == 0 && self.layers == 0
    }
}

/// A markup layer has to cover essentially the whole sheet. Not exactly
/// all of it: exporters routinely inset the overlay by a pixel or place
/// it against the crop box rather than the media box.
const PAGE_COVERAGE: f64 = 0.9;

/// Removes every annotation and flattened markup layer in the document.
///
/// Does not save — call [`Document::save_incremental`] afterwards.
pub fn remove_markup(doc: &mut Document) -> Result<Removed, UnmarkError> {
    let page_count = doc.page_count()?;
    let mut removed = Removed::default();

    for page_index in 0..page_count {
        removed.annotations += remove_annotations(doc, page_index)?;
        removed.layers += remove_layers(doc, page_index)?;
    }
    Ok(removed)
}

fn remove_annotations(doc: &mut Document, page_index: u32) -> Result<usize, UnmarkError> {
    let refs = doc.page_annotation_refs(page_index)?;
    let markup: Vec<ObjectId> = refs
        .into_iter()
        .filter(|id| is_markup_annotation(doc, *id))
        .collect();

    let mut removed = 0usize;
    for id in markup {
        doc.remove_annotation_ref(page_index, id)?;
        removed += 1;
    }
    Ok(removed)
}

/// Everything in `/Annots` except the document's own machinery.
fn is_markup_annotation(doc: &Document, id: ObjectId) -> bool {
    let Ok(dict) = doc.dictionary(id) else {
        // Unreadable, so unclassifiable — and removing something this
        // code cannot describe is not an improvement.
        return false;
    };
    match dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) {
        Some(b"Link") | Some(b"Widget") => false,
        Some(_) => true,
        None => false,
    }
}

/// Drops any full-page transparent overlay image from a page's content.
fn remove_layers(doc: &mut Document, page_index: u32) -> Result<usize, UnmarkError> {
    let media_box = doc.page_media_box(page_index)?;
    let page_area = f64::from(media_box[2] - media_box[0]).abs()
        * f64::from(media_box[3] - media_box[1]).abs();
    if page_area <= 0.0 {
        return Ok(0);
    }

    let resources = doc.page_resources(page_index)?;
    let masked = masked_images(doc, &resources);
    if masked.is_empty() {
        return Ok(0);
    }

    let content = doc.page_content_bytes(page_index)?;
    let survey = survey(&content, &masked, page_area)?;

    // Rule 3: never leave a page with nothing on it. If the overlay is
    // the only thing that paints, it is not an overlay.
    if survey.layers.is_empty() || survey.other_paints == 0 {
        return Ok(0);
    }

    let (bytes, dropped) = drop_draws(&content, &survey.layers)?;
    if dropped == 0 {
        return Ok(0);
    }
    doc.set_page_contents(page_index, bytes)?;
    Ok(survey.layers.len())
}

/// The page's image XObjects that carry a soft mask, by resource name.
fn masked_images(doc: &Document, resources: &Dictionary) -> Vec<Vec<u8>> {
    let entries = match resources.get(b"XObject").map(|o| doc.resolve(o)) {
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => return Vec::new(),
    };

    entries
        .iter()
        .filter(|(_, value)| {
            let Ok(id) = value.as_reference() else {
                return false;
            };
            let Ok(dict) = doc.dictionary_or_stream_dict(id) else {
                return false;
            };
            let is_image = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Image");
            // `/Mask` is the older stencil form of the same idea; both
            // say the same thing about intent.
            is_image && (dict.get(b"SMask").is_ok() || dict.get(b"Mask").is_ok())
        })
        .map(|(name, _)| name.to_vec())
        .collect()
}

struct Survey {
    /// Names of masked images placed across the whole page.
    layers: Vec<Vec<u8>>,
    /// How many other operators paint something.
    other_paints: usize,
}

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

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

fn number(obj: &Object) -> f64 {
    obj.as_float()
        .map(f64::from)
        .unwrap_or_else(|_| obj.as_i64().unwrap_or(0) as f64)
}

/// Walks the page's operators, tracking the transform, to find which of
/// the masked images are laid across the whole page and whether
/// anything else paints.
fn survey(content: &[u8], masked: &[Vec<u8>], page_area: f64) -> Result<Survey, UnmarkError> {
    let decoded =
        Content::decode(content).map_err(|e| UnmarkError::ContentDecode(e.to_string()))?;

    let mut stack: Vec<Matrix> = Vec::new();
    let mut ctm = IDENTITY;
    let mut layers: Vec<Vec<u8>> = Vec::new();
    let mut other_paints = 0usize;

    for op in &decoded.operations {
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => {
                if let Some(m) = stack.pop() {
                    ctm = m;
                }
            }
            "cm" if op.operands.len() == 6 => {
                let m: Matrix = std::array::from_fn(|i| number(&op.operands[i]));
                ctm = multiply(m, ctm);
            }
            "Do" => {
                let name = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                // An image is drawn on the unit square, so the CTM's
                // determinant is the area it covers.
                let area = (ctm[0] * ctm[3] - ctm[1] * ctm[2]).abs();
                let covers_page = area >= page_area * PAGE_COVERAGE;
                if masked.contains(&name) && covers_page {
                    if !layers.contains(&name) {
                        layers.push(name);
                    }
                } else {
                    other_paints += 1;
                }
            }
            // Everything that puts ink on the page. `n` is the
            // path-painting operator that paints nothing.
            "Tj" | "TJ" | "'" | "\"" | "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => {
                other_paints += 1;
            }
            _ => {}
        }
    }

    Ok(Survey {
        layers,
        other_paints,
    })
}

fn drop_draws(content: &[u8], names: &[Vec<u8>]) -> Result<(Vec<u8>, usize), UnmarkError> {
    let decoded =
        Content::decode(content).map_err(|e| UnmarkError::ContentDecode(e.to_string()))?;
    let mut dropped = 0usize;
    let kept: Vec<Operation> = decoded
        .operations
        .into_iter()
        .filter(|op| {
            if op.operator != "Do" {
                return true;
            }
            let named = op
                .operands
                .first()
                .and_then(|o| o.as_name().ok())
                .is_some_and(|n| names.iter().any(|candidate| candidate == n));
            if named {
                dropped += 1;
            }
            !named
        })
        .collect();
    let bytes = Content { operations: kept }
        .encode()
        .map_err(|e| UnmarkError::ContentEncode(e.to_string()))?;
    Ok((bytes, dropped))
}
