//! Document compare command, moved here (from
//! `apps/desktop/src-tauri/src/compare.rs`) for the same reason as
//! [`crate::annotations`]/[`crate::pages`]: the same logic should drive
//! both the desktop's thread-wrapped `EngineHandle` and (later) a bare
//! in-process engine for the wasm/Chrome-extension build. Read-only
//! cross-document comparison, independent of the docs store's open/edit
//! lifecycle — the two documents being compared don't need to already be
//! open in this session, and nothing about the comparison itself is
//! persisted. See `openpdfedit-compare`'s module doc for exactly what
//! each mode reports and its known limitations (line-of-runs text diff,
//! not word-level; pixel diff is sensitive to any rendering difference,
//! not just visually meaningful ones).
//!
//! Split the same way [`crate::pages`]'s merge/extract are: the
//! byte-level core ([`compare_bytes`]) takes both documents as in-memory
//! buffers and is fully portable — text mode via `Document::from_bytes`,
//! pixel mode by opening both through [`Engine::open_bytes`] just long
//! enough to render and diff every page, then closing them again (this
//! stays a one-shot comparison, not an editable session, so the handles
//! never touch [`crate::SessionState::docs`]). The path-based half
//! ([`CompareRequest`]/[`compare_documents_impl`], which reads two real
//! files off disk) is desktop-only, the same `#[cfg(not(target_arch =
//! "wasm32"))]` boundary [`crate::open_document_impl`] draws.
//!
//! Two behavioral changes fell out of this move, neither observable from
//! the DTO shape (unchanged) or `CompareRequest`'s fields (unchanged):
//!
//! - The desktop's original implementation read each of the two files
//!   twice — once via `Document::open` (text mode) and once via
//!   `EngineHandle::open` (pixel mode), both path-based. Routing
//!   everything through the portable bytes-level core means
//!   [`compare_documents_impl`] now reads each file once via
//!   `std::fs::read` and feeds the same buffer to `Document::from_bytes`
//!   and [`Engine::open_bytes`].
//! - The desktop file carried a hand-rolled pixel-diff loop
//!   (`compare_pixels_via_engine`) as a workaround for `EngineHandle`'s
//!   inherent `render_page` returning `Arc<RenderedTile>` rather than the
//!   [`Engine`] trait's owned-`RenderedTile` signature. Now that
//!   `EngineHandle` implements [`Engine`] directly (see
//!   `openpdfedit-engine::thread`'s `impl Engine for EngineHandle`), that
//!   workaround's premise no longer holds, so this module calls
//!   `openpdfedit_compare::compare_pixels` (which already takes `&dyn
//!   Engine`) directly instead of reimplementing its loop.
//!
//! `Err = SessionError` throughout. [`SessionError`] needed no new
//! variant: the existing `Doc` variant already covers
//! `openpdfedit_compare::CompareError`, exactly as the desktop's own
//! `CommandError::Doc` did before the move.

use openpdfedit_compare::{compare_pixels, compare_text, PixelPageDiff, TextPageDiff};
use openpdfedit_doc::Document;
use openpdfedit_engine::Engine;
#[cfg(not(target_arch = "wasm32"))]
use serde::Deserialize;
use serde::Serialize;

use crate::SessionError;

impl From<openpdfedit_compare::CompareError> for SessionError {
    fn from(e: openpdfedit_compare::CompareError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPageDiffDto {
    page_index: u32,
    added: Vec<String>,
    removed: Vec<String>,
}

impl From<TextPageDiff> for TextPageDiffDto {
    fn from(d: TextPageDiff) -> Self {
        Self {
            page_index: d.page_index,
            added: d.added,
            removed: d.removed,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelPageDiffDto {
    page_index: u32,
    differing_pixels: u64,
    total_pixels: u64,
    /// `[left, top, right, bottom]` in the compared tiles' pixel
    /// coordinates, or `None` if nothing differed (or the page was
    /// skipped — see `openpdfedit_compare::PixelPageDiff`'s doc).
    bbox: Option<[u32; 4]>,
}

impl From<PixelPageDiff> for PixelPageDiffDto {
    fn from(d: PixelPageDiff) -> Self {
        Self {
            page_index: d.page_index,
            differing_pixels: d.differing_pixels,
            total_pixels: d.total_pixels,
            bbox: d.bbox.map(|b| [b.left, b.top, b.right, b.bottom]),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareReportDto {
    page_count_a: u32,
    page_count_b: u32,
    text_pages: Vec<TextPageDiffDto>,
    pixel_pages: Vec<PixelPageDiffDto>,
}

/// Wasm-clean byte-level core behind [`compare_documents_impl`] — see
/// this module's doc for why the split exists. `pixel_target_width` of
/// `None` skips pixel mode entirely (text-only compare needs no engine
/// round trip at all).
pub fn compare_bytes<E: Engine>(
    engine: &E,
    bytes_a: &[u8],
    bytes_b: &[u8],
    pixel_target_width: Option<u32>,
) -> Result<CompareReportDto, SessionError> {
    let doc_a = Document::from_bytes(bytes_a)?;
    let doc_b = Document::from_bytes(bytes_b)?;
    let text_report = compare_text(&doc_a, &doc_b)?;

    let pixel_pages = match pixel_target_width {
        Some(target_width) => compare_pixels_bytes(engine, bytes_a, bytes_b, target_width)?,
        None => Vec::new(),
    };

    Ok(CompareReportDto {
        page_count_a: text_report.page_count_a,
        page_count_b: text_report.page_count_b,
        text_pages: text_report.pages.into_iter().map(Into::into).collect(),
        pixel_pages: pixel_pages.into_iter().map(Into::into).collect(),
    })
}

/// Opens both documents against `engine` just long enough to render and
/// diff every page, then closes them — see this module's doc for why
/// this never touches [`crate::SessionState::docs`].
fn compare_pixels_bytes<E: Engine>(
    engine: &E,
    bytes_a: &[u8],
    bytes_b: &[u8],
    target_width: u32,
) -> Result<Vec<PixelPageDiff>, SessionError> {
    let handle_a = engine.open_bytes(bytes_a.to_vec())?;
    let handle_b = engine.open_bytes(bytes_b.to_vec())?;

    let result = compare_pixels(engine, handle_a, handle_b, target_width);

    engine.close(handle_a);
    engine.close(handle_b);

    Ok(result?.pages)
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    pub path_a: String,
    pub path_b: String,
    /// Render width in pixels for the pixel-diff pass; omit to skip
    /// pixel mode entirely (text-only compare needs no PDFium round trip).
    pub pixel_target_width: Option<u32>,
}

/// The actual logic behind the desktop's `compare_documents_cmd`.
/// Path-based — desktop-only, see this module's doc.
#[cfg(not(target_arch = "wasm32"))]
pub fn compare_documents_impl<E: Engine>(
    engine: &E,
    request: CompareRequest,
) -> Result<CompareReportDto, SessionError> {
    let bytes_a = std::fs::read(&request.path_a)?;
    let bytes_b = std::fs::read(&request.path_b)?;
    compare_bytes(engine, &bytes_a, &bytes_b, request.pixel_target_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::shared_handle;

    fn text_page_pdf_bytes(text: &str) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 24.0.into()]),
                Operation::new("Td", vec![20.0.into(), 150.0.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    /// The wasm-clean core, exercised directly with in-memory buffers —
    /// no filesystem involved, proving [`compare_bytes`] works standalone
    /// (not just as a helper [`compare_documents_impl`] happens to call).
    #[test]
    fn compare_bytes_reports_both_text_and_pixel_differences() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let bytes_a = text_page_pdf_bytes("Hello World");
        let bytes_b = text_page_pdf_bytes("Goodbye World");

        let report = compare_bytes(engine, &bytes_a, &bytes_b, Some(300))
            .expect("compare_bytes should succeed");

        assert_eq!(report.page_count_a, 1);
        assert_eq!(report.page_count_b, 1);
        assert_eq!(report.text_pages.len(), 1);
        assert_eq!(report.text_pages[0].removed, vec!["Hello World"]);
        assert_eq!(report.text_pages[0].added, vec!["Goodbye World"]);

        assert_eq!(report.pixel_pages.len(), 1);
        assert!(report.pixel_pages[0].differing_pixels > 0);
        assert!(report.pixel_pages[0].bbox.is_some());
    }

    #[test]
    fn compare_bytes_skips_pixel_mode_when_not_requested() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let bytes = text_page_pdf_bytes("Same text");
        let report =
            compare_bytes(engine, &bytes, &bytes, None).expect("compare_bytes should succeed");

        assert!(report.text_pages.is_empty());
        assert!(report.pixel_pages.is_empty());
    }

    /// End-to-end through the path-based desktop wrapper: real files on
    /// disk, real `EngineHandle` (PDFium), real `compare_documents_impl`.
    #[test]
    fn compare_documents_impl_reads_two_real_files_and_reports_differences() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_dir = std::env::temp_dir();
        let path_a = tmp_dir.join(format!(
            "openpdfedit-session-compare-a-{}.pdf",
            std::process::id()
        ));
        let path_b = tmp_dir.join(format!(
            "openpdfedit-session-compare-b-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&path_a, text_page_pdf_bytes("Hello World")).unwrap();
        std::fs::write(&path_b, text_page_pdf_bytes("Goodbye World")).unwrap();

        let report = compare_documents_impl(
            engine,
            CompareRequest {
                path_a: path_a.to_string_lossy().into_owned(),
                path_b: path_b.to_string_lossy().into_owned(),
                pixel_target_width: Some(300),
            },
        )
        .expect("compare should succeed");

        assert_eq!(report.page_count_a, 1);
        assert_eq!(report.page_count_b, 1);
        assert_eq!(report.text_pages[0].removed, vec!["Hello World"]);
        assert_eq!(report.text_pages[0].added, vec!["Goodbye World"]);
        assert!(report.pixel_pages[0].differing_pixels > 0);

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }
}
