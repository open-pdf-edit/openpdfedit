#!/usr/bin/env bash
# App Store screenshots, at the sizes App Store Connect demands.
#
#   ./scripts/screenshots.sh setup          # boot, install, launch
#   ./scripts/screenshots.sh shot editing   # capture as editing.png
#   ./scripts/screenshots.sh setup --ipad   # the same, on a 13-inch iPad
#
# The app targets iPhone and iPad, so App Store Connect asks for both a
# 6.9-inch iPhone set and a 13-inch iPad set. The simulators chosen below
# render at exactly those pixel sizes, so a raw `simctl io screenshot` is
# already an acceptable upload — no scaling, which is worth caring about
# because a rescaled screenshot is rejected and the error does not say so.
#
# What it does not do is drive the app. Reaching an interesting screen
# means opening a document and tapping into a panel, and the only ways to
# automate that are a UI test target or a debug-only hook in the app —
# marketing machinery living in the shipped bundle. So: run `setup`, tap
# the app into the state you want, run `shot <name>`. The parts that are
# tedious and easy to get wrong — the device, the pixel size, the 9:41
# status bar, a full battery and no carrier noise — are handled.
#
# Note the app is a share-sheet handler, not the system default for PDFs;
# `simctl openurl file://…` opens Preview instead. Open documents through
# the app's own Open button.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$SCRIPT_DIR")"
OUT="$IOS_DIR/store/screenshots"
DERIVED="${IOS_DERIVED_DATA:-$IOS_DIR/.build}"
BUNDLE=com.openpdfedit.app

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }
die() { echo "screenshots.sh: $1" >&2; exit 1; }

DEVICE="iPhone 17 Pro Max"   # 1320 x 2868 — the 6.9-inch class
SUFFIX=""
ARGS=()
for arg in "$@"; do
  case "$arg" in
    --ipad) DEVICE="iPad Pro 13-inch (M5)"; SUFFIX="-ipad" ;;  # 2064 x 2752
    *)      ARGS+=("$arg") ;;
  esac
done
set -- "${ARGS[@]+"${ARGS[@]}"}"

udid() {
  xcrun simctl list devices available -j \
    | python3 -c "
import json, sys
want = sys.argv[1]
for devices in json.load(sys.stdin)['devices'].values():
    for d in devices:
        if d['name'] == want and d.get('isAvailable'):
            print(d['udid']); raise SystemExit
raise SystemExit(f'no simulator named {want!r}')
" "$DEVICE"
}

case "${1:-}" in
setup)
  UD="$(udid)"
  APP="$DERIVED/Build/Products/Debug-iphonesimulator/OpenPdfEdit.app"
  [ -d "$APP" ] || die "no app at $APP — run ./scripts/build-sim.sh first"

  log "Booting $DEVICE"
  xcrun simctl boot "$UD" 2>/dev/null || true
  xcrun simctl bootstatus "$UD" -b >/dev/null

  log "Installing"
  xcrun simctl install "$UD" "$APP"

  # Apple's own screenshots read 9:41. Full battery and steady bars keep
  # the eye on the app rather than on a dying phone.
  xcrun simctl status_bar "$UD" override \
    --time "9:41" \
    --batteryState charged --batteryLevel 100 \
    --cellularMode active --cellularBars 4 \
    --wifiMode active --wifiBars 3 \
    --dataNetwork wifi

  xcrun simctl launch "$UD" "$BUNDLE" >/dev/null
  open -a Simulator

  cat <<MSG

Ready on $DEVICE.

Tap the app into a state worth showing, then:

  ./scripts/screenshots.sh shot <name>${SUFFIX:+ --ipad}

Worth capturing, in the order the App Store shows them — the first is
the only one most people see:

  1  editing     a real document open, annotation visible
  2  pages       the page manager, with thumbnails
  3  forms       a form being filled, or a signature placed
  4  tools       the tool palette open
  5  iap-review  the account panel showing both credit packs
                 (App Store Connect asks for this one per purchase)
MSG
  ;;

shot)
  NAME="${2:-}"
  [ -n "$NAME" ] || die "usage: $0 shot <name>"
  UD="$(udid)"
  mkdir -p "$OUT"
  FILE="$OUT/${NAME}${SUFFIX}.png"
  xcrun simctl io "$UD" screenshot "$FILE" >/dev/null 2>&1
  SIZE="$(sips -g pixelWidth -g pixelHeight "$FILE" | awk '/pixel/{printf "%s ", $2}')"
  printf '%s  (%s)\n' "$FILE" "$(echo "$SIZE" | tr ' ' 'x' | sed 's/x$//')"
  ;;

*)
  sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
  exit 64
  ;;
esac
