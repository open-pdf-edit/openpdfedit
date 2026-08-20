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

use crate::{commit_mutation, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore};

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
    /// `[r, g, b]`, each `0.0..=1.0`. Defaults to solid black.
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
    let color = request.color.unwrap_or([0.0, 0.0, 0.0]);

    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        openpdfedit_redact::redact_page(doc, request.page_index, rect, color)?;
        Ok(())
    })
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
}
