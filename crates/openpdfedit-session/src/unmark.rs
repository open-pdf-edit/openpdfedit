//! Remove-markup command — the mirror of [`crate::flatten`].
//!
//! Both answer the same question about a page (which of this is
//! markup?) and then do opposite things with the answer. The reason
//! this one exists separately is that markup does not always survive as
//! annotations: most apps that export an annotated PDF flatten the pen
//! strokes into the page as one transparent overlay image, and after
//! that no annotation-handling code can touch them. See
//! `openpdfedit-unmark`'s module doc for how such a layer is told apart
//! from a page that simply is an image.
//!
//! An ordinary mutating command otherwise: through
//! [`crate::commit_mutation`] like flatten and redaction, so it lands in
//! the working copy and can be undone.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_unmark::{remove_markup, Removed};
use serde::{Deserialize, Serialize};

use crate::{commit_mutation, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore};

impl From<openpdfedit_unmark::UnmarkError> for SessionError {
    fn from(e: openpdfedit_unmark::UnmarkError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMarkupRequest {
    pub handle: DocHandle,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMarkupResultDto {
    pub document: OpenedDocumentInfo,
    /// Annotations deleted — highlights, notes, ink, stamps.
    pub annotations: usize,
    /// Flattened markup layers dropped from page content. Usually one
    /// per marked-up page.
    pub layers: usize,
}

pub fn remove_markup_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: RemoveMarkupRequest,
) -> Result<RemoveMarkupResultDto, SessionError> {
    // As in `flatten`: `commit_mutation` hands back only the document,
    // so the report is captured out of the closure.
    let mut report = Removed::default();
    let document =
        commit_mutation::<E, SessionError>(engine, docs, history, store, request.handle, |doc| {
            report = remove_markup(doc)?;
            Ok(())
        })?;

    Ok(RemoveMarkupResultDto {
        document,
        annotations: report.annotations,
        layers: report.layers,
    })
}
