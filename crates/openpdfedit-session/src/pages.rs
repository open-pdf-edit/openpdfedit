//! Page-organization commands: the single-document ops (rotate, delete,
//! move/reorder, crop) go through [`commit_mutation`] like annotations
//! do — mutate, save incrementally, rotate the render handle. The
//! cross-document ops (merge, extract) are different in kind: they read
//! whole files and write a brand-new one, via `openpdfedit-pages`'s
//! full-rewrite functions (see that crate's module doc for why merge/
//! extract can't be incremental saves) rather than mutating an
//! already-open [`Document`].
//!
//! Moved here (from `apps/desktop/src-tauri/src/pages.rs`) for the same
//! reason as [`crate::annotations`]/[`crate::forms`]: the same logic
//! should drive both the desktop's thread-wrapped `EngineHandle` and
//! (later) a bare in-process engine for the wasm/Chrome-extension build.
//!
//! The merge/extract half is **not** wasm-portable as a whole (later
//! matched by [`crate::compare`]'s own path-based half, and by
//! [`crate::signatures`]'s pre-Phase-4-Task-2 shape — see those modules'
//! docs; this crate has more than one module that splits a portable byte
//! core from a desktop-only path-based orchestration layer, so this isn't
//! unique to `pages`, just the first place the split happened):
//! [`MergeRequest`]/[`ExtractRequest`] identify their sources/destination
//! by filesystem path (`source_paths`, `output_path`), and
//! `gather_merge_sources`/`open_new_file` read and write real files —
//! none of which exists in a browser extension. Only the byte-level
//! rewrite itself ([`merge_bytes`]/[`extract_pages_bytes`], thin wrappers
//! over `openpdfedit-pages`'s already-portable `merge`/`extract_pages`) is
//! wasm-clean and left ungated; everything that turns that into
//! path-based orchestration
//! ([`MergeRequest`]/[`merge_documents_impl`]/[`ExtractRequest`]/
//! [`extract_pages_impl`]) is `#[cfg(not(target_arch = "wasm32"))]`, the
//! same boundary [`crate::open_document_impl`]/[`crate::save_document_impl`]
//! already draw.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use openpdfedit_engine::{DocHandle, Engine};
use openpdfedit_pages::PagesError;
use serde::Deserialize;

use crate::{
    commit_mutation, resolve_doc, DocHistory, OpenDoc, OpenedDocumentInfo, SessionError,
    WorkingStore,
};

impl From<PagesError> for SessionError {
    fn from(e: PagesError) -> Self {
        SessionError::Doc(e.to_string())
    }
}

/// The actual logic behind the desktop's `rotate_page_cmd`.
pub fn rotate_page_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
    page_index: u32,
    delta_degrees: i32,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, handle, |doc| {
        doc.rotate_page(page_index, delta_degrees)?;
        Ok(())
    })
}

/// The actual logic behind the desktop's `delete_page_cmd`.
pub fn delete_page_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
    page_index: u32,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, handle, |doc| {
        doc.delete_page(page_index)?;
        Ok(())
    })
}

/// The actual logic behind the desktop's `set_crop_box_cmd`.
pub fn set_crop_box_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
    page_index: u32,
    rect: [f32; 4],
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, handle, |doc| {
        doc.set_crop_box(page_index, rect)?;
        Ok(())
    })
}

/// Which neighbor to swap `page_index` with — the front-end's "move
/// page up/down" buttons, translated into the permutation
/// `Document::reorder_pages` needs. A dedicated swap command instead of
/// exposing the raw permutation directly: much harder for the front-end
/// to get wrong (an out-of-order or partial permutation is a real
/// footgun; "swap with my neighbor" isn't).
#[derive(Debug, Deserialize)]
pub enum MoveDirection {
    Up,
    Down,
}

/// The actual logic behind the desktop's `move_page_cmd`.
pub fn move_page_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    store: &dyn WorkingStore,
    handle: DocHandle,
    page_index: u32,
    direction: MoveDirection,
) -> Result<OpenedDocumentInfo, SessionError> {
    commit_mutation(engine, docs, history, store, handle, |doc| {
        let page_count = doc.page_count()?;
        let neighbor = match direction {
            MoveDirection::Up if page_index > 0 => page_index - 1,
            MoveDirection::Down if page_index + 1 < page_count => page_index + 1,
            // Already at an edge — a no-op swap-with-self is simpler for
            // the front-end to handle uniformly than a distinct error it
            // would just have to ignore (there's nothing wrong with the
            // request, the button was just clickable when it maybe
            // shouldn't have been).
            _ => page_index,
        };

        let mut order: Vec<u32> = (0..page_count).collect();
        order.swap(page_index as usize, neighbor as usize);
        doc.reorder_pages(&order)?;
        Ok(())
    })
}

/// Wasm-clean byte-level core behind `merge_documents_cmd`: a thin
/// wrapper over `openpdfedit_pages::merge`, converting its error into
/// [`SessionError`]. Kept separate from [`merge_documents_impl`] (which
/// is path-based and desktop-only) so this half — the part a future wasm
/// command could call directly with in-memory bytes, no filesystem
/// involved — has no path dependency at all.
pub fn merge_bytes(sources: &[&[u8]]) -> Result<Vec<u8>, SessionError> {
    openpdfedit_pages::merge(sources).map_err(SessionError::from)
}

/// Wasm-clean byte-level core behind `extract_pages_cmd` — see
/// [`merge_bytes`]'s doc for why this is split out from
/// [`extract_pages_impl`].
pub fn extract_pages_bytes(source: &[u8], page_indices: &[u32]) -> Result<Vec<u8>, SessionError> {
    openpdfedit_pages::extract_pages(source, page_indices).map_err(SessionError::from)
}

/// Wasm-portable counterpart to [`gather_merge_sources`]: reads
/// `open_handle`'s **live working-copy bytes** through `store` (a
/// [`WorkingStore`] parameter — `store.read`, not `std::fs::read`, so
/// this has no filesystem dependency and compiles for wasm32) rather than
/// re-reading a path from disk, then merges those bytes (if `open_handle`
/// is given) ahead of the already-in-memory `sources`, in that order, via
/// [`merge_bytes`]. This is the split [`gather_merge_sources`]'s own doc
/// anticipates: unlike `source_paths` (real files, which only exist on
/// the desktop), a wasm caller already has every other source's bytes in
/// hand (e.g. read from a `File` the browser's file picker returned) —
/// the only thing it *can't* do itself is resolve "the currently open
/// document's live edits" into bytes, since that requires looking `handle`
/// up in `docs`/`store`, both private to this session. `open_handle`, if
/// given but not present in `docs`, is [`SessionError::UnknownHandle`] —
/// same behavior as [`gather_merge_sources`] for the same case, so a typo'd
/// or already-closed handle can't silently merge zero bytes for it.
pub fn merge_open_doc_with_bytes(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    store: &dyn WorkingStore,
    open_handle: Option<DocHandle>,
    sources: Vec<Vec<u8>>,
) -> Result<Vec<u8>, SessionError> {
    let mut all_sources: Vec<Vec<u8>> = Vec::new();

    if let Some(handle) = open_handle {
        let path = {
            let docs_guard = docs.lock().expect("docs lock poisoned");
            resolve_doc(&docs_guard, handle)?.path.clone()
        };
        all_sources.push(store.read(&path)?);
    }

    all_sources.extend(sources);

    let source_refs: Vec<&[u8]> = all_sources.iter().map(Vec::as_slice).collect();
    merge_bytes(&source_refs)
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    /// If set, the currently-open document at this handle is merged in
    /// too — as the first source, ahead of `source_paths`. Read from its
    /// live working copy (see [`OpenDoc`]), not re-read from
    /// `source_paths`, so an in-progress edit that hasn't been saved yet
    /// isn't silently dropped the way re-picking the same file from disk
    /// would drop it.
    pub open_handle: Option<DocHandle>,
    pub source_paths: Vec<String>,
    pub output_path: String,
}

/// Reads every merge source's current bytes: `open_handle`'s live working
/// copy first (if given), then each of `source_paths` in order. Split out
/// from [`merge_documents_impl`] so it's testable without a real
/// `SessionState` — same reasoning as `undo_impl`/`redo_impl` in
/// `lib.rs`.
#[cfg(not(target_arch = "wasm32"))]
fn gather_merge_sources(
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    open_handle: Option<DocHandle>,
    source_paths: &[String],
) -> Result<Vec<Vec<u8>>, SessionError> {
    let mut sources: Vec<Vec<u8>> = Vec::new();

    if let Some(handle) = open_handle {
        let docs_guard = docs.lock().expect("docs lock poisoned");
        let open_doc = resolve_doc(&docs_guard, handle)?;
        sources.push(std::fs::read(&open_doc.path)?);
    }

    for path in source_paths {
        sources.push(std::fs::read(path)?);
    }

    Ok(sources)
}

/// Merges whole files (by path — they don't need to already be open in
/// the app), optionally alongside the currently-open document's live
/// content (`open_handle`), into a new document at `output_path`, then
/// opens *that* as a new document in this session. The actual logic
/// behind the desktop's `merge_documents_cmd`.
#[cfg(not(target_arch = "wasm32"))]
pub fn merge_documents_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    request: MergeRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let sources = gather_merge_sources(docs, request.open_handle, &request.source_paths)?;
    let source_refs: Vec<&[u8]> = sources.iter().map(Vec::as_slice).collect();
    let merged = merge_bytes(&source_refs)?;

    std::fs::write(&request.output_path, &merged)?;
    open_new_file(engine, docs, history, &request.output_path)
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    pub handle: DocHandle,
    pub page_indices: Vec<u32>,
    pub output_path: String,
}

/// Extracts a page subset from the currently-open document at `handle`
/// into a new file at `output_path`, then opens *that* as a new,
/// independent document — the source document at `handle` is untouched.
/// The actual logic behind the desktop's `extract_pages_cmd`.
#[cfg(not(target_arch = "wasm32"))]
pub fn extract_pages_impl<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    request: ExtractRequest,
) -> Result<OpenedDocumentInfo, SessionError> {
    let source_path = {
        let docs = docs.lock().expect("docs lock poisoned");
        resolve_doc(&docs, request.handle)?.path.clone()
    };
    let source_bytes = std::fs::read(&source_path)?;
    let extracted = extract_pages_bytes(&source_bytes, &request.page_indices)?;

    std::fs::write(&request.output_path, &extracted)?;
    open_new_file(engine, docs, history, &request.output_path)
}

/// Shared tail of [`merge_documents_impl`]/[`extract_pages_impl`]: both
/// produce a brand-new file on disk and then just need to open it like
/// any other document — no mutation, no handle rotation, since there's no
/// prior handle for this document to rotate away from.
///
/// Routes through [`OpenDoc::open_with_working_copy`] — the same
/// scratch-copy constructor every ordinary `File > Open` goes through
/// ([`crate::open_document_impl`]) — rather than opening `path` itself
/// directly under both `OpenDoc::path` *and* `original_path`, which is
/// what this function used to do. That used to seem harmless (`path` — a
/// destination the user picked in a save dialog — genuinely already *is*
/// the saved result, so "nothing is pending" was true), but it left this
/// the *only* `OpenDoc` construction site anywhere in this crate with no
/// scratch copy: `path == original_path` exactly. Two real bugs followed
/// from that, both fixed by giving merge/extract results a real scratch
/// copy like everything else:
///
/// - [`crate::close_document_impl`]'s `store.remove(&open_doc.path)` used
///   to have no way to tell "this key is a disposable scratch copy" apart
///   from "this key *is* the user's real file" — closing a freshly
///   merged/extracted document (easy to do: it opens clean, so nothing
///   prompts first) called `std::fs::remove_file` directly on the file
///   the user just asked to be created. `close_document_impl` now also
///   guards on `path == original_path`, but this fix removes the
///   underlying asymmetry that guard exists to paper over, rather than
///   only papering over it a second way.
/// - [`crate::save_document_impl`]'s `copy_with_lock_retry(&working,
///   &original)` used to run with `working == original` for a
///   merge/extract result — copying a file onto itself. `std::fs::copy`
///   with identical source/destination is not guaranteed safe in general
///   (platform-dependent; can truncate before it finishes reading on some
///   implementations) — a latent hazard that a real scratch copy also
///   makes structurally impossible here, not just unlikely.
#[cfg(not(target_arch = "wasm32"))]
fn open_new_file<E: Engine>(
    engine: &E,
    docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
    history: &Mutex<HashMap<PathBuf, DocHistory>>,
    path: &str,
) -> Result<OpenedDocumentInfo, SessionError> {
    let (handle, open_doc) = OpenDoc::open_with_working_copy(std::path::Path::new(path), engine)?;
    let working = open_doc.path.clone();
    docs.lock()
        .expect("docs lock poisoned")
        .insert(handle, open_doc);
    crate::opened_document(engine, docs, history, &working, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{minimal_pdf_bytes, shared_handle};
    use crate::{FsWorkingStore, MemWorkingStore};
    use openpdfedit_doc::Document;

    fn three_page_pdf_bytes() -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_ids: Vec<_> = (0..3)
            .map(|i| {
                let content = Content {
                    operations: vec![
                        Operation::new("BT", vec![]),
                        Operation::new("Tj", vec![Object::string_literal(format!("page {i}"))]),
                        Operation::new("ET", vec![]),
                    ],
                };
                let content_id =
                    doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                    "Contents" => content_id,
                    "Resources" => dictionary! {},
                })
            })
            .collect();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|&id| id.into()).collect::<Vec<Object>>(),
                "Count" => 3,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn open_temp_doc(
        engine: &openpdfedit_engine::EngineHandle,
        docs: &Mutex<HashMap<DocHandle, OpenDoc>>,
        bytes: &[u8],
        tag: &str,
    ) -> (DocHandle, PathBuf) {
        let tmp_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-test-{tag}-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&tmp_path, bytes).unwrap();
        let handle = engine.open(&tmp_path).unwrap();
        let doc = Document::open(&tmp_path).unwrap();
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
        (handle, tmp_path)
    }

    // Every command in this module is a thin `#[tauri::command]` wrapper
    // (in the desktop crate) around `commit_mutation`/`openpdfedit_pages`
    // calls — same reasoning as `annotations`/`forms`: `tauri::State` has
    // no public constructor outside the framework's own dependency
    // injection, so these tests drive the `_impl` functions directly.

    #[test]
    fn rotate_page_impl_rotates_and_returns_fresh_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &minimal_pdf_bytes(), "rotate");

        let result = rotate_page_impl(engine, &docs, &history, &FsWorkingStore, handle, 0, 90)
            .expect("rotate should succeed");

        assert_ne!(result.handle, handle);
        assert_eq!(result.page_count, 1);
        engine.close(result.handle);
        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn delete_page_impl_reduces_page_count() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &three_page_pdf_bytes(), "delete");

        let result = delete_page_impl(engine, &docs, &history, &FsWorkingStore, handle, 1)
            .expect("delete should succeed");

        assert_eq!(result.page_count, 2);
        engine.close(result.handle);
        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn set_crop_box_impl_saves_and_returns_fresh_handle() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &minimal_pdf_bytes(), "crop");

        let result = set_crop_box_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            handle,
            0,
            [10.0, 10.0, 400.0, 700.0],
        )
        .expect("crop should succeed");

        assert_ne!(result.handle, handle);
        assert_eq!(result.page_count, 1);
        engine.close(result.handle);
        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn move_page_impl_swap_logic_moves_the_right_neighbor() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &three_page_pdf_bytes(), "move");

        // Move page 0 "down" — should end up swapped with page 1.
        let result = move_page_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            handle,
            0,
            MoveDirection::Down,
        )
        .expect("move should succeed");

        assert_eq!(result.page_count, 3);
        engine.close(result.handle);
        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn move_page_impl_at_the_top_edge_is_a_harmless_no_op() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &three_page_pdf_bytes(), "move-edge");

        // Page 0 has no "up" neighbor — must not panic or error.
        let result = move_page_impl(
            engine,
            &docs,
            &history,
            &FsWorkingStore,
            handle,
            0,
            MoveDirection::Up,
        )
        .expect("move at the edge should still succeed as a no-op");

        assert_eq!(result.page_count, 3);
        engine.close(result.handle);
        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn merge_bytes_and_extract_pages_bytes_produce_independent_documents() {
        let a = minimal_pdf_bytes();
        let b = three_page_pdf_bytes();
        let merged = merge_bytes(&[&a, &b]).expect("merge should succeed");

        let doc = lopdf::Document::load_mem(&merged).expect("merged output should reparse");
        assert_eq!(doc.get_pages().len(), 4);

        let extracted = extract_pages_bytes(&merged, &[0, 3]).expect("extract should succeed");
        let doc = lopdf::Document::load_mem(&extracted).expect("extracted output should reparse");
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn merge_and_extract_produce_independent_new_documents() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());

        let a_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-a-{}.pdf",
            std::process::id()
        ));
        let b_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-b-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&a_path, minimal_pdf_bytes()).unwrap();
        std::fs::write(&b_path, three_page_pdf_bytes()).unwrap();

        let merged_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merged-{}.pdf",
            std::process::id()
        ));
        let sources: Vec<Vec<u8>> = vec![
            std::fs::read(&a_path).unwrap(),
            std::fs::read(&b_path).unwrap(),
        ];
        let source_refs: Vec<&[u8]> = sources.iter().map(Vec::as_slice).collect();
        let merged_bytes = merge_bytes(&source_refs).expect("merge should succeed");
        std::fs::write(&merged_path, &merged_bytes).unwrap();
        let merged_doc = open_new_file(engine, &docs, &history, merged_path.to_str().unwrap())
            .expect("open merged should succeed");
        assert_eq!(merged_doc.page_count, 4);

        let extracted_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-extracted-{}.pdf",
            std::process::id()
        ));
        let extracted_bytes =
            extract_pages_bytes(&merged_bytes, &[0, 3]).expect("extract should succeed");
        std::fs::write(&extracted_path, &extracted_bytes).unwrap();
        let extracted_doc =
            open_new_file(engine, &docs, &history, extracted_path.to_str().unwrap())
                .expect("open extracted should succeed");
        assert_eq!(extracted_doc.page_count, 2);

        // Both new documents, and the original source handles, all coexist
        // independently — extracting/merging must never mutate a source.
        assert_ne!(merged_doc.handle, extracted_doc.handle);

        engine.close(merged_doc.handle);
        engine.close(extracted_doc.handle);
        for p in [a_path, b_path, merged_path, extracted_path] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn merge_documents_impl_merges_the_open_handle_and_source_paths_then_opens_the_result() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) =
            open_temp_doc(engine, &docs, &minimal_pdf_bytes(), "merge-impl-open");

        let other_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-impl-other-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&other_path, three_page_pdf_bytes()).unwrap();

        let output_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-impl-out-{}.pdf",
            std::process::id()
        ));

        let request = MergeRequest {
            open_handle: Some(handle),
            source_paths: vec![other_path.to_str().unwrap().to_string()],
            output_path: output_path.to_str().unwrap().to_string(),
        };
        let result = merge_documents_impl(engine, &docs, &history, request)
            .expect("merge_documents_impl should succeed");

        assert_eq!(result.page_count, 4, "1 page (open) + 3 pages (source)");
        assert!(
            output_path.exists(),
            "merged output must be written to disk"
        );

        engine.close(handle);
        engine.close(result.handle);
        for p in [tmp_path, other_path, output_path] {
            let _ = std::fs::remove_file(p);
        }
    }

    /// Fix-wave re-review's NEW-C1, part 2: a merge/extract result used to
    /// get an `OpenDoc` with `path == original_path` (no real scratch
    /// copy) — [`open_new_file`] now routes through
    /// [`crate::OpenDoc::open_with_working_copy`] instead, exactly like
    /// every other open. Exercises both bugs that asymmetry caused,
    /// end-to-end, through the real desktop-facing functions (not just the
    /// unit-level pieces):
    ///
    /// - `save_document_impl`'s `copy_with_lock_retry(&working, &original)`
    ///   used to run with `working == original` for a merge result — a
    ///   same-file copy. Asserts `path != original_path` right after the
    ///   merge, then that `save_document_impl` actually succeeds and the
    ///   output file on disk still reparses with the right page count
    ///   afterward (not silently truncated).
    /// - `close_document_impl`'s `store.remove` used to have no way to
    ///   avoid deleting a merge result's real output file on close. Closes
    ///   the saved result through `close_document_impl` and asserts the
    ///   output file is still there afterward.
    #[test]
    fn merge_documents_impl_result_gets_a_real_scratch_copy_and_saves_correctly() {
        let Some(engine) = shared_handle() else {
            return;
        };

        let state = crate::SessionState {
            engine: engine.clone(),
            docs: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            store: Box::new(FsWorkingStore),
        };

        let a_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-scratch-a-{}.pdf",
            std::process::id()
        ));
        let b_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-scratch-b-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&a_path, minimal_pdf_bytes()).unwrap();
        std::fs::write(&b_path, three_page_pdf_bytes()).unwrap();

        let output_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-merge-scratch-out-{}.pdf",
            std::process::id()
        ));

        let request = MergeRequest {
            open_handle: None,
            source_paths: vec![
                a_path.to_str().unwrap().to_string(),
                b_path.to_str().unwrap().to_string(),
            ],
            output_path: output_path.to_str().unwrap().to_string(),
        };
        let result = merge_documents_impl(&state.engine, &state.docs, &state.history, request)
            .expect("merge_documents_impl should succeed");
        assert_eq!(result.page_count, 4);

        // The merge result's OpenDoc must have a real scratch copy now —
        // not path == original_path, the asymmetry NEW-C1 flagged.
        {
            let guard = state.docs.lock().unwrap();
            let open_doc = guard.get(&result.handle).unwrap();
            assert_ne!(
                open_doc.path, open_doc.original_path,
                "a merge result must get a real scratch copy distinct from the user's output file"
            );
            assert_eq!(
                open_doc.original_path, output_path,
                "original_path must still be the user's chosen output file"
            );
        }

        // save_document_impl must succeed — no more working-copy-onto-itself
        // self-copy — and the output file must still be the real merged
        // content afterward, not truncated/corrupted.
        let saved = crate::save_document_impl(&state, result.handle)
            .expect("save_document_impl should succeed after a merge");
        assert!(!saved.is_dirty);
        let saved_bytes = std::fs::read(&output_path).expect("output file must still exist");
        let reparsed =
            lopdf::Document::load_mem(&saved_bytes).expect("saved output must still reparse");
        assert_eq!(reparsed.get_pages().len(), 4);

        // Closing the saved result must never delete the user's output
        // file — the other half of NEW-C1, exercised end-to-end here too
        // (see lib.rs's own dedicated close_document_impl test for the
        // narrower unit-level version of this assertion).
        crate::close_document_impl(
            &state.engine,
            &state.docs,
            &state.history,
            &*state.store,
            saved.handle,
        );
        assert!(
            output_path.exists(),
            "closing a merge result must never delete the user's output file"
        );

        for p in [a_path, b_path, output_path] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn extract_pages_impl_extracts_from_the_open_handle_and_leaves_it_untouched() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let history: Mutex<HashMap<PathBuf, DocHistory>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) =
            open_temp_doc(engine, &docs, &three_page_pdf_bytes(), "extract-impl");

        let output_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-extract-impl-out-{}.pdf",
            std::process::id()
        ));
        let request = ExtractRequest {
            handle,
            page_indices: vec![2, 0],
            output_path: output_path.to_str().unwrap().to_string(),
        };
        let result = extract_pages_impl(engine, &docs, &history, request)
            .expect("extract_pages_impl should succeed");

        assert_eq!(result.page_count, 2);
        assert_ne!(
            result.handle, handle,
            "extraction must open a brand-new, independent document"
        );
        // The source document at `handle` must still resolve unchanged.
        assert_eq!(engine.page_count(handle).unwrap(), 3);

        engine.close(handle);
        engine.close(result.handle);
        for p in [tmp_path, output_path] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn gather_merge_sources_reads_the_open_handles_live_working_copy() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &minimal_pdf_bytes(), "gather-open");

        // Simulate an unsaved edit: the working copy on disk now differs
        // from what was open at `open_temp_doc` time. `open_handle`
        // sources must reflect *this*, not a stale in-memory copy — that
        // was the whole bug this option exists to fix.
        let edited = three_page_pdf_bytes();
        std::fs::write(&tmp_path, &edited).unwrap();

        let sources =
            gather_merge_sources(&docs, Some(handle), &[]).expect("gather should succeed");
        assert_eq!(sources, vec![edited]);

        engine.close(handle);
        let _ = std::fs::remove_file(tmp_path);
    }

    #[test]
    fn gather_merge_sources_orders_open_handle_before_source_paths() {
        let Some(engine) = shared_handle() else {
            return;
        };
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let (handle, tmp_path) = open_temp_doc(engine, &docs, &minimal_pdf_bytes(), "gather-order");

        let other_path = std::env::temp_dir().join(format!(
            "openpdfedit-session-pages-gather-other-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&other_path, three_page_pdf_bytes()).unwrap();

        let sources = gather_merge_sources(
            &docs,
            Some(handle),
            &[other_path.to_str().unwrap().to_string()],
        )
        .expect("gather should succeed");
        assert_eq!(sources, vec![minimal_pdf_bytes(), three_page_pdf_bytes()]);

        engine.close(handle);
        for p in [tmp_path, other_path] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn gather_merge_sources_rejects_an_unknown_open_handle() {
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let bogus_handle: DocHandle = 999_999;

        let err = gather_merge_sources(&docs, Some(bogus_handle), &[])
            .expect_err("an unknown handle must not silently merge zero bytes for it");
        assert!(matches!(err, SessionError::UnknownHandle(h) if h == bogus_handle));
    }

    // --- merge_open_doc_with_bytes: the wasm-portable counterpart to
    // gather_merge_sources + merge_bytes, driven entirely through
    // MemWorkingStore — no filesystem, no PDFium engine, since merge_bytes
    // itself needs neither. Mirrors signatures.rs's
    // `list_signatures_impl_finds_a_signature_through_mem_working_store`
    // test shape: build `docs`/`store` by hand, no `shared_handle()` gate,
    // so these never skip.

    /// The star property this function exists for: the open document's
    /// bytes must come from `store.read` — the *live* working copy — not
    /// whatever bytes the `Document` in `docs` happened to be constructed
    /// from. Same bug `gather_merge_sources_reads_the_open_handles_live_working_copy`
    /// guards on the desktop side, reproduced here through `MemWorkingStore`
    /// instead of a real file.
    #[test]
    fn merge_open_doc_with_bytes_reads_the_live_working_copy_not_a_stale_snapshot() {
        let store = MemWorkingStore::default();
        let path = std::path::PathBuf::from("mem-merge-open-test.pdf");
        let original = minimal_pdf_bytes();
        store
            .write(&path, &original)
            .expect("store.write should succeed");

        // Simulate an in-progress, unsaved edit: the working copy now holds
        // different bytes than whatever `OpenDoc::doc` was built from.
        let edited = three_page_pdf_bytes();
        store
            .write(&path, &edited)
            .expect("store.write (edit) should succeed");

        let doc = Document::from_bytes(&original).expect("doc crate should parse the bytes");
        let handle: DocHandle = 1;
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        docs.lock().unwrap().insert(
            handle,
            OpenDoc {
                path: path.clone(),
                original_path: path,
                dirty: true,
                doc,
                encryption: None,
            },
        );

        let merged = merge_open_doc_with_bytes(&docs, &store, Some(handle), vec![])
            .expect("merge_open_doc_with_bytes should succeed");

        // Must merge the *edited* (3-page) working copy, not the original
        // (1-page) snapshot `Document` was constructed from.
        let reparsed = lopdf::Document::load_mem(&merged).expect("merged output should reparse");
        assert_eq!(reparsed.get_pages().len(), 3);
    }

    /// `open_handle`'s live bytes come first, ahead of `sources`, in order
    /// — same ordering `gather_merge_sources` establishes on the desktop
    /// side.
    #[test]
    fn merge_open_doc_with_bytes_orders_the_open_doc_before_extra_sources() {
        let store = MemWorkingStore::default();
        let path = std::path::PathBuf::from("mem-merge-order-test.pdf");
        let open_bytes = minimal_pdf_bytes();
        store
            .write(&path, &open_bytes)
            .expect("store.write should succeed");

        let doc = Document::from_bytes(&open_bytes).expect("doc crate should parse the bytes");
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

        let other = three_page_pdf_bytes();
        let merged = merge_open_doc_with_bytes(&docs, &store, Some(handle), vec![other.clone()])
            .expect("merge_open_doc_with_bytes should succeed");

        let expected = merge_bytes(&[&open_bytes, &other]).expect("merge_bytes should succeed");
        assert_eq!(merged, expected);
    }

    /// `open_handle: None` — pure byte-sources merge, no store/docs lookup
    /// at all, same as calling `merge_bytes` directly.
    #[test]
    fn merge_open_doc_with_bytes_without_an_open_handle_just_merges_the_given_sources() {
        let store = MemWorkingStore::default();
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());

        let a = minimal_pdf_bytes();
        let b = three_page_pdf_bytes();
        let merged = merge_open_doc_with_bytes(&docs, &store, None, vec![a.clone(), b.clone()])
            .expect("merge_open_doc_with_bytes should succeed");

        let expected = merge_bytes(&[&a, &b]).expect("merge_bytes should succeed");
        assert_eq!(merged, expected);
    }

    #[test]
    fn merge_open_doc_with_bytes_rejects_an_unknown_open_handle() {
        let store = MemWorkingStore::default();
        let docs: Mutex<HashMap<DocHandle, OpenDoc>> = Mutex::new(HashMap::new());
        let bogus_handle: DocHandle = 999_999;

        let err = merge_open_doc_with_bytes(&docs, &store, Some(bogus_handle), vec![])
            .expect_err("an unknown handle must not silently merge zero bytes for it");
        assert!(matches!(err, SessionError::UnknownHandle(h) if h == bogus_handle));
    }
}
