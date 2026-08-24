# OpenPdfEdit

A fast, local-first PDF editor with a Rust core — as a desktop app
(macOS/Windows, Tauri + Svelte) and a Chrome extension (the same UI on a
WebAssembly build of the engine). Documents are rendered, edited, and
saved entirely on your machine; nothing is uploaded.

**Features:** view (tiled rendering, zoom/scroll) · annotate (highlight,
underline, strikeout, notes, ink, shapes) with comment threads · undo/redo ·
incremental save (original bytes preserved; existing digital signatures
survive annotation edits) · AcroForm fill **and** field creation · page
organization (rotate, delete, reorder, crop, extract, merge) · redaction
(true content removal) · signature list/placement · text-run and image
editing · watermark (tiled text/logo stamps, 0°/45°, opacity, band or
full-page) · document compare (text + pixel diff) · OCR (desktop only,
via a local tesseract) · optional OpenApps account panel (sign-in,
credits — never required, no network without it).

## Downloads

Prebuilt artifacts are attached to each
[GitHub Release](https://github.com/open-pdf-edit/OpenPdfEdit/releases):
the macOS app (`.dmg`, Apple Silicon), the Windows installer
(`…-setup.exe`), and the Chrome extension (`openpdfedit-dist.zip`).
They're built by [`.github/workflows/release.yml`](.github/workflows/release.yml)
on every `v*` tag.

### Opening the macOS app the first time

The `.dmg` is signed, but not *notarized* — notarization needs a paid
Apple Developer account. So the first launch shows **"Apple could not
verify OpenPdfEdit is free of malware."** To get past it once:

**System Settings → Privacy & Security →** scroll to the message about
OpenPdfEdit **→ Open Anyway.** Or, from a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OpenPdfEdit.app
```

If you instead see **"OpenPdfEdit is damaged and can't be opened"**, you
have v0.1.2 or earlier. That build shipped with a broken signature — the
app bundle was never sealed — and macOS reports a broken signature as
"damaged". The command above clears it, and releases after v0.1.2 are
fixed properly.

## Test the Chrome extension (fastest path)

A ready-to-load build is committed at
[`apps/extension/openpdfedit-dist.zip`](apps/extension/openpdfedit-dist.zip):

1. Unzip it somewhere.
2. Chrome → `chrome://extensions` → enable **Developer mode** →
   **Load unpacked** → pick the unzipped folder.
3. Click the OpenPdfEdit icon and open a PDF.

Requires Chrome 103+. Rebuilding it yourself: see
[`apps/extension/README.md`](apps/extension/README.md) (needs Rust with
the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` pinned to the
version in `Cargo.toml`; `npm run package` reproduces the zip).

## Build the desktop app

Prerequisites: Rust (stable), Node 20+, and the platform's Tauri
prerequisites (<https://tauri.app/start/prerequisites/>).

```sh
scripts/fetch-pdfium.sh          # downloads the PDFium binary for your OS
cd apps/desktop
npm install
npm run tauri dev                # run it
```

Installers: `scripts/build-dmg.sh` (macOS .dmg),
`scripts/build-installer.ps1` (Windows).

## Tests

```sh
cargo test --workspace           # Rust suites (uses .vendor/pdfium)
cd apps/extension && npm run e2e # Playwright against the packaged build
```

## Repository notes

- `apps/desktop/vendor/openapps/` holds prebuilt copies of
  `@openapps/sdk` and `@openapps/ui` (the OpenApps account/credits
  components), vendored so this repository installs standalone. They
  live under `apps/desktop` deliberately: Vite resolves their bare
  imports (`lit`) by walking up from the *real* file path, and only
  there does the walk-up reach `apps/desktop/node_modules`. Their
  source lives in the OpenApps monorepo.
- `site/` is the static marketing page (no build step — open
  `site/index.html`).
- `docs/` carries the design/research notes and implementation plans the
  project was built against; `PLAN.md` is the live milestone log.

## License

MIT OR Apache-2.0, at your option — see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
