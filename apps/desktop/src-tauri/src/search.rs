//! Find-in-document: thin `#[tauri::command]` wrapper over
//! `openpdfedit_session::search`, which holds the real logic so the same
//! code drives the wasm/Chrome-extension build — see that module's doc
//! for why search is generic over `E: Engine` and touches neither `docs`
//! nor the working copy.

use openpdfedit_engine::DocHandle;
pub use openpdfedit_session::search::SearchResultsDto;
use serde::Deserialize;
use tauri::State;

use crate::{AppState, CommandError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    handle: DocHandle,
    query: String,
    match_case: bool,
    whole_word: bool,
}

#[tauri::command]
pub fn search_document_cmd(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<SearchResultsDto, CommandError> {
    openpdfedit_session::search::search_document_impl(
        &state.engine,
        request.handle,
        &request.query,
        request.match_case,
        request.whole_word,
    )
    .map_err(Into::into)
}
