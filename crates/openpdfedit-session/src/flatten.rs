//! Flattening markup and form fields into the page.
//!
//! An ordinary mutating command — it goes through [`crate::commit_mutation`]
//! like redaction and page ops, so it's undoable and lands in the working
//! copy. That matters more here than for most edits, because flattening
//! is deliberately destructive: afterwards the highlights are page
//! content and there is no annotation left to adjust or remove.
//!
//! The drawing itself is `openpdfedit-flatten`; everything here is the
//! DTO boundary and the report the UI needs to say what it left alone.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_flatten::{flatten, FlattenOptions};
use serde::{Deserialize, Serialize};

use crate::{commit_mutation, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore};

impl From<openpdfedit_flatten::FlattenError> for SessionError {
    fn from(e: openpdfedit_flatten::FlattenError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenDocumentRequest {
    pub handle: DocHandle,
    /// Bake markup — highlights, notes, ink, drawn signatures — into the
    /// page.
    pub annotations: bool,
    /// Bake filled form values in and remove the interactive form. A
    /// flattened form can be read but not refilled.
    pub form_fields: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenResultDto {
    pub document: OpenedDocumentInfo,
    pub flattened: usize,
    /// Left interactive: a link, or something with no appearance to draw.
    pub skipped: usize,
    /// Popup windows removed alongside their parent markup annotation.
    pub popups_removed: usize,
}

pub fn flatten_document_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: FlattenDocumentRequest,
) -> Result<FlattenResultDto, SessionError> {
    let options = FlattenOptions {
        annotations: request.annotations,
        form_fields: request.form_fields,
    };

    // `commit_mutation` owns the snapshot/save/reopen sequence and hands
    // back only the document, so the report is captured out of the
    // closure rather than returned through it.
    let mut report = openpdfedit_flatten::FlattenReport::default();
    let document =
        commit_mutation::<E, SessionError>(engine, docs, history, store, request.handle, |doc| {
            report = flatten(doc, &options)?;
            Ok(())
        })?;

    Ok(FlattenResultDto {
        document,
        flattened: report.flattened,
        skipped: report.skipped,
        popups_removed: report.popups_removed,
    })
}
