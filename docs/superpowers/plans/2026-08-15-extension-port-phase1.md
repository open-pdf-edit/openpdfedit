# Extension Port Phase 1 (Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The desktop app's Svelte UI runs inside the Chrome extension as a
multi-page scroll/zoom viewer with open/save, driven by a `Backend`
adapter — Tauri on desktop (behavior unchanged), direct wasm calls in the
extension — over a new shared `openpdfedit-session` crate extracted from
the desktop's command layer.

**Architecture:** Spec:
`docs/superpowers/specs/2026-08-15-extension-full-port-design.md`. Three
moves: (1) `impl Engine for EngineHandle` so session logic can be generic
over the engine; (2) extract the desktop's portable `_impl` functions +
session state into `crates/openpdfedit-session`; (3) a TypeScript
`Backend` interface in the Svelte frontend with `TauriBackend` and
`WasmBackend` implementations, selected at build time, with the extension
packaging the desktop's SPA build.

**Tech Stack:** Rust (workspace crates), wasm-bindgen (=0.2.126 crate AND
CLI — `build-wasm.sh` enforces), Svelte 5 + SvelteKit (adapter-static SPA),
Vite, Manifest V3.

## Global Constraints

- Desktop behavior must not change. The guard is the existing workspace
  test battery (`cargo test --workspace` — all suites), `svelte-check`
  (0 errors), and a successful `cargo check -p openpdfedit-desktop`
  (its full build needs the vendored native PDFium dylib, absent in this
  environment — `check` is the available bar; note the resource-path
  build-script failure documented in task-4-report.md is pre-existing).
- `export CARGO_TARGET_DIR=/tmp/openpdfedit-target` before every cargo
  command (shared-mount rule).
- `openpdfedit-wasm` keeps its no-Worker / no-`async fn`-exports / bare
  `PdfiumEngine` architecture (safety argument documented in its
  `engine()` comment). Do not introduce Workers or async Rust exports.
- OCR (`ocr.rs`) and license (`license.rs`) stay desktop-only. Do not
  move them into the session crate.
- All work continues on branch `worktree-openpdfedit-tile-scheme-fix`.
- The UI's full backend surface (enumerated by grep this session, all
  28): open_document, save_document, save_document_as, close_window,
  undo_cmd, redo_cmd, list_page_annotations, add_annotation_cmd,
  delete_annotation_cmd, text_selection_quads_cmd, list_text_runs_cmd,
  edit_text_run_cmd, move_text_run_cmd, list_image_placements_cmd,
  move_image_cmd, list_form_fields_cmd, fill_form_fields_cmd,
  create_form_field_cmd, list_signatures_cmd, rotate_page_cmd,
  delete_page_cmd, move_page_cmd, set_crop_box_cmd, extract_pages_cmd,
  merge_documents_cmd, redact_page_cmd, compare_documents_cmd,
  ocr_document_cmd — plus the `tile://` bitmap fetch, file dialogs
  (`@tauri-apps/plugin-dialog`), and the close-requested event handshake.
- In Phase 1 the `WasmBackend` implements only the viewer surface
  (open, save, save-as, page bitmaps; close handshake as browser
  `beforeunload`). Every other method throws
  `new Error("not yet ported to the extension")` — later phases fill
  them in. The `Backend` interface itself is declared in full now.

---

### Task 1: `impl Engine for EngineHandle`

**Files:**
- Modify: `crates/openpdfedit-engine/src/thread.rs`

**Interfaces:**
- Produces: `impl Engine for EngineHandle` — each trait method forwards
  to the existing inherent method of the same name. For `render_page`
  (inherent returns `Arc<RenderedTile>`, trait returns `RenderedTile`),
  forward and unwrap with `Arc::try_unwrap(...).unwrap_or_else(|arc| (*arc).clone())`
  — requires `RenderedTile: Clone`; add `#[derive(Clone)]` to
  `RenderedTile` in `lib.rs` if not already present.

- [ ] **Step 1: Write the failing test** — in `thread.rs`'s existing
  `#[cfg(test)] mod tests`, using the existing `shared_handle()` +
  `test_corpus_path()` helpers (never a fresh engine — PDFium single-init
  constraint):

```rust
    #[test]
    fn engine_handle_works_through_the_engine_trait() {
        let Some(handle) = shared_handle() else { return };
        let bytes = std::fs::read(test_corpus_path()).expect("read fixture");
        // The whole point: session logic will hold `&dyn Engine` /
        // `E: Engine`, so EngineHandle must be usable through the trait.
        let engine: &dyn crate::Engine = handle;
        let doc = engine.open_bytes(bytes).expect("open via trait");
        assert_eq!(engine.page_count(doc).expect("page_count"), 1);
        engine.close(doc);
    }
```

- [ ] **Step 2: Run it to verify it fails to compile** —
  `cargo test -p openpdfedit-engine --lib thread::tests::engine_handle_works_through_the_engine_trait`
  → error: `EngineHandle` doesn't implement `Engine`.

- [ ] **Step 3: Implement** — `impl Engine for EngineHandle` with each
  method a one-line forward to the inherent method. Check the `Engine`
  trait's exact method list in `lib.rs` (open, close, page_count,
  render_page, page_char_boxes, page_sizes, open_bytes, save_to_bytes —
  verify against the source, don't trust this list blindly) and forward
  all of them. The trait's `open(&self, path)` forwards to inherent
  `open`; if any trait method has no inherent counterpart, add the
  forwarding through `request_reply` following the file's existing
  pattern.

- [ ] **Step 4: Run the full crate suite** —
  `cargo test -p openpdfedit-engine` → all green, including the new test.

- [ ] **Step 5: Commit** —
  `git add crates/openpdfedit-engine && git commit -m "openpdfedit-engine: implement Engine for EngineHandle so session logic can be engine-generic"`

---

### Task 2: Create `openpdfedit-session` with document/undo/save core

**Files:**
- Create: `crates/openpdfedit-session/Cargo.toml`
- Create: `crates/openpdfedit-session/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (route through the new crate)
- Modify: `apps/desktop/src-tauri/Cargo.toml` (depend on the new crate)

**Interfaces:**
- Produces: `SessionState<E: Engine>` holding what the desktop's
  `AppState` holds today minus Tauri specifics: the engine, the open-docs
  map, and the undo/redo history map. The moved functions keep their
  names: `opened_document`, `reopen_after_write`, `commit_undo_snapshot`,
  `undo_impl`, `redo_impl`, plus the open/save/save-as impl functions
  from `apps/desktop/src-tauri/src/lib.rs` (read that file first and move
  every non-`#[tauri::command]` session function it defines — the
  commands themselves stay behind as one-line wrappers calling the
  session crate). History stays keyed by `PathBuf` (a plain key type —
  works on wasm32; the extension will use synthetic paths derived from
  file names).
- Also produces: a bytes-based open for wasm callers —
  `open_document_bytes(state, display_name: &str, bytes: Vec<u8>) -> Result<OpenedDocumentInfo, SessionError>`
  — same handle/history bookkeeping as the path-based open but sourced
  from `Engine::open_bytes`, with `PathBuf::from(display_name)` as the
  history key. The path-based open stays for desktop
  (`#[cfg(not(target_arch = "wasm32"))]` if it needs `std::fs`).
- The moved code's existing unit tests move with it. Where desktop tests
  covering these paths exist in `src-tauri`, they move to the session
  crate (adjusting engine construction to the session crate's own test
  helper, which must follow the same shared-`OnceLock` pattern as
  `thread.rs` — one engine per test process, never per test).

- [ ] **Step 1: Read `apps/desktop/src-tauri/src/lib.rs` in full** and
  inventory: every non-command session function, the `AppState` fields,
  `DocHistory`, `OpenedDocument`/DTO types the UI receives. The
  inventory (with exact names) goes at the top of the session crate as a
  module doc comment.
- [ ] **Step 2: Write the failing test** — in the new crate, a
  round-trip: open bytes → edit is out of scope here, so: open bytes →
  `page_count` via state → save-to-bytes → reopen → page count matches.
  Use the corpus fixture (`testdata/minimal.pdf`, path relative to
  workspace root as the engine crate's tests do).
- [ ] **Step 3: Run it to verify it fails** (crate doesn't exist /
  doesn't compile).
- [ ] **Step 4: Move the code** — mechanical, preserving names and
  logic; genericize `&EngineHandle` parameters to `&E where E: Engine`
  (or `&dyn Engine` where object-safety suffices). Desktop's
  `#[tauri::command]` fns become wrappers.
- [ ] **Step 5: Verify** — `cargo test -p openpdfedit-session -p openpdfedit-engine`
  green, then `cargo test --workspace` green (OCR/license/desktop crates
  included), then `cargo check -p openpdfedit-desktop`.
- [ ] **Step 6: Commit** —
  `git commit -m "openpdfedit: extract engine-generic session core (docs/history/open/save/undo) into openpdfedit-session"`

---

### Task 3: Move annotation orchestration into the session crate

**Files:**
- Create: `crates/openpdfedit-session/src/annotations.rs`
- Modify: `apps/desktop/src-tauri/src/annotations.rs` (reduce to command wrappers)

**Interfaces:**
- Produces: the `_impl` functions from the desktop's `annotations.rs`
  (`add_annotation`, `list_page_annotations`, `delete_annotation`,
  `text_selection_quads` — move every `_impl`/helper the file defines;
  read it first, keep names) on `SessionState<E>`, plus their DTO types
  (`AnnotationSummaryDto` etc.) which the Tauri wrappers re-export so the
  UI's serialized shapes don't change.

- [ ] **Step 1: Write the failing test** — session-crate test: open the
  fixture via bytes, `add_annotation` (a Highlight, reuse the shapes the
  desktop tests use), `list_page_annotations` shows 1, `undo_impl`,
  list shows 0, `redo_impl`, list shows 1. This also gives the undo/redo
  machinery its first coverage in the new crate.
- [ ] **Step 2: Verify it fails** (functions not in session crate).
- [ ] **Step 3: Move + genericize**, wrappers behind.
- [ ] **Step 4: Full battery** — `cargo test --workspace`,
  `cargo check -p openpdfedit-desktop`.
- [ ] **Step 5: Commit** —
  `git commit -m "openpdfedit-session: move annotation orchestration out of the desktop shell"`

---

### Task 4: Move forms + field-creation orchestration

**Files:**
- Create: `crates/openpdfedit-session/src/forms.rs`
- Modify: `apps/desktop/src-tauri/src/forms.rs`, `apps/desktop/src-tauri/src/field_create.rs`

Same shape as Task 3: read both files, move the `_impl` functions and
DTOs, leave one-line command wrappers, move/adapt their tests.

- [ ] **Step 1: Failing session-crate test** — open a fixture with a
  form field if one exists in `testdata/` (check; the forms tests in the
  desktop crate will say which fixture they use — reuse it), list fields,
  fill one, list reflects the value.
- [ ] **Step 2: Verify fails → Step 3: Move → Step 4: Full battery →
  Step 5: Commit** `"openpdfedit-session: move forms + field-creation orchestration"`

---

### Task 5: Move page-organization orchestration

**Files:**
- Create: `crates/openpdfedit-session/src/pages.rs`
- Modify: `apps/desktop/src-tauri/src/pages.rs`

Same shape: rotate/delete/move/crop/extract/merge `_impl`s + DTOs +
tests. Note `merge_documents`/`extract_pages` are path-based on desktop
(they read/write files); move the byte-level core and keep the
file-touching entry points desktop-side (`#[cfg]` or wrapper-level) so
the session crate stays wasm-clean — **verify with
`cargo build -p openpdfedit-session --target wasm32-unknown-unknown`**
in this task's verification step, so path-dependencies surface here and
not in Task 8.

- [ ] Steps as Task 4; commit
  `"openpdfedit-session: move page-organization orchestration"` — and the
  wasm32 build of the session crate must pass before committing.

---

### Task 6: Move textedit / redact / signatures / compare orchestration

**Files:**
- Create: `crates/openpdfedit-session/src/{textedit,redact,signatures,compare}.rs`
- Modify: the four corresponding `apps/desktop/src-tauri/src/*.rs`

Same shape. `compare` takes two handles — both from the session's doc
map, no new state. After this task the desktop's `src-tauri/src/` should
contain only: thin command wrappers, OCR, license, the Tauri
builder/close-window/tile-protocol glue in `lib.rs`, and `main.rs`.

- [ ] Steps as Task 4, plus
  `cargo build -p openpdfedit-session --target wasm32-unknown-unknown`
  again; commit
  `"openpdfedit-session: move textedit/redact/signatures/compare orchestration"`

---

### Task 7: `Backend` interface + `TauriBackend` in the frontend

**Files:**
- Create: `apps/desktop/src/lib/backend/types.ts` (the interface + all DTO types, moved from their current scattered declarations)
- Create: `apps/desktop/src/lib/backend/tauri.ts`
- Create: `apps/desktop/src/lib/backend/index.ts`
- Modify: `apps/desktop/src/routes/+page.svelte` and every component/module currently importing `invoke`, `open`/`save` from plugin-dialog, or building `tile://` URLs (`PdfPage.svelte`, `PageThumb.svelte`, and whatever else grep finds — run `grep -rl 'invoke\|plugin-dialog\|TILE_ORIGIN' apps/desktop/src` and cover every hit)

**Interfaces:**
- Produces `interface Backend` in `types.ts` with one method per
  command in the Global Constraints list (camelCased, e.g.
  `openDocument(path or picked file) → OpenedDocument`), plus:

```ts
  /** Raw RGBA page bitmap — the shared wire format both backends produce. */
  getPageBitmap(handle: number, pageIndex: number, targetWidth: number): Promise<{ width: number; height: number; rgba: Uint8ClampedArray }>;
  /** Open picker + open document in one step — path-based on desktop, bytes-based in the extension. */
  pickAndOpenDocument(): Promise<OpenedDocument | null>;
  saveDocumentAs(handle: number): Promise<OpenedDocument | null>; // picker inside
  onCloseRequested(cb: () => void): Promise<() => void>;
  confirmClose(): Promise<void>;
```

- `index.ts` exports `backend`, chosen by
  `import.meta.env.VITE_BACKEND === "wasm"` → dynamic import of
  `wasm.ts` (Task 8) else `tauri.ts`. Because the wasm module can't
  exist in the desktop build, use a dynamic `import()` so Vite
  tree-shakes/code-splits per build; top-level await is already enabled
  in this stack (extension config) — for the desktop build, resolve the
  backend before app mount in `+layout.svelte` if top-level await is a
  problem under the desktop's Vite targets.
- `tauri.ts` wraps today's exact calls: each method is the current
  `invoke("...")` with the same args; `getPageBitmap` fetches
  `${TILE_ORIGIN}/{handle}/{page}/{width}` and reads the
  `X-Tile-Width`/`X-Tile-Height` headers + body exactly as
  `PdfPage.svelte` does today (move that logic here);
  `pickAndOpenDocument` = plugin-dialog `open()` + `invoke("open_document")`.

- [ ] **Step 1: Write `types.ts` + `tauri.ts` + `index.ts`.**
- [ ] **Step 2: Migrate call sites** — mechanical, one import swap and
  method call per site; `PdfPage.svelte`/`PageThumb.svelte` use
  `backend.getPageBitmap` and paint via `putImageData` (the fetch/header
  parsing moves INTO `tauri.ts`, deleting the component-local copies and
  the `tileOrigin.ts` import from components — `tileOrigin.ts` itself
  moves under `backend/`).
- [ ] **Step 3: Verify** — `npm run check` (svelte-check, 0 errors) and
  `npm run build` in `apps/desktop` both green. Behavior-preservation
  evidence: the built app is not runnable in this environment (missing
  native dylib), so the bar is: zero UI logic changes beyond the
  mechanical call rewrites (reviewer checks this), plus green typecheck.
- [ ] **Step 4: Commit** —
  `"openpdfedit: route the frontend through a Backend adapter (Tauri impl, behavior-preserving)"`

---

### Task 8: `WasmBackend` + wasm crate session rewiring

**Files:**
- Modify: `crates/openpdfedit-wasm/src/lib.rs` (rebuild on `openpdfedit-session` instead of raw engine calls)
- Create: `apps/desktop/src/lib/backend/wasm.ts`

**Interfaces:**
- `openpdfedit-wasm` re-exposes, via a `WasmSession` (wasm-bindgen
  class, replacing per-document `WasmDocument`): `openDocument(name, bytes) → JSON OpenedDocument`,
  `saveToBytes(handle) → Uint8Array`, `renderPage(handle, page, width) → RenderedPage`
  (keeping pointWidth/pointHeight getters — the coordinate transform
  depends on them), `pageSizes(handle) → JSON`, `closeDocument(handle)`
  — **not** `undo`/`redo`: discovered during Task 3, the entire
  mutate/undo pathway (`commit_mutation` → `std::fs::write` →
  `reopen_after_write` → path-based reopen) is filesystem-bound, so
  undo/redo on wasm requires a working-store abstraction (in-memory
  bytes vs on-disk working copy) that belongs in Phase 2's plan
  alongside the annotation port that needs it for anything. Phase 1's
  wasm surface is strictly read-render-save. All exposed methods are
  thin calls into
  `openpdfedit-session`'s state + impls, with `serde_json::to_string`
  for DTO-shaped returns (add `serde` derives in the session crate where
  missing; the desktop DTOs already derive `Serialize` for Tauri).
- `wasm.ts` implements `Backend`: viewer surface real (open via
  `showOpenFilePicker`, keeping the `FileSystemFileHandle` in a map so
  `saveDocument` can `createWritable` back to the same file; save-as via
  `showSaveFilePicker`), `onCloseRequested` via `beforeunload` when any
  doc is dirty, **all non-viewer methods `throw new Error("not yet ported to the extension")`**.
  The PDFium init sequence (PDFiumModule → init → initialize_pdfium_render)
  moves here from the skeleton's `editor.ts`, comments preserved.
- The skeleton's coordinate transform (canvas px ↔ PDF pt) lives with
  the UI's pointer handling (already correct in the shared components) —
  `wasm.ts` only supplies `pointWidth`/`pointHeight` via the bitmap
  metadata if the UI needs them; check how `PdfPage.svelte` computes
  `pxPerPt` (it derives from page sizes) and match that path.

- [ ] **Step 1: Rewrite `openpdfedit-wasm` onto the session crate** —
  failing check first: `cargo build -p openpdfedit-wasm --target wasm32-unknown-unknown --profile wasm-release`
  after the rewrite must pass; native `cargo test --workspace` stays green.
- [ ] **Step 2: Write `wasm.ts`.**
- [ ] **Step 3: Verify** — wasm32 build + `npm run check` in
  `apps/desktop` with `VITE_BACKEND=wasm` type-context (the file must
  typecheck even though the desktop build won't include it).
- [ ] **Step 4: Commit** —
  `"openpdfedit: WasmBackend over openpdfedit-session (viewer surface)"`

---

### Task 9: Extension ships the shared SPA

**Files:**
- Modify: `apps/extension/package.json`, `apps/extension/vite.config.js` → replaced by a build script that drives the desktop SPA build
- Create: `apps/extension/scripts/build-spa.sh` (builds `apps/desktop` frontend with `VITE_BACKEND=wasm` and relative base, copies output into `dist/`, then runs the existing vendor/wasm-gen copy steps)
- Create: `apps/extension/scripts/externalize-inline.mjs` (see CSP note)
- Modify: `apps/extension/public/manifest.json` (page becomes `index.html`)
- Delete: `apps/extension/editor.html`, `apps/extension/editor.ts` (their unique content — PDFium init, FS Access pickers — has moved into `wasm.ts` in Task 8; verify nothing else is lost before deleting, and say so in the report)

**Known trap, handle deliberately (CSP):** SvelteKit's adapter-static SPA
`index.html` contains an inline `<script>` bootstrap. MV3's
`extension_pages` CSP (`script-src 'self' 'wasm-unsafe-eval'`) forbids
inline scripts, so the built page would silently not boot.
`externalize-inline.mjs` post-processes `dist/index.html`: move each
inline script's content to `dist/inline-N.js` and replace with
`<script type="module" src="./inline-N.js">` (preserving execution
order). This is the same class of packaging fix `openscreenshot`'s
`copy-static.mjs` precedent establishes. If SvelteKit offers a config
flag that avoids the inline bootstrap entirely (check
`kit.output`/`bundleStrategy` options in the installed version first),
prefer that over post-processing.

- [ ] **Step 1: `build-spa.sh`** — `VITE_BACKEND=wasm npm run build` in
  `apps/desktop` with SvelteKit `paths.relative` default (verify asset
  URLs in the output are relative or root-absolute; both work at the
  extension origin root, matching the skeleton's earlier finding), copy
  `apps/desktop/build/*` → `apps/extension/dist/`, run
  `externalize-inline.mjs`, then the existing `copy-vendor.sh`.
- [ ] **Step 2: Wire `apps/extension/package.json`'s `build`** to:
  `build-wasm.sh` → typecheck → `build-spa.sh`. Update `background.ts`'s
  `chrome.tabs.create` URL to `index.html`, and `manifest.json`
  accordingly.
- [ ] **Step 3: Verify** — full `npm run build` in `apps/extension`;
  `dist/` must contain: `manifest.json`, `background.js`, `index.html`
  (with NO inline `<script>` — grep for `<script>` without `src=`),
  the SPA's `_app/` assets, `pdfium.js`, `pdfium.wasm`, wasm-gen glue.
- [ ] **Step 4: Commit** —
  `"openpdfedit-extension: package the shared desktop SPA as the extension UI"`

---

### Task 10: Phase 1 battery + ledger

- [ ] **Step 1:** Run everything, in order, all green:
  `cargo test --workspace` · `cargo build -p openpdfedit-session -p openpdfedit-wasm --target wasm32-unknown-unknown` ·
  `cargo check -p openpdfedit-desktop` · `npm run check` + `npm run build` (apps/desktop) ·
  `npm run build` (apps/extension) · `cargo fmt --check` · `clippy -D warnings`
  (workspace).
- [ ] **Step 2:** Append Phase 1 completion + any deferred findings to
  the SDD ledger; note anything Phase 2's plan must know (actual
  session-crate API names as landed, any deviation from this plan).
- [ ] **Step 3:** Commit any doc updates.

---

## Phases 2-5

Planned separately once Phase 1 lands, so their plans consume the
session crate's real, landed API instead of this plan's predictions.
Phase 2 = annotations parity (wasm wrappers + `wasm.ts` methods for the
annotation/undo/text-selection surface — the UI side already exists and
is shared). Phase 3 = forms + pages. Phase 4 = signatures/redact/
textedit/image-move. Phase 5 = compare + packaging.
