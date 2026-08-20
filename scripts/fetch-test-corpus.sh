#!/usr/bin/env bash
# Downloads a curated subset of the pdf.js test PDF corpus into
# testdata/corpus/, for engine/doc-crate tests and as cargo-fuzz seed
# input. pdf.js (mozilla/pdf.js) is Apache-2.0 — see
# docs/research/04-oss-borrow-map.md — so its committed test fixtures are
# safe to redistribute here. We do NOT vendor the whole 683-file corpus
# (that belongs in pdf.js's own repo); this is a deliberately small,
# feature-diverse slice, picked for:
#   - general structural coverage (multi-object docs, page trees, forms)
#   - non-Latin/CID font handling (CJK, Arabic)
#   - annotation variety
#   - two files literally named "*-fuzzed.pdf"/"*reduced.pdf" — pdf.js's
#     own minimized repro cases for past parser crashes, which make an
#     excellent fuzz corpus seed for openpdfedit-doc
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORPUS_DIR="$ROOT_DIR/testdata/corpus"
BASE_URL="https://raw.githubusercontent.com/mozilla/pdf.js/master/test/pdfs"

FILES=(
  "TAMReview.pdf"
  "basicapi.pdf"
  "acroform_calculation_order.pdf"
  "annotation-highlight.pdf"
  "annotation-freetext.pdf"
  "annotation-button-widget.pdf"
  "encrypted-attachment.pdf"
  "hello_world_rotated.pdf"
  "90ms_rksj_h_sample.pdf"
  "ArabicCIDTrueType.pdf"
  "XiaoBiaoSong.pdf"
  "ZapfDingbats.pdf"
  "Embedded_font.pdf"
  "Pages-tree-refs.pdf"
  "ContentStreamCycleType3insideType3.pdf"
  "IndexedCS_negative_and_high.pdf"
  "GHOSTSCRIPT-698804-1-fuzzed.pdf"
  "PDFBOX-3148-2-fuzzed.pdf"
  "PDFJS-7562-reduced.pdf"
  "PDFJS-9279-reduced.pdf"
)

mkdir -p "$CORPUS_DIR"

for f in "${FILES[@]}"; do
  dest="$CORPUS_DIR/$f"
  if [ -f "$dest" ]; then
    continue
  fi
  echo "fetching $f..."
  curl -fsSL -o "$dest" "$BASE_URL/$f"
done

cat > "$CORPUS_DIR/SOURCE.md" <<'EOF'
# Test corpus provenance

These PDFs are a curated subset of the [pdf.js](https://github.com/mozilla/pdf.js)
test suite (`test/pdfs/`), Apache-2.0 licensed. Fetched by
`scripts/fetch-test-corpus.sh` — not committed to this repo directly (see
`.gitignore`), so re-run that script after a fresh checkout.

Used for: openpdfedit-doc/openpdfedit-engine corpus tests, and as seed
input for the openpdfedit-doc cargo-fuzz target (`fuzz/fuzz_targets/parse_document.rs`).
EOF

echo "corpus ready: $CORPUS_DIR ($(ls "$CORPUS_DIR"/*.pdf 2>/dev/null | wc -l | tr -d ' ') files)"
