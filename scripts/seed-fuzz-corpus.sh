#!/usr/bin/env bash
# Seeds the openpdfedit-doc fuzz target's corpus directory from
# testdata/corpus/ (fetch that first via scripts/fetch-test-corpus.sh).
# libFuzzer mutates from real, structurally-valid-ish PDFs far more
# effectively than from nothing — the pdf.js-derived corpus already
# includes past-crash repro cases, which is exactly what a fuzzer wants
# to start mutating from.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$ROOT_DIR/testdata/corpus"
DEST_DIR="$ROOT_DIR/crates/openpdfedit-doc/fuzz/corpus/parse_document"

if [ ! -d "$SRC_DIR" ]; then
  echo "error: $SRC_DIR not present — run scripts/fetch-test-corpus.sh first" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"
cp "$SRC_DIR"/*.pdf "$DEST_DIR"/
echo "seeded $(ls "$DEST_DIR"/*.pdf | wc -l | tr -d ' ') files into $DEST_DIR"
