//! Document compare command: thin `#[tauri::command]` wrapper over the
//! real logic, which now lives in `openpdfedit_session::compare` (moved
//! there so the same code drives the wasm/Chrome-extension build) — see
//! that module's doc comment for the full rationale, including the two
//! small behavioral changes (invisible from this command's DTO shape)
//! the move produced. `CompareRequest`/`CompareReportDto` are re-exported
//! here under the same names/paths so Tauri's generated IPC bindings
//! (and this crate's own JSON shape) don't change.

pub use openpdfedit_session::compare::{CompareReportDto, CompareRequest};
use tauri::State;

use crate::{AppState, CommandError};

#[tauri::command]
pub fn compare_documents_cmd(
    state: State<'_, AppState>,
    request: CompareRequest,
) -> Result<CompareReportDto, CommandError> {
    openpdfedit_session::compare::compare_documents_impl(&state.engine, request).map_err(Into::into)
}

// `compare_documents_impl`/`CompareRequest`/`CompareReportDto`/
// `TextPageDiffDto`/`PixelPageDiffDto` moved to
// `openpdfedit_session::compare`, along with their tests — see that
// module for both.
