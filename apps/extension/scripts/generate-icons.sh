#!/usr/bin/env bash
# Phase 5 Task 3: generates the extension's toolbar/store icons
# (public/icons/{16,48,128}.png, referenced by manifest.json's "icons" and
# "action.default_icon") from the same brand mark apps/desktop's Tauri
# build already ships (apps/desktop/src-tauri/icons/icon.png, a 512x512
# master). Resampling from that 512px master rather than from the
# pre-shrunk 128x128.png avoids compounding a second lossy downscale on
# top of the one Tauri's own icon pipeline already did to produce it.
#
# Not wired into `npm run build` — the source master changes approximately
# never (a brand-asset update, not a code change), so this is a one-time/
# occasional manual step; its *output* (public/icons/*.png) is committed
# like any other static asset. Re-run this after any change to the
# desktop app's icon.png.
#
# Requires macOS's `sips` (present by default on any Mac; this repo's
# build environment is macOS — see openapps-build-env memory). No
# ImageMagick/other dependency introduced for what's a one-off, rarely-run
# script.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$EXT_DIR")")"
SOURCE_ICON="$WORKSPACE_DIR/apps/desktop/src-tauri/icons/icon.png"
OUT_DIR="$EXT_DIR/public/icons"

if [ ! -f "$SOURCE_ICON" ]; then
  echo "generate-icons.sh: expected source icon at $SOURCE_ICON — has apps/desktop/src-tauri/icons/ moved?" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

for size in 16 48 128; do
  echo "generate-icons.sh: writing ${size}x${size} -> $OUT_DIR/${size}.png"
  sips -z "$size" "$size" "$SOURCE_ICON" --out "$OUT_DIR/${size}.png" >/dev/null
done

echo "generate-icons.sh: done — $OUT_DIR now has 16.png, 48.png, 128.png"
