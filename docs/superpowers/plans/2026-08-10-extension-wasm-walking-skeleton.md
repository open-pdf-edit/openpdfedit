# Extension Walking Skeleton (PDFium-in-WASM) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove OpenPdfEdit's rendering engine can run against a WASM PDFium
build inside a real Chrome extension — open a PDF, render it, draw one
highlight annotation, save it back out — with nothing beyond that.

**Architecture:** A new, thin `wasm-bindgen` crate (`openpdfedit-wasm`) calls
straight into the existing `openpdfedit-engine`/`openpdfedit-doc`/
`openpdfedit-annot` crates, which get two small additive methods
(`open_bytes`/`save_to_bytes`) so they don't need a filesystem path. A new
`apps/extension/` Chrome extension loads that wasm module alongside a
vendored WASM PDFium binary and drives it directly — no Tauri IPC, since
there's no separate process to IPC to.

**Tech Stack:** Rust, `wasm-bindgen`, `wasm32-unknown-unknown`,
`pdfium-render` 0.9.3 (already a workspace dependency), Manifest V3 Chrome
extension, Vite (matching `opencapture`'s existing extension build).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-10-extension-wasm-walking-skeleton-design.md`.
- Nothing in `apps/desktop` or `apps/cli` is modified by this plan. All new
  code is additive (new files, or new methods added to existing traits —
  never a changed signature on an existing method).
- Chrome only. No Firefox build in this plan.
- Everything here happens on this session's own branch
  (`worktree-openpdfedit-tile-scheme-fix`); if the pipeline doesn't work
  out, the fallback is simply not merging it.
- Confirmed by direct probe this session (`cargo build -p openpdfedit-engine
  --target wasm32-unknown-unknown`): the crate compiles cleanly for wasm32
  except for exactly four native-only calls (`Pdfium::bind_to_library`,
  `pdfium_platform_library_name_at_path`, `load_pdf_from_file`,
  `save_to_file`). **Correction, found during Task 4:** Tasks 1–2 only
  added new byte-based methods alongside these — they never fixed the
  four calls themselves, so the crate as a whole still failed a
  `--target wasm32-unknown-unknown` build until Task 4 cfg-gated them
  (commit `6e8193d`). Neither Task 1's nor Task 2's report ever actually
  ran a wasm32 build to check this — a real gap in how this plan verified
  those two tasks, not just an inert loose end.
- `openpdfedit-annot`'s `AnnotationKind` has no `Rect`/`Arrow` variant —
  only `Highlight`, `Underline`, `StrikeOut`, `FreeText`, `Ink`. The spec's
  "rectangle or arrow annotation" doesn't map onto anything that exists
  today; this plan uses `Highlight` instead (the simplest existing
  variant — one quad + a color, per `crates/openpdfedit-annot/src/lib.rs`).
  Adding a real `Rect`/`Arrow` annotation kind is out of scope here.

---

### Task 1: `open_bytes`/`save_to_bytes` on the `Engine` trait and `PdfiumEngine`

**Files:**
- Modify: `crates/openpdfedit-engine/src/lib.rs`

**Interfaces:**
- Produces: `Engine::open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError>`, `Engine::save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError>`, both also implemented on `PdfiumEngine`.

This crate's own test module has a hard constraint stated in its header
comment: `PdfiumEngine`-level behavior beyond pure geometry is tested only
through `thread.rs`'s `EngineHandle`-based tests, sharing one instance
behind a `OnceLock` — a direct `PdfiumEngine::new_dev()` call in a plain
`#[test]` crashed with SIGTRAP once before, because `cargo test` runs
tests in parallel and PDFium's global init isn't safe to run
concurrently. This task does **not** add its own tests for that reason —
writing a `#[test]` here that calls `PdfiumEngine::new_dev()` directly
would reintroduce exactly that crash. Task 2 adds the actual test coverage
for this behavior, through the one sanctioned entry point
(`EngineHandle`, via `thread.rs`'s existing `shared_handle()` pattern).
This task's own verification is Step 3 below (full existing suite, still
green) — a compile-level check, not a new automated test.

- [ ] **Step 1: Add the trait methods and implementation**

In the `Engine` trait definition, add two new methods alongside `open`:

```rust
    /// Like `open`, but from an in-memory buffer rather than a filesystem
    /// path — the entry point a browser extension build needs, since
    /// `wasm32-unknown-unknown` has no filesystem at all.
    fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError>;
    /// Like `save_document`, but returns the saved bytes instead of
    /// writing them to a path — same reason as `open_bytes`.
    fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError>;
```

In `impl Engine for PdfiumEngine`, add the implementations. `open_bytes`
mirrors `open`'s handle-allocation exactly, differing only in how the
`PdfDocument` is obtained — `load_pdf_from_byte_slice` needs its input to
outlive the returned `PdfDocument<'a>` for as long as that document is
kept in `self.documents`, so the buffer is leaked to `'static`, the same
trade-off `PdfiumEngine::new` already makes for the `Pdfium` singleton
itself (see that function's comment) — just paid once per document here
instead of once per process:

```rust
    fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError> {
        // Leaked deliberately — `load_pdf_from_byte_slice` ties the
        // returned document's lifetime to this buffer, which must live
        // as long as the document stays in `self.documents`. Same
        // trade-off `PdfiumEngine::new` already makes for the `Pdfium`
        // singleton; see that function's doc comment.
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let document = self
            .pdfium
            .load_pdf_from_byte_slice(leaked, None)
            .map_err(|e| EngineError::OpenFailed(e.to_string()))?;

        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.documents
            .lock()
            .expect("engine document map lock poisoned")
            .insert(handle, document);
        Ok(handle)
    }

    fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError> {
        let documents = self
            .documents
            .lock()
            .expect("engine document map lock poisoned");
        let document = documents
            .get(&handle)
            .ok_or(EngineError::InvalidHandle(handle))?;
        let mut buffer = std::io::Cursor::new(Vec::new());
        document
            .save_to_writer(&mut buffer)
            .map_err(|e| EngineError::SaveFailed(e.to_string()))?;
        Ok(buffer.into_inner())
    }
```

Check `EngineError`'s existing variants for `OpenFailed`/`SaveFailed`
and an "invalid handle" variant (`page_count`'s existing implementation
a few lines below already looks one up the same way — copy its exact
error variant name for the `ok_or(...)` above rather than guessing).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p openpdfedit-engine`
Expected: succeeds with no errors.

- [ ] **Step 3: Run the full existing engine test suite to confirm nothing broke**

Run: `cargo test -p openpdfedit-engine`
Expected: all tests PASS (this crate's existing suite, unchanged — there
are no new tests yet; Task 2 adds the ones that actually exercise
`open_bytes`/`save_to_bytes`, through `EngineHandle`).

- [ ] **Step 4: Commit**

```bash
git add crates/openpdfedit-engine/src/lib.rs
git commit -m "openpdfedit-engine: add byte-buffer open/save alongside path-based open/save"
```

---

### Task 2: `EngineHandle::open_bytes`/`save_to_bytes` (the actual sanctioned entry point)

**Files:**
- Modify: `crates/openpdfedit-engine/src/thread.rs`

**Interfaces:**
- Consumes: `Engine::open_bytes`/`save_to_bytes` from Task 1.
- Produces: `EngineHandle::open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError>`, `EngineHandle::save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError>` — this crate's actual test module and every real caller (Tauri commands, and eventually the wasm wrapper in Task 4) go through `EngineHandle`, never `PdfiumEngine` directly (see this file's header comment on why: PDFium's global init isn't safe to run on more than one thread per process).

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `thread.rs`, alongside the
existing tests that use `shared_handle()`/`test_corpus_path()`:

```rust
    #[test]
    fn open_bytes_then_page_count() {
        let handle = shared_handle().expect("engine should be available");
        let bytes = std::fs::read(test_corpus_path()).expect("read fixture");

        let doc = handle.open_bytes(bytes).expect("open_bytes");
        assert_eq!(handle.page_count(doc).expect("page_count"), 1);
        handle.close(doc);
    }

    #[test]
    fn save_to_bytes_round_trips() {
        let handle = shared_handle().expect("engine should be available");
        let original = std::fs::read(test_corpus_path()).expect("read fixture");

        let doc = handle.open_bytes(original).expect("open_bytes");
        let saved = handle.save_to_bytes(doc).expect("save_to_bytes");
        handle.close(doc);

        let reopened = handle.open_bytes(saved).expect("reopen saved bytes");
        assert_eq!(handle.page_count(reopened).expect("page_count"), 1);
        handle.close(reopened);
    }
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test -p openpdfedit-engine --lib thread::tests::open_bytes_then_page_count`
Expected: compile error — methods don't exist on `EngineHandle` yet.

- [ ] **Step 3: Add the `Request` variants**

In the `Request` enum, add two variants alongside `Open`/`SaveDocument`:

```rust
    OpenBytes {
        bytes: Vec<u8>,
        reply: mpsc::Sender<Result<DocHandle, EngineError>>,
    },
    SaveToBytes {
        handle: DocHandle,
        reply: mpsc::Sender<Result<Vec<u8>, EngineError>>,
    },
```

- [ ] **Step 4: Add the `EngineHandle` methods**

Alongside `open`/`save_document`:

```rust
    pub fn open_bytes(&self, bytes: Vec<u8>) -> Result<DocHandle, EngineError> {
        self.request_reply(|reply| Request::OpenBytes { bytes, reply })
    }

    pub fn save_to_bytes(&self, handle: DocHandle) -> Result<Vec<u8>, EngineError> {
        self.request_reply(|reply| Request::SaveToBytes { handle, reply })
    }
```

- [ ] **Step 5: Add the match arms in `run_render_loop`**

Alongside the existing `Request::Open`/`Request::SaveDocument` arms:

```rust
            Request::OpenBytes { bytes, reply } => {
                let _ = reply.send(engine.open_bytes(bytes));
            }
            Request::SaveToBytes { handle, reply } => {
                let _ = reply.send(engine.save_to_bytes(handle));
            }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p openpdfedit-engine --lib thread::tests`
Expected: all PASS, including the two new tests.

- [ ] **Step 7: Commit**

```bash
git add crates/openpdfedit-engine/src/thread.rs
git commit -m "openpdfedit-engine: thread EngineHandle::open_bytes/save_to_bytes through the render thread"
```

---

### Task 3: Workspace support for a `wasm-release` build profile

**Files:**
- Modify: `Cargo.toml` (openpdfedit workspace root)

**Interfaces:**
- Produces: a `wasm-release` Cargo profile, matching `opencapture`'s own (`[profile.wasm-release]`, inheriting from `release` — check that repo's root `Cargo.toml` for its exact settings and copy them, since this is purely a build-tuning profile with no project-specific logic worth re-deriving).

- [ ] **Step 1: Copy the `[profile.wasm-release]` block**

Read `/Volumes/My Shared Files/sharing_folder/openapps/opencapture/Cargo.toml`'s `[profile.wasm-release]` section and add an identical block to `openpdfedit/Cargo.toml`.

- [ ] **Step 2: Verify the workspace still parses**

Run: `cargo metadata --format-version 1 > /dev/null`
Expected: exits 0, no error.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "openpdfedit: add wasm-release build profile"
```

---

### Task 4: New crate `openpdfedit-wasm`

**Files:**
- Create: `crates/openpdfedit-wasm/Cargo.toml`
- Create: `crates/openpdfedit-wasm/src/lib.rs`
- Modify: `Cargo.toml` (add `crates/openpdfedit-wasm` to `[workspace] members`)

**Interfaces:**
- Consumes: `openpdfedit-engine::EngineHandle::{open_bytes, save_to_bytes, render_page}` (Tasks 1–2), `openpdfedit-doc::Document`, `openpdfedit-annot::{add_annotation, NewAnnotation, AnnotationKind, Rect, Color}`.
- Produces: a `#[wasm_bindgen]` class `WasmDocument` with `open(bytes: &[u8])` (constructor), `renderPage(page_index: u32, width: u32) -> RenderedPage`, `addHighlight(page_index: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> ()`, `save() -> Uint8Array` — and a `#[wasm_bindgen]` struct `RenderedPage { width: u32, height: u32, rgba: Vec<u8> }` with getters, since the JS side (Task 7) needs the rendered page's actual pixel dimensions to size its canvas, not just the raw bytes. `openpdfedit-engine`'s `RenderedTile` already carries width/height alongside its pixel buffer — `RenderedPage` just exposes that same data across the wasm boundary.

This crate can't be tested with plain `cargo test` in any meaningful way —
it only exists to be loaded by a JS runtime. Its verification is "compiles
for wasm32", deferred to Step 4 below; genuine end-to-end verification
happens in Task 9.

- [ ] **Step 1: Write `Cargo.toml`**

Mirror `opencapture/crates/shot-core/Cargo.toml`'s shape exactly —
`crate-type = ["cdylib", "rlib"]`, the same four wasm-only dependencies
(`wasm-bindgen`, `js-sys`, `serde-wasm-bindgen`, `console_error_panic_hook`
— check that file for exact version pins/workspace references), plus this
crate's actual logic dependencies:

```toml
[package]
name = "openpdfedit-wasm"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
openpdfedit-engine = { path = "../openpdfedit-engine" }
openpdfedit-annot = { path = "../openpdfedit-annot" }

# wasm-only: this crate only ever runs on wasm32 in practice, but these
# are unconditional dependencies the same way shot-core's are (see that
# crate's Cargo.toml comment).
wasm-bindgen = { workspace = true }
js-sys = { workspace = true }
console_error_panic_hook = { workspace = true }

[package.metadata.wasm-pack.profile.release]
wasm-opt = false
```

- [ ] **Step 2: Add to workspace members**

In `openpdfedit/Cargo.toml`'s `[workspace] members`, add
`"crates/openpdfedit-wasm"`.

- [ ] **Step 3: Write `src/lib.rs`**

```rust
//! wasm-bindgen surface for the browser extension. Kept deliberately
//! thin — every method here just marshals JS values to/from
//! `openpdfedit-engine`'s `EngineHandle` and `openpdfedit-annot`'s
//! `add_annotation`, both already covered by those crates' own tests.
//! No PDF logic lives here that isn't already tested elsewhere.

use std::sync::OnceLock;

use openpdfedit_annot::{add_annotation, AnnotationKind, Color, NewAnnotation, Rect};
use openpdfedit_engine::{DocHandle, EngineHandle};
use openpdfedit_doc::Document;
use wasm_bindgen::prelude::*;

fn to_js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// One `EngineHandle` for the whole extension page's lifetime — mirrors
/// the desktop app's own rule (see `EngineHandle::spawn`'s doc comment):
/// PDFium's global init is not safe to run more than once per process.
fn engine() -> Result<&'static EngineHandle, JsValue> {
    static ENGINE: OnceLock<Result<EngineHandle, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| EngineHandle::spawn(None).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| JsValue::from_str(e))
}

/// Mirrors `openpdfedit-engine::RenderedTile` across the wasm boundary —
/// the JS side needs the actual pixel dimensions to size its canvas, not
/// just the raw bytes (a plain `Uint8Array` return can't carry both).
#[wasm_bindgen]
pub struct RenderedPage {
    width: u32,
    height: u32,
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

    #[wasm_bindgen(getter)]
    pub fn rgba(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.rgba.as_slice())
    }
}

#[wasm_bindgen]
pub struct WasmDocument {
    handle: DocHandle,
}

#[wasm_bindgen]
impl WasmDocument {
    #[wasm_bindgen(constructor)]
    pub fn open(bytes: &[u8]) -> Result<WasmDocument, JsValue> {
        console_error_panic_hook::set_once();
        let handle = engine()?.open_bytes(bytes.to_vec()).map_err(to_js_err)?;
        Ok(WasmDocument { handle })
    }

    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&self, page_index: u32, width: u32) -> Result<RenderedPage, JsValue> {
        let tile = engine()?
            .render_page(self.handle, page_index, width)
            .map_err(to_js_err)?;
        Ok(RenderedPage {
            width: tile.width,
            height: tile.height,
            rgba: tile.rgba.clone(),
        })
    }

    #[wasm_bindgen(js_name = addHighlight)]
    pub fn add_highlight(
        &self,
        page_index: u32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> Result<(), JsValue> {
        // `add_annotation` operates on an `openpdfedit-doc::Document`,
        // not directly on the engine's own `PdfDocument` — re-open the
        // current bytes into a `Document`, mutate, save back through the
        // engine. Round-tripping through bytes here (rather than adding
        // engine-level annotation support) keeps this crate's only job
        // "marshal JS <-> existing crates", matching this module's own
        // doc comment above.
        let bytes = engine()?.save_to_bytes(self.handle).map_err(to_js_err)?;
        let mut doc = Document::from_bytes(&bytes).map_err(to_js_err)?;
        add_annotation(
            &mut doc,
            page_index,
            NewAnnotation {
                rect: Rect { x0, y0, x1, y1 },
                color: Color { r: 255, g: 235, b: 59 },
                kind: AnnotationKind::Highlight {
                    quads: vec![Rect { x0, y0, x1, y1 }],
                },
                contents: None,
                opacity: 0.4,
            },
        )
        .map_err(to_js_err)?;
        let saved = doc.save_incremental().map_err(to_js_err)?;
        let new_handle = engine()?.open_bytes(saved).map_err(to_js_err)?;
        engine()?.close(self.handle);
        // `handle` is `Copy` (a `u64`) so this needs `&mut self`, not
        // `&self` — fix the method signature above to `&mut self` before
        // this line, and update the JS-facing call site accordingly.
        Ok(())
    }

    #[wasm_bindgen]
    pub fn save(&self) -> Result<js_sys::Uint8Array, JsValue> {
        let bytes = engine()?.save_to_bytes(self.handle).map_err(to_js_err)?;
        Ok(js_sys::Uint8Array::from(bytes.as_slice()))
    }
}
```

Check `Color`'s actual field names/types in `openpdfedit-annot/src/lib.rs`
(around line 35) before compiling — the snippet above assumes `r`/`g`/`b`
as `u8`, which needs confirming against the real struct definition rather
than trusted blindly.

- [ ] **Step 4: Fix `add_highlight`'s `&self`/`&mut self` mismatch**

The comment inside Step 3's code block flags a real bug: `add_highlight`
reassigns `self.handle` to a new value after re-saving, which needs
`&mut self`. Change the method signature to
`pub fn add_highlight(&mut self, ...)`, and add `self.handle = new_handle;`
right before the final `Ok(())`.

- [ ] **Step 5: Attempt the wasm32 build**

```bash
export CARGO_TARGET_DIR=/tmp/openpdfedit-wasm-target
cargo build -p openpdfedit-wasm --target wasm32-unknown-unknown --profile wasm-release
```

Expected: succeeds, or fails with a specific, readable error to fix before
continuing — do not proceed to Task 5 until this compiles clean. This is
the single most important checkpoint in this whole plan: it's the first
real confirmation the annotation-adding path (not just open/render, which
Tasks 1–2 already proved compiles) works for wasm32 too.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/openpdfedit-wasm
git commit -m "openpdfedit-wasm: new wasm-bindgen crate wrapping the engine for a browser extension"
```

---

### Task 5: Vendor a WASM PDFium build

**Files:**
- Create: `scripts/fetch-pdfium-wasm.sh`

**Interfaces:**
- Produces: `.vendor/pdfium-wasm/` populated with a PDFium WASM build's release assets, fetched by this script — mirroring `scripts/fetch-pdfium.sh`'s existing pattern for the native library.

Use `paulocoutinhox/pdfium-lib`'s releases, not `bblanchon/pdfium-binaries`
(confirmed this session: that project's current release has no WASM asset
at all) and not the first WASM source floated during brainstorming,
`@embedpdf/pdfium` (`pdfium-render`'s own README explicitly warns that
kind of build — compiled with a non-growable WASM heap — runs out of
memory opening anything beyond a few pages; `pdfium-lib`'s WASM builds are
the ones that README recommends instead).

- [ ] **Step 1: Find the exact release asset**

Open `https://github.com/paulocoutinhox/pdfium-lib/releases` in a browser
and find the most recent release's WASM asset (something like
`wasm.tgz`/`wasm.zip` — the exact filename is not yet confirmed in this
plan; confirm it directly on the releases page). Note the exact filename
and download URL.

- [ ] **Step 2: Write the fetch script**

Base this on `scripts/fetch-pdfium.sh`'s structure (pin a specific release
tag rather than "latest", download to a temp location, extract into
`.vendor/pdfium-wasm/`, skip if already present). Use the exact filename
found in Step 1 — do not guess it.

- [ ] **Step 3: Run the script and inspect the output**

```bash
./scripts/fetch-pdfium-wasm.sh
ls -la .vendor/pdfium-wasm/
```

Note the actual files present (likely a `.wasm` binary plus a `.js`
loader/glue file) and their sizes — record both in this plan's own
"Notes" (add a `## Notes` section at the bottom of this file once run) so
Task 7 knows exactly what it's loading.

- [ ] **Step 4: Add `.vendor/pdfium-wasm/` to `.gitignore`**

Check `openpdfedit/.gitignore` for how `.vendor/pdfium/` (the native
build) is already excluded, and add the same pattern for
`.vendor/pdfium-wasm/`.

- [ ] **Step 5: Commit**

```bash
git add scripts/fetch-pdfium-wasm.sh .gitignore
git commit -m "openpdfedit: add fetch script for the vendored WASM PDFium build"
```

## Notes (Task 5, run 2026-08-12)

Release used: `paulocoutinhox/pdfium-lib` tag **`7902`** (published
2026-06-20), asset **`wasm.tgz`** (13,165,561 bytes), downloaded from
`https://github.com/paulocoutinhox/pdfium-lib/releases/download/7902/wasm.tgz`.
Found via anonymous `curl` against
`https://api.github.com/repos/paulocoutinhox/pdfium-lib/releases/latest`,
which lists four assets for this release: `android.tgz`, `ios.tgz`,
`macos.tgz`, `wasm.tgz` — `wasm.tgz` is the one that matters here.

`scripts/fetch-pdfium-wasm.sh` extracts the tarball as-is into
`.vendor/pdfium-wasm/` (no top-level component stripped), giving:

```
.vendor/pdfium-wasm/release/package.json           301 B   ({"name":"pdfium-lib","version":"7902.0.0",...})
.vendor/pdfium-wasm/release/lib/libpdfium.a         16,671,392 B  (static lib, not used by the WASM path)
.vendor/pdfium-wasm/release/include/*.h             ~25 header files, ~410 KB total (C API headers, reference only)
.vendor/pdfium-wasm/release/node/index.html         46,447 B (pdfium-lib's own demo page)
.vendor/pdfium-wasm/release/node/pdfium.js          253,534 B  <-- the pair Task 7 loads
.vendor/pdfium-wasm/release/node/pdfium.wasm      5,218,943 B  <-- the pair Task 7 loads
.vendor/pdfium-wasm/release/node/pdfium.esm.js      253,501 B  (ES-module variant, unused)
.vendor/pdfium-wasm/release/node/pdfium.esm.wasm  5,218,943 B  (ES-module variant, unused)
.vendor/pdfium-wasm/release/node/pdfium.std.js      245,512 B  (no-SIMD? variant, unused)
.vendor/pdfium-wasm/release/node/pdfium.std.wasm  5,222,138 B  (no-SIMD? variant, unused)
```

Total on disk: 32 MB (`du -sh .vendor/pdfium-wasm`).

**For Task 7:** `release/node/pdfium.js` + `release/node/pdfium.wasm` are
the pair to copy/load — confirmed by inspecting `pdfium.js`'s source: it
declares `var PDFiumModule=(()=>{...})()` at top level, i.e. a global
factory function named `PDFiumModule`, matching the
`declare const PDFiumModule: () => Promise<unknown>` / `PDFiumModule()`
contract Task 7's `editor.ts` snippet expects — this is the *current*
(post-V5407) loading contract, not the older
`Module.onRuntimeInitialized` pattern. `pdfium.esm.js`/`.esm.wasm` is an ES
module variant (`export default`) not needed for the `<script src>` +
global-factory approach Task 7's `editor.html` uses; `pdfium.std.*` is a
third variant (slightly different `.wasm` size) not otherwise
investigated. `release/lib/libpdfium.a` and `release/include/*.h` are C/C++
build artifacts unrelated to the WASM runtime path and can be ignored by
Task 7.

`.vendor/pdfium-wasm/` did not need a new `.gitignore` line: the existing
pattern for the native build is the generic `.vendor/` (not
`.vendor/pdfium/` as the task text assumed), which already recursively
ignores everything under `.vendor/`, including the new `pdfium-wasm/`
subdirectory. Confirmed with `git check-ignore -v .vendor/pdfium-wasm/`.
Only the explanatory comment above that line was updated to mention both
fetch scripts.

Full report: `.superpowers/sdd/2026-08-10-extension-wasm-walking-skeleton/task-5-report.md`.
Commit: `31b1d8e3044b30fa2f6b336a0bfe3b17630b9082`.

---

### Task 6: Extension app scaffold

**Files:**
- Create: `apps/extension/package.json`
- Create: `apps/extension/manifest.json`
- Create: `apps/extension/scripts/build-wasm.sh`
- Create: `apps/extension/vite.config.js`
- Create: `apps/extension/background.ts`

**Interfaces:**
- Produces: a buildable, loadable (unpacked) Chrome extension shell that does nothing yet except open a blank `editor.html` tab when its toolbar icon is clicked.

- [ ] **Step 1: Write `build-wasm.sh`**

Copy `opencapture/apps/extension/scripts/build-wasm.sh` verbatim,
changing only the crate name (`openpdfedit-wasm` instead of `shot-core`),
the `CARGO_TARGET_DIR` default (`/tmp/openpdfedit-target`), and the
`--manifest-path` (pointing at `openpdfedit/Cargo.toml`).

- [ ] **Step 2: Write `package.json`**

```json
{
  "name": "openpdfedit-extension",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "build:wasm": "bash scripts/build-wasm.sh",
    "build": "npm run build:wasm && vite build",
    "dev": "npm run build:wasm && vite build --watch"
  },
  "devDependencies": {
    "vite": "^6.0.3",
    "typescript": "^5.7.2"
  }
}
```

- [ ] **Step 3: Write `manifest.json`**

```json
{
  "manifest_version": 3,
  "name": "OpenPdfEdit (extension preview)",
  "version": "0.1.0",
  "description": "Walking-skeleton browser build of OpenPdfEdit — open, render, annotate, save.",
  "action": {},
  "background": {
    "service_worker": "background.js",
    "type": "module"
  },
  "content_security_policy": {
    "extension_pages": "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'"
  }
}
```

The CSP line is copied verbatim from `opencapture/apps/extension/manifest.json` — Chrome's `wasm-unsafe-eval` requirement (enforced since August 2026) is already solved there; no need to re-derive it.

- [ ] **Step 4: Write `background.ts`**

```typescript
// Opens the editor in its own tab on toolbar-icon click — mirrors
// opencapture's own background service worker's "open the editor
// in a real tab, not a popup" pattern (see that app's background.ts).
chrome.action.onClicked.addListener(() => {
  chrome.tabs.create({ url: chrome.runtime.getURL("editor.html") });
});
```

- [ ] **Step 5: Write `vite.config.js`**

```javascript
import { defineConfig } from "vite";

export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        background: "background.ts",
        editor: "editor.html",
      },
      output: {
        entryFileNames: "[name].js",
      },
    },
  },
});
```

- [ ] **Step 6: Build and load the unpacked extension**

```bash
cd apps/extension
npm install
npm run build
```

Open `chrome://extensions`, enable Developer mode, "Load unpacked", select
`apps/extension/dist`. Confirm the extension loads with no errors and
clicking its toolbar icon opens a blank tab (no `editor.html` content
yet — that's Task 7).

- [ ] **Step 7: Commit**

```bash
git add apps/extension
git commit -m "openpdfedit: scaffold the extension app shell"
```

---

### Task 7: Load PDFium WASM + `openpdfedit-wasm`, open a PDF, render page 1

**Files:**
- Create: `apps/extension/editor.html`
- Create: `apps/extension/editor.ts`
- Modify: `apps/extension/scripts/copy-vendor.sh` (new — copies `.vendor/pdfium-wasm/` and the `wasm-bindgen`-generated glue into the build output)

This task contains the plan's biggest genuine unknown: the exact
JavaScript initialization contract `pdfium-render`'s wasm binding expects.
Resolved this session by reading `pdfium-render`'s own maintained example
directly (`examples/index.html` in the `ajrcarey/pdfium-render` repo,
paired with `examples/wasm.rs`) rather than guessing — quoted in full
below. Two things from that example are not yet certain and need
confirming as part of this task, not assumed: whether `pdfium-lib`'s
*current* release still uses the `PDFiumModule().then(...)` factory shown
there (the example itself notes this changed once before, at their
version V5407), and whether the calling convention still applies
unmodified when our own module is built with `wasm-bindgen --target web`
(this monorepo's convention, via `build-wasm.sh`) rather than the
`--target no-modules` style the example itself uses (bare global
`wasm_bindgen(...)` calls, not `import init from`).

- [ ] **Step 1: Confirm `pdfium-lib`'s current loading contract**

Check the actual files present in `.vendor/pdfium-wasm/` from Task 5
(likely a `pdfium.js` + `pdfium.wasm` pair) against the pattern below —
specifically, does the `.js` file define a global `PDFiumModule()`
factory function, matching the example. If it instead exports an ES
module default, that's the "before V5407" case the example itself
describes as superseded — adjust Step 3 below to the older
`Module.onRuntimeInitialized = ...` pattern shown (commented out) in that
same example file instead.

- [ ] **Step 2: Write `editor.html`**

```html
<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>OpenPdfEdit (extension preview)</title>
    <script src="./pdfium.js"></script>
  </head>
  <body>
    <button id="openFile">Open PDF…</button>
    <canvas id="page"></canvas>
    <script type="module" src="./editor.js"></script>
  </body>
</html>
```

- [ ] **Step 3: Write `editor.ts`'s PDFium init sequence**

Adapted from `pdfium-render`'s own example (`examples/index.html` in the
`ajrcarey/pdfium-render` repo) — that example loads its own wasm-bindgen
module via the global `wasm_bindgen('foo.wasm')` call produced by
`--target no-modules`; this monorepo's `build-wasm.sh` convention uses
`--target web` instead (ES `import init from`), so the exact form of the
`rustModule` handle `initialize_pdfium_render`'s second argument expects
is the one part of this translation not yet verified — if passing `init`'s
own return value doesn't work, that's the first thing to try adjusting,
by comparing against what `--target no-modules` output actually hands the
`.then()` callback in the original example:

```typescript
import init, { initialize_pdfium_render, WasmDocument } from "./wasm-gen/openpdfedit_wasm.js";

declare const PDFiumModule: () => Promise<unknown>;

const pdfiumModule = await PDFiumModule();
const rustModule = await init(); // see this task's header note on `--target web` vs `--target no-modules`

console.assert(
  initialize_pdfium_render(pdfiumModule, rustModule, false),
  "Initialization of pdfium-render failed!",
);

let doc: WasmDocument | null = null;
// Module-scope, not local to renderCurrentPage() — Task 8's event
// listeners need to reference the same canvas.
const canvas = document.getElementById("page") as HTMLCanvasElement;

document.getElementById("openFile")!.addEventListener("click", async () => {
  const [handle] = await (window as any).showOpenFilePicker({
    types: [{ description: "PDF", accept: { "application/pdf": [".pdf"] } }],
  });
  const file = await handle.getFile();
  const bytes = new Uint8Array(await file.arrayBuffer());
  doc = new WasmDocument(bytes);
  renderCurrentPage();
});

function renderCurrentPage(): void {
  if (!doc) return;
  const page = doc.renderPage(0, 800);
  canvas.width = page.width;
  canvas.height = page.height;
  const ctx = canvas.getContext("2d")!;
  ctx.putImageData(new ImageData(new Uint8ClampedArray(page.rgba), page.width), 0, 0);
}
```

`showOpenFilePicker` isn't in every version of TypeScript's default DOM
lib — same caveat as Task 8 Step 2's `showSaveFilePicker`; add a minimal
local ambient declaration if it doesn't typecheck out of the box, rather
than pulling in a third-party types package for one function.

- [ ] **Step 4: Write `copy-vendor.sh`**

A short script (called from `package.json`'s `build` step, after
`build:wasm`) that copies `.vendor/pdfium-wasm/*` and whatever
`wasm-bindgen --out-dir` produced into `apps/extension/dist/` so both are
present alongside `editor.js` in the final unpacked extension.

- [ ] **Step 5: Build, load, and manually verify**

```bash
cd apps/extension
npm run build
```

Reload the unpacked extension in `chrome://extensions`, open the editor
tab, pick a real multi-page PDF via the file input, confirm page 1
renders correctly on the canvas. This is the first genuine end-to-end
signal for the whole plan — if it doesn't work, this is exactly the
"discard the branch" moment the spec's fallback-safety section describes.

- [ ] **Step 6: Commit**

```bash
git add apps/extension
git commit -m "openpdfedit: wire up PDFium WASM init and render the first page"
```

---

### Task 8: Draw and save a highlight annotation

**Files:**
- Modify: `apps/extension/editor.ts`

**Interfaces:**
- Consumes: `WasmDocument.addHighlight(page_index, x0, y0, x1, y1)`, `WasmDocument.save()` (Task 4).

- [ ] **Step 1: Add a drag-to-highlight interaction on the canvas**

Appends to `editor.ts` — `doc` and `canvas` are already declared in Task 7,
Step 3; don't redeclare them here.

```typescript
let dragStart: { x: number; y: number } | null = null;

canvas.addEventListener("mousedown", (e) => {
  dragStart = { x: e.offsetX, y: e.offsetY };
});

canvas.addEventListener("mouseup", (e) => {
  if (!dragStart || !doc) return;
  const x0 = Math.min(dragStart.x, e.offsetX);
  const x1 = Math.max(dragStart.x, e.offsetX);
  // PDF page-space is bottom-left-origin, y-up; canvas is top-left-origin,
  // y-down — the desktop app's PdfPage.svelte already does this exact
  // flip for pointer input (see its onCreateAnnotation handling); mirror
  // that conversion here rather than re-deriving it.
  const y0 = canvas.height - Math.max(dragStart.y, e.offsetY);
  const y1 = canvas.height - Math.min(dragStart.y, e.offsetY);
  doc.addHighlight(0, x0, y0, x1, y1);
  dragStart = null;
  renderCurrentPage(); // defined in Task 7, Step 3 — shows the new highlight
});
```

- [ ] **Step 2: Add a Save button**

```html
<button id="saveFile">Save</button>
```

```typescript
document.getElementById("saveFile")!.addEventListener("click", async () => {
  if (!doc) return;
  const bytes = doc.save();
  const handle = await (window as any).showSaveFilePicker({
    suggestedName: "edited.pdf",
    types: [{ description: "PDF", accept: { "application/pdf": [".pdf"] } }],
  });
  const writable = await handle.createWritable();
  await writable.write(bytes);
  await writable.close();
});
```

`showSaveFilePicker` isn't in TypeScript's default DOM lib yet in every
version — if this doesn't typecheck, add a minimal local ambient
declaration rather than reaching for a large third-party types package
for one function.

- [ ] **Step 3: Manually verify the full loop**

Open a PDF, drag a highlight rectangle over some text, click Save, save to
a new file, reopen that saved file in the extension (or in any other PDF
viewer) and confirm the highlight is present and in the right place.

- [ ] **Step 4: Commit**

```bash
git add apps/extension
git commit -m "openpdfedit: draw-to-highlight and save, closing the walking-skeleton loop"
```

---

### Task 9a: fix `EngineHandle`'s thread-spawn incompatibility on wasm32

**Discovered during Task 9's manual browser check** (not part of the
original plan): the extension panicked on `WasmDocument::open` with
`failed to spawn render thread: Error { kind: Unsupported, message:
"operation not supported on this platform" }`, from
`crates/openpdfedit-engine/src/thread.rs:128`'s
`thread::Builder::new().spawn(...).expect(...)`. Root cause, confirmed
precisely, not guessed: `wasm32-unknown-unknown` has no OS threads at
all — `std::thread::Builder::spawn` always returns `Err(Unsupported)`
there. `EngineHandle`'s dedicated render thread exists to guarantee
PDFium is only ever touched by one caller at a time (a real constraint —
see `thread.rs`'s own header comment, `PLAN.md §6` invariant 5) — but a
wasm32 module running in a single browser tab is *already* single-threaded,
so that guarantee holds automatically, with no thread needed to enforce
it. `openpdfedit-engine`/`thread.rs` itself needs no change; the fix is
entirely in how `openpdfedit-wasm` obtains its engine.

**Files:**
- Modify: `crates/openpdfedit-wasm/src/lib.rs`

**Interfaces:**
- Consumes: `openpdfedit_engine::PdfiumEngine` directly (not
  `EngineHandle`) — `PdfiumEngine::new_dev()` and the `Engine` trait's
  `open_bytes`/`render_page`/`save_to_bytes` methods, all already
  implemented on `PdfiumEngine` per Tasks 1 and 4.
- Produces: `engine() -> Result<&'static PdfiumEngine, JsValue>` (same
  name, same call sites in `WasmDocument`'s methods, different return
  type) — every existing caller (`WasmDocument::open`/`render_page`/
  `add_highlight`/`save`) keeps working unchanged, since `PdfiumEngine`
  implements the same `Engine` trait methods `EngineHandle` forwarded to.

- [ ] **Step 1: Change `engine()`'s implementation**

```rust
fn engine() -> Result<&'static PdfiumEngine, JsValue> {
    static ENGINE: OnceLock<Result<PdfiumEngine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| PdfiumEngine::new_dev().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| JsValue::from_str(e))
}
```

Update the `use` statement accordingly (`PdfiumEngine` instead of
`EngineHandle` — check `openpdfedit_engine`'s public exports for the
exact path, likely `openpdfedit_engine::PdfiumEngine`).

- [ ] **Step 2: Fix `render_page`'s return type usage**

`EngineHandle::render_page` returned `Arc<RenderedTile>` (for cheap
cloning across its channel); `PdfiumEngine::render_page` (the trait
method) returns an owned `RenderedTile` directly. In
`WasmDocument::render_page`, `tile.rgba.clone()` can become `tile.rgba`
(a move) since `tile` is no longer shared behind an `Arc` — check whether
this actually needs to change to compile (an owned value's field can
still be `.clone()`d harmlessly if you'd rather not touch it, but a plain
move is simpler and avoids a real copy of potentially large pixel data).

- [ ] **Step 3: Verify natively first**

```bash
export CARGO_TARGET_DIR=/tmp/openpdfedit-target
cargo build -p openpdfedit-wasm
cargo test -p openpdfedit-engine  # confirm Tasks 1-2's coverage is untouched
```

- [ ] **Step 4: Verify the wasm32 build**

```bash
cargo build -p openpdfedit-wasm --target wasm32-unknown-unknown --profile wasm-release
```

- [ ] **Step 5: Rebuild the extension and confirm `dist/` output**

```bash
cd apps/extension
npm run build
find dist -type f
```

- [ ] **Step 6: Commit**

```bash
git add crates/openpdfedit-wasm/src/lib.rs
git commit -m "openpdfedit-wasm: call PdfiumEngine directly instead of EngineHandle, which needs OS threads wasm32 doesn't have"
```

This task cannot verify the actual fix end-to-end — that's the human's
job, redoing Task 9's manual browser check once this is reviewed and
merged.

---

### Task 9: Final verification against the spec's success criteria

**Files:** none — this task is verification only.

- [ ] **Step 1: Re-read the spec's "Success criteria" section**

`docs/superpowers/specs/2026-08-10-extension-wasm-walking-skeleton-design.md`.

- [ ] **Step 2: Walk through it literally, in a fresh unpacked-extension load**

Install the unpacked extension in a clean way (remove and re-"Load
unpacked" rather than relying on a hot-reloaded state), open a real
multi-page PDF (not just the `testdata/minimal.pdf` fixture used in
Tasks 1–2's automated tests), confirm correct rendering, draw a highlight,
save, and confirm the saved file has it.

- [ ] **Step 3: Record the outcome**

Add a `## Outcome` section to the bottom of the spec doc
(`2026-08-10-extension-wasm-walking-skeleton-design.md`) stating plainly
whether this cleared the bar, and if not, exactly where it broke down —
this is the concrete answer to the "is this worth continuing" question
the whole plan exists to produce.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-08-10-extension-wasm-walking-skeleton-design.md
git commit -m "openpdfedit: record walking-skeleton outcome"
```
