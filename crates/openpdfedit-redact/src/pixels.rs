//! Clearing part of an image, so that redacting a line of a scan does
//! not cost the whole page.
//!
//! Every other kind of content on a page is made of operators, and an
//! operator can simply be dropped. An image is one operator painting
//! one indivisible blob of bytes: drop it and everything it showed goes
//! with it. On a scanned document — which is one full-page image, and
//! exactly the kind of document people redact — that turns "hide this
//! address" into "delete this page".
//!
//! So a partly-covered image is decoded, the covered pixels are
//! overwritten, and it is encoded again. The bytes that carried the
//! redacted content are gone from the file, which is the whole promise;
//! the rest of the picture is untouched.
//!
//! ## What it can open
//!
//! Ordinary 8-bit-per-component greyscale, RGB and CMYK samples, stored
//! either as raw/Flate samples or as JPEG (`DCTDecode`). That is what
//! scanners, phone cameras and PDF writers actually produce.
//!
//! Everything else — 1-bit fax (`CCITTFaxDecode`), JPEG 2000, JBIG2,
//! indexed palettes, images with a `/Decode` array remapping the
//! samples, stencil masks — returns `None`, and the caller falls back
//! to dropping the image whole. That is a worse picture but not a worse
//! redaction: the content still goes. Guessing at a format's layout and
//! clearing the wrong bytes would be the one outcome that is actually
//! dangerous, because it would look redacted and not be.
//!
//! ## Re-encoding
//!
//! A JPEG is written back as a JPEG, at quality 90. It is a second
//! generation of a lossy format, which is a real if small cost — and
//! the alternative is worse: stored losslessly, a full-page colour scan
//! goes from about half a megabyte to eight, on every redaction.
//! Anything else is written back Flate-compressed, losslessly.

use image::codecs::jpeg::{JpegDecoder, JpegEncoder};
use image::{ColorType, ImageDecoder};
use lopdf::{Dictionary, Object, Stream};

use crate::Rect;

/// Quality for a re-encoded JPEG. High enough that the difference from
/// the original is invisible at any normal viewing size, low enough
/// that a redacted scan stays roughly the size it was.
const JPEG_QUALITY: u8 = 90;

/// What to write over the redacted pixels.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Fill {
    /// Blank: white in grey and RGB, no ink in CMYK.
    White,
    /// Nothing at all — for the soft mask beside an image, where the
    /// samples are opacity and zero means the image contributes
    /// nothing there. A mask left opaque would show the cleared white
    /// as a solid block covering whatever is underneath, which for a
    /// pen-and-highlighter overlay is most of the page.
    Transparent,
}

/// How the image's samples were stored, and so how to store them again.
#[derive(Clone, Copy, PartialEq)]
enum Encoding {
    /// Raw samples, or Flate-compressed ones.
    Samples,
    Jpeg,
}

/// A decoded image: 8 bits per component, one row after another from
/// the top down, which is how both PDF image space and every decoder
/// here lay them out.
struct Bitmap {
    width: u32,
    height: u32,
    components: usize,
    samples: Vec<u8>,
    encoding: Encoding,
}

/// The component count for a colour space, or `None` for one whose
/// samples are not straightforwardly `n` bytes per pixel.
///
/// `Indexed` is deliberately absent: its samples are palette offsets,
/// so writing "white" into them writes whichever colour happens to sit
/// at that palette index. The caller drops those images instead.
pub(crate) fn components_for(space: &Object, resolve: &dyn Fn(&Object) -> Object) -> Option<usize> {
    match resolve(space) {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(1),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        },
        Object::Array(items) => {
            let head = items
                .first()
                .and_then(|o| o.as_name().ok())
                .map(<[u8]>::to_vec)?;
            match head.as_slice() {
                // `/N` is the component count, and is required.
                b"ICCBased" => {
                    let stream_dict = match resolve(items.get(1)?) {
                        Object::Stream(s) => s.dict.clone(),
                        Object::Dictionary(d) => d,
                        _ => return None,
                    };
                    let n = resolve(stream_dict.get(b"N").ok()?).as_i64().ok()?;
                    matches!(n, 1 | 3 | 4).then_some(n as usize)
                }
                // A device space wearing a hat: the alternate is what
                // the samples actually are.
                b"CalRGB" => Some(3),
                b"CalGray" => Some(1),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Overwrites `regions` of the image with white and returns the
/// replacement stream, or `None` if the image is one this module will
/// not touch (see the module doc).
///
/// `regions` are in the image's own unit space — `0.0..=1.0` on each
/// axis with y pointing up, the space a `Do` maps through the CTM.
/// `data` is the stream's content after Flate/LZW have been undone,
/// which for a JPEG means the JPEG itself.
pub(crate) fn clear_regions(
    dict: &Dictionary,
    data: &[u8],
    regions: &[Rect],
    components: Option<usize>,
    fill: Fill,
) -> Option<Stream> {
    let mut bitmap = decode(dict, data, components)?;
    for region in regions {
        paint(&mut bitmap, *region, fill);
    }
    encode(dict, bitmap)
}

fn integer(dict: &Dictionary, key: &[u8]) -> Option<i64> {
    dict.get(key).ok()?.as_i64().ok()
}

/// The last filter in the chain — the one that produced the bytes
/// `decoded_stream` hands back.
fn final_filter(dict: &Dictionary) -> Option<Vec<u8>> {
    match dict.get(b"Filter").ok()? {
        Object::Name(name) => Some(name.clone()),
        Object::Array(items) => items.last()?.as_name().ok().map(<[u8]>::to_vec),
        _ => None,
    }
}

fn decode(dict: &Dictionary, data: &[u8], components: Option<usize>) -> Option<Bitmap> {
    // A stencil mask's samples are one bit each and mean "paint or
    // don't", not a colour; and a `/Decode` array remaps sample values
    // on the way out, so the byte that means white here is not
    // necessarily 255. Neither is worth guessing at.
    if dict.get(b"ImageMask").ok().and_then(|o| o.as_bool().ok()) == Some(true)
        || dict.get(b"Decode").is_ok()
    {
        return None;
    }

    let width = u32::try_from(integer(dict, b"Width")?).ok()?;
    let height = u32::try_from(integer(dict, b"Height")?).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    match final_filter(dict).as_deref() {
        Some(b"DCTDecode") | Some(b"DCT") => {
            let decoder = JpegDecoder::new(std::io::Cursor::new(data)).ok()?;
            let components = match decoder.color_type() {
                ColorType::L8 => 1,
                ColorType::Rgb8 => 3,
                // CMYK and YCCK JPEGs decode inconsistently across
                // readers; a redaction is the wrong place to find out.
                _ => return None,
            };
            let (w, h) = decoder.dimensions();
            let mut samples = vec![0u8; decoder.total_bytes() as usize];
            decoder.read_image(&mut samples).ok()?;
            Some(Bitmap {
                width: w,
                height: h,
                components,
                samples,
                encoding: Encoding::Jpeg,
            })
        }
        // Raw samples, or Flate/LZW ones that arrived already expanded.
        None | Some(b"FlateDecode") | Some(b"Fl") | Some(b"LZWDecode") | Some(b"LZW") => {
            if integer(dict, b"BitsPerComponent").unwrap_or(8) != 8 {
                return None;
            }
            let components = components?;
            let expected = (width as usize)
                .checked_mul(height as usize)?
                .checked_mul(components)?;
            // Short is corrupt; long usually means row padding this
            // code does not model. Either way, not ours to edit.
            if data.len() != expected {
                return None;
            }
            Some(Bitmap {
                width,
                height,
                components,
                samples: data.to_vec(),
                encoding: Encoding::Samples,
            })
        }
        _ => None,
    }
}

/// Writes `fill` over `region`, given in unit image space.
///
/// White rather than the redaction colour: the caller paints its own
/// box over the same spot on the page, so what is left here is only
/// what shows if that box is ever removed — and a white gap reads as
/// "nothing here", which is the truth, where a black one reads as
/// "something was here", which invites a look.
fn paint(bitmap: &mut Bitmap, region: Rect, fill: Fill) {
    let width = bitmap.width as f64;
    let height = bitmap.height as f64;

    // Outward-rounded, so a region covering 30% of a pixel clears that
    // pixel rather than leaving a sliver of the original showing.
    let x0 = (region.x0 * width).floor().max(0.0) as usize;
    let x1 = (region.x1 * width).ceil().clamp(0.0, width) as usize;
    // Unit space has y up from the bottom; rows run down from the top.
    let y0 = ((1.0 - region.y1) * height).floor().max(0.0) as usize;
    let y1 = ((1.0 - region.y0) * height).ceil().clamp(0.0, height) as usize;
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    // In CMYK, ink is what the numbers count, so no ink is zero. In
    // grey and RGB, full is white.
    let value = match fill {
        Fill::Transparent => 0u8,
        Fill::White if bitmap.components == 4 => 0u8,
        Fill::White => 255u8,
    };
    let stride = bitmap.width as usize * bitmap.components;
    for row in y0..y1 {
        let start = row * stride + x0 * bitmap.components;
        let end = row * stride + x1 * bitmap.components;
        bitmap.samples[start..end].fill(value);
    }
}

fn encode(original: &Dictionary, bitmap: Bitmap) -> Option<Stream> {
    let mut dict = original.clone();
    dict.set("Width", Object::Integer(bitmap.width as i64));
    dict.set("Height", Object::Integer(bitmap.height as i64));
    dict.set("BitsPerComponent", Object::Integer(8));

    match bitmap.encoding {
        Encoding::Jpeg => {
            let color = match bitmap.components {
                1 => ColorType::L8,
                3 => ColorType::Rgb8,
                _ => return None,
            };
            let mut out = Vec::new();
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
                .encode(&bitmap.samples, bitmap.width, bitmap.height, color.into())
                .ok()?;
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            dict.set(
                "ColorSpace",
                Object::Name(if bitmap.components == 1 {
                    b"DeviceGray".to_vec()
                } else {
                    b"DeviceRGB".to_vec()
                }),
            );
            Some(Stream::new(dict, out))
        }
        Encoding::Samples => {
            dict.remove(b"Filter");
            dict.remove(b"DecodeParms");
            let mut stream = Stream::new(dict, bitmap.samples);
            // Uncompressed samples of a full-page scan are megabytes;
            // failing to compress is survivable, so don't error on it.
            let _ = stream.compress();
            Some(stream)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// A 4×4 mid-grey RGB image, stored as raw samples.
    fn grey_rgb() -> (Dictionary, Vec<u8>) {
        (
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => 4, "Height" => 4,
                "BitsPerComponent" => 8, "ColorSpace" => "DeviceRGB",
            },
            vec![128u8; 4 * 4 * 3],
        )
    }

    fn pixel(samples: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
        let i = (y * width + x) * 3;
        [samples[i], samples[i + 1], samples[i + 2]]
    }

    /// The top-left quarter in unit space is `x 0..0.5`, `y 0.5..1` —
    /// and unit space counts y from the bottom while rows count from
    /// the top. Getting that flip wrong clears the wrong half of the
    /// picture and leaves the redacted half showing, so it is worth a
    /// test of its own.
    #[test]
    fn clears_the_named_corner_and_nothing_else() {
        let (dict, data) = grey_rgb();
        let region = Rect {
            x0: 0.0,
            y0: 0.5,
            x1: 0.5,
            y1: 1.0,
        };
        let stream =
            clear_regions(&dict, &data, &[region], Some(3), Fill::White).expect("should clear");
        let out = stream.decompressed_content().unwrap_or(stream.content);

        assert_eq!(
            pixel(&out, 4, 0, 0),
            [255, 255, 255],
            "top-left must be cleared"
        );
        assert_eq!(
            pixel(&out, 4, 1, 1),
            [255, 255, 255],
            "top-left must be cleared"
        );
        assert_eq!(
            pixel(&out, 4, 3, 0),
            [128, 128, 128],
            "top-right must survive"
        );
        assert_eq!(
            pixel(&out, 4, 0, 3),
            [128, 128, 128],
            "bottom-left must survive"
        );
        assert_eq!(
            pixel(&out, 4, 3, 3),
            [128, 128, 128],
            "bottom-right must survive"
        );
    }

    #[test]
    fn cmyk_is_cleared_to_no_ink_rather_than_to_full_ink() {
        let dict = dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 2, "Height" => 1,
            "BitsPerComponent" => 8, "ColorSpace" => "DeviceCMYK",
        };
        let region = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        };
        let stream = clear_regions(&dict, &[200u8; 8], &[region], Some(4), Fill::White)
            .expect("should clear");
        let out = stream.decompressed_content().unwrap_or(stream.content);
        assert_eq!(out, vec![0u8; 8], "255 in CMYK is solid ink, not white");
    }

    /// The formats this module refuses, each for its own reason — a
    /// palette whose samples are indices, a `/Decode` array that
    /// redefines what a sample value means, a stencil mask that has no
    /// colour at all, and a filter it cannot open. Every one of them
    /// has to come back `None` so the caller drops the image whole
    /// instead of writing bytes it has misread.
    #[test]
    fn refuses_images_it_would_have_to_guess_at() {
        let region = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        };
        let base = dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 2, "Height" => 1,
            "BitsPerComponent" => 8, "ColorSpace" => "DeviceRGB",
        };

        let mut decoded = base.clone();
        decoded.set("Decode", Object::Array(vec![1.into(), 0.into()]));
        assert!(clear_regions(&decoded, &[128u8; 6], &[region], Some(3), Fill::White).is_none());

        let mut stencil = base.clone();
        stencil.set("ImageMask", Object::Boolean(true));
        assert!(clear_regions(&stencil, &[128u8; 6], &[region], Some(3), Fill::White).is_none());

        let mut fax = base.clone();
        fax.set("Filter", Object::Name(b"CCITTFaxDecode".to_vec()));
        assert!(clear_regions(&fax, &[128u8; 6], &[region], Some(3), Fill::White).is_none());

        // An indexed palette resolves to no component count at all.
        assert!(clear_regions(&base, &[128u8; 6], &[region], None, Fill::White).is_none());

        let mut deep = base.clone();
        deep.set("BitsPerComponent", Object::Integer(16));
        assert!(clear_regions(&deep, &[128u8; 12], &[region], Some(3), Fill::White).is_none());
    }

    /// Samples that don't measure `width × height × components` are
    /// padded, truncated or laid out some way this code doesn't model,
    /// and clearing "row 3" of them would clear whatever happens to sit
    /// at that offset.
    #[test]
    fn refuses_samples_that_are_not_the_size_they_claim() {
        let (dict, _) = grey_rgb();
        let region = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        };
        assert!(clear_regions(&dict, &[128u8; 10], &[region], Some(3), Fill::White).is_none());
    }

    #[test]
    fn a_jpeg_is_cleared_and_written_back_as_a_jpeg() {
        let mut source = Vec::new();
        JpegEncoder::new_with_quality(&mut source, 95)
            .encode(&[64u8; 16 * 16 * 3], 16, 16, ColorType::Rgb8.into())
            .expect("should encode the fixture");

        let dict = dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 16, "Height" => 16,
            "BitsPerComponent" => 8, "ColorSpace" => "DeviceRGB",
            "Filter" => "DCTDecode",
        };
        let region = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.5,
            y1: 1.0,
        };
        let stream =
            clear_regions(&dict, &source, &[region], Some(3), Fill::White).expect("should clear");

        assert_eq!(
            stream.dict.get(b"Filter").unwrap(),
            &Object::Name(b"DCTDecode".to_vec()),
            "a scan must not come back as eight megabytes of raw samples"
        );

        let decoder =
            JpegDecoder::new(std::io::Cursor::new(&stream.content)).expect("should reopen");
        let mut out = vec![0u8; decoder.total_bytes() as usize];
        decoder.read_image(&mut out).expect("should decode");
        assert!(
            pixel(&out, 16, 2, 8)[0] > 200,
            "the cleared half must be white"
        );
        assert!(
            pixel(&out, 16, 13, 8)[0] < 120,
            "the other half must survive"
        );
    }
}
