//! Signature *inspection* command: thin `#[tauri::command]` wrapper over
//! the real logic, which now lives in `openpdfedit_session::signatures`
//! (moved there so the same code drives the wasm/Chrome-extension build)
//! — see that module's doc comment for the full rationale, including why
//! `list_signatures_impl` reads a path through `store` rather than
//! genericizing over `E: Engine` (it never touches the engine at all).
//! `SignatureInfoDto` is re-exported here under the same name/path so
//! Tauri's generated IPC bindings (and this crate's own JSON shape)
//! don't change.

use openpdfedit_engine::DocHandle;
pub use openpdfedit_session::signatures::SignatureInfoDto;
use tauri::State;

use crate::{AppState, CommandError};

#[tauri::command]
pub fn list_signatures_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
) -> Result<Vec<SignatureInfoDto>, CommandError> {
    openpdfedit_session::signatures::list_signatures_impl(&state.docs, &*state.store, handle)
        .map_err(Into::into)
}

// `list_signatures_impl`/`SignatureInfoDto` moved to
// `openpdfedit_session::signatures`, along with their tests — see that
// module for both.
