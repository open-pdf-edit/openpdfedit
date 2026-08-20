//! Redaction command: thin `#[tauri::command]` wrapper over the real
//! logic, which now lives in `openpdfedit_session::redact` (moved there
//! so the same code drives the wasm/Chrome-extension build) — see that
//! module's doc comment for the full rationale. `RedactPageRequest` is
//! re-exported here under the same name/path so Tauri's generated IPC
//! bindings (and this crate's own JSON shape) don't change.

pub use openpdfedit_session::redact::RedactPageRequest;
use tauri::State;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn redact_page_cmd(
    state: State<'_, AppState>,
    request: RedactPageRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::redact::redact_page_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}

// `redact_page_impl`/`RedactPageRequest` moved to
// `openpdfedit_session::redact`, along with their tests — see that
// module for both.
