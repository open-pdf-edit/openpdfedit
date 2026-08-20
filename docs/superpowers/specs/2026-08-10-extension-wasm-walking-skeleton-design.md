# Browser extension walking skeleton: PDFium-in-WASM

## Problem

OpenPdfEdit is a native desktop app (Tauri + Rust, `pdfium-render` late-bound
to a native `libpdfium.dylib`/`.so`/`pdfium.dll`). There's interest in a
Chrome/Firefox extension version, running the same engine and editing
logic, so a user doesn't need to install a desktop app to edit a PDF. The
one thing that makes this genuinely uncertain — not merely more work, but
possibly not viable at all — is whether OpenPdfEdit's actual rendering
crate can run against a WASM build of PDFium instead of a native library.
Nothing in this repo has tried that yet.

## Scope

A minimal, separate Chrome extension that proves the core pipeline end to
end: open a PDF, render it via a WASM PDFium build, draw one rectangle or
arrow annotation, save a PDF back out with that annotation baked in.
Nothing more.

Explicitly **not** in scope for this phase (each is either likely-easy
follow-on work once the core pipeline is proven, or its own separate
decision):

- Forms, signatures, redaction, document compare, batch operations —
  existing desktop features, deferred until the core pipeline is proven.
- OCR — cannot work the same way at all. The desktop app shells out to a
  local `tesseract` binary; a browser extension sandbox has no local
  binary to shell out to. This needs a WASM-native OCR engine as its own
  future decision, unrelated to this port.
- Firefox — Chrome only for this phase. The File System Access API this
  design uses for open/save isn't available in Firefox.
- Any change to `apps/desktop` or `apps/cli` — neither is touched by this
  work in any way.

## Fallback safety

This is built on its own git branch, in its own new directory
(`apps/extension/`, alongside the existing `apps/desktop` and `apps/cli`).
The existing desktop app is not modified. If the WASM PDFium pipeline
turns out to be too slow, too large to ship, or genuinely broken in a
browser, the fallback is simply not merging the branch — there is nothing
to revert, because nothing outside the new branch ever changes.

## Architecture

**New crate `crates/openpdfedit-wasm`** — a thin `wasm-bindgen` wrapper
that calls into the existing `openpdfedit-doc` and `openpdfedit-annot`
crates rather than reimplementing their logic. Exposed JS-facing API:

- `open_document(bytes: &[u8]) -> DocHandle`
- `render_page(handle, page_index, width) -> Vec<u8>` (raw RGBA)
- `add_rect_annotation(handle, page_index, rect)` /
  `add_arrow_annotation(handle, page_index, x0, y0, x1, y1)`
- `save_document(handle) -> Vec<u8>`

`openpdfedit-doc`'s `Document::from_bytes(&bytes)` is already the real
core of document loading (`Document::open(path)` is just
`std::fs::read` + `from_bytes`), and `save_incremental()` already returns
`Vec<u8>` — the document model crate has no filesystem assumption baked
into its actual logic, confirmed by reading
`crates/openpdfedit-doc/src/lib.rs` directly rather than assumed.

**The one real unknown**: whichever crate currently talks to
`pdfium-render` for rasterization — looks like `openpdfedit-engine`,
based on `apps/desktop/src-tauri/src/lib.rs`'s `EngineHandle::spawn(...)`
— has to work against `pdfium-render`'s WASM late-binding path instead of
its native-dylib late-binding path. This has not been verified. Proving
this one thing is the actual point of the walking skeleton; everything
else in this design exists to give that a real, running extension to
prove it in.

**Vendored PDFium WASM binary** — fetched into `.vendor/` by a new script
mirroring `scripts/fetch-pdfium.sh`'s pattern (fetch once, vendor
locally, no runtime CDN fetch), sourced from `@embedpdf/pdfium`. This is
**not** the same upstream (`bblanchon/pdfium-binaries`) the native build
already trusts — that project has no WASM release asset — so this is a
new third-party dependency, not an extension of an existing one. Swap for
`paulocoutinhox/pdfium-lib` or `urish/pdfium-wasm` if `@embedpdf/pdfium`
doesn't pan out; none of the three has been tried yet either.

Vendoring rather than a runtime CDN fetch isn't just a style preference
here — Chrome Web Store policy (enforced since August 2026) prohibits
remotely-hosted code in Manifest V3 extensions, and Chrome no longer
grants `wasm-unsafe-eval` by default, so the extension's
`content_security_policy.extension_pages` needs that directive added
explicitly.

**Extension shell** — a dedicated full-tab editor page, not a popup,
mirroring `openscreenshot`'s existing `editor.html`/`editor.ts` structure
(real precedent in this monorepo, and PDF editing needs real screen
space).

**File I/O** — the File System Access API (`showOpenFilePicker` /
`showSaveFilePicker`) replaces Tauri's native file dialogs. Genuinely
different from today's `@tauri-apps/plugin-dialog` usage, and the reason
this phase is Chrome-only.

**No Tauri IPC** — `invoke()` doesn't exist in this context. The WASM
module runs in the same JS realm as the extension page, so every call
into it is a direct function call, not a cross-process command.
Rendering stays on the main thread for this first skeleton rather than
also introducing a Web Worker — proving the rendering pipeline works at
all comes before optimizing where it runs.

**Data flow**: pick file → bytes → `open_document` → handle kept alive in
the WASM module's own memory (mirroring the desktop app's
`HashMap<handle, Document>` in `AppState`) → `render_page` per visible
page paints raw RGBA straight to a canvas — the same wire format the
desktop app's `tile://` protocol already produces, so `PdfPage.svelte`'s
canvas-painting logic is close to directly reusable → user draws a
rect/arrow → `add_rect_annotation` → re-render that page → `save_document`
→ `showSaveFilePicker` write-back.

## Success criteria

Install the unpacked extension in Chrome, open a real PDF, see it
rendered correctly, draw a rectangle or arrow on it, save a PDF back out
that has that annotation baked in when reopened. That is the entire bar
for this phase. Clearing it means there's a real, working pipeline to
decide whether to keep porting features into. Not clearing it — the
engine is unworkably slow, the WASM binary is too large to ship
reasonably, or the rendering crate can't be adapted at all — means
finding that out having built almost nothing, on a branch that's safe to
simply abandon.

## Open risks (unresolved, deliberately — the skeleton exists to answer these)

- Does `openpdfedit-engine` actually work against `pdfium-render`'s WASM
  late-binding path? Unverified.
- How large is the vendored `@embedpdf/pdfium` WASM binary once bundled,
  and is that an acceptable extension download size?
- Is main-thread rendering fast enough to feel responsive, or does this
  immediately need Web Worker offloading even for a walking skeleton?

## Outcome

**Cleared the bar.** Confirmed by a human in a real Chrome install: open a
real PDF, render it via the vendored WASM PDFium build, draw a highlight
by dragging on the canvas, save, and — reopening the saved file — the
highlight is genuinely present, in the right place. The full loop this
skeleton was scoped to prove now works end to end.

It didn't work on the first real-browser try, and what broke is worth
keeping on record precisely because it isn't the risk this doc predicted:

- **What did NOT break**: PDFium's own WASM late-binding
  (`pdfium-render`'s `bind_to_system_library()` path) worked once the JS
  init sequence was wired correctly — resolved during implementation by
  reading `pdfium-render`'s own maintained example rather than guessing,
  and independently re-verified against the same source during review.
  Bundle size and rendering responsiveness were never actually a problem
  either — no Web Worker offload was needed to get this working.
- **What DID break**: `openpdfedit-engine`'s dedicated render thread
  (`EngineHandle`, which exists to guarantee PDFium is only ever called
  from one place at a time) can't be constructed on `wasm32-unknown-unknown`
  at all — that target has no OS threads, so `std::thread::Builder::spawn`
  always fails there. This is an architectural fact this design doc didn't
  anticipate, not a bug in any task's implementation.
- **The fix, once found, was narrow**: a wasm32-unknown-unknown module
  running in one browser tab is already single-threaded, so the guarantee
  a dedicated thread exists to enforce holds automatically — no thread
  needed to make it so. `openpdfedit-wasm`'s `engine()` helper now
  constructs a bare `PdfiumEngine` directly instead of going through
  `EngineHandle`. Nothing in `openpdfedit-engine`, `apps/desktop`, or
  `apps/cli` changed. This was reviewed with the same scrutiny as every
  other task — including independently checking whether the "wasm32 is
  single-threaded" safety argument actually holds given this crate's
  specific code (it does: no `async fn` exports, no Worker/SharedArrayBuffer
  usage anywhere in the extension — a fact about the code today, worth
  re-checking if either of those two things ever changes).

**What this means for continuing past the walking skeleton**: the core
pipeline is proven. Porting further features (forms, signatures,
redaction, compare, batch) is now a reasonable next step rather than a
gamble — each one will need the same question asked (does it assume a
dedicated thread, a real filesystem, or anything else native-only), but
the pattern for answering that question and fixing it cheaply is now
established, not theoretical.
