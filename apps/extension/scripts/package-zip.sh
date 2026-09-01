#!/usr/bin/env bash
# Phase 5 Task 3: zips a already-built dist/ into the artifact the Chrome
# Web Store dashboard expects to upload — see STORE.md's "Producing the
# upload zip" section. Invoked via `npm run package` (which runs a fresh
# `npm run build` first; see package.json), not meant to be run standalone
# against a stale dist/.
#
# `cd dist && zip ...` (rather than `zip ... dist/*`) matters: it's what
# makes manifest.json land at the zip's own root instead of inside a
# nested dist/ directory — Chrome's uploader requires the former.
#
# The result is committed, unlike most build output. The root README
# points people at it as the fastest way to try the extension without a
# Rust toolchain, and the root .gitignore names it as tracked on
# purpose. So rebuild and commit it whenever the extension changes —
# a stale one there is a broken "fastest path".
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$EXT_DIR/dist"
OUT_ZIP="$EXT_DIR/openpdfedit-dist.zip"

if [ ! -f "$DIST_DIR/manifest.json" ]; then
  echo "package-zip.sh: expected $DIST_DIR/manifest.json — run npm run build first" >&2
  exit 1
fi

# Both stores cap the manifest description at 132 characters, and
# neither Chrome nor anything local enforces it: the extension loads
# unpacked, the build succeeds, and the first thing to measure it is the
# dashboard, after the upload. A v0.1.7 upload was rejected exactly this
# way. e2e/manifest.spec.ts covers it too, but nothing makes a person
# run the suite before uploading, and this is the step that produces the
# thing they upload.
DESCRIPTION_LIMIT=132
description_length="$(
  python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["description"]))' \
    "$DIST_DIR/manifest.json" 2>/dev/null || echo 0
)"
if [ "$description_length" -gt "$DESCRIPTION_LIMIT" ]; then
  echo "package-zip.sh: manifest description is $description_length characters;" >&2
  echo "  stores reject anything over $DESCRIPTION_LIMIT. Shorten it in public/manifest.json" >&2
  echo "  (and in STORE.md, which has to match)." >&2
  exit 1
fi

rm -f "$OUT_ZIP"
(cd "$DIST_DIR" && zip -r -X -q "$OUT_ZIP" .)

BYTES=$(wc -c < "$OUT_ZIP" | tr -d ' ')
echo "package-zip.sh: wrote $OUT_ZIP ($BYTES bytes)"
