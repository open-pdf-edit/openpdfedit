#!/usr/bin/env bash
# Builds the OpenPdfEdit release .dmg end-to-end: fetches PDFium, installs
# frontend deps, runs `tauri build`, and copies the finished .dmg out to
# a plain, memorable location — by default a build-target directory
# *outside* this checkout entirely, and an output copy on your Desktop.
#
# Why the build-target lives outside the repo by default: this checkout
# commonly lives on a shared/virtual-machine mount whose backing disk is
# small (see DEPLOYMENT.md's "Disk space note" and the mmap failure it
# documents for building directly on such a mount). Run this script from
# a Terminal on the real machine — not inside a VM sharing this folder —
# and every byte of the Rust build (multiple GB, mostly the Tauri/webview
# dependency tree) lands on that machine's own disk instead, under
# $BUILD_TARGET_DIR (default: ~/.cache/openpdfedit-build). Add --clean to
# remove that directory again once the build succeeds, if you'd rather
# not keep it around between runs.
#
# Usage:
#   ./scripts/build-dmg.sh                 # build, copy .dmg to ~/Desktop
#   ./scripts/build-dmg.sh --clean         # ...then delete the build-target dir
#   ./scripts/build-dmg.sh --out ~/Downloads
#   BUILD_TARGET_DIR=/some/other/disk ./scripts/build-dmg.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_TARGET_DIR="${BUILD_TARGET_DIR:-$HOME/.cache/openpdfedit-build}"
OUT_DIR="$HOME/Desktop"
CLEAN_AFTER=0
MIN_FREE_GB="${MIN_FREE_GB:-6}"

while [ $# -gt 0 ]; do
  case "$1" in
    --clean) CLEAN_AFTER=1; shift ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --out=*) OUT_DIR="${1#--out=}"; shift ;;
    -h|--help)
      cat <<'EOF'
Usage:
  ./scripts/build-dmg.sh                 build, copy the .dmg to ~/Desktop
  ./scripts/build-dmg.sh --clean         ...then delete the build-target dir
  ./scripts/build-dmg.sh --out ~/Downloads
  BUILD_TARGET_DIR=/some/other/disk ./scripts/build-dmg.sh

Run this on the real machine, not inside a VM sharing this checkout —
see the comment at the top of this file for why.
EOF
      exit 0
      ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf '\nerror: %s\n' "$1" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "this script builds a macOS .app/.dmg bundle and must run on macOS"

log "Checking prerequisites"
command -v cargo >/dev/null 2>&1 || die "Rust not found — install it from https://rustup.rs, then re-run"
command -v npm >/dev/null 2>&1 || die "Node.js/npm not found — install Node (e.g. 'brew install node'), then re-run"
xcode-select -p >/dev/null 2>&1 || die "Xcode Command Line Tools not found — run 'xcode-select --install', then re-run"
echo "  rust:  $(rustc --version)"
echo "  node:  $(node --version)"
echo "  npm:   $(npm --version)"

# A partial build can plausibly eat several GB; fail fast with a clear
# message instead of a confusing mid-build "No space left on device" deep
# in some dependency's build script.
free_gb="$(df -g "$HOME" | awk 'NR==2 {print $4}')"
if [ "$free_gb" -lt "$MIN_FREE_GB" ]; then
  die "only ${free_gb}GB free on this disk; want at least ${MIN_FREE_GB}GB free before a release build. Free up space and re-run."
fi
echo "  disk:  ${free_gb}GB free on $HOME's volume"

log "Fetching PDFium (no-op if already present)"
"$ROOT_DIR/scripts/fetch-pdfium.sh"

log "Installing frontend dependencies"
( cd "$ROOT_DIR/apps/desktop" && npm install )

run_tauri_build() {
  (
    cd "$ROOT_DIR/apps/desktop"
    export CARGO_TARGET_DIR="$BUILD_TARGET_DIR"
    npm run tauri build -- --bundles dmg
  )
}

# `bundle_dmg.sh` (Tauri's own packaging script, run after the Rust/Swift
# build succeeds) drives Finder via AppleScript to lay out the .dmg
# window, and creates a temporary read-write disk image along the way.
# Two things reliably break that step, both unrelated to this repo's
# code and both easy to clear: a disk image left mounted by an earlier
# interrupted attempt (hdiutil then fails the new `create` with
# "Resource busy"), and stray `rw.*.dmg` staging files from the same. Do
# this before every attempt, not just on retry, since it's a no-op when
# there's nothing to clean.
cleanup_stale_dmg_state() {
  while IFS= read -r dev; do
    [ -n "$dev" ] && hdiutil detach "$dev" -force >/dev/null 2>&1 || true
  done < <(hdiutil info 2>/dev/null | awk '/OpenPdfEdit/ { for (i = 1; i <= NF; i++) if ($i ~ /^\/dev\/disk/) print $i }')
  find "$BUILD_TARGET_DIR/release/bundle/dmg" -maxdepth 1 -name 'rw.*.dmg' -delete 2>/dev/null || true
}

log "Building release bundle (CARGO_TARGET_DIR=$BUILD_TARGET_DIR)"
echo "  first build compiles the whole Rust dependency tree — a few minutes;"
echo "  reruns with the same BUILD_TARGET_DIR are much faster."
mkdir -p "$BUILD_TARGET_DIR"
cleanup_stale_dmg_state

# Where tauri-bundler writes the packaging script it's about to run —
# deterministic, and (this matters below) it exists on disk by the time
# the *first* attempt fails, even though that attempt never got to run
# it successfully.
dmg_script="$BUILD_TARGET_DIR/release/bundle/dmg/bundle_dmg.sh"

# Tauri's own error for this step ("failed to run .../bundle_dmg.sh") is
# a wrapper around a bare OS-level spawn failure — it never shows what
# actually went wrong. That phrasing specifically (as opposed to "exited
# with status N") means the OS refused to *start* the process at all,
# which narrows it to two causes, both fixable on the script file
# itself: it isn't marked executable, or Gatekeeper has it quarantined.
# Fix whichever applies and retry; if it fails a second time even after
# that, run the script directly (bypassing tauri-cli's wrapper
# entirely) so its real stderr finally reaches the terminal instead of
# staying hidden.
diagnose_and_fix_dmg_script() {
  if [ ! -f "$dmg_script" ]; then
    echo "  (bundle_dmg.sh was never written — the failure is earlier than dmg packaging; see the full log)"
    return 1
  fi
  local fixed=0
  if [ ! -x "$dmg_script" ]; then
    echo "  bundle_dmg.sh exists but isn't marked executable — fixing (chmod +x)."
    chmod +x "$dmg_script"
    fixed=1
  fi
  if xattr -p com.apple.quarantine "$dmg_script" >/dev/null 2>&1; then
    echo "  bundle_dmg.sh is quarantined by Gatekeeper — clearing it (xattr -d)."
    xattr -d com.apple.quarantine "$dmg_script" 2>/dev/null || true
    fixed=1
  fi
  [ "$fixed" = "1" ]
}

build_log="$(mktemp)"
if ! run_tauri_build 2>&1 | tee "$build_log"; then
  if ! grep -qiE 'bundle_dmg\.sh|hdiutil|Resource busy' "$build_log"; then
    die "build failed — see output above. Full log: $build_log"
  fi

  log "The Rust/Tauri build itself succeeded — only .dmg packaging failed"
  cleanup_stale_dmg_state
  diagnose_and_fix_dmg_script || true
  echo "  Retrying..."
  sleep 1

  if ! run_tauri_build 2>&1 | tee "$build_log"; then
    cat <<'EOF'

Still failing after fixing permissions/quarantine and retrying. Running
bundle_dmg.sh directly now, bypassing Tauri's own wrapper, so its actual
error (not just "failed to run") reaches the terminal:
EOF
    if [ -f "$dmg_script" ]; then
      ( cd "$(dirname "$dmg_script")" && bash "$dmg_script" ) || true
    fi
    cat <<EOF

If the error above doesn't explain it, the next most likely cause is
Terminal not having permission to automate Finder yet — bundle_dmg.sh
uses Finder (via AppleScript) to lay out the .dmg window, and a Mac
that's never built a .dmg with this account before has never had reason
to grant that. Check/fix it at:
  System Settings -> Privacy & Security -> Automation -> [your terminal app] -> enable "Finder"
If your terminal isn't listed there yet, run this once so macOS prompts
you, click Allow, then re-run this script:
  osascript -e 'tell application "Finder" to get name'
EOF
    die "dmg bundling failed — see the actual error above. Full build log: $build_log"
  fi
fi
rm -f "$build_log"

dmg_path="$(find "$BUILD_TARGET_DIR/release/bundle/dmg" -maxdepth 1 -name '*.dmg' -print -quit)"
[ -n "$dmg_path" ] || die "build finished but no .dmg was found under $BUILD_TARGET_DIR/release/bundle/dmg"

mkdir -p "$OUT_DIR"
dest="$OUT_DIR/$(basename "$dmg_path")"
cp "$dmg_path" "$dest"

log "Done"
echo "  .dmg:  $dest"
echo "  size:  $(du -h "$dest" | cut -f1)"
echo
echo "This build is unsigned (no code-signing/notarization set up yet — see"
echo "DEPLOYMENT.md). If macOS refuses to open it with a damaged/unverified-"
echo "developer dialog, run:"
echo "  xattr -cr \"$dest\""

if [ "$CLEAN_AFTER" = "1" ]; then
  log "Cleaning up (--clean)"
  rm -rf "$BUILD_TARGET_DIR"
  echo "  removed $BUILD_TARGET_DIR"
fi
