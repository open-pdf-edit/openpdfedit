# OpenPdfEdit on the web

The same editor as the desktop app and the Chrome extension, served as an
ordinary web page: the Rust core compiled to WebAssembly, running
entirely in the browser. Nothing is uploaded — there is no server to
upload to.

## Build

```sh
npm --prefix apps/webapp run build     # -> apps/webapp/dist
npm --prefix apps/webapp run preview   # http://localhost:8081
```

Serve it over http, not `file://` — a service worker needs an origin.
The output is static: any file host will do.

## What this directory contains

Almost nothing, deliberately. The UI is `apps/desktop/src`, shared
verbatim with the other two builds; the WebAssembly build and the
vendored PDFium come from `apps/extension/scripts`, reused rather than
duplicated so the two can't drift. What's here is only what a *web page*
needs and the other targets don't:

| | |
|---|---|
| `scripts/build.sh` | assembles the SPA + wasm runtime into `dist/` |
| `service-worker.js` | offline support |
| `manifest.webmanifest` | installable-as-an-app metadata |

It is the extension's build minus its two extension-only steps: the MV3
CSP workaround (`externalize-inline.mjs`), and `background.js` /
`manifest.json`.

## Two ways this differs from the desktop app

**Saving.** Where the browser supports the File System Access API
(Chromium today), Save writes back over the file you opened, exactly
like the desktop. Where it doesn't (Firefox, Safari), a document opened
through the file input can only be saved as a **download** — there is no
API to write back to where it came from. The Save control says which one
it's about to do rather than promising a save it can't perform; see
`Backend.savesByDownloading`.

**OCR is unavailable.** It shells out to a local Tesseract binary, which
a browser cannot do. The tool is hidden in this build. Running Tesseract
in WebAssembly is possible (`tesseract.js`) and would be a lazy-loaded
addition; doing it server-side is not an option, because it would mean
uploading the document.

Everything else — annotation, redaction, text and image editing, forms,
signing, page operations, watermarks, numbering, encryption, search,
compare, flatten, XFDF — is the same code as the desktop, and runs here.

## Size

| | |
|---|---|
| `pdfium.wasm` | 5.0 MB (≈2.3 MB gzipped) |
| `openpdfedit_wasm_bg.wasm` | 4.2 MB (≈0.7 MB gzipped) |
| app JS + CSS | 528 KB |
| fonts | 276 KB |

Serve it with compression: the two WebAssembly binaries are most of the
download and compress to roughly a third. They are cached after first
load, and the service worker means a repeat visit needs the network for
nothing at all.
