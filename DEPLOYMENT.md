# OpenPdfEdit — Local Build & Deployment Guide

This is a from-scratch, run-it-yourself guide to building and testing
OpenPdfEdit locally: the Tauri desktop app and the headless
`openpdfedit-cli` binary. Every macOS command below was actually run
against this repository to write this guide — none of that is
speculative. The Windows path has been verified through GitHub Actions
(the Rust workspace and the actual `.exe` build both run there on every
push — see [§9](#9-windows-specific-notes)) but not by a human on real
Windows hardware; where that distinction matters, it's called out
explicitly rather than implied.

**Scope.** This covers *local* builds you run and test yourself, on
macOS and Windows (this project's two target platforms — see `PLAN.md`
§2). It does **not** cover code-signing/notarization or auto-update —
those are deliberately out of scope for this project so far (see
`PLAN.md`'s status log); a release build here produces an unsigned
`.dmg`/`.exe` you can run and hand to someone else to run, but Gatekeeper
and SmartScreen will both show a first-run warning for it. There's a
short note on what that means in practice under
[Building a release bundle](#6-building-a-release-bundle).

Primary instructions below are written from macOS (this repo's actual
dev environment); Windows-specific commands are given alongside them
wherever they differ, plus their own section ([§9](#9-windows-specific-notes)).

---

## 0. What you're building

- **`apps/desktop`** — the Tauri 2 + Svelte 5 desktop app (the product).
- **`apps/cli`** (binary name `openpdfedit`) — a headless companion CLI
  for batch PII redaction, merging, and document compare.
- 12 library crates under `crates/` that both consume.

Rendering goes through PDFium (via `pdfium-render`), which is *not*
vendored in git — you fetch a prebuilt dylib once (step 2). OCR is
optional and needs a locally-installed `tesseract` binary (step 8); the
app works fully without it, just without the "OCR document" button.

---

## 1. Prerequisites

| Tool | Version used to verify this guide | Why |
|---|---|---|
| Rust (`rustc`/`cargo`) | 1.97.1 | builds every crate |
| Node.js | 22.23.2 | builds the Svelte frontend |
| npm | 10.9.8 | installs frontend deps, runs the Tauri CLI |
| Xcode Command Line Tools (macOS) | any recent | native linking for Tauri's macOS webview bindings |
| `tesseract` (optional) | 5.5.3, via Homebrew | only needed to test the OCR feature |
| `cargo-deny` (optional) | 0.20.2 | only needed to reproduce the license/advisory check |

Install what's missing:

```bash
# Rust (if you don't have it): https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# macOS: Xcode Command Line Tools (one-time, if `xcode-select -p` prints nothing)
xcode-select --install

# Node.js: use whatever you already have if it's >=18; nvm/homebrew both fine
brew install node

# Optional: OCR support
brew install tesseract

# Optional: reproduce the license/advisory gate
cargo install cargo-deny
```

**Linux only** (Tauri needs these system libs — not needed on macOS/Windows):

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

**Windows only**: install the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (already present on modern Windows 10/11) and the MSVC Build Tools (Visual Studio Installer → "Desktop development with C++"). Run `scripts/fetch-pdfium.sh` from a bash shell (Git Bash works).

---

## 2. One-time setup

From the repo root (`openpdfedit/`):

```bash
# 1. Fetch the PDFium dynamic library for your platform into .vendor/pdfium/
#    (~7MB, not committed to git — see scripts/fetch-pdfium.sh's header for why).
./scripts/fetch-pdfium.sh
```

Expected output the first time:
```
fetching pdfium chromium/7961 (pdfium-mac-arm64.tgz)...
pdfium library: .../openpdfedit/.vendor/pdfium/lib/lib/libpdfium.dylib
```
(Re-running it is a fast no-op: `pdfium chromium/7961 already present ... — skipping download`.)

```bash
# 2. Install frontend dependencies
cd apps/desktop
npm install
cd ../..
```

That's it — no database, no services, no `.env` file to fill in. The app
opens PDFs you pick via a native file dialog; there's nothing else to
configure for local use.

Optional (only if you want the fuzz-corpus regression tests, not required for anything below):
```bash
./scripts/fetch-test-corpus.sh
```

---

## 3. Sanity check: run the test suite

Before running the app, confirm the toolchain and PDFium are wired up
correctly by running the real test suite (fast — a couple minutes):

```bash
cargo test --workspace 2>&1 | tail -30
```

You should see a long run of `test result: ok. N passed; 0 failed; ...`
blocks and no `FAILED` anywhere. Several tests render real PDFs through
real PDFium and OCR real text through a real `tesseract` subprocess (if
installed) — this isn't a mocked test suite.

If you want to reproduce the exact same checks used to verify every
milestone of this project:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check          # optional, only if you installed cargo-deny
```

All four should exit clean (0 warnings, 0 failures, `advisories ok,
bans ok, licenses ok, sources ok`).

**Disk space note**: a full workspace build produces a large `target/`
directory (multiple GB, mostly from the Tauri/webview dependency tree).
If you're on a constrained disk, you can point builds at a separate,
easily-wipeable location:
```bash
export CARGO_TARGET_DIR="$HOME/.cache/openpdfedit-target"
```
Safe to `rm -rf` that directory any time; the next build just regenerates it.

---

## 4. Run the desktop app in dev mode

This is the normal "make a change, see it live" loop — hot-reloads the
Svelte frontend and rebuilds the Tauri/Rust backend on change.

```bash
cd apps/desktop
npm run tauri dev
```

First run compiles the whole Rust dependency tree (Tauri + all the
`openpdfedit-*` crates) — expect a few minutes the very first time,
seconds after that. When it's ready, a native window titled "OpenPdfEdit"
opens (1100×800). Click **Open PDF…** and pick any PDF to try it —
`testdata/minimal.pdf` in this repo works as a quick smoke test.

Things to actually click through to confirm a working build:
- **Open PDF…** → picks a file, renders pages, zoom in/out works.
- Highlight/underline/strikeout/note/ink tools (top toolbar) → draw on a
  page, then **Show comments** to see them listed.
- **Show pages** → rotate/delete/move/crop a page; **Merge PDFs…** to
  combine two files.
- **OCR document** (only useful on a scanned/image-only PDF) — needs
  `tesseract` on your `PATH`; without it this returns an error, which is
  expected, not a bug.
- **Compare…** — pick a second PDF, get a text+pixel diff summary dialog.
- **Signature** (rail tool) — draw and save a signature once, then drag
  it onto any page to place it; saved signatures persist locally between
  documents.

Stop the dev server with `Ctrl+C` in the terminal.

---

## 5. Run the CLI

The CLI is a separate, PDFium-free binary — good for scripting
redaction/merge/compare without launching the GUI.

```bash
cargo run -p openpdfedit-cli -- help
```

```
openpdfedit — headless batch CLI

USAGE:
    openpdfedit redact-pii <input.pdf> <output.pdf> [--patterns email,ssn,phone,card]
        ...
    openpdfedit merge <output.pdf> <input1.pdf> <input2.pdf> [...]
        ...
    openpdfedit compare <a.pdf> <b.pdf>
        ...
```

Try it against the sample PDF in this repo:

```bash
cargo run -p openpdfedit-cli -- compare testdata/minimal.pdf testdata/minimal.pdf
# -> "... vs ... no text differences found" (comparing a file to itself)

cargo run -p openpdfedit-cli -- redact-pii testdata/minimal.pdf /tmp/redacted.pdf
# -> "redacted 0 match(es) across 1 page(s) -> /tmp/redacted.pdf" (minimal.pdf has no PII)
```

To install it as a standalone binary on your `PATH`:

```bash
cargo install --path apps/cli
openpdfedit help   # now runnable from anywhere
```

---

## 6. Building a release bundle

Both platforms have a one-command script that does the whole thing
(prerequisite checks, PDFium fetch, frontend install, release build,
copy the finished installer to your Desktop) — these are the recommended
way to build a release, and what the rest of this section explains the
internals of:

```bash
# macOS -> .dmg
./scripts/build-dmg.sh

# Windows (PowerShell) -> .exe
powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
```

Both default to a build-target directory **outside this checkout**
(`~/.cache/openpdfedit-build` / `$HOME\.cache\openpdfedit-build`) — see
the next paragraph for why that matters — and both print `--help` for
their options (`--out`, `--clean`, custom bundle target). If you'd rather
run the underlying commands yourself, or are building on Linux (no
installer target maintained there — see `ci.yml`'s comment on why), read
on.

**If your checkout lives on a network/shared mount** (as this repo's dev
copy does, under `/Volumes/My Shared Files/...`) **you must point
`CARGO_TARGET_DIR` at genuinely local disk first**, or the release build
fails partway through with a confusing, unrelated-looking error:
```
error: failed to build archive at `.../liburl-....rlib`: failed to map object file: memory map must have a non-zero length
```
This isn't a real dependency/code problem — it's the shared mount
mishandling `mmap`'d intermediate build artifacts under load, and it's
reproducible: it happened while writing this guide, on the exact command
below, and went away immediately once `CARGO_TARGET_DIR` pointed
somewhere off the mount. A subfolder *of* the same mount doesn't fix it —
it has to be a different physical volume (`/tmp` and anywhere under your
home directory both work if they're on the Mac's internal disk). Both
build scripts already do this for you by default; the caveat only
applies if you run `npm run tauri build` directly.

```bash
cd apps/desktop
export CARGO_TARGET_DIR=/tmp/openpdfedit-release-target   # skip this line if your checkout is already on local disk
npm run tauri build -- --bundles dmg     # macOS: produces a .dmg
npm run tauri build -- --bundles nsis    # Windows: produces a *-setup.exe
```

Expect ~2 minutes for the Rust release compile (plus the frontend build,
seconds) the first time; incremental reruns are much faster. On success
(macOS shown; Windows' `nsis` step is the direct equivalent):

```
    Bundling OpenPdfEdit.app (.../release/bundle/macos/OpenPdfEdit.app)
    Bundling OpenPdfEdit_0.1.0_aarch64.dmg (.../release/bundle/dmg/OpenPdfEdit_0.1.0_aarch64.dmg)
    Finished 2 bundles at: ...
```

(Bundle paths land under `$CARGO_TARGET_DIR/release/bundle/` if you set
that env var, or the default `apps/desktop/src-tauri/target/release/bundle/`
if you didn't need to.)

**Installing and running the unsigned build locally**: this project
doesn't have a code-signing certificate or notarization set up (see
[scope](#scope) above) on either platform, so both show a first-run
warning — expected, not a sign of a broken build.

On **macOS**, a `.dmg`/`.app` that arrived via Finder/Safari/AirDrop will
carry a quarantine flag and Gatekeeper will refuse to open it with a
normal double-click ("OpenPdfEdit is damaged and can't be opened" or
"cannot verify developer") — a `.dmg` you build yourself and move around
with `cp`/`hdiutil` on the command line generally does *not* pick up that
flag, but if you hit the Gatekeeper dialog, this fixes it for a binary
you trust (you just built it):

```bash
hdiutil attach OpenPdfEdit_0.1.0_aarch64.dmg -nobrowse -mountpoint /tmp/openpdfedit-dmg-mount
cp -R /tmp/openpdfedit-dmg-mount/OpenPdfEdit.app /Applications/OpenPdfEdit.app
hdiutil detach /tmp/openpdfedit-dmg-mount
xattr -cr /Applications/OpenPdfEdit.app   # only if Gatekeeper complains
open /Applications/OpenPdfEdit.app
```

Confirmed working end-to-end on this machine: the installed app launches,
registers a real window (WebKit's Networking/GPU/WebContent helper
processes all spawn — that only happens once a live webview is actually
loading), stays up with no crash report, and quits cleanly. This
environment has no screen-capture access (see PLAN.md's "known
verification gap"), so click-through UI testing of what's *inside* the
window is still on you — this confirms the app itself launches and runs,
not what every button does.

On **Windows**, running the `-setup.exe` will likely trigger a SmartScreen
warning ("Windows protected your PC") the first time, for the same
reason (no publisher certificate) — click **More info**, then **Run
anyway**. This repo has no Windows machine available to verify the
installed app actually launches end-to-end the way the macOS install was
verified above; see [§9](#9-windows-specific-notes) for what *has* and
hasn't been checked.

The release binary for the CLI lands at
`target/release/openpdfedit` (`openpdfedit.exe` on Windows), or under
`$CARGO_TARGET_DIR/release/` if you set that env var, after
`cargo build --release -p openpdfedit-cli` (same local-disk
`CARGO_TARGET_DIR` caveat applies if you're on the shared mount).

---

## 7. PDFium at runtime

Dev builds (`npm run tauri dev`, `cargo test`, `cargo run`) find PDFium
automatically via `.vendor/pdfium/` (step 2) — the dylib/so lands under
`.vendor/pdfium/lib/`, the Windows dll under `.vendor/pdfium/bin/`
(`pdfium-binaries`' own layout convention: runtime DLL in `bin/`, the
MSVC import stub in `lib/` — `scripts/fetch-pdfium.sh` and the
dev-mode lookup code both know the difference).

A release bundle **does** package the PDFium library into the app itself
— confirmed by installing and launching the built `.app` from a location
outside this checkout (§6) — via `tauri.conf.json`'s `bundle.resources`.
That mapping is platform-specific (a different filename and source path
per OS), so it lives in **per-platform override files** rather than the
shared config: `tauri.macos.conf.json` and `tauri.windows.conf.json`,
each merged automatically over `tauri.conf.json` by the Tauri CLI based
on the host OS you're building on (Tauri's own JSON-Merge-Patch
mechanism — no `--target` flag or extra step needed). At runtime, the
app looks up its bundled resource directory and checks for whichever of
`libpdfium.dylib` / `libpdfium.so` / `pdfium.dll` is present — see
`bundled_pdfium_dir()` in `apps/desktop/src-tauri/src/lib.rs`.

---

## 8. Testing OCR specifically

OCR shells out to a `tesseract` binary rather than linking it — confirm
it's on `PATH` first:

```bash
tesseract --version
```

If that's missing, `brew install tesseract` (macOS) or your distro's
package (`apt install tesseract-ocr` on Linux) — only `eng`/`osd`/`snum`
language data is required (Homebrew's default install already includes
these). Then in the running app: open a scanned/image-only PDF, click
**OCR document**, and its text becomes selectable/searchable afterward.
On Windows, install Tesseract from
[UB Mannheim's build](https://github.com/UB-Mannheim/tesseract/wiki) —
the app's own error message names this exact link if it can't find
`tesseract` (it searches `PATH`, then the installer's default location).

---

## 9. Windows-specific notes

Windows and macOS are this project's two target platforms (`PLAN.md`
§2) — Linux is deliberately not maintained as an installer target (see
`ci.yml`'s comment on `openpdfedit-installer`). What's true for Windows,
stated plainly rather than assumed:

**Continuously verified, on every push, via GitHub Actions
(`windows-latest` runners):**
- The whole Rust workspace — `cargo fmt`, `clippy -D warnings`, and the
  full `cargo test --workspace` suite — compiles and passes.
- The full release bundle (`tauri build --bundles nsis`) actually
  produces a `*-setup.exe`, uploaded as a downloadable CI artifact (see
  the Actions tab on any run — the `openpdfedit-installer` job's
  `openpdfedit-windows-latest` artifact). **This is the easiest way to
  get a Windows build without owning Windows hardware.**

**Not yet verified, by anyone, because no Windows machine has run this
app:** whether the installed `.exe` actually launches, opens a PDF
correctly, and renders it — the equivalent of the manual install+launch
check §6 walks through and confirms on macOS. CI proves the *build*
succeeds; it doesn't click the installed shortcut. If you're the first
person to try this on real Windows hardware, that's the gap worth
checking, and the app's behavior on a genuine "double-click, does
nothing happen" failure is the same class of bug documented for macOS in
`apps/desktop/src-tauri/src/lib.rs`'s `bundled_pdfium_dir()` doc comment
— PDFium failing to load before a window ever shows.

**If `scripts/build-installer.ps1` (or a bare `tauri build`) fails at the
`nsis`/bundling step specifically** (i.e. the Rust compile already
succeeded — you saw `Finished release profile`, and the failure is in a
later "Bundling..." step): this project doesn't yet have a documented
fix for that class of failure the way `scripts/build-dmg.sh` does for
macOS's `bundle_dmg.sh` (that fix exists because a real user hit it and
reported the exact error; nobody has reported a Windows equivalent yet).
Capture the full output and the exact error text — that's what turns a
guess into a real fix.

---

## Troubleshooting

- **`failed to load PDFium` on `npm run tauri dev` / `cargo test`** —
  you skipped step 2. Run `./scripts/fetch-pdfium.sh` from the repo root.
- **Port 1420 already in use** — another `npm run tauri dev` / `vite
  dev` is already running somewhere; stop it, or edit
  `apps/desktop/vite.config.js`'s dev server port (and the matching
  `devUrl` in `apps/desktop/src-tauri/tauri.conf.json`).
- **`No space left on device` mid-build** — the Rust build tree gets
  large. `rm -rf target` (or your `$CARGO_TARGET_DIR`) and rebuild; it's
  fully regenerable.
- **`OCR document` errors immediately** — `tesseract` isn't on `PATH`;
  see [step 8](#8-testing-ocr-specifically). This is a real, expected
  error, not a crash.
- **macOS says the built `.app` "is damaged"** — see the Gatekeeper note
  in [step 6](#6-building-a-release-bundle); this is expected for an
  unsigned local build, not a broken build.
- **Windows SmartScreen blocks the installed `.exe`** — expected for the
  same reason (no code-signing certificate); see [step 6](#6-building-a-release-bundle).
- **`build-dmg.sh`/`build-installer.ps1` fails at the packaging step, not
  the compile step** (you saw `Finished release profile`, then a later
  "Bundling..."/"Running bundle_dmg.sh" failure) — `build-dmg.sh` has an
  automatic diagnosis-and-retry for this on macOS (Finder automation
  permission or a stale mounted disk image, almost always); see
  [§9](#9-windows-specific-notes) for the Windows equivalent, which is
  less documented since it hasn't been hit by a real user yet.
- **A signature badge says "unverified"** — expected, always, for every
  PDF: this app deliberately does not implement cryptographic signature
  verification yet (see `PLAN.md`); it only reports what a signature
  *claims* about itself.

---

## What's intentionally not here

Straight from `PLAN.md`'s status log, so you don't go looking for it:
encryption (`lopdf` bug made it unsafe to ship), cryptographic signature
verification, PAdES signing, PDF/A convert/validate, and the parts of
release engineering that specifically require a paid certificate/service
— code signing, notarization, auto-update. (Installers themselves —
`.dmg` and `.exe` — *are* here now; see §6.) All are documented with the
specific reason they were deferred rather than shipped half-working.
