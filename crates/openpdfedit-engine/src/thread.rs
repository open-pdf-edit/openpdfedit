//! Dedicated render thread + LRU tile cache, per PLAN.md §6 invariant 5:
//! "Renderer on its own thread with an LRU tile cache keyed by (page,
//! zoom, generation); generation bumps on edit."
//!
//! [`EngineHandle`] is the thing every other crate should hold — never a
//! bare [`PdfiumEngine`]. It owns the single process-wide `PdfiumEngine`
//! (see the single-instance rule on `PdfiumEngine::new`) on one background
//! thread and exposes a plain synchronous, `Clone`-able API over a channel,
//! so callers (Tauri commands today, the CLI batch runner later) never
//! need to know PDFium exists, let alone that it isn't thread-safe.
//!
//! `page_index` here does not yet carry a "generation" component — there
//! is no document mutation yet (that lands with `openpdfedit-doc`
//! integration). When edits arrive, bump a generation counter per
//! `DocHandle` and fold it into the cache key so a stale tile is never
//! served after an edit invalidates it.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use lru::LruCache;

use crate::{
    CharBox, DocHandle, Engine, EngineError, FormField, PageSize, PdfiumEngine, RenderedTile,
    SearchHit, SearchOptions,
};

/// Default tile cache capacity. 64 tiles at a typical 1000px-wide render
/// (~4MB each at RGBA8) is a few hundred MB ceiling — generous for
/// interactive scrolling without being an unbounded leak. Revisit once
/// real virtualized-scrolling usage data exists.
const DEFAULT_CACHE_CAPACITY: usize = 64;

type CacheKey = (DocHandle, u32, u32); // (handle, page_index, target_width)

enum Request {
    Open {
        path: PathBuf,
        reply: mpsc::Sender<Result<DocHandle, EngineError>>,
    },
    Close {
        handle: DocHandle,
    },
    PageCount {
        handle: DocHandle,
        reply: mpsc::Sender<Result<u32, EngineError>>,
    },
    RenderPage {
        handle: DocHandle,
        page_index: u32,
        target_width: u32,
        reply: mpsc::Sender<Result<Arc<RenderedTile>, EngineError>>,
    },
    CharBoxes {
        handle: DocHandle,
        page_index: u32,
        reply: mpsc::Sender<Result<Vec<CharBox>, EngineError>>,
    },
    PageSizes {
        handle: DocHandle,
        reply: mpsc::Sender<Result<Vec<PageSize>, EngineError>>,
    },
    SearchDocument {
        handle: DocHandle,
        query: String,
        options: SearchOptions,
        max_hits: usize,
        reply: mpsc::Sender<Result<Vec<SearchHit>, EngineError>>,
    },
    ListFormFields {
        handle: DocHandle,
        reply: mpsc::Sender<Result<Vec<FormField>, EngineError>>,
    },
    FillFormFields {
        handle: DocHandle,
        values: HashMap<String, String>,
        reply: mpsc::Sender<Result<(), EngineError>>,
    },
    SaveDocument {
        handle: DocHandle,
        path: PathBuf,
        reply: mpsc::Sender<Result<(), EngineError>>,
    },
    OpenBytes {
        bytes: Vec<u8>,
        reply: mpsc::Sender<Result<DocHandle, EngineError>>,
    },
    SaveToBytes {
        handle: DocHandle,
        reply: mpsc::Sender<Result<Vec<u8>, EngineError>>,
    },
}

/// A cheap, `Clone`-able, `Send + Sync` handle to the render thread. All
/// methods block the calling thread until the render thread replies —
/// callers that need concurrency (e.g. a Tauri app serving several tile
/// requests at once) should call from multiple threads; the render thread
/// itself processes one request at a time by design (that's the whole
/// point: PDFium only ever sees one caller).
#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<Request>,
}

impl EngineHandle {
    /// Spawns the render thread and blocks until PDFium has finished
    /// loading on it (or failed to), so construction failures surface
    /// synchronously here instead of silently killing a background thread.
    pub fn spawn(lib_dir: Option<PathBuf>) -> Result<Self, EngineError> {
        let (tx, rx) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), EngineError>>();

        thread::Builder::new()
            .name("openpdfedit-render".into())
            .spawn(move || {
                let engine = match lib_dir {
                    Some(dir) => PdfiumEngine::new(Some(&dir)),
                    None => PdfiumEngine::new_dev(),
                };
                let engine = match engine {
                    Ok(engine) => {
                        let _ = ready_tx.send(Ok(()));
                        engine
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                run_render_loop(engine, rx);
            })
            .expect("failed to spawn render thread");

        ready_rx
            .recv()
            .expect("render thread dropped without reporting readiness")?;

        Ok(Self { tx })
    }

    fn request_reply<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, EngineError>>) -> Request,
    ) -> Result<T, EngineError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(build(reply_tx))
            .expect("render thread receiver dropped — it must have panicked");
        reply_rx
            .recv()
            .expect("render thread dropped the reply sender without responding")
    }

    pub fn open(&self, path: &Path) -> Result<DocHandle, EngineError> {
        let path = path.to_path_buf();
        self.request_reply(|reply| Request::Open { path, reply })
    }

    pub fn close(&self, handle: DocHandle) {
        // No reply needed; the render thread purges the cache and drops
        // the document on its own time. If the thread is already gone
        // this is a no-op close, which is fine.
        let _ = self.tx.send(Request::Close { handle });
    }

    pub fn page_count(&self, handle: DocHandle) -> Result<u32, EngineError> {
        self.request_reply(|reply| Request::PageCount { handle, reply })
    }

    /// Renders (or returns the cached render of) one page tile. The
    /// returned `Arc` is cheap to clone and share with, e.g., an async
    /// HTTP response body without copying pixels.
    pub fn render_page(
        &self,
        handle: DocHandle,
        page_index: u32,
        target_width: u32,
    ) -> Result<Arc<RenderedTile>, EngineError> {
        self.request_reply(|reply| Request::RenderPage {
            handle,
            page_index,
            target_width,
            reply,
        })
    }

    pub fn page_char_boxes(
        &self,
        handle: DocHandle,
        page_index: u32,
    ) -> Result<Vec<CharBox>, EngineError> {
        self.request_reply(|reply| Request::CharBoxes {
            handle,
            page_index,
            reply,
        })
    }

    pub fn page_sizes(&self, handle: DocHandle) -> Result<Vec<PageSize>, EngineError> {
        self.request_reply(|reply| Request::PageSizes { handle, reply })
    }

    /// Searches the whole open document — see [`Engine::search_document`]
    /// for why this is one bounded call rather than a per-page API, and
    /// what `max_hits` is protecting.
    pub fn search_document(
        &self,
        handle: DocHandle,
        query: &str,
        options: SearchOptions,
        max_hits: usize,
    ) -> Result<Vec<SearchHit>, EngineError> {
        let query = query.to_string();
        self.request_reply(|reply| Request::SearchDocument {
            handle,
            query,
            options,
            max_hits,
            reply,
        })
    }

    pub fn list_form_fields(&self, handle: DocHandle) -> Result<Vec<FormField>, EngineError> {
        self.request_reply(|reply| Request::ListFormFields { handle, reply })
    }

    /// See [`PdfiumEngine::fill_form_fields`] for exactly which field
    /// types are supported and how a `Checkbox`/`RadioButton` `value` is
    /// interpreted. Does not save — call [`EngineHandle::save_document`]
    /// afterwards.
    pub fn fill_form_fields(
        &self,
        handle: DocHandle,
        values: HashMap<String, String>,
    ) -> Result<(), EngineError> {
        self.request_reply(|reply| Request::FillFormFields {
            handle,
            values,
            reply,
        })
    }

    pub fn save_document(&self, handle: DocHandle, path: &Path) -> Result<(), EngineError> {
        let path = path.to_path_buf();
        self.request_reply(|reply| Request::SaveDocument {
            handle,
            path,
            reply,
        })
    }

    pub fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError> {
        self.request_reply(|reply| Request::OpenBytes { bytes, reply })
    }

    pub fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError> {
        self.request_reply(|reply| Request::SaveToBytes { handle, reply })
    }
}

/// Forwards each [`Engine`] method to `EngineHandle`'s inherent method of
/// the same name, so session logic that needs to be generic over either
/// engine (desktop's thread-wrapped `EngineHandle` today, a direct
/// in-process engine for the wasm/extension build later) can hold a
/// `&dyn Engine` / `E: Engine` and use either one identically.
impl Engine for EngineHandle {
    fn open(&self, path: &Path) -> Result<DocHandle, EngineError> {
        EngineHandle::open(self, path)
    }

    fn close(&self, handle: DocHandle) {
        EngineHandle::close(self, handle)
    }

    fn page_count(&self, handle: DocHandle) -> Result<u32, EngineError> {
        EngineHandle::page_count(self, handle)
    }

    fn search_document(
        &self,
        handle: DocHandle,
        query: &str,
        options: SearchOptions,
        max_hits: usize,
    ) -> Result<Vec<SearchHit>, EngineError> {
        EngineHandle::search_document(self, handle, query, options, max_hits)
    }

    fn render_page(
        &self,
        handle: DocHandle,
        page_index: u32,
        target_width: u32,
    ) -> Result<RenderedTile, EngineError> {
        // The inherent method returns `Arc<RenderedTile>` (cheap to share
        // across cache hits); the trait's object-safe signature returns an
        // owned `RenderedTile`. The `.clone()` fallback is the *expected*
        // path here, not a rare corner case: the render loop always keeps
        // its own `Arc` reference to the tile in the LRU cache — it clones
        // the `Arc` into the cache before replying on a miss (see
        // `Request::RenderPage`'s handling in `run_render_loop`) and
        // clones the cached `Arc` to build the reply on a hit — so this
        // call almost never holds the only reference, and `try_unwrap`
        // only succeeds in the narrow window where the cache has already
        // evicted the entry between the render thread's reply and this
        // call running (or the cache capacity is 0, which never happens in
        // practice). The one extra pixel-buffer copy this costs is
        // accepted: this trait path exists for engine-generic session
        // logic (wasm/extension build, which has no cache to share an Arc
        // with anyway), not for the desktop's hot path — the desktop
        // keeps calling `EngineHandle::render_page` directly and keeps the
        // Arc-sharing optimization untouched.
        EngineHandle::render_page(self, handle, page_index, target_width)
            .map(|tile| Arc::try_unwrap(tile).unwrap_or_else(|arc| (*arc).clone()))
    }

    fn page_char_boxes(
        &self,
        handle: DocHandle,
        page_index: u32,
    ) -> Result<Vec<CharBox>, EngineError> {
        EngineHandle::page_char_boxes(self, handle, page_index)
    }

    fn page_sizes(&self, handle: DocHandle) -> Result<Vec<PageSize>, EngineError> {
        EngineHandle::page_sizes(self, handle)
    }

    fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError> {
        EngineHandle::open_bytes(self, bytes)
    }

    fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError> {
        EngineHandle::save_to_bytes(self, handle)
    }

    fn list_form_fields(&self, handle: DocHandle) -> Result<Vec<FormField>, EngineError> {
        EngineHandle::list_form_fields(self, handle)
    }

    fn fill_form_fields(
        &self,
        handle: DocHandle,
        values: HashMap<String, String>,
    ) -> Result<(), EngineError> {
        EngineHandle::fill_form_fields(self, handle, values)
    }
}

// Deliberately no `Drop` impl. An earlier version sent an explicit
// `Request::Shutdown` on drop, which was wrong: `EngineHandle` is `Clone`
// (every Tauri command handler, and every worker in the concurrency test
// below, holds its own clone), and the render loop broke out on the
// *first* shutdown message it saw — so dropping even one clone killed the
// shared render thread out from under every other clone still in use.
// `mpsc::Sender` already does the right thing on its own: `rx.recv()`
// only returns `Err` once every `Sender` clone (i.e. every `EngineHandle`
// clone) has been dropped, at which point `run_render_loop`'s `while let
// Ok(request) = rx.recv()` exits naturally. No custom `Drop` needed, and
// getting it wrong is exactly the kind of bug that only shows up under
// concurrent use — see `concurrent_render_requests_from_multiple_threads_do_not_crash_or_deadlock`.

fn run_render_loop(engine: PdfiumEngine, rx: mpsc::Receiver<Request>) {
    let mut cache: LruCache<CacheKey, Arc<RenderedTile>> = LruCache::new(
        NonZeroUsize::new(DEFAULT_CACHE_CAPACITY).expect("capacity is a nonzero constant"),
    );

    while let Ok(request) = rx.recv() {
        match request {
            Request::Open { path, reply } => {
                let _ = reply.send(engine.open(&path));
            }
            Request::Close { handle } => {
                engine.close(handle);
                cache
                    .iter()
                    .map(|(k, _)| *k)
                    .filter(|(h, _, _)| *h == handle)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .for_each(|key| {
                        cache.pop(&key);
                    });
            }
            Request::PageCount { handle, reply } => {
                let _ = reply.send(engine.page_count(handle));
            }
            Request::RenderPage {
                handle,
                page_index,
                target_width,
                reply,
            } => {
                let key = (handle, page_index, target_width);
                let result = if let Some(cached) = cache.get(&key) {
                    Ok(Arc::clone(cached))
                } else {
                    engine
                        .render_page(handle, page_index, target_width)
                        .map(|tile| {
                            let tile = Arc::new(tile);
                            cache.put(key, Arc::clone(&tile));
                            tile
                        })
                };
                let _ = reply.send(result);
            }
            Request::CharBoxes {
                handle,
                page_index,
                reply,
            } => {
                let _ = reply.send(engine.page_char_boxes(handle, page_index));
            }
            Request::PageSizes { handle, reply } => {
                let _ = reply.send(engine.page_sizes(handle));
            }
            Request::SearchDocument {
                handle,
                query,
                options,
                max_hits,
                reply,
            } => {
                let _ = reply.send(engine.search_document(handle, &query, options, max_hits));
            }
            Request::ListFormFields { handle, reply } => {
                let _ = reply.send(engine.list_form_fields(handle));
            }
            Request::FillFormFields {
                handle,
                values,
                reply,
            } => {
                let _ = reply.send(engine.fill_form_fields(handle, &values));
            }
            Request::SaveDocument {
                handle,
                path,
                reply,
            } => {
                let _ = reply.send(engine.save_document(handle, &path));
            }
            Request::OpenBytes { bytes, reply } => {
                let _ = reply.send(engine.open_bytes(bytes));
            }
            Request::SaveToBytes { handle, reply } => {
                let _ = reply.send(engine.save_to_bytes(handle));
            }
        }
    }
    // Loop exits here once every `EngineHandle` clone (i.e. every `tx`
    // sender) has been dropped and `rx.recv()` returns `Err` — see the
    // note above `run_render_loop` for why that's the only correct
    // shutdown signal for a `Clone`-able handle.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    // Same reasoning as the PdfiumEngine tests: PDFium's global init isn't
    // safe to run concurrently across independent bindings, so every test
    // in this file shares one EngineHandle (which itself owns exactly one
    // PdfiumEngine on its dedicated thread).
    fn shared_handle() -> Option<&'static EngineHandle> {
        static HANDLE: OnceLock<Option<EngineHandle>> = OnceLock::new();
        HANDLE
            .get_or_init(
                || match EngineHandle::spawn(dev_vendor_lib_dir_for_tests()) {
                    Ok(h) => Some(h),
                    Err(e) => {
                        eprintln!("skipping render-thread tests: PDFium not available ({e})");
                        None
                    }
                },
            )
            .as_ref()
    }

    fn dev_vendor_lib_dir_for_tests() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent()?.parent()?;
        let dir = workspace_root.join(if cfg!(windows) {
            ".vendor/pdfium/bin"
        } else {
            ".vendor/pdfium/lib"
        });
        dir.exists().then_some(dir)
    }

    fn test_corpus_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/minimal.pdf")
    }

    /// A corpus document that actually carries a text layer — `minimal.pdf`
    /// has an empty content stream, so it can't exercise text search.
    fn text_corpus_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/corpus/hello_world_rotated.pdf")
    }

    /// End-to-end through real PDFium: the pure matcher is unit-tested in
    /// `lib.rs`, but only this proves the character sequence PDFium hands
    /// back is index-aligned with the boxes a hit gets mapped onto — the
    /// one assumption in the whole search path that can't be checked
    /// without a render engine.
    #[test]
    fn search_finds_real_page_text_and_maps_it_to_page_geometry() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = text_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }
        let doc = handle.open(&corpus).expect("open should succeed");

        // The fixture draws the same string at several rotations, so
        // several hits is the correct answer — the point of asserting on
        // all of them is that a rotated run maps to geometry just as a
        // horizontal one does.
        let hits = handle
            .search_document(doc, "hello", SearchOptions::default(), 100)
            .expect("search should succeed");
        assert!(
            !hits.is_empty(),
            "the fixture's text layer should be searchable"
        );

        let page_count = handle.page_count(doc).expect("page count should succeed");
        for hit in &hits {
            assert!(hit.page_index < page_count);
            assert_eq!(
                hit.context_match.to_lowercase(),
                "hello",
                "the reported match text must be what was actually matched"
            );
            assert!(
                !hit.quads.is_empty(),
                "a hit on rendered text must carry at least one quad to highlight"
            );
            for quad in &hit.quads {
                let [x0, y0, x1, y1] = *quad;
                assert!(x1 > x0 && y1 > y0, "quad {quad:?} is not a real rectangle");
            }
        }

        // Hits arrive in document order — page first, then position
        // within the page — which is what lets the UI step through them
        // with next/previous.
        assert!(hits
            .windows(2)
            .all(|w| (w[0].page_index, w[0].char_start) < (w[1].page_index, w[1].char_start)));

        // Case sensitivity is applied against the document's real casing.
        let cased = SearchOptions {
            match_case: true,
            whole_word: false,
        };
        assert!(handle
            .search_document(doc, "HELLO", cased, 100)
            .expect("search should succeed")
            .is_empty());

        handle.close(doc);
    }

    #[test]
    fn search_respects_the_hit_cap_and_tolerates_a_document_without_text() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = text_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }
        let doc = handle.open(&corpus).expect("open should succeed");
        // "l" occurs several times in "Hello world"; the cap must bound it.
        let capped = handle
            .search_document(doc, "l", SearchOptions::default(), 2)
            .expect("search should succeed");
        assert_eq!(capped.len(), 2);
        handle.close(doc);

        let blank = test_corpus_path();
        if blank.exists() {
            let doc = handle.open(&blank).expect("open should succeed");
            assert!(handle
                .search_document(doc, "anything", SearchOptions::default(), 100)
                .expect("a text-free document should search cleanly, not error")
                .is_empty());
            handle.close(doc);
        }
    }

    #[test]
    fn opens_and_renders_through_the_render_thread() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let count = handle.page_count(doc).expect("page count should succeed");
        assert!(count >= 1);

        let tile = handle
            .render_page(doc, 0, 150)
            .expect("render should succeed");
        assert_eq!(tile.width, 150);
        assert!(tile.height > 0);
        assert_eq!(tile.rgba.len(), (tile.width * tile.height * 4) as usize);

        handle.close(doc);
    }

    #[test]
    fn char_boxes_on_blank_page_returns_empty_without_error() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let boxes = handle
            .page_char_boxes(doc, 0)
            .expect("char boxes should succeed on a blank page");
        assert!(boxes.is_empty());

        handle.close(doc);
    }

    #[test]
    fn unknown_handle_returns_error_not_panic() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let result = handle.page_count(999_999);
        assert!(matches!(result, Err(EngineError::UnknownHandle(999_999))));
    }

    #[test]
    fn page_sizes_reports_one_entry_per_page_with_positive_dimensions() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let count = handle.page_count(doc).expect("page count should succeed");
        let sizes = handle.page_sizes(doc).expect("page sizes should succeed");

        assert_eq!(sizes.len(), count as usize);
        for size in &sizes {
            assert!(size.width > 0.0);
            assert!(size.height > 0.0);
        }

        handle.close(doc);
    }

    #[test]
    fn repeated_render_hits_the_cache_and_returns_identical_pixels() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let first = handle
            .render_page(doc, 0, 150)
            .expect("render should succeed");
        let second = handle
            .render_page(doc, 0, 150)
            .expect("render should succeed");

        // A cache hit returns the *same* allocation (Arc::ptr_eq), not
        // just equal bytes — proves the cache path actually ran instead
        // of silently re-rendering every time.
        assert!(Arc::ptr_eq(&first, &second));

        handle.close(doc);
    }

    #[test]
    fn close_purges_cached_tiles_for_that_handle() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let _ = handle
            .render_page(doc, 0, 150)
            .expect("render should succeed");
        handle.close(doc);

        // The handle is gone now, so even though the cache key might
        // collide with a *future* handle number (it won't, handles are
        // monotonic, but this documents the intent), a render call
        // against the closed handle must fail cleanly rather than
        // returning a stale tile.
        let result = handle.render_page(doc, 0, 150);
        assert!(result.is_err());
    }

    #[test]
    fn render_page_out_of_range_returns_error_not_panic() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let count = handle.page_count(doc).expect("page count should succeed");
        let result = handle.render_page(doc, count + 1000, 80);
        assert!(matches!(result, Err(EngineError::PageOutOfRange { .. })));

        handle.close(doc);
    }

    #[test]
    fn char_boxes_out_of_range_returns_error_not_panic() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let count = handle.page_count(doc).expect("page count should succeed");
        let result = handle.page_char_boxes(doc, count + 1000);
        assert!(matches!(result, Err(EngineError::PageOutOfRange { .. })));

        handle.close(doc);
    }

    #[test]
    fn concurrent_render_requests_from_multiple_threads_do_not_crash_or_deadlock() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");
        let count = handle.page_count(doc).expect("page count should succeed");

        // The render thread itself serializes requests by design (one
        // PDFium, one thread) — the property under test is that firing
        // requests concurrently from many *caller* threads is safe: no
        // deadlock, no crash, every request eventually gets a correct
        // reply on its own channel.
        let workers: Vec<_> = (0..16)
            .map(|i| {
                let handle = handle.clone();
                let page_index = i % count;
                thread::spawn(move || handle.render_page(doc, page_index, 60))
            })
            .collect();

        for worker in workers {
            let tile = worker
                .join()
                .expect("worker thread should not panic")
                .expect("concurrent render should succeed");
            assert!(tile.height > 0);
        }

        handle.close(doc);
    }

    #[test]
    fn cache_evicts_oldest_entry_once_over_capacity() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let corpus = test_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }

        let doc = handle.open(&corpus).expect("open should succeed");

        // A unique base width (not used by any other test in this file)
        // keeps this test's cache keys from colliding with tiles other
        // tests may have already warmed into the shared cache.
        let base_width = 900u32;
        let original = handle
            .render_page(doc, 0, base_width)
            .expect("render should succeed");

        // Push strictly more distinct (page, width) keys through than
        // DEFAULT_CACHE_CAPACITY holds, which must evict `base_width`'s
        // entry (LRU: least-recently-used, and nothing here re-touches it).
        for offset in 1..=(DEFAULT_CACHE_CAPACITY as u32 + 8) {
            handle
                .render_page(doc, 0, base_width + offset)
                .expect("render should succeed");
        }

        let refetched = handle
            .render_page(doc, 0, base_width)
            .expect("render should succeed");

        // If the cache never evicted, this would be the same Arc
        // (Arc::ptr_eq true) as `original`. It must not be — the whole
        // point of a *bounded* cache is that it doesn't grow forever.
        assert!(
            !Arc::ptr_eq(&original, &refetched),
            "expected the original tile to have been evicted and re-rendered as a new allocation"
        );

        handle.close(doc);
    }

    #[test]
    fn open_bytes_then_page_count() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let bytes = std::fs::read(test_corpus_path()).expect("read fixture");

        let doc = handle.open_bytes(bytes).expect("open_bytes");
        assert_eq!(handle.page_count(doc).expect("page_count"), 1);
        handle.close(doc);
    }

    #[test]
    fn engine_handle_works_through_the_engine_trait() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let bytes = std::fs::read(test_corpus_path()).expect("read fixture");
        // The whole point: session logic will hold `&dyn Engine` /
        // `E: Engine`, so EngineHandle must be usable through the trait.
        let engine: &dyn crate::Engine = handle;
        let doc = engine.open_bytes(bytes).expect("open via trait");
        assert_eq!(engine.page_count(doc).expect("page_count"), 1);

        // `render_page` is the one trait method whose inherent counterpart
        // has a different return type (`Arc<RenderedTile>` vs owned
        // `RenderedTile`) — exercise the unwrap/clone conversion through
        // the trait object and check the tile it hands back is a real,
        // consistent render, not just that the conversion compiles.
        let tile = engine.render_page(doc, 0, 150).expect("render via trait");
        assert_eq!(tile.width, 150);
        assert!(tile.height > 0);
        assert_eq!(tile.rgba.len(), (tile.width * tile.height * 4) as usize);

        engine.close(doc);
    }

    /// A minimal one-page AcroForm PDF with a single Text field, hand-built
    /// via `lopdf` the same way `openpdfedit-session`'s `forms.rs` builds
    /// its (richer) fixture — copied down to this crate rather than
    /// depending on that crate (which itself depends on this one, so a
    /// dependency the other direction isn't an option).
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

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
            "Annots" => vec![text_field_id.into()],
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
            "Fields" => vec![text_field_id.into()],
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

    /// Phase 3 Task 1: `list_form_fields`/`fill_form_fields` must be
    /// reachable through `&dyn Engine`, not just as `EngineHandle`
    /// inherent methods — session logic generic over `E: Engine` (the
    /// wasm/extension build's whole point) needs both on the trait.
    #[test]
    fn list_and_fill_form_fields_work_through_the_engine_trait() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let engine: &dyn crate::Engine = handle;
        let doc = engine
            .open_bytes(acroform_pdf_bytes())
            .expect("open via trait");

        let before = engine
            .list_form_fields(doc)
            .expect("list_form_fields via trait");
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].name, "full_name");
        assert_eq!(before[0].value.as_deref(), None);

        let mut values = HashMap::new();
        values.insert("full_name".to_string(), "Ada Lovelace".to_string());
        engine
            .fill_form_fields(doc, values)
            .expect("fill_form_fields via trait");

        let after = engine
            .list_form_fields(doc)
            .expect("list_form_fields via trait after fill");
        assert_eq!(after[0].value.as_deref(), Some("Ada Lovelace"));

        engine.close(doc);
    }

    #[test]
    fn save_to_bytes_round_trips() {
        let Some(handle) = shared_handle() else {
            return;
        };
        let original = std::fs::read(test_corpus_path()).expect("read fixture");

        let doc = handle.open_bytes(original).expect("open_bytes");
        let saved = handle.save_to_bytes(doc).expect("save_to_bytes");
        handle.close(doc);

        let reopened = handle.open_bytes(saved).expect("reopen saved bytes");
        assert_eq!(handle.page_count(reopened).expect("page_count"), 1);
        handle.close(reopened);
    }
}
