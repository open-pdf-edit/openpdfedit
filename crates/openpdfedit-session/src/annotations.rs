//! Annotation commands: the write side of the app, moved here (from
//! `apps/desktop/src-tauri/src/annotations.rs`) so the same logic can
//! drive both the desktop's thread-wrapped `EngineHandle` and (later) a
//! bare in-process engine for the wasm/Chrome-extension build.
//!
//! `add_annotation_impl` is the one function every markup tool
//! (highlight, underline, strikeout, note, ink) in the front-end drives
//! through, distinguished by the `kind` field of [`AnnotationInput`] —
//! one endpoint instead of five keeps the front-end/back-end contract in
//! one place.
//!
//! After a successful edit the on-disk file changes (via
//! `Document::save_incremental`), so the render-side engine handle for
//! this document is closed and reopened against the new bytes — see the
//! `openpdfedit-session` crate's module doc for why that means the
//! returned `OpenedDocumentInfo::handle` may differ from the one the
//! caller passed in, and why every mutating command returns the fresh
//! one.
//!
//! Every `_impl` function here takes `engine`/`docs`/`history` as
//! separate parameters (not a bundled `&SessionState<E>`) — matching
//! [`crate::commit_mutation`]/[`crate::undo_impl`]/[`crate::redo_impl`]
//! and the original desktop functions this module moved from, whose doc
//! comments explain why: Tauri's `State<T>` can't be constructed outside
//! a running app, so keeping the real work parameterized this way (with
//! the `#[tauri::command]` wrapper left in the desktop shell as a thin
//! `state.x` pass-through) is what makes an end-to-end test of this path
//! possible without spinning up a Tauri app instance.
//!
//! `Err = SessionError` is fixed throughout this module (unlike
//! [`crate::commit_mutation`] itself, which stays generic) — this is the
//! moved code's own home now, not a shared helper other error types also
//! need to drive.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_annot::{
    add_annotation, delete_annotation, list_annotations, AnnotationKind, Color, NewAnnotation, Rect,
};
use openpdfedit_engine::{DocHandle, Engine};
use serde::{Deserialize, Serialize};

use crate::{
    commit_mutation, resolve_doc, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError,
    WorkingStore,
};

/// Mirrors `openpdfedit_annot::AnnotationKind`, but as a serde-friendly
/// DTO the front-end can send as plain JSON (`AnnotationKind` itself
/// carries `Rect`, which has no serde impls — deliberately: this crate
/// is the IPC-payload boundary, `openpdfedit-annot` shouldn't need to
/// know Tauri/wasm-bindgen exist).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnnotationInput {
    Highlight {
        quads: Vec<[f32; 4]>,
    },
    Underline {
        quads: Vec<[f32; 4]>,
    },
    StrikeOut {
        quads: Vec<[f32; 4]>,
    },
    #[serde(rename_all = "camelCase")]
    FreeText {
        text: String,
        font_size: f32,
    },
    Ink {
        strokes: Vec<Vec<[f32; 2]>>,
    },
}

fn rect_from_flat(r: [f32; 4]) -> Rect {
    Rect {
        x0: r[0],
        y0: r[1],
        x1: r[2],
        y1: r[3],
    }
}

fn to_annotation_kind(input: AnnotationInput) -> AnnotationKind {
    match input {
        AnnotationInput::Highlight { quads } => AnnotationKind::Highlight {
            quads: quads.into_iter().map(rect_from_flat).collect(),
        },
        AnnotationInput::Underline { quads } => AnnotationKind::Underline {
            quads: quads.into_iter().map(rect_from_flat).collect(),
        },
        AnnotationInput::StrikeOut { quads } => AnnotationKind::StrikeOut {
            quads: quads.into_iter().map(rect_from_flat).collect(),
        },
        AnnotationInput::FreeText { text, font_size } => {
            AnnotationKind::FreeText { text, font_size }
        }
        AnnotationInput::Ink { strokes } => AnnotationKind::Ink {
            strokes: strokes
                .into_iter()
                .map(|s| s.into_iter().map(|p| (p[0], p[1])).collect())
                .collect(),
        },
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAnnotationRequest {
    pub handle: u64,
    pub page_index: u32,
    pub rect: [f32; 4],
    pub color: [f32; 3],
    pub opacity: f32,
    pub contents: Option<String>,
    pub annotation: AnnotationInput,
}

/// The actual logic behind the desktop's `add_annotation_cmd`.
pub fn add_annotation_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: AddAnnotationRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let AddAnnotationRequest {
        handle,
        page_index,
        rect,
        color,
        opacity,
        contents,
        annotation,
    } = request;

    commit_mutation(engine, docs, history, store, handle, |doc| {
        add_annotation(
            doc,
            page_index,
            NewAnnotation {
                rect: rect_from_flat(rect),
                color: Color {
                    r: color[0],
                    g: color[1],
                    b: color[2],
                },
                kind: to_annotation_kind(annotation),
                contents,
                opacity,
            },
        )?;
        Ok(())
    })
}

/// `rename_all = "camelCase"` added as part of the Phase 5 Task 4 parked-
/// minors sweep, to match every sibling DTO in this crate (it was the one
/// outlier without it). Verified as a wire-shape no-op, not a real
/// behavior change: every field name here (`id`, `subtype`, `rect`,
/// `contents`) is already a single word, so snake_case and camelCase
/// serialize identically — `apps/desktop/src/lib/backend/types.ts`'s
/// `AnnotationSummaryDto` needed no change either. Kept anyway as a
/// forward-safety net: a future field with a multi-word name won't
/// silently land on the wrong casing convention by omission.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationSummaryDto {
    /// The annotation's real `lopdf` object id (`[object_number,
    /// generation]`) — stable across a `list_page_annotations` call and
    /// a later `delete_annotation_cmd` for it, which is what makes
    /// select-then-delete possible without relying on array position
    /// (fragile the moment two annotations are deleted in one session).
    pub id: [u32; 2],
    pub subtype: String,
    pub rect: [f32; 4],
    pub contents: Option<String>,
}

/// The actual logic behind the desktop's `list_page_annotations`
/// command. Read-only, so unlike the mutating impls in this module it
/// only needs `docs`, not `engine`/`history`.
pub fn list_page_annotations_impl(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    handle: u64,
    page_index: u32,
) -> Result<Vec<AnnotationSummaryDto>, SessionError> {
    let docs = docs.lock().expect("docs lock poisoned");
    let open_doc = resolve_doc(&docs, handle)?;
    let summaries = list_annotations(&open_doc.doc, page_index)?;
    Ok(summaries
        .into_iter()
        .map(|s| AnnotationSummaryDto {
            id: [s.id.0, s.id.1 as u32],
            subtype: s.subtype,
            rect: [s.rect.x0, s.rect.y0, s.rect.x1, s.rect.y1],
            contents: s.contents,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAnnotationRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    /// `[object_number, generation]` — from a prior
    /// [`list_page_annotations_impl`]'s `AnnotationSummaryDto::id`.
    pub annotation_id: [u32; 2],
}

/// The actual logic behind the desktop's `delete_annotation_cmd`.
pub fn delete_annotation_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: DeleteAnnotationRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let annot_id = (request.annotation_id[0], request.annotation_id[1] as u16);
    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        delete_annotation(doc, request.page_index, annot_id)?;
        Ok(())
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSelectionQuadsRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    /// PDF page-space points, the drag gesture's start and end — in
    /// either order, a reversed drag (bottom-right back to top-left)
    /// selects the same range as the forward one.
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// How far (in PDF points) a drag endpoint may sit from the nearest
/// character and still count as "pointing at that character" —
/// `nearest_char_index` always returns *some* character (there's no
/// notion of "too far" baked into a pure nearest-neighbor search), so
/// this is the policy layer on top of it: without a cutoff, dragging in
/// clearly-empty space would still silently snap to whatever text
/// happens to be closest, anywhere on the page. Roughly 1.5 lines of
/// typical body text — generous enough for an imprecise drag near real
/// text, not so generous it reaches across unrelated paragraphs.
const SELECTION_SNAP_MARGIN: f32 = 20.0;

fn near_enough(c: &openpdfedit_engine::CharBox, x: f32, y: f32) -> bool {
    x >= c.left - SELECTION_SNAP_MARGIN
        && x <= c.right + SELECTION_SNAP_MARGIN
        && y >= c.bottom - SELECTION_SNAP_MARGIN
        && y <= c.top + SELECTION_SNAP_MARGIN
}

/// Real text selection for highlight/underline/strikeout: snaps a drag
/// gesture to the actual characters PDFium finds under it (via
/// `Engine::page_char_boxes`, the same real character-extraction PDFium
/// uses for its own text layer — not this codebase's own approximate
/// content-stream interpreter), returning one bounding quad per visual
/// line spanned. Empty on a page with no text, or a drag that starts/ends
/// off any character — the front-end treats that as "nothing to select,"
/// not an error. The actual logic behind the desktop's
/// `text_selection_quads_cmd`.
pub fn text_selection_quads_impl<E: Engine>(
    engine: &E,
    request: TextSelectionQuadsRequest,
) -> Result<Vec<[f32; 4]>, SessionError> {
    let chars = engine.page_char_boxes(request.handle, request.page_index)?;
    let (Some(start), Some(end)) = (
        openpdfedit_engine::nearest_char_index(&chars, request.x0, request.y0),
        openpdfedit_engine::nearest_char_index(&chars, request.x1, request.y1),
    ) else {
        return Ok(Vec::new());
    };
    let start_box = chars.iter().find(|c| c.char_index == start);
    let end_box = chars.iter().find(|c| c.char_index == end);
    let (Some(start_box), Some(end_box)) = (start_box, end_box) else {
        return Ok(Vec::new());
    };
    if !near_enough(start_box, request.x0, request.y0)
        || !near_enough(end_box, request.x1, request.y1)
    {
        return Ok(Vec::new());
    }
    Ok(openpdfedit_engine::char_range_to_line_quads(
        &chars, start, end,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{minimal_pdf_bytes, text_page_pdf_bytes};
    use crate::{redo_impl, undo_impl, FsWorkingStore, SessionState};
    use openpdfedit_doc::Document;

    #[test]
    fn to_annotation_kind_converts_highlight_quads() {
        let input = AnnotationInput::Highlight {
            quads: vec![[1.0, 2.0, 3.0, 4.0]],
        };
        let kind = to_annotation_kind(input);
        let AnnotationKind::Highlight { quads } = kind else {
            panic!("expected Highlight");
        };
        assert_eq!(
            quads,
            vec![Rect {
                x0: 1.0,
                y0: 2.0,
                x1: 3.0,
                y1: 4.0
            }]
        );
    }

    #[test]
    fn to_annotation_kind_converts_ink_strokes() {
        let input = AnnotationInput::Ink {
            strokes: vec![vec![[1.0, 2.0], [3.0, 4.0]]],
        };
        let kind = to_annotation_kind(input);
        let AnnotationKind::Ink { strokes } = kind else {
            panic!("expected Ink");
        };
        assert_eq!(strokes, vec![vec![(1.0, 2.0), (3.0, 4.0)]]);
    }

    #[test]
    fn add_annotation_request_deserializes_from_expected_json_shape() {
        let json = serde_json::json!({
            "handle": 1,
            "pageIndex": 0,
            "rect": [10.0, 20.0, 30.0, 40.0],
            "color": [1.0, 0.0, 0.0],
            "opacity": 0.5,
            "contents": "hello",
            "annotation": { "kind": "highlight", "quads": [[10.0, 20.0, 30.0, 40.0]] }
        });
        let request: AddAnnotationRequest =
            serde_json::from_value(json).expect("should deserialize");
        assert_eq!(request.handle, 1);
        assert_eq!(request.page_index, 0);
        assert_eq!(request.contents.as_deref(), Some("hello"));
    }

    /// The task's Step-1 test: a real document, a real `add_annotation`,
    /// then a full undo/redo round trip observed through
    /// `list_page_annotations_impl` — the first coverage of
    /// `undo_impl`/`redo_impl` driven by a real annotation edit (rather
    /// than the page-delete edits `crate::tests` uses), and the first
    /// test of this module's functions living in this crate at all.
    #[test]
    fn add_annotation_then_undo_then_redo_round_trips_through_list() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-annotation-undo-redo-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");
        let doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
        let mut docs = HashMap::new();
        docs.insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
            },
        );
        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(docs),
            history: Mutex::new(HashMap::new()),
            store: Box::new(FsWorkingStore),
        };

        let request = AddAnnotationRequest {
            handle,
            page_index: 0,
            rect: [72.0, 700.0, 300.0, 720.0],
            color: [1.0, 0.92, 0.23],
            opacity: 0.4,
            contents: Some("integration test".into()),
            annotation: AnnotationInput::Highlight {
                quads: vec![[72.0, 700.0, 300.0, 720.0]],
            },
        };
        let after_add = add_annotation_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            request,
        )
        .expect("add_annotation_impl should succeed");

        let after_add_list = list_page_annotations_impl(&state.docs, after_add.handle, 0).unwrap();
        assert_eq!(
            after_add_list.len(),
            1,
            "the new annotation should be listed"
        );

        let after_undo = undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_add.handle,
        )
        .unwrap();
        let after_undo_list =
            list_page_annotations_impl(&state.docs, after_undo.handle, 0).unwrap();
        assert_eq!(
            after_undo_list.len(),
            0,
            "undoing the add must remove the annotation again"
        );

        let after_redo = redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo.handle,
        )
        .unwrap();
        let after_redo_list =
            list_page_annotations_impl(&state.docs, after_redo.handle, 0).unwrap();
        assert_eq!(
            after_redo_list.len(),
            1,
            "redoing must bring the annotation back"
        );

        state.engine.close(after_redo.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// Real, end-to-end: real file on disk, real `EngineHandle` (PDFium),
    /// real `add_annotation_impl` — the same function the Tauri command
    /// calls, not a re-implementation of it. Exercises the full path
    /// this module exists for: mutate -> incremental save to disk ->
    /// engine handle rotation -> fresh `OpenedDocumentInfo` reflecting
    /// the new file.
    #[test]
    fn add_annotation_impl_saves_to_disk_and_returns_a_fresh_handle() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-add-annotation-impl-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");
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
            },
        );
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        let request = AddAnnotationRequest {
            handle,
            page_index: 0,
            rect: [72.0, 700.0, 300.0, 720.0],
            color: [1.0, 0.92, 0.23],
            opacity: 0.4,
            contents: Some("integration test".into()),
            annotation: AnnotationInput::Highlight {
                quads: vec![[72.0, 700.0, 300.0, 720.0]],
            },
        };

        let result = add_annotation_impl(engine, &docs, &history, &FsWorkingStore, request)
            .expect("add_annotation_impl should succeed");

        assert_ne!(
            result.handle, handle,
            "a successful edit must rotate to a fresh engine handle"
        );
        assert_eq!(result.page_count, 1);
        assert_eq!(result.page_sizes.len(), 1);

        let after_len = std::fs::metadata(&tmp_path).unwrap().len();
        assert!(
            after_len > before_len,
            "incremental save must have appended bytes to the file on disk"
        );

        // The old handle must no longer resolve — it was closed and its
        // doc-store entry removed as part of the handle rotation.
        assert!(engine.page_count(handle).is_err());

        // The new handle is real: PDFium can render through it.
        let tile = engine
            .render_page(result.handle, 0, 100)
            .expect("new handle should render");
        assert!(tile.height > 0);

        // And the annotation itself is genuinely reachable via the new
        // doc-store entry — not just "a save happened somewhere."
        let docs = docs.lock().unwrap();
        let open_doc = &docs
            .get(&result.handle)
            .expect("new handle should be in the doc store")
            .doc;
        let annots = list_annotations(open_doc, 0).expect("listing should succeed");
        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0].subtype, "Highlight");
        assert_eq!(annots[0].contents.as_deref(), Some("integration test"));
        drop(docs);

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// Real, end-to-end: two annotations added via the actual command
    /// logic, one deleted by the exact `id` `list_page_annotations_impl`
    /// reports for it — asserted against a freshly reopened `Document`
    /// (not just "the command returned Ok"), confirming the *other*
    /// annotation survives untouched and the deleted one's object id
    /// really is gone from the page's `/Annots`.
    #[test]
    fn delete_annotation_impl_removes_only_the_targeted_annotation() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-delete-annotation-impl-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");

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

        let make_request = |handle: DocHandle, rect: [f32; 4]| AddAnnotationRequest {
            handle,
            page_index: 0,
            rect,
            color: [1.0, 0.92, 0.23],
            opacity: 0.4,
            contents: None,
            annotation: AnnotationInput::Highlight { quads: vec![rect] },
        };

        let after_first = add_annotation_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            make_request(handle, [10.0, 10.0, 50.0, 30.0]),
        )
        .expect("first add should succeed");
        let after_second = add_annotation_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            make_request(after_first.handle, [10.0, 40.0, 50.0, 60.0]),
        )
        .expect("second add should succeed");

        let before = {
            let docs_guard = docs.lock().unwrap();
            list_annotations(&docs_guard.get(&after_second.handle).unwrap().doc, 0).unwrap()
        };
        assert_eq!(
            before.len(),
            2,
            "both annotations should be present before deletion"
        );
        let target_id = before[0].id;
        let surviving_id = before[1].id;

        let result = delete_annotation_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            DeleteAnnotationRequest {
                handle: after_second.handle,
                page_index: 0,
                annotation_id: [target_id.0, target_id.1 as u32],
            },
        )
        .expect("delete should succeed");

        assert_ne!(
            result.handle, after_second.handle,
            "a successful delete+save must rotate to a fresh engine handle"
        );

        let after = {
            let docs_guard = docs.lock().unwrap();
            list_annotations(&docs_guard.get(&result.handle).unwrap().doc, 0).unwrap()
        };
        assert_eq!(after.len(), 1, "exactly one annotation should remain");
        assert_eq!(
            after[0].id, surviving_id,
            "the surviving annotation must be the one that wasn't targeted"
        );

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn delete_annotation_impl_errors_for_an_id_not_on_the_page() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-delete-annotation-missing-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");

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

        let result = delete_annotation_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            DeleteAnnotationRequest {
                handle,
                page_index: 0,
                annotation_id: [999_999, 0],
            },
        );
        assert!(
            result.is_err(),
            "deleting a nonexistent annotation id must error, not silently succeed"
        );
        // Nothing was written, so the original handle is still live.
        assert!(engine.page_count(handle).is_ok());

        engine.close(handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// Real PDFium end-to-end: a drag over real, rendered Helvetica text
    /// must snap to that text's actual glyph positions (via
    /// `page_char_boxes`, not this codebase's own approximate
    /// content-stream interpreter) — the exact primitive that turns
    /// "freehand rectangle the user has to eyeball onto the words" into
    /// real text selection.
    #[test]
    fn text_selection_quads_impl_snaps_a_drag_to_real_text() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-text-selection-quads-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(
            &tmp_path,
            text_page_pdf_bytes("Hello World", 50.0, 700.0, 18.0),
        )
        .expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");

        // A generous drag rectangle loosely covering the text line — the
        // point is that the *result* should be tight to the real glyphs,
        // not that the input already is. x1=145 lands near "Hello
        // World"'s real right edge at 18pt Helvetica (comfortably inside
        // SELECTION_SNAP_MARGIN of it either way).
        let quads = text_selection_quads_impl(
            engine,
            TextSelectionQuadsRequest {
                handle,
                page_index: 0,
                x0: 40.0,
                y0: 695.0,
                x1: 145.0,
                y1: 715.0,
            },
        )
        .expect("text selection should succeed");

        assert_eq!(
            quads.len(),
            1,
            "a single line of text should produce exactly one quad"
        );
        let [x0, y0, x1, y1] = quads[0];
        assert!(
            x1 > x0 && y1 > y0,
            "quad must have positive width and height"
        );
        // The real text starts at x=50 (the Td we drew it at); the quad's
        // left edge should be close to that, not the drag rectangle's own
        // x0=40 — proof this snapped to the glyphs, not just echoing the
        // input rectangle back.
        assert!(
            (x0 - 50.0).abs() < 10.0,
            "quad left edge {x0} should be close to the real text's start (50), not the drag rect's (40)"
        );

        // A drag entirely below the text line (far outside
        // SELECTION_SNAP_MARGIN of any real character) must come back
        // empty, not silently snap to the nearest text regardless of
        // distance — that would select text nowhere near where the user
        // actually dragged.
        let empty = text_selection_quads_impl(
            engine,
            TextSelectionQuadsRequest {
                handle,
                page_index: 0,
                x0: 40.0,
                y0: 50.0,
                x1: 200.0,
                y1: 60.0,
            },
        )
        .expect("text selection should succeed (as an empty result) even with nothing nearby");
        assert!(
            empty.is_empty(),
            "a drag far from any text must not fabricate a selection"
        );

        engine.close(handle);
        let _ = std::fs::remove_file(&tmp_path);
    }
}
