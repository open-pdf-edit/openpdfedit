//! Form-fill + field-creation commands, moved here (from
//! `apps/desktop/src-tauri/src/forms.rs` and `field_create.rs`) for the
//! same reason as [`crate::annotations`]: the same logic should drive
//! both the desktop's thread-wrapped `EngineHandle` and a bare
//! in-process engine for the wasm/Chrome-extension build.
//!
//! As of Phase 3 Task 2, every function in this module is uniformly
//! generic over `E: Engine` and ungated (no `#[cfg(not(target_arch =
//! "wasm32"))]` anywhere in this file):
//!
//! - Form **listing/filling/saving** ([`list_form_fields_impl`],
//!   [`fill_form_fields_impl`]) now genericize over `E: Engine` the same
//!   way every other command module here does. `list_form_fields`/
//!   `fill_form_fields` **are** on the [`Engine`] trait (Phase 3 Task 1;
//!   see `openpdfedit-engine`'s module doc on its `impl PdfiumEngine`
//!   AcroForm block).
//! - Field **creation** ([`create_form_field_impl`]) was already
//!   portable: plain `lopdf` object-graph work through
//!   [`crate::commit_mutation`], exactly like
//!   `annotations::add_annotation_impl`. Verified as part of this task
//!   (no change needed) — it never touched `EngineHandle`/`std::fs`
//!   directly, so it had nothing to un-fence.
//!
//! ## Why `fill_form_fields_impl`'s write path unifies rather than splits
//!
//! Before this task, `fill_form_fields_impl` wrote its working copy
//! *outside* the [`WorkingStore`] abstraction: `EngineHandle::
//! save_document` (PDFium's own `save_to_file`) to a sibling
//! `.openpdfedit-tmp` file, followed by `std::fs::rename` into place. That
//! shape was `#[cfg(not(target_arch = "wasm32"))]`-fenced because a
//! `MemWorkingStore` has no filesystem to rename into.
//!
//! The fix is not a wasm-side branch alongside the desktop's tmp+rename
//! dance — it replaces it, on every target, with the same store-routed
//! shape every other mutating command in this crate already uses:
//! `engine.fill_form_fields(...)` (mutates PDFium's in-memory document),
//! `engine.save_to_bytes(...)` (the trait's wasm32-safe equivalent of
//! `save_document`, already used by [`crate::reopen_after_write`] and
//! this crate's own `SessionState` tests), [`normalize_button_states_in_bytes`]
//! (the byte-based counterpart of the old file-based
//! `normalize_button_states_in_file` — `openpdfedit_forms::
//! normalize_button_states`'s own logic was already `lopdf::Document`-only
//! and platform-agnostic; only its load/save wrapper was file-bound), then
//! `store.write(&path, &normalized)`.
//!
//! This is not merely "acceptable" but strictly better than keeping the
//! old path split behind `#[cfg]`:
//!
//! - **The tmp+rename dance existed to solve a problem that moves, not
//!   disappears — but the place it moves to is verified benign.**
//!   `save_document`'s doc explains why it went through a sibling temp
//!   file: PDFium's `save_to_file` cannot safely write back to a path the
//!   same document is still open from — confirmed the hard way (a native
//!   trap, not a returned error) — so the tmp file plus an atomic rename
//!   sidestepped ever letting PDFium's writer and its own reader target
//!   the same path concurrently. `engine.save_to_bytes(...)` itself
//!   doesn't touch that path at all (`document.save_to_writer` into an
//!   in-memory `Cursor`, fully drained into `saved`/`normalized` before
//!   this function ever calls `store.write`) — but the filesystem contact
//!   doesn't disappear, it moves one line later: `store.write(&path,
//!   &normalized)` overwrites the very path `engine`'s handle for
//!   `request.handle` was opened from — and that handle is not closed
//!   until [`reopen_after_write`] runs, *after* this overwrite. So there
//!   is a real window where a still-open PDFium handle's backing file has
//!   just been overwritten out from under it. It's benign, not because
//!   the old hazard vanished, but because nothing in that window ever
//!   reads through the stale handle again: the bytes PDFium needed were
//!   already fully read into `saved` by `save_to_bytes` *before*
//!   `store.write` runs, and the stale handle's only remaining use is
//!   `engine.close(old_handle)` inside `reopen_after_write`, whose
//!   `PdfDocument::drop` calls `FPDF_CloseDocument` alone — no re-read of
//!   the (now different) file on disk. `pdfium-render`'s lazy
//!   `load_pdf_from_file` reads happen against the file *as it existed
//!   when first accessed*, and nothing after `save_to_bytes` triggers a
//!   fresh access. `fill_form_fields_impl_fills_saves_and_rotates_the_handle`
//!   (real PDFium, a real on-disk file, this exact fill → save_to_bytes →
//!   store.write → reopen sequence) is what actually exercises this
//!   window end-to-end, not just an argument on paper.
//! - **`FsWorkingStore::write` is itself a write-tmp-then-rename, not a
//!   bare in-place `std::fs::write`** (fix-wave re-review's I1 — see that
//!   method's own doc in `crate::FsWorkingStore` for the full mechanics).
//!   This closes a real hazard the unification would otherwise have
//!   spread to every store-routed write at once, `fill_form_fields_impl`
//!   included: a failed in-place write (crash, disk full, power loss
//!   mid-`fs::write`, which truncates the destination before writing a
//!   single new byte) used to be able to leave the working copy shorter
//!   than either its old or new complete contents — and
//!   `save_document_impl` copies the working copy's bytes *as they sit on
//!   disk* over the user's real file, with no way to notice a truncated
//!   copy versus a complete one. The very next Save would then have
//!   copied that truncated garbage straight over the user's document.
//!   Write-tmp-then-rename makes every `store.write` call in this crate
//!   atomic *per write* with respect to that failure mode — the working
//!   copy at any given key is always either fully the old bytes or fully
//!   the new bytes after any single write, never a partial write,
//!   regardless of when a crash or error strikes mid-write. That's a
//!   narrower claim than "safe under concurrent writers to the same key,"
//!   and this crate used to have a real UI-reachable path where two
//!   writers (Fill and Undo/Redo) could race on the same document with no
//!   ordering between their writes at all — closed by Phase 4 Task 1:
//!   [`fill_form_fields_impl`] now holds the `docs` lock across its whole
//!   store-read/write + history-commit sequence, the same discipline
//!   [`crate::commit_mutation`] already used for every other write path in
//!   this crate; see `FsWorkingStore::write`'s own doc for the full
//!   concurrent-writer story and `crate::undo_impl`'s doc for the
//!   deadlock-freedom argument that fix relies on (identical here).
//! - A single portable code path is simpler to read, test, and maintain
//!   than a desktop-only fork that has to keep re-justifying its own
//!   existence — and per the two points above, both hazards the fork used
//!   to sidestep (the still-open-handle window, and now the in-place-write
//!   truncation risk) are, in the shape this task landed, verified benign
//!   or closed outright rather than merely avoided, so the fork buys
//!   nothing a single path doesn't already cover.
//!
//! `Err = SessionError` throughout (like [`crate::annotations`]) — this
//! is the moved code's own home now, not a shared helper other error
//! types also need to drive. [`SessionError`] needed no new variant: the
//! existing `Doc` variant already covers `openpdfedit_forms::FormsError`
//! and the raw-`lopdf` re-parse in [`normalize_button_states_in_bytes`],
//! exactly as the desktop's own `CommandError::Doc` did before the move.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine, FormField, FormFieldKind, FormFieldOption};
use openpdfedit_forms::{NewField, NewFieldKind};
use serde::{Deserialize, Serialize};

use crate::{
    capture_pre_edit_snapshot, commit_mutation, commit_undo_snapshot, reopen_after_write,
    resolve_doc, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError, WorkingStore,
};

impl From<openpdfedit_forms::FormsError> for SessionError {
    fn from(e: openpdfedit_forms::FormsError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FormFieldKindDto {
    Text,
    Checkbox,
    RadioButton,
    ComboBox,
    ListBox,
    PushButton,
    Signature,
    Unknown,
}

impl From<FormFieldKind> for FormFieldKindDto {
    fn from(kind: FormFieldKind) -> Self {
        match kind {
            FormFieldKind::Text => FormFieldKindDto::Text,
            FormFieldKind::Checkbox => FormFieldKindDto::Checkbox,
            FormFieldKind::RadioButton => FormFieldKindDto::RadioButton,
            FormFieldKind::ComboBox => FormFieldKindDto::ComboBox,
            FormFieldKind::ListBox => FormFieldKindDto::ListBox,
            FormFieldKind::PushButton => FormFieldKindDto::PushButton,
            FormFieldKind::Signature => FormFieldKindDto::Signature,
            FormFieldKind::Unknown => FormFieldKindDto::Unknown,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormFieldOptionDto {
    label: Option<String>,
    is_selected: bool,
}

impl From<FormFieldOption> for FormFieldOptionDto {
    fn from(o: FormFieldOption) -> Self {
        FormFieldOptionDto {
            label: o.label,
            is_selected: o.is_selected,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormFieldDto {
    page_index: u32,
    name: String,
    kind: FormFieldKindDto,
    /// `[x0, y0, x1, y1]` on the page, in PDF points — lets the viewer
    /// put an editable box over the field instead of sending the user to
    /// a side panel to type into a list.
    rect: [f32; 4],
    value: Option<String>,
    is_checked: Option<bool>,
    is_read_only: bool,
    options: Vec<FormFieldOptionDto>,
}

impl From<FormField> for FormFieldDto {
    fn from(f: FormField) -> Self {
        FormFieldDto {
            page_index: f.page_index,
            name: f.name,
            kind: f.kind.into(),
            rect: f.rect,
            value: f.value,
            is_checked: f.is_checked,
            is_read_only: f.is_read_only,
            options: f.options.into_iter().map(Into::into).collect(),
        }
    }
}

/// The actual logic behind the desktop's `list_form_fields_cmd`.
pub fn list_form_fields_impl<E: Engine>(
    engine: &E,
    handle: DocHandle,
) -> Result<Vec<FormFieldDto>, SessionError> {
    Ok(engine
        .list_form_fields(handle)?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillFormRequest {
    pub handle: DocHandle,
    pub values: HashMap<String, String>,
}

/// The actual logic behind the desktop's `fill_form_fields_cmd`.
///
/// Unlike `openpdfedit-doc`'s lopdf-based incremental save (the write
/// path every other mutating command here goes through), filling goes
/// through PDFium's own form model (`Engine::fill_form_fields`/
/// `Engine::save_to_bytes`) — see `openpdfedit-engine`'s form-fill module
/// doc for why that's a deliberately separate write path. Ends the same
/// way every other mutating command does: [`reopen_after_write`] closes
/// the now-stale render handle and reopens a fresh one against the
/// just-written working copy, so the caller always renders through a
/// handle whose cache is guaranteed live.
///
/// Portable (`E: Engine`, ungated) as of Phase 3 Task 2 — see this
/// module's doc comment ("Why `fill_form_fields_impl`'s write path
/// unifies rather than splits") for why routing the write through
/// `store.write` rather than `EngineHandle::save_document` + rename is
/// not just wasm-compatible but strictly safer than the code it replaces.
///
/// **Per-key write serialization (Phase 4 Task 1) — and the lost-update
/// window it does NOT close for this function specifically:** the `docs`
/// lock is held across the store read/write + history-commit sequence
/// below — see [`crate::undo_impl`]'s doc for the full argument (why
/// `docs` rather than a per-document lock map, and the deadlock-freedom
/// trace against [`reopen_after_write`], which applies here unchanged:
/// this function's `docs_guard` is dropped when its `let path = { ... };`
/// block ends, strictly before `reopen_after_write` — which itself needs
/// `docs.lock()` — is ever called). What that serializes is real: no two
/// `store.write` calls for the same `path` can ever overlap, and the
/// `pre_edit_snapshot`/`commit_undo_snapshot` pair inside the lock stays
/// consistent with whichever write actually lands.
///
/// What it does **not** serialize, and what an earlier version of this
/// doc got wrong by claiming there was "nothing to serialize" here: this
/// function's actual *payload* — `engine.fill_form_fields` +
/// `engine.save_to_bytes` + `normalize_button_states_in_bytes`, i.e. the
/// bytes `store.write` below actually writes — is computed *before* the
/// write-critical-section lock is acquired, from whatever state PDFium's
/// live handle happens to be in at that moment. [`commit_mutation`]
/// doesn't have this gap: its `mutate` closure and `save_incremental()`
/// call both run *inside* the lock, right after
/// `capture_pre_edit_snapshot`, so the bytes it writes are always derived
/// from the version of the document the lock itself just made current.
/// This function's `normalized` bytes carry no such guarantee — they were
/// finalized against a snapshot in time that predates ever asking for the
/// lock.
///
/// Concretely, the lost update: suppose a concurrent [`commit_mutation`]
/// call (say, `redact::redact_page_impl` racing a redaction against the
/// same document) acquires `docs`, does its own complete read-mutate-
/// write, and releases it — all while this function's
/// `engine.fill_form_fields`/`save_to_bytes`/`normalize_button_states_in_bytes`
/// sequence above is still running, unlocked. When this function then
/// acquires the lock and calls `store.write(&path, &normalized)`, that
/// write unconditionally overwrites the redaction's already-committed
/// result with `normalized` — bytes computed before the redaction ever
/// happened. The store, `docs`, and the undo/redo history all end up
/// self-consistent (nothing here corrupts), but the racing writer's edit
/// is silently gone: the redacted content reappears with no error, no
/// conflict signal, and no trace in the undo stack of the fact that it
/// was ever there. This is the same *shape* of loss `FsWorkingStore::write`'s
/// doc calls "last-writer-wins," except Phase 4 Task 1 doesn't touch it
/// here — that fix serializes *writes*, and this window is a stale *read*
/// wearing a write's timing.
///
/// The structural fix is the same one already named for the double-
/// rotation residual on [`crate::undo_impl`]'s doc and [`FsWorkingStore::write`]'s:
/// a per-key lock (distinct from `docs`, so — unlike `docs` — it can be
/// held across an entire operation) acquired *before* `engine.fill_form_fields`
/// runs and released only after this function's `store.write` returns,
/// so payload computation and the write it feeds are atomic together
/// against any other writer on the same key. Out of scope for this task
/// for the same reasons `undo_impl`'s doc gives (a new `SessionState`
/// field and a new parameter threaded through every write path, to fix a
/// distinct problem from the one this task was scoped to) — tracked
/// alongside the double-rotation residual in the Phase 4 ledger
/// (`.superpowers/sdd/2026-08-16-extension-port-phase4/progress.md`) as a
/// sibling entry, same candidate fix.
///
/// (There *is* a separate, brief `docs.lock()` even earlier than that —
/// fix-wave re-review's M2, a fail-fast unknown-handle check ahead of
/// the expensive `save_to_bytes`/normalize calls, released immediately
/// and replayed for real inside the write critical section below. It's
/// a perf short-circuit, not part of this discipline, and doesn't
/// change any of the above: the payload is still computed unlocked.)
pub fn fill_form_fields_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: FillFormRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    engine.fill_form_fields(request.handle, request.values)?;

    // Fail fast on an unknown handle before doing a full `save_to_bytes`
    // (serializes the whole document) + button-state re-parse that would
    // just be thrown away (fix-wave re-review's M2). This lookup is
    // deliberately released immediately, not held across the real
    // critical section below — it's a perf short-circuit ahead of two
    // potentially expensive calls, not part of the write-ordering
    // discipline itself. If `request.handle` is removed by a concurrent
    // rotation between this check and the real one below, the real check
    // re-derives the identical `UnknownHandle` error — no correctness
    // gap, just a redundant lookup in that rare race. Behavior for a
    // *known* handle is unchanged either way: same calls, same order
    // relative to each other, just with this cheap existence check
    // ahead of the two expensive ones instead of behind them.
    if !docs
        .lock()
        .expect("docs lock poisoned")
        .contains_key(&request.handle)
    {
        return Err(SessionError::UnknownHandle(request.handle));
    }

    let saved = engine.save_to_bytes(request.handle)?;
    // PDFium's writer leaves ticked checkboxes with a *string* `/AS`
    // where a *name* is required, so they render as still-unticked. Fix
    // that in the bytes it just produced, before they're written to the
    // working copy — see `openpdfedit_forms::normalize_button_states`.
    let normalized = normalize_button_states_in_bytes(&saved)?;

    let path = {
        let docs_guard = docs.lock().expect("docs lock poisoned");
        let path = resolve_doc(&docs_guard, request.handle)?.path.clone();

        let pre_edit_snapshot = capture_pre_edit_snapshot::<SessionError>(store, &path)?;
        store.write(&path, &normalized)?;
        commit_undo_snapshot(history, &path, pre_edit_snapshot);
        path
    };

    reopen_after_write(engine, docs, history, store, request.handle, path)
}

/// Parses `bytes`, repairs any string-valued button `/AS`/`/V` entries,
/// and returns rewritten bytes only when something actually changed —
/// otherwise hands back the original buffer unmodified, so the
/// overwhelmingly common no-checkbox case costs a parse and nothing
/// else. Bytes PDFium just wrote (via `Engine::save_to_bytes`) should
/// always reparse cleanly; if they somehow don't, that is a real error
/// worth surfacing rather than silently shipping a document whose
/// checkboxes don't render.
///
/// The byte-based counterpart of what was `normalize_button_states_in_file`
/// before Phase 3 Task 2 — `openpdfedit_forms::normalize_button_states`'s
/// own logic already operated on a parsed `lopdf::Document` and never
/// touched the filesystem itself; only its load-from-path/save-to-path
/// wrapper was platform-bound, so converting that wrapper to
/// load-from-bytes/save-to-bytes (`lopdf::Document::load_mem`/`save_to`,
/// both already used elsewhere in this workspace — see
/// `openpdfedit-doc::Document::from_bytes` and this module's own test
/// fixtures) is the entire change needed to make this portable.
fn normalize_button_states_in_bytes(bytes: &[u8]) -> Result<Vec<u8>, SessionError> {
    let mut doc = lopdf::Document::load_mem(bytes)
        .map_err(|e| SessionError::Doc(format!("re-reading the saved form failed: {e}")))?;
    if openpdfedit_forms::normalize_button_states(&mut doc) == 0 {
        return Ok(bytes.to_vec());
    }
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| SessionError::Doc(format!("rewriting the saved form failed: {e}")))?;
    Ok(out)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewFieldKindDto {
    Text,
    Checkbox,
}

impl From<NewFieldKindDto> for NewFieldKind {
    fn from(k: NewFieldKindDto) -> Self {
        match k {
            NewFieldKindDto::Text => NewFieldKind::Text,
            NewFieldKindDto::Checkbox => NewFieldKind::Checkbox,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFormFieldRequest {
    pub handle: DocHandle,
    pub page_index: u32,
    /// `[x0, y0, x1, y1]` in PDF page-space points.
    pub rect: [f32; 4],
    pub kind: NewFieldKindDto,
    pub name: String,
}

/// The actual logic behind the desktop's `create_form_field_cmd`. Plain
/// `lopdf` object-graph work via [`crate::commit_mutation`], unlike
/// [`fill_form_fields_impl`] above — see this module's doc comment.
pub fn create_form_field_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    request: CreateFormFieldRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, request.handle, |doc| {
        openpdfedit_forms::create_field(
            doc,
            NewField {
                page_index: request.page_index,
                name: request.name.clone(),
                rect: request.rect,
                kind: request.kind.into(),
            },
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Delete a text field, draw another, and the new one must be new.
    ///
    /// A field's identity is its name and its value lives on the
    /// document's `/AcroForm`/`/Fields` entry — not on the page. Deleting
    /// the widget used to unlink it from `/Annots` only, leaving that
    /// entry behind holding its value. The next field the UI drew got the
    /// same auto-generated name, and by the spec two widgets sharing a
    /// name are *one field*: the new box appeared already containing the
    /// old text, and neither could be edited away from the other.
    ///
    /// Reported as "no matter what I enter, it always goes back to test",
    /// with the Form fields panel equally unable to change it — both
    /// symptoms of the same shared field, so both are asserted here.
    #[test]
    fn deleting_a_field_and_drawing_another_does_not_resurrect_the_old_value() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let path = std::env::temp_dir().join(format!(
            "openpdfedit-refield-{}-{}.pdf",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, crate::test_support::minimal_pdf_bytes()).expect("write fixture");

        let state = crate::SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(crate::FsWorkingStore),
        };
        let mut handle = crate::open_document_impl(&state, &path)
            .expect("should open")
            .handle;

        let values_now = |h: DocHandle| -> Vec<(String, Option<String>)> {
            state
                .engine
                .list_form_fields(h)
                .expect("should list")
                .into_iter()
                .map(|f| (f.name, f.value))
                .collect()
        };
        let add_field = |h: DocHandle, top: f32| {
            create_form_field_impl(
                &state.engine,
                &state.docs,
                &state.history,
                &*state.store,
                CreateFormFieldRequest {
                    handle: h,
                    page_index: 0,
                    name: "text_1".to_string(),
                    rect: [72.0, top, 272.0, top + 24.0],
                    kind: NewFieldKindDto::Text,
                },
            )
            .expect("should create")
            .handle
        };
        let fill = |h: DocHandle, value: &str| {
            fill_form_fields_impl(
                &state.engine,
                &state.docs,
                &state.history,
                &*state.store,
                FillFormRequest {
                    handle: h,
                    values: [("text_1".to_string(), value.to_string())]
                        .into_iter()
                        .collect(),
                },
            )
            .expect("should fill")
            .handle
        };

        handle = add_field(handle, 700.0);
        handle = fill(handle, "test");
        assert_eq!(
            values_now(handle),
            [("text_1".to_string(), Some("test".to_string()))]
        );

        let annotations = crate::annotations::list_page_annotations_impl(&state.docs, handle, 0)
            .expect("should list annotations");
        assert_eq!(
            annotations.len(),
            1,
            "the field's widget should be the only annotation"
        );
        handle = crate::annotations::delete_annotation_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            crate::annotations::DeleteAnnotationRequest {
                handle,
                page_index: 0,
                annotation_id: annotations[0].id,
            },
        )
        .expect("should delete")
        .handle;
        assert!(
            values_now(handle).is_empty(),
            "the deleted field should be gone"
        );

        // The same name again — which is what the UI generates, since it
        // is once more the first one free.
        handle = add_field(handle, 600.0);
        assert_eq!(
            values_now(handle),
            [("text_1".to_string(), None)],
            "a newly drawn field must be empty, not holding the deleted field's value",
        );

        // And it must be editable on its own terms.
        handle = fill(handle, "second");
        assert_eq!(
            values_now(handle),
            [("text_1".to_string(), Some("second".to_string()))],
            "filling the new field must take effect rather than being shadowed",
        );

        let _ = std::fs::remove_file(&path);
    }

    use crate::test_support::shared_handle;
    use crate::FsWorkingStore;
    use openpdfedit_doc::Document;

    /// A minimal one-page AcroForm PDF with a Text field, a Checkbox
    /// (pre-set `/AS /Off` so `checkbox.rs`'s "appearance streams in use"
    /// branch is exercised — see the research notes in this session for
    /// why that's the reliable branch to test against), and a two-option
    /// RadioButton group, hand-built via lopdf the same way every other
    /// test PDF in this codebase is. No real `/AP` appearance XObjects:
    /// `set_value`/`set_checked` write `/V`/`/AS` directly on the
    /// annotation dict and are read back the same way, independent of
    /// whether a visual appearance stream exists.
    fn acroform_pdf_bytes() -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let content = Content {
            operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        let text_field_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("full_name"),
            "Rect" => vec![50.into(), 700.into(), 250.into(), 720.into()],
            "V" => Object::string_literal(""),
            "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
        });

        let checkbox_field_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Btn",
            "T" => Object::string_literal("agree"),
            "Rect" => vec![50.into(), 650.into(), 65.into(), 665.into()],
            "V" => Object::Name(b"Off".to_vec()),
            "AS" => Object::Name(b"Off".to_vec()),
        });

        // A real radio *group* is one non-terminal parent field (`/T`,
        // `/Ff` Radio flag, `/V` the shared selection, `/Kids`) plus one
        // terminal widget annotation per option, each `/Parent`-linked
        // back to it and carrying only its own `/AS` export name — not,
        // as a first draft of this fixture mistakenly had it, two
        // independent top-level fields that merely happen to share `/T`
        // (which PDFium does *not* treat as a shared group: each field's
        // `/V` stayed private, so selecting one never toggled the other).
        let radio_parent_id = doc.new_object_id();

        let radio_a_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Parent" => radio_parent_id,
            "Rect" => vec![50.into(), 600.into(), 65.into(), 615.into()],
            "AS" => Object::Name(b"Basic".to_vec()),
        });
        let radio_b_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Parent" => radio_parent_id,
            "Rect" => vec![80.into(), 600.into(), 95.into(), 615.into()],
            "AS" => Object::Name(b"Pro".to_vec()),
        });
        doc.objects.insert(
            radio_parent_id,
            Object::Dictionary(dictionary! {
                "FT" => "Btn",
                "Ff" => 1 << 15, // Radio flag
                "T" => Object::string_literal("plan"),
                "V" => Object::Name(b"Basic".to_vec()),
                "Kids" => vec![radio_a_id.into(), radio_b_id.into()],
            }),
        );

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
            "Annots" => vec![
                text_field_id.into(),
                checkbox_field_id.into(),
                radio_a_id.into(),
                radio_b_id.into(),
            ],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![
                text_field_id.into(),
                checkbox_field_id.into(),
                Object::Reference(radio_parent_id),
            ],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn minimal_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Object};

        let mut doc = lopdf::Document::with_version("1.5");
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
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    /// End-to-end: real file on disk, real `EngineHandle` (PDFium), real
    /// `fill_form_fields_impl`. Fills the text field and checks the
    /// checkbox — both verified to round-trip through a fresh handle
    /// opened against the saved file (not just the in-memory state before
    /// save). Also fills the "plan" radio group's "Pro" option, but only
    /// asserts that doing so doesn't error — see
    /// `PdfiumEngine::fill_form_fields`'s doc for the confirmed
    /// reliability gap in reading back a radio group's selected state
    /// through this API, discovered while writing this very test (the
    /// selection did not read back as selected, verified independently
    /// via a raw structural dump with a different PDF library — not a
    /// mistake in this test, a real limitation of the dependency).
    #[test]
    fn fill_form_fields_impl_fills_saves_and_rotates_the_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-fill-form-impl-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, acroform_pdf_bytes()).expect("should write temp file");

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

        // Sanity check the fixture before mutating it: nothing set yet.
        let before = engine
            .list_form_fields(handle)
            .expect("list should succeed");
        assert_eq!(before.len(), 4, "text + checkbox + 2 radio widgets");
        let checkbox_before = before.iter().find(|f| f.name == "agree").unwrap();
        assert_eq!(checkbox_before.is_checked, Some(false));

        let mut values = HashMap::new();
        values.insert("full_name".to_string(), "Ada Lovelace".to_string());
        values.insert("agree".to_string(), "true".to_string());
        values.insert("plan".to_string(), "Pro".to_string());

        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let request = FillFormRequest { handle, values };
        let result = fill_form_fields_impl(engine, &docs, &history, &FsWorkingStore, request)
            .expect("fill_form_fields_impl should succeed");

        // A real no-tmp-left-behind guard against `FsWorkingStore::write`'s
        // own write-tmp-then-rename (fix-wave re-review's I1/M2 — see that
        // method's doc): before I1, this assertion was vacuous —
        // `fill_form_fields_impl` called `store.write` directly, which was
        // a bare `std::fs::write` with no tmp file involved at all, so
        // `.openpdfedit-tmp*` could never have existed regardless of
        // whether the write path was correct. Now that `store.write`
        // itself does the tmp+rename dance, this is the real assertion
        // that a successful write actually renamed its tmp sibling away
        // rather than, say, leaving it orphaned by a rename that silently
        // no-opped. Checks via `any_tmp_sibling_exists` (a prefix scan),
        // not one hardcoded `.openpdfedit-tmp` suffix — the second re-review
        // found the tmp suffix now carries a per-write-call counter
        // (`{key}.openpdfedit-tmp-{n}` via `NEXT_TMP_WRITE_ID`, not a fixed
        // name — see `FsWorkingStore::write`'s doc for the concurrent-
        // writers reason), so a hardcoded-suffix check would have gone
        // vacuous again the moment that landed.
        assert!(!crate::test_support::any_tmp_sibling_exists(&tmp_path));

        assert_ne!(
            result.handle, handle,
            "a successful fill+save must rotate to a fresh engine handle"
        );

        // The old handle must no longer resolve.
        assert!(engine.page_count(handle).is_err());

        let after = engine
            .list_form_fields(result.handle)
            .expect("list on the new handle should succeed");
        assert_eq!(
            after.len(),
            4,
            "text + checkbox + 2 radio widgets, unchanged"
        );

        let text = after.iter().find(|f| f.name == "full_name").unwrap();
        assert_eq!(text.value.as_deref(), Some("Ada Lovelace"));

        let checkbox = after.iter().find(|f| f.name == "agree").unwrap();
        assert_eq!(checkbox.is_checked, Some(true));

        // Both radio widgets must still be present and still correctly
        // typed/named after the fill+save+reopen cycle — that much this
        // API does get right. Their `is_checked` state is deliberately
        // NOT asserted here; see this test's doc comment.
        let radio_widgets: Vec<_> = after.iter().filter(|f| f.name == "plan").collect();
        assert_eq!(radio_widgets.len(), 2);
        assert!(radio_widgets
            .iter()
            .all(|f| f.kind == openpdfedit_engine::FormFieldKind::RadioButton));

        // And the new render handle actually renders (proves the saved
        // file is structurally valid, not just that the JSON looks right).
        let tile = engine
            .render_page(result.handle, 0, 100)
            .expect("new handle should render");
        assert!(tile.height > 0);

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn fill_form_fields_impl_rejects_unknown_field_name() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-fill-form-unknown-field-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, acroform_pdf_bytes()).expect("should write temp file");

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

        let mut values = HashMap::new();
        values.insert("does_not_exist".to_string(), "x".to_string());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let request = FillFormRequest { handle, values };

        let result = fill_form_fields_impl(engine, &docs, &history, &FsWorkingStore, request);
        let Err(err) = result else {
            panic!("an unknown field name must be rejected");
        };
        assert!(format!("{err}").contains("does_not_exist"));

        // Rejected requests must not rotate the handle — nothing was
        // written, so the original handle is still the live one.
        assert!(engine.page_count(handle).is_ok());

        engine.close(handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// The task's Step-1 test: open a fixture with a form field, list
    /// fields, fill one, list reflects the value — but driven through
    /// [`list_form_fields_impl`] specifically (the thin listing wrapper,
    /// as distinct from the fill round trip the two tests above already
    /// cover end-to-end), confirming it too works against a real engine
    /// handle now that it lives in this crate.
    #[test]
    fn list_form_fields_impl_reflects_a_filled_value() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-list-form-fields-impl-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, acroform_pdf_bytes()).expect("should write temp file");

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

        let before = list_form_fields_impl(engine, handle).expect("list should succeed");
        let text_before = before.iter().find(|f| f.name == "full_name").unwrap();
        // Not `Some("")`, even though `acroform_pdf_bytes()` gives this
        // widget an explicit `"V" => Object::string_literal("")` — this
        // was this test's original (wrong) expectation, caught by a fix
        // round that finally ran this suite against a real PDFium build
        // (a stub `.vendor/pdfium/lib/libpdfium.dylib` had silently
        // early-returned/skipped this test, and every other real-PDFium
        // test in this crate, since some point after the Phase 1 forms
        // move). `PdfFormTextField::value()` (pdfium-render 0.9.3,
        // `field/private.rs`'s `value_impl`) calls
        // `FPDFAnnot_GetFormFieldValue` and treats a *zero-length* result
        // as "the form value is not set" -> `None`, by that function's own
        // doc comment — PDFium does not distinguish "key absent" from
        // "key present with an empty string" here, confirmed both by
        // reading `value_impl`'s source and by this test actually
        // observing `None` (not `Some("")`) against the real library.
        // `form_field_to_dto`'s `PdfFormField::Text(f) => (f.value(), None)`
        // is therefore correct as-is; this assertion was the actual bug.
        assert_eq!(text_before.value.as_deref(), None);

        let mut values = HashMap::new();
        values.insert("full_name".to_string(), "Grace Hopper".to_string());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let result = fill_form_fields_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            FillFormRequest { handle, values },
        )
        .expect("fill should succeed");

        let after = list_form_fields_impl(engine, result.handle).expect("list should succeed");
        let text_after = after.iter().find(|f| f.name == "full_name").unwrap();
        assert_eq!(
            text_after.value.as_deref(),
            Some("Grace Hopper"),
            "list_form_fields_impl must reflect the just-filled value"
        );

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// Phase 3 Task 2's dedicated portable test: the same fill/list round
    /// trip as [`fill_form_fields_impl_fills_saves_and_rotates_the_handle`]/
    /// [`list_form_fields_impl_reflects_a_filled_value`] above, but driven
    /// entirely through [`crate::SessionState`] + [`crate::MemWorkingStore`]
    /// — no temp files, no `std::fs` call anywhere in this test — and
    /// extended through an undo/redo cycle, mirroring
    /// `open_bytes_then_commit_mutation_then_undo_then_redo_all_through_mem_working_store`
    /// in `lib.rs`. Proves `fill_form_fields_impl`'s store-routed write
    /// path (this task's whole point) genuinely works without a
    /// filesystem, not just that it compiles for wasm32.
    #[test]
    fn open_bytes_then_fill_then_list_then_undo_then_redo_all_through_mem_working_store() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = crate::SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(crate::MemWorkingStore::default()),
        };

        let opened = crate::open_document_bytes(&state, "mem-forms-test.pdf", acroform_pdf_bytes())
            .expect("open_document_bytes should succeed");
        assert!(
            !opened.is_dirty,
            "a freshly-opened document must start clean"
        );

        let before =
            list_form_fields_impl(&state.engine, opened.handle).expect("list should succeed");
        let text_before = before.iter().find(|f| f.name == "full_name").unwrap();
        assert_eq!(text_before.value.as_deref(), None);

        let mut values = HashMap::new();
        values.insert("full_name".to_string(), "Ada Lovelace".to_string());
        let after_fill = fill_form_fields_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            FillFormRequest {
                handle: opened.handle,
                values,
            },
        )
        .expect("fill_form_fields_impl through MemWorkingStore should succeed");
        assert!(after_fill.can_undo);
        assert!(!after_fill.can_redo);
        assert!(
            after_fill.is_dirty,
            "a fill must dirty the working copy, even for a byte-opened document"
        );

        let after_fill_fields =
            list_form_fields_impl(&state.engine, after_fill.handle).expect("list should succeed");
        let text_after_fill = after_fill_fields
            .iter()
            .find(|f| f.name == "full_name")
            .unwrap();
        assert_eq!(
            text_after_fill.value.as_deref(),
            Some("Ada Lovelace"),
            "list_form_fields_impl must reflect the just-filled value, through MemWorkingStore"
        );

        // Undo: the field goes back to unfilled.
        let after_undo = crate::undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_fill.handle,
        )
        .expect("undo_impl through MemWorkingStore should succeed");
        assert!(!after_undo.can_undo);
        assert!(after_undo.can_redo);

        let after_undo_fields =
            list_form_fields_impl(&state.engine, after_undo.handle).expect("list should succeed");
        let text_after_undo = after_undo_fields
            .iter()
            .find(|f| f.name == "full_name")
            .unwrap();
        assert_eq!(
            text_after_undo.value.as_deref(),
            None,
            "undo must restore the pre-fill (unfilled) value"
        );

        // Redo: the fill is reapplied.
        let after_redo = crate::redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo.handle,
        )
        .expect("redo_impl through MemWorkingStore should succeed");
        assert!(after_redo.can_undo);
        assert!(!after_redo.can_redo);

        let after_redo_fields =
            list_form_fields_impl(&state.engine, after_redo.handle).expect("list should succeed");
        let text_after_redo = after_redo_fields
            .iter()
            .find(|f| f.name == "full_name")
            .unwrap();
        assert_eq!(
            text_after_redo.value.as_deref(),
            Some("Ada Lovelace"),
            "redo must reapply the filled value"
        );

        state.engine.close(after_redo.handle);
    }

    #[test]
    fn create_form_field_impl_creates_saves_and_rotates_the_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-field-create-impl-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, minimal_pdf_bytes()).expect("should write temp file");

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
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        let request = CreateFormFieldRequest {
            handle,
            page_index: 0,
            rect: [50.0, 700.0, 250.0, 720.0],
            kind: NewFieldKindDto::Text,
            name: "full_name".to_string(),
        };
        let result = create_form_field_impl(engine, &docs, &history, &FsWorkingStore, request)
            .expect("create_form_field_impl should succeed");

        assert_ne!(
            result.handle, handle,
            "a successful create+save must rotate the handle"
        );
        assert!(
            engine.page_count(handle).is_err(),
            "old handle must be closed"
        );

        let fields = engine
            .list_form_fields(result.handle)
            .expect("list_form_fields should succeed");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "full_name");

        engine.close(result.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }
}
