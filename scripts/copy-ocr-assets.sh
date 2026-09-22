#!/usr/bin/env bash
# Puts the OCR engine and its language data into a build.
#
#   scripts/copy-ocr-assets.sh <dest-dir>     # e.g. apps/webapp/dist/ocr
#
# Used by the web app build and by the desktop app's own build, both of
# which recognise text in the page with tesseract.js rather than by
# running a tesseract binary. The desktop used to run one — found on the
# customer's PATH, or at Homebrew's prefix — which meant OCR, one of the
# two tools the credits pay for, worked only for people who had typed
# `brew install tesseract` first, and could never work in the Mac App
# Store's sandbox at all.
#
# The extension build does not call this. It ships without OCR on
# purpose, and its store check asserts the engine is absent.
set -euo pipefail

DEST="${1:?usage: copy-ocr-assets.sh <dest-dir>}"
WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$WORKSPACE_DIR/apps/desktop"
TESS_CORE="$DESKTOP_DIR/node_modules/tesseract.js-core"
TESS_JS="$DESKTOP_DIR/node_modules/tesseract.js/dist"

bash "$WORKSPACE_DIR/scripts/fetch-tesseract-assets.sh"
mkdir -p "$DEST"

# Every core variant, not a chosen few. tesseract.js picks one at
# runtime from what the engine supports — plain, SIMD, or relaxed SIMD,
# each with an LSTM-only twin — and asks for it by name. Shipping a
# subset works on whatever machine the build was tested on and fails on
# someone else's with "failed to load", which is exactly what happened:
# the guess omitted relaxedsimd, which is what current Chrome asks for.
cp "$TESS_CORE"/tesseract-core*.wasm "$TESS_CORE"/tesseract-core*.wasm.js "$DEST/"
[ -f "$DEST/tesseract-core-relaxedsimd-lstm.wasm.js" ] || {
  echo "copy-ocr-assets: no tesseract core variants found in $TESS_CORE — run npm install in apps/desktop" >&2
  exit 1
}
[ -f "$TESS_JS/worker.min.js" ] || { echo "copy-ocr-assets: missing $TESS_JS/worker.min.js" >&2; exit 1; }
cp "$TESS_JS/worker.min.js" "$DEST/worker.min.js"

# Every language that was fetched, not a named one. Tesseract reads the
# script it has data for and returns silence or nonsense for anything
# else without failing, so a missing language is OCR appearing not to
# work rather than an error anyone sees.
shopt -s nullglob
TRAINED=("$WORKSPACE_DIR/.vendor/tesseract"/*.traineddata)
shopt -u nullglob
[ ${#TRAINED[@]} -gt 0 ] || {
  echo "copy-ocr-assets: no trained data in .vendor/tesseract — run scripts/fetch-tesseract-assets.sh" >&2
  exit 1
}
cp "${TRAINED[@]}" "$DEST/"
echo "copy-ocr-assets: $(printf '%s ' "${TRAINED[@]##*/}" | sed 's/\.traineddata//g')→ $DEST"
