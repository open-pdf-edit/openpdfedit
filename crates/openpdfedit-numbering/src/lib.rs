//! Page numbers and Bates numbering.
//!
//! Both are one operation — an incrementing label drawn at a fixed
//! position on each page — differing only in padding, prefix, and where
//! it sits. So there is one drawing primitive here and one entry point
//! over it.
//!
//! Distinct from `openpdfedit-watermark`, which tiles a repeating cell
//! across whole pages: this stamps a *single* short label into a margin,
//! and the number changes page to page. Neither is expressible in terms
//! of the other, which is why they're separate crates rather than one
//! with a mode flag.
//!
//! ## Fonts, and what this can't set
//!
//! Labels use the PDF standard 14 fonts, which every reader has built in
//! and which therefore need no embedding, no subsetting, and no font
//! file. That's the right trade for a page number, and it is also the
//! limit: text is WinAnsi-encoded, so the available characters are
//! Latin-1. Anything else is rejected up front by
//! [`validate_label_text`] rather than silently drawn as garbage — a
//! Bates prefix that comes out as `????` on every page of a discovery
//! set is worse than being told no.
//!
//! ## Why the text width is estimated, not measured
//!
//! Centring and right-aligning need the rendered width. Exactly means
//! reading the standard-14 Adobe Font Metrics tables — ~1,400 per-glyph
//! widths. Instead this uses per-character-class averages calibrated
//! against Helvetica's real AFM ([`estimated_text_width`]), which lands
//! within a few percent; for a page number that error is invisible.
//! If this ever needs to be exact, the AFM tables are the answer, not a
//! correction factor.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Object};
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NumberingError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("failed to encode the stamp's content stream: {0}")]
    ContentEncode(String),
    #[error("stamp text is empty")]
    EmptyText,
    #[error(
        "stamp text contains characters this font can't draw ({0:?}). Stamps use the PDF \
         standard fonts, which cover Latin-1 only."
    )]
    UnsupportedCharacters(String),
    #[error("no pages selected to stamp")]
    NoPagesSelected,
}

/// Where on the page a stamp is anchored. Positions are relative to the
/// `/MediaBox`, inset by the stamp's margin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    fn is_top(self) -> bool {
        matches!(self, Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight)
    }

    fn is_bottom(self) -> bool {
        matches!(
            self,
            Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight
        )
    }

    /// 0.0 = left edge, 0.5 = centre, 1.0 = right edge.
    fn horizontal_fraction(self) -> f32 {
        match self {
            Anchor::TopLeft | Anchor::BottomLeft => 0.0,
            Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => 0.5,
            Anchor::TopRight | Anchor::BottomRight => 1.0,
        }
    }
}

/// One of the PDF standard 14 fonts. Named rather than free-form so a
/// caller can't ask for a font that would need embedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelFont {
    #[default]
    Helvetica,
    HelveticaBold,
    TimesRoman,
    TimesBold,
    Courier,
}

impl LabelFont {
    fn base_font(self) -> &'static str {
        match self {
            LabelFont::Helvetica => "Helvetica",
            LabelFont::HelveticaBold => "Helvetica-Bold",
            LabelFont::TimesRoman => "Times-Roman",
            LabelFont::TimesBold => "Times-Bold",
            LabelFont::Courier => "Courier",
        }
    }

    /// Rough width-per-point-of-font-size for an average character.
    /// Courier is monospaced at exactly 0.6; the proportional faces are
    /// handled per character class in [`estimated_text_width`], and this
    /// is the scale factor applied on top.
    fn width_scale(self) -> f32 {
        match self {
            LabelFont::Helvetica => 1.0,
            // Bold faces run a few percent wider than their regular
            // counterparts across the standard 14.
            LabelFont::HelveticaBold => 1.05,
            LabelFont::TimesRoman => 0.92,
            LabelFont::TimesBold => 0.96,
            LabelFont::Courier => 1.0,
        }
    }

    fn is_monospaced(self) -> bool {
        matches!(self, LabelFont::Courier)
    }
}

/// How a stamp looks and where it goes.
#[derive(Debug, Clone)]
pub struct NumberStyle {
    pub anchor: Anchor,
    pub font: LabelFont,
    pub font_size: f32,
    /// `[r, g, b]`, each `0.0..=1.0`.
    pub color: [f32; 3],
    /// `0.0` (invisible) to `1.0` (solid).
    pub opacity: f32,
    /// Counter-clockwise, in degrees, about the stamp's own anchor point
    /// — 45° being the diagonal a watermark conventionally runs along.
    pub rotation_degrees: f32,
    /// Distance from the page edge, in points. Ignored for
    /// [`Anchor::Center`].
    pub margin: f32,
    /// Reduce `font_size` on any page where the label wouldn't otherwise
    /// fit. On by default: a document that mixes Letter with a landscape
    /// exhibit or an A5 insert would otherwise have the label run off the
    /// edge on some of its pages and nowhere else.
    pub shrink_to_fit: bool,
}

impl Default for NumberStyle {
    fn default() -> Self {
        Self {
            anchor: Anchor::BottomCenter,
            font: LabelFont::Helvetica,
            font_size: 10.0,
            color: [0.0, 0.0, 0.0],
            opacity: 1.0,
            rotation_degrees: 0.0,
            // Half an inch, the conventional page margin.
            margin: 36.0,
            shrink_to_fit: true,
        }
    }
}

/// A counter stamped onto each page in turn — page numbers and Bates
/// numbering are the same thing with different padding and prefixes.
#[derive(Debug, Clone)]
pub struct Numbering {
    /// Printed before the number. Bates numbering conventionally uses a
    /// case identifier here, e.g. `"ACME-"`.
    pub prefix: String,
    /// Printed after the number — `" of 20"`, a suffix code, or nothing.
    pub suffix: String,
    /// What the first stamped page is numbered. Not necessarily 1: a
    /// document that continues an earlier volume starts where that one
    /// stopped.
    pub start_at: u64,
    /// Zero-pad the number to this many digits; `0` leaves it unpadded.
    /// Bates numbering is conventionally six digits.
    pub digits: usize,
}

impl Default for Numbering {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            start_at: 1,
            digits: 0,
        }
    }
}

impl Numbering {
    /// The text for the `nth` stamped page, counting from 0.
    fn text_for(&self, nth: usize) -> String {
        let value = self.start_at.saturating_add(nth as u64);
        format!(
            "{}{:0>width$}{}",
            self.prefix,
            value,
            self.suffix,
            width = self.digits
        )
    }
}

/// Rejects text the standard-14 fonts can't draw, before anything is
/// written to the document.
///
/// WinAnsiEncoding is a superset of Latin-1 in the printable range, so
/// the test is "does every character fit in a byte" — with a carve-out
/// for the few code points WinAnsi maps into the 0x80–0x9F range that
/// Latin-1 leaves as control characters, which are exactly the
/// typographic characters people paste into a stamp without thinking
/// (curly quotes, en/em dashes, the euro sign).
pub fn validate_label_text(text: &str) -> Result<(), NumberingError> {
    if text.trim().is_empty() {
        return Err(NumberingError::EmptyText);
    }
    let unsupported: String = text
        .chars()
        .filter(|c| encode_winansi_char(*c).is_none())
        .collect();
    if unsupported.is_empty() {
        Ok(())
    } else {
        Err(NumberingError::UnsupportedCharacters(unsupported))
    }
}

/// The WinAnsi byte for a character, or `None` if the encoding has no
/// slot for it.
fn encode_winansi_char(c: char) -> Option<u8> {
    // The 0x80–0x9F block, where WinAnsi differs from Latin-1.
    const HIGH_RANGE: [(char, u8); 27] = [
        ('\u{20AC}', 0x80), // euro
        ('\u{201A}', 0x82),
        ('\u{0192}', 0x83),
        ('\u{201E}', 0x84),
        ('\u{2026}', 0x85), // ellipsis
        ('\u{2020}', 0x86),
        ('\u{2021}', 0x87),
        ('\u{02C6}', 0x88),
        ('\u{2030}', 0x89),
        ('\u{0160}', 0x8A),
        ('\u{2039}', 0x8B),
        ('\u{0152}', 0x8C),
        ('\u{017D}', 0x8E),
        ('\u{2018}', 0x91), // curly quotes
        ('\u{2019}', 0x92),
        ('\u{201C}', 0x93),
        ('\u{201D}', 0x94),
        ('\u{2022}', 0x95), // bullet
        ('\u{2013}', 0x96), // en dash
        ('\u{2014}', 0x97), // em dash
        ('\u{02DC}', 0x98),
        ('\u{2122}', 0x99), // trademark
        ('\u{0161}', 0x9A),
        ('\u{203A}', 0x9B),
        ('\u{0153}', 0x9C),
        ('\u{017E}', 0x9E),
        ('\u{0178}', 0x9F),
    ];

    match c {
        // Tab/newline have no glyph and would break a single-line stamp.
        '\t' | '\n' | '\r' => None,
        c if (c as u32) < 0x20 => None,
        c if (c as u32) < 0x80 => Some(c as u8),
        c if (0xA0..=0xFF).contains(&(c as u32)) => Some(c as u8),
        c => HIGH_RANGE
            .iter()
            .find(|(ch, _)| *ch == c)
            .map(|(_, byte)| *byte),
    }
}

/// Encodes stamp text as WinAnsi bytes. Assumes [`validate_label_text`]
/// already passed; anything it would have rejected is dropped rather
/// than substituted, since a wrong glyph is harder to notice than a
/// missing one.
fn encode_winansi(text: &str) -> Vec<u8> {
    text.chars().filter_map(encode_winansi_char).collect()
}

/// Approximate rendered width of `text`, in points.
///
/// Calibrated against Helvetica's real AFM widths by character class:
/// digits and uppercase run wide, lowercase middling, punctuation and
/// spaces narrow. See this module's header for why this is an estimate
/// and what the exact answer would cost.
pub fn estimated_text_width(text: &str, font: LabelFont, font_size: f32) -> f32 {
    if font.is_monospaced() {
        return text.chars().count() as f32 * 0.6 * font_size;
    }
    let em_units: f32 = text
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | 'I' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.28,
            ' ' => 0.28,
            'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' => 0.35,
            'm' | 'M' | 'W' | 'w' | '@' => 0.85,
            c if c.is_ascii_digit() => 0.556,
            c if c.is_uppercase() => 0.68,
            _ => 0.52,
        })
        .sum();
    em_units * font.width_scale() * font_size
}

/// Draws `text_for_page(index)` onto each page in `pages`.
///
/// The single primitive every entry point in this module goes through.
/// `text_for_page` is a closure rather than a string so a counter and a
/// constant are the same code path: it receives the 0-based position
/// within `pages` (not the page index), which is what makes "start
/// numbering at 1 on page 3" mean what a reader expects.
///
/// Returns how many pages were stamped.
pub fn number_pages(
    doc: &mut Document,
    pages: &[u32],
    style: &NumberStyle,
    mut text_for_page: impl FnMut(usize) -> String,
) -> Result<usize, NumberingError> {
    if pages.is_empty() {
        return Err(NumberingError::NoPagesSelected);
    }

    // One font and one graphics state for the whole run: they're
    // identical across pages, and a 500-page document should not carry
    // 500 copies of each. `ensure_page_font` and `merge_page_resource`
    // both return the resource name actually used, which is what keeps
    // this from colliding with a name the document already has.
    let opacity = style.opacity.clamp(0.0, 1.0);
    let gs_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "ExtGState",
        // Fill and stroke alpha both, so a future outlined label is
        // consistently translucent rather than half solid.
        "ca" => opacity,
        "CA" => opacity,
    }));

    let mut stamped = 0;
    for (nth, &page_index) in pages.iter().enumerate() {
        let text = text_for_page(nth);
        validate_label_text(&text)?;

        let font_name = doc.ensure_page_font(page_index, style.font.base_font())?;
        let gs_name = doc.merge_page_resource(page_index, "ExtGState", LABEL_GS_NAME, gs_id)?;

        let media_box = doc.page_media_box(page_index)?;
        let content = draw_operations(&text, style, media_box, &font_name, &gs_name)?;
        // Appends after the page's own content and brackets that content
        // in `q`/`Q` — which matters because plenty of real files leave a
        // `q` unclosed, and an appended stream would then render under a
        // stray clip path, i.e. invisibly.
        doc.wrap_and_append_page_content(page_index, &content)?;
        stamped += 1;
    }
    Ok(stamped)
}

/// Resource names for the stamp's own font and graphics state. Prefixed
/// so they can't collide with a name the document already uses — the
/// overlay merge would otherwise overwrite the document's own resource
/// of the same name and change how its existing content renders.
const LABEL_GS_NAME: &str = "OPENumGS";

/// The maximum length a line of text at `radians` can have inside a
/// `width` x `height` box, measured through the box's centre.
///
/// A 45° watermark on a Letter page has room for far more than the
/// page's width; a horizontal one has exactly the width. Getting this
/// right is what lets [`NumberStyle::shrink_to_fit`] shrink only when the
/// text genuinely doesn't fit, rather than shrinking every diagonal
/// stamp to the width of the page.
fn max_line_length(width: f32, height: f32, radians: f32) -> f32 {
    let (sin, cos) = radians.sin_cos();
    let (sin, cos) = (sin.abs(), cos.abs());
    // Guard the near-axis cases, where dividing by a near-zero component
    // sends the limit to infinity: the other axis is the only real
    // constraint there.
    const NEARLY_ZERO: f32 = 1e-4;
    match (cos < NEARLY_ZERO, sin < NEARLY_ZERO) {
        (true, true) => width.max(height), // unreachable; both can't be ~0
        (true, false) => height,
        (false, true) => width,
        (false, false) => (width / cos).min(height / sin),
    }
}

fn draw_operations(
    text: &str,
    style: &NumberStyle,
    media_box: [f32; 4],
    font_name: &str,
    gs_name: &str,
) -> Result<Vec<u8>, NumberingError> {
    let [x0, y0, x1, y1] = media_box;
    let page_width = x1 - x0;
    let page_height = y1 - y0;
    let radians = style.rotation_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();

    let mut font_size = style.font_size;
    if style.shrink_to_fit {
        let inset = if style.anchor == Anchor::Center {
            0.0
        } else {
            2.0 * style.margin
        };
        let available = max_line_length(
            (page_width - inset).max(1.0),
            (page_height - inset).max(1.0),
            radians,
        );
        let width_at_requested = estimated_text_width(text, style.font, font_size);
        if width_at_requested > available {
            font_size *= available / width_at_requested;
        }
    }

    let text_width = estimated_text_width(text, style.font, font_size);
    // Cap height rather than the full font size: vertical centring on
    // the whole em box sits visibly low, because the descender space
    // below the baseline is empty for most text.
    let cap_height = font_size * CAP_HEIGHT_RATIO;

    // Where on the page the stamp is pinned...
    let anchor_x = match style.anchor {
        Anchor::Center => x0 + page_width / 2.0,
        anchor => {
            x0 + style.margin + (page_width - 2.0 * style.margin) * anchor.horizontal_fraction()
        }
    };
    let anchor_y = if style.anchor.is_top() {
        y1 - style.margin
    } else if style.anchor.is_bottom() {
        y0 + style.margin
    } else {
        y0 + page_height / 2.0
    };

    // ...and which point *of the text* lands there. `along` is measured
    // from the start of the text towards its end; `across` from the
    // baseline up towards the cap line.
    let along_fraction = style.anchor.horizontal_fraction();
    let across_fraction = if style.anchor.is_top() {
        1.0
    } else if style.anchor.is_bottom() {
        0.0
    } else {
        0.5
    };

    // Text runs along (cos, sin) and its cap line is (-sin, cos) away
    // from the baseline, so backing off from the anchor by the chosen
    // fractions *along those rotated axes* is what makes rotation happen
    // about the anchor point rather than about the page origin. Doing
    // this in unrotated page space instead — the obvious version —
    // leaves a 45° watermark hanging off the corner rather than centred.
    let start_x = anchor_x - along_fraction * text_width * cos + across_fraction * cap_height * sin;
    let start_y = anchor_y - along_fraction * text_width * sin - across_fraction * cap_height * cos;

    let operations = vec![
        // `q`/`Q` around the stamp itself, as well as around the page's
        // own content: this stream must not leak its colour,
        // transparency or matrix into anything appended after it later.
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(gs_name.into())]),
        Operation::new(
            "rg",
            style
                .color
                .map(|c| Object::Real(c.clamp(0.0, 1.0)))
                .to_vec(),
        ),
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(font_name.into()), Object::Real(font_size)],
        ),
        Operation::new(
            "Tm",
            vec![
                Object::Real(cos),
                Object::Real(sin),
                Object::Real(-sin),
                Object::Real(cos),
                Object::Real(start_x),
                Object::Real(start_y),
            ],
        ),
        Operation::new(
            "Tj",
            vec![Object::String(
                encode_winansi(text),
                lopdf::StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ];

    Content { operations }
        .encode()
        .map_err(|e| NumberingError::ContentEncode(e.to_string()))
}

/// Cap height as a fraction of font size. 0.7 is close enough for every
/// standard-14 face (Helvetica is 0.718, Times 0.662) that the
/// difference is invisible at stamp sizes.
const CAP_HEIGHT_RATIO: f32 = 0.7;

/// Every page index of `doc`, for the common "stamp the whole document"
/// case.
pub fn all_pages(doc: &Document) -> Result<Vec<u32>, NumberingError> {
    Ok((0..doc.page_count()?).collect())
}

/// Stamps an incrementing counter — page numbers, or Bates numbering,
/// which differ only in [`Numbering`]'s padding and prefix.
pub fn add_numbering(
    doc: &mut Document,
    pages: &[u32],
    numbering: &Numbering,
    style: &NumberStyle,
) -> Result<usize, NumberingError> {
    // Validate the first and last labels before writing anything: they
    // bound the character set the whole run will produce (the prefix and
    // suffix are constant, and digits are always encodable), so this
    // catches an unsupported character in the prefix without formatting
    // every page's text twice.
    validate_label_text(&numbering.text_for(0))?;
    validate_label_text(&numbering.text_for(pages.len().saturating_sub(1)))?;
    number_pages(doc, pages, style, |nth| numbering.text_for(nth))
}

/// The conventional Bates configuration: six zero-padded digits, bottom
/// right, small and solid.
pub fn bates_style() -> NumberStyle {
    NumberStyle {
        anchor: Anchor::BottomRight,
        font: LabelFont::Helvetica,
        font_size: 9.0,
        color: [0.0, 0.0, 0.0],
        opacity: 1.0,
        rotation_degrees: 0.0,
        margin: 24.0,
        shrink_to_fit: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_counts_from_start_at_with_the_requested_padding() {
        let bates = Numbering {
            prefix: "ACME-".to_string(),
            suffix: String::new(),
            start_at: 41,
            digits: 6,
        };
        assert_eq!(bates.text_for(0), "ACME-000041");
        assert_eq!(bates.text_for(9), "ACME-000050");
    }

    #[test]
    fn numbering_without_padding_is_a_plain_page_number() {
        let plain = Numbering::default();
        assert_eq!(plain.text_for(0), "1");
        assert_eq!(plain.text_for(11), "12");
    }

    #[test]
    fn numbering_keeps_a_number_wider_than_its_padding_intact() {
        let narrow = Numbering {
            digits: 2,
            start_at: 998,
            ..Numbering::default()
        };
        // Truncating to the pad width would silently renumber the
        // document — the pad is a minimum, not a field width.
        assert_eq!(narrow.text_for(5), "1003");
    }

    #[test]
    fn numbering_can_carry_a_suffix() {
        let of_total = Numbering {
            suffix: " of 20".to_string(),
            ..Numbering::default()
        };
        assert_eq!(of_total.text_for(2), "3 of 20");
    }

    #[test]
    fn plain_ascii_and_latin1_text_is_accepted() {
        assert!(validate_label_text("CONFIDENTIAL").is_ok());
        assert!(validate_label_text("Dossier n° 12 — Ébauche").is_ok());
    }

    /// The characters people paste in without thinking. WinAnsi has
    /// slots for these even though Latin-1 doesn't.
    #[test]
    fn typographic_punctuation_is_accepted() {
        for text in ["“DRAFT”", "‘copy’", "A–B", "A—B", "€50", "3 •"] {
            assert!(validate_label_text(text).is_ok(), "rejected {text:?}");
        }
    }

    #[test]
    fn text_the_standard_fonts_cannot_draw_is_rejected_up_front() {
        let err = validate_label_text("机密").unwrap_err();
        assert!(matches!(err, NumberingError::UnsupportedCharacters(_)));
        assert!(validate_label_text("secret 🔒").is_err());
    }

    #[test]
    fn empty_or_blank_text_is_rejected() {
        assert!(matches!(
            validate_label_text(""),
            Err(NumberingError::EmptyText)
        ));
        assert!(matches!(
            validate_label_text("   "),
            Err(NumberingError::EmptyText)
        ));
    }

    /// A newline would silently produce a stamp showing only its first
    /// line's worth of glyphs on one baseline; better to say so.
    #[test]
    fn a_line_break_is_rejected_rather_than_flattened() {
        assert!(validate_label_text("line one\nline two").is_err());
    }

    #[test]
    fn winansi_encoding_maps_the_high_range_to_single_bytes() {
        assert_eq!(encode_winansi("A"), vec![0x41]);
        assert_eq!(encode_winansi("é"), vec![0xE9]);
        assert_eq!(encode_winansi("—"), vec![0x97]);
        assert_eq!(encode_winansi("€"), vec![0x80]);
    }

    #[test]
    fn width_estimation_scales_with_font_size_and_length() {
        let short = estimated_text_width("Page 1", LabelFont::Helvetica, 10.0);
        let long = estimated_text_width("Page 100 of 100", LabelFont::Helvetica, 10.0);
        assert!(long > short);
        let doubled = estimated_text_width("Page 1", LabelFont::Helvetica, 20.0);
        assert!((doubled - short * 2.0).abs() < 0.01);
    }

    /// The estimate only has to be close; this pins it against the real
    /// Helvetica AFM widths of "CONFIDENTIAL" at 72pt (7,334 em units,
    /// i.e. 528.05pt) so a future change to the width table can't drift
    /// far without a test noticing.
    #[test]
    fn width_estimation_stays_within_a_few_percent_of_real_metrics() {
        let estimated = estimated_text_width("CONFIDENTIAL", LabelFont::Helvetica, 72.0);
        let actual = 528.05;
        let error = (estimated - actual).abs() / actual;
        assert!(error < 0.05, "estimated {estimated}, actual {actual}");
    }

    #[test]
    fn monospaced_width_is_exactly_the_character_count() {
        assert!(
            (estimated_text_width("abcde", LabelFont::Courier, 10.0) - 30.0).abs() < 0.01,
            "Courier is 600/1000 em per character at every size"
        );
    }

    #[test]
    fn a_horizontal_line_is_limited_by_the_page_width() {
        assert!((max_line_length(612.0, 792.0, 0.0) - 612.0).abs() < 0.1);
    }

    #[test]
    fn a_vertical_line_is_limited_by_the_page_height() {
        let quarter_turn = std::f32::consts::FRAC_PI_2;
        assert!((max_line_length(612.0, 792.0, quarter_turn) - 792.0).abs() < 0.1);
    }

    /// The whole point of measuring along the text's own direction: a
    /// diagonal has more room than the page is wide, so a 45° watermark
    /// shouldn't be shrunk to fit the width.
    #[test]
    fn a_diagonal_line_has_more_room_than_the_page_is_wide() {
        let diagonal = max_line_length(612.0, 792.0, std::f32::consts::FRAC_PI_4);
        assert!(
            diagonal > 612.0,
            "a 45-degree line got {diagonal}, less than the page width"
        );
        // Bounded by the narrower side: 612 / cos(45°).
        assert!((diagonal - 865.5).abs() < 1.0, "got {diagonal}");
    }
}
