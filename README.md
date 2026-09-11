# OpenPdfEdit

A fast, local-first PDF editor with a Rust core. One engine, one
interface, six ways to run it: in a browser, as a desktop app, as a
browser extension, on iOS, from a shell, or over MCP. Documents are
rendered, edited, and saved entirely on your machine; nothing is
uploaded.

The interface speaks eight languages: English, 简体中文, 繁體中文, 日本語,
한국어, Deutsch, Español and Português. Pick one from the globe in the
toolbar; the choice is remembered.

**Features:** view (tiled rendering, zoom/scroll) · annotate (highlight,
underline, strikeout, notes, ink, shapes) with comment threads · undo/redo ·
incremental save (original bytes preserved; existing digital signatures
survive annotation edits) · AcroForm fill **and** field creation · page
organization (rotate, delete, reorder, crop, extract, merge) · redaction
(true content removal) · signature list/placement · text-run and image
editing · watermark (tiled text/logo stamps, 0°/45°, opacity, band or
full-page) · document compare (text + pixel diff) · OCR (a local
tesseract on the desktop, tesseract.js in the browser) · optional
OpenApps account panel (sign-in, credits — never required, no network
without it).

## Try it without building anything

Open [openpdfedit.com/app](https://openpdfedit.com/app). That is the
whole of it — same engine, same interface, nothing installed.

## Platforms

| Where | How to get it | State |
|---|---|---|
| **Web** | [openpdfedit.com/app](https://openpdfedit.com/app) | Live, and always the newest build — nothing to install or update |
| **macOS** | `OpenPdfEdit_<version>_aarch64.dmg` | Apple Silicon. Ad-hoc signed; see below for the first-run warning |
| **Windows** | `OpenPdfEdit_<version>_x64-setup.exe` | x64 installer |
| **Windows (Store)** | Microsoft Store | An MSIX, built by [`msix.yml`](.github/workflows/msix.yml). A separate product identity from the `.exe` — the two install side by side |
| **Browser extension** | Edge Add-ons, or `openpdfedit-dist.zip` loaded unpacked | Chrome 103+, Edge and other Chromium browsers. Not Firefox: MV3 differs enough to need its own build |
| **iOS** | — | Built and runnable from Xcode; not yet on the App Store |
| **CLI** | `apps/cli` | Headless batch operations, no browser and no PDFium |
| **MCP** | `apps/mcp` | Exposes the engine to an MCP client |

The web app is the one to reach for first. It is the same Svelte
interface on the same WebAssembly engine as the extension, it needs no
install, and it cannot fall behind — which the desktop app can, and
does, if nobody downloads a new build.

What the desktop app adds over the web app: native file dialogs, and an
OCR path that shells out to a local tesseract instead of running
tesseract.js in a worker. Everything after recognition is the same Rust
either way.

## Releases

Artifacts are attached to each
[GitHub Release](https://github.com/open-pdf-edit/OpenPdfEdit/releases),
built by [`release.yml`](.github/workflows/release.yml) on every `v*`
tag. A tag with a hyphen in it (`v0.1.11-rc3`) is a release candidate —
the same artifacts, marked pre-release. Nothing reaches a store from a
tag either way: both store workflows are `workflow_dispatch` only, so
publishing is always a deliberate act.

| | Latest |
|---|---|
| Stable | [`v0.1.10`](https://github.com/open-pdf-edit/OpenPdfEdit/releases/tag/v0.1.10) |
| Pre-release | [`v0.1.11-rc3`](https://github.com/open-pdf-edit/OpenPdfEdit/releases/tag/v0.1.11-rc3) — the eight languages, and in-app updates |

**Releases from `v0.1.11-rc2` update themselves.** The macOS and Windows
builds check for a new version on launch and offer to install it, which
is what the `latest.json` and `.sig` assets in a release are for — a
release without them cannot be offered as an update. `v0.1.10` and
earlier have to be downloaded by hand, and so does `rc1`.

### Opening the macOS app the first time

The `.dmg` carries an *ad-hoc* signature — a real signature, but not one
tied to an Apple Developer ID, and not notarized. Both of those need a
paid Apple Developer account. The ad-hoc signature is what stops macOS
calling the download **"damaged"**; what it can't stop is the weaker
warning, **"Apple could not verify OpenPdfEdit is free of malware."** To
get past that once:

**System Settings → Privacy & Security →** scroll to the message about
OpenPdfEdit **→ Open Anyway.** Or, from a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OpenPdfEdit.app
```

To remove the warning entirely, set `APPLE_CERTIFICATE`,
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
`APPLE_PASSWORD` and `APPLE_TEAM_ID` as repository secrets — the release
workflow already picks them up and notarizes when they're all present,
and falls back to ad-hoc signing when they aren't.

If you instead see **"OpenPdfEdit is damaged and can't be opened"**, you
have v0.1.2 or earlier. That build shipped with a broken signature — the
app bundle was never sealed — and macOS reports a broken signature as
"damaged". The command above clears it, and releases after v0.1.2 are
fixed properly.

## Load the extension (fastest local path)

A ready-to-load build is committed at
[`apps/extension/openpdfedit-dist.zip`](apps/extension/openpdfedit-dist.zip):

1. Unzip it somewhere.
2. `chrome://extensions` (or `edge://extensions`) → enable **Developer
   mode** → **Load unpacked** → pick the unzipped folder.
3. Click the OpenPdfEdit icon and open a PDF.

Requires Chrome 103+ or any Chromium browser of that vintage — Edge and
Brave included. Rebuilding it yourself: see
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

## Build the web app

```sh
cd apps/webapp && npm run build   # -> apps/webapp/dist
```

It is the desktop Svelte app built with `BASE_PATH=/app` and the
WebAssembly backend, which is why there is no separate source tree for
it. `scripts/deploy-webapp.sh` ships that `dist` and then checks what
the origin is actually serving, rather than trusting that rsync exited
zero.

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
