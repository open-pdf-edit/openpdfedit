#!/usr/bin/env bash
# Task 9: packages the shared desktop SPA (apps/desktop, a Svelte 5 +
# SvelteKit app built with adapter-static/SPA-fallback) as this
# extension's UI, replacing the hand-written editor.html/editor.ts
# walking skeleton those files' own git history documents. Run after
# scripts/build-wasm.sh (the wasm-gen glue this pulls in via
# copy-vendor.sh below has to already exist) and package.json's
# `typecheck` step; see package.json's `build` script for the exact
# chain this is one link in.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$EXT_DIR")")"
DESKTOP_DIR="$WORKSPACE_DIR/apps/desktop"
DIST_DIR="$EXT_DIR/dist"

# --- Step 1: this extension's own entry (background.js) -----------------
#
# `vite build` here also empties dist/ first (Vite's default
# `emptyOutDir` behavior when outDir is inside the project root) and, via
# vite.config.js's `copyVendorAfterBuild` plugin, runs copy-vendor.sh
# (pdfium.js/pdfium.wasm + src/wasm-gen/*) as its own `writeBundle` hook
# — see that plugin's header comment for why that has to be a build hook
# and not just a separate package.json step. Doing this *before* Step 2
# below matters: it's what makes dist/'s emptying happen before the SPA
# output lands in it, not after.
echo "build-spa.sh: building apps/extension's own background.js entry..."
(cd "$EXT_DIR" && npx vite build)

# --- Step 2: the shared desktop SPA, wasm-flavored -----------------------
#
# VITE_BACKEND=wasm makes apps/desktop/src/lib/backend/index.ts's
# initBackend() resolve to wasm.ts's real WasmBackend instead of the
# default Tauri one (see that file's doc comment) — this is what makes
# the SPA built here able to run standalone in a chrome-extension://
# origin with no Tauri runtime present. A clean `build/` +`.svelte-kit/`
# avoids stale output from a previous plain (Tauri-flavored) build
# landing in dist/ below.
echo "build-spa.sh: building apps/desktop's SPA with VITE_BACKEND=wasm..."
(cd "$DESKTOP_DIR" && rm -rf build .svelte-kit && VITE_BACKEND=wasm npm run build)

if [ ! -f "$DESKTOP_DIR/build/index.html" ]; then
  echo "build-spa.sh: expected $DESKTOP_DIR/build/index.html to exist after the desktop build — did it fail silently?" >&2
  exit 1
fi

# --- Step 3: merge the SPA's output into dist/ ---------------------------
#
# `cp -R .../build/. dist/` (trailing `/.` copies build/'s *contents*,
# not a nested build/ directory) merges on top of what Step 1 already put
# in dist/ (background.js, manifest.json, pdfium.js/wasm, wasm-gen/) —
# none of those filenames collide with anything SvelteKit's adapter-static
# output produces (index.html, _app/, favicon.png, fonts/, icons/,
# version.json), so this is a pure union, not an overwrite of Step 1's
# files.
#
# `icons/` here is `apps/desktop/static/icons/` verbatim — the whole
# directory, not curated per app. That includes `scan-text.svg` (the OCR
# button's glyph), even though the OCR button itself never renders in the
# extension (`+page.svelte`'s `{#if backendKind !== "wasm"}` gate) —
# checked as part of the Phase 5 final-review fix round (M3): this icon
# isn't something extension-specific to drop, since there's no
# per-extension icon-selection step to drop it from; it's shared
# desktop/extension static-asset copying doing exactly what it always
# does. Excluding just this one file here would desync this dist/ from a
# plain `cp -R` of the desktop build for a single unused ~1KB SVG — not
# worth the special-casing.
echo "build-spa.sh: copying the desktop SPA build into $DIST_DIR..."
cp -R "$DESKTOP_DIR/build/." "$DIST_DIR/"

# --- Step 4: fix the CSP trap ---------------------------------------------
#
# See externalize-inline.mjs's own header for the full story: SvelteKit's
# adapter-static index.html has an inline bootstrap <script> that MV3's
# CSP (manifest.json: `script-src 'self' 'wasm-unsafe-eval'`, no
# `'unsafe-inline'`) forbids outright.
echo "build-spa.sh: externalizing inline <script> tag(s) in dist/index.html (MV3 CSP forbids inline script content)..."
node "$SCRIPT_DIR/externalize-inline.mjs"

# --- Step 5: re-assert the vendored/generated assets ----------------------
#
# Step 1's vite-plugin hook already ran this once, before the SPA copy in
# Step 3 — which never touches these filenames (see Step 3's comment), so
# this second call is redundant *today*. Kept as an explicit, named step
# anyway (matching task-9-brief.md's documented pipeline order literally:
# ...copy SPA output into dist -> externalize inline scripts -> copy-vendor)
# so this script's own steps stay self-describing without requiring a
# reader to go trace through vite.config.js's plugin to find where pdfium/
# wasm-gen actually land — and so this stays correct on its own even if a
# future change ever drops the Step-1 plugin. copy-vendor.sh's own copies
# are plain overwrites of identical content, so re-running it is harmless.
echo "build-spa.sh: re-asserting pdfium.js/pdfium.wasm + wasm-gen/* in $DIST_DIR..."
bash "$SCRIPT_DIR/copy-vendor.sh"

# --- Step 6: reject reserved filenames Chrome refuses to load -------------
#
# Chrome will not load an unpacked extension containing any file or
# directory whose name starts with "_" (reserved; only Chrome's own
# _locales/_metadata are allowed) — SvelteKit's default "_app" asset dir
# tripped exactly this, and NOTHING automated catches it: Playwright's
# --load-extension path skips the check, so the packaged zip only failed
# at a human's chrome://extensions load-unpacked. svelte.config.js sets
# appDir: "app" to avoid it; this guard makes the mistake unshippable.
bad_names="$(find "$DIST_DIR" -name '_*' ! -name '_locales' ! -path '*/_locales/*' -print | head -5)"
if [ -n "$bad_names" ]; then
  echo "build-spa.sh: ERROR — dist contains _-prefixed names Chrome refuses to load:" >&2
  echo "$bad_names" >&2
  exit 1
fi

echo "build-spa.sh: done — $DIST_DIR is a loadable unpacked extension"
