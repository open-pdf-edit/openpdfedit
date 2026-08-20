//! Watermark command: thin `#[tauri::command]` wrapper over
//! `openpdfedit_session::watermark` (portable, engine-generic — the same
//! logic drives the wasm/Chrome-extension build). `ApplyWatermarkRequest`
//! is re-exported here so Tauri's IPC bindings and this crate's JSON
//! shape stay in one place, same arrangement as `redact.rs`.

pub use openpdfedit_session::watermark::ApplyWatermarkRequest;
use tauri::State;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn apply_watermark_cmd(
    state: State<'_, AppState>,
    request: ApplyWatermarkRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::watermark::apply_watermark_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}
