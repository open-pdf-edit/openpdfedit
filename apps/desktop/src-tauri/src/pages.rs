//! Page-organization commands: thin `#[tauri::command]` wrappers over the
//! real logic, which now lives in `openpdfedit_session::pages` (moved
//! there so the same code drives the wasm/Chrome-extension build) — see
//! that module's doc comment for the full rationale, including why
//! merge/extract's path-based orchestration stayed `#[cfg(not(target_arch
//! = "wasm32"))]`-gated there rather than becoming fully portable. The
//! request/DTO types (`MergeRequest`, `ExtractRequest`, `MoveDirection`)
//! are re-exported here under the same names/paths so Tauri's generated
//! IPC bindings (and this crate's own JSON shape) don't change.

use openpdfedit_engine::DocHandle;
use openpdfedit_session::pages::{
    delete_page_impl, extract_pages_impl, merge_documents_impl, move_page_impl, rotate_page_impl,
    set_crop_box_impl,
};
pub use openpdfedit_session::pages::{ExtractRequest, MergeRequest, MoveDirection};
use tauri::State;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn rotate_page_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
    page_index: u32,
    delta_degrees: i32,
) -> Result<OpenedDocument, CommandError> {
    rotate_page_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
        page_index,
        delta_degrees,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn delete_page_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
    page_index: u32,
) -> Result<OpenedDocument, CommandError> {
    delete_page_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
        page_index,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn set_crop_box_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
    page_index: u32,
    rect: [f32; 4],
) -> Result<OpenedDocument, CommandError> {
    set_crop_box_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
        page_index,
        rect,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn move_page_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
    page_index: u32,
    direction: MoveDirection,
) -> Result<OpenedDocument, CommandError> {
    move_page_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
        page_index,
        direction,
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn merge_documents_cmd(
    state: State<'_, AppState>,
    request: MergeRequest,
) -> Result<OpenedDocument, CommandError> {
    merge_documents_impl(&state.engine, &state.docs, &state.history, request).map_err(Into::into)
}

#[tauri::command]
pub fn extract_pages_cmd(
    state: State<'_, AppState>,
    request: ExtractRequest,
) -> Result<OpenedDocument, CommandError> {
    extract_pages_impl(&state.engine, &state.docs, &state.history, request).map_err(Into::into)
}

// Every `_impl` function this module used to define directly
// (`rotate_page_impl`/`delete_page_impl`/`set_crop_box_impl`/
// `move_page_impl`/`merge_documents_impl`/`extract_pages_impl`, plus
// `MoveDirection`/`MergeRequest`/`ExtractRequest`, `gather_merge_sources`,
// and `open_new_file`) moved to `openpdfedit_session::pages`, along with
// their tests — see that module for both.
