#!/usr/bin/env bash
# Put the Supporter tools' real implementations in place for an official
# build, or check that they are.
#
#   ./scripts/use-premium.sh           # copy them into the crates
#   ./scripts/use-premium.sh --check   # exit non-zero if they are not there
#
# Watermark and OCR are unlocked by redeeming credits, and their
# implementations live in a private repository checked out as a submodule
# at vendor-private/openpdfedit-premium. See docs/PREMIUM.md for why the
# contents are swapped rather than the dependency being made optional —
# Cargo reads every path dependency's manifest before it looks at
# features, so a missing path fails the build even when the feature is
# off.
#
# `--check` exists because the failure mode is silent: a release built
# without this having run ships a watermark button that charges 1,000
# credits and then reports the tool is missing. The release workflow runs
# it and refuses to publish if it fails.
set -euo pipefail

cd "$(dirname "$0")/.."

PREMIUM="vendor-private/openpdfedit-premium"
CRATES=(openpdfedit-watermark openpdfedit-ocr)

# The marker the stub carries and the real implementation does not. Chosen
# over a file hash so that editing the private code never breaks this.
STUB_MARKER="NotIncluded"

mode="copy"
for arg in "$@"; do
  case "$arg" in
    --check) mode="check" ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

if [ "$mode" = "check" ]; then
  missing=()
  for crate in "${CRATES[@]}"; do
    if grep -q "$STUB_MARKER" "crates/$crate/src/lib.rs" 2>/dev/null; then
      missing+=("$crate")
    fi
  done
  if [ ${#missing[@]} -gt 0 ]; then
    echo "use-premium.sh: still building against the stubs: ${missing[*]}" >&2
    echo "  This build would show the paid buttons, take 1000 credits, and then" >&2
    echo "  report the tool is missing. Run:" >&2
    echo "    git submodule update --init && ./scripts/use-premium.sh" >&2
    exit 1
  fi
  echo "use-premium.sh: the Supporter implementations are in place"
  exit 0
fi

if [ ! -d "$PREMIUM/crates" ]; then
  echo "use-premium.sh: $PREMIUM is not checked out." >&2
  echo "  git submodule update --init   (needs access to the private repository)" >&2
  exit 1
fi

for crate in "${CRATES[@]}"; do
  src="$PREMIUM/crates/$crate/src"
  [ -d "$src" ] || { echo "use-premium.sh: $src is missing" >&2; exit 1; }
  rsync -a --delete "$src/" "crates/$crate/src/"
  echo "  $crate <- $PREMIUM"
done

# The UI halves, which live beside the rest of the app rather than in a
# crate.
rsync -a "$PREMIUM/ui/WatermarkPanel.svelte" apps/desktop/src/lib/WatermarkPanel.svelte
rsync -a "$PREMIUM/ui/ocr-browser.ts" apps/desktop/src/lib/backend/ocr-browser.ts
echo "  ui <- $PREMIUM"

echo "use-premium.sh: done — this checkout now builds the full product."
echo
echo "These files are tracked, so \`git status\` will show them modified."
echo "That is expected; do not commit them."
