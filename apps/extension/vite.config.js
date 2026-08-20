import { defineConfig } from "vite";
import { execFileSync } from "node:child_process";

// Found during the whole-branch review: `npm run dev` (`vite build --watch`)
// left `dist/` with no PDFium in it after every rebuild, because
// `scripts/copy-vendor.sh` only ever ran once, as a separate step in
// `package.json`'s `build` script — `vite build` (in *both* one-shot and
// `--watch` mode) empties `outDir` on every build pass it does, watch mode
// included, which deletes whatever copy-vendor.sh had put there before.
// Confirmed by direct observation: running `vite build --watch` after a
// manual `copy-vendor.sh` immediately removed `dist/pdfium.js` and
// `dist/wasm-gen/` again, before any source file had even changed — so
// simply reordering `package.json`'s `dev` script to run copy-vendor
// *before* starting watch mode (the fix that looks obviously sufficient)
// is not actually sufficient; it still loses the vendor files to watch
// mode's own first pass.
//
// A Vite plugin's `writeBundle` hook runs after every build Vite does —
// the one-shot build, watch mode's initial build, and every subsequent
// watch-triggered rebuild alike — and by the time it runs, `outDir` is
// guaranteed to already exist (Vite just finished writing to it), so
// `copy-vendor.sh`'s own "did vite build already run" precondition check
// is satisfied without this plugin needing to duplicate it. This makes
// `npm run dev` self-correcting on every rebuild instead of only right
// after it starts, and makes the separate `&& npm run copy-vendor` step
// in `package.json`'s `build` script redundant (harmless — copy-vendor.sh
// only ever overwrites with identical content — but no longer load-bearing).
function copyVendorAfterBuild() {
  return {
    name: "copy-vendor-after-build",
    writeBundle() {
      execFileSync("bash", ["scripts/copy-vendor.sh"], { stdio: "inherit" });
    },
  };
}

export default defineConfig({
  plugins: [copyVendorAfterBuild()],
  build: {
    // Kept even though background.ts (the only entry left after Task 9
    // retired editor.html/editor.ts) has no top-level await of its own
    // today — esnext is still the right target for a Chrome-only MV3
    // extension (see the removed editor.ts's original comment this one
    // replaces), and nothing forces a more conservative one back on.
    target: "esnext",
    rollupOptions: {
      // Only background.ts is built by Vite now — the extension's UI
      // (formerly editor.html/editor.ts, a hand-written walking skeleton)
      // is the shared desktop SPA (apps/desktop, built separately with
      // VITE_BACKEND=wasm) copied into dist/ by
      // scripts/build-spa.sh, *after* this `vite build` runs. See that
      // script and package.json's `build` chain for the full order.
      input: {
        background: "background.ts",
      },
      output: {
        entryFileNames: "[name].js",
      },
    },
  },
});
