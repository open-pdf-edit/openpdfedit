# Research Report 5 — A local web app, running the same WASM core

*Assessed 2026-08-24, against the code at `977f959`. Every number below
was measured in this repo, not estimated.*

**Verdict up front: yes, and it is small. The web app already exists in
all but its shell — the Chrome extension *is* this SPA compiled against
the WASM backend. Making it a plain web page is one more build target,
not a second product. The real work is browser portability (~2 days),
not capability.**

## 1. How much already works

`Backend` (`apps/desktop/src/lib/backend/types.ts`) is the whole
contract between the UI and everything below it. Both implementations
are complete:

| | methods |
|---|---|
| declared in `types.ts` | 46 |
| implemented in `wasm.ts` | 46 |
| **genuinely unavailable in the browser** | **1** — `ocrDocument` |

Everything else — annotations, page operations, redaction, text and
image editing, forms, signatures, watermarking, compression, comparison,
search, outlines, flatten, XFDF, numbering, encryption — runs in
WebAssembly today, through the same `openpdfedit-session` code the
desktop uses.

The shared UI is already shell-agnostic: `grep -rn "chrome\.\|browser\."
apps/desktop/src` returns nothing. Only `apps/extension/background.ts`
touches extension APIs (`chrome.action`, `chrome.runtime`,
`chrome.tabs`), and a web app has no use for that file.

The build works standalone right now:

```sh
cd apps/desktop && VITE_BACKEND=wasm npm run build   # succeeds
```

The output bundles the WASM backend and tree-shakes `@tauri-apps/api`
out entirely (no match for it anywhere in `build/app/immutable`). That
is the web app's front end, already building, with no extension in the
picture.

## 2. What it would weigh

Measured from the shipped extension bundle and a wasm-mode SPA build:

| | raw | gzipped |
|---|---|---|
| `pdfium.wasm` | 5.2 MB | 2.3 MB |
| `openpdfedit_wasm_bg.wasm` | 4.0 MB | 0.7 MB |
| app JS + CSS | 528 KB | ~150 KB |
| **fonts (24 files)** | **3.9 MB** | — |

Two observations.

**The fonts are the problem, not the WASM.** 3.9 MB of Geist across 24
weights, against 528 KB of actual application. The app uses a handful.
Subsetting to the weights in use and serving `woff2` takes this to well
under 300 KB — a bigger saving than anything available on the WASM side,
and it helps the extension too.

**~3.2 MB gzipped for the engine is acceptable** for a full PDF editor,
and it is a one-time cost: WASM caches, and `WebAssembly.instantiate-
Streaming` compiles during download. Brotli would cut it further. For
comparison, that is roughly one large hero video.

## 3. What actually needs building

Ordered by how much it matters.

### 3.1 File System Access fallbacks — the only real portability work

`wasm.ts` uses `showOpenFilePicker`/`showSaveFilePicker` (7 call sites)
and `FileSystemFileHandle` (22 references), with no capability check.
Those are Chromium-only: Firefox and Safari have neither.

In an *extension* that is fine — it only ever runs in Chrome. In a web
app it decides whether two thirds of browsers can open a file at all.

The fallback is well-trodden: `<input type="file">` for opening, and a
`Blob` + `<a download>` for saving (the same approach `exportXfdf` and
`encryptDocument` already take in `wasm.ts`). The cost is that saving
becomes "download a copy" rather than "write back to the file you
opened" — worth surfacing honestly in the UI rather than silently
degrading.

### 3.2 A web app shell

`apps/extension/scripts/build-spa.sh` is nearly it already. A web app
build is that script minus two extension-specific steps:

- `externalize-inline.mjs`, which exists solely because MV3's CSP
  forbids inline `<script>`. A web page has no such restriction.
- the `background.js` entry and `manifest.json`.

Plus `copy-vendor.sh` to bring in `pdfium.js`/`pdfium.wasm` and the
`wasm-gen` glue, unchanged.

### 3.3 A service worker

There is none anywhere in the repo today. Without one the app needs the
network for its first load of every session — which undercuts the
central claim. With one, it installs once and works on a plane, and the
"nothing is uploaded" promise becomes literally demonstrable: load it,
go offline, keep editing.

### 3.4 OCR — the one real gap

`ocrDocument` shells out to a local Tesseract binary. A browser cannot.

Three options, in order of preference:

1. **Lazy-load `tesseract.js`** on first use. It is WASM Tesseract, and
   the language data (~15 MB for English) downloads only if the user
   asks for OCR. That keeps the base bundle honest.
2. **Hide the tool** in the web build, as it is hidden in the extension
   today (`{#if backendKind !== "wasm"}`), and point at the desktop app.
3. Server-side OCR — **rejected**, and worth stating plainly: it would
   mean uploading the document, which contradicts the one promise this
   product is built on.

## 4. What a web app gets that the desktop doesn't

**Print, cheaply.** The desktop needed a platform split (CUPS on
macOS/Linux, the shell verb on Windows) because WKWebView has never
implemented `window.print()`. A browser has, so print in a web app is a
print stylesheet and one call — easier there than it was natively.

**No install friction at all.** This is the strategic point. The market
research identifies "try it without committing" as the top of the
funnel, and the closest comparable product (BentoPDF, the reference for
the new site) is a web app precisely for that reason. A "try it in your
browser" link next to the download buttons converts visitors the
installers never reach.

## 5. Recommendation

Build it, as a fourth target of the existing SPA — `apps/webapp`
alongside `apps/extension`, sharing `apps/desktop/src` exactly as the
extension already does. Do **not** fork the UI.

Rough sequencing, with the portability work first because it is the only
part that can surprise:

| | work |
|---|---|
| 1 | File System Access fallbacks in `wasm.ts`, behind capability checks |
| 2 | `apps/webapp` shell + build script (adapted from `build-spa.sh`) |
| 3 | Font subsetting — the single biggest size win, and it helps the extension too |
| 4 | Service worker for offline |
| 5 | Decide OCR: lazy `tesseract.js`, or hide and point at the desktop |
| 6 | Print, which the desktop had to work for and the browser gives away |

Steps 1–4 are the minimum for something publishable. Nothing here is
research; it is all known work.

### The one thing to decide first

**Does saving write back, or download a copy?** In Chromium the File
System Access API can genuinely save over the file the user opened. In
Firefox and Safari it cannot — only a download. Supporting both means
two save behaviours in one product, which is a UX decision (and a
support-load decision) rather than a technical one. Settle it before
step 1, because it shapes the fallback design.
