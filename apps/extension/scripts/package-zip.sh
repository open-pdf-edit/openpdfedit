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

# Both stores cap the listing name at 75 characters and the description at
# 132, and neither Chrome nor anything local enforces either: the extension
# loads unpacked, the build succeeds, and the first thing to measure them is
# the dashboard, after the upload. A v0.1.7 upload was rejected exactly this
# way.
#
# Since the manifest was localised these strings are no longer *in* the
# manifest -- name and description are now "__MSG_name__" and
# "__MSG_description__", nineteen characters that fit any limit and say
# nothing. The text a store actually measures lives in _locales/<code>/
# messages.json, nineteen times over, and a single overlong translation
# rejects the whole submission. So measure every catalogue, and measure the
# ones in dist/ -- the artifact about to be zipped -- rather than the source
# they were built from.
if ! node "$SCRIPT_DIR/check-locales.mjs" "$DIST_DIR"; then
  echo "package-zip.sh: listing catalogues failed their checks (see above); not packaging" >&2
  exit 1
fi

rm -f "$OUT_ZIP"
(cd "$DIST_DIR" && zip -r -X -q "$OUT_ZIP" .)

BYTES=$(wc -c < "$OUT_ZIP" | tr -d ' ')
echo "package-zip.sh: wrote $OUT_ZIP ($BYTES bytes)"
