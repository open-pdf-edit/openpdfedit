#!/usr/bin/env bash
# Refresh the vendored copies of @openapps/sdk and @openapps/ui.
#
# Why openpdfedit vendors these at all, when opencapture just symlinks the
# workspace: the desktop app is built and released on its own, from a tree
# that may not sit next to the platform repo. `file:` pointing outside the
# app directory would make the build depend on a path that only exists on a
# developer's machine.
#
# The cost of that choice is this script. A vendored copy is a snapshot, and
# a snapshot goes stale silently — the app keeps building, keeps passing its
# own tests, and quietly ships components several changes behind. That is
# exactly how <openapps-referral> shipped here with no `app-id` support and
# no way to point invite links at openpdfedit.com.
#
# Run it after any change to sdk/ts or ui-elements that this app should pick
# up, and commit the result.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VENDOR="$ROOT_DIR/openpdfedit/apps/desktop/vendor/openapps"

if [[ ! -d "$VENDOR" ]]; then
  echo "no vendor directory at $VENDOR" >&2
  exit 1
fi

# Build from source first: the vendored payload is `dist/` plus the package
# manifest, so copying an unbuilt tree would vendor whatever was last built
# rather than what is on disk now.
echo "building @openapps/sdk…"
(cd "$ROOT_DIR/sdk/ts" && npm run build >/dev/null)
echo "building @openapps/ui…"
(cd "$ROOT_DIR/ui-elements" && npm run build >/dev/null)

sync_one() {
  local name="$1" src="$2" dest="$VENDOR/$3"
  echo "vendoring $name → ${dest#"$ROOT_DIR"/}"
  # Replace rather than merge: a file deleted upstream must not survive here
  # as a stale module that still resolves. `node_modules/` inside the vendor
  # directory belongs to the consuming install and is deliberately untouched.
  rm -rf "$dest/dist"
  cp -R "$src/dist" "$dest/dist"
  cp "$src/package.json" "$dest/package.json"
}

sync_one "@openapps/sdk" "$ROOT_DIR/sdk/ts" "sdk"
sync_one "@openapps/ui" "$ROOT_DIR/ui-elements" "ui"

echo
echo "done. Review with: git -C \"$ROOT_DIR\" status --short openpdfedit/apps/desktop/vendor"
