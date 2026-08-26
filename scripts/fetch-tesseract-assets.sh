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

# Every language the OCR dialog offers. Pass names to fetch a different
# set. English alone is what shipped first, and it is why OCR on a
# Chinese document appeared to run and produced nothing: tesseract reads
# the script it has data for and does not complain about the rest.
DEFAULT_LANGS=(eng chi_sim chi_tra jpn kor fra deu spa por ita rus ara)
LANGS=("$@")
[ ${#LANGS[@]} -gt 0 ] || LANGS=("${DEFAULT_LANGS[@]}")

mkdir -p "$DEST"

for LANG_CODE in "${LANGS[@]}"; do
  TARGET="$DEST/${LANG_CODE}.traineddata"
  if [ -f "$TARGET" ]; then
    echo "tesseract ${LANG_CODE}.traineddata already present — skipping"
    continue
  fi

  echo "fetching ${LANG_CODE}.traineddata…"
  curl -fsSL --max-time 300 -o "$TARGET.part" \
    "https://github.com/tesseract-ocr/tessdata_fast/raw/main/${LANG_CODE}.traineddata"

  # Trained data starts with a version/magic run; a captive-portal HTML
  # page or a 404 body would otherwise sit there looking like a download
  # that worked until OCR failed with something unrelated-sounding.
  if head -c 4 "$TARGET.part" | grep -qi "<"; then
    rm -f "$TARGET.part"
    echo "fetch-tesseract-assets.sh: ${LANG_CODE} did not download trained data — check the name" >&2
    exit 1
  fi

  mv "$TARGET.part" "$TARGET"
  echo "ready: $TARGET ($(du -h "$TARGET" | cut -f1))"
done
