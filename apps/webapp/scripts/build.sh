#!/usr/bin/env bash
# Builds OpenPdfEdit as a static web app: the shared SPA
# (apps/desktop/src) compiled against the WebAssembly backend, plus the
# two wasm binaries it fetches at runtime.
#
# This is deliberately apps/extension/scripts/build-spa.sh minus the two
# things that are only true of an extension:
#
#   - externalize-inline.mjs, which exists solely because MV3's CSP
#     forbids inline <script> content. A web page has no such rule, so
#     SvelteKit's own bootstrap script is left exactly as emitted.
#   - background.js and manifest.json, which have no meaning here.
#
# Everything else — the wasm build, the vendored pdfium, the asset
# layout wasm.ts expects — is shared, and is reused from the extension's
# scripts rather than duplicated, so the two builds cannot drift.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBAPP_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$WEBAPP_DIR")")"
DESKTOP_DIR="$WORKSPACE_DIR/apps/desktop"
EXT_DIR="$WORKSPACE_DIR/apps/extension"
DIST_DIR="$WEBAPP_DIR/dist"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }

log "Fetching the PDFium wasm binary (no-op if present)"
# Same vendored binary the extension uses, at the same path wasm.ts
# expects — fetched here so this build works from a clean checkout
# rather than only after someone has built the extension.
bash "$WORKSPACE_DIR/scripts/fetch-pdfium-wasm.sh"

log "Building the Rust core for wasm32"
# Shared with the extension: same crate, same pinned wasm-bindgen, same
# output location (apps/desktop/src/lib/wasm-gen + the vendored pdfium).
bash "$EXT_DIR/scripts/build-wasm.sh"

log "Building the SPA against the wasm backend"
# VITE_BACKEND=wasm is what makes backend/index.ts resolve to wasm.ts's
# WasmBackend instead of the Tauri default — see that file's doc.
(cd "$DESKTOP_DIR" && rm -rf build .svelte-kit && VITE_BACKEND=wasm npm run build)

if [ ! -f "$DESKTOP_DIR/build/index.html" ]; then
  echo "build.sh: $DESKTOP_DIR/build/index.html missing — did the SPA build fail silently?" >&2
  exit 1
fi

log "Assembling $DIST_DIR"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"
cp -R "$DESKTOP_DIR/build/." "$DIST_DIR/"

# The runtime assets no bundler ever sees: wasm.ts injects
# <script src="/pdfium.js"> itself and dynamically imports
# /wasm-gen/openpdfedit_wasm.js through a string specifier, so nothing
# copies these unless a script does it explicitly.
log "Copying the wasm runtime assets"
PDFIUM_DIR="$WORKSPACE_DIR/.vendor/pdfium-wasm/release/node"
for f in pdfium.js pdfium.wasm; do
  [ -f "$PDFIUM_DIR/$f" ] || { echo "build.sh: missing $PDFIUM_DIR/$f — run $EXT_DIR/scripts/build-wasm.sh" >&2; exit 1; }
  cp "$PDFIUM_DIR/$f" "$DIST_DIR/$f"
done
# build-wasm.sh emits into apps/extension/src/wasm-gen (it is the
# extension's script, shared rather than duplicated); the web app copies
# from there rather than re-running the generator into a second location.
WASM_GEN_DIR="$EXT_DIR/src/wasm-gen"
mkdir -p "$DIST_DIR/wasm-gen"
for f in openpdfedit_wasm.js openpdfedit_wasm_bg.wasm; do
  [ -f "$WASM_GEN_DIR/$f" ] || { echo "build.sh: missing $WASM_GEN_DIR/$f" >&2; exit 1; }
  cp "$WASM_GEN_DIR/$f" "$DIST_DIR/wasm-gen/$f"
done

# A service worker, so the app keeps working with no network at all —
# which is the whole claim, and only demonstrable if it's true offline.
log "Adding the offline service worker"
cp "$WEBAPP_DIR/service-worker.js" "$DIST_DIR/service-worker.js"
cp "$WEBAPP_DIR/manifest.webmanifest" "$DIST_DIR/manifest.webmanifest"

# Injected here rather than in apps/desktop/src/app.html, because that
# file is shared with the Tauri and extension builds: a manifest link
# would 404 in both, and registering a service worker inside a
# chrome-extension:// origin or a Tauri window is meaningless at best.
# Post-processing keeps the shared SPA free of web-app-only concerns.
log "Registering the service worker and web manifest in index.html"
node - "$DIST_DIR/index.html" <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
const file = process.argv[2];
const html = readFileSync(file, "utf8");
if (html.includes("service-worker.js")) process.exit(0);
const inject = [
  '<link rel="manifest" href="./manifest.webmanifest">',
  "<script>",
  "if ('serviceWorker' in navigator) {",
  "  addEventListener('load', () => {",
  "    navigator.serviceWorker.register('./service-worker.js').catch(() => {});",
  "  });",
  "}",
  "</script>",
].join("\n");
if (!html.includes("</head>")) {
  console.error("build.sh: no </head> in index.html — cannot register the service worker");
  process.exit(1);
}
writeFileSync(file, html.replace("</head>", inject + "\n</head>"));
NODE

log "Done"
echo "  output:  $DIST_DIR"
echo "  size:    $(du -sh "$DIST_DIR" | cut -f1)"
echo
echo "Serve it (a service worker needs an http origin, not file://):"
echo "  npm --prefix apps/webapp run preview   # http://localhost:8081"
