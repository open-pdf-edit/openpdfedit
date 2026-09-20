#!/usr/bin/env bash
# Builds OpenPdfEdit as a static web app: the shared SPA
# (apps/desktop/src) compiled against the WebAssembly backend, plus the
# two wasm binaries it fetches at runtime.
#
# This is deliberately apps/extension/scripts/build-spa.sh minus the two
# things that are only true of an extension:
#
#   - externalize-inline.mjs, which exists solely because MV3's CSP
#     forbids inline <script> content. A web page has no such rule, so
#     SvelteKit's own bootstrap script is left exactly as emitted.
#   - background.js and manifest.json, which have no meaning here.
#
# Everything else — the wasm build, the vendored pdfium, the asset
# layout wasm.ts expects — is shared, and is reused from the extension's
# scripts rather than duplicated, so the two builds cannot drift.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBAPP_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_DIR="$(dirname "$(dirname "$WEBAPP_DIR")")"
DESKTOP_DIR="$WORKSPACE_DIR/apps/desktop"
EXT_DIR="$WORKSPACE_DIR/apps/extension"
# Both overridable, for the one other caller: apps/ios/scripts/sync-web.sh
# builds the same SPA for the app bundle, where the base must be empty and
# the output must not overwrite the site's own dist.
DIST_DIR="${DIST_DIR:-$WEBAPP_DIR/dist}"
# Empty is a real value here — BASE_PATH= means "no base", which is what a
# bundle served from the root of a custom scheme needs — so ${BASE_PATH-…}
# rather than ${BASE_PATH:-…}.
BASE_PATH="${BASE_PATH-/app}"

log() { printf '\033[1m==> %s\033[0m\n' "$1"; }

log "Fetching the PDFium wasm binary (no-op if present)"
# Same vendored binary the extension uses, at the same path wasm.ts
# expects — fetched here so this build works from a clean checkout
# rather than only after someone has built the extension.
bash "$WORKSPACE_DIR/scripts/fetch-pdfium-wasm.sh"

log "Building the Rust core for wasm32"
# Shared with the extension: same crate, same pinned wasm-bindgen, same
# output location (apps/desktop/src/lib/wasm-gen + the vendored pdfium).
bash "$EXT_DIR/scripts/build-wasm.sh"

log "Building the SPA against the wasm backend"
# VITE_BACKEND=wasm is what makes backend/index.ts resolve to wasm.ts's
# WasmBackend instead of the Tauri default — see that file's doc.
# BASE_PATH is what makes this build servable from openpdfedit.com/app.
# SvelteKit bakes it into every chunk URL and into the client router, so it
# cannot be applied afterwards by nginx: a base-less build under /app/ loads
# every file and then renders "Not found: /app/". Only this build sets it --
# the desktop app and the extension are served from their own root.
(cd "$DESKTOP_DIR" && rm -rf build .svelte-kit && BASE_PATH="$BASE_PATH" VITE_BACKEND=wasm npm run build)

if [ ! -f "$DESKTOP_DIR/build/index.html" ]; then
  echo "build.sh: $DESKTOP_DIR/build/index.html missing — did the SPA build fail silently?" >&2
  exit 1
fi

log "Assembling $DIST_DIR"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"
cp -R "$DESKTOP_DIR/build/." "$DIST_DIR/"

# The runtime assets no bundler ever sees: wasm.ts injects
# <script src="/pdfium.js"> itself and dynamically imports
# /wasm-gen/openpdfedit_wasm.js through a string specifier, so nothing
# copies these unless a script does it explicitly.
# Nothing in the package may fetch code from another origin — the same
# check the extension build runs, for the same reason: a review reads the
# code, and the iOS bundle is built from this output too.
log "Checking the build loads no remote code"
node "$EXT_DIR/scripts/drop-remote-scripts.mjs" "$DIST_DIR"

log "Copying the wasm runtime assets"
PDFIUM_DIR="$WORKSPACE_DIR/.vendor/pdfium-wasm/release/node"
for f in pdfium.js pdfium.wasm; do
  [ -f "$PDFIUM_DIR/$f" ] || { echo "build.sh: missing $PDFIUM_DIR/$f — run $EXT_DIR/scripts/build-wasm.sh" >&2; exit 1; }
  cp "$PDFIUM_DIR/$f" "$DIST_DIR/$f"
done
# build-wasm.sh emits into apps/extension/src/wasm-gen (it is the
# extension's script, shared rather than duplicated); the web app copies
# from there rather than re-running the generator into a second location.
WASM_GEN_DIR="$EXT_DIR/src/wasm-gen"
mkdir -p "$DIST_DIR/wasm-gen"
for f in openpdfedit_wasm.js openpdfedit_wasm_bg.wasm; do
  [ -f "$WASM_GEN_DIR/$f" ] || { echo "build.sh: missing $WASM_GEN_DIR/$f" >&2; exit 1; }
  cp "$WASM_GEN_DIR/$f" "$DIST_DIR/wasm-gen/$f"
done

# OCR's engine and trained data, served from this origin rather than the
# CDN tesseract.js would otherwise reach for. See
# scripts/fetch-tesseract-assets.sh for why that default is wrong here.
# Deliberately *not* in the service worker's precache list: ~3 MB gzipped
# that only someone who OCRs a scan ever needs. The fetch handler caches
# them on first use, so OCR works offline from the second time on.
log "Copying the OCR engine and language data"
bash "$WORKSPACE_DIR/scripts/fetch-tesseract-assets.sh"
OCR_DIR="$DIST_DIR/ocr"
TESS_CORE="$DESKTOP_DIR/node_modules/tesseract.js-core"
TESS_JS="$DESKTOP_DIR/node_modules/tesseract.js/dist"
mkdir -p "$OCR_DIR"
# Every core variant, not a chosen few. tesseract.js picks one at
# runtime from what the browser supports — plain, SIMD, or relaxed SIMD,
# each with an LSTM-only twin — and asks for it by name. Shipping a
# subset works on whatever machine the build was tested on and fails on
# someone else's with "failed to load", which is exactly what happened:
# the guess omitted relaxedsimd, which is what current Chrome asks for.
# They are ~2.7 MB each but only one is ever fetched, so the cost is disk
# on the server, not bandwidth for the user.
cp "$TESS_CORE"/tesseract-core*.wasm "$TESS_CORE"/tesseract-core*.wasm.js "$OCR_DIR/"
[ -f "$OCR_DIR/tesseract-core-relaxedsimd-lstm.wasm.js" ] || {
  echo "build.sh: no tesseract core variants found in $TESS_CORE — run npm install in apps/desktop" >&2
  exit 1
}
[ -f "$TESS_JS/worker.min.js" ] || { echo "build.sh: missing $TESS_JS/worker.min.js" >&2; exit 1; }
cp "$TESS_JS/worker.min.js" "$OCR_DIR/worker.min.js"
# Every language that was fetched, not a named one. Tesseract reads the
# script it has data for and returns silence or nonsense for anything
# else without failing, so a missing language is not an error the user
# ever sees — it is OCR appearing not to work. Shipping only English is
# how that happened. Each file is 1-4 MB on the server; a run fetches
# only the languages it was asked for.
shopt -s nullglob
TRAINED=("$WORKSPACE_DIR/.vendor/tesseract"/*.traineddata)
shopt -u nullglob
[ ${#TRAINED[@]} -gt 0 ] || {
  echo "build.sh: no trained data in .vendor/tesseract — run scripts/fetch-tesseract-assets.sh" >&2
  exit 1
}
cp "${TRAINED[@]}" "$OCR_DIR/"
log "OCR languages: $(printf '%s ' "${TRAINED[@]##*/}" | sed 's/\.traineddata//g')"

# A service worker, so the app keeps working with no network at all —
# which is the whole claim, and only demonstrable if it's true offline.
log "Adding the offline service worker"
cp "$WEBAPP_DIR/manifest.webmanifest" "$DIST_DIR/manifest.webmanifest"

# The app host's own robots.txt. Without one, a request for /robots.txt
# falls through to the SPA fallback and answers with the app's HTML and a
# 200 — which is not a robots.txt, but is also not the 404 that would make
# a crawler assume "no rules".
log "Writing robots.txt for the app host"
cat > "$DIST_DIR/robots.txt" <<'ROBOTS'
# The editor itself. It is one page — every path under /app/ serves it —
# so there is one URL worth indexing and a canonical on the page naming
# it.
#
# Note this file is served at /app/robots.txt, where no crawler reads it:
# robots.txt is only honoured at an origin's root, and that one belongs
# to the marketing site. It is kept because the rules below are still the
# right answer if this build is ever served from a host of its own again,
# and because a sitemap naming a redirect is the same defect as a
# canonical naming one.
User-agent: *
Allow: /$
Allow: /index.html
# Build output: hashed bundles, the wasm, and the OCR language data.
# Nothing here reads as a page, and a crawler spending its budget on a
# 4 MB wasm binary is spending it on nothing.
Disallow: /app/
Disallow: /wasm-gen/
Disallow: /tesseract/
ROBOTS
# No sitemap of our own. /app/ is an ordinary URL on openpdfedit.com, and
# the site's sitemap (the marketing repo's) already lists it; a second
# file under /app/ listed the same URL with a different lastmod, and two
# answers to "when did this change?" is how a search engine learns to
# ignore the field for the whole site (APP-96). nginx answers 404 for
# /app/sitemap.xml explicitly, since the SPA fallback would otherwise
# serve the app's HTML there with a 200.
# The install icons. Separate from static/favicon.png, which is 256px and
# is what the manifest used to point at while claiming 512 — a size no
# browser could verify without downloading it, and one that stops Chrome
# treating the app as installable at all.
mkdir -p "$DIST_DIR/icons"
cp "$WEBAPP_DIR/icons/"*.png "$DIST_DIR/icons/"

# The cache name carries a digest of everything in dist/, so it changes
# exactly when the build does. Deriving it rather than hand-bumping a
# version string is what keeps a second build of an unchanged version
# number from serving the first build's index.html and wasm forever —
# and equally, keeps a rebuild that changed nothing from pointlessly
# evicting a returning visitor's 9 MB of cached binaries.
# Hashed from *inside* dist/, so the names that reach the digest are
# relative. `xargs shasum` prints "<hash>  <path>", so hashing its output
# with absolute paths made the build id depend on where the repository
# happens to sit on disk: moving the checkout produced a brand-new id for
# a byte-identical build, which tells every returning visitor's service
# worker to evict and re-download ~9 MB of WebAssembly for nothing.
# Relative paths keep what should matter — a renamed file is a different
# build — and drop what shouldn't.
BUILD_ID=$(
  cd "$DIST_DIR" &&
  find . -type f -print0 |
    LC_ALL=C sort -z |
    xargs -0 shasum -a 256 |
    shasum -a 256 |
    cut -c1-12
)
sed "s/__BUILD_ID__/$BUILD_ID/" "$WEBAPP_DIR/service-worker.js" > "$DIST_DIR/service-worker.js"
grep -q "__BUILD_ID__" "$DIST_DIR/service-worker.js" &&
  { echo "build.sh: service-worker.js still has an unstamped __BUILD_ID__" >&2; exit 1; }

# Injected here rather than in apps/desktop/src/app.html, because that
# file is shared with the Tauri and extension builds: a manifest link
# would 404 in both, and registering a service worker inside a
# chrome-extension:// origin or a Tauri window is meaningless at best.
# Post-processing keeps the shared SPA free of web-app-only concerns.
log "Registering the service worker and web manifest in index.html"
node - "$DIST_DIR/index.html" "$SCRIPT_DIR" <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";
const file = process.argv[2];
const html = readFileSync(file, "utf8");
if (html.includes("service-worker.js")) process.exit(0);
// What a search engine or a link preview is told, before any script runs
// (APP-96). The title leads with what people search for and ends with the
// name; og/twitter tags make a shared link a card instead of a bare URL.
const TITLE = "Free Online PDF Editor & Viewer — No Upload | OpenPdfEdit";
// Two descriptions, because the limits differ. A results page cuts the
// meta one around 158 characters, and the old 212-character version lost
// exactly the part that earns the click — "Free, no account" (APP-96,
// second review). A social card has no such limit, so og:description
// keeps the longer sentence.
const DESCRIPTION = "Edit PDFs in your browser — annotate, edit text, fill forms, redact and sign. Nothing is uploaded; every edit saves on your own machine. Free, no account.";
const OG_DESCRIPTION = "Edit PDFs in your browser — annotate, edit text, fill forms, redact, sign and reorganise pages. Nothing is uploaded: every page renders and every edit saves on your own machine. Free, no account, works offline.";
const inject = [
  '<link rel="manifest" href="./manifest.webmanifest">',
  // Every path on this host answers with this same file and a 200 —
  // that is what makes a single-page app work, and it also means a
  // crawler that guesses a URL is told the page exists. Left alone, one
  // thin page gets indexed under unlimited addresses, all competing
  // with each other and with the marketing site. A self-canonical
  // collapses them back into one.
  // The apex under /app, not app.openpdfedit.com: that host 301s here,
  // and a canonical pointing at a redirect is a mixed signal the search
  // engine resolves by picking a URL itself.
  '<link rel="canonical" href="https://openpdfedit.com/app/">',
  // The app is the tool; openpdfedit.com is what should rank for
  // someone still deciding. Indexed so the app can be found and cited
  // directly, with a description of its own rather than whatever a
  // crawler scrapes off an empty editor shell.
  '<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1">',
  `<meta name="description" content="${DESCRIPTION}">`,
  '<meta property="og:type" content="website">',
  '<meta property="og:url" content="https://openpdfedit.com/app/">',
  `<meta property="og:title" content="${TITLE}">`,
  `<meta property="og:description" content="${OG_DESCRIPTION}">`,
  '<meta property="og:image" content="https://openpdfedit.com/og.png">',
  '<meta name="twitter:card" content="summary_large_image">',
  // iOS reads none of the manifest's icons: it wants this tag, and puts
  // a white card behind anything transparent, which is why the file is
  // full-bleed rather than the artwork as drawn.
  '<link rel="apple-touch-icon" href="./icons/apple-touch-icon.png">',
  // Colours the address bar on Android and the status bar in a
  // standalone iOS window, so an installed app does not open with a
  // white strip above a dark UI.
  '<meta name="theme-color" content="#111111">',
  // Both spellings. `apple-mobile-web-app-capable` is the one iOS has
  // always read; Chrome now warns that it is deprecated and asks for the
  // standard name, which Android reads. Neither browser reads the
  // other's, so dropping either loses a platform.
  '<meta name="mobile-web-app-capable" content="yes">',
  '<meta name="apple-mobile-web-app-capable" content="yes">',
  '<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">',
  '<meta name="apple-mobile-web-app-title" content="OpenPdfEdit">',
  "<script>",
  // Catch beforeinstallprompt before the app exists. Chromium fires it
  // as soon as it decides the page is installable, which can be before
  // the SPA has hydrated — and the event is not replayed, so a listener
  // that arrives late never sees it and the install offer simply never
  // appears. Found by screenshotting the offer and not finding it.
  "window.__installPromptEvent = null;",
  "addEventListener('beforeinstallprompt', function (e) {",
  "  e.preventDefault();",
  "  window.__installPromptEvent = e;",
  "  dispatchEvent(new Event('openpdfedit:installable'));",
  "});",
  "if ('serviceWorker' in navigator) {",
  "  addEventListener('load', () => {",
  "    navigator.serviceWorker.register('./service-worker.js').catch(() => {});",
  "  });",
  "}",
  "</script>",
].join("\n");
if (!html.includes("</head>")) {
  console.error("build.sh: no </head> in index.html — cannot register the service worker");
  process.exit(1);
}
if (!/<title>[^<]*<\/title>/.test(html)) {
  console.error("build.sh: no <title> in index.html to replace");
  process.exit(1);
}
// The crawlable copy: an <h1> and ~400 words, in the HTML itself rather
// than put there by a script — crawlers that run no JavaScript (GPTBot,
// ClaudeBot, PerplexityBot) otherwise see an empty <div>, and Googlebot
// queues JavaScript rendering behind everything else. Visible, not
// hidden: the app moves it into its own empty state on start, where it
// sits under "Open PDF" until a document is opened (see
// $lib/landingCopy.ts). Kept in its own file so the words can be edited
// without reading this script.
const landing = readFileSync(new URL("../landing.html", `file://${process.argv[3]}/`), "utf8");
if (!html.includes("</body>")) {
  console.error("build.sh: no </body> in index.html — cannot add the page copy");
  process.exit(1);
}
writeFileSync(file, html
  .replace(/<title>[^<]*<\/title>/, `<title>${TITLE.replace(/&/g, "&amp;")}</title>`)
  .replace("</head>", inject + "\n</head>")
  .replace("</body>", landing + "</body>"));
NODE

# APP-96's acceptance, as part of the build rather than a checklist: each
# of these was a live defect, and each is cheap to break again without
# noticing — a new <title> in app.html, a copy edit that runs long.
log "Checking what a crawler sees in index.html"
node - "$DIST_DIR/index.html" "$DIST_DIR" <<'NODE'
import { existsSync, readFileSync } from "node:fs";
const [file, dist] = process.argv.slice(2);
const html = readFileSync(file, "utf8");
const body = html.slice(html.indexOf("<body")).replace(/<script[\s\S]*?<\/script>/g, "");
const words = body.replace(/<[^>]*>/g, " ").replace(/&amp;/g, "&").split(/\s+/).filter(Boolean).length;
const title = html.match(/<title>([^<]*)<\/title>/)?.[1].replace(/&amp;/g, "&") ?? "";
const description = html.match(/<meta name="description" content="([^"]*)"/)?.[1] ?? "";
const checks = [
  [words >= 350 && words <= 450, `static body text: ${words} words (350–450)`],
  [(html.match(/<h1[\s>]/g) ?? []).length === 1, "exactly one <h1>"],
  [title.length > 0 && title.length <= 60, `<title> "${title}" is ${title.length} characters (≤ 60)`],
  // Measured, not judged by eye: the first version of this was written
  // as "already good, leave it" and was 212 characters.
  [description.length > 0 && description.length <= 158, `description is ${description.length} characters (≤ 158)`],
  // OCR and the watermark are the same purchase. A free-looking list
  // entry for either is a paywall the reader walks into.
  [["OCR", "Watermark"].every((tool) => (body.split("<li>").find((li) => li.includes(`<strong>${tool}</strong>`)) ?? "").includes("Supporter")),
    "OCR and Watermark both marked Supporter"],
  [!/split one into parts/i.test(body), "no claim of splitting one file into many"],
  [["og:type", "og:url", "og:title", "og:description", "og:image"].every((p) => html.includes(`property="${p}"`)) && html.includes('name="twitter:card"'), "og and twitter tags"],
  [!existsSync(`${dist}/sitemap.xml`), "no sitemap.xml of its own"],
  [!/umami/i.test(html), "no analytics — privacy.html promises none"],
];
let failed = 0;
for (const [ok, what] of checks) { console.log(`  ${ok ? "ok  " : "FAIL"}  ${what}`); if (!ok) failed++; }
if (failed) process.exit(1);
NODE

log "Done"
echo "  output:  $DIST_DIR"
echo "  size:    $(du -sh "$DIST_DIR" | cut -f1)"
echo
echo "Serve it (a service worker needs an http origin, not file://):"
echo "  npm --prefix apps/webapp run preview   # http://localhost:8081"
