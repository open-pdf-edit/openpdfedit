//! Page numbers and Bates numbering.
//!
//! An ordinary mutating command through [`crate::commit_mutation`], so
//! it's undoable and lands in the working copy — the same shape as
//! [`crate::watermark`], which it deliberately sits beside rather than
//! inside: that one tiles a repeating cell across whole pages, this one
//! stamps a single label into a margin and changes it page to page.
//! Neither is expressible in terms of the other.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_numbering::{add_numbering, Anchor, LabelFont, NumberStyle, Numbering};
use serde::Deserialize;

use crate::{commit_mutation, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore};

impl From<openpdfedit_numbering::NumberingError> for SessionError {
    fn from(e: openpdfedit_numbering::NumberingError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

/// Bounds on font size: the floor is where a label stops being legible,
/// the ceiling is well past any plausible page number.
const MIN_FONT_SIZE: f32 = 4.0;
const MAX_FONT_SIZE: f32 = 96.0;

/// Bates numbering is conventionally six digits; past twenty is a typo,
/// not an intent.
const MAX_DIGITS: usize = 20;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberPagesRequest {
    pub handle: DocHandle,
    /// Printed before the number — a case identifier, for Bates.
    pub prefix: String,
    /// Printed after it: `" of 20"`, a suffix code, or nothing.
    pub suffix: String,
    /// What the first numbered page is called. Not necessarily 1: a
    /// volume continuing an earlier one starts where that stopped.
    pub start_at: u64,
    /// Zero-pad to this many digits; 0 leaves the number unpadded.
    pub digits: usize,
    /// `"topLeft"`, `"bottomCenter"`, and so on.
    pub anchor: String,
    /// `"helvetica"`, `"helveticaBold"`, `"timesRoman"`, `"timesBold"`,
    /// `"courier"`.
    pub font: String,
    pub font_size: f32,
    /// `[r, g, b]`, each `0.0..=1.0`.
    pub color: [f32; 3],
    pub opacity: f32,
    /// Distance from the page edge, in points.
    pub margin: f32,
    /// `None` numbers every page. 0-based page indices.
    pub pages: Option<Vec<u32>>,
}

impl NumberPagesRequest {
    fn style(&self) -> NumberStyle {
        NumberStyle {
            anchor: match self.anchor.as_str() {
                "topLeft" => Anchor::TopLeft,
                "topCenter" => Anchor::TopCenter,
                "topRight" => Anchor::TopRight,
                "center" => Anchor::Center,
                "bottomLeft" => Anchor::BottomLeft,
                "bottomRight" => Anchor::BottomRight,
                // Bottom-centre is where a page number goes when nobody
                // said otherwise, so it's also the fallback for an
                // unrecognized name: a label in a slightly unexpected
                // place beats a failed command.
                _ => Anchor::BottomCenter,
            },
            font: match self.font.as_str() {
                "helveticaBold" => LabelFont::HelveticaBold,
                "timesRoman" => LabelFont::TimesRoman,
                "timesBold" => LabelFont::TimesBold,
                "courier" => LabelFont::Courier,
                _ => LabelFont::Helvetica,
            },
            font_size: self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE),
            color: self.color.map(|c| c.clamp(0.0, 1.0)),
            opacity: self.opacity.clamp(0.0, 1.0),
            rotation_degrees: 0.0,
            margin: self.margin.max(0.0),
            shrink_to_fit: true,
        }
    }
}

pub fn number_pages_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: NumberPagesRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let style = request.style();
    let numbering = Numbering {
        prefix: request.prefix.clone(),
        suffix: request.suffix.clone(),
        start_at: request.start_at,
        digits: request.digits.min(MAX_DIGITS),
    };
    let pages = request.pages.clone();

    commit_mutation::<E, SessionError>(engine, docs, history, store, request.handle, |doc| {
        let targets = match &pages {
            Some(explicit) => explicit.clone(),
            None => (0..doc.page_count()?).collect(),
        };
        add_numbering(doc, &targets, &numbering, &style)?;
        Ok(())
    })
}
