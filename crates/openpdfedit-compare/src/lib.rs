//! Document compare (PLAN.md M9): two independent modes, built by
//! composing already-shipped crates rather than a new diff engine.
//!
//! - [`compare_text`]: per-page text-run diff via
//!   [`openpdfedit_textedit::list_text_runs`] (M7 infrastructure, reused
//!   as-is). No PDFium needed — pure content-stream reading, so it's fast
//!   and usable from a headless CLI context. This is a *line-of-runs*
//!   diff (each `Tj`/`TJ`/`'`/`"` call is one "line"), not a word-level or
//!   character-level diff, and it has the same text-extraction
//!   limitations as `list_text_runs` itself (no ligature/kerning
//!   reconstruction, one entry per show-text operator rather than per
//!   visually-wrapped line). Good for "did this paragraph change,"
//!   not for a tight-diff word-level red/green view.
//! - [`compare_pixels`]: renders both documents page-by-page via an
//!   [`openpdfedit_engine::Engine`] and reports how many pixels differ
//!   (plus a bounding box of the changed region). Catches anything text
//!   mode can't see (font substitution, images, vector art, layout
//!   shifts) at the cost of being sensitive to *any* rendering
//!   difference, including ones a human wouldn't call a "change" (e.g.
//!   antialiasing at a different render width). Pages that don't render
//!   at matching pixel dimensions are reported with no bounding box
//!   rather than guessed at.
//!
//! Both modes tolerate the two documents having different page counts
//! (extra pages show up as all-added/all-removed); [`compare_pixels`]
//! additionally requires matching per-page render dimensions to produce
//! a bounding box, since there's no meaningful pixel alignment across
//! differently-sized renders.

use openpdfedit_doc::{DocError, Document};
use openpdfedit_engine::{DocHandle, Engine, EngineError, RenderedTile};
use openpdfedit_textedit::TextEditError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompareError {
    #[error(transparent)]
    Doc(#[from] DocError),
    #[error(transparent)]
    TextEdit(#[from] TextEditError),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// One page's added/removed text runs. A page with no differences is
/// simply absent from [`TextCompareReport::pages`].
#[derive(Debug, Clone)]
pub struct TextPageDiff {
    pub page_index: u32,
    /// Runs present in the second document but not (at that sequence
    /// position) in the first.
    pub added: Vec<String>,
    /// Runs present in the first document but not (at that sequence
    /// position) in the second.
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TextCompareReport {
    pub page_count_a: u32,
    pub page_count_b: u32,
    pub pages: Vec<TextPageDiff>,
}

/// Diffs every page's text runs between `doc_a` and `doc_b`. Pages beyond
/// the shorter document's page count are treated as empty on that side
/// (so a trailing extra page reports entirely as `added` or `removed`).
pub fn compare_text(doc_a: &Document, doc_b: &Document) -> Result<TextCompareReport, CompareError> {
    let page_count_a = doc_a.page_count()?;
    let page_count_b = doc_b.page_count()?;
    let max_pages = page_count_a.max(page_count_b);

    let mut pages = Vec::new();
    for page_index in 0..max_pages {
        let lines_a = if page_index < page_count_a {
            page_lines(doc_a, page_index)?
        } else {
            Vec::new()
        };
        let lines_b = if page_index < page_count_b {
            page_lines(doc_b, page_index)?
        } else {
            Vec::new()
        };

        let (removed, added) = diff_lines(&lines_a, &lines_b);
        if !added.is_empty() || !removed.is_empty() {
            pages.push(TextPageDiff {
                page_index,
                added,
                removed,
            });
        }
    }

    Ok(TextCompareReport {
        page_count_a,
        page_count_b,
        pages,
    })
}

fn page_lines(doc: &Document, page_index: u32) -> Result<Vec<String>, CompareError> {
    let content = doc.page_content_bytes(page_index)?;
    let runs = openpdfedit_textedit::list_text_runs(page_index, &content)?;
    Ok(runs.into_iter().map(|run| run.text).collect())
}

/// Sequence diff via a classic longest-common-subsequence table —
/// appropriate here because a page's text runs are a short list (tens,
/// not thousands, of `Tj`/`TJ` calls), so the `O(n*m)` DP table is
/// negligible. Order-preserving and multiplicity-aware: two identical
/// runs at different positions are not silently collapsed into one.
fn diff_lines(a: &[String], b: &[String]) -> (Vec<String>, Vec<String>) {
    let n = a.len();
    let m = b.len();
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut removed = Vec::new();
    let mut added = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            removed.push(a[i].clone());
            i += 1;
        } else {
            added.push(b[j].clone());
            j += 1;
        }
    }
    removed.extend(a[i..].iter().cloned());
    added.extend(b[j..].iter().cloned());
    (removed, added)
}

/// A bounding box of differing pixels, in the pixel coordinates of the
/// rendered tiles compared (top-left origin, `right`/`bottom` exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelDiffRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(Debug, Clone)]
pub struct PixelPageDiff {
    pub page_index: u32,
    pub differing_pixels: u64,
    /// `0` if the page was skipped because the two renders didn't come
    /// out at matching dimensions (or the page doesn't exist on one
    /// side) — see this crate's module doc.
    pub total_pixels: u64,
    /// `None` when there were no differing pixels, or the page was
    /// skipped (see [`Self::total_pixels`]'s doc).
    pub bbox: Option<PixelDiffRect>,
}

#[derive(Debug, Clone, Default)]
pub struct PixelCompareReport {
    pub pages: Vec<PixelPageDiff>,
}

/// Renders every page of both open documents at `target_width` pixels
/// wide via `engine` and compares the RGBA buffers byte-for-byte. Takes
/// `&dyn Engine` (not `PdfiumEngine`) so this crate never needs to name a
/// concrete backend, matching the swappable-engine boundary the rest of
/// the workspace holds to.
pub fn compare_pixels(
    engine: &dyn Engine,
    handle_a: DocHandle,
    handle_b: DocHandle,
    target_width: u32,
) -> Result<PixelCompareReport, CompareError> {
    let page_count_a = engine.page_count(handle_a)?;
    let page_count_b = engine.page_count(handle_b)?;
    let max_pages = page_count_a.max(page_count_b);

    let mut pages = Vec::with_capacity(max_pages as usize);
    for page_index in 0..max_pages {
        if page_index >= page_count_a || page_index >= page_count_b {
            pages.push(PixelPageDiff {
                page_index,
                differing_pixels: 0,
                total_pixels: 0,
                bbox: None,
            });
            continue;
        }
        let tile_a = engine.render_page(handle_a, page_index, target_width)?;
        let tile_b = engine.render_page(handle_b, page_index, target_width)?;
        pages.push(diff_tile_pair(page_index, &tile_a, &tile_b));
    }

    Ok(PixelCompareReport { pages })
}

/// Compares two already-rendered tiles pixel-by-pixel. Exposed as a
/// standalone function (not folded into [`compare_pixels`]'s loop only)
/// so a caller that already has tiles from elsewhere — e.g. the desktop
/// app's `EngineHandle`, which returns `Arc<RenderedTile>` from a render
/// thread rather than implementing the [`Engine`] trait directly — can
/// diff them without going through this crate's own page-opening logic.
pub fn diff_tile_pair(page_index: u32, a: &RenderedTile, b: &RenderedTile) -> PixelPageDiff {
    if a.width != b.width || a.height != b.height {
        return PixelPageDiff {
            page_index,
            differing_pixels: 0,
            total_pixels: 0,
            bbox: None,
        };
    }

    let mut differing_pixels = 0u64;
    let (mut min_x, mut min_y) = (a.width, a.height);
    let (mut max_x, mut max_y) = (0u32, 0u32);

    for y in 0..a.height {
        for x in 0..a.width {
            let i = ((y * a.width + x) * 4) as usize;
            if a.rgba[i..i + 4] != b.rgba[i..i + 4] {
                differing_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    let bbox = (differing_pixels > 0).then_some(PixelDiffRect {
        left: min_x,
        top: min_y,
        right: max_x + 1,
        bottom: max_y + 1,
    });

    PixelPageDiff {
        page_index,
        differing_pixels,
        total_pixels: (a.width as u64) * (a.height as u64),
        bbox,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Object, Stream};

    fn diff_lines_pub(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>) {
        let a: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        let b: Vec<String> = b.iter().map(|s| s.to_string()).collect();
        diff_lines(&a, &b)
    }

    #[test]
    fn diff_lines_identical_sequences_yields_nothing() {
        let (removed, added) = diff_lines_pub(&["one", "two"], &["one", "two"]);
        assert!(removed.is_empty());
        assert!(added.is_empty());
    }

    #[test]
    fn diff_lines_detects_a_single_substitution() {
        let (removed, added) = diff_lines_pub(&["one", "two", "three"], &["one", "TWO", "three"]);
        assert_eq!(removed, vec!["two"]);
        assert_eq!(added, vec!["TWO"]);
    }

    #[test]
    fn diff_lines_detects_pure_insertion_and_deletion() {
        let (removed, added) = diff_lines_pub(&["a", "b"], &["a", "b", "c"]);
        assert!(removed.is_empty());
        assert_eq!(added, vec!["c"]);

        let (removed, added) = diff_lines_pub(&["a", "b", "c"], &["a", "c"]);
        assert_eq!(removed, vec!["b"]);
        assert!(added.is_empty());
    }

    fn one_line_pdf_bytes(text: &str) -> Vec<u8> {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
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

    #[test]
    fn compare_text_reports_no_pages_when_content_is_identical() {
        let bytes = one_line_pdf_bytes("Hello World");
        let doc_a = Document::from_bytes(&bytes).unwrap();
        let doc_b = Document::from_bytes(&bytes).unwrap();

        let report = compare_text(&doc_a, &doc_b).unwrap();
        assert!(report.pages.is_empty());
        assert_eq!(report.page_count_a, 1);
        assert_eq!(report.page_count_b, 1);
    }

    #[test]
    fn compare_text_reports_a_changed_run_on_its_page() {
        let doc_a = Document::from_bytes(&one_line_pdf_bytes("Hello World")).unwrap();
        let doc_b = Document::from_bytes(&one_line_pdf_bytes("Goodbye World")).unwrap();

        let report = compare_text(&doc_a, &doc_b).unwrap();
        assert_eq!(report.pages.len(), 1);
        let page_diff = &report.pages[0];
        assert_eq!(page_diff.page_index, 0);
        assert_eq!(page_diff.removed, vec!["Hello World"]);
        assert_eq!(page_diff.added, vec!["Goodbye World"]);
    }

    #[test]
    fn compare_text_handles_a_trailing_extra_page_as_pure_addition() {
        let bytes_one_page = one_line_pdf_bytes("Same text");
        let doc_a = Document::from_bytes(&bytes_one_page).unwrap();

        // A second document with the same first page plus a second page —
        // built directly since openpdfedit-pages::merge lives in a crate
        // this one doesn't depend on.
        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let font_id = raw.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let mut page_ids = Vec::new();
        for text in ["Same text", "Extra page"] {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                    Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                    Operation::new("Tj", vec![Object::string_literal(text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = raw.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            });
            page_ids.push(Object::Reference(page_id));
        }
        raw.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => page_ids, "Count" => 2,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut two_page_bytes = Vec::new();
        raw.save_to(&mut two_page_bytes).unwrap();
        let doc_b = Document::from_bytes(&two_page_bytes).unwrap();

        let report = compare_text(&doc_a, &doc_b).unwrap();
        assert_eq!(report.page_count_a, 1);
        assert_eq!(report.page_count_b, 2);
        assert_eq!(report.pages.len(), 1);
        assert_eq!(report.pages[0].page_index, 1);
        assert_eq!(report.pages[0].added, vec!["Extra page"]);
        assert!(report.pages[0].removed.is_empty());
    }

    #[test]
    fn diff_tiles_reports_no_bbox_for_identical_tiles() {
        let tile = RenderedTile {
            width: 4,
            height: 4,
            rgba: vec![0u8; 4 * 4 * 4],
        };
        let other = RenderedTile {
            width: 4,
            height: 4,
            rgba: tile.rgba.clone(),
        };
        let diff = diff_tile_pair(0, &tile, &other);
        assert_eq!(diff.differing_pixels, 0);
        assert!(diff.bbox.is_none());
        assert_eq!(diff.total_pixels, 16);
    }

    #[test]
    fn diff_tiles_finds_the_bounding_box_of_a_single_changed_pixel() {
        let a = vec![0u8; 4 * 4 * 4];
        let mut b = a.clone();
        // Pixel at (2, 1): index = (1*4 + 2) * 4 = 24.
        b[24] = 255;
        let tile_a = RenderedTile {
            width: 4,
            height: 4,
            rgba: a,
        };
        let tile_b = RenderedTile {
            width: 4,
            height: 4,
            rgba: b,
        };
        let diff = diff_tile_pair(0, &tile_a, &tile_b);
        assert_eq!(diff.differing_pixels, 1);
        assert_eq!(
            diff.bbox,
            Some(PixelDiffRect {
                left: 2,
                top: 1,
                right: 3,
                bottom: 2
            })
        );
    }

    #[test]
    fn diff_tiles_mismatched_dimensions_are_skipped_not_guessed_at() {
        let tile_a = RenderedTile {
            width: 4,
            height: 4,
            rgba: vec![0u8; 4 * 4 * 4],
        };
        let tile_b = RenderedTile {
            width: 5,
            height: 4,
            rgba: vec![0u8; 5 * 4 * 4],
        };
        let diff = diff_tile_pair(0, &tile_a, &tile_b);
        assert_eq!(diff.total_pixels, 0);
        assert!(diff.bbox.is_none());
    }
}
