//! Compress-copy command: thin `#[tauri::command]` wrapper over
//! `openpdfedit_session::compress_document_to_path_impl` (see that
//! function's doc for what "compress" means here and the signature
//! trade-off it documents). Export-shaped like `extract_pages_cmd`:
//! takes an output path from the UI's save picker, never mutates the
//! open document.

use openpdfedit_session::CompressStats;
use serde::Deserialize;
use std::path::Path;
use tauri::State;

use crate::{AppState, CommandError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressDocumentRequest {
    pub handle: openpdfedit_engine::DocHandle,
    pub output_path: String,
}

#[tauri::command]
pub fn compress_document_cmd(
    state: State<'_, AppState>,
    request: CompressDocumentRequest,
) -> Result<CompressStats, CommandError> {
    openpdfedit_session::compress_document_to_path_impl(
        &state,
        request.handle,
        Path::new(&request.output_path),
    )
    .map_err(Into::into)
}
