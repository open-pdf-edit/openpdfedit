#!/usr/bin/env bash
# Set the product version everywhere it is declared.
#
#   ./scripts/set-version.sh 0.1.7
#   ./scripts/set-version.sh            # print the current version(s)
#
# The version lives in five JSON files plus the Xcode project, and
# nothing keeps them in sync. The
# consequence only shows up at a store: every storefront rejects a
# version number it has already accepted and none of them release one
# back, so a mismatch between the extension manifest and the desktop
# bundle costs a whole review cycle to discover. Editing five files by
# hand is how they drift; this is the one place that writes them.
#
# The iOS app is the odd one out twice over: its version is a build
# setting rather than a JSON key, and it also carries a build number that
# App Store Connect requires to be unique *within* a version — two uploads
# of 0.1.10 need build 1 and build 2. So MARKETING_VERSION is set from
# here and CURRENT_PROJECT_VERSION is left alone, to be bumped per upload.
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

IOS_PROJECT=apps/ios/OpenPdfEdit.xcodeproj/project.pbxproj

current() { sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -1; }
ios_current() { sed -n 's/.*MARKETING_VERSION = \([^;]*\);.*/\1/p' "$IOS_PROJECT" | head -1; }

if [ $# -eq 0 ]; then
  for f in "${FILES[@]}"; do printf '%-46s %s\n' "$f" "$(current "$f")"; done
  printf '%-46s %s\n' "$IOS_PROJECT" "$(ios_current)"
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

# Every build configuration in the project, app and test target alike: an
# archive uploaded from the wrong one would carry a version nobody set,
# and there is no reason for the test bundle to disagree.
perl -pi -e 's/MARKETING_VERSION = [^;]*;/MARKETING_VERSION = '"$VERSION"';/' "$IOS_PROJECT"
printf '%-46s %s\n' "$IOS_PROJECT" "$(ios_current)"

# The marketing site carries the version twice more — in the JSON-LD that
# answer engines read, and in the footer. It is a separate repository
# since 11 September 2026, so this reaches across to it when it is
# checked out beside this one, and says so plainly when it is not.
# Leaving that to whoever bumps next is how this script would come to
# cause the drift it exists to prevent.
SITE="$(cd "$(dirname "$0")/../../openpdfedit-website" 2>/dev/null && pwd || true)"
if [ -n "$SITE" ] && [ -f "$SITE/index.html" ]; then
  perl -pi -e 's{<span class="mono">v[0-9.]+</span>}{<span class="mono">v'"$VERSION"'</span>}' \
    "$SITE/index.html" "$SITE/privacy.html"
  python3 "$SITE/scripts/build-jsonld.py"
  printf '%-46s %s\n' "openpdfedit-website/index.html" \
    "$(sed -n 's/.*"softwareVersion": "\([^"]*\)".*/\1/p' "$SITE/index.html" | head -1)"
else
  echo
  echo "  NOTE: openpdfedit-website is not checked out beside this repo, so the" >&2
  echo "        site still shows the old version. Bump it there and redeploy:" >&2
  echo "          openpdfedit-website/deploy.sh" >&2
fi
