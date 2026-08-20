//! AcroForm field *creation* command — thin `#[tauri::command]` wrapper.
//! The real logic (`create_form_field_impl`) now lives in
//! `openpdfedit_session::forms`, moved there so the same code can drive
//! both this desktop shell and (later) the wasm/Chrome-extension build —
//! see that module's doc comment for the full rationale (including why
//! field *creation*, unlike form fill, genericizes over `E: Engine` the
//! normal way).

use tauri::State;

pub use openpdfedit_session::forms::CreateFormFieldRequest;

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn create_form_field_cmd(
    state: State<'_, AppState>,
    request: CreateFormFieldRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::forms::create_form_field_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}
