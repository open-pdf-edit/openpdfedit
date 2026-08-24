//! Watermark command: tiled text/logo page stamps (see
//! `openpdfedit-watermark`'s module doc for the pattern and its
//! OpenCapture lineage). Fully portable — the drawing crate mutates the
//! `openpdfedit-doc` object graph and this module goes through
//! [`crate::commit_mutation`] like annotations/pages/redact: mutate,
//! incrementally save through the [`WorkingStore`], rotate the render
//! handle.
//!
//! The logo crosses the IPC/wasm boundary as base64-encoded raw RGBA
//! (plus explicit width/height) rather than as a PNG/JPEG file: the UI
//! already has a canvas that decodes any image format for its live
//! preview — same division of labor as the OpenCapture original, and it
//! keeps image-format parsing out of the Rust side entirely.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::Engine as _;
use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_watermark::{LogoRgba, WatermarkError, WatermarkLocation, WatermarkOptions};
use serde::Deserialize;

use crate::{commit_mutation, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore};

impl From<WatermarkError> for SessionError {
    fn from(e: WatermarkError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

/// Density for payloads written before the field existed — the pattern
/// every caller got until then.
fn default_density() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyWatermarkRequest {
    pub handle: DocHandle,
    /// May be empty when a logo is supplied.
    pub text: String,
    /// `"top" | "bottom" | "top-bottom" | "full"`.
    pub location: String,
    /// `0` or `45`.
    pub orientation_deg: u16,
    /// `0.0..=1.0`.
    pub opacity: f32,
    /// Multiplier on the automatic font size.
    pub text_scale: f32,
    /// How many tiles fit across a page, relative to the original
    /// pattern: `1.0` is that pattern, lower is sparser. Absent in
    /// payloads written before this existed, which is why it defaults
    /// rather than being required.
    #[serde(default = "default_density")]
    pub density: f32,
    /// Raw RGBA scanlines (top row first), base64-encoded; all three
    /// logo fields travel together or not at all.
    pub logo_rgba_base64: Option<String>,
    pub logo_width: Option<u32>,
    pub logo_height: Option<u32>,
    /// 0-based page indexes; `None` = every page.
    pub pages: Option<Vec<u32>>,
}

/// The actual logic behind the desktop's `apply_watermark_cmd` and the
/// wasm build's `applyWatermark`.
pub fn apply_watermark_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: ApplyWatermarkRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let location: WatermarkLocation = request
        .location
        .parse()
        .map_err(|e: WatermarkError| SessionError::Doc(e.to_string()))?;

    let logo = match (
        &request.logo_rgba_base64,
        request.logo_width,
        request.logo_height,
    ) {
        (Some(b64), Some(width), Some(height)) => Some(LogoRgba {
            rgba: base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| SessionError::Doc(format!("logo is not valid base64: {e}")))?,
            width,
            height,
        }),
        (None, None, None) => None,
        _ => {
            return Err(SessionError::Doc(
                "logoRgbaBase64, logoWidth and logoHeight must be supplied together".into(),
            ))
        }
    };

    let options = WatermarkOptions {
        text: request.text,
        location,
        orientation_deg: request.orientation_deg,
        opacity: request.opacity,
        text_scale: request.text_scale,
        density: request.density,
        logo,
        pages: request.pages,
    };

    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        openpdfedit_watermark::apply_watermark(doc, &options)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{minimal_pdf_bytes, shared_handle};
    use crate::FsWorkingStore;
    use openpdfedit_doc::Document;

    /// End-to-end against real PDFium: a blank page has no extractable
    /// characters; after `apply_watermark_impl` the stamped text must
    /// have real char boxes on the rendered page, the handle must rotate
    /// (mutate → incremental save → reopen), and the page must still
    /// render. Mirrors `redact_page_impl`'s test shape.
    #[test]
    fn apply_watermark_impl_stamps_text_saves_and_rotates_the_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-watermark-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");
        let before_boxes = engine
            .page_char_boxes(handle, 0)
            .expect("char boxes should succeed on the blank page");
        assert!(
            before_boxes.is_empty(),
            "fixture must start with no text for the assertion below to mean anything"
        );

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

        let request = ApplyWatermarkRequest {
            handle,
            text: "DRAFT".into(),
            location: "full".into(),
            orientation_deg: 45,
            opacity: 0.4,
            text_scale: 1.0,
            density: 1.0,
            logo_rgba_base64: None,
            logo_width: None,
            logo_height: None,
            pages: None,
        };
        let result = apply_watermark_impl(engine, &docs, &history, &FsWorkingStore, request)
            .expect("apply_watermark_impl should succeed");

        assert_ne!(
            result.handle, handle,
            "a successful watermark+save must rotate to a fresh engine handle"
        );
        assert!(
            engine.page_count(handle).is_err(),
            "old handle must be closed"
        );

        let after_boxes = engine
            .page_char_boxes(result.handle, 0)
            .expect("char boxes should succeed on the stamped page");
        assert!(
            !after_boxes.is_empty(),
            "the stamped watermark text must be visible to PDFium as real characters"
        );

        let tile = engine
            .render_page(result.handle, 0, 100)
            .expect("new handle should still render");
        assert!(tile.height > 0);

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn apply_watermark_impl_rejects_a_partial_logo_triple() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let request = ApplyWatermarkRequest {
            handle: 9999,
            text: "DRAFT".into(),
            location: "full".into(),
            orientation_deg: 0,
            opacity: 0.5,
            text_scale: 1.0,
            density: 1.0,
            logo_rgba_base64: Some("AAAA".into()),
            logo_width: Some(1),
            logo_height: None,
            pages: None,
        };
        let err = match apply_watermark_impl(engine, &docs, &history, &FsWorkingStore, request) {
            Err(e) => e,
            Ok(_) => panic!("a partial logo triple must be rejected"),
        };
        assert!(err.to_string().contains("together"), "got: {err}");
    }
}
