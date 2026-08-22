//! Engine-generic document session core for openpdfedit, extracted from
//! the desktop app's Tauri command layer (`apps/desktop/src-tauri/src/lib.rs`)
//! so the same open-documents/undo-redo/open-save-save-as logic — and
//! every mutating command module — can drive both the desktop's
//! thread-wrapped `EngineHandle` and a bare in-process engine for the
//! wasm/Chrome-extension build. Everything here is generic over
//! `E: openpdfedit_engine::Engine` (or takes `&dyn Engine`) — see that
//! trait for the read/render surface every engine implementation exposes
//! identically. **This crate must not depend on `tauri`.**
//!
//! ## Inventory: what lives here vs. what stays in the desktop shell
//!
//! This crate owns:
//!
//! - [`OpenDoc`] (the write-side per-document state: scratch working
//!   copy, original path, dirty flag, editable [`Document`]) and its
//!   `open_with_working_copy` constructor — path/filesystem-based, so
//!   `#[cfg(not(target_arch = "wasm32"))]`.
//! - [`WorkingStore`] — where a document's working-copy bytes and
//!   undo/redo snapshots actually live, abstracted so
//!   [`commit_mutation`]/[`undo_impl`]/[`redo_impl`]/[`reopen_after_write`]
//!   don't have to know whether they're running on the desktop (a real
//!   scratch file, [`FsWorkingStore`]) or in a browser extension (no
//!   filesystem at all, [`MemWorkingStore`]). [`FsWorkingStore`] is
//!   `#[cfg(not(target_arch = "wasm32"))]`; the trait and
//!   [`MemWorkingStore`] are portable/ungated. See [`SessionState::store`]
//!   for which one each build constructs, and this module's "The
//!   `WorkingStore` abstraction" section below for the reopen-by-bytes
//!   equivalence argument.
//! - [`DocHistory`] (undo/redo byte-snapshot stacks) and
//!   `MAX_HISTORY_DEPTH`.
//! - [`SessionState`] — the engine, the open-docs map, the undo/redo
//!   history map, and the [`WorkingStore`] (the desktop's `AppState` is a
//!   type alias to `SessionState<EngineHandle>`, so every
//!   `state.engine`/`state.docs`/`state.history`/`state.store` field
//!   access in the desktop crate compiles against this type unchanged).
//! - [`SessionError`] — this crate's own leaner error type than the
//!   desktop's `CommandError` (no `Ocr` variant, since this crate never
//!   touches that feature crate; it does carry `Annot`, since
//!   [`annotations`] lives here). The desktop shell converts via
//!   `From<SessionError> for CommandError`.
//! - [`PageSize`]/[`OpenedDocumentInfo`] — the DTOs every open/save/
//!   undo/redo/mutating command hands back.
//! - `opened_document`, `reopen_after_write`, `commit_undo_snapshot`,
//!   `undo_impl`, `redo_impl`, [`capture_pre_edit_snapshot`],
//!   [`commit_mutation`] — the shared open/save/undo/redo and
//!   mutate-save-rotate-the-render-handle plumbing every editing command
//!   drives. All fully portable now: every working-copy/snapshot read or
//!   write goes through the `store: &dyn WorkingStore` parameter each of
//!   these takes, rather than calling `std::fs` directly.
//! - `open_document_impl`, `save_document_impl`, `save_document_as_impl`
//!   — path-based, `#[cfg(not(target_arch = "wasm32"))]`, taking
//!   `&SessionState<E>` bundled rather than three separate params.
//! - [`open_document_bytes`] — the wasm-safe counterpart of
//!   `open_document_impl`: sources the document from
//!   `Engine::open_bytes`/`Document::from_bytes` instead of a filesystem
//!   path, keyed in `docs`/`history` by a unique
//!   `format!("{display_name}#{n}")` working key (`original_path` stays
//!   the bare `display_name` — see [`NEXT_BYTES_WORKING_ID`]'s doc for why
//!   the working key can't be the bare name too). Portable (no
//!   `std::fs`), ungated.
//! - [`mark_saved`] — the wasm-safe counterpart of what
//!   `save_document_impl`/`save_document_as_impl` do inline for the
//!   desktop's path-based flow (`d.dirty = false`, once the working copy
//!   is actually durably written): flips a byte-opened document's dirty
//!   bit back to `false` after its caller (`WasmSession::mark_saved`,
//!   driven by `wasm.ts`) has confirmed its own out-of-crate write (a
//!   `FileSystemFileHandle` write, not anything this crate does) actually
//!   succeeded. Portable, ungated — see its own doc for the caller
//!   contract.
//! - [`close_document_impl`] — the shared close path both the desktop's
//!   `close_document` Tauri command and `WasmSession::close_document`
//!   drive: closes the engine handle, drops the `docs` entry, and (new as
//!   of Phase 2's final-review fix wave) cleans up that document's
//!   [`WorkingStore`]/[`DocHistory`] entries too — see its own doc for the
//!   collision/leak bug this closes.
//! - Every mutating command module: [`annotations`] (`add_annotation`,
//!   `list_page_annotations`, `delete_annotation`, `text_selection_quads`),
//!   [`forms`] (incl. field creation — `list_form_fields`,
//!   `fill_form_fields`, `create_form_field`), [`pages`] (`rotate_page`,
//!   `delete_page`, `set_crop_box`, `move_page`, `merge_documents`,
//!   `extract_pages`), [`textedit`], [`redact`], [`signatures`],
//!   [`compare`]. Each single-document mutation genericizes over
//!   `E: Engine` through [`commit_mutation`]; a few modules don't fit
//!   that shape exactly:
//!     - `forms`'s field listing/filling/saving is fixed to the concrete
//!       `EngineHandle`, not generic — that surface is deliberately not
//!       part of the `Engine` trait at all (see `openpdfedit-engine`'s
//!       module doc).
//!     - `textedit`'s two listing commands need no engine at all (they
//!       read only the already-open [`Document`]) and so carry no
//!       `E: Engine` bound whatsoever.
//!     - `pages`'s merge/extract and `compare` each split into a
//!       wasm-clean byte-level core (`pages::merge_bytes`/
//!       `pages::extract_pages_bytes`, `compare::compare_bytes` — both
//!       ungated) plus a path-based, real-file-I/O orchestration layer
//!       around it (`pages::merge_documents_impl`/
//!       `pages::extract_pages_impl`, `compare::compare_documents_impl` —
//!       both `#[cfg(not(target_arch = "wasm32"))]`, since both write
//!       their result to an arbitrary caller-supplied output path outside
//!       the `WorkingStore` abstraction). `signatures` has the same
//!       byte-level-core-plus-path-based-orchestration split
//!       (`signatures::list_signatures_in_bytes` /
//!       `signatures::list_signatures_impl`), but as of Phase 4 Task 2
//!       both halves are portable and ungated — `list_signatures_impl`
//!       only ever *reads* the one path already tracked in `docs`/`store`
//!       for its handle (through `store.read`, like
//!       `capture_pre_edit_snapshot`), so it never had an
//!       arbitrary-output-path write to gate around in the first place.
//! - The Windows sharing-violation retry helpers used by open/save/
//!   save-as: `is_sharing_violation`, `with_sharing_violation_retry`,
//!   `sharing_violation_message`, `copy_with_lock_retry` —
//!   `#[cfg(not(target_arch = "wasm32"))]`.
//!
//! Stays in `apps/desktop/src-tauri/src/lib.rs` (Tauri-specific, or a
//! feature crate this session core deliberately doesn't touch):
//!
//! - `CommandError` — the desktop's app-wide command error, with its own
//!   `Ocr` variant (`ocr.rs` is the one command module that never moved
//!   here) — and a thin, non-generic wrapper around
//!   [`capture_pre_edit_snapshot`] fixing `Err = CommandError`: `ocr.rs`
//!   drives its own bespoke mutate/save/snapshot sequence rather than
//!   going through [`commit_mutation`], and needs `Err` pinned for its
//!   own bare `?` call sites (see [`capture_pre_edit_snapshot`]'s doc for
//!   why a bare re-export can't do that). There is no equivalent wrapper
//!   for `commit_mutation` — every command module that used to need one
//!   has moved into this crate, so nothing in the desktop crate calls it
//!   anymore.
//! - `undo_cmd`, `redo_cmd`, `open_document`, `save_document`,
//!   `save_document_as`, `close_document`, `close_window`, and every
//!   command in `annotations.rs`/`forms.rs`/`field_create.rs`/`pages.rs`/
//!   `textedit.rs`/`redact.rs`/`signatures.rs`/`compare.rs` — now thin
//!   `#[tauri::command]` wrappers over this crate's functions.
//! - `parse_tile_path`, `tile_response`, `TILE_CORS_HEADER`,
//!   `bundled_pdfium_dir`, `run()` — the `tile://` protocol handler and
//!   Tauri builder, which name `tauri::` types directly.
//! - `license.rs` — a separate feature, untouched by any of this.
//!
//! ## The generic `Err: From<DocError> + From<SessionError>` bound
//!
//! [`capture_pre_edit_snapshot`] and [`commit_mutation`] are generic over
//! the caller's error type rather than fixed to [`SessionError`], so both
//! this crate's own mutating modules (fixed to `SessionError`, since they
//! call these functions from inside functions that themselves return
//! `Result<_, SessionError>`) and the desktop's still-`CommandError`-typed
//! caller (`ocr.rs`, via `capture_pre_edit_snapshot`) can drive them
//! directly. A bare re-export can't serve a caller with nothing else
//! pinning `Err`: at a call site like `capture_pre_edit_snapshot(store,
//! &path)?`, the generic `Err` has more than one applicable `From` impl,
//! so `?`'s `From`-search is genuinely ambiguous (confirmed with a
//! throwaway repro — `cannot infer type for type parameter `Err``,
//! E0283). The desktop's `capture_pre_edit_snapshot` wrapper sidesteps
//! this by fixing `Err = CommandError` in a function whose own return
//! type is already concrete (and by supplying its own [`FsWorkingStore`]
//! at the call site — `ocr.rs` itself is unaffected). This crate's own
//! tests (below) call [`commit_mutation`] directly with a turbofish
//! (`commit_mutation::<_, SessionError>(...)`) for the same reason —
//! `SessionError` already satisfies every bound `commit_mutation` needs
//! (`From<DocError>` and `From<SessionError>` via the standard library's
//! blanket `impl<T> From<T> for T`), so no test-only stand-in is needed.
//! Note this bound dropped `From<std::io::Error>` (present before the
//! `WorkingStore` abstraction landed): every direct `std::fs`/`io::Error`
//! call inside these functions moved behind `WorkingStore::read`/`write`,
//! which already return [`SessionError`], so nothing in either function's
//! body needs to convert a bare `io::Error` into `Err` anymore.
//!
//! ## The `WorkingStore` abstraction and why bytes-based reopen is safe
//!
//! [`commit_mutation`]/[`undo_impl`]/[`redo_impl`] all end by calling
//! [`reopen_after_write`], which used to close the stale engine handle and
//! reopen path-based (`engine.open(&path)` / `Document::open(&path)`) —
//! filesystem-only, so it (and everything upstream of it) could never run
//! on wasm32. It now reads the just-written bytes back through the
//! [`WorkingStore`] and reopens from those bytes
//! (`engine.open_bytes(...)` / `Document::from_bytes(...)`) instead, which
//! is what actually makes the whole chain portable. This is exactly
//! equivalent to the old path-based reopen for every backend in this
//! crate, for two independent reasons verified by reading (not assuming)
//! both call sites' implementations:
//!
//! - `Document::open(path)` (`openpdfedit-doc`) is already defined as
//!   `std::fs::read(path)` followed by `Self::from_bytes(&bytes)` — so
//!   `Document::from_bytes(&store.read(&path)?)` is not merely equivalent
//!   to the old path-based open, it performs the identical sequence of
//!   calls the old code did, just with the intermediate `Vec<u8>` made
//!   explicit instead of hidden inside `Document::open`.
//! - Neither `Engine` implementation keys anything by path once a
//!   document is open. `PdfiumEngine::open`/`open_bytes` both allocate a
//!   fresh `u64` handle from the same counter and insert into the same
//!   `documents: Mutex<HashMap<DocHandle, PdfDocument>>` — the only
//!   difference is whether `pdfium-render` mmaps the path or takes
//!   ownership of the byte buffer; both produce a `PdfDocument` behind the
//!   same handle-keyed map, with identical `page_count`/`render_page`/
//!   `page_char_boxes`/`page_sizes` behavior afterwards. `EngineHandle`'s
//!   tile cache (`thread.rs`) is keyed `(DocHandle, page_index,
//!   target_width)` — never by path — so a bytes-based reopen (which
//!   still rotates to a brand-new `DocHandle`, exactly as the path-based
//!   reopen did) invalidates stale tiles exactly the same way. No
//!   path-keyed cache or engine-side "which path is this handle backed
//!   by" state exists anywhere in this workspace to be broken by the
//!   switch.
//!
//! Because of this, every `WorkingStore` implementation (desktop's
//! [`FsWorkingStore`], wasm's [`MemWorkingStore`]) reopens by bytes
//! uniformly — there is no `reopen_path()` fallback hook, because there is
//! no genuine desktop behavioral snag for one to work around.
//! [`FsWorkingStore::read`] is plain `std::fs::read`; [`FsWorkingStore::write`]
//! is a write-tmp-then-rename against the same working-copy path
//! `commit_mutation` always used (not a bare `std::fs::write` — see that
//! method's own doc for why), so the desktop's on-disk *result* (which
//! bytes end up readable from where) is unchanged from the old plain-write
//! behavior on every success path; only the *engine's* reopen call changed
//! from path- to bytes-based, and the bullet above is why that's a no-op
//! change in observable behavior.
//!
//! ## The `#[cfg(not(target_arch = "wasm32"))]` boundary
//!
//! Anything that touches a real filesystem (temp-dir working copies,
//! `std::fs::read`/`write` outside of a [`WorkingStore`] impl, the
//! Windows sharing-violation retry helpers, path-based open/save/save-as,
//! `pages`'s path-based merge/extract orchestration, `signatures`/
//! `compare`'s path-based wrappers, [`FsWorkingStore`] itself) is gated
//! `#[cfg(not(target_arch = "wasm32"))]` — `wasm32-unknown-unknown` has no
//! filesystem. Each of those has a wasm-clean, bytes-based counterpart
//! left ungated where one exists ([`open_document_bytes`];
//! [`WorkingStore`]/[`MemWorkingStore`];
//! `pages::merge_bytes`/`pages::extract_pages_bytes`;
//! `signatures::list_signatures_in_bytes`; `compare::compare_bytes`) so
//! `crates/openpdfedit-wasm` can drive the same logic without the
//! filesystem-bound half.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use openpdfedit_doc::{DocError, Document};
use openpdfedit_engine::{DocHandle, Engine, EngineError};
use serde::Serialize;
use thiserror::Error;

pub mod annotations;
pub mod compare;
pub mod flatten;
pub mod forms;
pub mod outline;
pub mod pages;
pub mod redact;
pub mod search;
pub mod signatures;
pub mod textedit;
pub mod watermark;

/// The write-side state for one open document: its editable object graph
/// plus the path edits get saved back to. See `apps/desktop/src-tauri`'s
/// original module doc (now this crate's) for why edits go through a
/// scratch working copy rather than the user's file directly.
pub struct OpenDoc {
    /// Scratch file the edit pipeline reads and writes. Never the user's
    /// file on desktop — including for `pages::open_new_file`'s
    /// merge/extract results, which get a real scratch copy via
    /// [`OpenDoc::open_with_working_copy`] exactly like every other open
    /// (fixed as part of the fix-wave re-review's NEW-C1: this field used
    /// to equal `original_path` for those results, which is what let
    /// [`close_document_impl`] delete a user's real merge/extract output
    /// on close before that fix landed). For the wasm bytes-based flow,
    /// this is a unique working key — `display_name` suffixed with a
    /// counter (see [`NEXT_BYTES_WORKING_ID`]'s doc) — since there is no
    /// on-disk scratch copy to speak of, but two same-named documents
    /// must still not collide on this key.
    pub path: PathBuf,
    /// Where "Save" writes to — the file the user opened, last saved as,
    /// or (desktop) the destination a merge/extract wrote to; for the
    /// wasm bytes-based flow, the bare, unsuffixed `display_name` (**not**
    /// the same key as `path` — see that field's doc — `path` carries the
    /// uniquifying `#n` suffix `original_path` deliberately does not, so
    /// the user-visible identity stays exactly `display_name`).
    pub original_path: PathBuf,
    /// Whether the working copy has edits not yet written to
    /// `original_path`.
    pub dirty: bool,
    pub doc: Document,
}

impl OpenDoc {
    /// Creates a working copy of `original` in the OS temp directory and
    /// returns the state for it. The working file is uniquely named per
    /// process and handle so several open documents never collide.
    /// Desktop-only: relies on a real filesystem (temp dir, file copy).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_with_working_copy<E: Engine>(
        original: &Path,
        engine: &E,
    ) -> Result<(DocHandle, OpenDoc), SessionError> {
        let stem = original
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".to_string());
        let mut working = std::env::temp_dir();
        working.push(format!(
            "openpdfedit-working-{}-{}-{stem}.pdf",
            std::process::id(),
            NEXT_WORKING_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        copy_with_lock_retry(original, &working)?;

        let handle = engine.open(&working)?;
        let doc = Document::open(&working)?;
        Ok((
            handle,
            OpenDoc {
                path: working,
                original_path: original.to_path_buf(),
                dirty: false,
                doc,
            },
        ))
    }
}

/// Distinguishes working-copy filenames when more than one document is
/// open at once.
#[cfg(not(target_arch = "wasm32"))]
static NEXT_WORKING_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Mints a unique working-copy key for [`open_document_bytes`] — portable
/// (no `#[cfg]` gate, unlike [`NEXT_WORKING_ID`] above), since bytes-based
/// open is exercised on every target including wasm32. Without this, two
/// documents opened under the same `display_name` (the common case of
/// closing one file and reopening a *different* file that happens to
/// share a filename, e.g. "invoice.pdf" from two different folders) would
/// key both their [`WorkingStore`] entry and their [`DocHistory`] entry by
/// the same bare `PathBuf::from(display_name)`, silently aliasing the new
/// document's undo/redo history and working-copy bytes onto whatever the
/// previous same-named document left behind — confirmed empirically: a
/// freshly-opened second document reported `can_undo: true`, and Undo
/// swapped in the *first* document's pre-edit bytes.
static NEXT_BYTES_WORKING_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Undo/redo history for one document, keyed by file path (not
/// [`DocHandle`] — a handle rotates on every edit, since a mutation
/// closes the old render handle and opens a fresh one against the
/// rewritten file, so history has to survive that or every edit would
/// wipe its own undo trail). Snapshots are whole-file byte copies rather
/// than a diff/command log — see the desktop shell's `commit_mutation`
/// doc for the full rationale, which still applies unchanged now that
/// the storage type lives here.
#[derive(Default)]
pub struct DocHistory {
    pub undo: Vec<Vec<u8>>,
    pub redo: Vec<Vec<u8>>,
}

/// How many edits back you can undo. Bounds memory for a long editing
/// session on a large PDF (each entry is a whole-file copy) — not a
/// product decision about how much undo history "feels right," just a
/// safety valve.
const MAX_HISTORY_DEPTH: usize = 20;

/// What the desktop's `AppState` held beyond Tauri specifics: the
/// engine, the open-documents map, the undo/redo history map, and the
/// [`WorkingStore`] every working-copy/snapshot read or write goes
/// through. The desktop shell aliases its `AppState` directly to
/// `SessionState<EngineHandle>` (see that crate's `lib.rs`) so every
/// existing `state.engine`/`state.docs`/`state.history`/`state.store`
/// field access elsewhere in the desktop crate keeps compiling unchanged.
/// All fields are `pub` for exactly that reason — this type is meant to
/// be used as a struct, not through an opaque API.
///
/// `store` is a single non-generic field (`Box<dyn WorkingStore>`) rather
/// than a second type parameter (`SessionState<E, S: WorkingStore>`) —
/// deliberately: every free function in this crate that needs a store
/// already takes `engine`/`docs`/`history` as separate parameters, not a
/// bundled `&SessionState<E>` (see e.g. [`commit_mutation`]'s doc, or
/// `annotations`' module doc, for why — `tauri::State<T>` has no public
/// constructor outside a running app, so keeping the real logic
/// parameterized this way is what makes testing it without one
/// possible), so adding a second generic to `SessionState` itself would
/// only have pushed a second type parameter onto every one of those
/// already-generic-over-`E` functions for no benefit — they take
/// `store: &dyn WorkingStore` directly instead. A boxed trait object here
/// keeps `SessionState<E>` itself at exactly the one type parameter it
/// already had (so `type AppState = SessionState<EngineHandle>` and
/// every existing `SessionState<PdfiumEngine>` construction site needs no
/// new generic argument, just one new field), which is the smallest
/// change to the existing generic bounds and desktop wrapper code the
/// task brief asked to optimize for. Every call site spells the argument
/// as `&*state.store` rather than `&state.store` — confirmed the hard way
/// that the latter does *not* coerce to `&dyn WorkingStore` on its own
/// (rustc reports `the trait bound Box<dyn WorkingStore>: WorkingStore is
/// not satisfied` instead of performing the deref); `&*state.store`
/// (explicit one-step deref of the `Box`, then take a reference) sidesteps
/// that entirely and needs no cast or `.as_ref()` call.
pub struct SessionState<E> {
    pub engine: E,
    pub docs: Mutex<HashMap<DocHandle, OpenDoc>>,
    pub history: Mutex<HashMap<PathBuf, DocHistory>>,
    pub store: Box<dyn WorkingStore>,
}

/// Session-core error type. Leaner than the desktop shell's
/// `CommandError` (no `Ocr` variant — this crate never touches that
/// feature crate; it does need `Annot`, since [`annotations`] lives
/// here); the desktop shell converts via `From<SessionError> for
/// CommandError`.
#[derive(Debug, Error, Serialize)]
pub enum SessionError {
    #[error("{0}")]
    Engine(String),
    #[error("{0}")]
    Doc(String),
    #[error("{0}")]
    Annot(String),
    #[error("unknown document handle {0}")]
    UnknownHandle(DocHandle),
    #[error("I/O error: {0}")]
    Io(String),
}

impl From<EngineError> for SessionError {
    fn from(e: EngineError) -> Self {
        SessionError::Engine(e.to_string())
    }
}

impl From<DocError> for SessionError {
    fn from(e: DocError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

impl From<openpdfedit_annot::AnnotError> for SessionError {
    fn from(e: openpdfedit_annot::AnnotError) -> Self {
        SessionError::Annot(e.to_string())
    }
}

impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        SessionError::Io(e.to_string())
    }
}

/// Where a document's working-copy bytes and undo/redo snapshots
/// actually live. `key` is always an [`OpenDoc::path`] (desktop: a real
/// scratch file in the OS temp dir; wasm: the unique
/// `format!("{display_name}#{n}")` working key [`open_document_bytes`]
/// keys documents by — see [`NEXT_BYTES_WORKING_ID`]'s doc for why it's
/// suffixed rather than the bare `display_name`) — this trait doesn't
/// care which, it just reads/writes whatever bytes are associated with
/// that key. See this module's "The `WorkingStore` abstraction" doc
/// section for the full rationale and the argument that switching
/// [`reopen_after_write`] from a path-based to a bytes-based engine
/// reopen preserves the desktop's existing behavior exactly.
///
/// `Send + Sync`, unlike [`Engine`]'s `Send`-only bound: `Sync` is needed
/// because [`SessionState::store`] is a boxed trait object
/// (`Box<dyn WorkingStore>`, not a generic type parameter) shared behind
/// `&SessionState<E>` across threads (Tauri's managed state requires
/// `Sync`). Putting the bound on the trait itself, rather than only on
/// the field's trait-object type (an earlier draft tried
/// `Box<dyn WorkingStore + Sync>` while keeping this trait `Send`-only),
/// keeps the field's type — and every `&*state.store` call site's target
/// type — a plain `dyn WorkingStore`, with no separate `+ Sync`
/// trait-object type to keep in sync by hand. Both [`FsWorkingStore`] (a
/// unit struct) and [`MemWorkingStore`] (a `Mutex`-guarded map) already
/// satisfy `Sync` via the usual auto-trait rules, so this costs both
/// implementations nothing.
pub trait WorkingStore: Send + Sync {
    /// Reads the current working-copy bytes for `key`. Errors if nothing
    /// has ever been written for `key` yet (desktop: the file doesn't
    /// exist; mem: the key isn't in the map) — every caller in this crate
    /// only ever reads a key after writing it at least once (the working
    /// copy is created before any read/write through this trait happens
    /// at all — see [`OpenDoc::open_with_working_copy`]/
    /// [`open_document_bytes`]), so in practice this is a "the caller
    /// passed a stale/foreign key" bug being surfaced, not a normal path.
    fn read(&self, key: &Path) -> Result<Vec<u8>, SessionError>;
    /// Overwrites the working copy for `key`, creating it if it doesn't
    /// exist yet.
    fn write(&self, key: &Path, bytes: &[u8]) -> Result<(), SessionError>;
    /// Removes the working copy for `key` (close/cleanup). Best-effort —
    /// intentionally infallible, matching `close_document`'s own
    /// best-effort desktop cleanup (a document being closed is on its way
    /// out regardless of whether the underlying remove succeeds).
    fn remove(&self, key: &Path);
}

/// Distinguishes concurrent [`FsWorkingStore::write`] calls' tmp
/// filenames from one another — see that method's doc for why a
/// deterministic-per-key tmp name (`{key}.openpdfedit-tmp`, no counter)
/// isn't safe. Deliberately a separate counter from
/// [`NEXT_BYTES_WORKING_ID`] rather than reusing it: the two mint IDs for
/// unrelated things (a working-copy *key* suffix, minted once per
/// `open_document_bytes` call, versus a tmp-file suffix, minted once per
/// `write` call — potentially many per key over a document's lifetime),
/// and giving each its own counter keeps a reader from having to work out
/// which meaning a given call site needs.
#[cfg(not(target_arch = "wasm32"))]
static NEXT_TMP_WRITE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Desktop's [`WorkingStore`]: the on-disk scratch working copy every
/// mutating command already read/wrote via bare `std::fs` calls before
/// this abstraction existed. `read`/`remove` are exactly `std::fs::read`/
/// `std::fs::remove_file` against `key` — no behavior change from what
/// `commit_mutation`/`undo_impl`/`redo_impl` used to do inline, just moved
/// behind this trait so the same call sites can also run against
/// [`MemWorkingStore`] on wasm. `write` is **not** a bare `std::fs::write`
/// — see its own doc for why.
#[cfg(not(target_arch = "wasm32"))]
pub struct FsWorkingStore;

#[cfg(not(target_arch = "wasm32"))]
impl WorkingStore for FsWorkingStore {
    fn read(&self, key: &Path) -> Result<Vec<u8>, SessionError> {
        std::fs::read(key).map_err(Into::into)
    }

    /// Write-tmp-then-rename, **not** `std::fs::write` (an in-place
    /// truncate-and-rewrite). Restored as part of the fix-wave re-review's
    /// I1: [`forms`]'s module doc used to accept `std::fs::write` here as
    /// a "documented trade-off" specifically because the working copy is
    /// disposable scratch state — but that argument assumed a failed
    /// write only ever left a corrupt *scratch* file, silently ignoring
    /// the one caller for which that's false. `save_document_impl`
    /// (`apps/desktop`) copies the working copy's bytes *as they sit on
    /// disk* over the user's real file — it never re-derives them from
    /// the in-memory `Document`, so it has no way to notice a truncated
    /// working copy versus a complete one. A process crash or disk-full
    /// error mid-`std::fs::write` (which truncates the destination before
    /// writing a single new byte) could leave the working copy shorter
    /// than either the old or new content; the very next `save_document`
    /// call would then copy that truncated garbage straight over the
    /// user's document, with no error anywhere in the chain — the actual
    /// data-loss hazard the old tmp+rename dance in [`forms`] (removed
    /// when the write path unified — see that module's doc) existed to
    /// prevent, just not documented as applying to every store-routed
    /// write once every mutating command started going through this one
    /// function instead of each having its own file-handling code.
    ///
    /// **Per-write atomicity, scoped honestly (I1's own re-review found the
    /// first cut of this doc overclaimed):** a rename is atomic on the
    /// same filesystem (both paths are siblings in the OS temp dir, so
    /// this is never a cross-filesystem rename) — so *for a single write*,
    /// `key` is always either fully the old bytes or fully the new bytes,
    /// never a partial write, regardless of when a crash or error strikes
    /// mid-write. The tmp filename is per-*write*-call-unique
    /// (`{key}.openpdfedit-tmp-{n}` via [`NEXT_TMP_WRITE_ID`]) so two
    /// writers can never corrupt *each other's* tmp file the way a single
    /// shared `{key}.openpdfedit-tmp` name would have — each writer's own
    /// write-then-rename is still individually atomic regardless of what
    /// else is happening concurrently.
    ///
    /// **Concurrent writers to the same key, historically:** this crate
    /// used to have a real UI-reachable path where two writers could race
    /// on the same key with no ordering between their `store.write` calls
    /// at all: `undo_impl`/`redo_impl`/`fill_form_fields_impl` used to
    /// release every lock they held *before* calling `store.write`. On the
    /// UI side this was wider than just "Fill vs Undo behind independent
    /// busy flags" (`formsBusy` vs. an undo/redo one) — that framing
    /// understated it: `apps/desktop/src/routes/+page.svelte` had only one
    /// *real* entry guard anywhere (`undoRedoBusy`, checked at the top of
    /// `handleUndo`/`handleRedo`); `formsBusy`/`pagesBusy`/`ocrBusy` each
    /// gated only their own single button's re-entrancy, not any other
    /// concurrent handler; and nine other mutating handlers (annotation
    /// create, redact, delete-annotation, text-run edit, text-run move,
    /// image move, form-field create, signature placement, and the
    /// open-a-different-document flow) had no busy flag at all. So nothing
    /// stopped a user from triggering *any two* of these close enough
    /// together that their `store.write` calls overlapped — Fill vs Undo
    /// was one reachable instance of that surface, not its full extent.
    /// Per-write tmp-file atomicity (above) made any such overlap a
    /// **last-writer-wins** race rather than a corruption hazard —
    /// whichever tmp was renamed onto `key` last is what every subsequent
    /// `read` saw, always clean data, just not necessarily the caller's
    /// own bytes, and (worse) not necessarily consistent with whichever
    /// writer's undo/redo stack bookkeeping actually reflected the bytes
    /// that landed. **Phase 4 Task 1 closed the write-ordering half of
    /// this**: `undo_impl`, `redo_impl`, and `forms::fill_form_fields_impl`
    /// now hold the `docs` lock across their whole read-pop/mutate-then-
    /// `store.write` sequence, the same discipline `commit_mutation`
    /// already used for every other write path in this crate — see
    /// `undo_impl`'s doc for the full argument (why `docs` rather than a
    /// per-key lock map, and the deadlock-freedom trace against
    /// [`reopen_after_write`]). Two writers on the same key can no longer
    /// have their `store.write` calls overlap at all; last-writer-wins
    /// only in the sense that whichever writer's turn came second still
    /// legitimately sees the first one's already-committed result as its
    /// own pre-edit state, same as if a user had triggered the two
    /// operations one after the other by hand — for the write itself,
    /// **for every write path except one**. That equivalence needs one
    /// more assumption than write-ordering alone: that each writer's own
    /// *payload* was itself derived from a snapshot no older than the
    /// moment it started waiting for the lock. `commit_mutation` satisfies
    /// this (its `mutate` closure and `save_incremental()` both run inside
    /// the lock, right after the snapshot read) and so do `undo_impl`/
    /// `redo_impl` (the history entry they act on is popped inside the
    /// lock too) — but `forms::fill_form_fields_impl` does not:
    /// `engine.fill_form_fields`/`engine.save_to_bytes`/button-state
    /// normalization all run *before* it ever asks for the lock, against
    /// whatever state was live at that earlier moment. See that function's
    /// own doc for the fill-specific lost-update window this leaves open —
    /// a real gap Phase 4 Task 1 does not close, tracked as a sibling
    /// residual to the one below with the same candidate fix. That claim
    /// is scoped to `store.write` specifically, not to the whole
    /// undo/commit/redo operation even where the snapshot-freshness
    /// assumption *does* hold: a residual race remains in
    /// [`reopen_after_write`], which every one of these functions calls
    /// *after* releasing `docs` (has to — see `undo_impl`'s doc for why
    /// holding `docs` through it would deadlock). If two writers were both
    /// dispatched against the *same* starting [`DocHandle`], the loser's
    /// `reopen_after_write` call still targets that same now-already-
    /// rotated `old_handle`; its `docs.remove(&old_handle)` finds nothing
    /// (the winner already removed it) and falls back to `original_path =
    /// path` — the scratch working-copy path, not the user's real file.
    /// Named and tracked as a follow-up (Phase 4 ledger), not fixed here —
    /// see `undo_impl`'s doc for the exact mechanics, why closing it needs
    /// a lock that can span `reopen_after_write` (which `docs` itself
    /// cannot), and — as of this fix wave — the UI-level `mutationBusy`
    /// gate that narrows how *reachable* this residual is without closing
    /// it at the store/session level.
    ///
    /// On a failed `rename`, best-effort removes the tmp file rather than
    /// leaving it behind for [`WorkingStore::remove`]/a future write to
    /// never clean up — still not a full guarantee: a crash between the
    /// `write` and the `rename` (or during the cleanup `remove` itself)
    /// can still orphan a stray `.openpdfedit-tmp-N` file in the OS temp
    /// dir. That's inert (a same-process-scoped name under
    /// [`OpenDoc::open_with_working_copy`]'s naming, on a system temp dir
    /// that's expected to accumulate and get cleaned by the OS/user over
    /// time) rather than a correctness or data-loss problem — nothing
    /// ever reads a `*.openpdfedit-tmp-*` path back in as a working copy.
    fn write(&self, key: &Path, bytes: &[u8]) -> Result<(), SessionError> {
        let n = NEXT_TMP_WRITE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut tmp = key.as_os_str().to_owned();
        tmp.push(format!(".openpdfedit-tmp-{n}"));
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, bytes)?;
        if let Err(e) = std::fs::rename(&tmp, key) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(())
    }

    fn remove(&self, key: &Path) {
        let _ = std::fs::remove_file(key);
    }
}

/// Portable, in-memory [`WorkingStore`] — `wasm32-unknown-unknown` has no
/// filesystem at all, so the wasm/Chrome-extension build's `SessionState`
/// constructs one of these instead of an [`FsWorkingStore`]. Not
/// `#[cfg]`-gated: it's plain `HashMap`/`Mutex` code with no
/// platform-specific dependency, so it works (and is exercised by this
/// crate's own non-wasm test suite too — see
/// [`open_bytes_then_commit_mutation_then_undo_then_redo_all_through_mem_working_store`])
/// on every target, not just wasm32.
#[derive(Default)]
pub struct MemWorkingStore(Mutex<HashMap<PathBuf, Vec<u8>>>);

impl WorkingStore for MemWorkingStore {
    fn read(&self, key: &Path) -> Result<Vec<u8>, SessionError> {
        self.0
            .lock()
            .expect("working store lock poisoned")
            .get(key)
            .cloned()
            .ok_or_else(|| SessionError::Io(format!("no working copy for {}", key.display())))
    }

    fn write(&self, key: &Path, bytes: &[u8]) -> Result<(), SessionError> {
        self.0
            .lock()
            .expect("working store lock poisoned")
            .insert(key.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn remove(&self, key: &Path) {
        self.0
            .lock()
            .expect("working store lock poisoned")
            .remove(key);
    }
}

/// Windows' `ERROR_SHARING_VIOLATION` (raw OS error 32): the destination
/// is open in another process without share-write access. See the
/// original desktop module doc (moved here verbatim) for the full
/// rationale — common causes are another PDF viewer, antivirus, or a
/// cloud-sync client transiently holding the file.
#[cfg(all(not(target_arch = "wasm32"), windows))]
fn is_sharing_violation(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(32)
}

#[cfg(all(not(target_arch = "wasm32"), not(windows)))]
fn is_sharing_violation(_e: &std::io::Error) -> bool {
    false
}

/// Retries `f` with a short backoff on a Windows sharing violation — the
/// lock is usually momentary, so the very first attempt right after the
/// user clicks Save is a common false failure. Any other error, or a
/// sharing violation still present after every retry, is returned to the
/// caller as-is. A no-op passthrough on non-Windows.
#[cfg(not(target_arch = "wasm32"))]
fn with_sharing_violation_retry<T>(
    mut f: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    const RETRY_DELAYS_MS: [u64; 4] = [50, 150, 400, 800];
    let mut attempt = 0;
    loop {
        match f() {
            Err(e) if is_sharing_violation(&e) && attempt < RETRY_DELAYS_MS.len() => {
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAYS_MS[attempt]));
                attempt += 1;
            }
            result => return result,
        }
    }
}

/// A sharing violation that survived every retry, turned into something a
/// user can actually act on — "os error 32" means nothing to someone who
/// doesn't know the Win32 error table by heart.
#[cfg(not(target_arch = "wasm32"))]
fn sharing_violation_message(to: &Path) -> String {
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| to.display().to_string());
    format!(
        "Couldn't save — \"{name}\" is still open in another program (or was just grabbed by \
         antivirus/cloud sync). Close any other program that has this file open, then try \
         Save again."
    )
}

/// Copies `from` to `to`, retrying past a transient Windows sharing
/// violation ([`with_sharing_violation_retry`]) and, if it's still
/// locked afterwards, failing with [`sharing_violation_message`] instead
/// of the raw OS string.
#[cfg(not(target_arch = "wasm32"))]
fn copy_with_lock_retry(from: &Path, to: &Path) -> Result<(), SessionError> {
    with_sharing_violation_retry(|| std::fs::copy(from, to).map(|_| ())).map_err(|e| {
        if is_sharing_violation(&e) {
            SessionError::Io(sharing_violation_message(to))
        } else {
            SessionError::Io(e.to_string())
        }
    })
}

#[derive(Serialize)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Serialize)]
pub struct OpenedDocumentInfo {
    pub handle: DocHandle,
    pub page_count: u32,
    /// One entry per page, in reading order, in PDF points — lets the
    /// viewer lay out a correctly-proportioned virtualized scroll
    /// container before any pixels have loaded.
    pub page_sizes: Vec<PageSize>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// True when the working copy has edits not yet written to the
    /// user's file — drives the unsaved-changes indicator and the
    /// prompt shown when closing.
    pub is_dirty: bool,
    /// The file "Save" writes to, for display in the title bar (or the
    /// `display_name` passed to [`open_document_bytes`], for the wasm
    /// flow).
    pub file_path: String,
}

/// Looks up `handle` in an already-locked `docs` map, mapping a miss to
/// [`SessionError::UnknownHandle`] — the read-only half of the
/// `docs.get(&handle).ok_or(SessionError::UnknownHandle(handle))?` idiom
/// this crate's command modules (`annotations`, `forms`, `pages`,
/// `signatures`, `textedit`) and this file itself repeat at every site
/// that resolves a handle to its [`OpenDoc`]. A thin wrapper, not a new
/// abstraction — callers still take the lock themselves (the guard's
/// lifetime governs how long the mutation inside a `{ }` block stays
/// scoped, which a helper that took the `Mutex` itself couldn't preserve).
pub(crate) fn resolve_doc(
    docs: &HashMap<DocHandle, OpenDoc>,
    handle: DocHandle,
) -> Result<&OpenDoc, SessionError> {
    docs.get(&handle).ok_or(SessionError::UnknownHandle(handle))
}

/// The `&mut` counterpart of [`resolve_doc`], for call sites that mutate
/// the resolved [`OpenDoc`] in place (e.g. flipping `dirty`) rather than
/// just reading it.
pub(crate) fn resolve_doc_mut(
    docs: &mut HashMap<DocHandle, OpenDoc>,
    handle: DocHandle,
) -> Result<&mut OpenDoc, SessionError> {
    docs.get_mut(&handle)
        .ok_or(SessionError::UnknownHandle(handle))
}

/// Builds the DTO handed back to callers after any open/save/undo/redo
/// operation. Takes `engine`/`docs`/`history` as separate parameters
/// (rather than a bundled `&SessionState<E>`) to match every existing
/// desktop call site this function keeps serving unchanged (e.g.
/// `pages.rs`'s `open_new_file`).
pub fn opened_document<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    path: &Path,
    handle: DocHandle,
) -> Result<OpenedDocumentInfo, SessionError> {
    let (is_dirty, file_path) = {
        let guard = docs.lock().expect("docs lock poisoned");
        match guard.get(&handle) {
            Some(d) => (d.dirty, d.original_path.to_string_lossy().into_owned()),
            None => (false, String::new()),
        }
    };
    let page_count = engine.page_count(handle)?;
    let page_sizes = engine
        .page_sizes(handle)?
        .into_iter()
        .map(|s| PageSize {
            width: s.width,
            height: s.height,
        })
        .collect();
    let (can_undo, can_redo) = {
        let history_guard = history.lock().expect("history lock poisoned");
        match history_guard.get(path) {
            Some(entry) => (!entry.undo.is_empty(), !entry.redo.is_empty()),
            None => (false, false),
        }
    };
    Ok(OpenedDocumentInfo {
        handle,
        page_count,
        page_sizes,
        can_undo,
        can_redo,
        is_dirty,
        file_path,
    })
}

/// The shared "close the old render handle, reopen against `path`, and
/// refresh the doc store under the fresh handle" tail every command that
/// changes a document's on-disk bytes needs afterwards. Returns the
/// fresh [`OpenedDocumentInfo`] — the handle a caller passed in isn't
/// necessarily the one it gets back.
///
/// Reopens from `store.read(&path)` rather than `engine.open(&path)`/
/// `Document::open(&path)` — see this module's "The `WorkingStore`
/// abstraction" doc section for why that's exactly equivalent to the old
/// path-based reopen for every engine/store combination in this crate,
/// not just an approximation of it.
pub fn reopen_after_write<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    old_handle: DocHandle,
    path: PathBuf,
) -> Result<OpenedDocumentInfo, SessionError> {
    // Carry the user-facing identity across the rotation: which file a
    // save targets, and the fact that reaching here means the working
    // copy now differs from it.
    let original_path = {
        let mut guard = docs.lock().expect("docs lock poisoned");
        guard
            .remove(&old_handle)
            .map(|d| d.original_path)
            .unwrap_or_else(|| path.clone())
    };
    engine.close(old_handle);

    let bytes = store.read(&path)?;
    // `Document::from_bytes` (a borrow) before `engine.open_bytes` (which
    // takes `bytes` by value): the reverse order used to need a full
    // `bytes.clone()` just to keep a copy around for `Document::from_bytes`
    // after `engine.open_bytes` had already consumed the original, and —
    // worse than the extra clone — a `Document::from_bytes` parse failure
    // after a successful `engine.open_bytes` would return via `?` having
    // already leaked that freshly-opened engine handle (never closed,
    // since this function has no more code left to close it with once the
    // early return fires). Parsing first means a parse failure never opens
    // an engine handle at all, and `engine.open_bytes(bytes)` can now move
    // `bytes` directly — no clone needed.
    let doc = Document::from_bytes(&bytes)?;
    let new_handle = engine.open_bytes(bytes)?;
    docs.lock().expect("docs lock poisoned").insert(
        new_handle,
        OpenDoc {
            path: path.clone(),
            original_path,
            dirty: true,
            doc,
        },
    );

    opened_document(engine, docs, history, &path, new_handle)
}

/// Pushes a pre-edit snapshot onto `path`'s undo stack (capped at
/// [`MAX_HISTORY_DEPTH`]) and clears its redo stack — a fresh edit
/// invalidates whatever redo trail existed, standard undo/redo
/// semantics.
pub fn commit_undo_snapshot(
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    path: &Path,
    pre_edit_bytes: Vec<u8>,
) {
    let mut history_guard = history.lock().expect("history lock poisoned");
    let entry = history_guard.entry(path.to_path_buf()).or_default();
    entry.undo.push(pre_edit_bytes);
    if entry.undo.len() > MAX_HISTORY_DEPTH {
        entry.undo.remove(0);
    }
    entry.redo.clear();
}

/// Reads `path`'s *current* on-disk bytes, to later hand to
/// [`commit_undo_snapshot`] — split into a separate "capture" step (call
/// this **before** mutating) from "commit" (call that **after** the
/// mutation and save both succeed) so a *failed* edit never pushes a
/// bogus undo entry or wipes an existing redo trail. Reading here (rather
/// than inside `commit_undo_snapshot`, after the file's already been
/// overwritten) is what makes this the pre-edit state at all.
///
/// Generic over the caller's error type rather than fixed to
/// [`SessionError`] — see this module's doc comment ("The generic `Err`
/// bound" section) for why callers that need a *concrete* return type (so
/// a bare `?` at the call site can infer `Err`) should go through a thin
/// wrapper instead of calling this directly.
///
/// Reads through `store` rather than calling `std::fs::read` directly —
/// see this module's "The `WorkingStore` abstraction" doc section.
pub fn capture_pre_edit_snapshot<Err: From<SessionError>>(
    store: &dyn WorkingStore,
    path: &Path,
) -> Result<Vec<u8>, Err> {
    store.read(path).map_err(Into::into)
}

/// The shared "mutate, save, rotate the render handle" sequence every
/// single-document editing command follows: snapshot the pre-edit bytes
/// for undo (see [`capture_pre_edit_snapshot`]), run `mutate` against the
/// open document's editable graph, save it incrementally to disk, then
/// hand off to [`reopen_after_write`].
///
/// Generic over the caller's error type (`Err`) rather than fixed to
/// [`SessionError`] — every mutating module in this crate
/// ([`annotations`], [`forms`], [`pages`], [`textedit`], [`redact`])
/// fixes `Err = SessionError` by using it in functions that themselves
/// return `Result<_, SessionError>` directly; the desktop shell has no
/// caller of this function at all anymore (see this module's doc comment
/// for why). This crate's own tests call it directly with a turbofish
/// (`commit_mutation::<_, SessionError>(...)`) for the same E0283 reason
/// documented there.
pub fn commit_mutation<E: Engine, Err>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
    mutate: impl FnOnce(&mut Document) -> Result<(), Err>,
) -> Result<OpenedDocumentInfo, Err>
where
    Err: From<DocError> + From<SessionError>,
{
    let path = {
        let mut docs_guard = docs.lock().expect("docs lock poisoned");
        let open_doc = docs_guard
            .get_mut(&handle)
            .ok_or_else(|| Err::from(SessionError::UnknownHandle(handle)))?;

        // Turbofish required: `Err` isn't otherwise pinned down at this
        // call site (same E0283 ambiguity described in this module's
        // doc comment), even though this function's own `Err` is
        // already a fixed type parameter here.
        let pre_edit_snapshot = capture_pre_edit_snapshot::<Err>(store, &open_doc.path)?;
        mutate(&mut open_doc.doc)?;
        let saved = open_doc.doc.save_incremental()?;
        store.write(&open_doc.path, &saved)?;
        commit_undo_snapshot(history, &open_doc.path, pre_edit_snapshot);
        open_doc.path.clone()
    };

    reopen_after_write(engine, docs, history, store, handle, path).map_err(Into::into)
}

/// Undoes the most recent edit for the document at `handle`: restores
/// the file to its pre-edit bytes and rotates the render handle, same as
/// any other write. Errors if there's nothing to undo.
///
/// **Per-key write serialization (Phase 4 Task 1):** holds `docs` across
/// the whole history-pop + `store.read`/`store.write` sequence below —
/// not just the initial `path` lookup, the way this function used to.
/// See [`FsWorkingStore::write`]'s doc for the race this closes:
/// [`commit_mutation`] already holds `docs` across its own `store.write`
/// (dropped only once its `let path = { ... };` block ends, *before* it
/// calls [`reopen_after_write`]); this function used to drop every lock
/// — `docs` first, then `history` — before ever touching `store.write`,
/// so a concurrent `commit_mutation` on the same document could have its
/// write land in the middle of this one's read-pop-write sequence, or
/// vice versa, with no ordering guarantee between the two beyond
/// `FsWorkingStore::write`'s own per-write tmp-then-rename atomicity
/// (clean bytes, but a coin-flip *which* writer's bytes survive, and no
/// guarantee the undo/redo stacks stay consistent with whichever one
/// did). `docs` is a single process-wide `Mutex`, not keyed per-document
/// — so this doesn't just block a second writer *on the same document*,
/// it blocks every other `docs`-locking call (any document's
/// `commit_mutation`/`undo_impl`/`redo_impl`/`fill_form_fields_impl`, or
/// a lookup like [`opened_document`]) for the duration of this
/// function's `store.write`. That's an accepted, pre-existing trade-off,
/// not a new one introduced here: [`commit_mutation`] — the majority of
/// this crate's write paths ([`annotations`], [`pages`], [`textedit`],
/// [`redact`], [`forms::create_form_field_impl`]) — already holds `docs`
/// across its own `store.write` today, so this function and
/// [`redo_impl`]/[`forms::fill_form_fields_impl`] adopting the identical
/// discipline doesn't change the shape of the bottleneck, just closes
/// the gap where three of eleven write paths didn't follow it. A
/// per-document lock (keyed by `path`, e.g. a
/// `Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>` acquired before the write
/// in every writer path) was considered and rejected for this task —
/// **not** because its only benefit is narrower blocking radius. A
/// per-key lock is a *distinct* `Mutex` from `docs`, so — unlike `docs`
/// itself — it could be held across the entire operation, including the
/// call to [`reopen_after_write`] (nesting `docs.lock()` calls inside an
/// already-held, different `Mutex` is fine; only re-acquiring the *same*
/// non-reentrant `Mutex` deadlocks, per the "Deadlock-freedom" paragraph
/// below). That's a real structural advantage `docs` provably cannot
/// match: it would also close the residual race documented on
/// [`FsWorkingStore::write`] (two writers sharing a starting handle,
/// where the second one's `reopen_after_write` call finds `old_handle`
/// already removed and falls back to the wrong `original_path`) — a
/// per-key lock held through `reopen_after_write` means a second writer
/// on the same key never *reaches* that call until the first one's
/// entire operation, rotation included, has finished. `docs` cannot do
/// this at any hold duration, because `docs` is the very lock
/// `reopen_after_write` itself needs to acquire twice.
///
/// Rejected for *this* task anyway: narrower blocking radius (blocking
/// only same-document writers, not every open document) is real but
/// secondary on its own, and closing the `reopen_after_write` residual
/// is a distinct, separately-scoped fix this task's brief didn't ask
/// for (per-key write serialization, not handle-rotation bookkeeping) —
/// bundling it in here would mean a new field on [`SessionState`] and a
/// new parameter threaded through `commit_mutation`/`undo_impl`/
/// `redo_impl`/`fill_form_fields_impl` and every one of their ~30 call
/// sites across this crate's own submodules plus
/// `apps/desktop/src-tauri` and `crates/openpdfedit-wasm`, to fix two
/// problems in one change instead of landing the smaller, targeted one
/// this task scoped. It also doesn't fix itself: applied only to the
/// three functions this task touches (leaving the *other* eight
/// `commit_mutation`-routed write paths still serialized on the coarser
/// `docs` lock) it would be a mixed discipline — some paths closing the
/// `reopen_after_write` residual, most not — that's harder to reason
/// about than either "all paths use `docs`, residual named and tracked"
/// (what this change lands) or "all paths use a per-key lock, residual
/// closed everywhere" (the real fix, follow-up work). Nothing about this
/// decision forecloses that follow-up.
///
/// Deadlock-freedom: this reuses [`commit_mutation`]'s own lock order
/// (`docs` held, `history` acquired and released *inside* that while
/// `docs` is still held, matching this function's nested
/// `history.lock()` call inside its `docs_guard`-held block below) — no
/// new ordering is introduced, so this can't deadlock against anything
/// `commit_mutation` doesn't already deadlock against (nothing does:
/// `history` is never held while acquiring `docs`, anywhere in this
/// crate — confirmed by inspection, there is no reverse-order call
/// site). Critically, `docs` is *not* held across the call to
/// [`reopen_after_write`] below — the `let path = { ... };` block ends,
/// dropping `docs_guard`, before that call. [`reopen_after_write`]
/// itself needs `docs.lock()` twice (once to `remove` the old handle's
/// entry, once to `insert` the rotated one) — a non-reentrant
/// `std::sync::Mutex`, so calling it with `docs_guard` still held would
/// deadlock this thread against itself on its own first `docs.lock()`
/// call. That's exactly why the critical section this function extends
/// stops at `store.write`, not at [`reopen_after_write`]'s tail — same
/// boundary [`commit_mutation`] already draws.
///
/// **Residual (not fixed by this task, named precisely so it isn't
/// mistaken for closed):** stopping the critical section at `store.write`
/// means two writers that both started from the *same* [`DocHandle`] can
/// still both reach [`reopen_after_write`] with that same `old_handle` —
/// this task serializes the *writes* (no two `store.write` calls for the
/// same key ever overlap), but does nothing about two already-serialized
/// writers each independently calling `reopen_after_write(..., old_handle,
/// ...)` with the identical `old_handle` value afterward. Concretely: the
/// first to acquire `docs.lock()` inside `reopen_after_write` removes
/// `old_handle` and gets the correct `original_path`; the second finds
/// `old_handle` already gone, and `reopen_after_write`'s
/// `.unwrap_or_else(|| path.clone())` fallback hands it `original_path =
/// path` — the scratch working-copy path, **not** the user's real file.
/// Consequences, all real and all downstream of that one wrong value:
/// the title bar (`OpenedDocumentInfo::file_path`) shows the scratch
/// path instead of the user's file; a subsequent desktop `save_document`
/// would copy the working copy onto *itself* (`copy_with_lock_retry(&working,
/// &original)` with `working == original`) — the same
/// read-while-truncating self-copy hazard `pages.rs`'s
/// `close_document_impl` doc (NEW-C1) already documents for a different
/// code path; `close_document_impl`'s `path != original_path` guard now
/// evaluates differently for this entry, so it may skip
/// `store.remove` it should have run (or vice versa); and the *first*
/// writer's own successfully-rotated handle is left as a live but
/// never-referenced-again entry in `docs` (not deleted — just orphaned,
/// since nothing closes it). This is a **pre-existing** condition, not
/// introduced by this task: `reopen_after_write`'s shape (call it only
/// after releasing `docs`, `.remove` with a `path`-cloning fallback) is
/// unchanged here, and the same same-old-handle sharing was always
/// possible between any two concurrent writers on a shared handle (e.g.
/// two racing `commit_mutation` calls, before this task existed).
/// Closing it needs a lock that can span `reopen_after_write` itself —
/// the "Rejected for this task anyway" paragraph above is why that's
/// out of scope here; tracked as a follow-up in the Phase 4 ledger
/// (`.superpowers/sdd/2026-08-16-extension-port-phase4/progress.md`).
///
/// **Reachability, narrowed but not closed (fix-wave 2, same ledger):**
/// triggering this residual needs two writers dispatched against the
/// *same* `DocHandle` close enough together to both still be mid-flight —
/// concretely, from the shared UI, two of `+page.svelte`'s mutating
/// handlers (annotate/redact/textedit/pages/forms/undo/redo/etc.) running
/// concurrently against the same open document. `+page.svelte` now sets
/// a single global `mutationBusy` flag for the duration of every one of
/// those handlers (checked at each handler's own entry, same early-return
/// shape `undoRedoBusy` already used just for undo/redo) and disables
/// `PdfPage.svelte`'s gesture overlay for as long as it's set, so the
/// shared UI itself can no longer have two mutating calls in flight at
/// once — the precondition for this residual can no longer arise from
/// normal use of the app. That closes *reachability from the UI*, not the
/// residual itself: `SessionState`/`commit_mutation`/`undo_impl`/
/// `redo_impl`/`forms::fill_form_fields_impl` gained no entry guard of
/// their own here, so any caller that doesn't go through this one shared
/// UI layer — a test driving the session crate directly (as
/// [`WriteOverlapProbe`] below does), a future second window/tab, or any
/// other integration built on this crate — can still trigger it. Stated
/// plainly so neither half is overclaimed: narrowed to UI-unreachable,
/// not closed at the store/session level, which still needs the per-key
/// lock spanning `reopen_after_write` described above.
pub fn undo_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
) -> Result<OpenedDocumentInfo, SessionError> {
    let path = {
        let docs_guard = docs.lock().expect("docs lock poisoned");
        let path = resolve_doc(&docs_guard, handle)?.path.clone();

        let snapshot = {
            let mut history_guard = history.lock().expect("history lock poisoned");
            let entry = history_guard.entry(path.clone()).or_default();
            let snapshot = entry
                .undo
                .pop()
                .ok_or_else(|| SessionError::Doc("nothing to undo".to_string()))?;
            let current = store.read(&path)?;
            entry.redo.push(current);
            if entry.redo.len() > MAX_HISTORY_DEPTH {
                entry.redo.remove(0);
            }
            snapshot
        };
        store.write(&path, &snapshot)?;
        path
    };

    reopen_after_write(engine, docs, history, store, handle, path)
}

/// The redo half of [`undo_impl`] — see that function's doc for the
/// per-key write serialization discipline and its deadlock-freedom
/// argument, which apply here identically (mirror-image of `undo`'s
/// stack bookkeeping, same lock shape).
pub fn redo_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
) -> Result<OpenedDocumentInfo, SessionError> {
    let path = {
        let docs_guard = docs.lock().expect("docs lock poisoned");
        let path = resolve_doc(&docs_guard, handle)?.path.clone();

        let snapshot = {
            let mut history_guard = history.lock().expect("history lock poisoned");
            let entry = history_guard.entry(path.clone()).or_default();
            let snapshot = entry
                .redo
                .pop()
                .ok_or_else(|| SessionError::Doc("nothing to redo".to_string()))?;
            let current = store.read(&path)?;
            entry.undo.push(current);
            if entry.undo.len() > MAX_HISTORY_DEPTH {
                entry.undo.remove(0);
            }
            snapshot
        };
        store.write(&path, &snapshot)?;
        path
    };

    reopen_after_write(engine, docs, history, store, handle, path)
}

/// The impl behind the desktop's `open_document` command: makes a
/// working copy of `path` and registers it in `state`. Path-based, so
/// desktop-only.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_document_impl<E: Engine>(
    state: &SessionState<E>,
    path: &Path,
) -> Result<OpenedDocumentInfo, SessionError> {
    let (handle, open_doc) = OpenDoc::open_with_working_copy(path, &state.engine)?;
    let working = open_doc.path.clone();
    state
        .docs
        .lock()
        .expect("docs lock poisoned")
        .insert(handle, open_doc);
    opened_document(&state.engine, &state.docs, &state.history, &working, handle)
}

/// Before/after byte counts of a [`compress_document_to_path_impl`] run,
/// serialized camelCase for the IPC boundary like every sibling DTO.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressStats {
    pub before_bytes: u64,
    pub after_bytes: u64,
}

/// The impl behind the desktop's `compress_document_cmd`: writes a
/// **compressed copy** of the document's current state to `output_path`
/// and reports before/after sizes. "Compress" here is a full PDFium
/// rewrite (`Engine::save_to_bytes`, FPDF_SaveAsCopy): it drops the
/// incremental-update revision chain this app's own save pipeline
/// accumulates (every edit only ever appends — see `openpdfedit-doc`'s
/// module doc) plus any orphaned/unreferenced objects, which is where
/// real size wins come from on an edited document. The flip side,
/// deliberately NOT hidden from the UI: a full rewrite does not carry
/// existing digital signatures over (same trade-off `fill_form_fields`
/// documents). The open document itself is untouched — this is an
/// export, not a mutation, so no handle rotation and no dirty change.
///
/// Path-based (writes a file), so desktop-only; the extension builds the
/// same feature UI-side from the already-exported `saveToBytes` +
/// `workingCopyBytes` wasm methods and the browser save picker — see
/// `wasm.ts`'s `compressDocument`.
#[cfg(not(target_arch = "wasm32"))]
pub fn compress_document_to_path_impl<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
    output_path: &Path,
) -> Result<CompressStats, SessionError> {
    let working = {
        let guard = state.docs.lock().expect("docs lock poisoned");
        resolve_doc(&guard, handle)?.path.clone()
    };
    let before_bytes = state.store.read(&working)?.len() as u64;
    let bytes = state.engine.save_to_bytes(handle)?;
    let after_bytes = bytes.len() as u64;
    std::fs::write(output_path, &bytes)?;
    Ok(CompressStats {
        before_bytes,
        after_bytes,
    })
}

/// The impl behind the desktop's `save_document` command: writes the
/// working copy over the file the user opened. Path-based, so
/// desktop-only.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_document_impl<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
) -> Result<OpenedDocumentInfo, SessionError> {
    let (working, original) = {
        let guard = state.docs.lock().expect("docs lock poisoned");
        let d = resolve_doc(&guard, handle)?;
        (d.path.clone(), d.original_path.clone())
    };
    copy_with_lock_retry(&working, &original)?;
    if let Some(d) = state
        .docs
        .lock()
        .expect("docs lock poisoned")
        .get_mut(&handle)
    {
        d.dirty = false;
    }
    opened_document(&state.engine, &state.docs, &state.history, &working, handle)
}

/// The impl behind the desktop's `save_document_as` command: writes the
/// working copy to a new location, which becomes the target of
/// subsequent saves. Path-based, so desktop-only.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_document_as_impl<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
    path: &Path,
) -> Result<OpenedDocumentInfo, SessionError> {
    let working = {
        let guard = state.docs.lock().expect("docs lock poisoned");
        resolve_doc(&guard, handle)?.path.clone()
    };
    copy_with_lock_retry(&working, path)?;
    if let Some(d) = state
        .docs
        .lock()
        .expect("docs lock poisoned")
        .get_mut(&handle)
    {
        d.original_path = path.to_path_buf();
        d.dirty = false;
    }
    opened_document(&state.engine, &state.docs, &state.history, &working, handle)
}

/// The wasm-safe counterpart of [`open_document_impl`]: opens a document
/// from in-memory `bytes` (no filesystem involved) via
/// `Engine::open_bytes`/`Document::from_bytes`, and registers it in
/// `state`'s docs/history maps keyed by a unique working key derived from
/// `display_name` (see the paragraph below — **not** the bare
/// `display_name` itself) — the extension's synthetic identity for a
/// document that never had a real on-disk path. Same handle/history
/// bookkeeping as [`open_document_impl`], just sourced from bytes.
///
/// Also seeds `state.store` with `bytes` under that same key — the
/// [`WorkingStore`] equivalent of what [`OpenDoc::open_with_working_copy`]
/// does for the path-based desktop flow (copying the original into the
/// scratch working-copy location *before* anything else can read it
/// through the store). Without this, the first [`commit_mutation`]/
/// [`undo_impl`]/[`redo_impl`] call against a bytes-opened document would
/// find nothing at this key and fail — this crate's own mutating command
/// modules don't yet have a wasm-facing entry point (that's follow-up
/// work, not this function's job), but this seeding is what makes it
/// possible once one exists, and is exercised directly by
/// [`open_bytes_then_commit_mutation_then_undo_then_redo_all_through_mem_working_store`]
/// below.
///
/// `path`/`store`/`history` are keyed by a **unique** working key —
/// `format!("{display_name}#{n}")` from [`NEXT_BYTES_WORKING_ID`] — not
/// the bare `display_name` a naive reading of "keyed by display name"
/// might suggest. Two documents opened under the same `display_name`
/// (closing one "invoice.pdf" and opening a *different* "invoice.pdf")
/// must not alias each other's [`WorkingStore`] bytes or [`DocHistory`]
/// undo/redo stacks — see [`NEXT_BYTES_WORKING_ID`]'s doc for the
/// empirically-confirmed failure this guards against. `original_path`
/// stays the bare `PathBuf::from(display_name)`, since that's what
/// [`opened_document`] surfaces as `file_path` (see that function's
/// `original_path.to_string_lossy()` line) — the user-visible name must
/// stay exactly `display_name`, unsuffixed.
pub fn open_document_bytes<E: Engine>(
    state: &SessionState<E>,
    display_name: &str,
    bytes: Vec<u8>,
) -> Result<OpenedDocumentInfo, SessionError> {
    let doc = Document::from_bytes(&bytes)?;
    let n = NEXT_BYTES_WORKING_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = PathBuf::from(format!("{display_name}#{n}"));
    let original_path = PathBuf::from(display_name);
    // Outside `docs.lock()` — unlike every write-path function above,
    // deliberately not a lost-update risk: `path` is fresh and
    // per-call-unique (`NEXT_BYTES_WORKING_ID`), so no other writer can
    // ever be targeting the same key concurrently until the `docs.lock()`
    // below publishes this handle for the first time.
    state.store.write(&path, &bytes)?;
    let handle = state.engine.open_bytes(bytes)?;
    state.docs.lock().expect("docs lock poisoned").insert(
        handle,
        OpenDoc {
            path: path.clone(),
            original_path,
            dirty: false,
            doc,
        },
    );
    opened_document(&state.engine, &state.docs, &state.history, &path, handle)
}

/// Marks the document at `handle` clean — flips [`OpenDoc::dirty`] back to
/// `false`, the same field [`save_document_impl`]/[`save_document_as_impl`]
/// set inline (`d.dirty = false`) once the desktop's path-based flow has
/// actually written the working copy back to the user's file (or the new
/// save-as location). This is that same "commit the save" moment's
/// portable/bytes-based counterpart: for a byte-opened document (wasm),
/// "save" is the `WasmSession::saveToBytes` + `wasm.ts`'s own
/// `FileSystemFileHandle` write — neither of which touches this crate's
/// `docs` map, since `saveToBytes` only asks the engine for bytes and
/// returns them (see that method's doc for why it deliberately doesn't
/// mutate). Nothing else in this crate would ever flip a byte-opened
/// document's `dirty` bit back to `false` without this function.
///
/// **Caller contract**: only call this *after* the bytes this document's
/// `dirty` flag is tracking have actually landed somewhere durable — never
/// speculatively before that write, and never after a failed one. This
/// mirrors the desktop's own dirty semantics exactly:
/// `copy_with_lock_retry`'s `?` inside [`save_document_impl`] short-circuits
/// the whole function before its `d.dirty = false` line ever runs on a
/// failed copy, so a failed desktop save leaves `dirty` untouched too — a
/// caller of this function (`WasmSession::mark_saved`, driven by
/// `wasm.ts`'s `writeToFileHandle`) gets the identical guarantee only by
/// being called at the identical point in its own save sequence: after the
/// write, not before, and only on that write's success path.
///
/// Portable/ungated — no filesystem or engine call here, just a `docs` map
/// mutation and the same [`opened_document`] DTO refresh every other
/// mutating command in this crate ends with.
pub fn mark_saved<E: Engine>(
    state: &SessionState<E>,
    handle: DocHandle,
) -> Result<OpenedDocumentInfo, SessionError> {
    let path = {
        let mut docs_guard = state.docs.lock().expect("docs lock poisoned");
        let open_doc = resolve_doc_mut(&mut docs_guard, handle)?;
        open_doc.dirty = false;
        open_doc.path.clone()
    };
    opened_document(&state.engine, &state.docs, &state.history, &path, handle)
}

/// Closes `handle`'s document: closes the engine-side handle and drops
/// this crate's own `docs` bookkeeping entry for it — the same two steps
/// the desktop's `close_document` Tauri command and `WasmSession::close_document`
/// used to do inline, independently of each other, before this function
/// existed — **plus** the cleanup neither of those ever did: removing the
/// closed document's [`WorkingStore`] entry and [`DocHistory`] entry too.
///
/// That gap is C1 of Phase 2's final whole-plan review, confirmed
/// empirically: [`open_document_bytes`] keys a document's working-copy
/// bytes and undo/redo history by its working `path` — before this
/// function existed, closing a document never removed either, so a
/// *different* document later opened under a working key that happened to
/// collide (same `display_name`, reused before [`NEXT_BYTES_WORKING_ID`]
/// existed to prevent it) would silently inherit the closed document's
/// leftover `WorkingStore` bytes and `DocHistory` undo/redo stacks — a
/// fresh document opening with `can_undo: true`, and Undo swapping in a
/// *different* document's bytes. [`NEXT_BYTES_WORKING_ID`] (see its own
/// doc) kills the collision going forward by minting a unique key per
/// open; this function kills the other half of the same bug, the leak
/// itself — a long editing session that opens and closes many documents
/// one at a time would otherwise grow `store`/`history` without bound,
/// even with collisions no longer possible.
///
/// Order matters, and matches the doc comment above: read `path` out of
/// `docs` and remove the entry *before* touching `store`/`history` (their
/// removal needs the very `path` the `docs` entry is keyed differently
/// than — `docs` is keyed by [`DocHandle`], `store`/`history` by `path`),
/// then `engine.close(handle)` (same as the two call sites' pre-existing
/// order: desktop's old `close_document` closed the engine before
/// touching `docs`; here the `docs` removal happens first purely because
/// it's also where `path` comes from — the engine-side close has no
/// ordering dependency on it either way). A handle with no `docs` entry
/// (an unknown/already-closed handle) still safely no-ops the
/// `store`/`history` cleanup — there's no `path` to clean up with — while
/// still calling `engine.close(handle)`, matching the old desktop
/// command's unconditional `state.engine.close(handle)` (best-effort,
/// like [`WorkingStore::remove`] itself).
///
/// **`store.remove` is skipped when `path == original_path`** (`history`
/// removal is *not* — it's always safe, since a `DocHistory` entry is
/// never anything but this crate's own byte snapshots, never the user's
/// file). This is NEW-C1 from the fix-wave re-review, found immediately
/// after this function first landed: `pages::open_new_file` (the shared
/// tail of merge/extract) constructs its `OpenDoc` with `path` **equal
/// to** `original_path` — the user's chosen destination file, e.g.
/// `~/Documents/merged.pdf` — because a merge/extract result is already
/// the saved output, with no scratch copy ever made (see that function's
/// own doc: "the file on disk already *is* the saved result"). Every
/// *other* `OpenDoc` construction site gives `path` a scratch-copy
/// identity distinct from `original_path`:
/// [`OpenDoc::open_with_working_copy`] (desktop) copies into a `temp_dir`
/// scratch file; [`open_document_bytes`] (wasm) suffixes `path` with
/// [`NEXT_BYTES_WORKING_ID`] specifically so it can never equal the bare
/// `original_path`. So `path == original_path` is exactly the "this
/// OpenDoc has no scratch copy — `store`'s entry for this key, if any, is
/// actually just an alias for the user's real file, not a disposable
/// working copy" signal, and discriminates all three construction sites
/// correctly. Without this guard, closing a merge/extract result (e.g.
/// opening a different document right after a merge, which Phase 2's I3
/// fix lets happen without even a prompt, since a freshly-merged document
/// is clean) would call `FsWorkingStore::remove` — a bare
/// `std::fs::remove_file` — directly on the user's merged/extracted PDF,
/// deleting it, with the resulting `io::Error` silently discarded (`remove`
/// is documented best-effort/infallible by design). `pages::open_new_file`
/// was also fixed directly (see its own doc): it now routes through
/// [`OpenDoc::open_with_working_copy`], so a merge/extract result gets a
/// real scratch copy and `path == original_path` no longer actually
/// happens anywhere in this crate. This predicate stays regardless —
/// belt-and-suspenders defense-in-depth against a *future* construction
/// site making the same mistake `open_new_file` did, at zero cost to
/// every construction site that already gets it right.
pub fn close_document_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
) {
    let removed = docs.lock().expect("docs lock poisoned").remove(&handle);
    engine.close(handle);
    if let Some(open_doc) = removed {
        // Only remove the store entry when `path` is a genuine scratch
        // copy distinct from the user's file — see this function's doc
        // for exactly why `path == original_path` means "no scratch copy
        // exists, this key aliases the user's real file."
        if open_doc.path != open_doc.original_path {
            store.remove(&open_doc.path);
        }
        history
            .lock()
            .expect("history lock poisoned")
            .remove(&open_doc.path);
    }
}

/// Shared across every test in this crate — not one engine per test.
/// PDFium's global init may only run once per process, and cargo runs
/// every `#[test]` fn in one binary's process, concurrently by default;
/// see `openpdfedit-engine`'s `thread.rs` test module for the full story
/// (two independent bindings in one process segfault).
#[cfg(test)]
mod test_support {
    use openpdfedit_engine::EngineHandle;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    pub(crate) fn shared_handle() -> Option<&'static EngineHandle> {
        static HANDLE: OnceLock<Option<EngineHandle>> = OnceLock::new();
        HANDLE
            .get_or_init(
                || match EngineHandle::spawn(dev_vendor_lib_dir_for_tests()) {
                    Ok(h) => Some(h),
                    Err(e) => {
                        eprintln!("skipping: PDFium not available ({e})");
                        None
                    }
                },
            )
            .as_ref()
    }

    fn dev_vendor_lib_dir_for_tests() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // crates/openpdfedit-session -> workspace root
        let workspace_root = manifest_dir.parent()?.parent()?;
        let dir = workspace_root.join(if cfg!(windows) {
            ".vendor/pdfium/bin"
        } else {
            ".vendor/pdfium/lib"
        });
        dir.exists().then_some(dir)
    }

    pub(crate) fn test_corpus_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/minimal.pdf")
    }

    /// Hand-built single-page PDF (empty `BT`/`ET` content stream, no
    /// text/fonts) — the "just needs *a* valid document" fixture shared by
    /// [`crate::annotations::tests`] and [`crate::pages::tests`], which
    /// had byte-identical private copies before this was pulled out.
    /// `forms::tests` has its own similarly-named `minimal_pdf_bytes` that
    /// is **not** a duplicate of this one (no content stream at all) — its
    /// AcroForm-focused tests don't need a content stream and building one
    /// there would be dead weight, so it stays local rather than being
    /// folded in here as a third variant of one name.
    pub(crate) fn minimal_pdf_bytes() -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
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

    /// Hand-built single-page PDF with one real Helvetica text run at
    /// `(x, y)` in `font_size`-pt type — the fixture shared by
    /// [`crate::annotations::tests`] and [`crate::redact::tests`], which
    /// had byte-identical private copies (same signature, same body) before
    /// this was pulled out. `compare::tests` and `textedit::tests` have
    /// their own same-*named* helpers that are **not** duplicates of this
    /// one — different fixed geometry/font baked in, no `Encoding` key, a
    /// different `MediaBox` — so those stay local rather than being forced
    /// into this one shared signature.
    pub(crate) fn text_page_pdf_bytes(text: &str, x: f64, y: f64, font_size: f64) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), font_size.into()]),
                Operation::new("Td", vec![x.into(), y.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
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

    /// Shared no-tmp-left-behind check for [`crate::FsWorkingStore::write`]
    /// tests, in both this crate's own `lib.rs` test module and
    /// `forms::tests`. `FsWorkingStore::write`'s tmp sibling is per-*call*
    /// unique (`{key}.openpdfedit-tmp-{n}` via `NEXT_TMP_WRITE_ID`, not a
    /// single deterministic `{key}.openpdfedit-tmp` — see that method's own
    /// doc for why: a fixed name would let two concurrent writers on the
    /// same key stomp each other's tmp file mid-write), so a caller can't
    /// check one hardcoded suffix the way the pre-I1-re-review version of
    /// these tests did — this scans `key`'s parent directory for any entry
    /// whose name starts with `{key}.openpdfedit-tmp-`, whatever `n` the
    /// write actually minted.
    pub(crate) fn any_tmp_sibling_exists(key: &std::path::Path) -> bool {
        let prefix = {
            let mut p = key.as_os_str().to_owned();
            p.push(".openpdfedit-tmp-");
            p
        };
        let Some(dir) = key.parent() else {
            return false;
        };
        std::fs::read_dir(dir)
            .expect("temp dir should be readable")
            .filter_map(|e| e.ok())
            .any(|e| {
                e.path()
                    .into_os_string()
                    .to_string_lossy()
                    .starts_with(prefix.to_string_lossy().as_ref())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::{shared_handle, test_corpus_path};

    /// Step 2's round-trip test: open bytes -> page_count via state ->
    /// save-to-bytes -> reopen -> page count matches. Exercises
    /// `open_document_bytes` (and therefore `opened_document`) through a
    /// real `SessionState`, then the raw engine for the save/reopen half
    /// (mutation is out of scope for this task).
    #[test]
    fn open_bytes_then_save_to_bytes_then_reopen_page_count_matches() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }
        let bytes = std::fs::read(&corpus).expect("read fixture");

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        let opened = open_document_bytes(&state, "minimal.pdf", bytes)
            .expect("open_document_bytes should succeed");
        assert!(opened.page_count >= 1);
        assert!(!opened.can_undo);
        assert!(!opened.can_redo);
        assert!(!opened.is_dirty);
        assert_eq!(opened.file_path, "minimal.pdf");

        let saved = state
            .engine
            .save_to_bytes(opened.handle)
            .expect("save_to_bytes should succeed");
        let reopened_handle = state
            .engine
            .open_bytes(saved)
            .expect("reopening saved bytes should succeed");
        let reopened_count = state
            .engine
            .page_count(reopened_handle)
            .expect("page_count should succeed");
        assert_eq!(
            reopened_count, opened.page_count,
            "page count must survive a save-to-bytes/reopen round trip"
        );

        state.engine.close(reopened_handle);
        state.engine.close(opened.handle);
    }

    /// The compress export end-to-end: grow the working copy with two
    /// real incremental-save mutations (watermarks), then
    /// `compress_document_to_path_impl` must write a strictly smaller,
    /// reparseable copy with the page count intact — and must not touch
    /// the open document (same handle, still dirty-tracked as before).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn compress_document_writes_a_smaller_reparseable_copy_without_mutating_the_doc() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }
        let bytes = std::fs::read(&corpus).expect("read fixture");

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };
        let opened =
            open_document_bytes(&state, "compress-me.pdf", bytes).expect("open should succeed");

        // Two watermark passes: each goes through commit_mutation's
        // incremental save, so the working copy grows twice — exactly the
        // revision-chain fat the compress rewrite exists to shed.
        let mut handle = opened.handle;
        for text in ["DRAFT", "CONFIDENTIAL"] {
            let info = crate::watermark::apply_watermark_impl(
                &state.engine,
                &state.docs,
                &state.history,
                &*state.store,
                crate::watermark::ApplyWatermarkRequest {
                    handle,
                    text: text.into(),
                    location: "full".into(),
                    orientation_deg: 45,
                    opacity: 0.3,
                    text_scale: 1.0,
                    logo_rgba_base64: None,
                    logo_width: None,
                    logo_height: None,
                    pages: None,
                },
            )
            .expect("watermark should succeed");
            handle = info.handle;
        }

        let out_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-compress-test-{}.pdf",
            std::process::id()
        ));
        let stats = compress_document_to_path_impl(&state, handle, &out_path)
            .expect("compress should succeed");

        assert!(
            stats.after_bytes < stats.before_bytes,
            "the rewrite must shed the incremental revision chain: before={} after={}",
            stats.before_bytes,
            stats.after_bytes
        );
        let written = std::fs::read(&out_path).expect("output file must exist");
        assert_eq!(written.len() as u64, stats.after_bytes);
        let reopened = lopdf::Document::load_mem(&written).expect("output must reparse");
        assert_eq!(
            reopened.get_pages().len() as u32,
            opened.page_count,
            "page count must survive compression"
        );

        // Export, not mutation: the open doc's handle is still live and
        // unrotated.
        assert!(state.engine.page_count(handle).is_ok());
        state.engine.close(handle);
        let _ = std::fs::remove_file(&out_path);
    }

    // Not `#[cfg(not(target_arch = "wasm32"))]`: plain `lopdf` byte
    // building, no filesystem or platform-specific code — used by both
    // the fs-backed tests below (which are gated, since they also touch
    // real temp files) and the portable `MemWorkingStore` test right
    // after it (which isn't).
    fn build_three_page_pdf() -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![Operation::new("BT", vec![]), Operation::new("ET", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let mut kids = Vec::new();
        for _ in 0..3 {
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {},
            });
            kids.push(page_id.into());
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => 3,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    /// This task's Step-1 test: the same round trip as
    /// `undo_and_redo_round_trip_through_real_edits` below (open -> real
    /// page-delete edit -> undo -> redo, checked via `page_count` at every
    /// step), but sourced from in-memory `bytes` via
    /// [`open_document_bytes`] and driven entirely through
    /// [`MemWorkingStore`] instead of [`FsWorkingStore`] — no temp files,
    /// no `std::fs` call, anywhere in this test. Proves the
    /// `WorkingStore`-routed `commit_mutation`/`undo_impl`/`redo_impl`
    /// pathway genuinely works without a filesystem, not just that it
    /// compiles for wasm32.
    #[test]
    fn open_bytes_then_commit_mutation_then_undo_then_redo_all_through_mem_working_store() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        let opened =
            open_document_bytes(&state, "mem-working-store-test.pdf", build_three_page_pdf())
                .expect("open_document_bytes should succeed");
        assert_eq!(opened.page_count, 3);
        assert!(!opened.can_undo);
        assert!(!opened.can_redo);
        assert!(
            !opened.is_dirty,
            "a freshly-opened document must start clean"
        );

        // Edit: delete the last page. 3 -> 2 pages.
        let after_edit = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            opened.handle,
            |doc| doc.delete_page(2).map_err(Into::into),
        )
        .expect("commit_mutation through MemWorkingStore should succeed");
        assert_eq!(after_edit.page_count, 2);
        assert!(after_edit.can_undo);
        assert!(!after_edit.can_redo);
        assert!(
            after_edit.is_dirty,
            "a mutation must dirty the working copy, even for a byte-opened document — \
             reopen_after_write (shared by commit_mutation/undo_impl/redo_impl) always inserts \
             the rotated OpenDoc with dirty: true, regardless of desktop vs. wasm"
        );

        // Undo: back to 3 pages.
        let after_undo = undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_edit.handle,
        )
        .expect("undo_impl through MemWorkingStore should succeed");
        assert_eq!(after_undo.page_count, 3);
        assert!(!after_undo.can_undo);
        assert!(after_undo.can_redo);
        // Undo still rotates through reopen_after_write, so the working
        // copy is (correctly) still dirty relative to the original bytes
        // — undoing an edit isn't the same thing as saving.
        assert!(after_undo.is_dirty);

        // Redo: forward to 2 pages again.
        let after_redo = redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo.handle,
        )
        .expect("redo_impl through MemWorkingStore should succeed");
        assert_eq!(after_redo.page_count, 2);
        assert!(after_redo.can_undo);
        assert!(!after_redo.can_redo);
        assert!(after_redo.is_dirty);

        state.engine.close(after_redo.handle);
    }

    /// This task's (Task 2) dedicated dirty-tracking test: a byte-opened
    /// document starts clean, a mutation dirties it (already covered above,
    /// asserted again here for a test whose name says so directly), and
    /// [`mark_saved`] — the wasm-safe counterpart of what
    /// `save_document_impl` does inline for the desktop's `save_document`
    /// command — takes it back to clean. This is the exact sequence
    /// `WasmSession::saveToBytes` + `wasm.ts`'s `FileSystemFileHandle`
    /// write + `WasmSession::mark_saved` will drive once a mutation surface
    /// exists on `WasmSession` itself; [`commit_mutation`] stands in here
    /// for that not-yet-built wasm-facing mutating command, exactly as it
    /// does in the test above.
    #[test]
    fn byte_opened_doc_becomes_dirty_after_mutation_and_clean_after_mark_saved() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        let opened = open_document_bytes(&state, "dirty-test.pdf", build_three_page_pdf())
            .expect("open_document_bytes should succeed");
        assert!(
            !opened.is_dirty,
            "a freshly-opened document must start clean"
        );

        let after_edit = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            opened.handle,
            |doc| doc.delete_page(2).map_err(Into::into),
        )
        .expect("commit_mutation through MemWorkingStore should succeed");
        assert!(
            after_edit.is_dirty,
            "a mutation must dirty the working copy"
        );

        let after_save = mark_saved(&state, after_edit.handle).expect("mark_saved should succeed");
        assert!(
            !after_save.is_dirty,
            "mark_saved must clear the dirty flag, mirroring save_document_impl's own \
             d.dirty = false"
        );
        // mark_saved is a pure docs-map bookkeeping update (no engine/store
        // call, no reopen) — unlike commit_mutation/undo_impl/redo_impl,
        // the handle must NOT rotate.
        assert_eq!(after_save.handle, after_edit.handle);

        // Independent re-derivation of the DTO confirms the clean state is
        // real session state (the `docs` map), not just this call's return
        // value.
        let refreshed = opened_document(
            &state.engine,
            &state.docs,
            &state.history,
            &PathBuf::from("dirty-test.pdf"),
            after_save.handle,
        )
        .expect("opened_document should succeed");
        assert!(!refreshed.is_dirty);

        state.engine.close(after_save.handle);
    }

    #[test]
    fn mark_saved_on_unknown_handle_errors() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        match mark_saved(&state, 999) {
            Err(SessionError::UnknownHandle(999)) => {}
            Err(other) => panic!("expected SessionError::UnknownHandle(999), got: {other}"),
            Ok(_) => panic!("an unknown handle must error, not succeed"),
        }
    }

    /// Phase 2 final-review C1's regression test: without this fix round,
    /// this reproduced the bug empirically (a freshly-opened second
    /// "doc.pdf" reported `can_undo: true`, and `undo_impl` swapped in the
    /// *first* "doc.pdf"'s pre-edit bytes) — two documents opened under the
    /// same `display_name`, the second opened only *after* the first was
    /// closed via the new [`close_document_impl`], must not alias each
    /// other's `WorkingStore` bytes or `DocHistory` stacks.
    ///
    /// Sequence: open "doc.pdf" (doc A) -> mutate it (so it has undo
    /// history and dirty working-copy bytes) -> close it through
    /// [`close_document_impl`] -> open *different* bytes, again under
    /// "doc.pdf" (doc B). Doc B must start with `can_undo: false`, and
    /// `undo_impl` against it must error ("nothing to undo") rather than
    /// silently restoring doc A's leftover snapshot.
    #[test]
    fn close_then_reopen_same_display_name_does_not_alias_history_or_working_copy() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        // Doc A: 3 pages, then delete one (3 -> 2) so it has real undo
        // history and a dirty working copy.
        let doc_a = open_document_bytes(&state, "doc.pdf", build_three_page_pdf())
            .expect("open_document_bytes(doc A) should succeed");
        let doc_a_after_edit = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            doc_a.handle,
            |doc| doc.delete_page(2).map_err(Into::into),
        )
        .expect("commit_mutation on doc A should succeed");
        assert!(
            doc_a_after_edit.can_undo,
            "doc A must have real undo history before it's closed"
        );

        // Close doc A through the new shared close path — this is the
        // fix's other half: without it, doc A's WorkingStore/DocHistory
        // entries would still be sitting under the "doc.pdf#<n>" key doc B
        // is about to reuse... except doc B gets a *different* unique key
        // now (NEXT_BYTES_WORKING_ID), so the real point of closing first
        // is proving close_document_impl's cleanup doesn't itself break
        // anything in this sequence, and that doc B's own fresh state is
        // never accidentally satisfied by a leftover doc A entry.
        close_document_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            doc_a_after_edit.handle,
        );

        // Doc B: same display_name ("doc.pdf"), genuinely different bytes
        // (a fresh 3-page PDF built independently — different object IDs/
        // byte layout than doc A's, even though the page count matches).
        let doc_b = open_document_bytes(&state, "doc.pdf", build_three_page_pdf())
            .expect("open_document_bytes(doc B) should succeed");

        assert_eq!(
            doc_b.file_path, "doc.pdf",
            "doc B's user-visible file_path must still be the bare display_name, unsuffixed"
        );
        assert!(
            !doc_b.can_undo,
            "doc B must start with no undo history of its own — got can_undo: true, which means \
             it aliased doc A's leftover DocHistory entry"
        );
        assert!(
            !doc_b.is_dirty,
            "a freshly-opened document must start clean"
        );

        match undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            doc_b.handle,
        ) {
            Err(SessionError::Doc(msg)) => assert!(
                msg.contains("nothing to undo"),
                "expected a 'nothing to undo' error, got: {msg}"
            ),
            Err(other) => panic!("expected SessionError::Doc(\"nothing to undo\"), got: {other}"),
            Ok(info) => panic!(
                "undo_impl on doc B must error, not succeed — it succeeded and returned \
                 page_count={}, which means it restored a snapshot doc B never pushed (almost \
                 certainly doc A's, aliased through a colliding working key)",
                info.page_count
            ),
        }

        state.engine.close(doc_b.handle);
    }

    /// Fix-wave re-review's NEW-m2: strengthens the C1 coverage above by
    /// pinning down *part 1* (the unique working key,
    /// [`NEXT_BYTES_WORKING_ID`]) in isolation from part 2
    /// ([`close_document_impl`]'s cleanup). The test above always closes
    /// doc A before opening doc B, so on its own it can't tell "the
    /// collision never happens" apart from "the collision happens but
    /// closing doc A first happens to clean it up in time" — this test
    /// opens doc B under the same `display_name` while doc A is **still
    /// open**, with `close_document_impl` never called at all, which only
    /// part 1's unique-key minting can possibly get right. Doc A's own
    /// undo history must also survive doc B's existence untouched — proof
    /// the two documents' `DocHistory` entries are genuinely independent
    /// entries, not merely "doc B's read happened not to see doc A's
    /// write yet."
    #[test]
    fn opening_two_docs_under_the_same_display_name_without_closing_the_first_does_not_alias_history(
    ) {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(MemWorkingStore::default()),
        };

        // Doc A: open, then a real edit (3 -> 2 pages) so it has real
        // undo history — and is deliberately left open for the rest of
        // this test.
        let doc_a = open_document_bytes(&state, "doc.pdf", build_three_page_pdf())
            .expect("open_document_bytes(doc A) should succeed");
        let doc_a_after_edit = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            doc_a.handle,
            |doc| doc.delete_page(2).map_err(Into::into),
        )
        .expect("commit_mutation on doc A should succeed");
        assert!(
            doc_a_after_edit.can_undo,
            "doc A must have real undo history before doc B is opened"
        );

        // Doc B: same display_name, opened while doc A is still open —
        // close_document_impl is never called anywhere in this test.
        let doc_b = open_document_bytes(&state, "doc.pdf", build_three_page_pdf())
            .expect("open_document_bytes(doc B) should succeed");
        assert_eq!(doc_b.file_path, "doc.pdf");
        assert!(
            !doc_b.can_undo,
            "doc B must not inherit doc A's undo history merely by sharing a display_name — this \
             is exactly what a colliding (non-unique) working key would produce"
        );

        // Doc A's own undo must still work — its DocHistory entry must be
        // a genuinely separate entry from doc B's, not the same one doc B
        // happened not to disturb yet.
        let doc_a_after_undo = undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            doc_a_after_edit.handle,
        )
        .expect("undo on doc A must still work after doc B opened under the same display_name");
        assert_eq!(
            doc_a_after_undo.page_count, 3,
            "undo must restore doc A's own pre-edit bytes, unaffected by doc B"
        );

        state.engine.close(doc_a_after_undo.handle);
        state.engine.close(doc_b.handle);
    }

    /// Direct, minimal-setup unit test for `close_document_impl`'s
    /// `path == original_path` guard (NEW-C1 from the fix-wave re-review —
    /// see that function's own doc for the full story: `path ==
    /// original_path` means "no real scratch copy exists," and
    /// `store.remove` must be skipped or it deletes the user's real file).
    /// `pages::merge_documents_impl_result_gets_a_real_scratch_copy_and_saves_correctly`
    /// exercises the same guard end-to-end through real merge/extract
    /// machinery; this test isolates the guard itself by constructing an
    /// `OpenDoc` the same shape `pages::open_new_file` used to build
    /// *before* this fix wave's part 2 (`path == original_path`, real
    /// on-disk file, no scratch copy at all) directly — so this still
    /// catches a regression in `close_document_impl` even if some future
    /// change stops `open_new_file` from ever exercising this path again.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn close_document_impl_never_deletes_a_file_when_path_equals_original_path() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-close-no-scratch-copy-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, build_three_page_pdf()).expect("should write temp file");

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
            },
        );
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        close_document_impl(engine, &docs, &history, &FsWorkingStore, handle);

        assert!(
            tmp_path.exists(),
            "close_document_impl must never delete a file whose OpenDoc had no real scratch copy \
             (path == original_path) — that shape means this key aliases the user's real file, \
             not a disposable working copy"
        );

        let _ = std::fs::remove_file(&tmp_path);
    }

    /// Real, end-to-end undo/redo through the moved functions: two real
    /// edits (page deletions, checked via `page_count`), asserted at
    /// every step, plus the boundary cases (undoing/redoing past the end
    /// of history must error, not panic) and `can_undo`/`can_redo`
    /// tracked throughout. Equivalent to the desktop's
    /// `undo_and_redo_round_trip_through_real_edits`, moved here — see
    /// this module's doc comment for why the mutation path differs.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn undo_and_redo_round_trip_through_real_edits() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-undo-redo-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, build_three_page_pdf()).expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");
        let doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
        let mut docs = HashMap::new();
        docs.insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
            },
        );
        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(docs),
            history: Mutex::new(HashMap::new()),
            store: Box::new(FsWorkingStore),
        };

        // Freshly opened: nothing to undo or redo yet.
        let opened = opened_document(
            &state.engine,
            &state.docs,
            &state.history,
            &tmp_path,
            handle,
        )
        .unwrap();
        assert!(!opened.can_undo);
        assert!(!opened.can_redo);
        assert_eq!(opened.page_count, 3);

        // Edit 1: delete the last page. 3 -> 2 pages.
        let after_edit1 = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            handle,
            |doc| doc.delete_page(2).map_err(Into::into),
        )
        .unwrap();
        assert_eq!(after_edit1.page_count, 2);
        assert!(after_edit1.can_undo);
        assert!(!after_edit1.can_redo);

        // Edit 2: delete another page. 2 -> 1 page.
        let after_edit2 = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_edit1.handle,
            |doc| doc.delete_page(1).map_err(Into::into),
        )
        .unwrap();
        assert_eq!(after_edit2.page_count, 1);
        assert!(after_edit2.can_undo);
        assert!(!after_edit2.can_redo);

        // Undo #1: back to 2 pages. Redo becomes available.
        let after_undo1 = undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_edit2.handle,
        )
        .unwrap();
        assert_eq!(
            after_undo1.page_count, 2,
            "undo must restore the pre-edit-2 byte snapshot"
        );
        assert!(
            after_undo1.can_undo,
            "edit 1 is still further back in history"
        );
        assert!(
            after_undo1.can_redo,
            "edit 2 was just undone, so it's redoable"
        );

        // Undo #2: back to 3 pages. Nothing left to undo.
        let after_undo2 = undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo1.handle,
        )
        .unwrap();
        assert_eq!(
            after_undo2.page_count, 3,
            "undo must restore the pristine pre-edit-1 file"
        );
        assert!(!after_undo2.can_undo, "both edits have been undone");
        assert!(after_undo2.can_redo);

        // Undoing past the beginning of history must error cleanly.
        assert!(undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo2.handle
        )
        .is_err());

        // Redo #1: forward to 2 pages again.
        let after_redo1 = redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo2.handle,
        )
        .unwrap();
        assert_eq!(after_redo1.page_count, 2);
        assert!(after_redo1.can_undo);
        assert!(
            after_redo1.can_redo,
            "edit 2 is still ahead in the redo stack"
        );

        // Redo #2: forward to 1 page again — back to the fully-edited state.
        let after_redo2 = redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_redo1.handle,
        )
        .unwrap();
        assert_eq!(after_redo2.page_count, 1);
        assert!(after_redo2.can_undo);
        assert!(!after_redo2.can_redo, "nothing left to redo");

        // Redoing past the end of history must also error cleanly.
        assert!(redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_redo2.handle
        )
        .is_err());

        state.engine.close(after_redo2.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    /// A fresh edit after an undo must drop the now-stale redo entry.
    /// Equivalent to the desktop's
    /// `a_new_edit_after_undo_clears_the_redo_stack`, moved here.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_new_edit_after_undo_clears_the_redo_stack() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-undo-redo-branch-test-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, build_three_page_pdf()).expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");
        let doc = Document::open(&tmp_path).expect("doc crate should open the temp file");
        let mut docs = HashMap::new();
        docs.insert(
            handle,
            OpenDoc {
                path: tmp_path.clone(),
                original_path: tmp_path.clone(),
                dirty: false,
                doc,
            },
        );
        let state = SessionState {
            engine: engine.clone(),
            docs: Mutex::new(docs),
            history: Mutex::new(HashMap::new()),
            store: Box::new(FsWorkingStore),
        };

        let after_edit1 = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            handle,
            |doc| doc.delete_page(2).map_err(Into::into),
        )
        .unwrap();
        let after_undo1 = undo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_edit1.handle,
        )
        .unwrap();
        assert!(
            after_undo1.can_redo,
            "the just-undone edit should be redoable"
        );

        let after_edit2 = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_undo1.handle,
            |doc| doc.delete_page(1).map_err(Into::into),
        )
        .unwrap();
        assert!(
            !after_edit2.can_redo,
            "a fresh edit must invalidate the old redo trail"
        );
        assert!(redo_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            after_edit2.handle
        )
        .is_err());

        state.engine.close(after_edit2.handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    // --- sharing-violation retry helpers: moved verbatim from the
    // desktop crate, which tested these directly (no engine involved). ---

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sharing_violation_message_names_the_locked_file() {
        let msg = sharing_violation_message(Path::new("/tmp/report.pdf"));
        assert!(msg.contains("report.pdf"), "{msg}");
        assert!(
            !msg.contains("os error"),
            "{msg} still leaks the raw OS string"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn with_sharing_violation_retry_passes_through_a_non_sharing_error_immediately() {
        let mut calls = 0;
        let result = with_sharing_violation_retry(|| {
            calls += 1;
            Err::<(), _>(std::io::Error::new(std::io::ErrorKind::NotFound, "nope"))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn with_sharing_violation_retry_returns_ok_once_the_lock_clears() {
        let mut calls = 0;
        let result = with_sharing_violation_retry(|| {
            calls += 1;
            Ok::<_, std::io::Error>(42)
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 1);
    }

    #[cfg(all(not(target_arch = "wasm32"), windows))]
    #[test]
    fn with_sharing_violation_retry_retries_a_real_sharing_violation_then_succeeds() {
        let mut calls = 0;
        let result = with_sharing_violation_retry(|| {
            calls += 1;
            if calls < 3 {
                Err(std::io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        });
        assert!(result.is_ok());
        assert_eq!(
            calls, 3,
            "should have retried past the first two sharing violations"
        );
    }

    #[cfg(all(not(target_arch = "wasm32"), windows))]
    #[test]
    fn with_sharing_violation_retry_gives_up_after_the_last_configured_delay() {
        let mut calls = 0;
        let result = with_sharing_violation_retry(|| {
            calls += 1;
            Err::<(), _>(std::io::Error::from_raw_os_error(32))
        });
        assert!(result.is_err());
        assert_eq!(calls, 5);
    }

    /// Fix-wave re-review's I1: direct, engine-free unit test of
    /// `FsWorkingStore::write`'s write-tmp-then-rename, exercising it in
    /// isolation from any mutating command (unlike
    /// `forms::fill_form_fields_impl_fills_saves_and_rotates_the_handle`'s
    /// no-tmp-left-behind assertion, which proves the same property but
    /// only as a side effect of a real fill+save). A first write (create),
    /// a second write (overwrite existing content — the case that matters:
    /// `std::fs::write` alone truncates-then-rewrites in place here, which
    /// is exactly the failure window write-tmp-then-rename closes), and
    /// both leave `read` returning the just-written bytes with no
    /// `.openpdfedit-tmp` sibling left on disk afterwards.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn fs_working_store_write_leaves_no_tmp_sibling_and_read_reflects_it() {
        // `std::process::id()` alone is enough for uniqueness here — this
        // filename is only ever touched by this one `#[test]` fn, unlike
        // [`NEXT_BYTES_WORKING_ID`]/[`NEXT_TMP_WRITE_ID`] (each minted many
        // times over a document's/write's lifetime for a different
        // purpose); reusing either of those counters just to build a
        // once-per-test-run scratch filename was flagged as a naming wart
        // by the fix-wave re-review — this matches every other test in
        // this file's own `std::process::id()`-only convention instead
        // (e.g. `fill_form_fields_impl_fills_saves_and_rotates_the_handle`'s
        // `tmp_path`).
        let key = std::env::temp_dir().join(format!(
            "openpdfedit-session-fswork-store-write-test-{}.bin",
            std::process::id()
        ));

        let store = FsWorkingStore;

        store
            .write(&key, b"first")
            .expect("first write should succeed");
        assert_eq!(store.read(&key).expect("read after first write"), b"first");
        assert!(
            !test_support::any_tmp_sibling_exists(&key),
            "a successful write must not leave its tmp sibling behind"
        );

        // Overwrite with different (and differently-sized) content — the
        // case a bare in-place `std::fs::write` truncates for before
        // writing the new bytes, which is exactly the partial-write window
        // this method's tmp+rename exists to close.
        store
            .write(&key, b"second, longer content")
            .expect("second write should succeed");
        assert_eq!(
            store.read(&key).expect("read after second write"),
            b"second, longer content"
        );
        assert!(
            !test_support::any_tmp_sibling_exists(&key),
            "a successful overwrite must not leave its tmp sibling behind either"
        );

        let _ = std::fs::remove_file(&key);
    }

    /// Test-only [`WorkingStore`] wrapper for
    /// [`concurrent_rotate_and_undo_redo_on_same_document_do_not_corrupt_state`]
    /// below, that directly detects whether two `write` calls are ever
    /// in flight at the same time — the exact property Phase 4 Task 1's
    /// `docs`-lock discipline exists to guarantee.
    ///
    /// This exists because the more "black box" invariants that test
    /// first reached for (final `page_count`, history-stack depth) don't
    /// actually distinguish serialized writes from interleaved ones for
    /// *this* crate's specific shapes, confirmed empirically while
    /// writing this test: [`MemWorkingStore::write`] is a single atomic
    /// `HashMap::insert`, so an interleaved-but-lost write never produces
    /// a torn/partial read (there's no byte-level corruption *to*
    /// detect that way), page rotation never changes `page_count`, and
    /// this crate's handle-rotation design (a fresh [`DocHandle`] per
    /// write, old one discarded) tolerates two overlapping writers each
    /// completing and rotating independently without erroring either.
    /// Reverting `undo_impl`/`redo_impl` to their pre-Phase-4-Task-1
    /// shape (every lock released before `store.write`) and rerunning
    /// the page-count/history-depth-only version of this test 200 rounds
    /// deep with an artificial delay never once failed — silent,
    /// undetectable-by-those-invariants desync between what `history`
    /// records and what `store` actually holds, not a crash. This
    /// wrapper closes that gap by observing `write()` itself: `in_flight`
    /// flags whether a write is currently executing, and a short `sleep`
    /// inside the call (after setting the flag, before delegating to the
    /// real store) widens the window a genuinely concurrent second
    /// `write()` would land in, making an actual overlap *more likely* to
    /// be observed within this test's modest round count than relying on
    /// incidental OS scheduling alone.
    ///
    /// **This is a probabilistic best-effort detector, not a guarantee —
    /// a green run is evidence, not proof.** Measured directly (10 runs
    /// each, same machine) against the same pre-fix `undo_impl`/
    /// `redo_impl` revert described above: 3/10 and 4/10 runs tripped
    /// `overlap_detected` across separate measurement sessions, plus one
    /// independent reviewer measurement of 3/8. An earlier draft of this
    /// comment claimed the trip was reliable "every run" — false; that
    /// claim came from two small, favorably-selected samples (4/5, then
    /// 5/8) rather than a properly repeated measurement, and did not
    /// survive being checked again. Two consequences follow from the
    /// true rate being roughly a coin flip, not near-certain: (1) this
    /// test passing on a given run is *not*, by itself, strong evidence
    /// the discipline is intact — the primary evidence for that is the
    /// lock-order trace in `undo_impl`'s doc, this test is corroborating,
    /// not load-bearing; (2) a *regression* that reintroduced the pre-fix
    /// bug would only have a rough coin-flip chance of being caught by
    /// any single CI run of this test, not a near-certainty — acceptable
    /// for this task's "best-effort deterministic interleaving" brief,
    /// but worth stating precisely rather than overselling.
    ///
    /// `in_flight`/`overlap_detected` are **store-global, not per-key** —
    /// a single flag shared across every `key` this store is ever asked
    /// to write, not a `HashMap<PathBuf, AtomicBool>` keyed the way
    /// [`MemWorkingStore`]'s own map is. That's only valid as a check for
    /// "no two writes to *the same document's key* overlapped" because
    /// the test below opens exactly one document and never writes any
    /// other key through this store — if a future test reused this probe
    /// across multiple concurrently-written documents, a real overlap
    /// between two *different* keys' writes would also trip
    /// `overlap_detected`, which this struct cannot distinguish from a
    /// same-key overlap. (That specific false-positive risk is currently
    /// moot for correctness, not just for this test: this crate's chosen
    /// fix (a) serializes *all* writes through the single global `docs`
    /// lock regardless of key, so today a same-store cross-key overlap
    /// genuinely can't happen either — see `undo_impl`'s doc. But this
    /// struct's own detection logic doesn't know that; it would need a
    /// per-key map to make the "same key" claim on its own evidence
    /// rather than by leaning on that separate, external fact.
    /// [`open_document_bytes`]'s write is a concrete instance of exactly
    /// this false-positive shape, on the other side of the same coin: it
    /// writes to a fresh, per-call-unique key *outside* `docs.lock()` by
    /// construction, so racing it against another write through the same
    /// `Arc<WriteOverlapProbe>` instance would trip `overlap_detected`
    /// even though no lost update is possible — a different key, not a
    /// same-key race. Not exercised by the test below, which never calls
    /// `open_document_bytes` concurrently with anything else.)
    struct WriteOverlapProbe {
        inner: MemWorkingStore,
        in_flight: std::sync::atomic::AtomicBool,
        overlap_detected: std::sync::atomic::AtomicBool,
    }

    impl WriteOverlapProbe {
        fn new() -> Self {
            WriteOverlapProbe {
                inner: MemWorkingStore::default(),
                in_flight: std::sync::atomic::AtomicBool::new(false),
                overlap_detected: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    // Implemented for `Arc<WriteOverlapProbe>` rather than
    // `WriteOverlapProbe` directly so the test can keep its own `Arc`
    // clone around to inspect `overlap_detected` after the threads that
    // share `Box<dyn WorkingStore>` (via `SessionState::store`) have
    // finished with it — `WorkingStore: Send + Sync` but the trait's own
    // methods only take `&self`, so there's no other way to reach back
    // into the concrete type once it's behind the trait object. Allowed
    // under the orphan rule despite `Arc` being a foreign type:
    // `WorkingStore` itself is local to this crate, and that alone is
    // sufficient (the restriction only bites when *both* the trait and
    // the outermost type are foreign).
    impl WorkingStore for std::sync::Arc<WriteOverlapProbe> {
        fn read(&self, key: &Path) -> Result<Vec<u8>, SessionError> {
            self.inner.read(key)
        }

        fn write(&self, key: &Path, bytes: &[u8]) -> Result<(), SessionError> {
            use std::sync::atomic::Ordering;
            if self.in_flight.swap(true, Ordering::SeqCst) {
                self.overlap_detected.store(true, Ordering::SeqCst);
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            let result = self.inner.write(key, bytes);
            self.in_flight.store(false, Ordering::SeqCst);
            result
        }

        fn remove(&self, key: &Path) {
            self.inner.remove(key)
        }
    }

    /// Phase 4 Task 1's dedicated concurrency test: two `std::thread`s
    /// racing `commit_mutation` page-rotates against alternating
    /// `undo_impl`/`redo_impl` calls on the *same* bytes-opened document,
    /// for many rounds, joined at the end. A "best-effort deterministic
    /// interleaving" test rather than a loom-style exhaustive one (per
    /// this task's brief) — real OS thread scheduling widened by
    /// [`WriteOverlapProbe`]'s artificial delay, not a controlled
    /// interleaving oracle. The primary assertion
    /// (`!probe.overlap_detected`) directly checks the property this
    /// task's fix provides — see that struct's doc for why the more
    /// "black box" alternatives (final `page_count`, history-stack
    /// depth) turned out not to reliably distinguish serialized writes
    /// from interleaved ones for this crate's specific shapes; both are
    /// still asserted below too, as secondary sanity checks that hold
    /// under the fix, just not as the test's teeth. Before Phase 4 Task
    /// 1's fix (`undo_impl`/`redo_impl` holding `docs` across their
    /// `store.write`, matching `commit_mutation`'s own discipline — see
    /// `undo_impl`'s doc), this test is exactly the shape of race
    /// `FsWorkingStore::write`'s doc used to describe: two writers with
    /// no ordering between their `store.write` calls, hitting the same
    /// key.
    ///
    /// **Handle-chain coordination, and why it's needed:** every
    /// successful `commit_mutation`/`undo_impl`/`redo_impl` call rotates
    /// to a brand-new [`DocHandle`] (`reopen_after_write` always closes
    /// the old one and opens a fresh one — see that function's doc). If
    /// each thread tracked its own private "current handle" across
    /// rounds, the two threads would only ever *share* a handle on round
    /// 0 — from round 1 onward they'd each be rotating their own private,
    /// mutually invisible handle chain (since neither operation removes
    /// any handle but its own), and the test would stop exercising any
    /// actual interleaving after the very first round. A shared
    /// `Mutex<DocHandle>` (`current_handle`, test-only — nothing this
    /// crate exposes for production use) is how both threads keep
    /// converging on the same evolving handle: each round, a thread reads
    /// the shared value, attempts its operation against it, and (only on
    /// success) writes its own resulting handle back — so subsequent
    /// rounds keep both threads pointed at whichever operation most
    /// recently won.
    ///
    /// This *can* still produce `SessionError::UnknownHandle` on either
    /// thread in a given round: `docs.lock()`'s single global mutex fully
    /// serializes every operation's read-mutate/pop-write critical
    /// section, but the plain `Mutex<DocHandle>` handshake around it is
    /// not part of that same lock — a thread can read `current_handle`,
    /// then lose the race to actually acquire `docs.lock()` to a peer
    /// whose *own* operation (on the very same handle value) runs first,
    /// rotates it away, and writes a different handle back before this
    /// thread's own call ever reaches `docs.lock()`. That's expected and
    /// tolerated (skip the round, don't update `current_handle`, keep
    /// going) — it's a property of handle rotation, not something Phase 4
    /// Task 1's fix claims to solve (this task is about `store.write`
    /// ordering for a given key, not about arbitrating which of two
    /// simultaneous callers "owns" a handle next). What must never happen
    /// is any *other* error (a corrupt/unparseable working copy would
    /// surface as a `Doc`/`Engine` error from `reopen_after_write`'s
    /// `Document::from_bytes`/`engine.open_bytes` — exactly what
    /// serializing the writes exists to prevent).
    #[test]
    fn concurrent_rotate_and_undo_redo_on_same_document_do_not_corrupt_state() {
        use std::sync::{Arc, Barrier};

        let Some(engine) = shared_handle() else {
            return;
        };

        let probe = Arc::new(WriteOverlapProbe::new());
        let state = Arc::new(SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(Arc::clone(&probe)),
        });

        let opened = open_document_bytes(&state, "race-test.pdf", build_three_page_pdf())
            .expect("open_document_bytes should succeed");
        assert_eq!(opened.page_count, 3);
        // `OpenDoc::path` (the `docs`/`history`/`store` key) stays fixed
        // for this document's whole lifetime even as its `DocHandle`
        // rotates on every write — captured once here so the final
        // history-depth check below can look it up regardless of which
        // handle either thread ends up on. `OpenedDocumentInfo` itself
        // has no `path` field (only the user-facing `file_path` string),
        // so this reads it out of `docs` directly.
        let path = state
            .docs
            .lock()
            .expect("docs lock poisoned")
            .get(&opened.handle)
            .expect("just-opened handle must be present")
            .path
            .clone();

        // Seed one undo entry up front so the undo/redo thread has
        // something to undo from round 0, rather than spending early
        // rounds only exercising the "nothing to undo" error path.
        let seeded = commit_mutation::<_, SessionError>(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            opened.handle,
            |doc| doc.rotate_page(0, 90).map_err(Into::into),
        )
        .expect("seed rotate should succeed");

        let current_handle = Arc::new(Mutex::new(seeded.handle));
        // Records any error that isn't one of the expected benign races
        // (see this test's doc comment) — checked once at the end so a
        // failure reports every offending message, not just the first.
        let unexpected_errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // Kept modest deliberately (this task's brief: "the new test may
        // be slow — keep iterations modest, <5s") — enough rounds for
        // real OS thread scheduling to produce genuine overlap between
        // the two threads' critical sections without ballooning runtime;
        // each round does a full mutate-or-undo/redo + reopen cycle
        // against a real PDFium engine.
        const ROUNDS: usize = 40;
        let barrier = Arc::new(Barrier::new(2));

        let is_benign_race_error = |e: &SessionError| {
            matches!(e, SessionError::UnknownHandle(_))
                || matches!(e, SessionError::Doc(msg) if msg == "nothing to undo" || msg == "nothing to redo")
        };

        let mutator = {
            let state = Arc::clone(&state);
            let current_handle = Arc::clone(&current_handle);
            let unexpected_errors = Arc::clone(&unexpected_errors);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                for _ in 0..ROUNDS {
                    barrier.wait();
                    let h = *current_handle.lock().expect("current_handle poisoned");
                    match commit_mutation::<_, SessionError>(
                        &state.engine,
                        &state.docs,
                        &state.history,
                        &*state.store,
                        h,
                        |doc| doc.rotate_page(0, 90).map_err(Into::into),
                    ) {
                        Ok(info) => {
                            *current_handle.lock().expect("current_handle poisoned") = info.handle;
                        }
                        Err(e) if is_benign_race_error(&e) => {}
                        Err(e) => unexpected_errors
                            .lock()
                            .expect("unexpected_errors poisoned")
                            .push(format!("mutator: {e}")),
                    }
                }
            })
        };

        let undoer = {
            let state = Arc::clone(&state);
            let current_handle = Arc::clone(&current_handle);
            let unexpected_errors = Arc::clone(&unexpected_errors);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut want_undo = true;
                for _ in 0..ROUNDS {
                    barrier.wait();
                    let h = *current_handle.lock().expect("current_handle poisoned");
                    let result = if want_undo {
                        undo_impl(&state.engine, &state.docs, &state.history, &*state.store, h)
                    } else {
                        redo_impl(&state.engine, &state.docs, &state.history, &*state.store, h)
                    };
                    match result {
                        Ok(info) => {
                            *current_handle.lock().expect("current_handle poisoned") = info.handle;
                            // Alternate direction only on success — an
                            // empty stack (the "nothing to undo/redo"
                            // benign race below) should retry the same
                            // direction next round rather than ping-pong
                            // off an operation that never actually ran.
                            want_undo = !want_undo;
                        }
                        Err(e) if is_benign_race_error(&e) => {}
                        Err(e) => unexpected_errors
                            .lock()
                            .expect("unexpected_errors poisoned")
                            .push(format!("undoer: {e}")),
                    }
                }
            })
        };

        mutator.join().expect("mutator thread should not panic");
        undoer.join().expect("undoer thread should not panic");

        let errors = unexpected_errors
            .lock()
            .expect("unexpected_errors poisoned")
            .clone();
        assert!(
            errors.is_empty(),
            "no operation should fail for any reason other than a benign handle/history race: {errors:?}"
        );

        // The test's actual teeth: no two `store.write` calls on `probe`
        // were ever in flight at the same time. See `WriteOverlapProbe`'s
        // doc for why this — not the page_count/history-depth checks
        // below — is what actually distinguishes "the fix is in place"
        // from "the fix regressed," and for why this is a store-global
        // check (not a per-key one) that only stands in for "same
        // document key" because this test opens exactly one document and
        // writes no other key through `probe`.
        assert!(
            !probe
                .overlap_detected
                .load(std::sync::atomic::Ordering::SeqCst),
            "two store.write calls ran concurrently on this test's single-document store — since \
             this test only ever writes one key, that's exactly the same-key race Phase 4 Task \
             1's docs-lock discipline in undo_impl/redo_impl/fill_form_fields_impl exists to \
             prevent"
        );

        // Coherence check: whichever handle either thread most recently
        // rotated to must still be a fully valid, openable document with
        // the page count `rotate_page` can never change — the concrete
        // "legal serialized outcome" invariant this test's doc comment
        // promises. A corrupted or partially-written working copy would
        // have already surfaced as an unexpected error above (a parse
        // failure inside `reopen_after_write`), but checking page_count
        // independently here confirms the final state is not just
        // "didn't error" but actually the right document.
        let final_handle = *current_handle.lock().expect("current_handle poisoned");
        let final_page_count = state
            .engine
            .page_count(final_handle)
            .expect("the final handle must still resolve to a live, valid document");
        assert_eq!(
            final_page_count, 3,
            "rotating a page must never change page_count, however the rotate/undo/redo calls \
             that produced the final handle happened to interleave"
        );

        // History depth sanity: the cap this crate enforces
        // (`MAX_HISTORY_DEPTH`) must still hold after 40 rounds of
        // concurrent pushes/pops from two threads — a broken push/pop
        // sequence (e.g. two threads each assuming they alone popped the
        // last entry) is exactly the kind of history/store desync this
        // task's lock discipline exists to prevent.
        {
            let history_guard = state.history.lock().expect("history lock poisoned");
            if let Some(entry) = history_guard.get(&path) {
                assert!(entry.undo.len() <= MAX_HISTORY_DEPTH);
                assert!(entry.redo.len() <= MAX_HISTORY_DEPTH);
            }
        }

        state.engine.close(final_handle);
    }
}
