//! Page numbers and Bates numbering: thin `#[tauri::command]` wrapper
//! over `openpdfedit_session::numbering`, which holds the real logic so
//! the same code drives the wasm/Chrome-extension build.

pub use openpdfedit_session::numbering::NumberPagesRequest;
use tauri::State;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn number_pages_cmd(
    state: State<'_, AppState>,
    request: NumberPagesRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::numbering::number_pages_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}
