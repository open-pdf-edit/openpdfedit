#!/usr/bin/env bash
# Build and publish the web app (and, with --site, the marketing site).
#
#   ./scripts/deploy-webapp.sh              # build + deploy the web app
#   ./scripts/deploy-webapp.sh --site       # the marketing site as well
#   ./scripts/deploy-webapp.sh --dry-run    # show what would change
#
# This exists because the two steps used to be two commands on two
# lines. A shell runs the second whether or not the first succeeded, so
# a failed build published the *previous* build without saying anything
# — which happened, and the deploy looked entirely normal. Here the
# build failing ends the script.
#
# Override the target with HOST=user@example.com if it ever moves.
set -euo pipefail

cd "$(dirname "$0")/.."

HOST="${HOST:-root@104.36.65.54}"
APP_ROOT="/var/www/openpdfedit-app"
SITE_ROOT="/var/www/openpdfedit"

WITH_SITE=""
# Expanded below as ${RSYNC_EXTRA[@]+"${RSYNC_EXTRA[@]}"}, not the bare
# form. macOS still ships bash 3.2, where expanding an empty array under
# `set -u` is an unbound-variable error rather than nothing — so the
# plain spelling worked on every run *with* flags and failed on the
# ordinary run without them, which is the one everybody makes.
RSYNC_EXTRA=()
for arg in "$@"; do
  case "$arg" in
    --site) WITH_SITE=1 ;;
    --dry-run) RSYNC_EXTRA+=(--dry-run --itemize-changes); DRY_RUN=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }

log "Building the web app"
npm --prefix apps/webapp run build

# Belt and braces: the build script already fails loudly, but a deploy
# that ships an empty or half-written tree is worse than one that stops.
for required in index.html service-worker.js pdfium.wasm wasm-gen/openpdfedit_wasm_bg.wasm ocr/eng.traineddata; do
  [ -f "apps/webapp/dist/$required" ] || {
    echo "deploy-webapp.sh: apps/webapp/dist/$required is missing — refusing to deploy" >&2
    exit 1
  }
done

BUILD_ID="$(grep -o 'openpdfedit-[0-9a-f]*' apps/webapp/dist/service-worker.js | head -1)"
log "Publishing $BUILD_ID to $HOST:$APP_ROOT"

# --delete matters: JavaScript filenames are content hashes, so without
# it every deploy leaves the previous build's chunks behind for good.
#
# The first run moves ~57 MB, most of it the OCR engine and language
# data — twelve core variants, because tesseract.js picks one at runtime
# from what the browser supports. Every run after that moves almost
# nothing: those files never change, and rsync only sends what differs.
rsync -az --delete --human-readable --stats ${RSYNC_EXTRA[@]+"${RSYNC_EXTRA[@]}"} \
  apps/webapp/dist/ "$HOST:$APP_ROOT/"

if [ -n "$WITH_SITE" ]; then
  log "Publishing the marketing site to $HOST:$SITE_ROOT"
  rsync -az --delete --human-readable ${RSYNC_EXTRA[@]+"${RSYNC_EXTRA[@]}"} site/ "$HOST:$SITE_ROOT/"
fi

if [ -n "${DRY_RUN:-}" ]; then
  log "Dry run — nothing was changed"
  exit 0
fi

log "Checking what is actually live"
# Fetched, not assumed. The cache name is a digest of the build, so this
# is the one check that proves the deploy landed rather than that rsync
# exited zero.
LIVE="$(curl -fsS --max-time 20 https://app.openpdfedit.com/service-worker.js |
  grep -o 'openpdfedit-[0-9a-f]*' | head -1 || true)"
if [ "$LIVE" = "$BUILD_ID" ]; then
  echo "  live: $LIVE ✓"
else
  echo "  live: ${LIVE:-<no response>}, expected $BUILD_ID" >&2
  echo "  the files went up but the site is serving something else — check nginx's root" >&2
  exit 1
fi

echo
echo "Visitors pick this up on their next load; the service worker's cache"
echo "name changed with the build. A hard reload settles any doubt."
