//! Flatten: thin `#[tauri::command]` wrapper over
//! `openpdfedit_session::flatten`, which holds the real logic so the same
//! code drives the wasm/Chrome-extension build.

pub use openpdfedit_session::flatten::{FlattenDocumentRequest, FlattenResultDto};
use tauri::State;

use crate::{AppState, CommandError};

#[tauri::command]
pub fn flatten_document_cmd(
    state: State<'_, AppState>,
    request: FlattenDocumentRequest,
) -> Result<FlattenResultDto, CommandError> {
    openpdfedit_session::flatten::flatten_document_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}
