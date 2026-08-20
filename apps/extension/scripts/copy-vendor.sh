#!/usr/bin/env bash
# Copies vendored/generated files that no `vite build` in this pipeline
# ever sees into apps/extension/dist/, so the packaged extension (Task 9:
# the shared desktop SPA, built separately by scripts/build-spa.sh — see
# that script and apps/desktop/src/lib/backend/wasm.ts) has everything it
# fetches at runtime, at the fixed paths wasm.ts's asset-layout contract
# expects:
#
#   - .vendor/pdfium-wasm/release/node/{pdfium.js,pdfium.wasm} — wasm.ts's
#     `loadPdfiumScript` injects `<script src="/pdfium.js">` itself at
#     runtime (there's no static HTML `<script>` tag for it to rely on
#     anymore — that was editor.html's approach, before Task 9 replaced
#     the hand-written editor.html/editor.ts skeleton with the shared
#     SPA). Neither `apps/extension`'s own `vite build` (background.js
#     only) nor `apps/desktop`'s SPA build ever reference pdfium.js/.wasm
#     through an import Rollup can see — a runtime-injected classic
#     `<script src>` is neither a JS/TS import nor an asset reference
#     inside one — so nothing copies these two files unless this script
#     does it explicitly. See wasm.ts's own header comment for why
#     pdfium.js/pdfium.wasm specifically, not the .esm.* or .std.*
#     variants also present in that release tree.
#
#   - src/wasm-gen/{openpdfedit_wasm.js,openpdfedit_wasm_bg.wasm} — wasm.ts
#     dynamically imports `/wasm-gen/openpdfedit_wasm.js` via a runtime
#     string specifier + `/* @vite-ignore */` (see its
#     `WASM_GLUE_MODULE_PATH`/asset-layout comment for why that import is
#     deliberately left unresolved by the bundler), so neither
#     `apps/desktop`'s build nor `apps/extension`'s own `vite build` ever
#     bundles or content-hashes it — it has to exist as a real, plain-named
#     file at this fixed dist path for that runtime import to resolve at
#     all, which is what this script guarantees. `src/wasm-gen/` also has
#     two `.d.ts` files (`openpdfedit_wasm.d.ts`,
#     `openpdfedit_wasm_bg.wasm.d.ts`) generated alongside these by
#     `build-wasm.sh` — type declarations only, never fetched by anything
#     at runtime (wasm.ts's own hand-typed `WasmSessionHandle` interface is
#     what it actually type-checks against — see that file's doc for why),
#     so this script copies the two runtime files by name rather than
#     globbing `openpdfedit_wasm*` and shipping ~42KB of dead `.d.ts` bytes
#     in every packaged dist/zip for nothing (Phase 5 final-review fix
#     round, M2).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$EXT_DIR")")"

VENDOR_NODE_DIR="$WORKSPACE_DIR/.vendor/pdfium-wasm/release/node"
WASM_GEN_DIR="$EXT_DIR/src/wasm-gen"
DIST_DIR="$EXT_DIR/dist"

if [ ! -f "$VENDOR_NODE_DIR/pdfium.js" ] || [ ! -f "$VENDOR_NODE_DIR/pdfium.wasm" ]; then
  echo "copy-vendor.sh: $VENDOR_NODE_DIR/pdfium.js + pdfium.wasm not found — run scripts/fetch-pdfium-wasm.sh first" >&2
  exit 1
fi

if [ ! -d "$DIST_DIR" ]; then
  echo "copy-vendor.sh: $DIST_DIR not found — run 'vite build' before this script" >&2
  exit 1
fi

cp "$VENDOR_NODE_DIR/pdfium.js" "$DIST_DIR/pdfium.js"
cp "$VENDOR_NODE_DIR/pdfium.wasm" "$DIST_DIR/pdfium.wasm"

mkdir -p "$DIST_DIR/wasm-gen"
cp "$WASM_GEN_DIR/openpdfedit_wasm.js" "$DIST_DIR/wasm-gen/"
cp "$WASM_GEN_DIR/openpdfedit_wasm_bg.wasm" "$DIST_DIR/wasm-gen/"

echo "copy-vendor.sh: copied pdfium.js/pdfium.wasm and wasm-gen/* into $DIST_DIR"
