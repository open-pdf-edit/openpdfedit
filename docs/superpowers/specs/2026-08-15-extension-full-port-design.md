# Extension full port: shared UI, shared session logic, feature parity

## Problem

The walking skeleton (see
`2026-08-10-extension-wasm-walking-skeleton-design.md`, Outcome section)
proved OpenPdfEdit's engine runs against WASM PDFium in a real Chrome
extension. But the skeleton's editor is a bare canvas with two buttons,
while the desktop app is a full editor: ten markup tools, comments/
pages/forms/signatures panels, text and image editing, redaction,
compare, undo/redo. The user has asked for the extension to be developed
to feature parity ("develop everything"), continuously, with one manual
browser test at the end.

## Decisions already made by the user (do not re-litigate)

- **Shared UI**: the extension reuses the desktop's Svelte 5 frontend
  through a backend-adapter interface — not a separate hand-built
  extension UI.
- **Continuous execution**: phases run back-to-back without per-phase
  user check-ins; the user tests everything in one manual pass at the
  end.
- **Deferred entirely**: OCR (desktop shells out to a local `tesseract`
  binary; a browser build needs a WASM OCR engine — its own future
  project), Firefox (no File System Access API), batch (CLI concern),
  and license gating.

## Standing risk, accepted explicitly

The walking skeleton's coordinate fix (canvas px → PDF points) was
verified by independent mathematical re-derivation in review but has
**not yet had its real-Chrome re-test** — the user chose to test
everything in one go at the end. If that final test shows misplaced
annotations, the coordinate transform is the first suspect, and
everything annotation-placing built here inherits that risk knowingly.

## Architecture

Three load-bearing facts, verified by direct code inspection this
session, make the shared approach cheap rather than heroic:

1. **The UI's entire backend surface is 28 `invoke()` commands** (plus
   the `tile://` pixel fetch, file dialogs, and the close-window
   handshake). Enumerated by grep over `apps/desktop/src`; nothing else
   crosses the boundary.
2. **The desktop's command layer is already split portable/glue.**
   Every command in `apps/desktop/src-tauri/src/*.rs` is a plain
   `_impl` function taking bare `&Mutex<...>`/engine references, with
   the `#[tauri::command]` wrapper as a thin `state.x` adapter — a
   deliberate prior design choice (documented in `annotations.rs`'s own
   comments). Undo/redo history lives in these portable impls.
3. **`EngineHandle` and `PdfiumEngine` expose the same operations**, so
   the impls can be made generic over the existing `Engine` trait (with
   `impl Engine for EngineHandle` forwarding), letting the same session
   logic drive the desktop's thread-wrapped engine and the extension's
   direct engine.

### The three structural pieces

**1. `crates/openpdfedit-session` (new)** — the shared orchestration
crate. The `_impl` functions and session state (`docs` map, `history`
map, dirty tracking, handle rotation on edit) move here from
`apps/desktop/src-tauri/src/*.rs`, genericized over `Engine`. Tauri
commands become one-line wrappers calling into it (a behavior-preserving
refactor, guarded by the existing 31-suite workspace test battery, which
moves with the code). `openpdfedit-wasm` calls the same crate directly.
OCR's command stays desktop-side (it isn't portable and never will be in
this form).

**2. `Backend` adapter in the Svelte frontend** — a TypeScript interface
mirroring the 28 commands plus `getPageBitmap(handle, page, width)`
(replacing raw `tile://` fetches; the desktop impl fetches the tile URL,
the extension impl calls wasm `renderPage` — both yield the same
raw-RGBA + dimensions shape, which is already the shared wire format),
`pickOpenFile`/`pickSaveFile` (plugin-dialog vs File System Access API),
and `onCloseRequested` (Tauri close handshake vs browser `beforeunload`).
UI components stop importing `invoke` and call the backend singleton;
which implementation loads is a build-time flag (`VITE_BACKEND`).

**3. The extension ships the desktop's SPA build.** `apps/desktop`'s
SvelteKit SPA (adapter-static, SPA mode) is built a second time with
the wasm backend flag and relative asset paths; `apps/extension` becomes
a thin packaging harness around that output — manifest, background
worker, PDFium vendor copy, CSP. The skeleton's hand-written
`editor.html`/`editor.ts` are retired once the shared shell works
(their PDFium init sequence and coordinate transform move into the wasm
backend implementation, they don't get rewritten from scratch).

### wasm threading note (standing tripwire)

`openpdfedit-wasm` keeps its bare-`PdfiumEngine`, no-Worker,
no-`async fn`-exports architecture, whose safety argument was verified
in the skeleton. Rendering stays on the main thread through this entire
port. If profiling later demands Worker offloading, the whole wasm
module moves into one Worker (preserving single-threadedness inside it)
— that is out of scope here and must not be done casually.

## Phases

Each phase = one implementation plan, executed with the same
subagent-per-task + per-task-review + final-review process as the
skeleton. A phase must leave the workspace green (native tests, wasm32
build, extension build, typecheck, svelte-check) before the next starts.

- **Phase 1 — foundation**: `openpdfedit-session` extraction (desktop
  refactored onto it, all existing tests green); `Backend` interface +
  Tauri impl (desktop behavior unchanged); wasm backend covering the
  viewer surface (open/save/render/page-sizes); extension ships the
  shared SPA with multi-page scroll/zoom viewer working. This is the
  phase where the architecture can still fail; it gets the most
  scrutiny.
- **Phase 2 — annotations parity**: all markup tools, comments panel,
  delete/undo/redo, text-selection quads, via wasm backend methods over
  `openpdfedit-session`.
- **Phase 3 — forms + page organization**: list/fill/create form
  fields; rotate/delete/reorder/crop/extract/merge pages.
- **Phase 4 — signatures, redaction, text/image editing**: signature
  listing + placement (drawn signatures), redact-page, text-run
  edit/move, image move.
- **Phase 5 — compare + packaging**: compare-documents flow (second
  file picked via File System Access API), extension icons, store
  listing text, packaging checklist. Publishing itself (store accounts,
  payments) stays with the user.

## Success criteria

The user performs one manual pass in real Chrome at the end: every
ported feature exercised against a real PDF — and the desktop app,
rebuilt from the same branch, still behaves identically to before
(spot-check, since its test suites guard the refactor continuously).
Highlight placement accuracy (the standing risk above) is explicitly
part of that pass.

## Out of scope

OCR, Firefox, batch, license gating/monetization of the extension,
Worker offloading, store publication itself.
