//! XFDF import and export.
//!
//! The desktop half: the portable string-in/string-out logic lives in
//! `openpdfedit_session::xfdf` (so the extension can hand the same XML to
//! a download or take it from a file input), and this wraps it in the
//! filesystem reads and writes only a desktop app can do.

use openpdfedit_engine::DocHandle;
use openpdfedit_session::xfdf::ImportXfdfDto;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{AppState, CommandError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportXfdfRequest {
    handle: DocHandle,
    output_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportXfdfResult {
    path: String,
    exported: usize,
}

#[tauri::command]
pub fn export_xfdf_cmd(
    state: State<'_, AppState>,
    request: ExportXfdfRequest,
) -> Result<ExportXfdfResult, CommandError> {
    let exported = openpdfedit_session::xfdf::export_xfdf_impl(&state.docs, request.handle)?;
    std::fs::write(&request.output_path, exported.xml)?;
    Ok(ExportXfdfResult {
        path: request.output_path,
        exported: exported.exported,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportXfdfRequest {
    handle: DocHandle,
    input_path: String,
}

#[tauri::command]
pub fn import_xfdf_cmd(
    state: State<'_, AppState>,
    request: ImportXfdfRequest,
) -> Result<ImportXfdfDto, CommandError> {
    let xml = std::fs::read_to_string(&request.input_path)?;
    openpdfedit_session::xfdf::import_xfdf_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        request.handle,
        &xml,
    )
    .map_err(Into::into)
}
