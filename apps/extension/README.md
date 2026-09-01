# OpenPdfEdit — browser extension

A Chrome MV3 extension build of OpenPdfEdit. As of Task 9, this ships the
same Svelte 5 SPA `apps/desktop` builds (its full UI — Viewer, Pages,
Forms, Signatures, Comments panels, etc.) running against an in-browser
WASM PDF engine instead of Tauri's native IPC commands. There is no
extension-specific UI left in this package: `apps/extension` builds
`background.ts` (the MV3 service worker) and a small asset pipeline;
everything a user actually sees comes from `apps/desktop`.

Before Task 9, this was a hand-written walking skeleton (`editor.html` +
`editor.ts`: open a PDF, render its first page, drag to add one
highlight, save) — see
`docs/superpowers/plans/2026-08-10-extension-wasm-walking-skeleton.md`
for the plan that built it, and `.superpowers/sdd/2026-08-15-extension-port-phase1/`
for the plan that replaced it. `editor.ts`'s unique content (the PDFium
loading-contract archaeology, the `WasmSession`/`WasmModuleExports`
loading sequence, the File System Access API ambient type declarations)
moved into `apps/desktop/src/lib/backend/wasm.ts` in Task 8/9 — see that
file's header comment. The coordinate-transform lesson (canvas pixels vs.
PDF points, y-flip) already lived in the shared `PdfPage.svelte`, which
this build now uses directly instead of re-deriving it.

**Status as of Phase 5 (port-closing):** the wasm backend (`wasm.ts`) now
covers the full editing surface — annotations, text edit, forms,
signatures, page organization (rotate/delete/move/crop/extract), redact,
undo/redo, and (Phase 5 Task 2) multi-file merge and compare. The one
method that still throws is `ocrDocument` — genuinely unavailable in this
backend (the desktop's Tesseract-sidecar OCR has no wasm story), not just
unported. Unlike the rest of this file's history, that gap is no longer a
"button that throws when clicked": Phase 5 Task 3 gated it out via a
build-time `backendKind` export from `apps/desktop/src/lib/backend/index.ts`
(`"tauri" | "wasm"`), so `+page.svelte` hides the OCR button entirely
under `VITE_BACKEND=wasm`. See `wasm.ts`'s own doc comment for the
method-by-method history of how this surface got built out.

## Prerequisites (one-time)

These aren't obvious from `package.json` alone — each was a real failure
someone hit while building this the first time, so they're spelled out
here rather than left to be rediscovered:

1. **Fetch the vendored PDFium WASM build first.** Run
   `bash ../../scripts/fetch-pdfium-wasm.sh` (from this directory) before
   anything else. This downloads `pdfium.js`/`pdfium.wasm` into
   `.vendor/pdfium-wasm/` at the workspace root — the *WASM* build of
   PDFium, a different artifact from the native `.dylib`/`.so`/`.dll`
   the desktop app uses (fetched by `scripts/fetch-pdfium.sh`, not this
   one). `scripts/copy-vendor.sh` fails with a clear error naming this
   script if it hasn't been run.

2. **Install the `wasm32-unknown-unknown` Rust target**, if you haven't
   already: `rustup target add wasm32-unknown-unknown`.

3. **Install `wasm-bindgen-cli` at the exact version the workspace's root
   `Cargo.toml` pins** (`wasm-bindgen = "=<version>"`, currently 0.2.126 —
   check `Cargo.toml` for the current value, don't trust this number to
   stay in sync). The `wasm-bindgen` crate version and the `wasm-bindgen-cli`
   *binary* version must match exactly (the generated JS glue is versioned
   against the CLI); `scripts/build-wasm.sh` checks this automatically
   and fails with the exact install command if they're out of sync, e.g.:
   ```
   cargo install wasm-bindgen-cli --version 0.2.126 --force
   ```

4. **`apps/desktop`'s own dependencies must be installed too** (`cd
   ../desktop && npm install`) — `scripts/build-spa.sh` drives a real
   build of that package as part of this one's `npm run build`, it isn't
   vendored or pre-built.

## Building

```bash
npm install
npm run build
```

This runs, in order (see `package.json`'s `build` script and
`scripts/build-spa.sh` for the exact chain):

1. `scripts/build-wasm.sh` — compiles `crates/openpdfedit-wasm` for
   `wasm32-unknown-unknown` and generates wasm-bindgen's JS/TS glue into
   `src/wasm-gen/` (gitignored, regenerated fresh by every build — see
   that script's own comment for why the CLI/crate version pinning
   matters here).
2. `tsc --noEmit` against `background.ts` (the only TypeScript source
   file left in this package — see `tsconfig.json`'s comment for why
   `@types/chrome` is a dependency now, closing a gap that used to force
   excluding this file from typecheck entirely).
3. `scripts/build-spa.sh`, which itself:
   - Runs `vite build` here to produce `dist/background.js` (and, via
     `vite.config.js`'s `copyVendorAfterBuild` plugin, a first pass of
     `scripts/copy-vendor.sh`).
   - Builds `apps/desktop`'s SPA with `VITE_BACKEND=wasm` — this makes
     that app's `initBackend()` resolve to the real `WasmBackend`
     (`wasm.ts`) instead of the default Tauri one; see
     `apps/desktop/src/lib/backend/index.ts`'s doc comment for how the
     two build flavors stay cleanly separated (the Tauri-flavored build
     ships zero bytes of `wasm.ts`, and vice versa).
   - Copies that SPA build's output (`apps/desktop/build/`) into `dist/`,
     merging with what step 3a already put there.
   - Runs `scripts/externalize-inline.mjs` against `dist/index.html` —
     **the load-bearing CSP fix**: SvelteKit's adapter-static output
     always has one inline bootstrap `<script>`, and MV3's
     `extension_pages` CSP (`script-src 'self' 'wasm-unsafe-eval'`, no
     `'unsafe-inline'`) forbids inline script content outright. Without
     this step the built extension loads but silently never boots — no
     error beyond a CSP violation line in devtools. See that script's own
     header comment for what was tried and ruled out first
     (`kit.output.bundleStrategy`'s `'single'`/`'inline'` settings —
     neither avoids the inline bootstrap; `'inline'` makes it worse).
   - Re-runs `scripts/copy-vendor.sh` (pdfium.js/pdfium.wasm +
     `wasm-gen/*`) once more, so the vendored/generated assets are
     guaranteed present regardless of build-step ordering elsewhere.

The repo lives on a shared VM mount that produces spurious errors when
used as Cargo's target directory directly — set
`CARGO_TARGET_DIR=/tmp/openpdfedit-target` (or similar, off-mount) before
building. The same mount has occasionally produced spurious, non-Cargo
build failures too (a stray "Unexpected end of JSON input" or ENOENT from
Vite/SvelteKit mid-build, gone on an immediate retry with no other change)
— if a build fails with an error that doesn't obviously point at this
codebase, retry once before assuming something real broke.

`npm run dev` (`vite build --watch`, unchanged by Task 9) only rebuilds
this package's own `background.js` on file changes plus `copy-vendor.sh`
— it does **not** rebuild the SPA, so `dist/index.html` and `dist/_app/`
are only ever produced/refreshed by a full `npm run build`. There's no
fast watch-mode loop for the SPA side of this extension today; iterate on
`apps/desktop` directly (`cd ../desktop && VITE_BACKEND=wasm npm run
dev`) for that, then re-run `npm run build` here to package a fresh
snapshot.

## Loading the unpacked extension

In Chrome: `chrome://extensions` → enable Developer mode → "Load
unpacked" → select this directory's `dist/`.

## Packaging for the Chrome Web Store

```bash
npm run package
```

Runs a fresh `npm run build`, then zips `dist/`'s contents into
`openpdfedit-dist.zip` (this package's root) via
`scripts/package-zip.sh`. That zip *is* committed, deliberately — the
root README points people at it as the fastest way to load the
extension without a Rust toolchain, and the root `.gitignore` says so.
Rebuild and commit it when the extension changes.
See `STORE.md` for the listing copy (short/long description, category,
privacy declaration) and the submission checklist — most of that
checklist is human-only (developer account, screenshots, actually
submitting for review); this script only produces the upload artifact.

`manifest.json`'s icons (`public/icons/{16,48,128}.png`, wired into both
the top-level `icons` key and `action.default_icon`) are generated from
`apps/desktop/src-tauri/icons/icon.png` — the same brand mark the desktop
app ships — via `scripts/generate-icons.sh` (uses macOS's `sips`; not run
automatically by `npm run build`, since the source master essentially
never changes). Re-run it after any brand-asset update.

`manifest.json`'s `minimum_chrome_version` is `"103"` — JSON has no
comment syntax, so the rationale lives here instead: `content_security_policy.extension_pages`'s `'wasm-unsafe-eval'` source
only exists starting Chrome 103, and the built bundle also calls
`Array.prototype.findLast` (Chrome 97+), so 103 is the real floor, not
the `"89"` this manifest declared before the Phase 5 final-review fix round.

## Automated boot check (`npm run e2e`)

`npm run build`/`npm run check`/`vite build` all stay green regardless of
whether the packaged `dist/index.html` actually renders the editor —
that mismatch (SvelteKit's client router rendering its own "404 — Not
found" instead of the editor, because `chrome-extension://<id>/index.html`
doesn't match any route in the table) is a *runtime* routing decision,
invisible to every static check in this pipeline. `e2e/boot.spec.ts` is
the one check that actually catches it: it launches a real (headless)
Chromium with this package's own `dist/` loaded as an unpacked
extension — mirroring `opencapture/apps/extension/e2e/fixtures.ts`'s
`launchPersistentContext` + `--load-extension` setup — opens
`chrome-extension://<id>/index.html`, and asserts the editor's
empty-state "Open PDF…" button is visible (and that the page body never
contains "404"). See `apps/desktop/src/hooks.ts`'s `reroute` hook for the
actual fix this test guards.

```bash
npm run build   # dist/ must exist and be current — the test doesn't build it
npm run e2e
```

Deliberately **not** chained into `npm run build`: it needs a real
browser (`npx playwright install chromium` once, if this machine doesn't
already have Playwright's Chromium cached — `channel: "chromium"`
specifically requires Playwright's own bundled Chromium, not any
system/Chrome-stable install), and `build` should stay usable in
environments without one.

## Known gaps (as of Phase 5 Task 3 — port-closing)

This section used to say "Phase 1 — viewer-only wasm backend" and list
every editing operation as unported; that was true through Phase 1 but
stale from Phase 2 onward (annotations/undo/redo, then forms/pages, then
signatures/redact/textedit/image, then merge/compare each landed real —
see `wasm.ts`'s own doc comment for the task-by-task history) and never
corrected here until now. Updated to reflect what's actually still true:

- **OCR is the only unported operation**, and it's a real backend gap,
  not a "not yet" — the desktop's Tesseract-sidecar OCR has no wasm
  story. As of Task 3 the OCR button is hidden in the extension build
  (gated on `backendKind !== "wasm"`, see `+page.svelte`) rather than
  shown and throwing when clicked.
- Undo/redo **is** supported in the wasm backend (Phase 2's
  `WorkingStore` abstraction removed the filesystem-bound constraint that
  used to rule it out) — the earlier version of this bullet claiming
  otherwise was wrong as of Phase 2.
- The wasm backend's normal save path is `WasmSession::workingCopyBytes`
  (an incremental, append-only save — the same bytes the desktop
  backend's `save_document_impl` writes to disk), **not**
  `saveToBytes` (a full PDFium rewrite, kept around for merge/compare's
  own byte-plumbing needs — see `crates/openpdfedit-wasm/src/lib.rs`).
  The earlier version of this bullet warned that saving silently dropped
  any existing digital signature; that was true when `saveToBytes` *was*
  the save path, and is no longer true now that it isn't.
- `npm run e2e` covers two things, not one: `boot.spec.ts` (the packaged
  `dist/index.html` renders the editor, not SvelteKit's 404) and
  `wasm-session.spec.ts` (an in-page probe of the real `WasmSession`
  surface — open, fill a form field, mutate, undo, save-to-bytes —
  against the actual vendored PDFium wasm build, bypassing the file
  picker since Playwright can't script the real File System Access
  dialog headlessly). Neither drives the Svelte UI itself via real
  clicks — there is still no automated coverage of the actual editing UI
  end to end, only of the wasm backend surface it calls into. See
  `.superpowers/sdd/2026-08-15-extension-port-phase1/task-9-report.md`
  for what Task 9's own (static-only) verification covered, and
  `.superpowers/sdd/2026-08-10-extension-wasm-walking-skeleton/` for the
  walking skeleton's own earlier (and, for this now-retired UI, no longer
  current) manual-Chrome verification.
