//! Tiled page watermarks — a PDF replication of OpenCapture's watermark
//! tool (its canvas renderer lives in that project's
//! `vendor-private/watermark-premium/src/watermark.ts`; the tiling math
//! here is a 1:1 port, re-based from canvas pixels onto PDF points and
//! from a y-down to a y-up axis).
//!
//! The pattern: one "cell" (optional logo above optional text, white
//! fill with a black stroke so it stays legible over light *and* dark
//! content) repeated in an axis-aligned grid across one or more bands of
//! each page — the whole page, or a single row at the top and/or bottom
//! edge. Each cell is rotated in place (0° or 45°) around its own
//! center, which keeps the grid itself axis-aligned (reliably covering
//! corners and edges) while the content reads as the conventional
//! diagonal stamp.
//!
//! Everything is baked into the page as an appended content stream via
//! [`Document::wrap_and_append_page_content`], so the original content
//! is wrapped in `q…Q` and the watermark always draws in the page's
//! initial coordinate system. Opacity rides an `/ExtGState` (`ca`/`CA`),
//! the logo is an RGB image XObject with an `/SMask` alpha channel
//! (callers hand raw RGBA — decoding PNG/JPEG stays the UI's job, where
//! a canvas already does it for free), and text uses the same
//! WinAnsi-encoded Helvetica the annotation sidecar already registers
//! via [`Document::ensure_page_font`].

use lopdf::{dictionary, Object, ObjectId};
use openpdfedit_doc::{DocError, Document};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatermarkError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error("a watermark needs text, a logo, or both")]
    Empty,
    #[error("opacity must be within 0..=1, got {0}")]
    BadOpacity(f32),
    #[error("orientation must be 0 or 45 degrees, got {0}")]
    BadOrientation(u16),
    #[error("text scale must be a positive finite number, got {0}")]
    BadTextScale(f32),
    #[error("unknown watermark location {0:?} (expected top, bottom, top-bottom or full)")]
    BadLocation(String),
    #[error("logo buffer is {len} bytes but {width}x{height} RGBA needs {expected}")]
    BadLogoBuffer {
        len: usize,
        width: u32,
        height: u32,
        expected: usize,
    },
}

/// Which band(s) of each page the pattern covers. Same vocabulary as the
/// OpenCapture tool: the edge variants are exactly one cell-row tall, so
/// they read as a single line of repeated stamps rather than a filled
/// strip; `Full` tiles the whole page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkLocation {
    Top,
    Bottom,
    TopBottom,
    Full,
}

impl std::str::FromStr for WatermarkLocation {
    type Err = WatermarkError;
    fn from_str(s: &str) -> Result<Self, WatermarkError> {
        match s {
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            "top-bottom" => Ok(Self::TopBottom),
            "full" => Ok(Self::Full),
            other => Err(WatermarkError::BadLocation(other.to_string())),
        }
    }
}

/// A decoded logo image: raw 8-bit RGBA scanlines, top row first — what
/// `CanvasRenderingContext2D::getImageData` hands the UI verbatim.
#[derive(Debug, Clone)]
pub struct LogoRgba {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct WatermarkOptions {
    /// May be empty when `logo` is present.
    pub text: String,
    pub location: WatermarkLocation,
    /// `0` or `45`.
    pub orientation_deg: u16,
    /// `0.0..=1.0`, applied via `/ExtGState` to fills and strokes alike.
    pub opacity: f32,
    /// Multiplies the otherwise-automatic font size (see [`cell_font_size`]).
    pub text_scale: f32,
    /// How many tiles fit across a page, relative to the original
    /// OpenCapture pattern: `1.0` is that pattern, lower is sparser,
    /// higher is denser. Clamped to [`MIN_DENSITY`]..=[`MAX_DENSITY`].
    pub density: f32,
    pub logo: Option<LogoRgba>,
    /// 0-based page indexes; `None` = every page.
    pub pages: Option<Vec<u32>>,
}

/// One tile's box, proportional to the page's own width so the pattern
/// reads consistently across page sizes — the exact constants of the
/// OpenCapture original (`watermarkCellSize`), with its pixel floors
/// carried over as point floors.
pub fn cell_size(page_width: f32) -> (f32, f32) {
    // Density is deliberately *not* a factor here. It used to divide
    // this, which made a lower density draw a bigger mark fewer times —
    // and since the font size is fitted to the cell (see
    // [`cell_font_size`]), turning the dial down grew the lettering
    // instead of spreading it out. That is not what the word means, and
    // not what anyone reaching for it wants: density is how many marks
    // land on a page, not how large each one is. It now only affects the
    // gap between cells — see [`cell_gap`].
    let width = (page_width * 0.16).max(40.0);
    let height = (width * 0.5).max(24.0);
    (width, height)
}

/// The space left between one cell and the next.
///
/// This is where `density` lives now. At `1.0` the gap is half a cell in
/// each direction, which is exactly what the tiling did before density
/// was separated from size — so a watermark applied at the default is
/// unchanged, down to the operator stream.
///
/// Lower density widens the gap without touching the mark; higher
/// narrows it. The divisor keeps the gap positive at every allowed
/// density, so cells never overlap however far the dial is pushed —
/// at [`MAX_DENSITY`] they still sit about a sixth of a cell apart.
pub fn cell_gap(cell_w: f32, cell_h: f32, density: f32) -> (f32, f32) {
    let density = density.clamp(MIN_DENSITY, MAX_DENSITY);
    (cell_w * 0.5 / density, cell_h * 0.5 / density)
}

/// Bounds on [`WatermarkOptions::density`]. The floor is one cell on a
/// Letter page (any sparser is a single stamp, which is what the
/// numbering tool is for); the ceiling is where the cells collide with
/// the 40pt width floor and stop getting denser anyway.
pub const MIN_DENSITY: f32 = 0.15;
pub const MAX_DENSITY: f32 = 3.0;

/// The automatic font size for `text` inside one cell (before
/// `text_scale`): fits the cell height and shrinks with the text's
/// length so long strings stay inside the box — `drawWatermarkCell`'s
/// formula verbatim.
pub fn cell_font_size(
    text_area_height: f32,
    cell_width: f32,
    text_len: usize,
    text_scale: f32,
) -> f32 {
    let fit = (text_area_height * 0.7).min(cell_width / (text_len.max(1) as f32) * 1.7);
    fit.max(10.0) * text_scale
}

/// A band in page-canvas space: `y` measured *downward* from the page's
/// top edge, like the canvas original — [`page_watermark_ops`] flips to
/// PDF's y-up axis at emit time, keeping this math diff-able against the
/// TS reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Port of `watermarkBands`: which band(s) a location covers. Edge bands
/// are exactly one cell tall; `TopBottom` is one independent row at each
/// edge, never allowed to overlap even on an unusually short page.
pub fn bands(location: WatermarkLocation, page_w: f32, page_h: f32, density: f32) -> Vec<Band> {
    if location == WatermarkLocation::Full {
        return vec![Band {
            x: 0.0,
            y: 0.0,
            width: page_w,
            height: page_h,
        }];
    }
    // An edge band is one cell tall — the mark's own height, which no
    // longer moves with density.
    let _ = density;
    let (_, cell_h) = cell_size(page_w);
    match location {
        WatermarkLocation::Top => vec![Band {
            x: 0.0,
            y: 0.0,
            width: page_w,
            height: cell_h.min(page_h),
        }],
        WatermarkLocation::Bottom => {
            let height = cell_h.min(page_h);
            vec![Band {
                x: 0.0,
                y: page_h - height,
                width: page_w,
                height,
            }]
        }
        WatermarkLocation::TopBottom => {
            let height = cell_h.min((page_h / 2.0).floor());
            vec![
                Band {
                    x: 0.0,
                    y: 0.0,
                    width: page_w,
                    height,
                },
                Band {
                    x: 0.0,
                    y: page_h - height,
                    width: page_w,
                    height,
                },
            ]
        }
        WatermarkLocation::Full => unreachable!(),
    }
}

/// Escapes a string for a PDF literal string `( … )`, mapping characters
/// outside WinAnsi's byte range to `?` — the registered Helvetica is
/// WinAnsi-encoded, so anything wider would render as garbage anyway.
fn escape_pdf_text(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 4);
    for ch in text.chars() {
        let code = ch as u32;
        if code > 255 {
            out.push(b'?');
            continue;
        }
        let byte = code as u8;
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out
}

fn fmt(v: f32) -> String {
    // Content streams don't need more precision than 1/100 pt, and fixed
    // formatting keeps the output diff-stable for tests.
    format!("{v:.2}")
}

/// Emits the operators for one cell's content, in a frame whose origin is
/// the cell's center, y-up (the caller has already translated/rotated).
/// Mirrors `drawWatermarkCell`: logo (if any) fills the top of the cell
/// preserving aspect ratio, text sits below it, or centered alone.
fn cell_ops(
    ops: &mut String,
    cell_w: f32,
    cell_h: f32,
    text: &str,
    font_name: Option<&str>,
    logo: Option<(&str, u32, u32)>,
    text_scale: f32,
) {
    let has_text = !text.is_empty() && font_name.is_some();
    let logo_area_h = match (&logo, has_text) {
        (Some(_), true) => cell_h * 0.6,
        _ => cell_h,
    };

    if let Some((logo_name, px_w, px_h)) = logo {
        let scale = (cell_w / px_w as f32).min(logo_area_h / px_h as f32);
        let lw = px_w as f32 * scale;
        let lh = px_h as f32 * scale;
        // Canvas placed the logo centered in the top `logo_area_h` of the
        // cell; in this y-up frame that puts its bottom-left corner at:
        let x = -lw / 2.0;
        let y = cell_h / 2.0 - (logo_area_h + lh) / 2.0;
        ops.push_str(&format!(
            "q\n{} 0 0 {} {} {} cm\n/{} Do\nQ\n",
            fmt(lw),
            fmt(lh),
            fmt(x),
            fmt(y),
            logo_name,
        ));
    }

    if has_text {
        let text_area_h = cell_h - if logo.is_some() { logo_area_h } else { 0.0 };
        let len = text.chars().count();
        let font_size = cell_font_size(text_area_h, cell_w, len, text_scale);
        // Approximate Helvetica advance at 0.5 em — the same
        // approximation openpdfedit-annot's FreeText wrapping uses.
        let approx_width = font_size * 0.5 * len as f32;
        // Canvas centered the text (middle baseline) in the area below
        // the logo — with a logo that area starts `logo_area_h` down from
        // the cell top, alone it is the whole cell. Then down 0.35 em
        // from that optical center to the real baseline so cap-height
        // glyphs sit visually centered.
        let text_top_offset = if logo.is_some() { logo_area_h } else { 0.0 };
        let center_y = cell_h / 2.0 - text_top_offset - text_area_h / 2.0;
        let baseline_y = center_y - font_size * 0.35;
        let line_width = (font_size / 8.0).max(2.0);
        ops.push_str(&format!(
            "{} w\n0 0 0 RG\n1 1 1 rg\nBT\n/{} {} Tf\n2 Tr\n{} {} Td\n(",
            fmt(line_width),
            font_name.unwrap_or("F0"),
            fmt(font_size),
            fmt(-approx_width / 2.0),
            fmt(baseline_y),
        ));
        ops.push_str(&String::from_utf8_lossy(&escape_pdf_text(text)));
        ops.push_str(") Tj\nET\n");
    }
}

/// Builds the appended content-stream operators for one page: an outer
/// `q … Q` with the opacity ExtGState set, then every cell of every band
/// — the port of `drawWatermarkTile`, with band coordinates flipped from
/// canvas y-down to PDF y-up at the point of emission, offset by the
/// MediaBox origin.
#[allow(clippy::too_many_arguments)]
fn page_watermark_ops(
    media_box: [f32; 4],
    opts: &WatermarkOptions,
    gs_name: &str,
    font_name: Option<&str>,
    logo: Option<(&str, u32, u32)>,
) -> String {
    let page_w = media_box[2] - media_box[0];
    let page_h = media_box[3] - media_box[1];
    let (cell_w, cell_h) = cell_size(page_w);
    let (gap_x, gap_y) = cell_gap(cell_w, cell_h, opts.density);
    let stride_x = cell_w + gap_x;
    let stride_y = cell_h + gap_y;

    // Canvas rotate(-45°) with a y-down axis reads as a rising
    // bottom-left-to-top-right diagonal; with PDF's y-up axis the same
    // visual is a +45° rotation.
    let angle_rad = (opts.orientation_deg as f32).to_radians();
    let (cos, sin) = (angle_rad.cos(), angle_rad.sin());

    let mut ops = String::from("q\n");
    ops.push_str(&format!("/{gs_name} gs\n"));

    for band in bands(opts.location, page_w, page_h, opts.density) {
        let cols = ((band.width / stride_x).ceil() as i64).max(1);
        let rows = ((band.height / stride_y).ceil() as i64).max(1);
        for row in 0..rows {
            for col in 0..cols {
                // Cell center in canvas space (y down from page top)…
                let cx = band.x + gap_x / 2.0 + col as f32 * stride_x + cell_w / 2.0;
                let cy = band.y + gap_y / 2.0 + row as f32 * stride_y + cell_h / 2.0;
                // …flipped to PDF space and offset by the MediaBox origin.
                let px = media_box[0] + cx;
                let py = media_box[1] + (page_h - cy);
                ops.push_str(&format!("q\n1 0 0 1 {} {} cm\n", fmt(px), fmt(py)));
                if opts.orientation_deg != 0 {
                    ops.push_str(&format!(
                        "{} {} {} {} 0 0 cm\n",
                        fmt(cos),
                        fmt(sin),
                        fmt(-sin),
                        fmt(cos),
                    ));
                }
                cell_ops(
                    &mut ops,
                    cell_w,
                    cell_h,
                    &opts.text,
                    font_name,
                    logo,
                    opts.text_scale,
                );
                ops.push_str("Q\n");
            }
        }
    }
    ops.push_str("Q\n");
    ops
}

/// Embeds the logo as a DeviceRGB image XObject with a DeviceGray
/// `/SMask` carrying the alpha channel, both Flate-compressed. Returns
/// the image's object id (the SMask is referenced from its dict).
fn embed_logo(doc: &mut Document, logo: &LogoRgba) -> Result<ObjectId, WatermarkError> {
    let expected = logo.width as usize * logo.height as usize * 4;
    if logo.rgba.len() != expected || expected == 0 {
        return Err(WatermarkError::BadLogoBuffer {
            len: logo.rgba.len(),
            width: logo.width,
            height: logo.height,
            expected,
        });
    }
    let pixel_count = expected / 4;
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    let mut alpha = Vec::with_capacity(pixel_count);
    for px in logo.rgba.as_chunks::<4>().0 {
        rgb.extend_from_slice(&px[..3]);
        alpha.push(px[3]);
    }

    let mut smask = lopdf::Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => logo.width as i64,
            "Height" => logo.height as i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        },
        alpha,
    );
    // Best-effort: an uncompressed image is bigger but still correct.
    let _ = smask.compress();
    let smask_id = doc.add_object(Object::Stream(smask));

    let mut image = lopdf::Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => logo.width as i64,
            "Height" => logo.height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "SMask" => smask_id,
        },
        rgb,
    );
    let _ = image.compress();
    Ok(doc.add_object(Object::Stream(image)))
}

/// Bakes the watermark into every requested page of `doc` (the working
/// copy only — callers persist via the usual incremental-save path).
pub fn apply_watermark(doc: &mut Document, opts: &WatermarkOptions) -> Result<(), WatermarkError> {
    if opts.text.trim().is_empty() && opts.logo.is_none() {
        return Err(WatermarkError::Empty);
    }
    if !(0.0..=1.0).contains(&opts.opacity) || !opts.opacity.is_finite() {
        return Err(WatermarkError::BadOpacity(opts.opacity));
    }
    if opts.orientation_deg != 0 && opts.orientation_deg != 45 {
        return Err(WatermarkError::BadOrientation(opts.orientation_deg));
    }
    if !opts.text_scale.is_finite() || opts.text_scale <= 0.0 {
        return Err(WatermarkError::BadTextScale(opts.text_scale));
    }

    let page_count = doc.page_count()?;
    let pages: Vec<u32> = match &opts.pages {
        Some(list) => list.clone(),
        None => (0..page_count).collect(),
    };

    // Shared across pages: one ExtGState, one logo XObject pair.
    let gs_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "ExtGState",
        "ca" => opts.opacity,
        "CA" => opts.opacity,
    }));
    let logo_id = match &opts.logo {
        Some(logo) => Some((embed_logo(doc, logo)?, logo.width, logo.height)),
        None => None,
    };
    let has_text = !opts.text.trim().is_empty();

    for &page in &pages {
        let media_box = doc.page_media_box(page)?;
        let gs_name = doc.merge_page_resource(page, "ExtGState", "OPEWmGs", gs_id)?;
        let font_name = if has_text {
            Some(doc.ensure_page_font(page, "Helvetica")?)
        } else {
            None
        };
        let logo_name = match logo_id {
            Some((id, w, h)) => Some((
                doc.merge_page_resource(page, "XObject", "OPEWmLogo", id)?,
                w,
                h,
            )),
            None => None,
        };
        let ops = page_watermark_ops(
            media_box,
            opts,
            &gs_name,
            font_name.as_deref(),
            logo_name.as_ref().map(|(n, w, h)| (n.as_str(), *w, *h)),
        );
        doc.wrap_and_append_page_content(page, ops.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::Stream;

    fn n_page_pdf_bytes(n: usize) -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_ids: Vec<ObjectId> = (0..n)
            .map(|_| {
                let content = Content {
                    operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
                };
                let content_id =
                    doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                    "Contents" => content_id,
                    "Resources" => dictionary! {},
                })
            })
            .collect();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
                "Count" => n as i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn text_options(text: &str, location: WatermarkLocation) -> WatermarkOptions {
        WatermarkOptions {
            text: text.to_string(),
            location,
            orientation_deg: 45,
            opacity: 0.5,
            text_scale: 1.0,
            density: 1.0,
            logo: None,
            pages: None,
        }
    }

    /// Density reads the way the word does — higher packs more cells in
    /// — and 1.0 has to stay bit-identical to the pattern every caller
    /// got before the parameter existed.
    #[test]
    fn density_spaces_the_marks_out_without_resizing_them() {
        // Reported: turning density down made the watermark bigger and
        // fewer, when what it should do is leave the mark alone and put
        // more space between copies. It used to divide the cell, and the
        // font size is fitted to the cell, so the lettering grew with it.
        let (base_w, base_h) = cell_size(612.0);
        let (sparse_gx, sparse_gy) = cell_gap(base_w, base_h, 0.5);
        let (gx, gy) = cell_gap(base_w, base_h, 1.0);
        let (dense_gx, dense_gy) = cell_gap(base_w, base_h, 2.0);

        assert!(
            sparse_gx > gx && sparse_gy > gy,
            "lower density = more space between marks"
        );
        assert!(
            dense_gx < gx && dense_gy < gy,
            "higher density = less space between marks"
        );
        // Twice as sparse means twice the gap, not twice the mark.
        assert_eq!(sparse_gx, gx * 2.0);
    }

    /// Whatever the density, the mark itself is the same size — which is
    /// the whole point of the change, and the thing a caller would most
    /// easily undo by reintroducing density into `cell_size`.
    #[test]
    fn the_mark_is_the_same_size_at_every_density() {
        let base = cell_size(612.0);
        for density in [MIN_DENSITY, 0.5, 1.0, 2.0, MAX_DENSITY] {
            assert_eq!(cell_size(612.0), base, "density {density} changed the cell");
        }
    }

    /// Cells must not overlap however far the dial is pushed: the gap
    /// stays positive at the densest allowed setting.
    #[test]
    fn marks_never_collide_even_at_maximum_density() {
        let (w, h) = cell_size(612.0);
        let (gx, gy) = cell_gap(w, h, MAX_DENSITY);
        assert!(gx > 0.0 && gy > 0.0, "cells would overlap at max density");
    }

    #[test]
    fn density_is_clamped_rather_than_allowed_to_collapse_the_tile() {
        // A zero or negative density would divide the tile to nothing (or
        // flip it), so the bounds are enforced in cell_size itself rather
        // than trusted from the caller.
        let (w, h) = cell_size(612.0);
        assert_eq!(cell_gap(w, h, 0.0), cell_gap(w, h, MIN_DENSITY));
        assert_eq!(cell_gap(w, h, -3.0), cell_gap(w, h, MIN_DENSITY));
        assert_eq!(cell_gap(w, h, 99.0), cell_gap(w, h, MAX_DENSITY));
    }

    #[test]
    fn cell_size_matches_the_opencapture_reference_constants() {
        // 612 pt US-Letter width: 0.16 * 612 = 97.92 wide, half that tall.
        assert_eq!(cell_size(612.0), (97.92, 48.96));
        // A tiny page hits the 40/24 floors.
        assert_eq!(cell_size(100.0), (40.0, 24.0));
    }

    #[test]
    fn bands_cover_the_right_regions() {
        let full = bands(WatermarkLocation::Full, 612.0, 792.0, 1.0);
        assert_eq!(full.len(), 1);
        assert_eq!((full[0].width, full[0].height), (612.0, 792.0));

        let (_, cell_h) = cell_size(612.0);
        let top = bands(WatermarkLocation::Top, 612.0, 792.0, 1.0);
        assert_eq!(top.len(), 1);
        assert_eq!((top[0].y, top[0].height), (0.0, cell_h));

        let bottom = bands(WatermarkLocation::Bottom, 612.0, 792.0, 1.0);
        assert_eq!(bottom[0].y, 792.0 - cell_h);

        let both = bands(WatermarkLocation::TopBottom, 612.0, 792.0, 1.0);
        assert_eq!(both.len(), 2);
        assert!(
            both[0].y + both[0].height <= both[1].y,
            "rows must never overlap"
        );
    }

    #[test]
    fn escape_pdf_text_escapes_delimiters_and_replaces_wide_chars() {
        assert_eq!(escape_pdf_text(r"a(b)c\d"), b"a\\(b\\)c\\\\d".to_vec());
        assert_eq!(escape_pdf_text("héllo — ok"), b"h\xe9llo ? ok".to_vec());
    }

    #[test]
    fn apply_watermark_rejects_bad_options() {
        let mut doc = Document::from_bytes(&n_page_pdf_bytes(1)).unwrap();
        let empty = WatermarkOptions {
            text: "  ".into(),
            ..text_options("", WatermarkLocation::Full)
        };
        assert!(matches!(
            apply_watermark(&mut doc, &empty),
            Err(WatermarkError::Empty)
        ));
        let bad_opacity = WatermarkOptions {
            opacity: 1.5,
            ..text_options("DRAFT", WatermarkLocation::Full)
        };
        assert!(matches!(
            apply_watermark(&mut doc, &bad_opacity),
            Err(WatermarkError::BadOpacity(_))
        ));
        let bad_angle = WatermarkOptions {
            orientation_deg: 30,
            ..text_options("DRAFT", WatermarkLocation::Full)
        };
        assert!(matches!(
            apply_watermark(&mut doc, &bad_angle),
            Err(WatermarkError::BadOrientation(30))
        ));
        let bad_logo = WatermarkOptions {
            text: String::new(),
            logo: Some(LogoRgba {
                rgba: vec![0; 10],
                width: 2,
                height: 2,
            }),
            ..text_options("", WatermarkLocation::Full)
        };
        assert!(matches!(
            apply_watermark(&mut doc, &bad_logo),
            Err(WatermarkError::BadLogoBuffer { .. })
        ));
    }

    #[test]
    fn full_watermark_text_survives_save_and_is_extractable_on_every_page() {
        let mut doc = Document::from_bytes(&n_page_pdf_bytes(3)).unwrap();
        apply_watermark(
            &mut doc,
            &text_options("CONFIDENTIAL", WatermarkLocation::Full),
        )
        .expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).expect("saved bytes should reload");
        for page in 1..=3u32 {
            let text = reopened.extract_text(&[page]).expect("text extraction");
            assert!(
                text.contains("CONFIDENTIAL"),
                "page {page} must carry the watermark text, got: {text:?}"
            );
        }
    }

    #[test]
    fn pages_subset_only_stamps_the_requested_pages() {
        let mut doc = Document::from_bytes(&n_page_pdf_bytes(3)).unwrap();
        let opts = WatermarkOptions {
            pages: Some(vec![1]),
            ..text_options("DRAFT", WatermarkLocation::Full)
        };
        apply_watermark(&mut doc, &opts).expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");
        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        assert!(!reopened.extract_text(&[1]).unwrap().contains("DRAFT"));
        assert!(reopened.extract_text(&[2]).unwrap().contains("DRAFT"));
        assert!(!reopened.extract_text(&[3]).unwrap().contains("DRAFT"));
    }

    #[test]
    fn edge_band_emits_one_row_and_full_emits_many() {
        let opts_top = text_options("DRAFT", WatermarkLocation::Top);
        let ops_top =
            page_watermark_ops([0.0, 0.0, 612.0, 792.0], &opts_top, "GS", Some("F"), None);
        let opts_full = text_options("DRAFT", WatermarkLocation::Full);
        let ops_full =
            page_watermark_ops([0.0, 0.0, 612.0, 792.0], &opts_full, "GS", Some("F"), None);
        let cells = |s: &str| s.matches("2 Tr").count();
        // 612/(97.92*1.5) ⇒ 5 columns; full: 792/(48.96*1.5) ⇒ 11 rows.
        assert_eq!(cells(&ops_top), 5, "one edge row of cells");
        assert_eq!(cells(&ops_full), 55, "full-page grid of cells");
        assert!(ops_full.contains("/GS gs"));
        assert!(
            ops_full.contains("0.71 0.71 -0.71 0.71 0 0 cm"),
            "45° cell rotation"
        );
    }

    #[test]
    fn logo_watermark_embeds_an_rgb_xobject_with_an_smask() {
        let mut doc = Document::from_bytes(&n_page_pdf_bytes(1)).unwrap();
        let opts = WatermarkOptions {
            text: String::new(),
            logo: Some(LogoRgba {
                rgba: vec![200; 4 * 4 * 4],
                width: 4,
                height: 4,
            }),
            ..text_options("", WatermarkLocation::Full)
        };
        apply_watermark(&mut doc, &opts).expect("should succeed");
        let saved = doc.save_incremental().expect("save should succeed");

        let reopened = lopdf::Document::load_mem(&saved).unwrap();
        let images: Vec<&lopdf::Stream> = reopened
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| s.dict.get(b"Subtype").and_then(|v| v.as_name()).ok() == Some(b"Image"))
            .collect();
        assert_eq!(images.len(), 2, "the logo image plus its SMask");
        let rgb = images
            .iter()
            .find(|s| s.dict.has(b"SMask"))
            .expect("the DeviceRGB image must reference an SMask");
        assert_eq!(
            rgb.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceRGB"
        );
        // And the page's content must actually invoke it.
        let doc2 = Document::from_bytes(&saved).unwrap();
        let content = String::from_utf8_lossy(&doc2.page_content_bytes(0).unwrap()).to_string();
        assert!(content.contains("/OPEWmLogo Do"), "got: {content}");
    }
}

#[cfg(test)]
mod density_behaviour {
    use super::*;

    /// The user-visible promise, measured on a real page rather than
    /// inferred from the helpers: turning density down must leave the
    /// lettering the same size and simply put fewer copies on the page.
    ///
    /// Counting `Tj` operators is what makes this a test of the output
    /// rather than of the arithmetic — the previous behaviour would fail
    /// it on the font size while passing every unit assertion about
    /// cells and gaps.
    #[test]
    fn lower_density_means_fewer_marks_at_the_same_size() {
        let media_box = [0.0, 0.0, 612.0, 792.0];
        let render = |density: f32| {
            let opts = WatermarkOptions {
                text: "CONFIDENTIAL".to_string(),
                location: WatermarkLocation::Full,
                orientation_deg: 0,
                opacity: 0.4,
                text_scale: 1.0,
                density,
                logo: None,
                pages: None,
            };
            page_watermark_ops(media_box, &opts, "GS0", Some("F0"), None)
        };

        let sparse = render(0.5);
        let dense = render(2.0);

        let marks = |ops: &str| ops.matches("Tj").count();
        assert!(
            marks(&sparse) < marks(&dense),
            "lower density should put fewer marks on the page, got {} vs {}",
            marks(&sparse),
            marks(&dense),
        );

        // Same font size at both densities. `Tf` carries it, and it is
        // fitted to the cell — which is exactly what used to move.
        let font_op = |ops: &str| {
            ops.lines()
                .find(|l| l.contains(" Tf"))
                .map(str::to_string)
                .expect("a text watermark must set a font size")
        };
        assert_eq!(
            font_op(&sparse),
            font_op(&dense),
            "density changed the lettering size; it must only change the spacing",
        );
    }
}
