//! AcroForm fill commands — thin `#[tauri::command]` wrappers. The real
//! logic (`list_form_fields_impl`/`fill_form_fields_impl`) now lives in
//! `openpdfedit_session::forms`, moved there so the same code can drive
//! both this desktop shell and the wasm/Chrome-extension build — see that
//! module's doc comment for the full rationale. As of Phase 3 Task 2,
//! both operations are generic over `E: Engine` like every other command
//! module in that crate (this file just instantiates them at
//! `EngineHandle` by passing `&state.engine`, same as every other command
//! wrapper here).

use openpdfedit_engine::DocHandle;
use tauri::State;

pub use openpdfedit_session::forms::{FillFormRequest, FormFieldDto};

use crate::{AppState, CommandError, OpenedDocument};

#[tauri::command]
pub fn list_form_fields_cmd(
    state: State<'_, AppState>,
    handle: DocHandle,
) -> Result<Vec<FormFieldDto>, CommandError> {
    openpdfedit_session::forms::list_form_fields_impl(&state.engine, handle).map_err(Into::into)
}

#[tauri::command]
pub fn fill_form_fields_cmd(
    state: State<'_, AppState>,
    request: FillFormRequest,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::forms::fill_form_fields_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request,
    )
    .map_err(Into::into)
}
