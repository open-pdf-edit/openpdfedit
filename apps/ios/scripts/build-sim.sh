#!/usr/bin/env bash
# Builds the app for a simulator and, with --run, installs and launches it.
#
# What this is for: seeing the shell actually work without an Apple
# Developer account. Everything except a real purchase runs here — the
# bundled editor, the document handoff, sign-in — and purchases run too,
# against the local product catalogue in Products.storekit.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$SCRIPT_DIR")"
DEVICE="${IOS_SIMULATOR:-iPhone 17}"
DERIVED="${IOS_DERIVED_DATA:-$IOS_DIR/.build}"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }

[ -d "$IOS_DIR/www" ] || {
  echo "build-sim.sh: apps/ios/www is missing — run apps/ios/scripts/sync-web.sh" >&2
  exit 1
}

log "Building for $DEVICE"
xcodebuild build \
  -project "$IOS_DIR/OpenPdfEdit.xcodeproj" \
  -scheme OpenPdfEdit \
  -configuration Debug \
  -destination "platform=iOS Simulator,name=$DEVICE" \
  -derivedDataPath "$DERIVED" \
  | grep -E "error:|warning:|BUILD" || true

APP="$DERIVED/Build/Products/Debug-iphonesimulator/OpenPdfEdit.app"
[ -d "$APP" ] || { echo "build-sim.sh: no app at $APP" >&2; exit 1; }
echo "  app: $APP"

if [ "${1:-}" = "--run" ]; then
  log "Installing and launching"
  xcrun simctl boot "$DEVICE" 2>/dev/null || true
  xcrun simctl bootstatus "$DEVICE" -b >/dev/null
  xcrun simctl install "$DEVICE" "$APP"
  # Not through Xcode, so the scheme's StoreKit configuration does not
  # apply and product lookups go to the real App Store — which answers
  # nothing until there is a developer account. Everything else works.
  xcrun simctl launch "$DEVICE" com.openpdfedit.app
fi
