//! Save a password-protected copy: thin `#[tauri::command]` wrapper over
//! `openpdfedit_session::encrypt`. Export-shaped like the compress
//! command — writes a new file, never mutates the open document; see
//! that module's doc for why.

use openpdfedit_engine::DocHandle;
pub use openpdfedit_session::encrypt::{EncryptChoices, EncryptStats};
use serde::Deserialize;
use std::path::Path;
use tauri::State;

use crate::{AppState, CommandError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptDocumentRequest {
    handle: DocHandle,
    output_path: String,
    #[serde(flatten)]
    choices: EncryptChoices,
}

#[tauri::command]
pub fn encrypt_document_cmd(
    state: State<'_, AppState>,
    request: EncryptDocumentRequest,
) -> Result<EncryptStats, CommandError> {
    openpdfedit_session::encrypt::encrypt_document_to_path_impl(
        &state,
        request.handle,
        Path::new(&request.output_path),
        &request.choices,
    )
    .map_err(Into::into)
}
