#!/usr/bin/env bash
# Builds a signed .ipa for the App Store, and optionally uploads it.
#
#   ./scripts/archive.sh              # build an .ipa
#   ./scripts/archive.sh --validate   # ...and have Apple check it
#   ./scripts/archive.sh --upload     # ...and send it to App Store Connect
#
# Why this exists rather than Xcode's Organizer: the build number has to
# be unique on every upload and Apple never releases one back, so a
# duplicate costs a round trip to discover. Deriving it here makes that
# impossible to get wrong. The web app also has to be re-synced before
# every archive — the bundle is a *copy* of apps/desktop's build output,
# so archiving without syncing quietly ships whatever was there last.
#
# Uploading needs an App Store Connect API key (Users and Access → Keys →
# App Store Connect API, role Developer or App Manager). Put the .p8
# where altool looks for it:
#
#   mkdir -p ~/.appstoreconnect/private_keys
#   mv ~/Downloads/AuthKey_XXXXXXXXXX.p8 ~/.appstoreconnect/private_keys/
#
# then set, in the environment:
#
#   ASC_KEY_ID     the ten-character key id, e.g. XXXXXXXXXX
#   ASC_ISSUER_ID  the issuer uuid shown above the key list
#
# The .p8 is a credential. It is not readable twice — Apple lets you
# download it once — and it is never committed here.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$SCRIPT_DIR")"
BUILD="${IOS_ARCHIVE_DIR:-$IOS_DIR/.build/archive}"
ARCHIVE="$BUILD/OpenPdfEdit.xcarchive"
EXPORT_DIR="$BUILD/export"
IPA="$EXPORT_DIR/OpenPdfEdit.ipa"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }
die() { echo "archive.sh: $1" >&2; exit 1; }

MODE=build
case "${1:-}" in
  "")          ;;
  --validate)  MODE=validate ;;
  --upload)    MODE=upload ;;
  *)           die "unknown option '$1' (expected --validate or --upload)" ;;
esac

PROJECT="$IOS_DIR/OpenPdfEdit.xcodeproj/project.pbxproj"
TEAM="$(grep -m1 -o 'DEVELOPMENT_TEAM = [^;]*;' "$PROJECT" | sed 's/DEVELOPMENT_TEAM = //; s/;$//; s/"//g')"
[ -n "$TEAM" ] || die "DEVELOPMENT_TEAM is unset — run ./scripts/set-team.sh <TEAMID>"

security find-identity -v -p codesigning 2>/dev/null | grep -q "Apple Distribution\|Apple Development" \
  || die "no signing identity in the keychain — sign in under Xcode → Settings → Accounts, then Manage Certificates → +"

# The bundle's web app is a copy, so a stale one archives silently.
log "Syncing the web app"
"$SCRIPT_DIR/sync-web.sh" --build

# Marketing version comes from ../../scripts/set-version.sh, which writes
# it in all five places at once. The build number only has to be larger
# than every one already uploaded, and minutes-since-2020 is monotonic,
# needs no state, and stays inside the 32-bit range until 2103.
VERSION="$(grep -m1 -o 'MARKETING_VERSION = [^;]*;' "$PROJECT" | sed 's/MARKETING_VERSION = //; s/;$//')"
BUILD_NUMBER=$(( ($(date +%s) - 1577836800) / 60 ))
log "OpenPdfEdit $VERSION ($BUILD_NUMBER), team $TEAM"

rm -rf "$BUILD"
mkdir -p "$BUILD"

log "Archiving"
xcodebuild archive \
  -project "$IOS_DIR/OpenPdfEdit.xcodeproj" \
  -scheme OpenPdfEdit \
  -configuration Release \
  -destination "generic/platform=iOS" \
  -archivePath "$ARCHIVE" \
  -allowProvisioningUpdates \
  CURRENT_PROJECT_VERSION="$BUILD_NUMBER" \
  | grep -E "error:|warning:|ARCHIVE" || true

[ -d "$ARCHIVE" ] || die "no archive at $ARCHIVE — rerun without the grep filter to see why"

# The team id lives in one place (the project), so the export options are
# generated rather than committed with a second copy of it.
OPTIONS="$BUILD/ExportOptions.plist"
sed "s/__TEAM_ID__/$TEAM/" "$IOS_DIR/ExportOptions.plist" > "$OPTIONS"

log "Exporting"
xcodebuild -exportArchive \
  -archivePath "$ARCHIVE" \
  -exportPath "$EXPORT_DIR" \
  -exportOptionsPlist "$OPTIONS" \
  -allowProvisioningUpdates \
  | grep -E "error:|EXPORT" || true

[ -f "$IPA" ] || die "no .ipa at $IPA"
log "$IPA ($(du -h "$IPA" | cut -f1))"

[ "$MODE" = build ] && exit 0

[ -n "${ASC_KEY_ID:-}" ]    || die "ASC_KEY_ID is unset — see the header of this script"
[ -n "${ASC_ISSUER_ID:-}" ] || die "ASC_ISSUER_ID is unset — see the header of this script"

if [ "$MODE" = validate ]; then
  log "Validating with Apple"
  exec xcrun altool --validate-app -f "$IPA" -t ios \
    --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
fi

# Validate first even when uploading. A rejected upload still consumes the
# build number; a failed validation does not.
log "Validating with Apple"
xcrun altool --validate-app -f "$IPA" -t ios \
  --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"

log "Uploading build $BUILD_NUMBER"
xcrun altool --upload-app -f "$IPA" -t ios \
  --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"

echo
echo "Uploaded. Processing takes a few minutes; the build appears under"
echo "TestFlight in App Store Connect when it is done. Sandbox-test a"
echo "real purchase there before submitting for review."
