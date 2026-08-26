//! Annotation commands: thin `#[tauri::command]` wrappers over the real
//! logic, which now lives in `openpdfedit_session::annotations` (moved
//! there so the same code drives the wasm/Chrome-extension build) — see
//! that module's doc comment for the full rationale. The request/DTO
//! types (`AddAnnotationRequest`, `AnnotationSummaryDto`,
//! `DeleteAnnotationRequest`, `TextSelectionQuadsRequest`) are
//! re-exported here under the same names/paths so Tauri's generated IPC
//! bindings (and this crate's own JSON shape) don't change.

use openpdfedit_session::annotations::{
    add_annotation_impl, delete_annotation_impl, list_page_annotations_impl, select_text_impl,
    text_selection_quads_impl,
};
pub use openpdfedit_session::annotations::{
    AddAnnotationRequest, AnnotationSummaryDto, DeleteAnnotationRequest, TextSelection,
    TextSelectionQuadsRequest,
};
use tauri::State;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn add_annotation_cmd(
    state: State<'_, AppState>,
    request: AddAnnotationRequest,
) -> Result<OpenedDocument, CommandError> {
    add_annotation_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn list_page_annotations(
    state: State<'_, AppState>,
    handle: u64,
    page_index: u32,
) -> Result<Vec<AnnotationSummaryDto>, CommandError> {
    list_page_annotations_impl(&state.docs, handle, page_index).map_err(Into::into)
}

#[tauri::command]
pub fn delete_annotation_cmd(
    state: State<'_, AppState>,
    request: DeleteAnnotationRequest,
) -> Result<OpenedDocument, CommandError> {
    delete_annotation_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}

/// Real text selection for highlight/underline/strikeout — see
/// `openpdfedit_session::annotations::text_selection_quads_impl` for the
/// snapping logic.
#[tauri::command]
pub fn text_selection_quads_cmd(
    state: State<'_, AppState>,
    request: TextSelectionQuadsRequest,
) -> Result<Vec<[f32; 4]>, CommandError> {
    text_selection_quads_impl(&state.engine, request).map_err(Into::into)
}

/// The Select tool's drag: the same geometry, plus the characters, so
/// there is something to copy.
#[tauri::command]
pub fn select_text_cmd(
    state: State<'_, AppState>,
    request: TextSelectionQuadsRequest,
) -> Result<TextSelection, CommandError> {
    select_text_impl(&state.engine, request).map_err(Into::into)
}

// Every `_impl` function this module used to define directly
// (`add_annotation_impl`/`list_page_annotations`/`delete_annotation_impl`/
// `text_selection_quads_impl`, plus their DTO types and helper functions
// `rect_from_flat`/`to_annotation_kind`/`near_enough`/
// `SELECTION_SNAP_MARGIN`) moved to `openpdfedit_session::annotations`,
// along with their tests — see that module for both.
