//! Font encoding support: what makes editing text in a *real-world* PDF
//! possible rather than only in toy files.
//!
//! ## The problem this solves
//!
//! Almost every PDF produced by a modern tool (Google Docs, Word,
//! LaTeX, Pages...) embeds its fonts as **subsets** with an
//! `Identity-H` encoding. In those files a `Tj` operand is not text — it
//! is a sequence of 2-byte **glyph ids** private to that one embedded
//! font subset. Reading those bytes as characters produces garbage, and
//! writing new characters as raw bytes produces the wrong glyphs.
//!
//! An earlier pass of this crate concluded from that "subset fonts can't
//! be edited," which is wrong, and would have made the edit feature
//! useless on essentially every document a user actually has. The
//! missing piece is the font's **`/ToUnicode` CMap**: a small table,
//! present in virtually all such PDFs precisely so text can be
//! copied/searched, mapping each glyph id to the Unicode it renders as.
//!
//! - Reading it forwards (glyph id -> character) recovers the real text.
//! - Reading it *backwards* (character -> glyph id) lets us **write**
//!   replacement text, for any character the subset already contains.
//!
//! That last clause is the honest remaining limit: a subset only embeds
//! the glyphs the document actually used, so typing a character that
//! appears nowhere in the original document has no glyph to point at.
//! Handling *that* needs true font subsetting (adding a glyph to the
//! embedded font program), which is still out of scope — but it's a
//! narrow, explainable gap ("the letter ‘Q’ isn't in this document's
//! font subset") rather than a blanket refusal, and
//! [`FontInfo::encode`] reports exactly which characters are missing so
//! the UI can say so.
//!
//! ## Widths
//!
//! CID fonts also carry real per-glyph widths (`/W`, with `/DW` as the
//! default). Using those instead of the crate's
//! character-count-times-font-size guess makes the run bounding boxes —
//! which drive click-to-edit hit testing and the redaction rect used to
//! remove the old text — genuinely accurate for these fonts rather than
//! approximate.

use std::collections::HashMap;

use lopdf::{Dictionary, Object};
use openpdfedit_doc::Document;

/// One page font resource, decoded into the two things text editing
/// needs: how to turn its bytes into characters, and how to turn
/// characters back into its bytes.
#[derive(Debug, Clone, Default)]
pub struct FontInfo {
    /// True for `Type0`/`Identity-H` fonts, whose content-stream bytes
    /// are 2-byte glyph ids rather than single-byte character codes.
    pub is_cid: bool,
    /// glyph id -> the text it renders as, from `/ToUnicode`.
    to_unicode: HashMap<u16, String>,
    /// The inverse, for writing. Only single-character mappings are
    /// invertible in a useful way (a glyph standing for a multi-character
    /// ligature can't be produced from one typed character), so those are
    /// the only ones kept. First mapping wins when two glyphs claim the
    /// same character, which keeps encoding deterministic.
    from_unicode: HashMap<char, u16>,
    /// glyph id -> width in 1/1000 text-space units (`/W`).
    widths: HashMap<u16, f64>,
    /// Default width for glyphs absent from `widths` (`/DW`, default 1000
    /// per the PDF spec).
    default_width: f64,
    /// Whether this font is a bold weight — read from `/BaseFont` and
    /// `/FontDescriptor`/`/Flags`, not assumed. Exists so that a caller
    /// falling back to a substitute font (see `edit_text_run`'s doc) can
    /// pick a matching weight instead of silently un-bolding a heading.
    pub bold: bool,
    /// Whether this font is italic/oblique, detected the same way.
    pub italic: bool,
}

impl FontInfo {
    /// Whether replacement text can be written in this font at all —
    /// true for ordinary single-byte fonts, and for CID fonts that came
    /// with a usable `/ToUnicode` table to invert.
    pub fn can_encode_text(&self) -> bool {
        !self.is_cid || !self.from_unicode.is_empty()
    }

    /// Decodes a raw `Tj`/`TJ` string operand into readable text.
    pub fn decode(&self, raw: &[u8]) -> String {
        if !self.is_cid {
            return String::from_utf8_lossy(raw).into_owned();
        }
        let mut out = String::new();
        for pair in raw.chunks(2) {
            let cid = match pair {
                [hi, lo] => u16::from_be_bytes([*hi, *lo]),
                [only] => *only as u16,
                _ => continue,
            };
            match self.to_unicode.get(&cid) {
                Some(s) => out.push_str(s),
                // No mapping for this glyph — a placeholder keeps the
                // string's shape (and the user's sense of position within
                // it) rather than silently dropping a character.
                None => out.push('\u{FFFD}'),
            }
        }
        out
    }

    /// Encodes `text` into this font's raw content-stream bytes, or
    /// reports the distinct characters that have no glyph in the subset.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, Vec<char>> {
        if !self.is_cid {
            return Ok(text.as_bytes().to_vec());
        }
        let mut bytes = Vec::with_capacity(text.len() * 2);
        let mut missing: Vec<char> = Vec::new();
        for ch in text.chars() {
            match self.from_unicode.get(&ch) {
                Some(cid) => bytes.extend_from_slice(&cid.to_be_bytes()),
                None if !missing.contains(&ch) => missing.push(ch),
                None => {}
            }
        }
        if missing.is_empty() {
            Ok(bytes)
        } else {
            Err(missing)
        }
    }

    /// The rendered width of `raw`'s glyphs, in text-space units at font
    /// size 1 (i.e. multiply by the `Tf` size for on-page points).
    /// `None` when this font carries no width information, leaving the
    /// caller to fall back to its own estimate.
    pub fn raw_width(&self, raw: &[u8]) -> Option<f64> {
        if !self.is_cid || (self.widths.is_empty() && self.default_width == 0.0) {
            return None;
        }
        let mut total = 0.0;
        for pair in raw.chunks(2) {
            let cid = match pair {
                [hi, lo] => u16::from_be_bytes([*hi, *lo]),
                [only] => *only as u16,
                _ => continue,
            };
            total += *self.widths.get(&cid).unwrap_or(&self.default_width);
        }
        Some(total / 1000.0)
    }

    /// The width of `text` after encoding, for sizing replacement text.
    /// `None` under the same conditions as [`FontInfo::raw_width`], or if
    /// `text` can't be encoded at all.
    pub fn text_width(&self, text: &str) -> Option<f64> {
        if !self.is_cid {
            return None;
        }
        let encoded = self.encode(text).ok()?;
        self.raw_width(&encoded)
    }

    /// How many glyphs `raw` encodes — exact for CID fonts (2 bytes
    /// each), one-per-byte otherwise.
    pub fn glyph_count(&self, raw: &[u8]) -> usize {
        if self.is_cid {
            raw.len().div_ceil(2)
        } else {
            raw.len()
        }
    }
}

/// Reads every font resource on `page_index` into a `Tf`-resource-name
/// keyed map. Fonts that can't be understood still get a default
/// [`FontInfo`] entry (treated as a plain single-byte font), so a
/// caller never has to handle "missing font" separately from "font I
/// couldn't decode."
pub fn page_fonts(doc: &Document, page_index: u32) -> HashMap<String, FontInfo> {
    let Ok(resources) = doc.page_font_resources(page_index) else {
        return HashMap::new();
    };
    resources
        .into_iter()
        .map(|(name, dict)| (name, font_info(doc, &dict)))
        .collect()
}

fn font_info(doc: &Document, font: &Dictionary) -> FontInfo {
    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    let is_cid = subtype == "Type0";

    let mut info = FontInfo {
        is_cid,
        default_width: if is_cid { 1000.0 } else { 0.0 },
        ..Default::default()
    };

    if let Ok(Object::Reference(id)) = font.get(b"ToUnicode") {
        if let Ok(bytes) = doc.decoded_stream(*id) {
            info.to_unicode = parse_to_unicode(&bytes);
        }
    }
    for (cid, text) in &info.to_unicode {
        let mut chars = text.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            info.from_unicode.entry(ch).or_insert(*cid);
        }
    }

    let mut descriptor_source = font.clone();
    if is_cid {
        if let Some(descendant) = descendant_font(doc, font) {
            if let Ok(dw) = descendant.get(b"DW") {
                info.default_width = number(doc.resolve(dw));
            }
            if let Ok(w) = descendant.get(b"W") {
                if let Object::Array(arr) = doc.resolve(w) {
                    info.widths = parse_w_array(doc, arr);
                }
            }
            // Weight/style live on the descendant's own /FontDescriptor
            // for a CID font, not on the wrapping /Type0 dict.
            descriptor_source = descendant;
        }
    }

    let base_font = font
        .get(b"BaseFont")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    let flags = descriptor_source
        .get(b"FontDescriptor")
        .ok()
        .and_then(|o| match doc.resolve(o) {
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        })
        .and_then(|d| d.get(b"Flags").ok().map(|f| number(doc.resolve(f)) as i64));
    (info.bold, info.italic) = detect_weight_and_style(&base_font, flags);

    info
}

/// Reads bold/italic from a font's own name first — real-world `/BaseFont`
/// names almost always say so directly (`Arial-BoldMT`,
/// `TimesNewRomanPS-BoldItalicMT`, `Calibri-Bold`) — and falls back to the
/// `/FontDescriptor`/`/Flags` bits (19 = ForceBold, 7 = Italic; PDF32000-1
/// Table 123) when the name alone doesn't say. Named subset prefixes
/// (`AAAAAA+`) don't interfere: they're a fixed 6 characters plus `+`
/// ahead of the real name, so a `contains` check still finds "bold"/
/// "italic"/"oblique" wherever they appear.
fn detect_weight_and_style(base_font: &str, flags: Option<i64>) -> (bool, bool) {
    let lower = base_font.to_ascii_lowercase();
    let mut bold = lower.contains("bold");
    let mut italic = lower.contains("italic") || lower.contains("oblique");
    if let Some(flags) = flags {
        bold |= flags & (1 << 18) != 0;
        italic |= flags & (1 << 6) != 0;
    }
    (bold, italic)
}

fn descendant_font(doc: &Document, font: &Dictionary) -> Option<Dictionary> {
    let descendants = font.get(b"DescendantFonts").ok()?;
    let Object::Array(arr) = doc.resolve(descendants) else {
        return None;
    };
    let first = arr.first()?;
    match doc.resolve(first) {
        Object::Dictionary(d) => Some(d.clone()),
        _ => None,
    }
}

fn number(obj: &Object) -> f64 {
    obj.as_float()
        .map(f64::from)
        .unwrap_or_else(|_| obj.as_i64().unwrap_or(0) as f64)
}

/// Parses a CID font's `/W` array, which interleaves two shapes:
/// `c [w1 w2 ...]` (widths for consecutive glyphs starting at `c`) and
/// `cFirst cLast w` (one width for a whole range).
fn parse_w_array(doc: &Document, arr: &[Object]) -> HashMap<u16, f64> {
    let mut widths = HashMap::new();
    let mut i = 0;
    while i < arr.len() {
        let first = number(doc.resolve(&arr[i]));
        let Some(next) = arr.get(i + 1) else { break };
        match doc.resolve(next) {
            Object::Array(list) => {
                for (offset, w) in list.iter().enumerate() {
                    let cid = first as u32 + offset as u32;
                    if cid <= u16::MAX as u32 {
                        widths.insert(cid as u16, number(doc.resolve(w)));
                    }
                }
                i += 2;
            }
            _ => {
                let Some(third) = arr.get(i + 2) else { break };
                let last = number(doc.resolve(next));
                let w = number(doc.resolve(third));
                let (lo, hi) = (first as u32, last as u32);
                if hi >= lo && hi - lo <= u16::MAX as u32 {
                    for cid in lo..=hi.min(u16::MAX as u32) {
                        widths.insert(cid as u16, w);
                    }
                }
                i += 3;
            }
        }
    }
    widths
}

/// Parses a `/ToUnicode` CMap stream into a glyph-id -> text map.
///
/// CMaps are PostScript-ish, but the two constructs that carry the
/// mappings are rigidly shaped, so this scans for those rather than
/// implementing a PostScript interpreter:
///   - `beginbfchar` ... `endbfchar`, pairs of `<src> <dst>`
///   - `beginbfrange` ... `endbfrange`, triples of `<lo> <hi> <dstStart>`
///     or `<lo> <hi> [<dst> <dst> ...]`
///
/// Destinations are UTF-16BE, and may be several code units long (a
/// glyph standing for a ligature or an astral-plane character).
pub fn parse_to_unicode(bytes: &[u8]) -> HashMap<u16, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut map = HashMap::new();
    for section in split_sections(&text, "beginbfchar", "endbfchar") {
        parse_bfchar(section, &mut map);
    }
    for section in split_sections(&text, "beginbfrange", "endbfrange") {
        parse_bfrange(section, &mut map);
    }
    map
}

fn split_sections<'a>(text: &'a str, start: &str, end: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(s) = rest.find(start) {
        let after = &rest[s + start.len()..];
        match after.find(end) {
            Some(e) => {
                out.push(&after[..e]);
                rest = &after[e + end.len()..];
            }
            None => break,
        }
    }
    out
}

/// Yields each `<...>` hex token and each `[...]` array in order, so the
/// bfchar/bfrange parsers can consume them positionally.
enum Token<'a> {
    Hex(&'a str),
    Array(Vec<&'a str>),
}

fn tokenize(section: &str) -> Vec<Token<'_>> {
    let bytes = section.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                let Some(close) = section[i..].find(']') else {
                    break;
                };
                let inner = &section[i + 1..i + close];
                out.push(Token::Array(hex_tokens(inner)));
                i += close + 1;
            }
            b'<' => {
                let Some(close) = section[i..].find('>') else {
                    break;
                };
                out.push(Token::Hex(section[i + 1..i + close].trim()));
                i += close + 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn hex_tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        match after.find('>') {
            Some(close) => {
                out.push(after[..close].trim());
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    out
}

fn parse_bfchar(section: &str, map: &mut HashMap<u16, String>) {
    let tokens = tokenize(section);
    let mut i = 0;
    while i + 1 < tokens.len() {
        if let (Token::Hex(src), Token::Hex(dst)) = (&tokens[i], &tokens[i + 1]) {
            if let (Some(cid), Some(text)) = (hex_to_u16(src), utf16be_hex_to_string(dst)) {
                map.insert(cid, text);
            }
        }
        i += 2;
    }
}

fn parse_bfrange(section: &str, map: &mut HashMap<u16, String>) {
    let tokens = tokenize(section);
    let mut i = 0;
    while i + 2 < tokens.len() {
        let (Token::Hex(lo), Token::Hex(hi)) = (&tokens[i], &tokens[i + 1]) else {
            i += 1;
            continue;
        };
        let (Some(lo), Some(hi)) = (hex_to_u16(lo), hex_to_u16(hi)) else {
            i += 3;
            continue;
        };
        match &tokens[i + 2] {
            // `<lo> <hi> <dstStart>`: consecutive destinations.
            Token::Hex(dst) => {
                if let Some(start) = utf16be_hex_to_units(dst) {
                    for (offset, cid) in (lo..=hi).enumerate() {
                        let mut units = start.clone();
                        if let Some(last) = units.last_mut() {
                            *last = last.wrapping_add(offset as u16);
                        }
                        if let Some(text) = units_to_string(&units) {
                            map.insert(cid, text);
                        }
                    }
                }
            }
            // `<lo> <hi> [<d1> <d2> ...]`: one destination per glyph.
            Token::Array(items) => {
                for (offset, item) in items.iter().enumerate() {
                    let cid = lo as u32 + offset as u32;
                    if cid > hi as u32 {
                        break;
                    }
                    if let Some(text) = utf16be_hex_to_string(item) {
                        map.insert(cid as u16, text);
                    }
                }
            }
        }
        i += 3;
    }
}

fn hex_to_u16(s: &str) -> Option<u16> {
    u32::from_str_radix(s.trim(), 16).ok().map(|v| v as u16)
}

fn utf16be_hex_to_units(s: &str) -> Option<Vec<u16>> {
    let s = s.trim();
    if s.is_empty() || !s.len().is_multiple_of(4) {
        // Odd-length or non-UTF16-sized destinations are malformed; a
        // single 2-hex-digit value is occasionally seen, so allow that.
        if s.len() == 2 {
            return u8::from_str_radix(s, 16).ok().map(|v| vec![v as u16]);
        }
        return None;
    }
    let mut units = Vec::with_capacity(s.len() / 4);
    for chunk in s.as_bytes().chunks(4) {
        let hex = std::str::from_utf8(chunk).ok()?;
        units.push(u16::from_str_radix(hex, 16).ok()?);
    }
    Some(units)
}

fn units_to_string(units: &[u16]) -> Option<String> {
    String::from_utf16(units).ok().filter(|s| !s.is_empty())
}

fn utf16be_hex_to_string(s: &str) -> Option<String> {
    units_to_string(&utf16be_hex_to_units(s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bfchar_mappings() {
        let cmap = b"begincmap
2 beginbfchar
<0028> <0044>
<002E> <004A>
endbfchar
endcmap";
        let map = parse_to_unicode(cmap);
        assert_eq!(map.get(&0x28).map(String::as_str), Some("D"));
        assert_eq!(map.get(&0x2E).map(String::as_str), Some("J"));
    }

    #[test]
    fn parses_bfrange_consecutive_form() {
        // <0011> <0013> <0041> means 0x11->'A', 0x12->'B', 0x13->'C'.
        let cmap = b"1 beginbfrange
<0011> <0013> <0041>
endbfrange";
        let map = parse_to_unicode(cmap);
        assert_eq!(map.get(&0x11).map(String::as_str), Some("A"));
        assert_eq!(map.get(&0x12).map(String::as_str), Some("B"));
        assert_eq!(map.get(&0x13).map(String::as_str), Some("C"));
    }

    #[test]
    fn parses_bfrange_array_form() {
        let cmap = b"1 beginbfrange
<0020> <0022> [<0058> <0059> <005A>]
endbfrange";
        let map = parse_to_unicode(cmap);
        assert_eq!(map.get(&0x20).map(String::as_str), Some("X"));
        assert_eq!(map.get(&0x21).map(String::as_str), Some("Y"));
        assert_eq!(map.get(&0x22).map(String::as_str), Some("Z"));
    }

    #[test]
    fn parses_multi_code_unit_destinations() {
        // One glyph standing for the two-character sequence "fi".
        let cmap = b"1 beginbfchar
<0100> <00660069>
endbfchar";
        let map = parse_to_unicode(cmap);
        assert_eq!(map.get(&0x100).map(String::as_str), Some("fi"));
    }

    #[test]
    fn malformed_cmap_yields_no_mappings_rather_than_panicking() {
        assert!(parse_to_unicode(b"begincmap garbage <00 endcmap").is_empty());
        assert!(parse_to_unicode(b"").is_empty());
        assert!(parse_to_unicode(b"beginbfchar <0001>").is_empty());
    }

    fn cid_font(pairs: &[(u16, &str)]) -> FontInfo {
        let mut info = FontInfo {
            is_cid: true,
            default_width: 500.0,
            ..Default::default()
        };
        for (cid, text) in pairs {
            info.to_unicode.insert(*cid, (*text).to_string());
            let mut chars = text.chars();
            if let (Some(ch), None) = (chars.next(), chars.next()) {
                info.from_unicode.entry(ch).or_insert(*cid);
            }
        }
        info
    }

    #[test]
    fn cid_round_trips_text_through_decode_and_encode() {
        let font = cid_font(&[(0x24, "A"), (0x25, "B"), (0x03, " ")]);
        let raw = vec![0x00, 0x24, 0x00, 0x03, 0x00, 0x25];
        assert_eq!(font.decode(&raw), "A B");
        assert_eq!(font.encode("A B").unwrap(), raw);
    }

    #[test]
    fn encode_reports_exactly_the_characters_missing_from_the_subset() {
        let font = cid_font(&[(0x24, "A"), (0x25, "B")]);
        let missing = font
            .encode("ABZQZ")
            .expect_err("Z and Q aren't in the subset");
        assert_eq!(
            missing,
            vec!['Z', 'Q'],
            "each missing char reported once, in order"
        );
    }

    #[test]
    fn cid_font_without_a_to_unicode_table_cannot_encode() {
        let bare = FontInfo {
            is_cid: true,
            ..Default::default()
        };
        assert!(!bare.can_encode_text());
        // ...but a plain single-byte font always can.
        assert!(FontInfo::default().can_encode_text());
    }

    #[test]
    fn widths_use_the_w_table_and_fall_back_to_dw() {
        let mut font = cid_font(&[(0x24, "A"), (0x25, "B")]);
        font.widths.insert(0x24, 700.0);
        // 0x25 has no explicit width, so /DW (500) applies.
        let width = font.raw_width(&[0x00, 0x24, 0x00, 0x25]).unwrap();
        assert!((width - 1.2).abs() < 1e-9, "got {width}");
    }

    #[test]
    fn glyph_count_is_two_bytes_per_glyph_for_cid_fonts() {
        let font = cid_font(&[(0x24, "A")]);
        assert_eq!(font.glyph_count(&[0x00, 0x24, 0x00, 0x25]), 2);
        assert_eq!(FontInfo::default().glyph_count(b"abcd"), 4);
    }
}
