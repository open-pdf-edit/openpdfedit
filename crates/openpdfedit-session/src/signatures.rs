//! Signature *inspection* command, moved here (from
//! `apps/desktop/src-tauri/src/signatures.rs`) for the same reason as
//! [`crate::annotations`]: the same logic should drive both the
//! desktop's thread-wrapped `EngineHandle` and a bare in-process engine
//! for the wasm/Chrome-extension build — though this module never touches
//! the engine at all, only a document's raw bytes. See `openpdfedit-sign`'s
//! module doc for why this reports structural facts, not a cryptographic
//! verdict. The DTO field names deliberately spell that out
//! (`isVerified: false`, always) rather than a bare boolean the front-end
//! could accidentally read as "signature is good."
//!
//! Split the same way [`crate::pages`]'s merge/extract used to be: the
//! byte-level scan ([`list_signatures_in_bytes`]) is wasm-clean and left
//! ungated, since `openpdfedit_sign::find_signatures` already takes an
//! in-memory buffer; the path-based half that reads a document's *working
//! copy* ([`list_signatures_impl`]) resolves the handle's path and hands
//! it to [`list_signatures_in_bytes`].
//!
//! As of Phase 4 Task 2, [`list_signatures_impl`] is portable and
//! ungated too — it reads the working copy through `store.read(&path)`
//! (a [`crate::WorkingStore`] parameter) rather than `std::fs::read`, the
//! same reroute [`crate::fill_form_fields_impl`] and
//! [`crate::reopen_after_write`] already use. Unlike
//! [`crate::pages`]'s merge/extract or [`crate::compare`] (both still
//! `#[cfg(not(target_arch = "wasm32"))]`), this function has no
//! arbitrary-output-path write side to worry about — it only ever reads
//! the one path already tracked in `docs`/`store` for `handle`, exactly
//! the read [`crate::capture_pre_edit_snapshot`] performs, which is
//! already portable — so there was no desktop-only filesystem operation
//! left here to gate around once the read moved behind `store`.

use std::collections::HashMap;
use std::sync::Mutex;

use openpdfedit_engine::DocHandle;
use serde::Serialize;

use crate::{resolve_doc, OpenDoc, SessionError, WorkingStore};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfoDto {
    sub_filter: Option<String>,
    reason: Option<String>,
    name: Option<String>,
    signing_time: Option<String>,
    byte_range_is_structurally_sound: bool,
    /// Always `false` in this build — no cryptographic signature
    /// verification is implemented yet (see `openpdfedit-sign`'s module
    /// doc). Present explicitly, rather than omitted, so the front-end
    /// (and anyone reading the wire format) can't mistake "we found a
    /// signature" for "we confirmed it's valid."
    is_verified: bool,
}

impl From<openpdfedit_sign::SignError> for SessionError {
    fn from(e: openpdfedit_sign::SignError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

impl From<openpdfedit_sign::SignatureInfo> for SignatureInfoDto {
    fn from(s: openpdfedit_sign::SignatureInfo) -> Self {
        SignatureInfoDto {
            sub_filter: s.sub_filter,
            reason: s.reason,
            name: s.name,
            signing_time: s.signing_time,
            byte_range_is_structurally_sound: s.byte_range_is_structurally_sound,
            is_verified: false,
        }
    }
}

/// Wasm-clean byte-level core behind [`list_signatures_impl`] — see this
/// module's doc for why the split exists.
pub fn list_signatures_in_bytes(bytes: &[u8]) -> Result<Vec<SignatureInfoDto>, SessionError> {
    let signatures = openpdfedit_sign::find_signatures(bytes)?;
    Ok(signatures.into_iter().map(Into::into).collect())
}

/// The actual logic behind the desktop's `list_signatures_cmd`.
/// Path-based (reads the open document's working copy through `store`)
/// — portable as of Phase 4 Task 2, see this module's doc.
pub fn list_signatures_impl(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
) -> Result<Vec<SignatureInfoDto>, SessionError> {
    let path = {
        let docs_guard = docs.lock().expect("docs lock poisoned");
        resolve_doc(&docs_guard, handle)?.path.clone()
    };
    let bytes = store.read(&path)?;
    list_signatures_in_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FsWorkingStore, MemWorkingStore};
    use openpdfedit_doc::Document;

    fn signed_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Object, StringFormat};

        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let sig_id = doc.add_object(dictionary! {
            "Type" => "Sig",
            "SubFilter" => "adbe.pkcs7.detached",
            "Reason" => Object::string_literal("Approval"),
            "ByteRange" => vec![0.into(), 100.into(), 200.into(), 50.into()],
            "Contents" => Object::String(vec![0u8; 16], StringFormat::Hexadecimal),
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "SigRef" => sig_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn list_signatures_in_bytes_finds_a_signature_and_marks_it_unverified() {
        let dtos = list_signatures_in_bytes(&signed_pdf_bytes())
            .expect("list_signatures_in_bytes should succeed");
        assert_eq!(dtos.len(), 1);
        assert!(!dtos[0].is_verified, "must never report verified=true");
        assert_eq!(dtos[0].sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
    }

    #[test]
    fn list_signatures_impl_finds_a_signature_via_the_open_documents_working_copy() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };
        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-signatures-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, signed_pdf_bytes()).expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");
        let doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
                encryption: None,
            },
        );

        let dtos = list_signatures_impl(&docs, &FsWorkingStore, handle)
            .expect("list_signatures_impl should succeed");
        assert_eq!(dtos.len(), 1);
        assert!(!dtos[0].is_verified);

        engine.close(handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn list_signatures_impl_unknown_handle_returns_error_not_panic() {
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        assert!(list_signatures_impl(&docs, &FsWorkingStore, 999_999).is_err());
    }

    /// Phase 4 Task 2's dedicated portable test: driven entirely through
    /// [`MemWorkingStore`] — no temp files, no `std::fs` call anywhere in
    /// this test. Unlike the portable tests in `forms.rs`/`lib.rs`, this
    /// one needs no [`crate::test_support::shared_handle`]/real PDFium
    /// engine at all and so never skips: `list_signatures_impl` never
    /// touches the engine (see this module's doc), only `docs` and
    /// `store`, both of which this test builds by hand.
    #[test]
    fn list_signatures_impl_finds_a_signature_through_mem_working_store() {
        let store = MemWorkingStore::default();
        let path = std::path::PathBuf::from("mem-signatures-test.pdf");
        let bytes = signed_pdf_bytes();
        store
            .write(&path, &bytes)
            .expect("store.write should succeed");

        let doc = Document::from_bytes(&bytes).expect("doc crate should parse the bytes");
        let handle: DocHandle = 1;
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            handle,
            OpenDoc {
                path: path.clone(),
                original_path: path,
                dirty: false,
                doc,
                encryption: None,
            },
        );

        let dtos = list_signatures_impl(&docs, &store, handle)
            .expect("list_signatures_impl through MemWorkingStore should succeed");
        assert_eq!(dtos.len(), 1);
        assert!(!dtos[0].is_verified, "must never report verified=true");
        assert_eq!(dtos[0].sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
    }
}
