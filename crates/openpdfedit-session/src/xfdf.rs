//! XFDF import and export — annotations as a portable file.
//!
//! Split the way [`crate::signatures`] is, and for the same reason: the
//! parts that deal in *bytes* are portable and ungated, and only the
//! desktop wraps them in filesystem reads and writes. Here that split is
//! especially natural, because XFDF is text — export produces a string
//! and import consumes one, so both halves are portable as they stand
//! and the extension can hand the string to a download or take it from a
//! file input without any of this changing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use serde::Serialize;

use crate::{
    commit_mutation, resolve_doc, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError,
    WorkingStore,
};

impl From<openpdfedit_xfdf::XfdfError> for SessionError {
    fn from(e: openpdfedit_xfdf::XfdfError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportXfdfDto {
    /// The XFDF document itself.
    pub xml: String,
    pub exported: usize,
    /// A filename the caller can offer as a default — the document's own
    /// name with an `.xfdf` extension.
    pub suggested_name: String,
}

/// Reads every markup annotation out of the open document and serializes
/// it as XFDF.
pub fn export_xfdf_impl(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    handle: DocHandle,
) -> Result<ExportXfdfDto, SessionError> {
    let (annotations, source_name) = {
        let docs_guard = docs.lock().expect("docs lock poisoned");
        let open_doc = resolve_doc(&docs_guard, handle)?;
        // The name of the file the user opened, not the scratch copy —
        // an XFDF naming a temp file tells a recipient nothing.
        let name = open_doc
            .original_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        (openpdfedit_xfdf::extract(&open_doc.doc)?, name)
    };

    let xml = openpdfedit_xfdf::to_xfdf(&annotations, source_name.as_deref())?;
    let suggested_name = source_name
        .as_deref()
        .map(|n| {
            let stem = n
                .strip_suffix(".pdf")
                .or_else(|| n.strip_suffix(".PDF"))
                .unwrap_or(n);
            format!("{stem}.xfdf")
        })
        .unwrap_or_else(|| "annotations.xfdf".to_string());

    Ok(ExportXfdfDto {
        exported: annotations.len(),
        xml,
        suggested_name,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportXfdfDto {
    pub document: OpenedDocumentInfo,
    pub imported: usize,
    /// Annotation kinds this app can't draw. Counted rather than
    /// approximated with something the sender didn't draw.
    pub skipped: usize,
    /// Annotations addressed to pages this document doesn't have — an
    /// XFDF written against a different or later revision of the file.
    pub out_of_range: usize,
}

/// Adds every annotation in `xml` that this app knows how to create.
pub fn import_xfdf_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
    xml: &str,
) -> Result<ImportXfdfDto, SessionError> {
    // Parsed before the document is touched, so a file that isn't XFDF
    // fails without leaving a half-applied import behind.
    let annotations = openpdfedit_xfdf::from_xfdf(xml)?;

    let mut report = openpdfedit_xfdf::ImportReport::default();
    let document =
        commit_mutation::<E, SessionError>(engine, docs, history, store, handle, |doc| {
            report = openpdfedit_xfdf::import(doc, &annotations)?;
            Ok(())
        })?;

    Ok(ImportXfdfDto {
        document,
        imported: report.imported,
        skipped: report.skipped,
        out_of_range: report.out_of_range,
    })
}
