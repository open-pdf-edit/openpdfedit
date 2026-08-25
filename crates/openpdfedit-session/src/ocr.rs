//! The half of OCR that runs anywhere.
//!
//! `openpdfedit-ocr` does three things: render a page to pixels,
//! recognise words in it, and write those words back as an invisible
//! text layer. Only the middle step is platform-bound — it shells out to
//! the `tesseract` binary, which a browser cannot do — so it is cfg'd
//! out of `wasm32` and the desktop keeps calling
//! `openpdfedit_ocr::ocr_document` directly.
//!
//! This module is the seam that lets the browser reach the third step
//! with words it recognised itself (tesseract.js, in a worker). The
//! recogniser differs; the PDF that comes out does not, because both
//! paths end in the same `add_text_layer`.
//!
//! Coordinates arrive in **pixel space**, relative to the bitmap that
//! was recognised, together with that bitmap's size and the page's size
//! in points. `add_text_layer` scales between them. Handing over
//! already-scaled coordinates would push that arithmetic into JavaScript
//! and give the two paths two chances to disagree about it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_ocr::OcrWord;
use serde::{Deserialize, Serialize};

use crate::{commit_mutation, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore};

/// One recognised word, as the caller's recogniser reported it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OcrWordDto {
    pub text: String,
    /// Pixel-space box, top-left origin — the same shape Tesseract's own
    /// TSV reports and `OcrWord` stores, so nothing has to be converted
    /// on the way in.
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    /// 0–100. Carried through rather than filtered here: what counts as
    /// too uncertain to write is a product decision, and the caller is
    /// the one that knows which recogniser produced the number.
    pub confidence: f32,
}

/// One page's recognition result.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OcrPageWords {
    pub page_index: u32,
    pub page_width_pt: f32,
    pub page_height_pt: f32,
    pub image_width_px: u32,
    pub image_height_px: u32,
    pub words: Vec<OcrWordDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddOcrTextLayerRequest {
    pub handle: DocHandle,
    pub pages: Vec<OcrPageWords>,
}

/// Writes an invisible text layer onto every page in `pages`, making a
/// scan searchable without changing a visible pixel.
///
/// All pages land in **one** mutation, so one Undo takes the whole
/// document's OCR back. Per-page commits would leave someone who
/// OCR'd forty pages pressing Undo forty times, and — worse — able to
/// stop halfway and keep a document that is half searchable with no
/// indication which half.
pub fn add_ocr_text_layer_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: AddOcrTextLayerRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        for page in &request.pages {
            let words: Vec<OcrWord> = page
                .words
                .iter()
                .map(|w| OcrWord {
                    text: w.text.clone(),
                    left: w.left,
                    top: w.top,
                    width: w.width,
                    height: w.height,
                    confidence: w.confidence,
                })
                .collect();
            openpdfedit_ocr::add_text_layer(
                doc,
                page.page_index,
                page.page_width_pt,
                page.page_height_pt,
                page.image_width_px,
                page.image_height_px,
                &words,
            )
            .map_err(|e| SessionError::Doc(e.to_string()))?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::shared_handle;
    use crate::{FsWorkingStore, SessionState};

    /// The browser's path, end to end: hand in words the way tesseract.js
    /// reports them, and the page must come out searchable.
    ///
    /// Searchable is checked by asking the engine to find the text, not
    /// by inspecting the content stream — a layer that is present but
    /// positioned or encoded wrongly would pass the second and fail the
    /// only thing a user cares about.
    #[test]
    fn words_recognised_in_a_browser_make_the_page_searchable() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let path = std::env::temp_dir().join(format!("openpdfedit-ocr-{}.pdf", std::process::id()));
        std::fs::write(&path, crate::test_support::minimal_pdf_bytes()).expect("write fixture");

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(FsWorkingStore),
        };
        let opened = crate::open_document_impl(&state, &path).expect("open");

        // A 612x792pt page rendered at 150 DPI is 1275x1650px; these are
        // the numbers tesseract.js would report for a word near the top.
        let info = add_ocr_text_layer_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            AddOcrTextLayerRequest {
                handle: opened.handle,
                pages: vec![OcrPageWords {
                    page_index: 0,
                    page_width_pt: 612.0,
                    page_height_pt: 792.0,
                    image_width_px: 1275,
                    image_height_px: 1650,
                    words: vec![OcrWordDto {
                        text: "SCANNED".to_string(),
                        left: 150.0,
                        top: 200.0,
                        width: 400.0,
                        height: 50.0,
                        confidence: 92.0,
                    }],
                }],
            },
        )
        .expect("should add the layer");

        let hits = state
            .engine
            .search_document(
                info.handle,
                "SCANNED",
                openpdfedit_engine::SearchOptions::default(),
                10,
            )
            .expect("should search");
        assert!(
            !hits.is_empty(),
            "a word written into the text layer must be findable in the page",
        );

        let _ = std::fs::remove_file(&path);
    }
}
