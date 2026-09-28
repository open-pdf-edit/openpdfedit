#!/usr/bin/env bash
# Puts the web build where the Android shell serves it from.
#
#   apps/android/scripts/sync-web.sh            # copy an existing build
#   apps/android/scripts/sync-web.sh --build    # build it first
#   OCR=0 apps/android/scripts/sync-web.sh      # ...without the OCR assets
#
# This is a thin wrapper around `apps/ios/scripts/sync-web.sh`, called
# with a different destination, and deliberately not a copy of it. That
# script's real content is the list of things that must come *out* of the
# web build before it goes inside an app binary — the service worker,
# whose cache would outlive an app update and serve the old bundle; the
# crawler copy, which reads as "a repackaged website" to a reviewer; any
# cross-origin <script>. None of that is specific to Apple. Keeping one
# copy means Android cannot quietly ship what iOS learned not to.
#
# The destination is `app/src/main/assets/www`, served at
# `https://appassets.androidplatform.net/` by `MainActivity`'s
# WebViewAssetLoader — an https origin because the page must be a secure
# context for `crypto.subtle`, the same reason iOS serves from
# `openpdfedit://localhost` rather than a bare custom host.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(cd "$ANDROID_DIR/../.." && pwd)"
DEST="$ANDROID_DIR/app/src/main/assets/www"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }

# Its own build directory, not iOS's: the two shells are built from the
# same source but at different times, and sharing one output means an
# Android sync can silently hand iOS a half-written tree.
export IOS_WEB_DIR="${ANDROID_WEB_DIR:-$ANDROID_DIR/.build/web}"
export WEB_DEST="$DEST"

bash "$WORKSPACE_DIR/apps/ios/scripts/sync-web.sh" "$@"

# 70 of the bundle's 84 MB is OCR: six Tesseract core variants and twelve
# languages. Play accepts a 200 MB bundle so it fits, but it is a poor
# download for a feature most people never open, and the browser
# extension already ships without it. Dropping it here leaves OCR the one
# tool that needs a network; fetching the assets on first use is the
# proper fix and wants its own change.
if [ "${OCR:-1}" = "0" ]; then
  log "Dropping the OCR assets (OCR=0)"
  rm -rf "$DEST/ocr"
fi

log "Android bundle ready"
echo "  files: $(find "$DEST" -type f | wc -l | tr -d ' ')"
echo "  size:  $(du -sh "$DEST" | cut -f1)"
