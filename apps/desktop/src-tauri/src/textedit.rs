//! MVP text-run/image-placement editing commands: thin `#[tauri::command]`
//! wrappers over the real logic, which now lives in
//! `openpdfedit_session::textedit` (moved there so the same code drives
//! the wasm/Chrome-extension build) — see that module's doc comment for
//! the full rationale. The request/DTO types are re-exported here under
//! the same names/paths so Tauri's generated IPC bindings (and this
//! crate's own JSON shape) don't change.

use openpdfedit_engine::DocHandle;
pub use openpdfedit_session::textedit::{
    EditTextRunRequest, ImagePlacementDto, MoveImageRequest, MoveTextRunRequest, TextRunDto,
};
use tauri::State;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn list_text_runs_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
    page_index: u32,
) -> Result<Vec<TextRunDto>, CommandError> {
    openpdfedit_session::textedit::list_text_runs_impl(&state.docs, handle, page_index)
        .map_err(Into::into)
}

#[tauri::command]
pub fn edit_text_run_cmd(
    state: State<'_, AppState>,
    request: EditTextRunRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::textedit::edit_text_run_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}

/// Relocates a text run without touching its content — see
/// `openpdfedit_session::textedit::move_text_run_impl`'s doc for why this
/// imposes no `isEditable` requirement, unlike [`edit_text_run_cmd`].
#[tauri::command]
pub fn move_text_run_cmd(
    state: State<'_, AppState>,
    request: MoveTextRunRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::textedit::move_text_run_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn list_image_placements_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
    page_index: u32,
) -> Result<Vec<ImagePlacementDto>, CommandError> {
    openpdfedit_session::textedit::list_image_placements_impl(&state.docs, handle, page_index)
        .map_err(Into::into)
}

#[tauri::command]
pub fn move_image_cmd(
    state: State<'_, AppState>,
    request: MoveImageRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::textedit::move_image_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}

// Every `_impl` function this module used to define directly
// (`list_text_runs_impl`/`edit_text_run_impl`/`move_text_run_impl`/
// `list_image_placements_impl`/`move_image_impl`), plus the request/DTO
// types, moved to `openpdfedit_session::textedit`, along with their
// tests — see that module for both.
