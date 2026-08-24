//! Saving a password-protected copy.
//!
//! Export-shaped, like [`crate::compress_document_to_path_impl`]: it
//! writes a *new* file and leaves the open document alone. That isn't
//! arbitrary — encrypting the working copy in place would immediately
//! require the password to render the very document still on screen,
//! and every subsequent edit would have to decrypt and re-encrypt it.
//! "Save a protected copy" is both simpler and what people actually
//! want.
//!
//! The bytes come from the working copy rather than from PDFium's own
//! writer, so the copy carries the edits made in this session.

use std::path::Path;

use openpdfedit_crypt::{encrypt_document, Permissions};
use openpdfedit_engine::{DocHandle, Engine};
use serde::{Deserialize, Serialize};

use crate::{resolve_doc, SessionError, SessionState};

impl From<openpdfedit_crypt::CryptError> for SessionError {
    fn from(e: openpdfedit_crypt::CryptError) -> Self {
        match e {
            // Kept distinct all the way to the front-end, which turns it
            // into a password prompt rather than an error banner.
            openpdfedit_crypt::CryptError::PasswordRequired => SessionError::PasswordRequired,
            other => SessionError::Doc(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptChoices {
    /// The password a reader is prompted for. Required.
    pub user_password: String,
    /// Unlocks full permissions. Empty reuses the user password, which
    /// is what "just put a password on it" means.
    #[serde(default)]
    pub owner_password: String,
    #[serde(default = "yes")]
    pub allow_print: bool,
    #[serde(default = "yes")]
    pub allow_modify: bool,
    #[serde(default = "yes")]
    pub allow_copy: bool,
    #[serde(default = "yes")]
    pub allow_annotate: bool,
}

fn yes() -> bool {
    true
}

impl EncryptChoices {
    fn permissions(&self) -> Permissions {
        Permissions {
            print: self.allow_print,
            modify: self.allow_modify,
            copy: self.allow_copy,
            annotate: self.allow_annotate,
            fill_forms: self.allow_annotate,
            // Never withheld: refusing text extraction to assistive
            // technology locks the document away from screen readers,
            // and the flag stops nobody else — /P is honoured by
            // convention, not enforced.
            extract_for_accessibility: true,
            assemble: self.allow_modify,
            print_high_resolution: self.allow_print,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptStats {
    pub bytes: u64,
}

/// Produces the encrypted bytes for `handle`'s current working copy.
/// Portable — no filesystem — so the extension can hand the result to a
/// download.
pub fn encrypt_document_bytes<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
    choices: &EncryptChoices,
) -> Result<Vec<u8>, SessionError> {
    if choices.user_password.is_empty() {
        return Err(SessionError::Doc("A password is required.".to_string()));
    }
    let working = {
        let guard = state.docs.lock().expect("docs lock poisoned");
        resolve_doc(&guard, handle)?.path.clone()
    };
    let plain = state.store.read(&working)?;
    Ok(encrypt_document(
        &plain,
        &choices.user_password,
        &choices.owner_password,
        choices.permissions(),
    )?)
}

/// The desktop half: the same bytes, written to a chosen path.
#[cfg(not(target_arch = "wasm32"))]
pub fn encrypt_document_to_path_impl<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
    output_path: &Path,
    choices: &EncryptChoices,
) -> Result<EncryptStats, SessionError> {
    let bytes = encrypt_document_bytes(state, handle, choices)?;
    std::fs::write(output_path, &bytes)?;
    Ok(EncryptStats {
        bytes: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FsWorkingStore, OpenDoc};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// The dangerous direction: a document opened *with* a password must
    /// not be written back without one. The working copy is stored
    /// decrypted so the edit paths can ignore encryption, which makes
    /// saving the single place that has to put it back — and a silent
    /// failure here strips protection off the user's own file.
    #[test]
    fn saving_a_protected_document_writes_it_back_protected() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };

        let plain = crate::test_support::text_page_pdf_bytes("secret contents", 72.0, 700.0, 24.0);
        let protected = openpdfedit_crypt::encrypt_document(
            &plain,
            "hunter2",
            "hunter2",
            openpdfedit_crypt::Permissions::default(),
        )
        .expect("fixture should encrypt");

        let dir = std::env::temp_dir();
        let original = dir.join(format!(
            "openpdfedit-save-protected-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&original, &protected).expect("should write the fixture");

        let (handle, open_doc) =
            OpenDoc::open_with_working_copy_password(&original, engine, Some("hunter2"))
                .expect("should open with the password");
        assert_eq!(
            open_doc.encryption.as_deref(),
            Some("hunter2"),
            "the password wasn't remembered, so saving can't restore protection"
        );

        // The working copy itself is plaintext — that's the point.
        let working = open_doc.path.clone();
        assert!(
            !openpdfedit_crypt::is_encrypted(&std::fs::read(&working).unwrap()),
            "the working copy should be decrypted for the edit paths"
        );

        let docs: Mutex<HashMap<openpdfedit_engine::DocHandle, OpenDoc>> =
            Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(handle, open_doc);
        let history = Mutex::new(HashMap::new());
        let state = crate::SessionState {
            engine: engine.clone(),
            docs,
            history,
            store: Box::new(FsWorkingStore),
        };

        crate::save_document_impl(&state, handle).expect("save should succeed");

        let saved = std::fs::read(&original).expect("the saved file should exist");
        assert!(
            openpdfedit_crypt::is_encrypted(&saved),
            "Save wrote the document back WITHOUT its password protection"
        );
        // ...and it's still the same password, not some new one.
        assert!(
            openpdfedit_crypt::decrypt_document(&saved, "hunter2").is_ok(),
            "the re-protected file doesn't open with the original password"
        );

        engine.close(handle);
        let _ = std::fs::remove_file(&original);
        let _ = std::fs::remove_file(&working);
    }

    /// Opening a protected document with no password is a distinct
    /// signal, so the UI can prompt rather than show a parser message.
    #[test]
    fn opening_without_a_password_asks_for_one() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };
        let plain = crate::test_support::text_page_pdf_bytes("secret", 72.0, 700.0, 24.0);
        let protected = openpdfedit_crypt::encrypt_document(
            &plain,
            "pw",
            "pw",
            openpdfedit_crypt::Permissions::default(),
        )
        .expect("fixture should encrypt");
        let path = std::env::temp_dir().join(format!(
            "openpdfedit-open-noprompt-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&path, &protected).expect("should write the fixture");

        // `OpenDoc` isn't Debug (it holds a whole Document), so match on
        // the result rather than unwrapping the error out of it.
        match OpenDoc::open_with_working_copy_password(&path, engine, None) {
            Err(SessionError::PasswordRequired) => {}
            Err(other) => panic!("expected PasswordRequired, got {other:?}"),
            Ok(_) => panic!("opening a protected document with no password must fail"),
        }

        let _ = std::fs::remove_file(&path);
    }
}
