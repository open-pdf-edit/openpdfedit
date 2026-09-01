#!/usr/bin/env bash
# Set the product version everywhere it is declared.
#
#   ./scripts/set-version.sh 0.1.7
#   ./scripts/set-version.sh            # print the current version(s)
#
# The version lives in five files that nothing keeps in sync, and the
# consequence only shows up at a store: every storefront rejects a
# version number it has already accepted and none of them release one
# back, so a mismatch between the extension manifest and the desktop
# bundle costs a whole review cycle to discover. Editing five files by
# hand is how they drift; this is the one place that writes them.
#
# The Cargo workspace version is deliberately not touched. That numbers
# the library crates, which are versioned against each other rather than
# against the shipped product, and no store ever sees it.
set -euo pipefail

cd "$(dirname "$0")/.."

FILES=(
  apps/extension/package.json
  apps/extension/public/manifest.json
  apps/webapp/package.json
  apps/desktop/package.json
  apps/desktop/src-tauri/tauri.conf.json
)

current() { sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -1; }

if [ $# -eq 0 ]; then
  for f in "${FILES[@]}"; do printf '%-46s %s\n' "$f" "$(current "$f")"; done
  exit 0
fi

VERSION="$1"

# Three dot-separated numbers. Chrome and Edge accept up to four parts
# and reject anything with a letter in it, so a "0.2.0-beta" that seems
# fine locally is refused at upload time; catch it here instead.
if ! printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "set-version.sh: '$VERSION' is not major.minor.patch (digits only)" >&2
  exit 2
fi

for f in "${FILES[@]}"; do
  [ -f "$f" ] || { echo "set-version.sh: missing $f" >&2; exit 1; }
  # Only the first "version" key — package.json files list dependency
  # versions further down, and a global replace would rewrite those too.
  perl -0pi -e 's/"version"(\s*:\s*)"[^"]*"/"version"${1}"'"$VERSION"'"/' "$f"
  printf '%-46s %s\n' "$f" "$(current "$f")"
done
