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
        SessionError::Doc(e.to_string())
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
