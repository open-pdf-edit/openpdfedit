#!/usr/bin/env bash
# Fetch the OCR language data the browser build needs.
#
# tesseract.js ships its engine (a ~1.2 MB gzipped wasm core) in
# node_modules, but not the trained data — by default it downloads that
# from a CDN the first time you OCR anything. That default is wrong for
# this product twice over: it makes "your documents never leave your
# machine" depend on a third party being asked for something at the
# moment you use the feature, and it means OCR is the one tool that
# stops working offline.
#
# So the data is served from our own origin, and this fetches it. Same
# shape as fetch-pdfium-wasm.sh: downloaded into .vendor/, never
# committed, no-op if already present.
#
# `tessdata_fast` rather than `tessdata_best`: 3.9 MB against 15 MB, for
# accuracy that is worse on hard scans and indistinguishable on ordinary
# ones. A first-use download four times larger is the wrong trade for a
# feature most people use occasionally.
set -euo pipefail

cd "$(dirname "$0")/.."

DEST=".vendor/tesseract"
LANG="${1:-eng}"
URL="https://github.com/tesseract-ocr/tessdata_fast/raw/main/${LANG}.traineddata"

mkdir -p "$DEST"

if [ -f "$DEST/${LANG}.traineddata" ]; then
  echo "tesseract ${LANG}.traineddata already present at $DEST — skipping download"
  exit 0
fi

echo "fetching ${LANG}.traineddata…"
curl -fsSL --max-time 300 -o "$DEST/${LANG}.traineddata.part" "$URL"

# Trained data starts with a version/magic run; a captive-portal HTML
# page or a 404 body would otherwise sit there looking like a download
# that worked until OCR failed with something unrelated-sounding.
if head -c 4 "$DEST/${LANG}.traineddata.part" | grep -qi "<"; then
  rm -f "$DEST/${LANG}.traineddata.part"
  echo "fetch-tesseract-assets.sh: that download is not trained data — check the URL or your network" >&2
  exit 1
fi

mv "$DEST/${LANG}.traineddata.part" "$DEST/${LANG}.traineddata"
echo "tesseract data ready: $DEST/${LANG}.traineddata ($(du -h "$DEST/${LANG}.traineddata" | cut -f1))"
