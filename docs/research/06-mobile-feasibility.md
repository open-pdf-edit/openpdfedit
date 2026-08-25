# Android and iOS: what it would take

Written 2026-08-25, against the code as it stands. Everything measured
here was measured, not estimated — where something is a guess it says so.

## The short version

The engine is ready and the interface is not. Three things that usually
sink a mobile port are already done: the Rust core compiles for
non-desktop targets and already runs on one (WebAssembly), PDFium
publishes the binaries needed for both platforms, and the whole UI was
built on pointer events, so touch works today with no conversion.

What is missing is a phone-shaped interface. Opened on an iPhone 13 the
editor puts **24 buttons in a toolbar 430px wide**, wraps them over the
path bar, runs a second row off the right edge, and gives a 56px tool
rail out of the same 430 — leaving a sliver for the document. Nothing is
broken; it is a desktop layout on a phone.

That is the honest split: the hard, expensive part is finished, and the
remaining work is design, not engineering risk.

## What already works

| | |
|---|---|
| Rust core on a non-desktop target | Yes — wasm32 ships today |
| PDFium for arm64 | Published: `pdfium-android-arm64`, `pdfium-ios-device-arm64`, `pdfium-ios-simulator-arm64` |
| Touch input | Already pointer events (`onpointerdown/move/up/cancel`) — no mouse handlers anywhere in the UI |
| Tauri version | 2, which supports iOS and Android |
| Native plugin surface | Two: `tauri-plugin-dialog`, `tauri-plugin-opener`. Both support mobile |
| No-filesystem operation | Already solved for the browser build — the whole bytes-based path exists |
| Viewport | `width=device-width` is set; the empty state fits a phone with no horizontal overflow |

The last one deserves emphasis. Porting a desktop app to mobile usually
means discovering that the core assumes a filesystem it will not have.
That discovery already happened here, for the web build, and produced
`open_document_bytes` and the `WorkingStore` abstraction. A mobile build
inherits the answer.

## Two routes, and they are not equally expensive

### Route A — ship the web app as an installable PWA

Cost: **near zero.** It already runs on a phone browser today. There is a
`manifest.webmanifest`, a service worker, and offline support. Adding
Android/iOS install prompts and testing is days, not months.

What the user gets: every tool, working offline, no store review, no
signing, instant updates. What they do not get: a home-screen app store
listing, and on iOS the usual PWA limits.

Saving is the honest caveat. The File System Access API does not exist on
mobile browsers, so saving is a download rather than a write-back — but
that path is already built and already labels itself correctly (see
`Backend.savesByDownloading`).

**This is the cheapest real mobile version and it is one layout away.**

### Route B — native apps via Tauri mobile

Cost: **substantial, but bounded.** `tauri android init` / `tauri ios
init` generate the shells; the work is everything around them.

Known work, in rough order of size:

1. **The phone layout** — the same work Route A needs, and the larger
   half of either. See below.
2. **PDFium for arm64.** `scripts/fetch-pdfium.sh` covers desktop only
   (`x86_64`, `aarch64`, linux/win/mac). It needs android/ios assets
   added and bundling into each app package. The upstream releases exist,
   so this is plumbing.
3. **File access.** `tauri-plugin-dialog` works on mobile, but Android
   returns `content://` URIs rather than paths, and the desktop backend
   is path-based. The bytes-based backend already sidesteps this — the
   likely answer is that mobile uses the byte path, not the desktop one.
4. **OCR.** The subprocess recogniser is gated `not(target_arch =
   "wasm32")`, so on Android/iOS it *compiles* and then finds no
   `tesseract` binary at runtime. Mobile should use the tesseract.js path
   the browser build uses — the split already exists, the cfg just needs
   to name mobile too.
5. **Sign-in.** `AccountPanel` opens a second `WebviewWindow`, which
   mobile has no equivalent for. The browser build's approach — a tab,
   with the session handed back through shared storage — is what mobile
   needs, and it is already written.
6. **Store logistics.** Apple Developer membership, signing,
   notarisation, review; Play Console, signing, review. This is
   paperwork, but it is the part that cannot be compressed and it is
   where a first submission usually loses a week.

Nothing on that list is research. Every item has a known answer, and
four of the six already have working code elsewhere in this repository.

## The interface is the actual project

Both routes need it, so it should be costed once.

Measured on an iPhone 13 (390pt) and a Pixel 5 (393pt), with a document
open at 430pt:

- 24 buttons in the topbar, wrapping onto the path bar and clipping
  "100%" and "Page 1 of 1"
- a second toolbar row of document tools running past the right edge
- a 56px permanent tool rail
- the page itself reduced to a sliver

What a phone build needs is not a narrower version of this. It is a
different information architecture: one primary surface (the page), tools
behind a sheet or a bottom bar, panels as full-screen modals rather than
side-by-side, and a much smaller default tool set with the rest a tap
away. That is a design exercise before it is an implementation one.

Two things make it cheaper than it sounds. The UI is one Svelte codebase
shared by three builds already, so a fourth layout is a variant rather
than a rewrite. And the `Backend` interface means none of it touches
document logic.

## Recommendation

**Do Route A first, and treat the layout as the deliverable.** It turns
the existing web app into a usable phone product for the cost of the
design work alone, with no store review, no signing, and no second
distribution channel to maintain. It also de-risks Route B completely:
if the phone layout is good, the native shells are mechanical; if it is
not, that is worth discovering before paying Apple $99 and waiting on
review.

Route B is then a decision about distribution and native integration
(share sheets, "Open in…", document providers), not about whether the
product can work on a phone.

## What was not checked

- Neither Rust core has been **compiled** for `aarch64-linux-android` or
  `aarch64-apple-ios`. Nothing in the code looks platform-locked beyond
  the OCR subprocess, but "looks fine" is not "builds".
- PDFium's mobile binaries are published; they have not been run against
  this engine.
- No device or emulator was involved in any of this. The measurements are
  Chromium emulating iPhone 13 and Pixel 5 viewports, which is the right
  tool for layout and the wrong one for performance, memory, and how a
  real PDF feels under a finger.
