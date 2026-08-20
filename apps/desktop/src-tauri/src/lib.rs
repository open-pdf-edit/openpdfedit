//! Tauri command layer for the openpdfedit desktop shell.
//!
//! Document lifecycle (`open_document`/`close_document`) goes through
//! Tauri's normal JSON IPC — small payloads, fine as-is. Pixels do not:
//! they're served over a custom `tile://` URI scheme as a raw RGBA byte
//! response (width/height in headers), so the front-end does one `fetch()`
//! per tile and builds `ImageData` directly from the response body. No
//! JSON, no base64 inflation — see PLAN.md §5/§6 on why "JSON IPC for
//! pixels" doesn't survive contact with a real virtualized-scrolling
//! viewer.
//!
//! State holds two things per open document, keyed by the **same**
//! [`DocHandle`]: the read/render side (one process-wide [`EngineHandle`]
//! covering every open document — see `openpdfedit-engine`'s `thread`
//! module) and the write side (one [`openpdfedit_doc::Document`] per
//! document, since edits are per-document state, not shared). See
//! `annotations` for how a write (e.g. `add_annotation`) reconciles the
//! two: after `save_incremental` writes new bytes to disk, the engine
//! side is closed and reopened against the updated file, which means
//! **the `DocHandle` a client is holding can change after an edit** —
//! every mutating command returns the new [`OpenedDocument`] so the
//! front-end always has the current one.
//!
//! The document/undo-redo/open-save-save-as core that doesn't touch
//! `tauri::` types lives in the engine-generic `openpdfedit-session`
//! crate (see that crate's module doc for exactly what moved there vs.
//! what stayed here) — `AppState`, `OpenDoc`, `DocHistory`,
//! `OpenedDocument` below are re-exports/aliases of that crate's types,
//! kept under these names so every other command module in this crate
//! keeps compiling unchanged. Every other command module's real logic
//! moved there too (`openpdfedit_session::annotations`,
//! `openpdfedit_session::forms`, `openpdfedit_session::pages`,
//! `openpdfedit_session::textedit`, `openpdfedit_session::redact`,
//! `openpdfedit_session::signatures`, `openpdfedit_session::compare`) —
//! every command module in this crate other than `ocr.rs`/`license.rs`
//! is now just `#[tauri::command]` wrappers. The shared "mutate, save,
//! rotate the render handle" pathway (`commit_mutation`) moved there as
//! well, generic over the caller's error type; now that every mutating
//! command module has moved, nothing in this crate still needs a
//! same-named non-generic wrapper for it (compare the still-needed one
//! for [`capture_pre_edit_snapshot`] below, which `ocr.rs` — out of
//! scope for every one of these moves — still calls directly).

mod annotations;
mod compare;
mod compress;
mod field_create;
mod forms;
mod license;
mod ocr;
mod pages;
mod redact;
mod signatures;
mod textedit;
mod watermark;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use openpdfedit_doc::DocError;
use openpdfedit_engine::{DocHandle, EngineError, EngineHandle};
use openpdfedit_session::SessionError;
pub(crate) use openpdfedit_session::{
    close_document_impl, commit_undo_snapshot, reopen_after_write, DocHistory, FsWorkingStore,
    OpenDoc,
};
use serde::Serialize;
use tauri::http::{Request, Response, StatusCode};
use tauri::{Emitter, Manager, State};
use thiserror::Error;

/// What the desktop's hand-written `AppState` struct used to be: the
/// engine, the open-docs map, and the undo/redo history map. Aliased
/// directly to `openpdfedit_session::SessionState<EngineHandle>` so
/// every existing `state.engine`/`state.docs`/`state.history` field
/// access elsewhere in this crate keeps compiling unchanged (the aliased
/// type's fields are `pub`).
pub(crate) type AppState = openpdfedit_session::SessionState<EngineHandle>;

/// The DTO every open/save/undo/redo command hands back to the
/// front-end. Aliased to `openpdfedit_session::OpenedDocumentInfo` for
/// the same reason as [`AppState`] above.
pub(crate) type OpenedDocument = openpdfedit_session::OpenedDocumentInfo;

/// Thin, non-generic wrapper over `openpdfedit_session::
/// capture_pre_edit_snapshot`, which stayed generic over the caller's
/// error type. This can't be a bare `pub(crate) use` re-export: at the
/// call site `capture_pre_edit_snapshot(&path)?` (used directly by
/// `ocr.rs`, which drives its own bespoke mutate/save/snapshot sequence
/// rather than going through `openpdfedit_session::commit_mutation`),
/// nothing pins down the generic `Err` — `CommandError` has more than one
/// applicable `From` impl, so `?`'s `From`-search is genuinely ambiguous
/// (confirmed with a throwaway repro: `cannot infer type for type
/// parameter `Err`` / E0283). Fixing `Err = CommandError` here, in a
/// function whose own return type is already concrete, sidesteps that
/// entirely. Also supplies the desktop's `FsWorkingStore` at the call
/// site, so `ocr.rs` itself doesn't need to know a `WorkingStore` exists.
fn capture_pre_edit_snapshot(path: &Path) -> Result<Vec<u8>, CommandError> {
    openpdfedit_session::capture_pre_edit_snapshot(&FsWorkingStore, path)
}

#[derive(Debug, Error, Serialize)]
enum CommandError {
    #[error("{0}")]
    Engine(String),
    #[error("{0}")]
    Doc(String),
    #[error("{0}")]
    Annot(String),
    #[error("{0}")]
    Ocr(String),
    #[error("unknown document handle {0}")]
    UnknownHandle(DocHandle),
    #[error("I/O error: {0}")]
    Io(String),
}

impl From<EngineError> for CommandError {
    fn from(e: EngineError) -> Self {
        CommandError::Engine(e.to_string())
    }
}

impl From<DocError> for CommandError {
    fn from(e: DocError) -> Self {
        CommandError::Doc(e.to_string())
    }
}

impl From<openpdfedit_annot::AnnotError> for CommandError {
    fn from(e: openpdfedit_annot::AnnotError) -> Self {
        CommandError::Annot(e.to_string())
    }
}

impl From<openpdfedit_ocr::OcrError> for CommandError {
    fn from(e: openpdfedit_ocr::OcrError) -> Self {
        CommandError::Ocr(e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}

/// Converts the `openpdfedit-session` crate's leaner error (no `Ocr`
/// variant — that crate never touches `openpdfedit-ocr`; it does have
/// `Annot`, since `openpdfedit_session::annotations` lives there now)
/// into this crate's app-wide command error, preserving each variant's
/// identity rather than collapsing everything to a string.
impl From<SessionError> for CommandError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::Engine(s) => CommandError::Engine(s),
            SessionError::Doc(s) => CommandError::Doc(s),
            SessionError::Annot(s) => CommandError::Annot(s),
            SessionError::UnknownHandle(h) => CommandError::UnknownHandle(h),
            SessionError::Io(s) => CommandError::Io(s),
        }
    }
}

/// Undoes the most recent edit for the document at `handle`: restores
/// the file to its pre-edit bytes and rotates the render handle, same as
/// any other write. Errors if there's nothing to undo (mirrors
/// `commit_mutation`-style commands — the front-end should already be
/// disabling the Undo button via `OpenedDocument::can_undo`, so this is a
/// defensive backstop, not the primary UX guard). Thin wrapper over
/// `openpdfedit_session::undo_impl`.
#[tauri::command]
fn undo_cmd(state: State<'_, AppState>, handle: DocHandle) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::undo_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
    )
    .map_err(Into::into)
}

/// The redo half of [`undo_cmd`] — see that command's doc. Thin wrapper
/// over `openpdfedit_session::redo_impl`.
#[tauri::command]
fn redo_cmd(state: State<'_, AppState>, handle: DocHandle) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::redo_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
    )
    .map_err(Into::into)
}

/// Thin wrapper over `openpdfedit_session::open_document_impl`.
#[tauri::command]
fn open_document(state: State<'_, AppState>, path: String) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::open_document_impl(&state, std::path::Path::new(&path)).map_err(Into::into)
}

/// Closes the window for real, after the front-end has resolved the
/// unsaved-changes prompt raised by the `close-requested` event.
#[tauri::command]
fn close_window(window: tauri::Window) {
    let _ = window.destroy();
}

/// Writes the working copy over the file the user opened. This is the
/// *only* path that touches their file — every other command edits the
/// scratch copy (see [`OpenDoc`]). Thin wrapper over
/// `openpdfedit_session::save_document_impl`.
#[tauri::command]
fn save_document(
    state: State<'_, AppState>,
    handle: DocHandle,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::save_document_impl(&state, handle).map_err(Into::into)
}

/// Writes the working copy to a new location, which becomes the target
/// of subsequent saves. Thin wrapper over
/// `openpdfedit_session::save_document_as_impl`.
#[tauri::command]
fn save_document_as(
    state: State<'_, AppState>,
    handle: DocHandle,
    path: String,
) -> Result<OpenedDocument, CommandError> {
    openpdfedit_session::save_document_as_impl(&state, handle, Path::new(&path)).map_err(Into::into)
}

/// Thin wrapper over `openpdfedit_session::close_document_impl` — as of
/// Phase 2's final-review fix wave (C1), this is also what finally cleans
/// up a closed document's scratch working-copy file (`FsWorkingStore::remove`
/// is a plain `std::fs::remove_file`) and its `DocHistory` undo/redo
/// entry, neither of which the old inline `state.engine.close(handle)` +
/// `state.docs...remove(&handle)` body ever did — a behavior improvement
/// this crate's `WorkingStore` doc already flagged as sanctioned
/// follow-up ("`close_document` never cleans scratch files").
#[tauri::command]
fn close_document(state: State<'_, AppState>, handle: DocHandle) {
    close_document_impl(
        &state.engine,
        &state.docs,
        &state.history,
        &*state.store,
        handle,
    );
}

/// Parses `tile://localhost/<handle>/<page_index>/<target_width>` into its
/// three numeric path segments. The `localhost` host segment is required
/// by how Tauri/the OS webview normalize custom-scheme URLs across
/// platforms; the real routing is in the path.
fn parse_tile_path(path: &str) -> Option<(DocHandle, u32, u32)> {
    let mut parts = path.trim_start_matches('/').split('/');
    let handle: DocHandle = parts.next()?.parse().ok()?;
    let page_index: u32 = parts.next()?.parse().ok()?;
    let target_width: u32 = parts.next()?.parse().ok()?;
    Some((handle, page_index, target_width))
}

/// The packaged app's frontend loads from the `tauri://localhost` origin
/// (macOS/Linux) — a *different* origin than the `tile://` custom scheme
/// this handler serves, as far as the webview's `fetch()` CORS
/// enforcement is concerned, even though both are registered by the same
/// app. Without this header, every `tile://` response is fetched and
/// returned by this handler successfully (a real 200, real bytes — this
/// isn't a rendering or IPC problem) but silently discarded by the
/// webview before JS ever sees it: `fetch()` rejects with "Origin
/// tauri://localhost is not allowed by Access-Control-Allow-Origin,"
/// which reads exactly like a network failure from the caller's side.
/// Confirmed via Safari Web Inspector against a real packaged build —
/// this was never caught by `cargo test` because nothing in this
/// codebase's test suite drives an actual webview's `fetch()`, only the
/// Rust-side HTTP handler directly (which — correctly — never saw the
/// browser-side CORS check fail). Every branch below needs the header,
/// not just the success path: the webview blocks reading a cross-origin
/// response's status/body regardless of whether that status is 200, 400,
/// or 404.
const TILE_CORS_HEADER: (&str, &str) = ("Access-Control-Allow-Origin", "*");

fn tile_response(engine: &EngineHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path();
    let Some((handle, page_index, target_width)) = parse_tile_path(path) else {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header(TILE_CORS_HEADER.0, TILE_CORS_HEADER.1)
            .body(b"malformed tile request path".to_vec())
            .expect("static response is well-formed");
    };

    match engine.render_page(handle, page_index, target_width) {
        Ok(tile) => Response::builder()
            .status(StatusCode::OK)
            .header(TILE_CORS_HEADER.0, TILE_CORS_HEADER.1)
            // The CORS spec hides all but a small "simple" allowlist of
            // response headers from JS by default — Content-Type is on
            // that list, but X-Tile-Width/X-Tile-Height are not, so
            // without this, `res.headers.get("X-Tile-Width")` silently
            // returns null (which `Number(null)` turns into 0) even
            // though the body itself now arrives fine post-CORS-fix.
            // Confirmed live: a real render came back with a real body
            // but "0x0" dimensions until this was added.
            .header(
                "Access-Control-Expose-Headers",
                "X-Tile-Width, X-Tile-Height",
            )
            .header("Content-Type", "application/octet-stream")
            .header("X-Tile-Width", tile.width.to_string())
            .header("X-Tile-Height", tile.height.to_string())
            // The tile Arc may be cached and shared with other in-flight
            // requests; cloning the bytes here is the one copy this path
            // can't avoid (the response body must own its bytes).
            .body(tile.rgba.clone())
            .expect("tile response is well-formed"),
        Err(e) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(TILE_CORS_HEADER.0, TILE_CORS_HEADER.1)
            .body(e.to_string().into_bytes())
            .expect("error response is well-formed"),
    }
}

/// Where to load the PDFium dynamic library from.
///
/// In a packaged app it ships inside the bundle (see `tauri.conf.json`'s
/// `bundle.resources`) and is found through Tauri's resource directory.
/// That indirection is the whole point: `PdfiumEngine::new_dev()`'s
/// fallback locates the library through `env!("CARGO_MANIFEST_DIR")`, a
/// path baked in at *compile* time that points into the build machine's
/// own source tree. It resolves on the machine that produced the build
/// and nowhere else — so a distributed app failed to load PDFium and
/// died before showing a window, which looks exactly like "double-click
/// does nothing". Returning `None` here falls back to the dev lookup,
/// which is what `tauri dev` should use.
fn bundled_pdfium_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?;
    // Only claim the directory if the library is actually in it, so a
    // packaging mistake falls through to the dev path rather than
    // failing outright.
    let present = ["libpdfium.dylib", "libpdfium.so", "pdfium.dll"]
        .iter()
        .any(|name| dir.join(name).exists());
    present.then_some(dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Engine construction moved into `setup` because locating the
            // bundled library needs the `AppHandle`, which doesn't exist
            // until the builder runs.
            let engine = EngineHandle::spawn(bundled_pdfium_dir(app.handle()))?;
            app.manage(AppState {
                engine,
                docs: Mutex::new(HashMap::new()),
                history: Mutex::new(HashMap::new()),
                store: Box::new(FsWorkingStore),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing with unsaved edits must not silently discard them.
            // The decision belongs to the user, so block the close and
            // hand it to the front-end, which knows whether anything is
            // dirty and can offer Save / Discard / Cancel. It calls
            // `close_window` once resolved.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.emit("close-requested", ());
            }
        })
        .register_uri_scheme_protocol("tile", move |ctx, request| {
            // Pulled from managed state rather than a captured clone, so
            // the engine can be built in `setup` above.
            let state = ctx.app_handle().state::<AppState>();
            tile_response(&state.engine, &request)
        })
        .invoke_handler(tauri::generate_handler![
            open_document,
            close_document,
            save_document,
            save_document_as,
            close_window,
            undo_cmd,
            redo_cmd,
            annotations::add_annotation_cmd,
            annotations::list_page_annotations,
            annotations::delete_annotation_cmd,
            annotations::text_selection_quads_cmd,
            pages::rotate_page_cmd,
            pages::delete_page_cmd,
            pages::set_crop_box_cmd,
            pages::move_page_cmd,
            pages::merge_documents_cmd,
            pages::extract_pages_cmd,
            forms::list_form_fields_cmd,
            forms::fill_form_fields_cmd,
            ocr::ocr_document_cmd,
            signatures::list_signatures_cmd,
            redact::redact_page_cmd,
            watermark::apply_watermark_cmd,
            compress::compress_document_cmd,
            license::import_license_cmd,
            license::get_license_status_cmd,
            textedit::list_text_runs_cmd,
            textedit::edit_text_run_cmd,
            textedit::move_text_run_cmd,
            textedit::list_image_placements_cmd,
            textedit::move_image_cmd,
            field_create::create_form_field_cmd,
            compare::compare_documents_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Shared across every test module in this crate (`tests` below,
/// `annotations::tests`, and any future one) — not one `EngineHandle`
/// per module. PDFium's global init may only run once per process, and
/// cargo runs every `#[test]` fn in one binary's process, concurrently
/// by default, so two independent module-local singletons would
/// reproduce the exact SIGSEGV `openpdfedit-engine`'s own tests hit
/// before they were consolidated the same way — see that crate's module
/// docs for the full story.
#[cfg(test)]
pub(crate) mod test_support {
    use openpdfedit_engine::EngineHandle;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    pub(crate) fn shared_engine() -> Option<&'static EngineHandle> {
        static ENGINE: OnceLock<Option<EngineHandle>> = OnceLock::new();
        ENGINE
            .get_or_init(
                || match EngineHandle::spawn(dev_vendor_lib_dir_for_tests()) {
                    Ok(e) => Some(e),
                    Err(e) => {
                        eprintln!("skipping: PDFium not available ({e})");
                        None
                    }
                },
            )
            .as_ref()
    }

    pub(crate) fn dev_vendor_lib_dir_for_tests() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // apps/desktop/src-tauri -> workspace root
        let workspace_root = manifest_dir.parent()?.parent()?.parent()?;
        let dir = workspace_root.join(if cfg!(windows) {
            ".vendor/pdfium/bin"
        } else {
            ".vendor/pdfium/lib"
        });
        dir.exists().then_some(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_tile_path() {
        assert_eq!(parse_tile_path("/42/3/800"), Some((42, 3, 800)));
    }

    #[test]
    fn parses_path_without_leading_slash() {
        assert_eq!(parse_tile_path("42/3/800"), Some((42, 3, 800)));
    }

    #[test]
    fn rejects_missing_segments() {
        assert_eq!(parse_tile_path("/42/3"), None);
        assert_eq!(parse_tile_path("/42"), None);
        assert_eq!(parse_tile_path("/"), None);
        assert_eq!(parse_tile_path(""), None);
    }

    #[test]
    fn rejects_non_numeric_segments() {
        assert_eq!(parse_tile_path("/abc/3/800"), None);
        assert_eq!(parse_tile_path("/42/abc/800"), None);
        assert_eq!(parse_tile_path("/42/3/abc"), None);
    }

    #[test]
    fn rejects_negative_numbers() {
        // DocHandle/page_index/target_width are all unsigned — a `-` must
        // fail to parse, not silently wrap.
        assert_eq!(parse_tile_path("/-1/3/800"), None);
        assert_eq!(parse_tile_path("/42/-3/800"), None);
    }

    #[test]
    fn ignores_extra_trailing_segments() {
        // Only the first three segments are consumed; a trailing slash or
        // extra path component (e.g. a cache-busting query-like suffix)
        // must not cause a parse failure.
        assert_eq!(parse_tile_path("/42/3/800/extra"), Some((42, 3, 800)));
    }

    // The sharing-violation-retry tests moved to `openpdfedit-session`
    // along with `is_sharing_violation`/`with_sharing_violation_retry`/
    // `sharing_violation_message`/`copy_with_lock_retry` themselves.

    #[test]
    fn tile_response_returns_bad_request_for_malformed_path() {
        let Some(engine) = test_support::shared_engine() else {
            return;
        };

        let request = Request::builder()
            .uri("tile://localhost/not-a-number")
            .body(Vec::new())
            .unwrap();
        let response = tile_response(engine, &request);
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get("Access-Control-Allow-Origin")
                .map(|v| v.to_str().unwrap()),
            Some("*"),
            "every tile_response branch must carry a CORS header, or the packaged app's \
             webview (origin tauri://localhost) silently discards the response — see \
             TILE_CORS_HEADER's doc for the real bug this caught"
        );
    }

    #[test]
    fn tile_response_returns_not_found_for_unknown_handle() {
        let Some(engine) = test_support::shared_engine() else {
            return;
        };

        let request = Request::builder()
            .uri("tile://localhost/999999/0/100")
            .body(Vec::new())
            .unwrap();
        let response = tile_response(engine, &request);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get("Access-Control-Allow-Origin")
                .map(|v| v.to_str().unwrap()),
            Some("*")
        );
    }

    /// The happy path this file's other two `tile_response` tests never
    /// covered: a real opened document, a real `tile://` request exactly
    /// as the frontend builds it (`PdfPage.svelte`/`PageThumb.svelte`),
    /// asserting not just a 200 but that the returned RGBA bytes contain
    /// actual rendered content — not just a same-size blank/white buffer,
    /// which a 200-with-empty-pixels regression would otherwise pass
    /// silently. This is the exact pipeline a user-reported "PDF shows
    /// white pages" bug report would need a test to have caught.
    #[test]
    fn tile_response_returns_real_non_blank_pixels_for_an_opened_document() {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let Some(engine) = test_support::shared_engine() else {
            return;
        };

        // A solid black filled rectangle covering most of the page — high
        // contrast, no font/glyph rendering ambiguity, unambiguous to
        // detect as "not blank."
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![
                Operation::new("re", vec![50.into(), 50.into(), 500.into(), 700.into()]),
                Operation::new("f", vec![]),
            ],
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
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-tile-happy-path-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, &bytes).expect("should write temp file");

        let handle = engine
            .open(&tmp_path)
            .expect("engine should open the temp file");

        let request = Request::builder()
            .uri(format!("tile://localhost/{handle}/0/300"))
            .body(Vec::new())
            .unwrap();
        let response = tile_response(engine, &request);
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a valid render request must succeed"
        );
        assert_eq!(
            response
                .headers()
                .get("Access-Control-Allow-Origin")
                .map(|v| v.to_str().unwrap()),
            Some("*"),
            "the packaged app's webview loads from a different origin (tauri://localhost) \
             than the tile:// scheme it fetches from; without this header the webview \
             discards an otherwise-successful response before JS ever sees it"
        );
        assert_eq!(
            response
                .headers()
                .get("Access-Control-Expose-Headers")
                .map(|v| v.to_str().unwrap()),
            Some("X-Tile-Width, X-Tile-Height"),
            "X-Tile-Width/X-Tile-Height aren't on the CORS 'simple headers' allowlist — \
             without exposing them explicitly, res.headers.get() silently returns null in \
             the webview (Number(null) === 0), which is exactly the '0x0 tile' bug this \
             caught live"
        );

        let width: usize = response
            .headers()
            .get("X-Tile-Width")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .expect("response must carry a tile width header");
        let height: usize = response
            .headers()
            .get("X-Tile-Height")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .expect("response must carry a tile height header");
        let body = response.body();
        assert_eq!(
            body.len(),
            width * height * 4,
            "body length must match width*height*4 RGBA bytes exactly"
        );

        let non_white_pixels = body
            .chunks_exact(4)
            .filter(|px| !(px[0] == 255 && px[1] == 255 && px[2] == 255))
            .count();
        assert!(
            non_white_pixels > 0,
            "the rendered tile must contain real (non-white) pixel content — \
             a page with a black filled rectangle came back entirely white, \
             which is exactly the 'PDF shows white pages' failure mode"
        );

        engine.close(handle);
        let _ = std::fs::remove_file(&tmp_path);
    }

    // The undo/redo round-trip tests (`undo_and_redo_round_trip_through_
    // real_edits`, `a_new_edit_after_undo_clears_the_redo_stack`) moved
    // to `openpdfedit-session` along with `undo_impl`/`redo_impl`/
    // `reopen_after_write`/`commit_undo_snapshot` themselves — see that
    // crate's test module for the (adapted, `commit_mutation`-free)
    // equivalents.
}
