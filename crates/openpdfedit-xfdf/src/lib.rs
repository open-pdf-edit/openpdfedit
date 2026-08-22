//! XFDF import and export.
//!
//! XFDF is the interchange format for PDF annotations: a small XML file
//! carrying comments, highlights and drawings *without* the document
//! they belong to. It is what makes "here are my comments on your
//! contract" a 4 KB attachment instead of a 40 MB copy of the contract,
//! and it is how markup moves between PDF tools at all — Acrobat, Foxit,
//! Xodo and PDF-XChange all read and write it.
//!
//! ## Coordinates
//!
//! XFDF uses PDF page space (origin bottom-left, points), the same as
//! everything else in this workspace, so no conversion is needed — but
//! its `page` attribute is **0-based** while the page numbers a user
//! sees are 1-based, and getting that wrong silently puts every
//! imported comment one page off.
//!
//! ## What round-trips
//!
//! Export covers every markup annotation the document carries, with its
//! geometry, colour, opacity, author and text. Import can only recreate
//! the kinds `openpdfedit-annot` can build — highlight, underline,
//! strikeout, free text and ink. Anything else in an incoming file
//! (squares, circles, stamps, file attachments) is counted as skipped
//! rather than dropped silently or approximated with something that
//! isn't what the sender drew.

use std::io::Cursor;

use lopdf::Object;
use openpdfedit_annot::{add_annotation, AnnotError, AnnotationKind, Color, NewAnnotation, Rect};
use openpdfedit_doc::{DocError, Document};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XfdfError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error(transparent)]
    Annot(#[from] AnnotError),
    #[error("this doesn't look like an XFDF file: {0}")]
    Malformed(String),
    #[error("failed to write XFDF: {0}")]
    Write(String),
}

/// One annotation, in the neutral shape both directions work with.
#[derive(Debug, Clone, PartialEq)]
pub struct XfdfAnnotation {
    /// XFDF element name: `highlight`, `underline`, `strikeout`,
    /// `freetext`, `ink`, `text`, and so on. Lowercase, as XFDF writes
    /// them — PDF's own `/Subtype` names are capitalised differently
    /// (`StrikeOut` vs `strikeout`), which is a mapping worth doing once
    /// here rather than at every call site.
    pub kind: String,
    /// 0-based, as XFDF stores it.
    pub page: u32,
    pub rect: [f32; 4],
    pub color: [f32; 3],
    pub opacity: f32,
    pub contents: Option<String>,
    /// Author.
    pub title: Option<String>,
    /// Four corners per quad, as PDF `/QuadPoints`: x1 y1 x2 y2 x3 y3 x4 y4.
    pub quads: Vec<[f32; 8]>,
    /// One entry per stroke, each a flat list of x,y pairs.
    pub ink: Vec<Vec<(f32, f32)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportReport {
    pub imported: usize,
    /// Annotations of a kind this app can't create. Counted rather than
    /// silently dropped or approximated with something the sender didn't
    /// draw.
    pub skipped: usize,
    /// Annotations whose `page` isn't in this document — an XFDF written
    /// against a different or later revision of the file.
    pub out_of_range: usize,
}

/// Reads every markup annotation out of `doc`.
pub fn extract(doc: &Document) -> Result<Vec<XfdfAnnotation>, XfdfError> {
    let mut out = Vec::new();
    for page in 0..doc.page_count()? {
        for id in doc.page_annotation_refs(page)? {
            let Ok(dict) = doc.dictionary(id) else {
                continue;
            };

            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .unwrap_or(b"");
            let Some(kind) = xfdf_element_name(subtype) else {
                continue;
            };
            let Some(rect) = numbers(doc, dict.get(b"Rect").ok(), 4) else {
                continue;
            };

            out.push(XfdfAnnotation {
                kind: kind.to_string(),
                page,
                rect: [rect[0], rect[1], rect[2], rect[3]],
                color: numbers(doc, dict.get(b"C").ok(), 3)
                    .map(|c| [c[0], c[1], c[2]])
                    // No `/C` means the reader picks; black is the
                    // conventional stand-in and is what XFDF requires a
                    // value for.
                    .unwrap_or([0.0, 0.0, 0.0]),
                opacity: dict
                    .get(b"CA")
                    .ok()
                    .and_then(|o| doc.resolve(o).as_float().ok())
                    .unwrap_or(1.0),
                contents: text_string(doc, dict.get(b"Contents").ok()),
                title: text_string(doc, dict.get(b"T").ok()),
                quads: quad_points(doc, dict.get(b"QuadPoints").ok()),
                ink: ink_list(doc, dict.get(b"InkList").ok()),
            });
        }
    }
    Ok(out)
}

/// Maps a PDF `/Subtype` to its XFDF element name, or `None` for
/// something that isn't a markup annotation at all (a widget, a link, a
/// popup).
fn xfdf_element_name(subtype: &[u8]) -> Option<&'static str> {
    match subtype {
        b"Highlight" => Some("highlight"),
        b"Underline" => Some("underline"),
        b"StrikeOut" => Some("strikeout"),
        b"Squiggly" => Some("squiggly"),
        b"Text" => Some("text"),
        b"FreeText" => Some("freetext"),
        b"Ink" => Some("ink"),
        b"Square" => Some("square"),
        b"Circle" => Some("circle"),
        b"Line" => Some("line"),
        b"Polygon" => Some("polygon"),
        b"PolyLine" => Some("polyline"),
        b"Stamp" => Some("stamp"),
        b"Caret" => Some("caret"),
        _ => None,
    }
}

fn numbers(doc: &Document, object: Option<&Object>, expected: usize) -> Option<Vec<f32>> {
    let array = doc.resolve(object?).as_array().ok()?;
    if array.len() != expected {
        return None;
    }
    array
        .iter()
        .map(|item| doc.resolve(item).as_float().ok())
        .collect()
}

fn text_string(doc: &Document, object: Option<&Object>) -> Option<String> {
    let bytes = doc.resolve(object?).as_str().ok()?;
    if bytes.is_empty() {
        return None;
    }
    // Same two encodings as everywhere else a PDF stores text: UTF-16BE
    // behind a byte-order mark, or PDFDocEncoding (Latin-1 in its
    // printable range).
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        Some(String::from_utf16_lossy(&units))
    } else {
        Some(bytes.iter().map(|b| *b as char).collect())
    }
}

fn quad_points(doc: &Document, object: Option<&Object>) -> Vec<[f32; 8]> {
    let Some(object) = object else {
        return Vec::new();
    };
    let Ok(array) = doc.resolve(object).as_array() else {
        return Vec::new();
    };
    let values: Vec<f32> = array
        .iter()
        .filter_map(|item| doc.resolve(item).as_float().ok())
        .collect();
    values.as_chunks::<8>().0.to_vec()
}

fn ink_list(doc: &Document, object: Option<&Object>) -> Vec<Vec<(f32, f32)>> {
    let Some(object) = object else {
        return Vec::new();
    };
    let Ok(strokes) = doc.resolve(object).as_array() else {
        return Vec::new();
    };
    strokes
        .iter()
        .filter_map(|stroke| {
            let coordinates = doc.resolve(stroke).as_array().ok()?;
            let values: Vec<f32> = coordinates
                .iter()
                .filter_map(|item| doc.resolve(item).as_float().ok())
                .collect();
            Some(
                values
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| (pair[0], pair[1]))
                    .collect(),
            )
        })
        .collect()
}

/// Serializes annotations as an XFDF document.
///
/// `source_href` is recorded as the file the markup belongs to, which is
/// what lets a reader tell whether an XFDF matches the PDF it's being
/// applied to.
pub fn to_xfdf(
    annotations: &[XfdfAnnotation],
    source_href: Option<&str>,
) -> Result<String, XfdfError> {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    // quick-xml's writer surfaces I/O errors rather than parse errors,
    // and the exact type differs by event kind, so this stays generic.
    let write_err = |e: std::io::Error| XfdfError::Write(e.to_string());

    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            None,
        )))
        .map_err(write_err)?;

    let mut root = BytesStart::new("xfdf");
    root.push_attribute(("xmlns", "http://ns.adobe.com/xfdf/"));
    // Without `xml:space="preserve"` a reader is free to collapse the
    // whitespace inside <contents>, which quietly reflows every comment.
    root.push_attribute(("xml:space", "preserve"));
    writer.write_event(Event::Start(root)).map_err(write_err)?;

    if let Some(href) = source_href {
        // `<f href="…"/>` names the PDF this markup belongs to, which is
        // how a reader can tell an XFDF is being applied to the right
        // document.
        let mut file = BytesStart::new("f");
        file.push_attribute(("href", href));
        writer.write_event(Event::Empty(file)).map_err(write_err)?;
    }

    writer
        .write_event(Event::Start(BytesStart::new("annots")))
        .map_err(write_err)?;

    for annotation in annotations {
        let mut element = BytesStart::new(annotation.kind.as_str());
        element.push_attribute(("page", annotation.page.to_string().as_str()));
        element.push_attribute(("rect", join_numbers(&annotation.rect).as_str()));
        element.push_attribute(("color", hex_color(annotation.color).as_str()));
        element.push_attribute(("opacity", format_number(annotation.opacity).as_str()));
        if let Some(title) = &annotation.title {
            element.push_attribute(("title", title.as_str()));
        }
        if !annotation.quads.is_empty() {
            let coords: Vec<String> = annotation
                .quads
                .iter()
                .map(|quad| join_numbers(quad))
                .collect();
            element.push_attribute(("coords", coords.join(",").as_str()));
        }
        writer
            .write_event(Event::Start(element))
            .map_err(write_err)?;

        if let Some(contents) = &annotation.contents {
            writer
                .write_event(Event::Start(BytesStart::new("contents")))
                .map_err(write_err)?;
            writer
                .write_event(Event::Text(BytesText::new(contents)))
                .map_err(write_err)?;
            writer
                .write_event(Event::End(BytesEnd::new("contents")))
                .map_err(write_err)?;
        }

        if !annotation.ink.is_empty() {
            writer
                .write_event(Event::Start(BytesStart::new("inklist")))
                .map_err(write_err)?;
            for stroke in &annotation.ink {
                writer
                    .write_event(Event::Start(BytesStart::new("gesture")))
                    .map_err(write_err)?;
                // XFDF gestures are "x,y;x,y;…" — semicolons between
                // points, commas within one.
                let points: Vec<String> = stroke
                    .iter()
                    .map(|(x, y)| format!("{},{}", format_number(*x), format_number(*y)))
                    .collect();
                writer
                    .write_event(Event::Text(BytesText::new(&points.join(";"))))
                    .map_err(write_err)?;
                writer
                    .write_event(Event::End(BytesEnd::new("gesture")))
                    .map_err(write_err)?;
            }
            writer
                .write_event(Event::End(BytesEnd::new("inklist")))
                .map_err(write_err)?;
        }

        writer
            .write_event(Event::End(BytesEnd::new(annotation.kind.as_str())))
            .map_err(write_err)?;
    }

    writer
        .write_event(Event::End(BytesEnd::new("annots")))
        .map_err(write_err)?;
    writer
        .write_event(Event::End(BytesEnd::new("xfdf")))
        .map_err(write_err)?;

    String::from_utf8(writer.into_inner().into_inner()).map_err(|e| XfdfError::Write(e.to_string()))
}

/// Formats a coordinate without a trailing `.0`, which is what every
/// other XFDF writer produces and keeps files diffable.
fn format_number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn join_numbers(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format_number(*v))
        .collect::<Vec<_>>()
        .join(",")
}

fn hex_color([r, g, b]: [f32; 3]) -> String {
    let to_byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", to_byte(r), to_byte(g), to_byte(b))
}

fn parse_hex_color(value: &str) -> Option<[f32; 3]> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 {
        return None;
    }
    let byte = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    Some([byte(0..2)?, byte(2..4)?, byte(4..6)?])
}

/// Parses an XFDF document into annotations.
pub fn from_xfdf(xml: &str) -> Result<Vec<XfdfAnnotation>, XfdfError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut annotations: Vec<XfdfAnnotation> = Vec::new();
    // Which element's text we're currently inside, so `<contents>` and
    // `<gesture>` bodies land on the annotation that owns them.
    let mut in_contents = false;
    let mut in_gesture = false;
    let mut saw_root = false;

    loop {
        match reader.read_event() {
            Err(e) => return Err(XfdfError::Malformed(e.to_string())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let name = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
                match name.as_str() {
                    "xfdf" => saw_root = true,
                    "contents" | "contents-richtext" => in_contents = true,
                    "gesture" => {
                        in_gesture = true;
                        if let Some(last) = annotations.last_mut() {
                            last.ink.push(Vec::new());
                        }
                    }
                    "annots" | "inklist" | "f" | "ids" | "fields" => {}
                    _ => {
                        if let Some(annotation) = parse_annotation(&name, &element) {
                            annotations.push(annotation);
                        }
                    }
                }
            }
            Ok(Event::Text(text)) => {
                let value = text
                    .decode()
                    .map_err(|e| XfdfError::Malformed(e.to_string()))?
                    .into_owned();
                if in_contents {
                    if let Some(last) = annotations.last_mut() {
                        // Appended rather than assigned: an entity
                        // reference inside the text splits it into
                        // several Text events.
                        last.contents
                            .get_or_insert_with(String::new)
                            .push_str(&value);
                    }
                } else if in_gesture {
                    if let Some(stroke) = annotations.last_mut().and_then(|a| a.ink.last_mut()) {
                        stroke.extend(parse_gesture(&value));
                    }
                }
            }
            // quick-xml 0.41 reports `&amp;`, `&#167;` and friends as
            // their own event rather than folding them into the
            // surrounding text, so a parser that only handles `Text`
            // silently *deletes* every escaped character. An ampersand
            // or an angle bracket in a comment is ordinary, and losing
            // it corrupts what someone wrote.
            Ok(Event::GeneralRef(reference)) => {
                let resolved = if reference.is_char_ref() {
                    reference
                        .resolve_char_ref()
                        .ok()
                        .flatten()
                        .map(|c| c.to_string())
                } else {
                    reference
                        .decode()
                        .ok()
                        .and_then(|name| resolve_predefined_entity(&name).map(str::to_string))
                };
                // An entity this parser can't resolve (a DTD-defined one)
                // is dropped rather than guessed at; XFDF in the wild
                // uses only the predefined five and numeric refs.
                if let Some(resolved) = resolved {
                    if in_contents {
                        if let Some(last) = annotations.last_mut() {
                            last.contents
                                .get_or_insert_with(String::new)
                                .push_str(&resolved);
                        }
                    }
                }
            }
            Ok(Event::End(element)) => {
                match String::from_utf8_lossy(element.local_name().as_ref()).as_ref() {
                    "contents" | "contents-richtext" => in_contents = false,
                    "gesture" => in_gesture = false,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if !saw_root {
        return Err(XfdfError::Malformed(
            "no <xfdf> root element — is this an FDF or a plain XML file?".to_string(),
        ));
    }
    Ok(annotations)
}

fn parse_annotation(name: &str, element: &BytesStart<'_>) -> Option<XfdfAnnotation> {
    // Only elements that name a markup annotation type; anything else at
    // this level is metadata this parser doesn't model.
    let known = matches!(
        name,
        "highlight"
            | "underline"
            | "strikeout"
            | "squiggly"
            | "text"
            | "freetext"
            | "ink"
            | "square"
            | "circle"
            | "line"
            | "polygon"
            | "polyline"
            | "stamp"
            | "caret"
    );
    if !known {
        return None;
    }

    let attribute = |key: &str| -> Option<String> {
        element
            .attributes()
            .flatten()
            .find(|a| a.key.local_name().as_ref() == key.as_bytes())
            // The decoder comes from the element the attribute was read
            // from, so it matches the document's declared encoding
            // rather than assuming UTF-8. `Implicit1_0` is quick-xml's
            // name for "no XML declaration seen", the right assumption
            // for a fragment taken out of an attribute.
            .and_then(|a| {
                a.decoded_and_normalized_value(XmlVersion::Implicit1_0, element.decoder())
                    .ok()
            })
            .map(|v| v.into_owned())
    };

    let rect = attribute("rect").and_then(|value| {
        let parts = parse_number_list(&value);
        (parts.len() == 4).then(|| [parts[0], parts[1], parts[2], parts[3]])
    })?;

    Some(XfdfAnnotation {
        kind: name.to_string(),
        // A missing or unparseable page means page 1, which is where a
        // reader would put it; better than discarding the annotation.
        page: attribute("page")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0),
        rect,
        color: attribute("color")
            .and_then(|v| parse_hex_color(&v))
            .unwrap_or([0.0, 0.0, 0.0]),
        opacity: attribute("opacity")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
        contents: None,
        title: attribute("title"),
        quads: attribute("coords")
            .map(|value| parse_number_list(&value).as_chunks::<8>().0.to_vec())
            .unwrap_or_default(),
        ink: Vec::new(),
    })
}

fn parse_number_list(value: &str) -> Vec<f32> {
    value
        .split([',', ' ', '\n', '\t', '\r'])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .collect()
}

/// One `<gesture>` body: `x,y;x,y;…`.
fn parse_gesture(value: &str) -> Vec<(f32, f32)> {
    value
        .split(';')
        .filter_map(|point| {
            let numbers = parse_number_list(point);
            (numbers.len() == 2).then(|| (numbers[0], numbers[1]))
        })
        .collect()
}

/// Adds every annotation this app knows how to create into `doc`.
///
/// Anything else is counted, not approximated: a stamp imported as a
/// rectangle is not what the sender drew, and silently changing someone
/// else's markup is worse than telling the user two of their forty
/// comments couldn't be brought across.
pub fn import(
    doc: &mut Document,
    annotations: &[XfdfAnnotation],
) -> Result<ImportReport, XfdfError> {
    let page_count = doc.page_count()?;
    let mut report = ImportReport::default();

    for annotation in annotations {
        if annotation.page >= page_count {
            report.out_of_range += 1;
            continue;
        }
        let Some(kind) = to_annotation_kind(annotation) else {
            report.skipped += 1;
            continue;
        };

        add_annotation(
            doc,
            annotation.page,
            NewAnnotation {
                rect: Rect {
                    x0: annotation.rect[0],
                    y0: annotation.rect[1],
                    x1: annotation.rect[2],
                    y1: annotation.rect[3],
                },
                color: Color {
                    r: annotation.color[0],
                    g: annotation.color[1],
                    b: annotation.color[2],
                },
                kind,
                contents: annotation.contents.clone(),
                opacity: annotation.opacity,
            },
        )?;
        report.imported += 1;
    }
    Ok(report)
}

fn to_annotation_kind(annotation: &XfdfAnnotation) -> Option<AnnotationKind> {
    let quads = || -> Vec<Rect> {
        annotation
            .quads
            .iter()
            .map(|quad| {
                // /QuadPoints corners aren't ordered; take the extremes
                // so a quad written in any corner order still yields the
                // rectangle it describes.
                let xs = [quad[0], quad[2], quad[4], quad[6]];
                let ys = [quad[1], quad[3], quad[5], quad[7]];
                Rect {
                    x0: xs.iter().copied().fold(f32::INFINITY, f32::min),
                    y0: ys.iter().copied().fold(f32::INFINITY, f32::min),
                    x1: xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                    y1: ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                }
            })
            .collect()
    };

    match annotation.kind.as_str() {
        "highlight" => Some(AnnotationKind::Highlight { quads: quads() }),
        "underline" => Some(AnnotationKind::Underline { quads: quads() }),
        "strikeout" => Some(AnnotationKind::StrikeOut { quads: quads() }),
        "ink" if !annotation.ink.is_empty() => Some(AnnotationKind::Ink {
            strokes: annotation.ink.clone(),
        }),
        // A `text` annotation is a sticky note: no drawn geometry of its
        // own, just a comment anchored at a point. Rendered here as the
        // free-text box this app can create, carrying the note's words.
        "freetext" | "text" => Some(AnnotationKind::FreeText {
            text: annotation.contents.clone().unwrap_or_default(),
            font_size: 11.0,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<XfdfAnnotation> {
        vec![
            XfdfAnnotation {
                kind: "highlight".into(),
                page: 2,
                rect: [100.0, 200.0, 300.0, 220.0],
                color: [1.0, 1.0, 0.0],
                opacity: 0.4,
                contents: Some("Check this clause".into()),
                title: Some("Dana".into()),
                quads: vec![[100.0, 220.0, 300.0, 220.0, 100.0, 200.0, 300.0, 200.0]],
                ink: Vec::new(),
            },
            XfdfAnnotation {
                kind: "ink".into(),
                page: 0,
                rect: [10.0, 10.0, 60.0, 40.0],
                color: [1.0, 0.0, 0.0],
                opacity: 1.0,
                contents: None,
                title: None,
                quads: Vec::new(),
                ink: vec![vec![(10.0, 10.0), (30.0, 25.5), (60.0, 40.0)]],
            },
        ]
    }

    #[test]
    fn a_round_trip_preserves_every_field() {
        let xml = to_xfdf(&sample(), Some("contract.pdf")).expect("export should succeed");
        let parsed = from_xfdf(&xml).expect("import should succeed");
        assert_eq!(parsed, sample());
    }

    #[test]
    fn the_output_is_a_well_formed_xfdf_document() {
        let xml = to_xfdf(&sample(), Some("contract.pdf")).expect("export should succeed");
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("xmlns=\"http://ns.adobe.com/xfdf/\""));
        // Without this, a reader may collapse the whitespace inside
        // <contents> and quietly reflow every comment.
        assert!(xml.contains("xml:space=\"preserve\""));
        assert!(xml.contains("href=\"contract.pdf\""));
    }

    /// XFDF pages are 0-based while the numbers a user sees are 1-based.
    /// Getting it wrong puts every imported comment one page off, which
    /// looks plausible enough to go unnoticed.
    #[test]
    fn pages_stay_zero_based_across_a_round_trip() {
        let xml = to_xfdf(&sample(), None).unwrap();
        assert!(xml.contains("page=\"2\""), "{xml}");
        assert_eq!(from_xfdf(&xml).unwrap()[0].page, 2);
    }

    #[test]
    fn coordinates_are_written_without_trailing_decimals() {
        let xml = to_xfdf(&sample(), None).unwrap();
        assert!(xml.contains("rect=\"100,200,300,220\""), "{xml}");
        // ...but a real fraction survives.
        assert!(xml.contains("25.5"), "{xml}");
    }

    #[test]
    fn colours_round_trip_through_hex() {
        assert_eq!(hex_color([1.0, 1.0, 0.0]), "#FFFF00");
        assert_eq!(hex_color([0.0, 0.0, 0.0]), "#000000");
        assert_eq!(parse_hex_color("#FFFF00"), Some([1.0, 1.0, 0.0]));
        assert_eq!(parse_hex_color("FFFF00"), Some([1.0, 1.0, 0.0]));
        assert_eq!(parse_hex_color("#GGG"), None);
    }

    /// Comment text must survive XML escaping — an ampersand or an angle
    /// bracket in a note is ordinary, and mangling it corrupts what
    /// someone wrote.
    #[test]
    fn special_characters_in_a_comment_survive() {
        let annotations = vec![XfdfAnnotation {
            kind: "text".into(),
            page: 0,
            rect: [0.0, 0.0, 10.0, 10.0],
            color: [0.0, 0.0, 0.0],
            opacity: 1.0,
            contents: Some("Terms & conditions <see §4> \"as amended\"".into()),
            title: None,
            quads: Vec::new(),
            ink: Vec::new(),
        }];
        let xml = to_xfdf(&annotations, None).unwrap();
        assert_eq!(
            from_xfdf(&xml).unwrap()[0].contents,
            annotations[0].contents
        );
    }

    #[test]
    fn a_file_that_is_not_xfdf_is_rejected_with_a_useful_message() {
        let err = from_xfdf("<html><body>not xfdf</body></html>").unwrap_err();
        assert!(matches!(err, XfdfError::Malformed(_)));
        assert!(from_xfdf("").is_err());
    }

    #[test]
    fn an_annotation_kind_this_app_cannot_draw_is_reported_not_approximated() {
        let stamp = XfdfAnnotation {
            kind: "stamp".into(),
            page: 0,
            rect: [0.0, 0.0, 50.0, 50.0],
            color: [0.0, 0.0, 0.0],
            opacity: 1.0,
            contents: None,
            title: None,
            quads: Vec::new(),
            ink: Vec::new(),
        };
        assert!(
            to_annotation_kind(&stamp).is_none(),
            "a stamp imported as something else is not what the sender drew"
        );
    }

    /// `/QuadPoints` corners are not required to be in any particular
    /// order, so the rectangle has to come from the extremes.
    #[test]
    fn quad_corners_in_any_order_yield_the_same_rectangle() {
        let mut annotation = sample()[0].clone();
        // Reverse the corner order.
        annotation.quads = vec![[300.0, 200.0, 100.0, 200.0, 300.0, 220.0, 100.0, 220.0]];
        let Some(AnnotationKind::Highlight { quads }) = to_annotation_kind(&annotation) else {
            panic!("expected a highlight");
        };
        assert_eq!(quads.len(), 1);
        assert_eq!((quads[0].x0, quads[0].y0), (100.0, 200.0));
        assert_eq!((quads[0].x1, quads[0].y1), (300.0, 220.0));
    }

    /// Text split across several XML events (which an entity reference
    /// causes) must be joined, not truncated to the last fragment.
    #[test]
    fn contents_split_by_an_entity_are_reassembled() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<xfdf xmlns="http://ns.adobe.com/xfdf/" xml:space="preserve"><annots>
<highlight page="0" rect="0,0,10,10" color="#FFFF00" opacity="1">
<contents>before &amp; after</contents></highlight></annots></xfdf>"##;
        assert_eq!(
            from_xfdf(xml).unwrap()[0].contents.as_deref(),
            Some("before & after")
        );
    }
}
