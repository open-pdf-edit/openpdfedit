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
full-page) · document compare (text + pixel diff) · OCR (tesseract.js,
in the page, twelve languages) · optional OpenApps account panel.

**Two of those are paid.** Watermark and OCR are the Supporter tools:
1,000 credits once, for both together, and that is the only charge
anywhere in the product. Signing in is needed for those two and for
nothing else — everything else on the list is free, with no account.

**What crosses the network.** Nothing you open does, with or without an
account: every page is rendered, edited and saved on your machine. The
one exception is the OCR engine itself. In the web app it is fetched on
first use of OCR — the WebAssembly core and the one language you asked
for, a few megabytes, from openpdfedit.com, the same origin that served
the app — and recognition is offline from then on. The desktop and iOS
builds carry those files inside the bundle and fetch nothing at all, and
the extension ships without OCR.

## Try it without building anything

Open [openpdfedit.com/app](https://openpdfedit.com/app). That is the
whole of it — same engine, same interface, nothing installed.

## Platforms

| Where | How to get it | State |
|---|---|---|
| **Web** | [openpdfedit.com/app](https://openpdfedit.com/app) | Live, and always the newest build — nothing to install or update |
| **macOS** | `OpenPdfEdit_<version>_aarch64.dmg` | Apple Silicon. Ad-hoc signed; see below for the first-run warning |
| **Windows** | `OpenPdfEdit_<version>_x64-setup.exe` | x64 installer |
| **Windows (Store)** | — | An MSIX is built by [`msix.yml`](.github/workflows/msix.yml), with a separate product identity from the `.exe` so the two install side by side. Not submitted to the Microsoft Store yet |
| **Browser extension** | `openpdfedit-dist.zip`, loaded unpacked | Chrome 103+, Edge and other Chromium browsers. Not on a store yet: [`publish-edge.yml`](.github/workflows/publish-edge.yml) can only replace the package of a product that already exists, and the first submission has to be made by hand. Not Firefox: MV3 differs enough to need its own build |
| **iOS** | — | Built and runnable from Xcode; not yet on the App Store |
| **CLI** | `apps/cli` | Headless batch operations, no browser and no PDFium |
| **MCP** | `apps/mcp` | Exposes the engine to an MCP client |

The web app is the one to reach for first. It is the same Svelte
interface on the same WebAssembly engine as the extension, it needs no
install, and it cannot fall behind — which the desktop app can, and
does, if nobody downloads a new build.

What the desktop app adds over the web app: native file dialogs, and
saving over the original — with a recents list that survives a relaunch
— always, where the web app can only do either in the Chromium browsers
that implement the File System Access API.

OCR is no longer one of those differences. It used to shell out to a
`tesseract` binary found on the customer's PATH, so it ran only for
people who had installed one themselves and could never run inside the
Mac App Store's sandbox. Both now recognise with tesseract.js in the
page, so OCR works everywhere, and everything after recognition was
always the same Rust. What changed is the engine and where it runs, not
what it costs: OCR is still a Supporter tool, with watermark.

## Releases

Artifacts are attached to each
[GitHub Release](https://github.com/open-pdf-edit/openpdfedit/releases),
built by [`release.yml`](.github/workflows/release.yml) on every `v*`
tag; the newest is always at
[`/releases/latest`](https://github.com/open-pdf-edit/openpdfedit/releases/latest).
A tag with a hyphen in it (`v1.0.1-rc1`) is a release candidate — the
same artifacts, marked pre-release. Nothing reaches a store from a tag
either way: both store workflows are `workflow_dispatch` only, so
publishing is always a deliberate act.

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
- The marketing site is **not in this repository**. It lives in
  `openpdfedit-website`, which is private: this repo is the app, and the
  website — the page, the privacy policy, the SEO metadata, the deploy
  scripts — is not part of the open-source product. The app still builds
  from here into `/app/`, and `scripts/deploy-webapp.sh` ships it.
- `docs/` carries the design/research notes and implementation plans the
  project was built against; `PLAN.md` is the live milestone log.

## License

[AGPL-3.0-or-later](LICENSE). Read it, run it, change it, share it. The
one condition that matters: if you build on this code and hand the
result to anyone — as a download *or* as a website people use over a
network — that version has to be open too, under the same licence.
Using it yourself, inside a company or for research, obliges you
nothing.

Two parts of the tree are not under it, and say so themselves:
`apps/mcp` is `LicenseRef-Proprietary`, and the vendored OpenApps
packages under `apps/desktop/vendor/openapps` stay MIT OR Apache-2.0,
which is their own licence and is compatible with this one.

**Releases up to and including 1.0.1 were published under MIT OR
Apache-2.0**, and that grant cannot be withdrawn: any copy of those
commits stays permissively licensed for whoever holds it, forever. The
change applies from the next version onward. It brings this repository
in line with OpenCapture and OpenTabs, which have been AGPL from the
start.
