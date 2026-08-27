//! Remove markup: thin `#[tauri::command]` wrapper over
//! `openpdfedit_session::unmark`, which holds the real logic so the same
//! code drives the wasm/Chrome-extension build.

pub use openpdfedit_session::unmark::{RemoveMarkupRequest, RemoveMarkupResultDto};
use tauri::State;

use crate::{AppState, CommandError};

#[tauri::command]
pub fn remove_markup_cmd(
    state: State<'_, AppState>,
    request: RemoveMarkupRequest,
) -> Result<RemoveMarkupResultDto, CommandError> {
    openpdfedit_session::unmark::remove_markup_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}
