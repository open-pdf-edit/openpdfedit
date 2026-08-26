//! wasm-bindgen surface for the browser extension. Kept deliberately
//! thin — every method here is a marshaling call into
//! `openpdfedit-session`'s engine-generic session core (the same crate
//! that drives the desktop's Tauri commands), which itself calls into
//! `openpdfedit-engine`'s `PdfiumEngine`. No PDF logic lives in this
//! crate that isn't already tested in `openpdfedit-session`/
//! `openpdfedit-engine`'s own suites.
//!
//! ## Rebuilt on `openpdfedit-session` (Task 8)
//!
//! This crate used to expose a per-document `WasmDocument` class calling
//! `PdfiumEngine` directly (including its own hand-rolled `addHighlight`
//! that round-tripped through `openpdfedit-doc`/`openpdfedit-annot`
//! bytes). That's gone: [`WasmSession`] now holds one
//! `openpdfedit_session::SessionState<PdfiumEngine>` for the whole page
//! and exposes only the Phase-1 **read-render-save** surface —
//! `openDocument`, `saveToBytes`, `renderPage`, `pageSizes`,
//! `closeDocument` — plus, as of Phase 2 Task 2, `markSaved` (see that
//! method's doc — it exists for real dirty-tracking even though at that
//! point this crate still had no wasm-facing *mutating* command yet), and,
//! as of Phase 2's final-review fix wave (I2), `workingCopyBytes` — see
//! that method's doc for why `saveToBytes` alone was the wrong thing for
//! `wasm.ts`'s save path to call once real mutations existed.
//!
//! ## Annotation/undo surface (Phase 2, Task 3)
//!
//! `openpdfedit-session`'s `commit_mutation`/`undo_impl`/`redo_impl`
//! pathway used to be filesystem-bound (snapshot -> `std::fs::write` ->
//! `reopen_after_write`'s path-based reopen), which made it unusable on
//! wasm32 no matter what this crate did. That's no longer true as of the
//! `WorkingStore` abstraction (Phase 2, Task 1): every one of those
//! functions now takes a `store: &dyn WorkingStore` parameter and this
//! session already constructs the portable [`MemWorkingStore`] for
//! exactly that reason (see [`WasmSession::new`]). This is where those
//! wasm-facing entry points land: [`WasmSession::add_annotation`],
//! [`WasmSession::delete_annotation`], [`WasmSession::list_page_annotations`],
//! [`WasmSession::text_selection_quads`], [`WasmSession::undo`], and
//! [`WasmSession::redo`] — thin `serde_json` marshaling over
//! `openpdfedit-session`'s `annotations::{add_annotation_impl,
//! delete_annotation_impl, list_page_annotations_impl,
//! text_selection_quads_impl}` and `{undo_impl, redo_impl}`, exactly
//! mirroring `apps/desktop/src-tauri/src/annotations.rs`'s and
//! `lib.rs`'s Tauri command wrappers over the same functions. The request
//! JSON each of `addAnnotation`/`deleteAnnotation`/`textSelectionQuads`
//! deserializes is byte-for-byte what `apps/desktop/src/lib/backend/
//! types.ts`'s `Backend` interface already sends the Tauri commands
//! (`AddAnnotationRequest`/`DeleteAnnotationRequest`/
//! `TextSelectionQuadsRequest` all derive `Deserialize` for that reason,
//! and each DTO already carries its own `handle` field — so, matching
//! `tauri.ts`'s `invoke(..., { request })` calls, these three methods
//! take only the request JSON, not a separate `handle` parameter).
//! `listPageAnnotations`/`undo`/`redo` take a plain `handle` (and, for
//! `listPageAnnotations`, `pageIndex`) the same way `tauri.ts` passes
//! them, since the underlying `_impl` functions take those as bare
//! parameters rather than bundling them into a request DTO.
//!
//! ## Forms/pages surface (Phase 3, Task 3)
//!
//! [`WasmSession::list_form_fields`]/[`WasmSession::fill_form_fields`]/
//! [`WasmSession::create_form_field`]/[`WasmSession::rotate_page`]/
//! [`WasmSession::delete_page`]/[`WasmSession::move_page`]/
//! [`WasmSession::set_crop_box`]/[`WasmSession::extract_pages`] — thin
//! `serde_json`/plain-argument marshaling over
//! `openpdfedit_session::forms::{list_form_fields_impl,
//! fill_form_fields_impl, create_form_field_impl}` and
//! `openpdfedit_session::pages::{rotate_page_impl, delete_page_impl,
//! move_page_impl, set_crop_box_impl, extract_pages_bytes}`, exactly
//! mirroring `apps/desktop/src-tauri/src/{forms,field_create,pages}.rs`'s
//! Tauri command wrappers over the same functions — merge is excluded
//! (plan-level decision: multi-file input lands in a later phase). Each
//! method's argument shape was chosen by reading `tauri.ts`'s actual
//! `invoke(...)` call for its desktop counterpart, not assumed from the
//! command name:
//!
//! - `listFormFields`/`rotatePage`/`deletePage`/`movePage`/`setCropBox`
//!   take plain `handle`(+`pageIndex`+...) arguments, not a request-JSON
//!   blob — `list_form_fields_cmd`/`rotate_page_cmd`/`delete_page_cmd`/
//!   `move_page_cmd`/`set_crop_box_cmd` all take bare `State` + scalar
//!   arguments on the desktop side (no request DTO exists for any of
//!   them), and `tauri.ts` calls each with `invoke(cmd, { handle,
//!   pageIndex, ... })` — plain named arguments, not `{ request }`. This
//!   matters most for `movePage`: the brief's own shorthand suggested a
//!   request-JSON shape, but `move_page_cmd`/`tauri.ts`'s `movePage` both
//!   take bare `(handle, pageIndex, direction)`, so that's what this
//!   method takes too — `direction` as a plain `"Up"`/`"Down"` string
//!   (matching `types.ts`'s `PageMoveDirection` and `MoveDirection`'s own
//!   derived, non-`rename_all`'d serde shape) rather than a JSON-wrapped
//!   enum. `setCropBox`'s `rect` is a `&[f32]` (a JS `Float32Array` at the
//!   call site) for the same bare-arguments reason, rather than a fifth
//!   JSON string parameter.
//! - `fillFormFields`/`createFormField` take only `request_json` —
//!   `FillFormRequest`/`CreateFormFieldRequest` already carry their own
//!   `handle` field, and `fill_form_fields_cmd`/`create_form_field_cmd`
//!   both take `request: FillFormRequest`/`request:
//!   CreateFormFieldRequest` on the desktop side, called as
//!   `invoke(cmd, { request })` — the same "request DTO embeds its own
//!   handle" shape [`WasmSession::add_annotation`] already established.
//!
//! `extractPages` is the deliberate exception: `openpdfedit_session::
//! pages::ExtractRequest`/`extract_pages_impl` are
//! `#[cfg(not(target_arch = "wasm32"))]`-gated (path-based: they read the
//! source from disk and write the result to `output_path`, neither of
//! which exists here) and so are not even compiled into this crate's
//! wasm32 build. This method instead calls the portable byte-level core
//! ([`extract_pages_bytes`], a thin wrapper over `openpdfedit-pages`'s
//! `extract_pages`) directly against the working copy's current bytes
//! (read via `self.state.store`, the same source [`Self::working_copy_bytes`]
//! reads from) and returns the **extracted document's raw bytes**
//! (`Uint8Array`) rather than an `OpenedDocumentInfo` — there is no
//! filesystem here for a new document to be opened from the way the
//! desktop's `open_new_file` does it. `wasm.ts`'s `extractPages` is what
//! turns those bytes into a real, newly-opened `OpenedDocument`: it writes
//! them to the `FileSystemFileHandle` its caller already picked (via
//! `pickSavePath`, whose synthetic-key/`pendingSavePicks` machinery
//! already exists for exactly this — see that file's "Open-document
//! bookkeeping" comment) and then calls `WasmSession::openDocument` on the
//! same bytes to mint the new, independent document handle the `Backend`
//! interface's `extractPages` contract promises — matching the desktop's
//! own behavior (a brand-new handle, source document at the original
//! `handle` left untouched) without `types.ts`'s `ExtractPagesRequest`/
//! `Backend.extractPages` signature having to change at all.
//!
//! **Why no `#[wasm_bindgen(js_name = ...)]` local request struct reuses
//! `openpdfedit_session::pages::ExtractRequest`**: that type is exactly
//! the desktop-only, path-based one described above (`output_path` field,
//! `#[cfg(not(target_arch = "wasm32"))]`); [`WasmSession::extract_pages`]
//! instead deserializes a small crate-local struct (`handle` +
//! `page_indices` only, no `output_path` — this crate has no use for one)
//! defined right at its call site, the same way every other method here
//! that needs a request shape the desktop's own DTOs don't quite match
//! would.
//!
//! **Why no `#[wasm_bindgen(js_name = ...)]` local request struct reuses
//! `openpdfedit_session::pages::MoveDirection` for JSON**: it doesn't need
//! to — `MoveDirection` derives plain `Deserialize` with no
//! `#[serde(rename_all = ...)]`, so its wire form is already just the bare
//! string `"Up"`/`"Down"`, letting [`WasmSession::move_page`] match on a
//! plain `&str` argument directly instead of parsing a one-field JSON
//! wrapper for it.
//!
//! `listFormFields` needs no wasm-crate-local DTO wrapper at all:
//! [`WasmSession::list_form_fields`] serializes
//! `openpdfedit_session::forms::list_form_fields_impl`'s
//! `Vec<FormFieldDto>` return value as-is — that type already has the
//! right serde shape (`#[serde(rename_all = "camelCase")]`, matching
//! `types.ts`'s `FormFieldDto`).
//!
//! ## Signatures/redact/textedit/image surface (Phase 4, Task 3)
//!
//! [`WasmSession::list_signatures`]/[`WasmSession::redact_page`]/
//! [`WasmSession::list_text_runs`]/[`WasmSession::edit_text_run`]/
//! [`WasmSession::move_text_run`]/[`WasmSession::list_image_placements`]/
//! [`WasmSession::move_image`] — thin marshaling over
//! `openpdfedit_session::signatures::list_signatures_impl`,
//! `openpdfedit_session::redact::redact_page_impl`, and
//! `openpdfedit_session::textedit::{list_text_runs_impl,
//! edit_text_run_impl, move_text_run_impl, list_image_placements_impl,
//! move_image_impl}`, exactly mirroring
//! `apps/desktop/src-tauri/src/{signatures,redact,textedit}.rs`'s Tauri
//! command wrappers over the same functions. Every one of these `_impl`
//! functions was already portable (no `#[cfg(not(target_arch =
//! "wasm32"))]` gate anywhere in `signatures.rs`/`redact.rs`/
//! `textedit.rs`, unlike `pages`'s merge/extract or `compare`) before this
//! task touched a single line of Rust here — `list_signatures_impl` was
//! made portable in Phase 4 Task 2 (routed through `store.read` instead of
//! `std::fs::read`), and `redact`/`textedit`'s mutating functions have
//! been generic over `E: Engine` and store-routed via
//! [`crate::commit_mutation`] since Phase 1 (textedit)/were moved here
//! already-portable (redact). This task is purely wiring: no new
//! `openpdfedit-session` logic, no new gating decisions.
//!
//! Argument shapes were re-verified against `tauri.ts`'s actual
//! `invoke(...)` calls and the desktop command wrappers' actual
//! parameters, not assumed from the plan's shorthand (which the crate's
//! own module doc above already flags as having been wrong twice for the
//! forms/pages surface):
//!
//! - `listSignatures`/`listTextRuns`/`listImagePlacements` take plain
//!   `handle`(+`pageIndex`) arguments, not a request-JSON blob —
//!   `list_signatures_cmd`/`list_text_runs_cmd`/`list_image_placements_cmd`
//!   all take bare `State` + scalar arguments on the desktop side, and
//!   `tauri.ts` calls each as `invoke(cmd, { handle })` /
//!   `invoke(cmd, { handle, pageIndex })`. All three are read-only (no
//!   `history`/handle rotation involved) — `list_signatures_impl` doesn't
//!   even take an `E: Engine` parameter (it only ever reads working-copy
//!   bytes through `store`, never the engine — see `signatures.rs`'s
//!   module doc), so this crate's `list_signatures` method is the one
//!   list method here that doesn't touch `self.state.engine` at all.
//! - `redactPage`/`editTextRun`/`moveTextRun`/`moveImage` take only
//!   `request_json` — `RedactPageRequest`/`EditTextRunRequest`/
//!   `MoveTextRunRequest`/`MoveImageRequest` already carry their own
//!   `handle` field, and `redact_page_cmd`/`edit_text_run_cmd`/
//!   `move_text_run_cmd`/`move_image_cmd` all take a single `request: ...`
//!   argument on the desktop side, called as `invoke(cmd, { request })` —
//!   the same "request DTO embeds its own handle" shape
//!   [`WasmSession::add_annotation`] already established. All four are
//!   mutating: each goes through `commit_mutation` inside its `_impl` and
//!   returns a **rotated** `OpenedDocumentInfo` DTO, same as every other
//!   mutating method on this type.
//!
//! **Signature placement rides the already-live annotation surface, no
//! new wasm method needed.** Traced `apps/desktop/src/routes/
//! +page.svelte`'s `handlePlaceSignature`: it maps a saved signature's
//! normalized strokes into PDF-point space and calls
//! `handleCreateAnnotation(pageIndex, { ..., annotation: { kind: "ink",
//! strokes: mapped } })`, which itself calls `backend.addAnnotation({
//! handle, pageIndex, rect, color: [0,0,0], opacity: 1, contents: null,
//! annotation })` — the exact same `AddAnnotationRequest`/
//! `add_annotation_cmd` path the freehand Draw tool already uses (see
//! `apps/desktop/src/lib/signatures.svelte.ts`'s module doc, referenced
//! from that same comment in `+page.svelte`). [`WasmSession::add_annotation`]
//! has been live since Phase 2 Task 4; a saved signature is just a reusable
//! stroke template fed through that same Ink-annotation path, not a
//! distinct backend operation — so this task adds no "place signature"
//! method. What *was* still `notImplemented` on the wasm backend, and is
//! what this task actually lands for signatures, is the read side:
//! `listSignatures`, which is what `+page.svelte`'s `refreshSignatures`
//! calls after open and after a fill — see that function's own doc for why
//! a fill (not a redact/textedit/annotation edit — see this crate's own
//! `working_copy_bytes` I3 doc above) is the one write path signatures
//! must be re-fetched around, and note that this refetch-after-fill
//! behavior needed no new code here either: it was already how
//! `+page.svelte` called the (until now `notImplemented`) `listSignatures`
//! `Backend` method — only `wasm.ts`'s implementation of that method was
//! missing, not the call site or its invalidation logic.
//!
//! **Why no `async fn` exports and no Workers**: see [`build_engine`]'s
//! doc — this crate calls `PdfiumEngine` directly, not through
//! `openpdfedit-engine::EngineHandle`'s dedicated-thread wrapper, because
//! `wasm32-unknown-unknown` has no OS threads at all. The safety argument
//! that makes calling `PdfiumEngine` directly sound here (a wasm32 module
//! in one browser tab is already single-threaded, so PDFium's
//! "one caller at a time" requirement holds automatically) depends on
//! every call staying synchronous and on the main JS thread — an `async
//! fn` export or a Worker would reintroduce genuine concurrent access to
//! the same `PdfiumEngine`, which is exactly what this design avoids
//! having to guard against with a mutex/thread the way the desktop's
//! `EngineHandle` does.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use openpdfedit_engine::{DocHandle, Engine, PdfiumEngine};
use openpdfedit_session::annotations::{
    add_annotation_impl, delete_annotation_impl, list_page_annotations_impl, select_text_impl,
    text_selection_quads_impl, AddAnnotationRequest, DeleteAnnotationRequest,
    TextSelectionQuadsRequest,
};
use openpdfedit_session::compare::compare_bytes;
use openpdfedit_session::encrypt::{encrypt_document_bytes, EncryptChoices};
use openpdfedit_session::flatten::{flatten_document_impl, FlattenDocumentRequest};
use openpdfedit_session::forms::{
    create_form_field_impl, fill_form_fields_impl, list_form_fields_impl, CreateFormFieldRequest,
    FillFormRequest,
};
use openpdfedit_session::numbering::{number_pages_impl, NumberPagesRequest};
use openpdfedit_session::outline::document_outline_impl;
use openpdfedit_session::pages::{
    delete_page_impl, extract_pages_bytes, merge_open_doc_with_bytes, move_page_impl,
    rotate_page_impl, set_crop_box_impl, MoveDirection,
};
use openpdfedit_session::redact::{redact_page_impl, RedactPageRequest};
use openpdfedit_session::search::search_document_impl;
use openpdfedit_session::signatures::list_signatures_impl;
use openpdfedit_session::textedit::{
    edit_text_run_impl, list_image_placements_impl, list_text_runs_impl, move_image_impl,
    move_text_run_impl, EditTextRunRequest, MoveImageRequest, MoveTextRunRequest,
};
use openpdfedit_session::watermark::{apply_watermark_impl, ApplyWatermarkRequest};
use openpdfedit_session::xfdf::{export_xfdf_impl, import_xfdf_impl};
use openpdfedit_session::{
    close_document_impl, redo_impl, undo_impl, MemWorkingStore, PageSize, SessionState,
};
use wasm_bindgen::prelude::*;

fn to_js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// The one error the UI has to recognise rather than merely display: a
/// document that needs a password should raise a prompt, not a banner.
///
/// The desktop's `CommandError::PasswordRequired` serializes as the exact
/// string `"password required"`, and `isPasswordRequired` in
/// `apps/desktop/src/lib/backend/index.ts` matches on it. Mapping to the
/// same string here keeps that one contract rather than teaching the UI
/// a second phrasing — `SessionError::PasswordRequired`'s own `Display`
/// is prose ("this document is password-protected") and would silently
/// fail that match, which is exactly the shape of the bug this replaced:
/// the raw engine error reached the user as
/// `PdfiumLibraryInternalError(PasswordError)`.
fn session_to_js_err(e: openpdfedit_session::SessionError) -> JsValue {
    match e {
        openpdfedit_session::SessionError::PasswordRequired => {
            JsValue::from_str("password required")
        }
        other => to_js_err(other),
    }
}

/// One `PdfiumEngine` for the whole extension page's lifetime — mirrors
/// the desktop app's own rule (see `PdfiumEngine::new`'s doc comment):
/// PDFium's global init is not safe to run more than once per process.
/// Called from [`WasmSession::new`], which is itself guarded against
/// running more than once — see that constructor's doc for the layered
/// (JS memoization + Rust-enforced) guard.
///
/// Unlike the desktop app, this does *not* go through `EngineHandle`,
/// which dedicates an OS thread to serialize access to PDFium.
/// `std::thread::Builder::spawn` always returns `Err(Unsupported)` on
/// `wasm32-unknown-unknown` (no OS threads there at all), so `EngineHandle`
/// can't be constructed in a wasm32 build. A wasm32 module running in a
/// single browser tab is already single-threaded, so the one-caller-at-a-time
/// guarantee a dedicated thread exists to enforce holds automatically —
/// calling `PdfiumEngine` directly is safe here for the same reason no
/// thread is needed to make it so.
///
/// Uses `PdfiumEngine::new(None)`, not `new_dev()` — `new_dev()` first
/// probes `.vendor/pdfium/lib` on the filesystem (via `dev_vendor_lib_dir`)
/// before falling back to `bind_to_system_library`, a check meant for the
/// desktop app's local dev loop. On `wasm32-unknown-unknown` `lib_dir` is
/// ignored entirely either way (see `PdfiumEngine::new`'s own cfg-gated
/// body — this target always calls `bind_to_system_library`, i.e. the
/// already-loaded-and-initialized-from-JS `pdfium.wasm` module), so the
/// probe can only ever find nothing and fall through; `new(None)` skips
/// straight to the branch that's actually taken, instead of performing a
/// filesystem check that's meaningless for this build target.
fn build_engine() -> Result<PdfiumEngine, JsValue> {
    PdfiumEngine::new(None).map_err(to_js_err)
}

/// Mirrors `openpdfedit-engine::RenderedTile` across the wasm boundary —
/// the JS side needs the actual pixel dimensions to size its canvas, not
/// just the raw bytes (a plain `Uint8Array` return can't carry both).
///
/// Also carries the page's untransformed size in PDF points
/// (`pointWidth`/`pointHeight`, from `Engine::page_sizes`) alongside the
/// rendered pixel size — the coordinate transform between canvas pixels
/// and PDF points (pointer input, drag-to-highlight, etc.) needs both.
#[wasm_bindgen]
pub struct RenderedPage {
    width: u32,
    height: u32,
    point_width: f32,
    point_height: f32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl RenderedPage {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[wasm_bindgen(getter, js_name = pointWidth)]
    pub fn point_width(&self) -> f32 {
        self.point_width
    }

    #[wasm_bindgen(getter, js_name = pointHeight)]
    pub fn point_height(&self) -> f32 {
        self.point_height
    }

    #[wasm_bindgen(getter)]
    pub fn rgba(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.rgba.as_slice())
    }
}

/// One open-documents session for the whole extension page, backed by
/// `openpdfedit-session`'s engine-generic core — the same DTOs
/// (`OpenedDocumentInfo`/`PageSize`) the desktop's Tauri commands emit,
/// so the shared Svelte UI's coordinate math (which reads `page_sizes`
/// off the JSON `OpenedDocument` shape) works unchanged against this
/// backend. See this module's doc comment for why there is no
/// `undo`/`redo` here.
///
/// `DocHandle` (a plain `u64` in `openpdfedit-engine`) crosses the wasm
/// boundary as `u32` in every method below, not `u64` — wasm-bindgen maps
/// `u64` to a JS `bigint`, but `Backend`'s TypeScript surface
/// (`apps/desktop/src/lib/backend/types.ts`) types every handle as a
/// plain `number`, matching what Tauri's JSON-serialized `OpenedDocument.handle`
/// already is. `PdfiumEngine`'s handles are an in-process
/// `AtomicU64` counter starting at 1 and incrementing by 1 per
/// `open`/`open_bytes` call (see that crate's `next_handle` field) — a
/// single extension page opening/closing documents one at a time will
/// never come close to exhausting a `u32`, so widening on the way in
/// (`handle as u64`) and narrowing on the way out (the JSON `handle`
/// field is `u64` serialized as a plain JSON number, which JS's
/// `JSON.parse` already reads as a `number`, not a `bigint`) is safe in
/// practice without needing wasm-bindgen's `bigint` support at all.
#[wasm_bindgen]
pub struct WasmSession {
    state: SessionState<PdfiumEngine>,
}

#[wasm_bindgen]
impl WasmSession {
    /// Layered guard against constructing more than one `WasmSession`
    /// (and therefore more than one `PdfiumEngine`, hence more than one
    /// `FPDF_InitLibrary` call) per process: `wasm.ts`'s `ensureSession()`
    /// memoizes its own single call to `new WasmSession()` — the
    /// friendly, everyday-operation path — but that's a JS-side
    /// *convention*, not something this constructor can rely on; any JS
    /// caller (a bug in `wasm.ts` itself, a future caller that doesn't
    /// go through `ensureSession()`, hot-reloaded dev code, ...) could
    /// still call `new WasmSession()` a second time. This
    /// `OnceLock<()>`-backed check is the backstop that makes a second
    /// call fail loudly with a clear error instead of silently
    /// double-initializing PDFium's process-global state — restored
    /// after a review found the previous design (a `OnceLock`-cached
    /// `&'static PdfiumEngine` behind a free function every method went
    /// through) had been dropped when `WasmSession` started owning its
    /// engine by value, leaving the single-init invariant enforced only
    /// by `wasm.ts`'s memoization. Once the slot is claimed, it stays
    /// claimed even if `build_engine()` below then fails — matching the
    /// old design's own no-retry-after-failure behavior (its
    /// `get_or_init` closure ran at most once too, Ok or Err), not a new
    /// regression: a second `WasmSession::new()` call always fails from
    /// here on, whether or not the first call actually produced a
    /// working engine.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WasmSession, JsValue> {
        console_error_panic_hook::set_once();

        static SESSION_CREATED: OnceLock<()> = OnceLock::new();
        SESSION_CREATED.set(()).map_err(|_| {
            JsValue::from_str(
                "WasmSession::new() called more than once in this process — PDFium's global \
                 init (FPDF_InitLibrary) is not safe to run twice; construct at most one \
                 WasmSession per page load (see wasm.ts's ensureSession(), which is meant to be \
                 every caller's only path here)",
            )
        })?;

        Ok(WasmSession {
            state: SessionState {
                engine: build_engine()?,
                docs: Mutex::new(HashMap::new()),
                history: Mutex::new(HashMap::new()),
                // No filesystem on wasm32 — every working-copy/snapshot
                // read or write this session ever does (today: none,
                // since Phase 1 has no mutation surface; see this
                // module's doc comment) goes through this in-memory store
                // instead of `openpdfedit-session`'s desktop-only
                // `FsWorkingStore`.
                store: Box::new(MemWorkingStore::default()),
            },
        })
    }

    /// Opens a document from in-memory `bytes` (no filesystem — there is
    /// none on `wasm32-unknown-unknown`) via
    /// `openpdfedit_session::open_document_bytes_with_password`, registers it under
    /// `display_name` (the extension's synthetic identity for a document
    /// with no real on-disk path — typically the picked file's name), and
    /// returns the resulting `OpenedDocumentInfo` DTO serialized as JSON.
    /// Field names/casing are exactly what `#[derive(Serialize)]` emits
    /// for that struct (no `rename_all`), which is also exactly what
    /// `apps/desktop/src/lib/backend/types.ts`'s `OpenedDocument`
    /// interface expects — see that crate's own doc comment inventory.
    ///
    /// `password` is `None`/`undefined` for the ordinary case. A protected
    /// document opened without one fails with `"password required"` —
    /// see [`session_to_js_err`] — which is the UI's cue to prompt and
    /// call again, exactly as the desktop's `open_document` command
    /// behaves.
    #[wasm_bindgen(js_name = openDocument)]
    pub fn open_document(
        &self,
        display_name: &str,
        bytes: &[u8],
        password: Option<String>,
    ) -> Result<String, JsValue> {
        let info = openpdfedit_session::open_document_bytes_with_password(
            &self.state,
            display_name,
            bytes.to_vec(),
            password.as_deref(),
        )
        .map_err(session_to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// The engine-side bytes of the currently-open document at `handle`
    /// — i.e. whatever `Engine::save_to_bytes` (a full PDFium rewrite of
    /// the in-memory document, not a copy of the original opened bytes)
    /// produces *right now*. Phase 1 has no mutation surface at all (see
    /// this module's doc comment), so today this is always byte-for-byte
    /// equivalent to a straight PDFium round-trip of whatever was opened
    /// — but the method itself doesn't assume that; it always asks the
    /// engine for its current bytes, so it stays correct once Phase 2
    /// adds real edits.
    ///
    /// **Deliberately does not mark the document clean.** Takes `&self`
    /// (no interior mutation happens here either) because the bytes this
    /// returns aren't durably saved *anywhere* yet — this method only asks
    /// PDFium to serialize the in-memory document; the actual "save",
    /// writing those bytes to the file the user opened, is `wasm.ts`'s job
    /// (a `FileSystemFileHandle` write, entirely outside this crate and
    /// this wasm module). If this method flipped `dirty` to `false` before
    /// that write even started, a failed `FileSystemWritableFileStream`
    /// write (disk full, permission revoked mid-session, ...) would leave
    /// the document showing clean while the user's file on disk was never
    /// actually updated — silently discarding the "you have unsaved
    /// changes" signal exactly when it matters most. [`Self::mark_saved`]
    /// exists as the separate call `wasm.ts` makes *after* its own write
    /// resolves successfully, so the dirty flag's truth always tracks
    /// what's actually durable, not what's merely been computed.
    ///
    /// **Not what `wasm.ts`'s own save path calls** (Phase 2 final-review
    /// I2): calling this unconditionally would re-derive a *fresh* full
    /// engine-side rewrite of the in-memory document right now, which can
    /// diverge from whatever the working copy's *last* store-routed write
    /// actually produced — see [`Self::working_copy_bytes`]'s doc (and its
    /// I3 correction) for the method that actually byte-matches the
    /// desktop's save output, and for why "the mutating commands preserve
    /// signatures" stopped being universally true once form-filling's own
    /// full-PDFium-rewrite write path became reachable here too. Kept
    /// around for any caller that genuinely wants a full PDFium rewrite of
    /// the in-memory document rather than the working copy's own bytes;
    /// today nothing in this crate is that caller.
    #[wasm_bindgen(js_name = saveToBytes)]
    pub fn save_to_bytes(&self, handle: u32) -> Result<js_sys::Uint8Array, JsValue> {
        let bytes = self
            .state
            .engine
            .save_to_bytes(handle as DocHandle)
            .map_err(to_js_err)?;
        Ok(js_sys::Uint8Array::from(bytes.as_slice()))
    }

    /// The **working-copy** bytes this session's `MemWorkingStore` already
    /// holds for `handle`'s document — exactly what the *last* store-routed
    /// write for it produced. This is what `wasm.ts`'s save path
    /// (`writeToFileHandle`) must actually write to the
    /// `FileSystemFileHandle` — see [`Self::save_to_bytes`]'s doc for why
    /// that method is the wrong one for this job (it always re-derives a
    /// fresh, full PDFium rewrite of the *current* in-memory document,
    /// which may not even match what this method returns). This method
    /// mirrors what the desktop backend does for the same reason:
    /// `save_document_impl` copies the working copy's *bytes on disk* over
    /// the original path (`copy_with_lock_retry`), never re-derives them.
    ///
    /// **Corrected (fix-wave re-review's I3):** this doc used to claim the
    /// working copy is *always* an `openpdfedit-doc` lopdf incremental
    /// save — true for `commit_mutation`/`undo_impl`/`redo_impl` (every
    /// mutation routed through [`crate::commit_mutation`]: annotations,
    /// pages, redact, textedit, form-field creation), but false once a
    /// form has been filled. `openpdfedit_session::forms::fill_form_fields_impl`
    /// writes a *different* way — `Engine::fill_form_fields` mutates
    /// PDFium's in-memory document, then `Engine::save_to_bytes`
    /// (PDFium's own `FPDF_SaveAsCopy`/`FPDF_SaveWithVersion`, a full
    /// rewrite of the *entire* file) produces the bytes that get
    /// `store.write`-ten — see that function's own doc for why filling
    /// can't go through the lopdf incremental path at all (PDFium's own
    /// form model, not `openpdfedit-doc`'s object graph, is what actually
    /// updates field values/appearances). A full rewrite renumbers and
    /// repositions every object in the file, which invalidates any
    /// existing signature's `/ByteRange` — so **a fill invalidates
    /// existing signature byte ranges**, exactly like every other
    /// full-rewrite write path in this codebase. This is not new
    /// wasm-specific behavior: the desktop's own `fill_form_fields_impl`
    /// has written this same way since forms-fill landed in M4, on the
    /// same `EngineHandle`/PDFium write path — Phase 3 Task 2 made that
    /// code portable (this crate can now call it too), it didn't change
    /// what it does to a signed document's byte ranges. A caller of this
    /// method can't tell which write path produced the bytes it returns
    /// just from the return value — see `apps/desktop/src/routes/
    /// +page.svelte`'s `refreshSignatures` for how the desktop UI accounts
    /// for this (re-fetching signatures after a fill, not assuming they
    /// survived it).
    ///
    /// Errors if `handle` is unknown (mirrors every other handle-taking
    /// method in this crate — an unknown handle is a caller bug, not a
    /// normal path). Read-only against the store — like `saveToBytes`,
    /// does **not** mark the document clean; that's still [`Self::mark_saved`]'s
    /// job, called only after the caller's own write actually succeeds.
    ///
    /// Delegates rather than reading the store directly, because the
    /// working copy of a protected document is stored *decrypted* — see
    /// `openpdfedit_session::working_copy_bytes`, which is what puts the
    /// protection back before these bytes reach a file.
    /// Writes an invisible text layer from words recognised in the
    /// browser, making a scan searchable.
    ///
    /// Recognition happens in JavaScript (tesseract.js in a worker),
    /// because shelling out to the tesseract binary — what the desktop
    /// does — has no equivalent here. Everything after recognition is
    /// the same code the desktop runs: the words land in
    /// `openpdfedit_ocr::add_text_layer` either way, so a page OCR'd in
    /// the browser and the same page OCR'd on the desktop differ only by
    /// what the two recognisers read, never by how the layer is written.
    ///
    /// `request_json` is an `AddOcrTextLayerRequest`: every page at once,
    /// so the whole document's OCR is a single undo step.
    #[wasm_bindgen(js_name = addOcrTextLayer)]
    pub fn add_ocr_text_layer(&self, request_json: &str) -> Result<String, JsValue> {
        let request: openpdfedit_session::ocr::AddOcrTextLayerRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = openpdfedit_session::ocr::add_ocr_text_layer_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    #[wasm_bindgen(js_name = workingCopyBytes)]
    pub fn working_copy_bytes(&self, handle: u32) -> Result<js_sys::Uint8Array, JsValue> {
        let bytes = openpdfedit_session::working_copy_bytes(&self.state, handle as DocHandle)
            .map_err(to_js_err)?;
        Ok(js_sys::Uint8Array::from(bytes.as_slice()))
    }

    /// Marks `handle`'s document clean (`is_dirty: false` in the next
    /// `OpenedDocumentInfo` refresh) — thin wrapper over
    /// `openpdfedit_session::mark_saved`. **Caller contract** (see that
    /// function's own doc for the full rationale): call this only after
    /// `wasm.ts` has confirmed its own `FileSystemFileHandle` write of the
    /// bytes `saveToBytes` returned actually succeeded — never before that
    /// write, and never on a failed one. Takes `&self` even though it
    /// mutates: `SessionState::docs` is a `Mutex`-guarded map (interior
    /// mutability), the same pattern every other method on this type
    /// already relies on for the engine's own `Mutex<HashMap<...>>`
    /// document table.
    #[wasm_bindgen(js_name = markSaved)]
    pub fn mark_saved(&self, handle: u32) -> Result<String, JsValue> {
        let info =
            openpdfedit_session::mark_saved(&self.state, handle as DocHandle).map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Renders `page_index` at `target_width` pixels wide (aspect-ratio
    /// preserved), returning both the rendered pixels and the page's
    /// untransformed size in PDF points (see `RenderedPage`'s doc).
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(
        &self,
        handle: u32,
        page_index: u32,
        target_width: u32,
    ) -> Result<RenderedPage, JsValue> {
        let tile = self
            .state
            .engine
            .render_page(handle as DocHandle, page_index, target_width)
            .map_err(to_js_err)?;
        // One extra call per render for the page's point size. Simplicity
        // over round-trip count here: `page_sizes` returns every page's
        // size in one call, and this crate has no per-page-size cache to
        // populate it from, so a page-size-only cache would be new state
        // to keep in sync for a value that's cheap to ask PDFium for
        // again each time (this whole call is a handful of memory reads
        // on an already-open document, not a re-parse).
        let sizes = self
            .state
            .engine
            .page_sizes(handle as DocHandle)
            .map_err(to_js_err)?;
        let size = sizes
            .get(page_index as usize)
            .ok_or_else(|| to_js_err(format!("page index {page_index} out of range")))?;
        Ok(RenderedPage {
            width: tile.width,
            height: tile.height,
            point_width: size.width,
            point_height: size.height,
            rgba: tile.rgba,
        })
    }

    /// Every page's size in PDF points, in reading order, as a JSON array
    /// of `{width, height}` objects (`openpdfedit_session::PageSize`'s
    /// own serde shape — the same one embedded in `OpenedDocumentInfo`'s
    /// `page_sizes` field). Lets a caller lay out a virtualized scroll
    /// container without a full `openDocument`/reopen round trip.
    #[wasm_bindgen(js_name = pageSizes)]
    pub fn page_sizes(&self, handle: u32) -> Result<String, JsValue> {
        let sizes = self
            .state
            .engine
            .page_sizes(handle as DocHandle)
            .map_err(to_js_err)?;
        let dto: Vec<PageSize> = sizes
            .into_iter()
            .map(|s| PageSize {
                width: s.width,
                height: s.height,
            })
            .collect();
        serde_json::to_string(&dto).map_err(to_js_err)
    }

    /// Releases `handle`'s engine-side document (and its owned byte
    /// buffer — see `Engine::open_bytes`'s doc), drops the session's own
    /// `docs` bookkeeping entry for it, and — as of Phase 2's final-review
    /// fix wave (C1) — also removes its `MemWorkingStore` entry and
    /// `DocHistory` undo/redo entry, via `openpdfedit_session::close_document_impl`.
    /// Without ever calling this, a `WasmSession` that opens many
    /// documents over a page's lifetime (e.g. one-at-a-time via repeated
    /// "Open…") leaks every previous document's engine-side state, the
    /// same shape of bug the old per-document `WasmDocument`'s `Drop` impl
    /// existed to avoid (see this crate's pre-Task-8 history) —
    /// `WasmSession` has no `Drop` equivalent of its own since it isn't a
    /// per-document type, so a caller (`wasm.ts`, or whatever Task 9 wires
    /// up) is responsible for calling this when a document is genuinely
    /// done with, not just when a new one is opened. The store/history
    /// cleanup matters even more here than it did before this fix:
    /// [`openpdfedit_session::open_document_bytes`] mints a unique working
    /// key per open now (see that function's doc), so this is what
    /// actually reclaims a closed document's `MemWorkingStore` bytes and
    /// `DocHistory` stacks instead of letting them accumulate for the rest
    /// of the page's lifetime.
    #[wasm_bindgen(js_name = closeDocument)]
    pub fn close_document(&self, handle: u32) {
        close_document_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
        );
    }

    /// Adds one markup annotation (highlight/underline/strikeout/
    /// free-text/ink — see `AnnotationInput`'s `kind` tag) to an open
    /// document and returns the resulting `OpenedDocumentInfo` DTO
    /// serialized as JSON, exactly like every other mutating method on
    /// this type — see this module's doc comment for why `request_json`
    /// alone (no separate `handle` argument) is this method's whole
    /// input: `AddAnnotationRequest` already carries its own `handle`
    /// field, and this is the same JSON `types.ts`'s `addAnnotation`
    /// sends the Tauri `add_annotation_cmd`. Thin wrapper over
    /// `openpdfedit_session::annotations::add_annotation_impl`.
    #[wasm_bindgen(js_name = addAnnotation)]
    pub fn add_annotation(&self, request_json: &str) -> Result<String, JsValue> {
        let request: AddAnnotationRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = add_annotation_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Deletes one annotation (identified by its stable `lopdf` object id
    /// — see `AnnotationSummaryDto::id`'s doc) from an open document and
    /// returns the resulting `OpenedDocumentInfo` DTO serialized as JSON.
    /// Thin wrapper over
    /// `openpdfedit_session::annotations::delete_annotation_impl` — see
    /// [`Self::add_annotation`]'s doc for why this takes only
    /// `request_json`.
    #[wasm_bindgen(js_name = deleteAnnotation)]
    pub fn delete_annotation(&self, request_json: &str) -> Result<String, JsValue> {
        let request: DeleteAnnotationRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = delete_annotation_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Every annotation on `page_index` of `handle`'s document, as a JSON
    /// array of `AnnotationSummaryDto`. Read-only, so — like
    /// `openpdfedit_session::annotations::list_page_annotations_impl`
    /// itself — this takes plain `handle`/`page_index` parameters instead
    /// of a request DTO, matching `tauri.ts`'s
    /// `listPageAnnotations(handle, pageIndex)` call shape.
    #[wasm_bindgen(js_name = listPageAnnotations)]
    pub fn list_page_annotations(&self, handle: u32, page_index: u32) -> Result<String, JsValue> {
        let summaries =
            list_page_annotations_impl(&self.state.docs, handle as DocHandle, page_index)
                .map_err(to_js_err)?;
        serde_json::to_string(&summaries).map_err(to_js_err)
    }

    /// Snaps a drag gesture's PDF-point start/end coordinates to the
    /// nearest character boundaries and returns the covered text's line
    /// quads (a JSON array of `[x0, y0, x1, y1]`) — the same snapping
    /// logic the highlight/underline/strikeout tools use to build their
    /// `quads` input to [`Self::add_annotation`]. Read-only (no
    /// `docs`/`history`/`store` involved), so unlike the mutating methods
    /// above this never touches undo/redo history. Thin wrapper over
    /// `openpdfedit_session::annotations::text_selection_quads_impl` —
    /// see [`Self::add_annotation`]'s doc for why this takes only
    /// `request_json`.
    #[wasm_bindgen(js_name = textSelectionQuads)]
    pub fn text_selection_quads(&self, request_json: &str) -> Result<String, JsValue> {
        let request: TextSelectionQuadsRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let quads = text_selection_quads_impl(&self.state.engine, request).map_err(to_js_err)?;
        serde_json::to_string(&quads).map_err(to_js_err)
    }

    /// The same selection, with the characters as well as their
    /// geometry — what the Select tool needs, since a selection nobody
    /// can copy is only a highlight that does not persist.
    #[wasm_bindgen(js_name = selectText)]
    pub fn select_text(&self, request_json: &str) -> Result<String, JsValue> {
        let request: TextSelectionQuadsRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let selection = select_text_impl(&self.state.engine, request).map_err(to_js_err)?;
        serde_json::to_string(&selection).map_err(to_js_err)
    }

    /// Undoes the most recent edit for `handle`'s document (restores the
    /// working copy's pre-edit bytes via the `MemWorkingStore` this
    /// session was constructed with, and rotates the render handle, same
    /// as any other write) and returns the resulting `OpenedDocumentInfo`
    /// DTO serialized as JSON. Errors if there's nothing to undo — the
    /// front-end should already be disabling the Undo button via
    /// `OpenedDocumentInfo::can_undo`, so this is a defensive backstop,
    /// not the primary UX guard, mirroring `undo_cmd`'s own doc on the
    /// desktop side. Thin wrapper over `openpdfedit_session::undo_impl`.
    #[wasm_bindgen(js_name = undo)]
    pub fn undo(&self, handle: u32) -> Result<String, JsValue> {
        let info = undo_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// The redo half of [`Self::undo`] — see that method's doc. Thin
    /// wrapper over `openpdfedit_session::redo_impl`.
    #[wasm_bindgen(js_name = redo)]
    pub fn redo(&self, handle: u32) -> Result<String, JsValue> {
        let info = redo_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Every AcroForm field on `handle`'s document, as a JSON array of
    /// `FormFieldDto`. Read-only (no handle rotation), so — like
    /// `listPageAnnotations` — this takes a plain `handle` argument
    /// instead of a request DTO, matching `tauri.ts`'s
    /// `listFormFields(handle)` call shape. Thin wrapper over
    /// `openpdfedit_session::forms::list_form_fields_impl` — see this
    /// module's doc comment ("Forms/pages surface") for the full argument-
    /// shape rationale.
    #[wasm_bindgen(js_name = listFormFields)]
    pub fn list_form_fields(&self, handle: u32) -> Result<String, JsValue> {
        let fields =
            list_form_fields_impl(&self.state.engine, handle as DocHandle).map_err(to_js_err)?;
        serde_json::to_string(&fields).map_err(to_js_err)
    }

    /// Fills one or more AcroForm field values on `request`'s document and
    /// returns the resulting `OpenedDocumentInfo` DTO serialized as JSON,
    /// exactly like every other mutating method on this type. Takes only
    /// `request_json` — `FillFormRequest` already carries its own `handle`
    /// field, matching `tauri.ts`'s `fillFormFields({ request })` call
    /// shape (see this module's doc comment). Thin wrapper over
    /// `openpdfedit_session::forms::fill_form_fields_impl`.
    #[wasm_bindgen(js_name = fillFormFields)]
    pub fn fill_form_fields(&self, request_json: &str) -> Result<String, JsValue> {
        let request: FillFormRequest = serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = fill_form_fields_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Creates a new AcroForm field (text or checkbox) on `request`'s
    /// document and returns the resulting `OpenedDocumentInfo` DTO
    /// serialized as JSON. Takes only `request_json` — see
    /// [`Self::fill_form_fields`]'s doc for why. Thin wrapper over
    /// `openpdfedit_session::forms::create_form_field_impl`.
    #[wasm_bindgen(js_name = createFormField)]
    pub fn create_form_field(&self, request_json: &str) -> Result<String, JsValue> {
        let request: CreateFormFieldRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = create_form_field_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Rotates `page_index` of `handle`'s document by `delta_degrees` and
    /// returns the resulting `OpenedDocumentInfo` DTO serialized as JSON.
    /// Plain `handle`/`page_index`/`delta_degrees` arguments, not a
    /// request DTO — `rotate_page_cmd` takes bare `State` + scalar
    /// arguments on the desktop side (no request DTO exists for it) and
    /// `tauri.ts` calls it as `invoke("rotate_page_cmd", { handle,
    /// pageIndex, deltaDegrees })` — see this module's doc comment.  Thin
    /// wrapper over `openpdfedit_session::pages::rotate_page_impl`.
    #[wasm_bindgen(js_name = rotatePage)]
    pub fn rotate_page(
        &self,
        handle: u32,
        page_index: u32,
        delta_degrees: i32,
    ) -> Result<String, JsValue> {
        let info = rotate_page_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
            page_index,
            delta_degrees,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Deletes `page_index` from `handle`'s document and returns the
    /// resulting `OpenedDocumentInfo` DTO serialized as JSON. Plain
    /// `handle`/`page_index` arguments — see [`Self::rotate_page`]'s doc
    /// for why. Thin wrapper over
    /// `openpdfedit_session::pages::delete_page_impl`.
    #[wasm_bindgen(js_name = deletePage)]
    pub fn delete_page(&self, handle: u32, page_index: u32) -> Result<String, JsValue> {
        let info = delete_page_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
            page_index,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Swaps `page_index` with its "up" or "down" neighbor (`direction`,
    /// a plain `"Up"`/`"Down"` string — `MoveDirection`'s own
    /// non-`rename_all`'d serde shape, matching `types.ts`'s
    /// `PageMoveDirection`) in `handle`'s document, returning the
    /// resulting `OpenedDocumentInfo` DTO serialized as JSON. Plain
    /// `handle`/`page_index`/`direction` arguments, not a request DTO —
    /// `move_page_cmd` takes bare `State` + scalar arguments on the
    /// desktop side and `tauri.ts` calls it as `invoke("move_page_cmd", {
    /// handle, pageIndex, direction })` — see this module's doc comment
    /// for why this is the one place the brief's own request-JSON
    /// shorthand didn't match the real desktop shape. Thin wrapper over
    /// `openpdfedit_session::pages::move_page_impl`.
    #[wasm_bindgen(js_name = movePage)]
    pub fn move_page(
        &self,
        handle: u32,
        page_index: u32,
        direction: &str,
    ) -> Result<String, JsValue> {
        let direction = match direction {
            "Up" => MoveDirection::Up,
            "Down" => MoveDirection::Down,
            other => {
                return Err(to_js_err(format!(
                    "unknown move direction {other:?} — expected \"Up\" or \"Down\""
                )))
            }
        };
        let info = move_page_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
            page_index,
            direction,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Sets `page_index`'s crop box to `rect` (`[x0, y0, x1, y1]` in PDF
    /// page-space points, a JS `Float32Array` at the call site) in
    /// `handle`'s document, returning the resulting `OpenedDocumentInfo`
    /// DTO serialized as JSON. Plain `handle`/`page_index`/`rect`
    /// arguments — `set_crop_box_cmd` takes bare `State` + scalar
    /// arguments (including a bare `rect: [f32; 4]`) on the desktop side —
    /// see this module's doc comment. Thin wrapper over
    /// `openpdfedit_session::pages::set_crop_box_impl`.
    #[wasm_bindgen(js_name = setCropBox)]
    pub fn set_crop_box(
        &self,
        handle: u32,
        page_index: u32,
        rect: &[f32],
    ) -> Result<String, JsValue> {
        let rect: [f32; 4] = rect
            .try_into()
            .map_err(|_| to_js_err("rect must have exactly 4 elements: [x0, y0, x1, y1]"))?;
        let info = set_crop_box_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
            page_index,
            rect,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Extracts `page_indices` from the currently-open document identified
    /// by `request_json`'s `handle` and returns the **extracted
    /// document's raw bytes** (`Uint8Array`) — not an `OpenedDocumentInfo`
    /// JSON string, unlike every other mutating method on this type. See
    /// this module's doc comment ("Forms/pages surface") for the full
    /// rationale: `openpdfedit_session::pages::ExtractRequest`/
    /// `extract_pages_impl` are desktop-only (path-based, `#[cfg(not(
    /// target_arch = "wasm32"))]`) and aren't even compiled into this
    /// crate's wasm32 build, so this method instead reads the source
    /// document's current working-copy bytes directly from
    /// `self.state.store` (the same source [`Self::working_copy_bytes`]
    /// reads from) and hands them to the portable byte-level
    /// [`extract_pages_bytes`]. The source document at `handle` is left
    /// completely untouched — no mutation, no handle rotation, mirroring
    /// `extract_pages_impl`'s own behavior on the desktop. `wasm.ts`'s
    /// `extractPages` is what turns these bytes into a real, newly-opened
    /// `OpenedDocument` (see this module's doc comment for how).
    #[wasm_bindgen(js_name = extractPages)]
    pub fn extract_pages(&self, request_json: &str) -> Result<js_sys::Uint8Array, JsValue> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ExtractPagesRequest {
            handle: u32,
            page_indices: Vec<u32>,
        }
        let request: ExtractPagesRequest = serde_json::from_str(request_json).map_err(to_js_err)?;

        let path = {
            let docs = self.state.docs.lock().expect("docs lock poisoned");
            docs.get(&(request.handle as DocHandle))
                .map(|d| d.path.clone())
                .ok_or_else(|| to_js_err(format!("unknown document handle {}", request.handle)))?
        };
        let source_bytes = self.state.store.read(&path).map_err(to_js_err)?;
        let extracted =
            extract_pages_bytes(&source_bytes, &request.page_indices).map_err(to_js_err)?;
        Ok(js_sys::Uint8Array::from(extracted.as_slice()))
    }

    /// Every signature found on `handle`'s document, as a JSON array of
    /// `SignatureInfoDto` — structural inspection only, never a
    /// cryptographic verdict (`isVerified` is always `false`; see
    /// `openpdfedit_session::signatures`'s module doc). Read-only, plain
    /// `handle` argument, matching `tauri.ts`'s `listSignatures(handle)`
    /// call shape — see this module's doc comment ("Signatures/redact/
    /// textedit/image surface") for the full argument-shape rationale.
    /// Thin wrapper over `openpdfedit_session::signatures::list_signatures_impl`,
    /// which — unlike every other method on this type — never touches
    /// `self.state.engine` at all (it only reads working-copy bytes
    /// through `self.state.store`).
    #[wasm_bindgen(js_name = listSignatures)]
    pub fn list_signatures(&self, handle: u32) -> Result<String, JsValue> {
        let signatures =
            list_signatures_impl(&self.state.docs, &*self.state.store, handle as DocHandle)
                .map_err(to_js_err)?;
        serde_json::to_string(&signatures).map_err(to_js_err)
    }

    /// Finds every occurrence of `query` in the open document, returning
    /// a `SearchResultsDto` serialized as JSON. Read-only: no mutation,
    /// no handle rotation, nothing written — see
    /// `openpdfedit_session::search`'s module doc for why this one is
    /// engine-only and needed no `WorkingStore` plumbing to become
    /// portable.
    #[wasm_bindgen(js_name = searchDocument)]
    pub fn search_document(
        &self,
        handle: u32,
        query: &str,
        match_case: bool,
        whole_word: bool,
    ) -> Result<String, JsValue> {
        let results = search_document_impl(
            &self.state.engine,
            handle as DocHandle,
            query,
            match_case,
            whole_word,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&results).map_err(to_js_err)
    }

    /// The document's outline (bookmarks) as a flattened, depth-tagged
    /// JSON array of `OutlineEntryDto`. Read-only: reads the parsed
    /// object graph only — no engine, no working copy, no mutation.
    #[wasm_bindgen(js_name = documentOutline)]
    pub fn document_outline(&self, handle: u32) -> Result<String, JsValue> {
        let entries =
            document_outline_impl(&self.state.docs, handle as DocHandle).map_err(to_js_err)?;
        serde_json::to_string(&entries).map_err(to_js_err)
    }

    /// Bakes markup (and optionally filled form values) into the page,
    /// returning a `FlattenResultDto` JSON string. Mutating: rotates the
    /// handle like every other mutating method here, and is undoable.
    #[wasm_bindgen(js_name = flattenDocument)]
    pub fn flatten_document(&self, request_json: &str) -> Result<String, JsValue> {
        let request: FlattenDocumentRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let result = flatten_document_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&result).map_err(to_js_err)
    }

    /// Every markup annotation on the document, serialized as XFDF —
    /// returns an `ExportXfdfDto` JSON string carrying the XML plus a
    /// suggested filename. Read-only. The extension hands the XML to a
    /// download rather than writing a file, which is why the portable
    /// half returns a string instead of taking an output path.
    #[wasm_bindgen(js_name = exportXfdf)]
    pub fn export_xfdf(&self, handle: u32) -> Result<String, JsValue> {
        let exported =
            export_xfdf_impl(&self.state.docs, handle as DocHandle).map_err(to_js_err)?;
        serde_json::to_string(&exported).map_err(to_js_err)
    }

    /// Adds every annotation in `xml` this app can draw, returning an
    /// `ImportXfdfDto` JSON string. Mutating: rotates the handle, and is
    /// undoable.
    #[wasm_bindgen(js_name = importXfdf)]
    pub fn import_xfdf(&self, handle: u32, xml: &str) -> Result<String, JsValue> {
        let result = import_xfdf_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            handle as DocHandle,
            xml,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&result).map_err(to_js_err)
    }

    /// Stamps page numbers or Bates numbering into a margin of each
    /// page, returning the resulting `OpenedDocumentInfo` DTO as JSON.
    /// Mutating: rotates the handle, and is undoable.
    #[wasm_bindgen(js_name = numberPages)]
    pub fn number_pages(&self, request_json: &str) -> Result<String, JsValue> {
        let request: NumberPagesRequest = serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = number_pages_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// The encrypted bytes for this document's working copy, for the
    /// extension to hand to a download. Export, not mutation: the open
    /// document is untouched and no handle rotates — see
    /// `openpdfedit_session::encrypt`'s module doc for why encrypting in
    /// place would be the wrong shape.
    #[wasm_bindgen(js_name = encryptDocumentBytes)]
    pub fn encrypt_document_bytes_js(
        &self,
        handle: u32,
        choices_json: &str,
    ) -> Result<Vec<u8>, JsValue> {
        let choices: EncryptChoices = serde_json::from_str(choices_json).map_err(to_js_err)?;
        encrypt_document_bytes(&self.state, handle as DocHandle, &choices).map_err(to_js_err)
    }

    /// Permanently removes the content (text and images, not just a black
    /// box painted over live data — see `openpdfedit-redact`'s module doc)
    /// under `rect` on one page of `request`'s document, returning the
    /// resulting `OpenedDocumentInfo` DTO serialized as JSON, exactly like
    /// every other mutating method on this type. Takes only
    /// `request_json` — `RedactPageRequest` already carries its own
    /// `handle` field, matching `tauri.ts`'s `redactPage({ request })`
    /// call shape (see this module's doc comment). Thin wrapper over
    /// `openpdfedit_session::redact::redact_page_impl`.
    #[wasm_bindgen(js_name = redactPage)]
    pub fn redact_page(&self, request_json: &str) -> Result<String, JsValue> {
        let request: RedactPageRequest = serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = redact_page_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Tiled text/logo watermark baked into the document's pages (see
    /// `openpdfedit-watermark`'s module doc). Mutating: takes the same
    /// camelCase `ApplyWatermarkRequest` JSON as the desktop command
    /// (with the optional logo as base64 RGBA inside the request) and
    /// returns the rotated `OpenedDocumentInfo` JSON, exactly like
    /// [`Self::redact_page`]. Thin wrapper over
    /// `openpdfedit_session::watermark::apply_watermark_impl`.
    #[wasm_bindgen(js_name = applyWatermark)]
    pub fn apply_watermark(&self, request_json: &str) -> Result<String, JsValue> {
        let request: ApplyWatermarkRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = apply_watermark_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Every text run on `page_index` of `handle`'s document, as a JSON
    /// array of `TextRunDto`. Read-only, plain `handle`/`page_index`
    /// arguments, matching `tauri.ts`'s `listTextRuns(handle, pageIndex)`
    /// call shape — see this module's doc comment. Thin wrapper over
    /// `openpdfedit_session::textedit::list_text_runs_impl`, which reads
    /// only the already-open document's object graph (no engine, no
    /// store).
    #[wasm_bindgen(js_name = listTextRuns)]
    pub fn list_text_runs(&self, handle: u32, page_index: u32) -> Result<String, JsValue> {
        let runs = list_text_runs_impl(&self.state.docs, handle as DocHandle, page_index)
            .map_err(to_js_err)?;
        serde_json::to_string(&runs).map_err(to_js_err)
    }

    /// Substitutes the text of one run (identified by `runIndex` into a
    /// freshly re-listed [`Self::list_text_runs`] array — see
    /// `openpdfedit-textedit`'s module doc for why index-based, not an
    /// opaque id or re-sent coordinates) on `request`'s document, returning
    /// the resulting `OpenedDocumentInfo` DTO serialized as JSON. Takes
    /// only `request_json` — see [`Self::redact_page`]'s doc for why. Thin
    /// wrapper over `openpdfedit_session::textedit::edit_text_run_impl`.
    #[wasm_bindgen(js_name = editTextRun)]
    pub fn edit_text_run(&self, request_json: &str) -> Result<String, JsValue> {
        let request: EditTextRunRequest = serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = edit_text_run_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Relocates one text run by `(dx, dy)` PDF-point offsets without
    /// touching its content — see
    /// `openpdfedit_session::textedit::move_text_run_impl`'s doc for why
    /// this imposes no `isEditable` requirement, unlike
    /// [`Self::edit_text_run`]. Returns the resulting `OpenedDocumentInfo`
    /// DTO serialized as JSON. Takes only `request_json` — see
    /// [`Self::redact_page`]'s doc for why. Thin wrapper over
    /// `openpdfedit_session::textedit::move_text_run_impl`.
    #[wasm_bindgen(js_name = moveTextRun)]
    pub fn move_text_run(&self, request_json: &str) -> Result<String, JsValue> {
        let request: MoveTextRunRequest = serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = move_text_run_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Every image placement on `page_index` of `handle`'s document, as a
    /// JSON array of `ImagePlacementDto`. Read-only, plain
    /// `handle`/`page_index` arguments, matching `tauri.ts`'s
    /// `listImagePlacements(handle, pageIndex)` call shape — see this
    /// module's doc comment. Thin wrapper over
    /// `openpdfedit_session::textedit::list_image_placements_impl`, which
    /// — like [`Self::list_text_runs`] — reads only the already-open
    /// document's object graph.
    #[wasm_bindgen(js_name = listImagePlacements)]
    pub fn list_image_placements(&self, handle: u32, page_index: u32) -> Result<String, JsValue> {
        let placements =
            list_image_placements_impl(&self.state.docs, handle as DocHandle, page_index)
                .map_err(to_js_err)?;
        serde_json::to_string(&placements).map_err(to_js_err)
    }

    /// Relocates one image placement (identified by `placementIndex` into
    /// a freshly re-listed [`Self::list_image_placements`] array) by `(dx,
    /// dy)` PDF-point offsets on `request`'s document, returning the
    /// resulting `OpenedDocumentInfo` DTO serialized as JSON. Takes only
    /// `request_json` — see [`Self::redact_page`]'s doc for why. Thin
    /// wrapper over `openpdfedit_session::textedit::move_image_impl`.
    #[wasm_bindgen(js_name = moveImage)]
    pub fn move_image(&self, request_json: &str) -> Result<String, JsValue> {
        let request: MoveImageRequest = serde_json::from_str(request_json).map_err(to_js_err)?;
        let info = move_image_impl(
            &self.state.engine,
            &self.state.docs,
            &self.state.history,
            &*self.state.store,
            request,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&info).map_err(to_js_err)
    }

    /// Merges the currently-open document at `request_json`'s
    /// `openHandle` (if given — its **live working-copy** bytes, read via
    /// `self.state.store`, not a stale snapshot; see
    /// [`merge_open_doc_with_bytes`]'s doc) ahead of every source packed
    /// into `sources_buffer`, and returns the merged document's raw bytes
    /// (`Uint8Array`) — not `OpenedDocumentInfo` JSON. Same "no filesystem
    /// to open a new document from" situation [`Self::extract_pages`] is
    /// in, for the same reason (see its doc): `wasm.ts`'s `mergeDocuments`
    /// is what turns these bytes into a real, newly opened
    /// `OpenedDocument`, mirroring `extractPages`' landed pattern. The
    /// document at `openHandle`, if any, is left completely untouched —
    /// no mutation, no handle rotation — matching
    /// `merge_documents_impl`'s own behavior on the desktop (a merge never
    /// rotates the *source* handle; only the brand-new merged document
    /// gets opened, under its own fresh handle).
    ///
    /// **Wasm boundary for multiple source files.** wasm-bindgen cannot
    /// marshal a `Vec<Vec<u8>>` parameter directly, so this method needed
    /// a deliberate design for "N source files' worth of bytes, in one
    /// call." Two shapes were on the table (see task-2-brief.md):
    ///
    /// 1. A stateful two-step API — `beginMerge(openHandle)`, then
    ///    `addMergeSource(bytes)` once per source, then `finishMerge() ->
    ///    bytes`. This would require `WasmSession` to grow a new mutable
    ///    staging field (e.g. a `Mutex<Vec<Vec<u8>>>`) that has to live
    ///    *between* otherwise-independent calls — real complexity for real
    ///    hazards: a second merge started before the first one's
    ///    `finishMerge`, or a caller that simply forgets to call it, would
    ///    leave staged buffers stuck in session state with nothing to
    ///    notice or clean them up.
    /// 2. A single call carrying every source's bytes as one flat,
    ///    length-prefixed buffer (**chosen**): `sources_buffer` is a
    ///    concatenation of `[u32 length, little-endian][that many bytes]`
    ///    records, one per source, decoded by
    ///    [`parse_length_prefixed_sources`].
    ///
    /// (2) wins on the grounds this whole crate already runs on: every
    /// method here is synchronous, single-threaded, on the one JS main
    /// thread (see this module's doc, "Why no `async fn` exports and no
    /// Workers") — there is no scenario where interleaving several calls
    /// would help, so a stateful multi-call API buys nothing (2) doesn't
    /// already give for free, while (2) adds no new mutable state to
    /// `WasmSession` at all and can never be left half-finished. Building
    /// the length-prefixed buffer on the JS side (`wasm.ts`) is a handful
    /// of lines with `DataView`/`Uint8Array.set` — no heavier than the
    /// JSON marshaling every other mutating method on this type already
    /// does.
    #[wasm_bindgen(js_name = mergeDocuments)]
    pub fn merge_documents(
        &self,
        request_json: &str,
        sources_buffer: &[u8],
    ) -> Result<js_sys::Uint8Array, JsValue> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct MergeDocumentsRequest {
            open_handle: Option<u32>,
        }
        let request: MergeDocumentsRequest =
            serde_json::from_str(request_json).map_err(to_js_err)?;
        let sources = parse_length_prefixed_sources(sources_buffer).map_err(to_js_err)?;

        let merged = merge_open_doc_with_bytes(
            &self.state.docs,
            &*self.state.store,
            request.open_handle.map(|h| h as DocHandle),
            sources,
        )
        .map_err(to_js_err)?;

        Ok(js_sys::Uint8Array::from(merged.as_slice()))
    }

    /// Compares two documents' bytes — text mode always, pixel mode too
    /// if `optionsJson`'s `pixelTargetWidth` is present — and returns a
    /// `CompareReportDto` JSON string. Thin marshaling over the
    /// already-portable `openpdfedit_session::compare::compare_bytes`
    /// (verified: no `#[cfg(not(target_arch = "wasm32"))]` gate anywhere
    /// on it or its callees — only the path-based `CompareRequest`/
    /// `compare_documents_impl` half of that module is desktop-only, see
    /// that module's own doc). Neither document needs to already be open
    /// in this session; this is a one-shot, read-only comparison, exactly
    /// like the desktop's `compare_documents_cmd` — no handle, no
    /// rotation, nothing persisted. `wasm.ts`'s `compareDocuments` is
    /// responsible for turning `types.ts`'s `CompareDocumentsRequest`
    /// (two path *strings*) into `bytesA`/`bytesB` before calling this —
    /// see that method's own doc for how it resolves the currently-open
    /// document's live working-copy bytes for one side and a
    /// freshly-picked file's raw bytes for the other, with no change to
    /// `types.ts`'s `Backend` interface at all.
    #[wasm_bindgen(js_name = compareDocuments)]
    pub fn compare_documents(
        &self,
        bytes_a: &[u8],
        bytes_b: &[u8],
        options_json: &str,
    ) -> Result<String, JsValue> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CompareOptions {
            pixel_target_width: Option<u32>,
        }
        let options: CompareOptions = serde_json::from_str(options_json).map_err(to_js_err)?;
        let report = compare_bytes(
            &self.state.engine,
            bytes_a,
            bytes_b,
            options.pixel_target_width,
        )
        .map_err(to_js_err)?;
        serde_json::to_string(&report).map_err(to_js_err)
    }
}

/// Parses [`WasmSession::merge_documents`]'s `sources_buffer` argument: a
/// flat concatenation of `[u32 length, little-endian][that many bytes]`
/// records, one per extra merge source, with no trailing padding. See
/// that method's doc for why this length-prefixed single-buffer shape was
/// chosen over a stateful two-step begin/add/finish API. Returns a
/// `String` error (not `SessionError`) since this is pure wasm-boundary
/// marshaling, not a session-level failure.
fn parse_length_prefixed_sources(buffer: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut sources = Vec::new();
    let mut offset = 0usize;
    while offset < buffer.len() {
        let len_bytes = buffer.get(offset..offset + 4).ok_or_else(|| {
            format!(
                "merge sources buffer is truncated: {} byte(s) left at offset {offset}, need 4 for a length prefix",
                buffer.len() - offset
            )
        })?;
        let len = u32::from_le_bytes(len_bytes.try_into().expect("slice is exactly 4 bytes long"))
            as usize;
        offset += 4;
        let bytes = buffer.get(offset..offset + len).ok_or_else(|| {
            format!(
                "merge sources buffer is truncated: length prefix says {len} byte(s) at offset \
                 {offset}, but only {} remain",
                buffer.len().saturating_sub(offset)
            )
        })?;
        sources.push(bytes.to_vec());
        offset += len;
    }
    Ok(sources)
}

#[cfg(test)]
mod merge_sources_tests {
    use super::parse_length_prefixed_sources;

    #[test]
    fn parses_zero_sources_from_an_empty_buffer() {
        assert_eq!(
            parse_length_prefixed_sources(&[]).unwrap(),
            Vec::<Vec<u8>>::new()
        );
    }

    #[test]
    fn parses_multiple_sources_in_order() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&3u32.to_le_bytes());
        buffer.extend_from_slice(b"abc");
        buffer.extend_from_slice(&0u32.to_le_bytes());
        buffer.extend_from_slice(&5u32.to_le_bytes());
        buffer.extend_from_slice(b"hello");

        let sources = parse_length_prefixed_sources(&buffer).unwrap();
        assert_eq!(
            sources,
            vec![b"abc".to_vec(), Vec::<u8>::new(), b"hello".to_vec()]
        );
    }

    #[test]
    fn rejects_a_buffer_truncated_mid_length_prefix() {
        let err = parse_length_prefixed_sources(&[1, 0]).unwrap_err();
        assert!(err.contains("truncated"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_a_buffer_truncated_mid_payload() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&10u32.to_le_bytes());
        buffer.extend_from_slice(b"short");

        let err = parse_length_prefixed_sources(&buffer).unwrap_err();
        assert!(err.contains("truncated"), "unexpected error: {err}");
    }
}
