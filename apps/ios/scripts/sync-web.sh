#!/usr/bin/env bash
# Copies the built web app into the iOS app bundle's payload.
#
# The editor is not fetched at runtime. Bundling it is what makes the app
# work with no network at all — the product's central claim, and one a
# wrapper around a remote URL could not honour. It is also the difference
# between an app and a bookmark, which is the test App Review applies under
# guideline 4.2.
#
# `www/` is generated and gitignored. Rebuild it whenever the web app
# changes; the Xcode build copies whatever is there at the time.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$IOS_DIR")")"
# The app's own build of the same SPA, not the website's dist. The site is
# served from openpdfedit.com/app, so its build bakes BASE_PATH=/app into
# every chunk URL and into the router; inside the bundle, where the scheme
# handler serves www/ at the root of openpdfedit://localhost, every one of
# those URLs is a 404 and the page renders nothing at all. Same script,
# BASE_PATH empty, its own output directory so the two cannot overwrite
# each other.
SRC="${IOS_WEB_DIR:-$IOS_DIR/.build/web}"
DEST="$IOS_DIR/www"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }

if [ "${1:-}" = "--build" ] || [ ! -d "$SRC" ]; then
  log "Building the web app with no base path"
  DIST_DIR="$SRC" BASE_PATH= bash "$WORKSPACE_DIR/apps/webapp/scripts/build.sh"
fi

[ -f "$SRC/index.html" ] || {
  echo "sync-web.sh: $SRC/index.html missing — run apps/webapp/scripts/build.sh" >&2
  exit 1
}

log "Copying $SRC into $DEST"
rm -rf "$DEST"
mkdir -p "$DEST"
cp -R "$SRC/." "$DEST/"

# The service worker has no job here and one real cost: it would cache the
# bundled files a second time, in a store that survives an app update, so a
# new build could go on serving the old one. The scheme handler already
# reads straight from the bundle, offline, with no cache to go stale.
rm -f "$DEST/service-worker.js"
# Crawler files for a host that does not exist.
rm -f "$DEST/robots.txt" "$DEST/sitemap.xml"

# The registration is injected into index.html by the web app's build, so
# removing the file is not enough — the page would still ask for it and log
# a failure on every launch.
if grep -q "serviceWorker" "$DEST/index.html"; then
  log "Removing the service worker registration from index.html"
  node - "$DEST/index.html" <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
const file = process.argv[2];
let html = readFileSync(file, "utf8");
const before = html.length;
html = html.replace(
  /if \('serviceWorker' in navigator\) \{[\s\S]*?\n\}\n/,
  "",
);
if (html.length === before) {
  console.error("sync-web.sh: the service worker registration block was not where it was expected");
  process.exit(1);
}
writeFileSync(file, html);
NODE
fi

grep -q "serviceWorker" "$DEST/index.html" &&
  { echo "sync-web.sh: index.html still registers a service worker" >&2; exit 1; }

# The web build injects ~400 words of marketing copy into the page for
# crawlers (APP-96), and the app moves it into the empty state, where on a
# phone it is the entire first screen: a wall of prose above the Open
# button, with no editor in sight. In a browser that is the product page
# doing its job. In an app binary it is the first thing a reviewer sees,
# and "a repackaged website" is guideline 4.2, the most common rejection
# there is. No crawler reads this copy inside an app bundle, so it goes.
if grep -q 'id="landing-copy"' "$DEST/index.html"; then
  log "Removing the crawler copy from index.html"
  node - "$DEST/index.html" <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
const file = process.argv[2];
let html = readFileSync(file, "utf8");
const before = html.length;
html = html.replace(/<section id="landing-copy"[\s\S]*?<\/section>\s*/, "");
if (html.length === before) {
  console.error("sync-web.sh: the landing copy was not where it was expected");
  process.exit(1);
}
writeFileSync(file, html);
NODE
fi

grep -q 'id="landing-copy"' "$DEST/index.html" &&
  { echo "sync-web.sh: index.html still carries the crawler copy" >&2; exit 1; }

grep -q 'base: "/app"' "$DEST/index.html" &&
  { echo "sync-web.sh: index.html still carries the site's /app base — the app would render nothing" >&2; exit 1; }

# The bundle is a binary Apple reviews: a <script> from another origin in
# it is a question at review, and there is no reason for one to be here.
node "$WORKSPACE_DIR/apps/extension/scripts/drop-remote-scripts.mjs" "$DEST"

log "Done"
echo "  files: $(find "$DEST" -type f | wc -l | tr -d ' ')"
echo "  size:  $(du -sh "$DEST" | cut -f1)"
