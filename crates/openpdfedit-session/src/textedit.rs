//! MVP text-run/image-placement editing commands, moved here (from
//! `apps/desktop/src-tauri/src/textedit.rs`) for the same reason as
//! [`crate::annotations`]/[`crate::pages`]: the same logic should drive
//! both the desktop's thread-wrapped `EngineHandle` and (later) a bare
//! in-process engine for the wasm/Chrome-extension build. See
//! `openpdfedit-textedit`'s module doc for exactly what this does and
//! does not provide (single-run substitution at the original position
//! and font with an approximate width-matching scale, not glyph-accurate
//! re-layout).
//!
//! Runs/placements are identified by index into a freshly re-listed
//! array (not by an opaque id or by re-sending float coordinates that
//! could drift through JSON round-tripping) — the front-end lists, shows
//! the user what's there, and sends back the index of whichever one they
//! picked.
//!
//! Listing ([`list_text_runs_impl`]/[`list_image_placements_impl`]) reads
//! only the already-open [`crate::OpenDoc::doc`]'s object graph — no
//! engine, no filesystem — so both are fully portable as-is, the same way
//! [`crate::annotations::list_page_annotations_impl`]/
//! [`crate::forms::list_form_fields_impl`]/
//! [`crate::signatures::list_signatures_impl`] are (this was once the only
//! portable listing half in this crate; it no longer is, so this doesn't
//! claim uniqueness — see those modules' own docs). The three mutating commands
//! ([`edit_text_run_impl`]/[`move_text_run_impl`]/[`move_image_impl`])
//! go through [`crate::commit_mutation`] like every other single-document
//! edit, and genericize over `E: Engine` the normal way. `Err =
//! SessionError` throughout (like [`crate::annotations`]/[`crate::forms`]/
//! [`crate::pages`]) — this is the moved code's own home now, not a
//! shared helper other error types also need to drive. [`SessionError`]
//! needed no new variant: the existing `Doc` variant already covers
//! `openpdfedit_textedit::TextEditError`, exactly as the desktop's own
//! `CommandError::Doc` did before the move.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use serde::{Deserialize, Serialize};

use crate::{
    commit_mutation, resolve_doc, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError,
    WorkingStore,
};

impl From<openpdfedit_textedit::TextEditError> for SessionError {
    fn from(e: openpdfedit_textedit::TextEditError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRunDto {
    index: usize,
    text: String,
    /// `[x0, y0, x1, y1]` in PDF page-space points.
    rect: [f32; 4],
    font_size: f32,
    /// `false` for runs using an embedded subset font whose bytes are
    /// glyph ids rather than characters — `text` is unreliable for those
    /// and [`edit_text_run_impl`] will refuse them. See
    /// `openpdfedit_textedit::looks_cid_encoded`'s doc.
    is_editable: bool,
}

/// The actual logic behind the desktop's `list_text_runs_cmd`. Reads only
/// the already-open document's object graph — no engine, no filesystem
/// involved, so this is portable as-is.
pub fn list_text_runs_impl(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    handle: DocHandle,
    page_index: u32,
) -> Result<Vec<TextRunDto>, SessionError> {
    let docs_guard = docs.lock().expect("docs lock poisoned");
    let open_doc = resolve_doc(&docs_guard, handle)?;
    let runs = openpdfedit_textedit::list_text_runs_in_page(&open_doc.doc, page_index)?;
    Ok(runs
        .into_iter()
        .enumerate()
        .map(|(index, r)| TextRunDto {
            index,
            text: r.text,
            rect: [
                r.bbox.x0 as f32,
                r.bbox.y0 as f32,
                r.bbox.x1 as f32,
                r.bbox.y1 as f32,
            ],
            font_size: r.font_size as f32,
            is_editable: r.is_editable,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTextRunRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    pub run_index: usize,
    pub new_text: String,
}

/// The actual logic behind the desktop's `edit_text_run_cmd`.
pub fn edit_text_run_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: EditTextRunRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        let runs = openpdfedit_textedit::list_text_runs_in_page(doc, request.page_index)?;
        let run = runs.get(request.run_index).ok_or_else(|| {
            SessionError::Doc(format!(
                "run index {} out of range ({} runs on this page)",
                request.run_index,
                runs.len()
            ))
        })?;
        openpdfedit_textedit::edit_text_run(doc, run, &request.new_text)?;
        Ok(())
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveTextRunRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    pub run_index: usize,
    pub dx: f32,
    pub dy: f32,
}

/// The actual logic behind the desktop's `move_text_run_cmd`. Unlike
/// [`edit_text_run_impl`] this imposes no `isEditable` requirement — see
/// `openpdfedit_textedit::move_text_run` for why moving text never needs
/// to decode it.
pub fn move_text_run_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: MoveTextRunRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        let runs = openpdfedit_textedit::list_text_runs_in_page(doc, request.page_index)?;
        let run = runs.get(request.run_index).ok_or_else(|| {
            SessionError::Doc(format!(
                "run index {} out of range ({} runs on this page)",
                request.run_index,
                runs.len()
            ))
        })?;
        openpdfedit_textedit::move_text_run(doc, run, request.dx as f64, request.dy as f64)?;
        Ok(())
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePlacementDto {
    index: usize,
    rect: [f32; 4],
}

/// The actual logic behind the desktop's `list_image_placements_cmd`.
/// Portable — see [`list_text_runs_impl`]'s doc.
pub fn list_image_placements_impl(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    handle: DocHandle,
    page_index: u32,
) -> Result<Vec<ImagePlacementDto>, SessionError> {
    let docs_guard = docs.lock().expect("docs lock poisoned");
    let open_doc = resolve_doc(&docs_guard, handle)?;
    let content = open_doc.doc.page_content_bytes(page_index)?;
    let placements = openpdfedit_textedit::list_image_placements(page_index, &content)?;
    Ok(placements
        .into_iter()
        .enumerate()
        .map(|(index, p)| ImagePlacementDto {
            index,
            rect: [
                p.bbox.x0 as f32,
                p.bbox.y0 as f32,
                p.bbox.x1 as f32,
                p.bbox.y1 as f32,
            ],
        })
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveImageRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    pub placement_index: usize,
    pub dx: f32,
    pub dy: f32,
}

/// The actual logic behind the desktop's `move_image_cmd`.
pub fn move_image_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: MoveImageRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        let content = doc.page_content_bytes(request.page_index)?;
        let placements = openpdfedit_textedit::list_image_placements(request.page_index, &content)?;
        let placement = placements.get(request.placement_index).ok_or_else(|| {
            SessionError::Doc(format!(
                "placement index {} out of range ({} images on this page)",
                request.placement_index,
                placements.len()
            ))
        })?;
        openpdfedit_textedit::move_image(doc, placement, request.dx as f64, request.dy as f64)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::shared_handle;
    use crate::FsWorkingStore;
    use openpdfedit_doc::Document;

    fn text_page_pdf_bytes(text: &str) -> Vec<u8> {
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

    #[test]
    fn edit_text_run_impl_by_index_edits_the_right_run_saves_and_rotates_the_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-textedit-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, text_page_pdf_bytes("original text"))
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

        let before = list_text_runs_impl(&docs, handle, 0).expect("list should succeed");
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].text, "original text");

        let request = EditTextRunRequest {
            handle,
            page_index: 0,
            run_index: 0,
            new_text: "edited text".to_string(),
        };
        let result = edit_text_run_impl(engine, &docs, &history, &FsWorkingStore, request)
            .expect("edit_text_run_impl should succeed");

        assert_ne!(
            result.handle, handle,
            "a successful edit+save must rotate the handle"
        );
        assert!(
            engine.page_count(handle).is_err(),
            "old handle must be closed"
        );

        let after_boxes = engine
            .page_char_boxes(result.handle, 0)
            .expect("char boxes should succeed");
        assert_eq!(after_boxes.len(), "edited text".chars().count());

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn edit_text_run_impl_rejects_an_out_of_range_run_index() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-textedit-oob-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, text_page_pdf_bytes("only run")).expect("should write temp file");

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

        let request = EditTextRunRequest {
            handle,
            page_index: 0,
            run_index: 5,
            new_text: "n/a".to_string(),
        };
        let result = edit_text_run_impl(engine, &docs, &history, &FsWorkingStore, request);
        let Err(err) = result else {
            panic!("an out-of-range run index must be rejected");
        };
        assert!(format!("{err}").contains("out of range"));

        engine.close(handle);
        let _ = std::fs::remove_file(&tmp_path);
    }
}
