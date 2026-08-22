#!/usr/bin/env bash
# Builds the OpenPdfEdit Linux bundles (.AppImage and .deb) end-to-end:
# fetches PDFium, installs frontend deps, runs `tauri build`, and copies
# the finished artifacts somewhere obvious.
#
# The Linux counterpart to scripts/build-dmg.sh (macOS) and
# scripts/build-installer.ps1 (Windows). Linux isn't a v1 release target
# (PLAN.md §2), but the stack makes it nearly free and CI keeps it green
# — this script is what turns that into something a person can actually
# download and run.
#
# Usage:
#   ./scripts/build-linux.sh                 # artifacts into ./dist
#   ./scripts/build-linux.sh --out ~/Desktop
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/dist"

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT_DIR="$2"; shift 2 ;;
    --out=*) OUT_DIR="${1#--out=}"; shift ;;
    -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
die() { printf '\nerror: %s\n' "$1" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "this script builds Linux bundles and must run on Linux"

log "Checking prerequisites"
command -v cargo >/dev/null 2>&1 || die "Rust not found — install it from https://rustup.rs, then re-run"
command -v npm >/dev/null 2>&1 || die "Node.js/npm not found — install Node, then re-run"

# Tauri's Linux backend builds against the system webview. Checking here
# turns an unreadable wall of C linker errors deep in a -sys crate into
# one line naming the packages to install.
if command -v pkg-config >/dev/null 2>&1; then
  pkg-config --exists webkit2gtk-4.1 || die "libwebkit2gtk-4.1-dev is missing. On Debian/Ubuntu:
  sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf"
else
  echo "  (pkg-config not found — skipping the webkit2gtk check; the build will fail later if it's missing)"
fi

log "Fetching PDFium (no-op if already present)"
"$ROOT_DIR/scripts/fetch-pdfium.sh"

log "Installing frontend dependencies"
( cd "$ROOT_DIR/apps/desktop" && npm install )

log "Building release bundles"
( cd "$ROOT_DIR/apps/desktop" && npm run tauri build -- --bundles appimage,deb )

mkdir -p "$OUT_DIR"
found=0
while IFS= read -r artifact; do
  cp "$artifact" "$OUT_DIR/"
  echo "  $(basename "$artifact")  ($(du -h "$artifact" | cut -f1))"
  found=1
done < <(find "$ROOT_DIR/target/release/bundle" -maxdepth 2 \( -name '*.AppImage' -o -name '*.deb' \) 2>/dev/null)

[ "$found" = "1" ] || die "build finished but no .AppImage or .deb was found under target/release/bundle"

log "Done"
echo "  artifacts in: $OUT_DIR"
echo
echo "The .AppImage needs no installation — mark it executable and run it:"
echo "  chmod +x \"$OUT_DIR\"/*.AppImage && \"$OUT_DIR\"/*.AppImage"
