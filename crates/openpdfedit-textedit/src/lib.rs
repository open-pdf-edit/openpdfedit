//! MVP in-place editing (PLAN.md M7, "the moat" — scoped way down from
//! that milestone's full ambition; see below).
//!
//! Two operations, both built by composing crates M4/M5 already shipped
//! and tested rather than writing a new engine from scratch:
//!
//! - [`edit_text_run`]: replaces one text-showing operator's content.
//!   Finds the run via [`list_text_runs`] (a content-stream walk that
//!   mirrors `openpdfedit-redact`'s graphics-state tracking, but
//!   *collects* text runs with their exact rendering matrix instead of
//!   dropping overlapping ones), removes the old text by calling
//!   `openpdfedit_redact::redact_content` scoped to that run's bounding
//!   box (exactly the "true removal" primitive M5 already built and
//!   PDFium-verified — reused here, not reimplemented), then appends new
//!   text at the *exact* original rendering matrix (`Tm`, not an
//!   approximate bounding-box reconstruction) using the *same* font
//!   resource the original run referenced (no new font object, no
//!   `/Resources` changes needed).
//! - [`move_image`]: relocates one image/form XObject placement.
//!   Finds it via [`list_image_placements`], removes the old `Do` call
//!   the same way (redaction already drops `Do` operators whose
//!   unit-square-through-CTM bounding box overlaps a target rect — the
//!   exact same code path, just invoked here instead of by the redaction
//!   UI), then re-emits `Do` under a translated `cm`.
//!
//! ## What "the moat" actually needs that this does NOT provide
//!
//! Real in-place text editing — the kind where changing "cat" to
//! "elephant" reflows the surrounding paragraph, respects the embedded
//! font's actual glyph metrics, and re-subsets/merges the font program
//! so only the glyphs actually used are embedded — needs: (1) parsing
//! the embedded font program (TrueType/CFF) for real glyph widths
//! instead of the character-count-times-font-size approximation used
//! here (the same conservative approximation `openpdfedit-redact` uses
//! for its bounding boxes — see that crate's module doc for why it's a
//! safe direction to be wrong in for *redaction*, but it is a genuine
//! accuracy limitation here, not just a safe one: replacement text can
//! visibly overflow or leave a gap versus the original), (2) font
//! subsetting/merging (the `subsetter` crate PLAN.md names) to keep
//! embedded font size from growing unboundedly as edits accumulate, and
//! (3) paragraph-level re-layout (Tier-3 in PLAN.md's own tiering,
//! explicitly "a fast-follow, not v1" even in the full spec). None of
//! that is here. What *is* here: single-run substitution that keeps the
//! new text at the original position and font, with a horizontal-scale
//! (`Tz`) adjustment that approximately compresses/stretches it to the
//! old run's estimated width — a real, spec-legal technique for keeping
//! a like-for-like footprint without real font metrics, not a hack, but
//! also not glyph-accurate. Good enough for "fix a typo, change a date"
//! on runs that are their own complete `Tj`/`TJ` call (the common case
//! for form-filled or simply-laid-out text); not a substitute for actual
//! reflow on runs that are part of a longer wrapped paragraph.

pub mod font;

use std::collections::HashMap;

use lopdf::content::{Content, Operation};
use lopdf::Object;
use openpdfedit_doc::{DocError, Document};

pub use font::FontInfo;
use openpdfedit_redact::{Rect, RedactError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TextEditError {
    #[error("failed to decode content stream: {0}")]
    ContentDecode(String),
    #[error("failed to encode content stream: {0}")]
    ContentEncode(String),
    #[error(transparent)]
    Redact(#[from] RedactError),
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error(
        "this text's font provides no /ToUnicode table, so its glyph ids can't be mapped to \
         characters (see openpdfedit-textedit's font module doc)"
    )]
    RunNotEditable,
    #[error(
        "this document's embedded font subset has no glyph for: {}",
        .0.iter().collect::<String>()
    )]
    UnavailableGlyphs(Vec<char>),
    #[error(
        "this text is no longer where it was on the page — re-open the document and try again"
    )]
    RunNotFound,
}

/// How large a negative `TJ` kerning adjustment (in thousandths of a
/// text-space unit) is read as a word space rather than letter kerning.
/// 120/1000 em sits well above normal inter-letter tightening and below
/// a typical space advance (~250-330/1000 em for common text faces).
const WORD_GAP_KERN_THRESHOLD: f64 = 120.0;

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

fn transform_point(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

fn bbox_of_points(points: &[(f64, f64)]) -> Option<Rect> {
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

fn number(obj: &Object) -> f64 {
    obj.as_float()
        .map(f64::from)
        .unwrap_or_else(|_| obj.as_i64().unwrap_or(0) as f64)
}

/// Estimates how many glyphs a raw `Tj`/`TJ` string operand encodes, for
/// [`text_run_bbox`]'s width approximation. Real-world PDFs commonly
/// embed subsetted fonts with a 2-byte-per-glyph (CID/Identity-H style)
/// encoding rather than one byte per visible character — confirmed on a
/// real resume PDF, where every string operand had this exact shape
/// (every other byte `0x00`). Naively using `text.chars().count()` after
/// lossy-UTF8-decoding those raw bytes roughly *doubles* the apparent
/// character count (each `0x00`/low byte pair decodes as two separate
/// low-codepoint chars, since both bytes are individually valid UTF-8),
/// which was making `text_run_bbox`'s width come out far too wide —
/// confirmed by rendering the actual computed bboxes over the real page
/// and finding several ran well past the page's right edge. Detecting
/// the "every other byte is 0x00" shape and halving the byte count for
/// those strings is a heuristic, not real CMap-aware decoding (that
/// needs the font's actual `/Encoding`/`ToUnicode`, out of scope for this
/// MVP — see the module doc), but it's a meaningfully better estimate
/// than treating every raw byte as its own glyph.
fn estimated_glyph_count(raw: &[u8]) -> usize {
    if looks_cid_encoded(raw) {
        return raw.len() / 2;
    }
    raw.len()
}

/// Whether `raw` looks like 2-byte-per-glyph CID/Identity-H text (the
/// "every other byte is `0x00`" shape described on
/// [`estimated_glyph_count`]).
///
/// This is load-bearing for more than the width estimate: for a
/// CID-encoded run, the bytes in a `Tj`/`TJ` operand are *glyph ids in
/// that font's private subset*, not characters. Two consequences, both
/// of which make such a run genuinely uneditable by this MVP:
/// (1) `String::from_utf8_lossy` over those bytes produces garbage, not
/// the readable text a user would expect to see when editing; and
/// (2) — the real blocker — writing a *replacement* would require
/// mapping each new character back to that subset's glyph id, which
/// needs the embedded font's `/Encoding` CMap plus, for any character
/// the subset doesn't already contain, adding a glyph to the font
/// program itself (subsetting). That's exactly the font work this
/// crate's module doc lists as out of scope. So rather than let a caller
/// silently produce a run of wrong glyphs, [`TextRun::is_editable`]
/// reports `false` and the UI can say so honestly.
fn looks_cid_encoded(raw: &[u8]) -> bool {
    if raw.len() < 2 || !raw.len().is_multiple_of(2) {
        return false;
    }
    let zero_even_bytes = raw.iter().step_by(2).filter(|&&b| b == 0).count();
    zero_even_bytes * 2 >= raw.len()
}

fn text_run_bbox(matrix: Matrix, font_size: f64, width: f64) -> Option<Rect> {
    let height = font_size.max(1.0);
    let corners = [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)];
    let points: Vec<(f64, f64)> = corners
        .iter()
        .map(|&(x, y)| transform_point(matrix, x, y))
        .collect();
    bbox_of_points(&points)
}

/// The fill colour in effect when a run was drawn, captured as the
/// *exact* content-stream operators that established it rather than as a
/// decoded RGB triple.
///
/// Replaying the original operators verbatim is what makes this correct
/// for colour spaces this crate can't itself interpret. A page that says
/// `/CS0 cs 0.9 0.2 0.1 scn` is selecting a colour from a resource-
/// defined space (ICCBased, Separation, Indexed, …); resolving that to
/// device RGB would mean implementing colour management, but re-emitting
/// the same two operators reproduces it exactly, because the `/CS0`
/// resource is still on the page.
///
/// Why this exists at all: replacement text was previously appended with
/// no colour operator, so it always drew in the PDF default — pure black.
/// White text on a dark banner therefore became black-on-dark
/// (indistinguishable from having vanished), and coloured headings
/// silently turned black on edit.
#[derive(Debug, Clone, Default)]
pub struct FillColor {
    /// A `cs` (set colour space) operator, when one is in effect.
    colorspace: Option<Operation>,
    /// The operator that set the colour itself: `g`/`rg`/`k`/`sc`/`scn`.
    color: Option<Operation>,
    /// A cheap value-equality key — `lopdf::Operation` is not `PartialEq`,
    /// and [`merge_adjacent_runs`] needs to know whether two runs are the
    /// same colour before folding them into one editable unit.
    fingerprint: String,
}

impl FillColor {
    /// A `cs` operator. Clears any previously selected colour: per the
    /// spec, changing colour space resets the current colour to that
    /// space's initial value, and the component counts wouldn't match
    /// anyway.
    fn set_colorspace(&mut self, operands: Vec<Object>) {
        self.colorspace = Some(Operation::new("cs", operands));
        self.color = None;
        self.refresh();
    }

    /// `g`/`rg`/`k` name their own device colour space, so any pending
    /// `cs` no longer applies; `sc`/`scn` select within the current `cs`
    /// and must keep it.
    fn set_color(&mut self, operator: &str, operands: Vec<Object>) {
        if matches!(operator, "g" | "rg" | "k") {
            self.colorspace = None;
        }
        self.color = Some(Operation::new(operator, operands));
        self.refresh();
    }

    fn refresh(&mut self) {
        self.fingerprint = self
            .ops()
            .iter()
            .map(|op| format!("{} {:?};", op.operator, op.operands))
            .collect();
    }

    /// The operators to replay to restore this colour, in order. Empty
    /// means "the page never set one", and the PDF default (black) is
    /// already what an appended block starts from — so emitting nothing
    /// is correct, not a fallback.
    fn ops(&self) -> Vec<Operation> {
        self.colorspace
            .iter()
            .chain(self.color.iter())
            .cloned()
            .collect()
    }
}

/// One contiguous text-showing operator's payload, kept verbatim so
/// [`move_text_run`] can re-emit the run *exactly* — same glyph codes,
/// same intra-run kerning — instead of round-tripping through decoded
/// text and re-encoding it (which would lose kerning and could fail
/// outright for glyphs a subset font can't re-encode).
#[derive(Debug, Clone)]
struct RunSegment {
    matrix: Matrix,
    /// This operator's position in the page's decoded operation list —
    /// what lets [`remove_run_operators`] delete *precisely this run* and
    /// nothing else. See that function's doc for why identifying the run
    /// by index beats identifying it by the rectangle it occupies.
    op_index: usize,
    /// The operator that showed this text (`Tj`, `TJ`, `'`, `"`), kept so
    /// removal can both verify it is removing what it thinks it is and
    /// preserve the operator's non-drawing side effects.
    operator: String,
    /// The `TJ` kerning adjustment that reproduces this segment's exact
    /// horizontal advance without drawing anything — emitted in the
    /// removed operator's place so that any text positioned by the
    /// implicit advance (rather than by its own `Td`/`Tm`) stays put.
    /// `None` when the font supplied no real widths to compute it from.
    advance_kern: Option<f64>,
    /// A `TJ`-style element list: strings interleaved with numeric
    /// kerning adjustments. A `Tj` becomes a single-element list, which
    /// `TJ` renders identically.
    elements: Vec<Object>,
}

/// Text-state parameters that affect how a run is painted and therefore
/// have to be re-established when it's rewritten or relocated. Without
/// these, an edited run silently lost its colour, its letter/word
/// spacing, and its render mode (so an invisible OCR layer would become
/// visible, and outlined text would fill solid).
#[derive(Debug, Clone)]
pub struct TextStyle {
    pub fill: FillColor,
    /// `Tr` — 0 fill, 1 stroke, 3 invisible, 7 clip, etc.
    pub render_mode: i64,
    /// `Tc`, in unscaled text-space units.
    pub char_spacing: f64,
    /// `Tw`, in unscaled text-space units.
    pub word_spacing: f64,
    /// `Tz`, as a percentage (100 = normal).
    pub horiz_scale: f64,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            fill: FillColor::default(),
            render_mode: 0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horiz_scale: 100.0,
        }
    }
}

impl TextStyle {
    fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.fill.fingerprint,
            self.render_mode,
            self.char_spacing,
            self.word_spacing,
            self.horiz_scale
        )
    }

    /// The operators that re-establish this style, for emission inside a
    /// `BT`/`ET` block. Parameters at their default value are omitted —
    /// an appended block already starts from the defaults.
    fn text_state_ops(&self) -> Vec<Operation> {
        let mut ops = Vec::new();
        if self.render_mode != 0 {
            ops.push(Operation::new("Tr", vec![self.render_mode.into()]));
        }
        if self.char_spacing != 0.0 {
            ops.push(Operation::new("Tc", vec![self.char_spacing.into()]));
        }
        if self.word_spacing != 0.0 {
            ops.push(Operation::new("Tw", vec![self.word_spacing.into()]));
        }
        if (self.horiz_scale - 100.0).abs() > 1e-6 {
            ops.push(Operation::new("Tz", vec![self.horiz_scale.into()]));
        }
        ops
    }
}

/// One text-showing operator (`Tj`/`TJ`/`'`/`"`), as found by
/// [`list_text_runs`]. `bbox` is the same character-count-based
/// approximation `openpdfedit-redact` uses; `render_matrix` (not public
/// — only [`edit_text_run`] needs it) is the *exact* text-rendering
/// matrix in effect, so a replacement can be positioned precisely rather
/// than reconstructed from the approximate box.
#[derive(Debug, Clone)]
pub struct TextRun {
    pub page_index: u32,
    pub text: String,
    pub bbox: Rect,
    /// The page's `/Resources`/`/Font` key this run's `Tf` call named
    /// (e.g. `"F1"`) — reused as-is for the replacement, so no new font
    /// resource is needed.
    pub font_name: String,
    pub font_size: f64,
    /// `false` when this run's bytes are 2-byte CID/Identity-H glyph ids
    /// rather than characters — see [`looks_cid_encoded`] for why such a
    /// run can be *located* (its position and extent are still known)
    /// but not meaningfully read or rewritten by this MVP. `text` is
    /// unreliable garbage in that case, and [`edit_text_run`] would
    /// produce wrong glyphs, so callers should refuse to edit it rather
    /// than presenting it as editable.
    pub is_editable: bool,
    /// Colour and text-state parameters in effect, so a rewrite or a move
    /// can reproduce the run's appearance rather than resetting it to
    /// plain black.
    pub style: TextStyle,
    render_matrix: Matrix,
    segments: Vec<RunSegment>,
}

/// The decoded form of one text-showing operator's string operand(s):
/// what to display, how many glyphs it's estimated to be (for the width
/// approximation), and whether it's encoded in a way this MVP can
/// actually rewrite. Bundled into one struct rather than three
/// parallel arguments purely to keep `push_run`'s signature manageable.
struct ShownText {
    text: String,
    glyph_count: usize,
    is_editable: bool,
    /// The run's true width in text-space units at font size 1, when the
    /// font supplied real glyph widths. `None` falls back to the
    /// character-count estimate.
    width: Option<f64>,
}

impl ShownText {
    /// From a single `Tj`/`'`/`"` string operand, decoded through `font`
    /// when one is known for the active `Tf` resource.
    fn from_raw(raw: &[u8], font: Option<&FontInfo>) -> Self {
        match font {
            Some(font) => Self {
                text: font.decode(raw),
                glyph_count: font.glyph_count(raw),
                is_editable: font.can_encode_text(),
                width: font.raw_width(raw),
            },
            // No font resource resolved (a malformed page, or the
            // font-unaware `list_text_runs` entry point): fall back to
            // the byte-shape heuristic.
            None => Self {
                text: String::from_utf8_lossy(raw).into_owned(),
                glyph_count: estimated_glyph_count(raw),
                is_editable: !looks_cid_encoded(raw),
                width: None,
            },
        }
    }

    /// From a `TJ` array's string elements (its numeric kerning elements
    /// carry no glyphs and are skipped). Editable only if *every* piece
    /// is — a run mixing encodings can't be safely rewritten wholesale.
    fn from_tj_array(arr: &[Object], font: Option<&FontInfo>) -> Self {
        let mut text = String::new();
        let mut glyph_count = 0usize;
        let mut is_editable = true;
        let mut width = Some(0.0);
        let mut saw_any = false;
        for o in arr {
            if let Ok(raw) = o.as_str() {
                saw_any = true;
                let piece = Self::from_raw(raw, font);
                text.push_str(&piece.text);
                glyph_count += piece.glyph_count;
                is_editable &= piece.is_editable;
                width = match (width, piece.width) {
                    (Some(acc), Some(w)) => Some(acc + w),
                    _ => None,
                };
                continue;
            }
            // A numeric element is a kerning adjustment, subtracted from
            // the current position in thousandths of a text-space unit.
            // Typesetters routinely render *word spacing* this way
            // instead of emitting a space glyph, so a sufficiently large
            // negative jump has to be read back as a space or the
            // extracted text comes out as "DeJianK." — confirmed on a
            // real resume PDF. The threshold is a heuristic: real
            // inter-letter kerning is a handful of units, while a word
            // gap is a sizeable fraction of an em.
            let adjustment = number(o);
            if adjustment <= -WORD_GAP_KERN_THRESHOLD && !text.ends_with(' ') && !text.is_empty() {
                text.push(' ');
            }
            if let (Some(acc), true) = (width, adjustment != 0.0) {
                width = Some(acc - adjustment / 1000.0);
            }
        }
        Self {
            text,
            glyph_count,
            is_editable: is_editable && saw_any,
            width,
        }
    }
}

/// Walks a page's content stream and returns every text-showing
/// operator's run, in document order, decoding text through the page's
/// own font resources. **This is the entry point callers should use**:
/// without the font information, text in an embedded subset font (which
/// is to say, text in most real-world PDFs) decodes to garbage and can't
/// be rewritten — see this crate's `font` module doc.
pub fn list_text_runs_in_page(
    doc: &Document,
    page_index: u32,
) -> Result<Vec<TextRun>, TextEditError> {
    let content = doc.page_content_bytes(page_index)?;
    let fonts = font::page_fonts(doc, page_index);
    let runs = list_text_runs_with_fonts(page_index, &content, &fonts)?;
    Ok(merge_adjacent_runs(runs))
}

/// Joins consecutive runs that are visually one piece of text back into
/// a single editable unit.
///
/// PDF producers are free to split a line across as many text-showing
/// operators as they like, and they do: Google Docs emits **one `Tj` per
/// character**, so without this a user clicking a word to edit it would
/// only be offered the single letter they happened to hit. Merging
/// restores the unit people actually think in terms of.
///
/// Runs merge only when they're unambiguously continuous: same font
/// resource and size, same text orientation and baseline, and the next
/// run starting at (or very near) where the previous one ended. A gap
/// wider than a fraction of the font size becomes a space rather than a
/// merge-breaker, since that's how word spacing is often encoded.
fn merge_adjacent_runs(runs: Vec<TextRun>) -> Vec<TextRun> {
    let mut merged: Vec<TextRun> = Vec::with_capacity(runs.len());
    for run in runs {
        let Some(prev) = merged.last_mut() else {
            merged.push(run);
            continue;
        };

        // Colour and text state are part of "same style": merging a
        // white run into a black one would make the whole merged line
        // rewrite in whichever colour happened to come first, quietly
        // recolouring text the user never touched.
        let same_style = prev.font_name == run.font_name
            && (prev.font_size - run.font_size).abs() < 0.01
            && prev.is_editable == run.is_editable
            && prev.style.fingerprint() == run.style.fingerprint();
        // Same orientation and baseline: the matrix's a/b/c/d must match
        // and the translation's y must be level. Comparing the rendering
        // matrix (not the bbox) is what keeps rotated or differently
        // scaled text from being folded together.
        let same_baseline = prev.render_matrix[..4]
            .iter()
            .zip(&run.render_matrix[..4])
            .all(|(a, b)| (a - b).abs() < 1e-6)
            && (prev.render_matrix[5] - run.render_matrix[5]).abs() < 0.01;

        let gap = run.bbox.x0 - prev.bbox.x1;
        let space_width = prev.font_size.max(1.0) * 0.5;
        let adjacent = gap >= -space_width && gap <= space_width;

        if !(same_style && same_baseline && adjacent) {
            merged.push(run);
            continue;
        }

        // A visible gap between the two means a word break.
        if gap > prev.font_size.max(1.0) * 0.12
            && !prev.text.ends_with(' ')
            && !run.text.starts_with(' ')
        {
            prev.text.push(' ');
        }
        prev.text.push_str(&run.text);
        prev.bbox.x1 = prev.bbox.x1.max(run.bbox.x1);
        prev.bbox.y0 = prev.bbox.y0.min(run.bbox.y0);
        prev.bbox.y1 = prev.bbox.y1.max(run.bbox.y1);
        // Each segment keeps its own matrix, so a merged line can be
        // relocated piece-by-piece with its original spacing intact —
        // no need to reconstruct the gaps that merging turned into
        // spaces in `text`.
        prev.segments.extend(run.segments);
    }
    merged
}

/// Font-unaware variant, kept for callers that only have raw content
/// bytes. Falls back to a byte-shape heuristic for encoding detection,
/// so runs in CID fonts come back with unreliable `text` and
/// `is_editable == false`. Prefer [`list_text_runs_in_page`].
pub fn list_text_runs(
    page_index: u32,
    content_bytes: &[u8],
) -> Result<Vec<TextRun>, TextEditError> {
    list_text_runs_with_fonts(page_index, content_bytes, &HashMap::new())
}

fn list_text_runs_with_fonts(
    page_index: u32,
    content_bytes: &[u8],
    fonts: &HashMap<String, FontInfo>,
) -> Result<Vec<TextRun>, TextEditError> {
    let content =
        Content::decode(content_bytes).map_err(|e| TextEditError::ContentDecode(e.to_string()))?;

    // `q`/`Q` save and restore the *whole* graphics state, which under
    // the spec includes the text-state parameters (`Tf`/`Tc`/`Tw`/`Tz`/
    // `Tr`) and the current colour, not just the CTM. Tracking only the
    // CTM meant a run inside a `q`...`Q` block reported whatever colour
    // leaked in from outside it.
    #[derive(Clone)]
    struct GState {
        ctm: Matrix,
        style: TextStyle,
        font_name: String,
        font_size: f64,
        leading: f64,
    }

    let mut stack: Vec<GState> = Vec::new();
    let mut gs = GState {
        ctm: IDENTITY,
        style: TextStyle::default(),
        font_name: String::new(),
        font_size: 0.0,
        leading: 0.0,
    };
    let mut text_matrix: Matrix = IDENTITY;
    let mut text_line_matrix: Matrix = IDENTITY;

    let mut runs = Vec::new();

    // Takes the whole graphics state rather than its pieces: the font,
    // its size and the text style all come from the same place and are
    // all needed, so threading them individually just made a long
    // parameter list.
    fn push_run(
        runs: &mut Vec<TextRun>,
        page_index: u32,
        shown: ShownText,
        combined: Matrix,
        gs: &GState,
        source: (usize, &str, Vec<Object>),
    ) {
        let (op_index, operator, elements) = source;
        let (font_name, font_size, style) = (&gs.font_name, gs.font_size, &gs.style);
        let ShownText {
            text,
            glyph_count,
            is_editable,
            width,
        } = shown;
        if glyph_count == 0 || text.is_empty() {
            return;
        }
        let advance = match width {
            Some(w) => font_size.max(1.0) * w,
            None => font_size.max(1.0) * glyph_count as f64 * 0.5,
        } * (style.horiz_scale / 100.0);
        // A `TJ` kern of `k` displaces by `-k/1000 * font_size` (times
        // the horizontal scale, which cancels out since the original was
        // subject to it too). `width` is already in glyph-space units, so
        // the character- and word-spacing contributions — which are in
        // *text*-space points — have to be divided by the font size to
        // land in the same units.
        let advance_kern = width.filter(|_| font_size > 0.0).map(|w| {
            let spaces = text.chars().filter(|c| *c == ' ').count() as f64;
            let spacing =
                (glyph_count as f64 * style.char_spacing + spaces * style.word_spacing) / font_size;
            -(w + spacing) * 1000.0
        });
        if let Some(bbox) = text_run_bbox(combined, font_size, advance) {
            runs.push(TextRun {
                page_index,
                text,
                bbox,
                is_editable,
                font_name: font_name.to_string(),
                font_size,
                style: style.clone(),
                render_matrix: combined,
                segments: vec![RunSegment {
                    matrix: combined,
                    op_index,
                    operator: operator.to_string(),
                    advance_kern,
                    elements,
                }],
            });
        }
    }

    for (op_index, op) in content.operations.into_iter().enumerate() {
        match op.operator.as_str() {
            "q" => stack.push(gs.clone()),
            "Q" => {
                if let Some(saved) = stack.pop() {
                    gs = saved;
                }
            }
            "cs" => gs.style.fill.set_colorspace(op.operands.clone()),
            "g" | "rg" | "k" | "sc" | "scn" => {
                // A pattern fill (`/P0 scn`) names a resource rather than
                // giving colour components; replaying it verbatim is
                // still correct, so no special case is needed here.
                gs.style.fill.set_color(&op.operator, op.operands.clone());
            }
            "Tr" => {
                if let Some(v) = op.operands.first() {
                    gs.style.render_mode = number(v) as i64;
                }
            }
            "Tc" => {
                if let Some(v) = op.operands.first() {
                    gs.style.char_spacing = number(v);
                }
            }
            "Tw" => {
                if let Some(v) = op.operands.first() {
                    gs.style.word_spacing = number(v);
                }
            }
            "Tz" => {
                if let Some(v) = op.operands.first() {
                    gs.style.horiz_scale = number(v);
                }
            }
            "cm" => {
                if op.operands.len() == 6 {
                    let m: Matrix = std::array::from_fn(|i| number(&op.operands[i]));
                    gs.ctm = multiply(m, gs.ctm);
                }
            }
            "BT" => {
                text_matrix = IDENTITY;
                text_line_matrix = IDENTITY;
            }
            "Tf" => {
                if op.operands.len() == 2 {
                    gs.font_name = op.operands[0]
                        .as_name()
                        .map(|n| String::from_utf8_lossy(n).into_owned())
                        .unwrap_or_default();
                    gs.font_size = number(&op.operands[1]);
                }
            }
            "Tm" => {
                if op.operands.len() == 6 {
                    let m: Matrix = std::array::from_fn(|i| number(&op.operands[i]));
                    text_matrix = m;
                    text_line_matrix = m;
                }
            }
            "Td" | "TD" => {
                if op.operands.len() == 2 {
                    let tx = number(&op.operands[0]);
                    let ty = number(&op.operands[1]);
                    if op.operator == "TD" {
                        gs.leading = -ty;
                    }
                    text_line_matrix = multiply([1.0, 0.0, 0.0, 1.0, tx, ty], text_line_matrix);
                    text_matrix = text_line_matrix;
                }
            }
            "T*" => {
                text_line_matrix =
                    multiply([1.0, 0.0, 0.0, 1.0, 0.0, -gs.leading], text_line_matrix);
                text_matrix = text_line_matrix;
            }
            "TL" => {
                if let Some(v) = op.operands.first() {
                    gs.leading = number(v);
                }
            }
            "Tj" => {
                if let Some(raw) = op.operands.first().and_then(|o| o.as_str().ok()) {
                    push_run(
                        &mut runs,
                        page_index,
                        ShownText::from_raw(raw, fonts.get(&gs.font_name)),
                        multiply(text_matrix, gs.ctm),
                        &gs,
                        (op_index, "Tj", vec![op.operands[0].clone()]),
                    );
                }
            }
            "'" => {
                text_line_matrix =
                    multiply([1.0, 0.0, 0.0, 1.0, 0.0, -gs.leading], text_line_matrix);
                text_matrix = text_line_matrix;
                if let Some(raw) = op.operands.first().and_then(|o| o.as_str().ok()) {
                    push_run(
                        &mut runs,
                        page_index,
                        ShownText::from_raw(raw, fonts.get(&gs.font_name)),
                        multiply(text_matrix, gs.ctm),
                        &gs,
                        (op_index, "'", vec![op.operands[0].clone()]),
                    );
                }
            }
            "\"" => {
                text_line_matrix =
                    multiply([1.0, 0.0, 0.0, 1.0, 0.0, -gs.leading], text_line_matrix);
                text_matrix = text_line_matrix;
                if let Some(raw) = op.operands.get(2).and_then(|o| o.as_str().ok()) {
                    push_run(
                        &mut runs,
                        page_index,
                        ShownText::from_raw(raw, fonts.get(&gs.font_name)),
                        multiply(text_matrix, gs.ctm),
                        &gs,
                        (op_index, "\"", vec![op.operands[2].clone()]),
                    );
                }
            }
            "TJ" => {
                if let Some(arr) = op.operands.first().and_then(|o| o.as_array().ok()) {
                    let elements = arr.clone();
                    push_run(
                        &mut runs,
                        page_index,
                        ShownText::from_tj_array(arr, fonts.get(&gs.font_name)),
                        multiply(text_matrix, gs.ctm),
                        &gs,
                        (op_index, "TJ", elements),
                    );
                }
            }
            _ => {}
        }
    }

    Ok(runs)
}

/// The standard, always-available face used when a run's own embedded
/// font can't represent the replacement text — see [`edit_text_run`].
/// Picks the base-14 Helvetica variant matching the original font's own
/// weight/style (see `font::detect_weight_and_style`), so a fallback edit
/// on a bold heading stays bold instead of silently landing in plain
/// Helvetica — reported from real use: "I edit Bold text and it comes
/// back un-bold," which is exactly what an unconditional plain-Helvetica
/// fallback does every time it triggers, and headings/short labels are
/// disproportionately likely to trigger it (their subset only ever
/// embedded the few glyphs the original heading used).
fn fallback_base_font(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (true, true) => "Helvetica-BoldOblique",
        (true, false) => "Helvetica-Bold",
        (false, true) => "Helvetica-Oblique",
        (false, false) => "Helvetica",
    }
}

/// Encodes text for a `WinAnsiEncoding` simple font. WinAnsi matches
/// Latin-1 across the range that matters here, so single-byte codepoints
/// pass through directly; anything outside it has no encodable form in a
/// base-14 font and becomes `?` rather than silently vanishing.
fn to_winansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
        .collect()
}

/// Appended content must not inherit the page's leftover graphics state.
///
/// A page's content stream is free to apply a transform at top level
/// without wrapping it in `q`/`Q` — and real producers do: Google Docs
/// exports open with a bare `1 0 0 -1 0 792 cm`, a full-page vertical
/// flip that stays in effect for the entire stream. Anything appended
/// after that inherits the flip, so replacement text drawn at its own
/// correct page coordinates lands mirrored (observed: a run at y=604
/// reappearing at y=188 on a 792pt page, i.e. exactly `792 - y`), and a
/// repositioned image comes out upside down.
///
/// Wrapping everything that came before in `q` ... `Q` pops whatever
/// state it left behind, so the appended block starts from the page's
/// initial CTM — which is the space `render_matrix`/`ctm` were captured
/// in. Adding a balanced `q`/`Q` pair around a whole content stream is
/// always legal and never changes how the original content renders.
fn append_in_clean_state(existing: Vec<u8>, addition: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(existing.len() + addition.len() + 8);
    bytes.extend_from_slice(b"q\n");
    bytes.extend(existing);
    bytes.extend_from_slice(b"\nQ\n");
    bytes.extend(addition);
    bytes
}

/// Deletes exactly the text-showing operators that make up `run`,
/// leaving every other byte of the content stream alone.
///
/// This replaced an earlier approach that removed *everything overlapping
/// the run's bounding rectangle* by calling into `openpdfedit-redact`.
/// That was wrong in a way that got worse the more real the document:
/// the redaction crate estimates a run's width as
/// `byte_count * 0.5 * font_size`, and for a CID/Identity-H font — which
/// is to say, most real PDFs — the byte count is *twice* the glyph count.
/// Neighbouring text therefore reported a bounding box around twice its
/// true width, overlapped a rectangle it was nowhere near, and was
/// deleted wholesale. Reported from real use as editing one banner
/// wiping out the text in another banner beside it.
///
/// Widening the estimate or demanding a larger overlap fraction would
/// only have made the collateral damage rarer, not impossible. Since the
/// run being edited is already known operator-by-operator (see
/// [`RunSegment`]), there is no need to infer it from geometry at all.
///
/// Two things are preserved where the operators used to be:
///
/// - **Non-drawing side effects.** `'` advances to the next line before
///   showing text, and `"` also sets word and character spacing. Those
///   effects belong to the surrounding text block, not to the run, so
///   they are re-emitted; dropping them would shift every following line.
/// - **The horizontal advance,** as a `TJ` kern with no glyphs in it, so
///   that text positioned by the implicit advance rather than by its own
///   `Td`/`Tm` does not slide left into the gap.
fn remove_run_operators(content_bytes: &[u8], run: &TextRun) -> Result<Vec<u8>, TextEditError> {
    let content =
        Content::decode(content_bytes).map_err(|e| TextEditError::ContentDecode(e.to_string()))?;

    let targets: HashMap<usize, &RunSegment> =
        run.segments.iter().map(|s| (s.op_index, s)).collect();

    let mut kept: Vec<Operation> = Vec::with_capacity(content.operations.len() + targets.len());
    let mut removed = 0usize;
    for (index, op) in content.operations.into_iter().enumerate() {
        let Some(segment) = targets.get(&index) else {
            kept.push(op);
            continue;
        };
        // The indices came from a walk of this same content stream, so a
        // mismatch means the document changed underneath us. Keeping the
        // operator is the safe response: the caller's replacement text
        // will be appended anyway, so the worst case is a visible
        // duplicate rather than a silently deleted paragraph.
        if op.operator != segment.operator {
            kept.push(op);
            continue;
        }
        removed += 1;

        match op.operator.as_str() {
            "'" => kept.push(Operation::new("T*", vec![])),
            "\"" => {
                if op.operands.len() >= 2 {
                    kept.push(Operation::new("Tw", vec![op.operands[0].clone()]));
                    kept.push(Operation::new("Tc", vec![op.operands[1].clone()]));
                }
                kept.push(Operation::new("T*", vec![]));
            }
            _ => {}
        }
        if let Some(kern) = segment.advance_kern {
            kept.push(Operation::new("TJ", vec![Object::Array(vec![kern.into()])]));
        }
    }

    if removed == 0 {
        return Err(TextEditError::RunNotFound);
    }

    Content { operations: kept }
        .encode()
        .map_err(|e| TextEditError::ContentEncode(e.to_string()))
}

/// Replaces `run`'s text with `new_text`, in place at `run`'s exact
/// original position and font. Removes the old run via
/// `openpdfedit_redact::redact_content` (true removal, not a visual
/// cover — see that crate's module doc), then appends the new text with
/// a `Tz` horizontal-scale adjustment that approximately matches the old
/// run's estimated width (see this crate's module doc for why that's an
/// approximation, not glyph-accurate re-layout). Does not save — call
/// [`Document::save_incremental`] afterwards.
pub fn edit_text_run(
    doc: &mut Document,
    run: &TextRun,
    new_text: &str,
) -> Result<(), TextEditError> {
    // Encode the replacement into whatever this font actually expects:
    // raw bytes for an ordinary single-byte font, 2-byte glyph ids for a
    // CID/subset font (resolved through its `/ToUnicode` table — see the
    // `font` module doc). Doing this here rather than trusting the
    // caller means the invariant holds for every entry point, and the
    // failure is specific: exactly which characters the embedded subset
    // has no glyph for.
    let font = font::page_fonts(doc, run.page_index)
        .remove(&run.font_name)
        .unwrap_or_default();

    // Try the run's own font first, so an edit that *can* keep the
    // original typeface does. When it can't — because an embedded subset
    // carries only the glyphs that font already used, and the
    // replacement needs one it doesn't have (a heading face may contain
    // no `b` at all) — fall back to a standard, non-embedded font rather
    // than refusing the edit. The base-14 fonts need no font program, so
    // this always works; the cost is that the replaced run may not match
    // its neighbours' typeface, which is a visible difference the user
    // can see and judge, not a dead end. Refusing was the wrong call:
    // it made ordinary edits fail on ordinary letters.
    let (write_font_name, encoded) = match font.can_encode_text().then(|| font.encode(new_text)) {
        Some(Ok(bytes)) => (run.font_name.clone(), bytes),
        _ => {
            let base_font = fallback_base_font(font.bold, font.italic);
            let fallback = doc.ensure_page_font(run.page_index, base_font)?;
            (fallback, to_winansi(new_text))
        }
    };

    let original = doc.page_content_bytes(run.page_index)?;
    // Remove precisely this run's own operators — not everything inside
    // its bounding box. See `remove_run_operators` for what went wrong
    // with the rectangle-based approach.
    let without_old = remove_run_operators(&original, run)?;

    // Deliberately NO horizontal-scale (`Tz`) stretching.
    //
    // An earlier pass squeezed/stretched the replacement to occupy the
    // original run's exact width. That made sense when a "run" was a
    // short fragment, but runs are now merged into whole lines, so
    // replacing a full sentence with one word stretched that word across
    // the entire line — visually grotesque and nothing like what the
    // user asked for. Rendering at the font's natural width is both what
    // people expect from "replace this text with that text" and, now
    // that real `/W` glyph metrics are used, genuinely accurate.
    // Replacement text longer than the original can overflow its old
    // footprint; that is the honest trade and is preferable to distortion.
    let mut ops = vec![Operation::new("q", vec![])];
    // Restore the colour the run was drawn in *before* `BT`. Colour is a
    // general graphics-state parameter, not a text-state one, so it is
    // equally legal either side of `BT` — but putting it first keeps the
    // emitted block readable as "set up the state, then draw".
    ops.extend(run.style.fill.ops());
    ops.push(Operation::new("BT", vec![]));
    ops.extend(run.style.text_state_ops());
    ops.extend([
        Operation::new(
            "Tf",
            vec![write_font_name.as_str().into(), run.font_size.into()],
        ),
        Operation::new("Tm", run.render_matrix.iter().map(|&v| v.into()).collect()),
        Operation::new(
            "Tj",
            vec![Object::String(encoded, lopdf::StringFormat::Hexadecimal)],
        ),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ]);
    let new_ops_bytes = Content { operations: ops }
        .encode()
        .map_err(|e| TextEditError::ContentEncode(e.to_string()))?;

    let bytes = append_in_clean_state(without_old, new_ops_bytes);
    doc.set_page_contents(run.page_index, bytes)?;
    Ok(())
}

/// Translates `run` by `(dx, dy)` in page-space points, leaving its
/// content untouched.
///
/// Unlike [`edit_text_run`], this never re-encodes anything: each of the
/// run's original text-showing operators is replayed verbatim under a
/// translated `Tm`, so the glyphs, their intra-run kerning, the font, the
/// colour and the text state all survive exactly. That also means a run
/// whose font has no `/ToUnicode` table — one [`edit_text_run`] must
/// refuse — can still be moved: relocating text doesn't require being
/// able to read it.
///
/// Does not save — call [`Document::save_incremental`] afterwards.
pub fn move_text_run(
    doc: &mut Document,
    run: &TextRun,
    dx: f64,
    dy: f64,
) -> Result<(), TextEditError> {
    let original = doc.page_content_bytes(run.page_index)?;
    // Exactly this run's operators, same as the edit path — moving one
    // line must not disturb whatever happens to share its bounding box.
    let without_old = remove_run_operators(&original, run)?;

    let mut ops = vec![Operation::new("q", vec![])];
    ops.extend(run.style.fill.ops());
    ops.push(Operation::new("BT", vec![]));
    ops.extend(run.style.text_state_ops());
    ops.push(Operation::new(
        "Tf",
        vec![run.font_name.as_str().into(), run.font_size.into()],
    ));
    for segment in &run.segments {
        // Post-multiplying by the translation shifts the result in the
        // *outer* space, independently of whatever scale or rotation the
        // segment's own matrix applies — the same composition
        // `move_image` uses.
        let translated = multiply(segment.matrix, [1.0, 0.0, 0.0, 1.0, dx, dy]);
        ops.push(Operation::new(
            "Tm",
            translated.iter().map(|&v| v.into()).collect(),
        ));
        // `TJ` with a single string element renders identically to `Tj`,
        // so both operator shapes collapse to one emission path.
        ops.push(Operation::new(
            "TJ",
            vec![Object::Array(segment.elements.clone())],
        ));
    }
    ops.push(Operation::new("ET", vec![]));
    ops.push(Operation::new("Q", vec![]));

    let new_ops_bytes = Content { operations: ops }
        .encode()
        .map_err(|e| TextEditError::ContentEncode(e.to_string()))?;

    let bytes = append_in_clean_state(without_old, new_ops_bytes);
    doc.set_page_contents(run.page_index, bytes)?;
    Ok(())
}

/// One `Do` (image/form XObject) placement, as found by
/// [`list_image_placements`].
#[derive(Debug, Clone)]
pub struct ImagePlacement {
    pub page_index: u32,
    /// The page's `/Resources`/`/XObject` key this `Do` call named.
    pub xobject_name: String,
    /// The unit-square-through-CTM bounding box — see
    /// `openpdfedit-redact`'s module doc on the `Do` convention this
    /// relies on.
    pub bbox: Rect,
    ctm: Matrix,
    /// This `Do` call's position in the decoded operation list, so
    /// [`move_image`] can remove exactly this placement — the same
    /// precision [`remove_run_operators`] gives text, and for the same
    /// reason: a logo sitting on top of a background image overlaps it,
    /// and a rectangle-based removal would delete both.
    op_index: usize,
}

/// Walks `content_bytes` and returns every `Do` call's placement, in
/// document order.
pub fn list_image_placements(
    page_index: u32,
    content_bytes: &[u8],
) -> Result<Vec<ImagePlacement>, TextEditError> {
    let content =
        Content::decode(content_bytes).map_err(|e| TextEditError::ContentDecode(e.to_string()))?;

    let mut ctm_stack: Vec<Matrix> = Vec::new();
    let mut ctm: Matrix = IDENTITY;
    let mut placements = Vec::new();

    for (op_index, op) in content.operations.into_iter().enumerate() {
        match op.operator.as_str() {
            "q" => ctm_stack.push(ctm),
            "Q" => {
                if let Some(m) = ctm_stack.pop() {
                    ctm = m;
                }
            }
            "cm" => {
                if op.operands.len() == 6 {
                    let m: Matrix = std::array::from_fn(|i| number(&op.operands[i]));
                    ctm = multiply(m, ctm);
                }
            }
            "Do" => {
                if let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) {
                    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
                    let points: Vec<(f64, f64)> = corners
                        .iter()
                        .map(|&(x, y)| transform_point(ctm, x, y))
                        .collect();
                    if let Some(bbox) = bbox_of_points(&points) {
                        placements.push(ImagePlacement {
                            page_index,
                            xobject_name: String::from_utf8_lossy(name).into_owned(),
                            bbox,
                            ctm,
                            op_index,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Ok(placements)
}

/// Translates `placement` by `(dx, dy)` in page-space points. Removes the
/// old `Do` call the same way [`edit_text_run`] removes old text (true
/// removal via `openpdfedit_redact::redact_content`), then re-emits `Do`
/// under a translated `cm` — the same `/Resources`/`/XObject` entry, no
/// new object needed. Does not save.
pub fn move_image(
    doc: &mut Document,
    placement: &ImagePlacement,
    dx: f64,
    dy: f64,
) -> Result<(), TextEditError> {
    let original = doc.page_content_bytes(placement.page_index)?;
    // Exactly this `Do` call: moving a logo must not delete the text,
    // artwork, or *other image* that happens to sit within its bounding
    // box — a logo overlaying a background photo is the common case.
    let without_old = remove_placement_operator(&original, placement)?;

    // Apply `placement.ctm` first (its original placement), then
    // translate the result by (dx, dy) in device/page space — a shift
    // independent of whatever scale/rotation the original ctm applied.
    let translated = multiply(placement.ctm, [1.0, 0.0, 0.0, 1.0, dx, dy]);
    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new("cm", translated.iter().map(|&v| v.into()).collect()),
        Operation::new("Do", vec![placement.xobject_name.as_str().into()]),
        Operation::new("Q", vec![]),
    ];
    let new_ops_bytes = Content { operations: ops }
        .encode()
        .map_err(|e| TextEditError::ContentEncode(e.to_string()))?;

    let bytes = append_in_clean_state(without_old, new_ops_bytes);
    doc.set_page_contents(placement.page_index, bytes)?;
    Ok(())
}

/// Deletes exactly `placement`'s own `Do` call. The image-side
/// counterpart of [`remove_run_operators`]; a `Do` has no side effects
/// beyond drawing, so nothing needs re-emitting in its place.
fn remove_placement_operator(
    content_bytes: &[u8],
    placement: &ImagePlacement,
) -> Result<Vec<u8>, TextEditError> {
    let content =
        Content::decode(content_bytes).map_err(|e| TextEditError::ContentDecode(e.to_string()))?;

    let mut kept = Vec::with_capacity(content.operations.len());
    let mut removed = false;
    for (index, op) in content.operations.into_iter().enumerate() {
        if index == placement.op_index && op.operator == "Do" {
            removed = true;
            continue;
        }
        kept.push(op);
    }
    if !removed {
        return Err(TextEditError::RunNotFound);
    }

    Content { operations: kept }
        .encode()
        .map_err(|e| TextEditError::ContentEncode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_page_content() -> Vec<u8> {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![100.0.into(), 200.0.into()]),
                Operation::new("Tj", vec![Object::string_literal("hello")]),
                Operation::new("ET", vec![]),
            ],
        };
        content.encode().unwrap()
    }

    /// A one-page PDF whose `F1` is a real `Type0`/`Identity-H` subset
    /// font with a `/ToUnicode` CMap — i.e. exactly the shape Google
    /// Docs, Word and LaTeX all emit, and the shape an earlier pass of
    /// this crate wrongly declared uneditable.
    fn cid_font_pdf(cid_text: &[u8]) -> Vec<u8> {
        use lopdf::{dictionary, Object as O, Stream};

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                Operation::new(
                    "Tj",
                    vec![O::String(
                        cid_text.to_vec(),
                        lopdf::StringFormat::Hexadecimal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        // 0x24->'A', 0x25->'B', 0x03->' ', 0x26->'C'
        let cmap = b"/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
4 beginbfchar
<0024> <0041>
<0025> <0042>
<0003> <0020>
<0026> <0043>
endbfchar
endcmap"
            .to_vec();
        let tounicode_id = raw.add_object(Stream::new(dictionary! {}, cmap));

        let descendant_id = raw.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "AAAAAA+TestSubset",
            "DW" => 500,
            "W" => vec![0x24.into(), vec![600.into(), 700.into()].into()],
        });
        let font_id = raw.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "AAAAAA+TestSubset",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![O::Reference(descendant_id)],
            "ToUnicode" => O::Reference(tounicode_id),
        });

        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => O::Reference(font_id) } },
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();
        bytes
    }

    /// Same shape as [`cid_font_pdf`], but the embedded font's `/BaseFont`
    /// says Bold (as real producers always do — `Arial-BoldMT`,
    /// `Calibri-Bold`, etc.) and the text sits on a filled coloured
    /// rectangle, matching the exact real-world shape reported: a bold
    /// heading/label on a coloured background.
    fn bold_cid_font_pdf_on_colored_background(cid_text: &[u8]) -> Vec<u8> {
        use lopdf::{dictionary, Object as O, Stream};

        let content = Content {
            operations: vec![
                // The coloured background the heading sits on.
                Operation::new("rg", vec![0.1.into(), 0.2.into(), 0.6.into()]),
                Operation::new("re", vec![40.into(), 690.into(), 300.into(), 30.into()]),
                Operation::new("f", vec![]),
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                Operation::new(
                    "Tj",
                    vec![O::String(
                        cid_text.to_vec(),
                        lopdf::StringFormat::Hexadecimal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        // Same "A B C" mapping as cid_font_pdf.
        let cmap = b"/CIDInit /ProcSet findresource begin
begincmap
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
4 beginbfchar
<0024> <0041>
<0025> <0042>
<0003> <0020>
<0026> <0043>
endbfchar
endcmap"
            .to_vec();
        let tounicode_id = raw.add_object(Stream::new(dictionary! {}, cmap));

        let descriptor_id = raw.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "AAAAAA+Arial-BoldMT",
            // Bit 19 (ForceBold) set too, so the flags path is exercised
            // even on a document whose name a stricter parser might not
            // trust alone.
            "Flags" => 1 << 18,
        });
        let descendant_id = raw.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "AAAAAA+Arial-BoldMT",
            "FontDescriptor" => O::Reference(descriptor_id),
            "DW" => 500,
            "W" => vec![0x24.into(), vec![600.into(), 700.into()].into()],
        });
        let font_id = raw.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "AAAAAA+Arial-BoldMT",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![O::Reference(descendant_id)],
            "ToUnicode" => O::Reference(tounicode_id),
        });

        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => O::Reference(font_id) } },
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();
        bytes
    }

    /// The regression test for "I edit Bold text whose background is
    /// coloured and it comes back un-bold." The fallback path used to
    /// hardcode plain Helvetica regardless of the original font's
    /// weight, so any edit that fell back — which is disproportionately
    /// likely on a heading/label, since its subset only ever embedded
    /// the handful of glyphs the original short text used — silently
    /// dropped the bold weight (and the typeface) with no indication
    /// anything had changed.
    #[test]
    fn editing_a_bold_run_falls_back_to_a_bold_standard_font() {
        // Only A, B, C and space are in this subset — 'Z' forces the
        // fallback path, the same way a real short bold heading's
        // sparse subset would on almost any edit.
        let mut doc = Document::from_bytes(&bold_cid_font_pdf_on_colored_background(&[
            0x00, 0x24, 0x00, 0x25,
        ]))
        .expect("should parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("should list runs");
        assert!(runs[0].is_editable);

        edit_text_run(&mut doc, &runs[0], "AZ").expect("fallback edit must succeed");

        let saved = doc.save_incremental().expect("save");
        let reopened = Document::from_bytes(&saved).expect("reparse");

        let fonts = reopened.page_font_resources(0).expect("fonts");
        assert!(
            fonts.iter().any(|(_, d)| {
                d.get(b"BaseFont")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .is_some_and(|n| n == b"Helvetica-Bold")
            }),
            "the fallback font must stay bold (Helvetica-Bold), not silently become plain \
             Helvetica; page fonts: {:?}",
            fonts
                .iter()
                .filter_map(|(_, d)| d.get(b"BaseFont").ok()?.as_name().ok())
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .collect::<Vec<_>>()
        );
        assert!(
            !fonts.iter().any(|(_, d)| {
                d.get(b"BaseFont")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .is_some_and(|n| n == b"Helvetica")
            }),
            "the plain (non-bold) Helvetica fallback must not have been used here"
        );

        // And the coloured background — unrelated to the font bug, but
        // the exact scenario reported — must still be intact: the fix
        // must not have regressed the background-preservation behaviour.
        let ops = page_operations(&reopened, 0);
        assert!(
            ops.iter().any(|(op, args)| op == "re"
                && args
                    .first()
                    .is_some_and(|a| (number(a) - 40.0).abs() < 0.01)),
            "the background rectangle must survive the edit"
        );
    }

    /// The core regression test for the whole subset-font fix: text in a
    /// real Identity-H subset font must decode to readable characters
    /// and be reported as editable — the exact case that previously
    /// returned garbage and refused to edit.
    #[test]
    fn cid_subset_font_text_decodes_and_is_editable() {
        // "A B" as glyph ids.
        let doc = Document::from_bytes(&cid_font_pdf(&[0x00, 0x24, 0x00, 0x03, 0x00, 0x25]))
            .expect("should parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("should list runs");

        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].text, "A B",
            "glyph ids must decode through /ToUnicode into real text"
        );
        assert!(
            runs[0].is_editable,
            "a subset font with a /ToUnicode table IS editable — this is the bug that made \
             the feature useless on real-world PDFs"
        );
    }

    #[test]
    fn editing_a_cid_subset_run_writes_correct_glyph_ids() {
        let mut doc = Document::from_bytes(&cid_font_pdf(&[0x00, 0x24, 0x00, 0x03, 0x00, 0x25]))
            .expect("should parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("should list runs");

        edit_text_run(&mut doc, &runs[0], "CAB").expect("editing a subset-font run must succeed");

        let reopened =
            Document::from_bytes(&doc.save_incremental().expect("save")).expect("should reparse");
        let after = list_text_runs_in_page(&reopened, 0).expect("should list runs");
        let texts: Vec<&str> = after.iter().map(|r| r.text.as_str()).collect();
        assert!(
            texts.contains(&"CAB"),
            "the replacement must round-trip back to readable text, got {texts:?}"
        );
    }

    /// A character the embedded subset has no glyph for must still be
    /// editable — by falling back to a standard font — rather than
    /// failing. Refusing was the old behavior and it made ordinary edits
    /// fail on ordinary letters (reported in the wild for a plain "b").
    #[test]
    fn editing_falls_back_to_a_standard_font_for_glyphs_outside_the_subset() {
        let mut doc = Document::from_bytes(&cid_font_pdf(&[0x00, 0x24])).expect("should parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("should list runs");

        // 'Z' has no glyph in this subset (only A, B, C and space do).
        edit_text_run(&mut doc, &runs[0], "AZ")
            .expect("a character outside the subset must still be editable via fallback");

        let saved = doc.save_incremental().expect("save");
        let reopened = Document::from_bytes(&saved).expect("reparse");

        // The fallback font must have been added to the page's resources
        // and actually referenced, so the text really can render.
        let fonts = reopened.page_font_resources(0).expect("fonts");
        assert!(
            fonts.iter().any(|(_, d)| {
                d.get(b"BaseFont")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .is_some_and(|n| n == b"Helvetica")
            }),
            "a standard fallback font should have been added to the page"
        );

        let after = list_text_runs_in_page(&reopened, 0).expect("list");
        assert!(
            after.iter().any(|r| r.text.contains("AZ")),
            "the replacement must be readable back, got {:?}",
            after.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ensure_page_font_is_idempotent() {
        let mut doc = Document::from_bytes(&cid_font_pdf(&[0x00, 0x24])).expect("should parse");
        let first = doc.ensure_page_font(0, "Helvetica").expect("add");
        let second = doc.ensure_page_font(0, "Helvetica").expect("reuse");
        assert_eq!(
            first, second,
            "the same base font must reuse its resource name"
        );
    }

    #[test]
    fn cid_run_bbox_uses_real_glyph_widths_not_the_estimate() {
        // Glyph 0x24 is 600/1000 wide, 0x25 is 700/1000 (from /W), at
        // 12pt => 0.6*12 + 0.7*12 = 15.6pt. The old estimate would have
        // said 2 glyphs * 12 * 0.5 = 12pt.
        let doc =
            Document::from_bytes(&cid_font_pdf(&[0x00, 0x24, 0x00, 0x25])).expect("should parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("should list runs");
        let width = runs[0].bbox.x1 - runs[0].bbox.x0;
        assert!(
            (width - 15.6).abs() < 0.01,
            "expected real /W-derived width 15.6, got {width}"
        );
    }

    #[test]
    fn plain_encoded_runs_remain_editable() {
        let runs = list_text_runs(0, &text_page_content()).expect("should parse");
        assert!(
            runs[0].is_editable,
            "ordinary single-byte-encoded text must stay editable"
        );
    }

    #[test]
    fn list_text_runs_finds_the_run_with_its_text_and_font() {
        let runs = list_text_runs(0, &text_page_content()).expect("should parse");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "hello");
        assert_eq!(runs[0].font_name, "F1");
        assert_eq!(runs[0].font_size, 12.0);
        assert_eq!(runs[0].page_index, 0);
    }

    #[test]
    fn list_text_runs_positions_the_bbox_at_the_td_offset() {
        let runs = list_text_runs(0, &text_page_content()).expect("should parse");
        // Td moved to (100, 200); the run's bbox origin should be there.
        assert!((runs[0].bbox.x0 - 100.0).abs() < 0.01);
        assert!((runs[0].bbox.y0 - 200.0).abs() < 0.01);
    }

    #[test]
    fn list_text_runs_ignores_empty_strings() {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Tj", vec![Object::string_literal("")]),
                Operation::new("ET", vec![]),
            ],
        };
        let bytes = content.encode().unwrap();
        let runs = list_text_runs(0, &bytes).expect("should parse");
        assert!(runs.is_empty());
    }

    #[test]
    fn edit_text_run_removes_the_old_text_and_keeps_the_page_valid() {
        use lopdf::{dictionary, Object as O, Stream};

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, text_page_content()));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            }}},
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
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
        let runs =
            list_text_runs(0, &doc.page_content_bytes(0).unwrap()).expect("should find runs");
        assert_eq!(runs.len(), 1);

        edit_text_run(&mut doc, &runs[0], "goodbye").expect("edit should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let content_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = reopened
            .get_object(content_id)
            .unwrap()
            .as_stream()
            .unwrap();
        let decoded = Content::decode(&stream.content).unwrap();
        let strings: Vec<String> = decoded
            .operations
            .iter()
            .filter(|op| op.operator == "Tj")
            .filter_map(|op| op.operands.first())
            .filter_map(|o| o.as_str().ok())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect();

        assert_eq!(
            strings,
            vec!["goodbye"],
            "old text gone, new text present, no duplicates"
        );
    }

    /// A page whose text is drawn under an explicit graphics state:
    /// `ops_before` runs before the `BT` block. Used to check that colour
    /// and text-state parameters survive an edit.
    fn styled_text_pdf(ops_before: Vec<Operation>, inside_q: bool) -> Vec<u8> {
        use lopdf::{dictionary, Object as O, Stream};

        let mut operations = Vec::new();
        if inside_q {
            operations.push(Operation::new("q", vec![]));
        }
        operations.extend(ops_before);
        operations.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
            Operation::new("Td", vec![100.0.into(), 200.0.into()]),
            Operation::new("Tj", vec![Object::string_literal("hello")]),
            Operation::new("ET", vec![]),
        ]);
        if inside_q {
            operations.push(Operation::new("Q", vec![]));
        }

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(
            dictionary! {},
            Content { operations }.encode().unwrap(),
        ));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            }}},
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();
        bytes
    }

    /// Returns every operator/operand pair in a page's content stream,
    /// for asserting on what an edit actually emitted.
    fn page_operations(doc: &Document, page_index: u32) -> Vec<(String, Vec<Object>)> {
        let bytes = doc.page_content_bytes(page_index).unwrap();
        Content::decode(&bytes)
            .unwrap()
            .operations
            .into_iter()
            .map(|op| (op.operator, op.operands))
            .collect()
    }

    /// The regression test for "editing text on a coloured background
    /// changes the font colour / makes the text disappear": white text
    /// rewritten with no colour operator drew in the PDF default (black),
    /// so on a dark banner it looked like the line had vanished.
    #[test]
    fn editing_preserves_the_runs_fill_colour() {
        let white = Operation::new("rg", vec![1.into(), 1.into(), 1.into()]);
        let mut doc = Document::from_bytes(&styled_text_pdf(vec![white], false)).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");

        edit_text_run(&mut doc, &runs[0], "goodbye").expect("edit");

        let ops = page_operations(&doc, 0);
        let tj_at = ops
            .iter()
            .position(|(operator, _)| operator == "Tj" && operator_text(&ops, "Tj") == "goodbye")
            .expect("the replacement Tj must be present");
        let rg_before_tj = ops[..tj_at].iter().rev().find(|(o, _)| o == "rg");
        assert!(
            rg_before_tj.is_some_and(|(_, operands)| operands
                .iter()
                .all(|v| (number(v) - 1.0).abs() < 1e-6)),
            "the replacement must be drawn in the original white, not the default black; got {ops:?}"
        );
    }

    /// Helper for the assertion above: the text of the last `Tj`.
    fn operator_text(ops: &[(String, Vec<Object>)], operator: &str) -> String {
        ops.iter()
            .filter(|(o, _)| o == operator)
            .filter_map(|(_, operands)| operands.first())
            .filter_map(|o| o.as_str().ok())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .next_back()
            .unwrap_or_default()
    }

    /// Colour is part of the graphics state `q`/`Q` saves. Text inside a
    /// `q`...`Q` block must report the colour set *inside* it.
    #[test]
    fn fill_colour_is_scoped_by_q_and_q() {
        let ops = vec![
            Operation::new("rg", vec![0.into(), 0.into(), 1.into()]),
            Operation::new("Q", vec![]),
            Operation::new("q", vec![]),
        ];
        // Outer black -> q -> blue text -> Q. The `styled_text_pdf`
        // helper wraps in q/Q, so the sequence seen by the walker is
        // q, rg(blue), Q, q, BT ... which must leave the text at the
        // *restored* default, not blue.
        let doc = Document::from_bytes(&styled_text_pdf(ops, true)).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        assert!(
            runs[0].style.fill.ops().is_empty(),
            "a colour set and then popped by Q must not leak onto later text"
        );
    }

    #[test]
    fn editing_preserves_render_mode_and_spacing() {
        let state = vec![
            Operation::new("Tr", vec![3.into()]),
            Operation::new("Tc", vec![2.into()]),
        ];
        let mut doc = Document::from_bytes(&styled_text_pdf(state, false)).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        assert_eq!(runs[0].style.render_mode, 3);
        assert!((runs[0].style.char_spacing - 2.0).abs() < 1e-6);

        edit_text_run(&mut doc, &runs[0], "x").expect("edit");
        let ops = page_operations(&doc, 0);
        // Two of each: the original (still present, its Tj redacted away)
        // and the one the replacement re-established.
        assert_eq!(
            ops.iter().filter(|(o, _)| o == "Tr").count(),
            2,
            "the replacement must re-establish the invisible render mode, \
             or an OCR text layer would become visible on edit"
        );
    }

    /// Moving must not re-encode: the run's original glyph bytes are
    /// replayed verbatim at the translated matrix.
    #[test]
    fn move_text_run_translates_without_re_encoding() {
        let mut doc = Document::from_bytes(&styled_text_pdf(vec![], false)).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        let before = runs[0].bbox.x0;

        move_text_run(&mut doc, &runs[0], 40.0, -25.0).expect("move");
        let saved = doc.save_incremental().expect("save");
        let reopened = Document::from_bytes(&saved).expect("reparse");
        let after = list_text_runs_in_page(&reopened, 0).expect("list");

        assert_eq!(after.len(), 1, "exactly one run, not zero or two");
        assert_eq!(after[0].text, "hello", "the text itself must be unchanged");
        assert!(
            (after[0].bbox.x0 - (before + 40.0)).abs() < 0.01,
            "expected x0 {} , got {}",
            before + 40.0,
            after[0].bbox.x0
        );
        assert!(
            (after[0].bbox.y0 - (200.0 - 25.0)).abs() < 0.01,
            "expected y0 175, got {}",
            after[0].bbox.y0
        );
    }

    /// A run whose font has no `/ToUnicode` can't be *edited*, but there
    /// is no reason it can't be *moved* — moving never needs to read or
    /// re-encode the glyphs.
    #[test]
    fn move_works_on_a_run_that_is_not_editable() {
        use lopdf::{dictionary, Object as O, Stream};

        // A Type0 font with no /ToUnicode: uneditable by construction.
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                Operation::new(
                    "Tj",
                    vec![O::String(
                        vec![0x00, 0x24, 0x00, 0x25],
                        lopdf::StringFormat::Hexadecimal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };
        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let descendant_id = raw.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "CIDFontType2",
            "BaseFont" => "AAAAAA+Icons", "DW" => 500,
        });
        let font_id = raw.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "AAAAAA+Icons",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![O::Reference(descendant_id)],
        });
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => O::Reference(font_id) } },
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let mut doc = Document::from_bytes(&bytes).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        assert!(!runs[0].is_editable, "no /ToUnicode: not editable");

        move_text_run(&mut doc, &runs[0], 10.0, 0.0)
            .expect("moving must not require a readable font");

        let saved = doc.save_incremental().expect("save");
        let reopened = Document::from_bytes(&saved).expect("reparse");
        let after = list_text_runs_in_page(&reopened, 0).expect("list");
        assert_eq!(after.len(), 1);
        assert!(
            (after[0].bbox.x0 - 60.0).abs() < 0.01,
            "expected x0 60, got {}",
            after[0].bbox.x0
        );
    }

    /// Runs in different colours must stay separate editing units, or a
    /// merged line would be rewritten entirely in the first run's colour.
    #[test]
    fn runs_in_different_colours_do_not_merge() {
        use lopdf::{dictionary, Object as O, Stream};

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![100.0.into(), 200.0.into()]),
                Operation::new("rg", vec![1.into(), 0.into(), 0.into()]),
                Operation::new("Tj", vec![Object::string_literal("red")]),
                Operation::new("rg", vec![0.into(), 0.into(), 1.into()]),
                Operation::new("Tj", vec![Object::string_literal("blue")]),
                Operation::new("ET", vec![]),
            ],
        };
        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            }}},
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let doc = Document::from_bytes(&bytes).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        assert_eq!(
            runs.len(),
            2,
            "differently-coloured runs must not merge, got {:?}",
            runs.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
    }

    /// Builds a page from arbitrary operations with one Helvetica font.
    fn page_from_ops(operations: Vec<Operation>) -> Vec<u8> {
        use lopdf::{dictionary, Object as O, Stream};

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(
            dictionary! {},
            Content { operations }.encode().unwrap(),
        ));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            }}},
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();
        bytes
    }

    /// A one-page PDF with two separated runs on the same baseline, both
    /// in a real Identity-H subset font — the encoding essentially every
    /// modern producer uses, and the one that made the bug below bite.
    /// Glyph ids 0x24..0x2D map to 'A'..'J'; every glyph is 500/1000 wide.
    fn two_column_cid_pdf(left_glyphs: usize, left_x: f64, right_x: f64) -> Vec<u8> {
        use lopdf::{dictionary, Object as O, Stream};

        let bytes_for = |count: usize| -> Vec<u8> {
            (0..count)
                .flat_map(|i| [0x00, 0x24 + (i % 10) as u8])
                .collect()
        };

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 20.into()]),
                Operation::new("Td", vec![left_x.into(), 700.into()]),
                Operation::new(
                    "Tj",
                    vec![O::String(
                        bytes_for(left_glyphs),
                        lopdf::StringFormat::Hexadecimal,
                    )],
                ),
                Operation::new("Td", vec![(right_x - left_x).into(), 0.into()]),
                Operation::new(
                    "Tj",
                    vec![O::String(bytes_for(3), lopdf::StringFormat::Hexadecimal)],
                ),
                Operation::new("ET", vec![]),
            ],
        };

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\nbegincmap\n1 begincodespacerange\n\
             <0000> <FFFF>\nendcodespacerange\n10 beginbfchar\n",
        );
        for i in 0..10u32 {
            cmap.push_str(&format!("<{:04X}> <{:04X}>\n", 0x24 + i, 0x41 + i));
        }
        cmap.push_str("endbfchar\nendcmap");
        let tounicode_id = raw.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));

        let descendant_id = raw.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "CIDFontType2",
            "BaseFont" => "AAAAAA+TestSubset", "DW" => 500,
        });
        let font_id = raw.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "AAAAAA+TestSubset",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![O::Reference(descendant_id)],
            "ToUnicode" => O::Reference(tounicode_id),
        });
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => O::Reference(font_id) } },
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();
        bytes
    }

    /// The regression test for "edit text on the right banner removes the
    /// text on the left banner".
    ///
    /// The numbers matter, so they are chosen to sit in the gap between
    /// truth and the old estimate. 20 glyphs at 20pt, each 500/1000 wide,
    /// occupy 20 x 0.5 x 20 = **200pt**: the left run really spans
    /// x = 40..240 and gets nowhere near the right run at x = 300.
    ///
    /// But the old removal path asked `openpdfedit-redact` to delete
    /// everything overlapping the edited run's rectangle, and that crate
    /// sizes a run as `byte_count * 0.5 * font_size`. In an Identity-H
    /// font each glyph is *two* bytes, so it computed 40 x 0.5 x 20 =
    /// **400pt** — an apparent span of x = 40..440 that swallows the
    /// right-hand run's rectangle whole. Editing the right run therefore
    /// deleted the left one.
    ///
    /// Precise operator removal has no width estimate to be wrong about.
    #[test]
    fn editing_one_run_never_touches_another_on_the_same_line() {
        let mut doc = Document::from_bytes(&two_column_cid_pdf(20, 40.0, 300.0)).expect("parse");

        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        assert_eq!(runs.len(), 2, "fixture should give two separate runs");
        let left_text = runs[0].text.clone();
        assert!(
            runs[0].bbox.x1 < runs[1].bbox.x0,
            "fixture sanity: the runs must not really overlap ({} vs {})",
            runs[0].bbox.x1,
            runs[1].bbox.x0
        );

        let right = runs[1].clone();
        edit_text_run(&mut doc, &right, "CHANGED").expect("edit");

        let reopened =
            Document::from_bytes(&doc.save_incremental().expect("save")).expect("reparse");
        let after = list_text_runs_in_page(&reopened, 0).expect("list");
        let all: String = after.iter().map(|r| r.text.as_str()).collect();

        assert!(
            all.contains(&left_text),
            "the untouched left-hand run ({left_text:?}) must survive verbatim; \
             page now reads {all:?}"
        );
        assert!(all.contains("CHANGED"), "the replacement must be present");
    }

    /// The same collateral-damage guarantee for images: a logo moved off
    /// a background photo must not take the photo with it.
    #[test]
    fn moving_one_image_never_removes_another_that_overlaps_it() {
        use lopdf::{dictionary, Object as O, Stream};

        let ops = vec![
            // A full-page background image.
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    600.0.into(),
                    0.0.into(),
                    0.0.into(),
                    700.0.into(),
                    0.0.into(),
                    0.0.into(),
                ],
            ),
            Operation::new("Do", vec!["Bg".into()]),
            Operation::new("Q", vec![]),
            // A small logo sitting on top of it.
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    50.0.into(),
                    0.0.into(),
                    0.0.into(),
                    50.0.into(),
                    30.0.into(),
                    30.0.into(),
                ],
            ),
            Operation::new("Do", vec!["Logo".into()]),
            Operation::new("Q", vec![]),
        ];

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().unwrap(),
        ));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => dictionary! {
                "Bg" => dictionary! { "Type" => "XObject", "Subtype" => "Image" },
                "Logo" => dictionary! { "Type" => "XObject", "Subtype" => "Image" },
            }},
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let mut doc = Document::from_bytes(&bytes).expect("parse");
        let placements = list_image_placements(0, &doc.page_content_bytes(0).unwrap()).unwrap();
        let logo = placements
            .iter()
            .find(|p| p.xobject_name == "Logo")
            .expect("the logo");
        move_image(&mut doc, logo, 200.0, 0.0).expect("move");

        let reopened =
            Document::from_bytes(&doc.save_incremental().expect("save")).expect("reparse");
        let after = list_image_placements(0, &reopened.page_content_bytes(0).unwrap()).unwrap();
        assert!(
            after.iter().any(|p| p.xobject_name == "Bg"),
            "the background image the logo sat on must survive the move"
        );
        let moved = after
            .iter()
            .find(|p| p.xobject_name == "Logo")
            .expect("the logo must still be there");
        assert!(
            (moved.bbox.x0 - 230.0).abs() < 0.01,
            "expected the logo at x0=230, got {}",
            moved.bbox.x0
        );
    }

    /// Removing a run must not shift text that follows it on the same
    /// line by relying on the implicit advance rather than its own
    /// positioning operator.
    #[test]
    fn removing_a_run_preserves_the_advance_for_following_text() {
        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![50.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal("AAAA")]),
            // No Td: this run's position depends entirely on the advance
            // of the one before it.
            Operation::new("rg", vec![1.into(), 0.into(), 0.into()]),
            Operation::new("Tj", vec![Object::string_literal("BBBB")]),
            Operation::new("ET", vec![]),
        ];
        let mut doc = Document::from_bytes(&page_from_ops(ops)).expect("parse");
        let runs = list_text_runs_in_page(&doc, 0).expect("list");
        // Different colours, so the two runs stay separate.
        let first = runs.iter().find(|r| r.text == "AAAA").expect("first run");
        let second_x0 = runs
            .iter()
            .find(|r| r.text == "BBBB")
            .expect("second")
            .bbox
            .x0;

        move_text_run(&mut doc, first, 0.0, -100.0).expect("move");

        let reopened =
            Document::from_bytes(&doc.save_incremental().expect("save")).expect("reparse");
        let after = list_text_runs_in_page(&reopened, 0).expect("list");
        let moved_second = after
            .iter()
            .find(|r| r.text == "BBBB")
            .expect("the following run must still exist");
        assert!(
            (moved_second.bbox.x0 - second_x0).abs() < 0.5,
            "the following run relied on the removed run's advance; expected it to stay at \
             x0={second_x0}, found {}",
            moved_second.bbox.x0
        );
    }

    #[test]
    fn list_image_placements_finds_the_do_call_and_its_bbox() {
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
        let placements = list_image_placements(0, &bytes).expect("should parse");
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].xobject_name, "Im1");
        assert!((placements[0].bbox.x0 - 10.0).abs() < 0.01);
        assert!((placements[0].bbox.x1 - 60.0).abs() < 0.01);
    }

    #[test]
    fn move_image_translates_the_do_call() {
        use lopdf::{dictionary, Object as O, Stream};

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
        let content_bytes = content.encode().unwrap();

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let content_id = raw.add_object(Stream::new(dictionary! {}, content_bytes.clone()));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
            }}},
        });
        raw.objects.insert(
            pages_id,
            O::Dictionary(dictionary! {
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
        let placements = list_image_placements(0, &doc.page_content_bytes(0).unwrap()).unwrap();
        assert_eq!(placements.len(), 1);

        move_image(&mut doc, &placements[0], 100.0, 0.0).expect("move should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = Document::from_bytes(&saved).expect("should reparse");
        let after = list_image_placements(0, &reopened.page_content_bytes(0).unwrap()).unwrap();
        assert_eq!(after.len(), 1, "still exactly one Do call, not zero or two");
        assert!(
            (after[0].bbox.x0 - 110.0).abs() < 0.01,
            "moved 100pt right: expected x0=110, got {}",
            after[0].bbox.x0
        );
    }
}
