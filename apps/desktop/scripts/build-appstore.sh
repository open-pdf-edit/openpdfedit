#!/usr/bin/env bash
# Builds OpenPdfEdit for the Mac App Store.
#
#   apps/desktop/scripts/build-appstore.sh test    # sandboxed, ad-hoc, runs on this Mac
#   apps/desktop/scripts/build-appstore.sh store   # signed for the store, packaged as a .pkg
#   apps/desktop/scripts/build-appstore.sh upload  # ...then validated and uploaded
#
# Two modes because a store-signed app does not run anywhere but the
# store: signed with Apple Distribution and carrying the Mac App Store
# profile, macOS refuses to launch it locally. So `test` builds the same
# sandboxed app signed ad hoc with only the sandbox entitlements — enough
# to prove that opening, saving, OCR and sign-in all survive the sandbox —
# and `store` produces what App Store Connect accepts. StoreKit itself is
# exercised through TestFlight for Mac, after an upload.
#
# What differs from the Developer ID build on GitHub, and why:
#   - bundle id com.openpdfedit.app, the universal record, so one purchase
#     and one set of credit packs covers iPhone, iPad and Mac;
#   - the App Sandbox (required), with user-selected files and the
#     network client only;
#   - no updater and no inspector: the Rust `updater` and `devtools`
#     features are left out, and the page's update panel draws nothing.
#
# Needs, once: apps/ios/scripts/asc.py ensure-cert, ensure-installer-cert
# and ensure-mac-profile (the last writes the profile this embeds).
set -euo pipefail

MODE="${1:-test}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"          # apps/desktop
ROOT="$(cd "$HERE/../.." && pwd)"
TAURI_DIR="$HERE/src-tauri"
STORE_DIR="$TAURI_DIR/appstore"
OUT="${APPSTORE_OUT:-$ROOT/target/appstore}"
TEAM="JY2NWT5QFV"
APP_IDENTITY="Apple Distribution: DE JIAN KOH ($TEAM)"
PKG_IDENTITY="3rd Party Mac Developer Installer: DE JIAN KOH ($TEAM)"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }
die() { echo "build-appstore: $1" >&2; exit 1; }

case "$MODE" in test|store|upload) ;; *) die "mode is test, store or upload — not '$MODE'" ;; esac

# PDFium is the bundle's other binary. It is signed here, on a copy, with
# the identity this build uses: the vendored file stays as it was for the
# Developer ID build, which signs it with a different identity.
mkdir -p "$OUT"
cp "$ROOT/.vendor/pdfium/lib/libpdfium.dylib" "$STORE_DIR/libpdfium.dylib"

CONFIG="$OUT/tauri.appstore.$MODE.conf.json"
if [ "$MODE" = test ]; then
  log "Sandboxed test build (ad hoc — runs on this Mac only)"
  codesign --force --sign - "$STORE_DIR/libpdfium.dylib"
  # Only the sandbox itself: no application identifier, which would need
  # the store profile, and without which the app still runs sandboxed.
  cat > "$STORE_DIR/OpenPdfEdit.test.entitlements" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
	<key>com.apple.security.app-sandbox</key><true/>
	<key>com.apple.security.files.user-selected.read-write</key><true/>
	<key>com.apple.security.network.client</key><true/>
</dict></plist>
XML
  python3 - "$TAURI_DIR/tauri.appstore.conf.json" "$CONFIG" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c["bundle"]["macOS"].update({"signingIdentity": "-", "entitlements": "appstore/OpenPdfEdit.test.entitlements"})
c["bundle"]["macOS"].pop("files", None)
c["bundle"]["resources"] = {"../../../.vendor/pdfium/lib/libpdfium.dylib": None, "appstore/libpdfium.dylib": "libpdfium.dylib"}
json.dump(c, open(sys.argv[2], "w"), indent=2)
PY
else
  [ -f "$STORE_DIR/embedded.provisionprofile" ] || die "no profile — run apps/ios/scripts/asc.py ensure-mac-profile"
  security find-identity -v -p codesigning | grep -q "$APP_IDENTITY" || die "no '$APP_IDENTITY' — run asc.py ensure-cert"
  security find-identity -v | grep -q "$PKG_IDENTITY" || die "no '$PKG_IDENTITY' — run asc.py ensure-installer-cert"
  log "Store build"
  codesign --force --timestamp --sign "$APP_IDENTITY" "$STORE_DIR/libpdfium.dylib"
  python3 - "$TAURI_DIR/tauri.appstore.conf.json" "$CONFIG" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c["bundle"]["resources"] = {"../../../.vendor/pdfium/lib/libpdfium.dylib": None, "appstore/libpdfium.dylib": "libpdfium.dylib"}
json.dump(c, open(sys.argv[2], "w"), indent=2)
PY
fi

log "Building (no updater, no inspector)"
cd "$HERE"
npx tauri build --bundles app --config "$CONFIG" --features appstore -- --no-default-features

APP="$ROOT/target/release/bundle/macos/OpenPdfEdit.app"
[ -d "$APP" ] || die "no app at $APP"
rm -rf "$OUT/OpenPdfEdit.app" && cp -R "$APP" "$OUT/OpenPdfEdit.app"
APP="$OUT/OpenPdfEdit.app"

log "What was built"
/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$APP/Contents/Info.plist"
codesign -d --entitlements - --xml "$APP" 2>/dev/null | plutil -convert json -o - - 2>/dev/null \
  | python3 -c "import json,sys; e=json.load(sys.stdin); print('  entitlements:', ', '.join(k.split('.')[-1] if 'security' in k else k for k in e))"
# Not merely "was it built without the updater" — is any of it in there.
if strings "$APP/Contents/MacOS/"* | grep -q "plugin:updater"; then
  die "the updater is still in the binary"
fi
echo "  updater: absent from the binary"

[ "$MODE" = test ] && { log "Ready: open \"$APP\""; exit 0; }

[ -f "$APP/Contents/embedded.provisionprofile" ] || die "the profile was not embedded"
codesign --verify --deep --strict "$APP"

log "Packaging"
PKG="$OUT/OpenPdfEdit.pkg"
productbuild --component "$APP" /Applications --sign "$PKG_IDENTITY" "$PKG"
log "$PKG ($(du -h "$PKG" | cut -f1))"

[ "$MODE" = upload ] || exit 0
[ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ] || die "ASC_KEY_ID and ASC_ISSUER_ID are needed to upload"
log "Validating with Apple"
xcrun altool --validate-app -f "$PKG" -t macos --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
log "Uploading"
xcrun altool --upload-app -f "$PKG" -t macos --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
