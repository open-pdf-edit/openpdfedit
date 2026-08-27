//! Redaction command, moved here (from
//! `apps/desktop/src-tauri/src/redact.rs`) for the same reason as
//! [`crate::annotations`]/[`crate::pages`]: the same logic should drive
//! both the desktop's thread-wrapped `EngineHandle` and (later) a bare
//! in-process engine for the wasm/Chrome-extension build. True content
//! removal (not a black box painted over live data) — see
//! `openpdfedit-redact`'s module doc for exactly what "removal" means
//! here and its known limitations. Goes through [`crate::commit_mutation`]
//! like annotations/page-ops: mutate the `openpdfedit-doc` object graph,
//! incrementally save, rotate the render handle.
//!
//! `Err = SessionError` throughout (like [`crate::annotations`]/
//! [`crate::forms`]/[`crate::pages`]/[`crate::textedit`]) — this is the
//! moved code's own home now. [`SessionError`] needed no new variant: the
//! existing `Doc` variant already covers `openpdfedit_redact::RedactError`,
//! exactly as the desktop's own `CommandError::Doc` did before the move.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_redact::Rect;
use serde::Deserialize;

use crate::{
    commit_mutation_saving, DocHistory, OpenDoc, OpenedDocumentInfo, SaveMode, SessionError,
    WorkingStore,
};

impl From<openpdfedit_redact::RedactError> for SessionError {
    fn from(e: openpdfedit_redact::RedactError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactPageRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    /// `[x0, y0, x1, y1]` in PDF page-space points.
    pub rect: [f32; 4],
    /// `[r, g, b]`, each `0.0..=1.0`. Defaults to white.
    pub color: Option<[f32; 3]>,
}

/// The actual logic behind the desktop's `redact_page_cmd`.
pub fn redact_page_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: RedactPageRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let rect = Rect {
        x0: request.rect[0] as f64,
        y0: request.rect[1] as f64,
        x1: request.rect[2] as f64,
        y1: request.rect[3] as f64,
    };
    // White, not the conventional black. A black bar is a mark on the
    // page saying "something was taken out here", which is right for a
    // disclosed document and wrong for the ordinary case — someone
    // tidying a scan before sending it on, who wants the removed thing
    // to be absent rather than conspicuous. Callers that want the bar
    // can still ask for it.
    let color = request.color.unwrap_or([1.0, 1.0, 1.0]);

    // A full rewrite rather than the incremental save every other edit
    // takes. Redaction is the one edit whose point is that something
    // becomes unrecoverable, and an appended revision keeps the bytes it
    // removed — see `Document::save_full`. The cost is any existing
    // signature, which redacting the bytes it covers was going to break
    // whichever way the file was written.
    commit_mutation_saving(
        engine,
        docs,
        history,
        store,
        request.handle,
        SaveMode::Full,
        |doc| {
            openpdfedit_redact::redact_page(doc, request.page_index, rect, color)?;
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{shared_handle, text_page_pdf_bytes};
    use crate::FsWorkingStore;
    use openpdfedit_doc::Document;

    /// End-to-end: real file on disk, real `EngineHandle` (PDFium), real
    /// `redact_page_impl`. Confirms the whole command-layer path (mutate
    /// -> incremental save -> handle rotation) works, on top of
    /// `openpdfedit-redact`'s own PDFium cross-validation test (which
    /// proves the removal itself is real) and `commit_mutation`'s
    /// existing coverage (which proves the handle-rotation plumbing this
    /// command reuses is sound).
    #[test]
    fn redact_page_impl_removes_text_saves_and_rotates_the_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-redact-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(
            &tmp_path,
            text_page_pdf_bytes("CONFIDENTIAL", 50.0, 50.0, 24.0),
        )
        .expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");
        let doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
                encryption: None,
            },
        );
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        let request = RedactPageRequest {
            handle,
            page_index: 0,
            rect: [40.0, 40.0, 300.0, 80.0],
            color: None,
        };
        let result = redact_page_impl(engine, &docs, &history, &FsWorkingStore, request)
            .expect("redact_page_impl should succeed");

        assert_ne!(
            result.handle, handle,
            "a successful redact+save must rotate to a fresh engine handle"
        );
        assert!(
            engine.page_count(handle).is_err(),
            "old handle must be closed"
        );

        let after_boxes = engine
            .page_char_boxes(result.handle, 0)
            .expect("char boxes should succeed on the redacted page");
        assert!(
            after_boxes.is_empty(),
            "the redacted text must have no extractable characters left, found {}",
            after_boxes.len()
        );

        let tile = engine
            .render_page(result.handle, 0, 100)
            .expect("new handle should still render");
        assert!(tile.height > 0);

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// The removed text must not still be sitting in the file.
    ///
    /// Every other edit here is saved incrementally — the change is
    /// appended and every earlier byte is kept — which is right for an
    /// edit and wrong for a redaction: the content stream the text was
    /// removed from stays in the file one revision back, where
    /// `strings` finds it. The page was genuinely redacted and the file
    /// still carried a copy of what had been taken off it.
    ///
    /// Checked in the saved bytes rather than through PDFium, because
    /// PDFium reads the current revision and reports success either
    /// way — which is exactly why this went unnoticed.
    #[test]
    fn the_redacted_text_is_gone_from_the_saved_bytes_not_just_the_page() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-redact-bytes-{}.pdf",
            std::process::id()
        ));
        let fixture = text_page_pdf_bytes("CONFIDENTIAL", 50.0, 50.0, 24.0);
        assert!(
            contains(&fixture, b"CONFIDENTIAL"),
            "the fixture has to carry the word in plain bytes for this to mean anything"
        );
        std::fs::write(&tmp_path, &fixture).expect("should write temp file");

        let handle = engine.open(&tmp_path).expect("engine should open");
        let doc = Document::open(&tmp_path).expect("doc crate should open");
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
                encryption: None,
            },
        );
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        let result = redact_page_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            RedactPageRequest {
                handle,
                page_index: 0,
                rect: [40.0, 40.0, 300.0, 80.0],
                color: None,
            },
        )
        .expect("redact_page_impl should succeed");

        let saved = std::fs::read(&tmp_path).expect("the working copy should be readable");
        assert!(
            !contains(&saved, b"CONFIDENTIAL"),
            "the redacted word is still in the file"
        );

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// The box a redaction leaves is white unless asked otherwise.
    ///
    /// Worth pinning rather than leaving to the default's own comment,
    /// because the colour is the only part of a redaction anyone
    /// actually sees, and `None` reaching here from the UI is the path
    /// every redaction takes.
    #[test]
    fn the_default_box_is_white() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-redact-white-{}.pdf",
            std::process::id()
        ));
        std::fs::write(
            &tmp_path,
            text_page_pdf_bytes("CONFIDENTIAL", 50.0, 50.0, 24.0),
        )
        .expect("should write temp file");

        let handle = engine.open(&tmp_path).expect("engine should open");
        let doc = Document::open(&tmp_path).expect("doc crate should open");
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
                encryption: None,
            },
        );
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        let result = redact_page_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            RedactPageRequest {
                handle,
                page_index: 0,
                rect: [40.0, 40.0, 300.0, 80.0],
                color: None,
            },
        )
        .expect("redact_page_impl should succeed");

        let size = engine.page_sizes(result.handle).expect("page size")[0];
        let width = 306u32;
        let scale = f64::from(width) / f64::from(size.width);
        let tile = engine
            .render_page(result.handle, 0, width)
            .expect("should render");

        // The middle of the redacted rect, in page points, mapped to
        // the rendered tile — PDF counts y up from the bottom, the tile
        // counts rows down from the top.
        let x = (170.0 * scale) as u32;
        let y = ((f64::from(size.height) - 60.0) * scale) as u32;
        let i = ((y * tile.width + x) * 4) as usize;
        let pixel = [tile.rgba[i], tile.rgba[i + 1], tile.rgba[i + 2]];
        assert_eq!(
            pixel,
            [255, 255, 255],
            "the default redaction box should be white, got {pixel:?}"
        );

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }
}
