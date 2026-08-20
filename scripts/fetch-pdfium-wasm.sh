#!/usr/bin/env bash
# Downloads the prebuilt PDFium WASM build for browser/extension targets
# from paulocoutinhox/pdfium-lib into .vendor/pdfium-wasm/, so the
# wasm32-unknown-unknown build of pdfium-render can load it at runtime. See
# docs/superpowers/plans/2026-08-10-extension-wasm-walking-skeleton.md: this
# is the specific WASM build pdfium-render's own README recommends —
# bblanchon/pdfium-binaries (used by scripts/fetch-pdfium.sh for the native
# library) ships no WASM asset at all, and @embedpdf/pdfium is compiled
# with a non-growable WASM heap that runs out of memory opening anything
# beyond a few pages. PDFium is BSD-3; we do NOT vendor the binary in git
# (see .gitignore) — this script is the reproducible fetch step, run it
# locally and in CI.
set -euo pipefail

# Pin a specific pdfium-lib release tag rather than "latest" so builds are
# reproducible; bump this deliberately when upgrading.
PDFIUM_WASM_TAG="7902"
ASSET="wasm.tgz"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/.vendor/pdfium-wasm"

# pdfium-lib's wasm.tgz has no standalone VERSION file (unlike
# bblanchon/pdfium-binaries); release/package.json's "version" field is
# "<tag>.0.0", so use that as the already-fetched marker.
if [ -f "$VENDOR_DIR/release/package.json" ] && grep -q "\"version\": \"${PDFIUM_WASM_TAG}.0.0\"" "$VENDOR_DIR/release/package.json" 2>/dev/null; then
  echo "pdfium wasm ${PDFIUM_WASM_TAG} already present at $VENDOR_DIR — skipping download"
  exit 0
fi

echo "fetching pdfium wasm ${PDFIUM_WASM_TAG} (${ASSET})..."
mkdir -p "$VENDOR_DIR"
tmpfile="$(mktemp)"
url="https://github.com/paulocoutinhox/pdfium-lib/releases/download/${PDFIUM_WASM_TAG}/${ASSET}"
curl -fsSL -o "$tmpfile" "$url"
tar xzf "$tmpfile" -C "$VENDOR_DIR"
rm -f "$tmpfile"

echo "pdfium wasm build: $VENDOR_DIR/release/node/pdfium.js + pdfium.wasm"
