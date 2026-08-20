#!/usr/bin/env bash
# Downloads the prebuilt PDFium dynamic library for the current platform
# from bblanchon/pdfium-binaries into .vendor/pdfium/, so pdfium-render can
# bind to it at dev/test/build time. See PLAN.md §5: PDFium is BSD-3, these
# binaries are the standard way every Rust pdfium-render project ships it —
# we do NOT vendor the binary in git (see .gitignore); this script is the
# reproducible fetch step, run it locally and in CI.
set -euo pipefail

# Pin a specific pdfium-binaries release tag rather than "latest" so builds
# are reproducible; bump this deliberately when upgrading.
PDFIUM_TAG="chromium/7961"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/.vendor/pdfium"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin)
    case "$arch" in
      arm64) asset="pdfium-mac-arm64.tgz" ;;
      x86_64) asset="pdfium-mac-x64.tgz" ;;
      *) echo "unsupported macOS arch: $arch" >&2; exit 1 ;;
    esac
    lib_glob="lib/libpdfium.dylib"
    ;;
  Linux)
    case "$arch" in
      x86_64) asset="pdfium-linux-x64.tgz" ;;
      aarch64) asset="pdfium-linux-arm64.tgz" ;;
      *) echo "unsupported Linux arch: $arch" >&2; exit 1 ;;
    esac
    lib_glob="lib/libpdfium.so"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    asset="pdfium-win-x64.tgz"
    lib_glob="bin/pdfium.dll"
    ;;
  *)
    echo "unsupported OS: $os (Windows CI should invoke this via a bash shell, e.g. git-bash)" >&2
    exit 1
    ;;
esac

if [ -f "$VENDOR_DIR/VERSION" ] && grep -q "BUILD=${PDFIUM_TAG##*/}" "$VENDOR_DIR/VERSION" 2>/dev/null; then
  echo "pdfium ${PDFIUM_TAG} already present at $VENDOR_DIR — skipping download"
  exit 0
fi

echo "fetching pdfium ${PDFIUM_TAG} (${asset})..."
mkdir -p "$VENDOR_DIR"
tmpfile="$(mktemp)"
url="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_TAG}/${asset}"
curl -fsSL -o "$tmpfile" "$url"
tar xzf "$tmpfile" -C "$VENDOR_DIR"
rm -f "$tmpfile"

echo "pdfium library: $VENDOR_DIR/$lib_glob"
