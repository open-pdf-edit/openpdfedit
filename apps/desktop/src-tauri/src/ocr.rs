//! OCR command: makes a scanned document searchable in place.
//!
//! Blocking, not progress-reporting — `openpdfedit_ocr::ocr_document`
//! runs synchronously through every page (render, shell out to
//! `tesseract`, append the invisible text layer) before this command
//! returns. Fine for the document sizes this milestone targets; a
//! genuinely long-running OCR job would want a Tauri event stream for
//! per-page progress instead of one blocking round trip — tracked as a
//! follow-up once real usage shows that's needed, not built speculatively
//! now.
//!
//! Saves through the same `openpdfedit-doc` incremental-save pipeline
//! annotations/page-ops use (unlike forms fill, which saves through
//! PDFium's own writer — see that module's doc for why): `ocr_document`
//! only ever calls `Document::add_object`/`append_content_stream`, both
//! plain `openpdfedit-doc` mutations, so there's no PDFium-native-save
//! self-overwrite hazard here to route around.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, EngineHandle};
use serde::Deserialize;
use tauri::State;

use crate::{
    capture_pre_edit_snapshot, commit_undo_snapshot, reopen_after_write, AppState, CommandError,
    DocHistory, FsWorkingStore, OpenDoc, OpenedDocument,
};

const DEFAULT_DPI: u32 = 200;
const DEFAULT_LANG: &str = "eng";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrDocumentRequest {
    handle: DocHandle,
    /// Scan resolution in pixels per inch. Higher improves recognition
    /// accuracy at the cost of OCR time; 200 is a reasonable default for
    /// typical printed-document text. Defaults applied server-side (not
    /// baked into the front-end) so a future settings UI has one place to
    /// change them.
    dpi: Option<u32>,
    /// A `tesseract` language code (`tesseract --list-langs`). Defaults
    /// to `"eng"` — the only language data this dev environment's
    /// `tesseract` install actually has (see `openpdfedit-ocr`'s module
    /// doc); a packaged app would need to ship/install the language data
    /// it wants to default to.
    lang: Option<String>,
}

#[tauri::command]
pub fn ocr_document_cmd(
    state: State<'_, AppState>,
    request: OcrDocumentRequest,
) -> Result<OpenedDocument, CommandError> {
    ocr_document_impl(&state.engine, &state.docs, &state.history, request)
}

fn ocr_document_impl(
    engine: &EngineHandle,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    request: OcrDocumentRequest,
) -> Result<OpenedDocument, CommandError> {
    let dpi = request.dpi.unwrap_or(DEFAULT_DPI);
    let lang = request.lang.as_deref().unwrap_or(DEFAULT_LANG);

    let path = {
        let mut docs_guard = docs.lock().expect("docs lock poisoned");
        let open_doc = docs_guard
            .get_mut(&request.handle)
            .ok_or(CommandError::UnknownHandle(request.handle))?;

        let pre_edit_snapshot = capture_pre_edit_snapshot(&open_doc.path)?;
        openpdfedit_ocr::ocr_document(engine, request.handle, &mut open_doc.doc, dpi, lang)?;
        let saved = open_doc.doc.save_incremental()?;
        std::fs::write(&open_doc.path, &saved)?;
        commit_undo_snapshot(history, &open_doc.path, pre_edit_snapshot);
        open_doc.path.clone()
    };

    reopen_after_write(engine, docs, history, &FsWorkingStore, request.handle, path)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpdfedit_doc::Document;

    fn text_page_pdf_bytes(text: &str, font_size: f64) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), font_size.into()]),
                Operation::new("Td", vec![72.into(), 700.into()]),
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
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn tesseract_available() -> bool {
        std::process::Command::new("tesseract")
            .arg("--version")
            .output()
            .is_ok()
    }

    /// End-to-end: real file on disk, real `EngineHandle` (PDFium), real
    /// `tesseract`, real `ocr_document_impl` — the same function the
    /// Tauri command calls. Confirms the whole chain (render -> OCR ->
    /// append text layer -> incremental save -> handle rotation) works
    /// through this crate's actual command layer, not just
    /// `openpdfedit-ocr` in isolation (already covered by that crate's
    /// own PDFium cross-validation test).
    #[test]
    fn ocr_document_impl_saves_and_rotates_the_handle() {
        let Some(engine) = crate::test_support::shared_engine() else {
            return;
        };
        if !tesseract_available() {
            eprintln!("skipping: tesseract not installed (brew install tesseract)");
            return;
        }

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-ocr-cmd-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, text_page_pdf_bytes("SCANNED PAGE", 40.0))
            .expect("should write temp file");
        let before_len = std::fs::metadata(&tmp_path).unwrap().len();

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
        let request = OcrDocumentRequest {
            handle,
            dpi: Some(200),
            lang: Some("eng".to_string()),
        };
        let result = ocr_document_impl(engine, &docs, &history, request)
            .expect("ocr_document_impl should succeed");

        assert_ne!(
            result.handle, handle,
            "a successful OCR+save must rotate to a fresh engine handle"
        );
        assert!(
            engine.page_count(handle).is_err(),
            "old handle must be closed"
        );

        let after_len = std::fs::metadata(&tmp_path).unwrap().len();
        assert!(
            after_len > before_len,
            "incremental save must have appended the text-layer bytes to disk"
        );

        let after_boxes = engine
            .page_char_boxes(result.handle, 0)
            .expect("char boxes should succeed on the OCR'd page");
        assert!(
            !after_boxes.is_empty(),
            "the OCR'd page should have extractable characters"
        );

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn ocr_document_impl_unknown_handle_returns_error_not_panic() {
        let Some(engine) = crate::test_support::shared_engine() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let request = OcrDocumentRequest {
            handle: 999_999,
            dpi: None,
            lang: None,
        };
        assert!(ocr_document_impl(engine, &docs, &history, request).is_err());
    }
}
