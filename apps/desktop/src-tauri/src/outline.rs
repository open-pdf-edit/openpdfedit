//! The document's outline (bookmarks): thin `#[tauri::command]` wrapper
//! over `openpdfedit_session::outline`, which holds the real logic so the
//! same code drives the wasm/Chrome-extension build.

use openpdfedit_engine::DocHandle;
pub use openpdfedit_session::outline::OutlineEntryDto;
use tauri::State;

use crate::{AppState, CommandError};

#[tauri::command]
pub fn document_outline_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
) -> Result<Vec<OutlineEntryDto>, CommandError> {
    openpdfedit_session::outline::document_outline_impl(&state.docs, handle).map_err(Into::into)
}
