#!/usr/bin/env bash
# Builds crates/openpdfedit-wasm for wasm32-unknown-unknown and generates
# the wasm-bindgen JS/TS glue into src/wasm-gen/. Run before `vite build`
# (see package.json's `build` script) — Vite never touches Rust, it only
# copies the already-generated output as a static asset.
set -euo pipefail

export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"

# The repo lives on a shared VM mount, which produces spurious archive/GC
# errors when used as the cargo target dir directly (see
# opencapture/docs/build-environment.md). Build off-mount, then only the
# final .wasm crosses back onto the mount via wasm-bindgen's --out-dir.
: "${CARGO_TARGET_DIR:=/tmp/openpdfedit-target}"
export CARGO_TARGET_DIR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$EXT_DIR")")"
OUT_DIR="$EXT_DIR/src/wasm-gen"

# wasm-bindgen requires the `wasm-bindgen` crate version (pinned exactly
# in "$WORKSPACE_DIR/Cargo.toml") and the `wasm-bindgen-cli` *binary*
# version installed below to match exactly — the JS-glue schema is
# versioned, and a mismatch fails later with a cryptic schema error, not
# a clear "wrong version" one. This used to be documented only in an
# untracked `.superpowers/` task report (not visible from a fresh clone,
# since that directory isn't committed) — moved here, next to the
# command that actually needs it, after that gap was flagged in review.
EXPECTED_WASM_BINDGEN_VERSION="0.2.126"
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen-cli is not installed (or not on PATH)." >&2
  echo "Install the version pinned in $WORKSPACE_DIR/Cargo.toml:" >&2
  echo "  cargo install wasm-bindgen-cli --version $EXPECTED_WASM_BINDGEN_VERSION --force" >&2
  exit 1
fi
ACTUAL_WASM_BINDGEN_VERSION="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$ACTUAL_WASM_BINDGEN_VERSION" != "$EXPECTED_WASM_BINDGEN_VERSION" ]; then
  echo "error: installed wasm-bindgen-cli is $ACTUAL_WASM_BINDGEN_VERSION, but this workspace's Cargo.toml pins wasm-bindgen = \"=$EXPECTED_WASM_BINDGEN_VERSION\"." >&2
  echo "The generated JS glue is versioned against the CLI; a mismatch fails at generation time with an unhelpful schema error, not this one." >&2
  echo "Fix: cargo install wasm-bindgen-cli --version $EXPECTED_WASM_BINDGEN_VERSION --force" >&2
  exit 1
fi

cargo build -p openpdfedit-wasm --target wasm32-unknown-unknown --profile wasm-release --manifest-path "$WORKSPACE_DIR/Cargo.toml"

wasm-bindgen \
  --target web \
  --out-dir "$OUT_DIR" \
  --out-name openpdfedit_wasm \
  "$CARGO_TARGET_DIR/wasm32-unknown-unknown/wasm-release/openpdfedit_wasm.wasm"

if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -O3 -o "$OUT_DIR/openpdfedit_wasm_bg.wasm" "$OUT_DIR/openpdfedit_wasm_bg.wasm"
  echo "wasm-opt: optimized openpdfedit_wasm_bg.wasm"
else
  echo "wasm-opt not found on PATH — skipping (glue still works, just larger/slower than optimal)"
fi

echo "wasm build complete: $OUT_DIR"
