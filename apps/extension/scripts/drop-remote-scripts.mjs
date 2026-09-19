// Takes the Telegram loader out of the extension's index.html, and then
// proves nothing else in dist/ loads code from the internet.
//
// apps/desktop/src/app.html carries a small bootstrap that appends
// <script src="https://telegram.org/js/telegram-web-app.js"> when the URL
// fragment says the page is open inside a Telegram Mini App. The web app
// needs it. The extension cannot use it at all: its pages are opened as
// chrome-extension://…/index.html, where no fragment ever says tgWebApp,
// and MV3's `script-src 'self' 'wasm-unsafe-eval'` would refuse the tag
// anyway.
//
// It still has to go, because a store review reads the code rather than
// what the code can reach: a <script> pointing at another origin is
// exactly the Chrome Web Store's definition of remote code, and leaving
// it in means answering "yes, we use remote code" and defending it. With
// it removed the honest answer is no — which is also the true one, since
// everything the extension runs is in the package: PDFium and the Rust
// core as WebAssembly, the SPA's own chunks, nothing fetched.
//
// Run by scripts/build-spa.sh after the SPA is copied in and before the
// inline scripts are externalized, so the loader never reaches an
// inline-N.js file. It fails the build rather than warn: a quiet pass
// here would be discovered in a store review weeks later.
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIST = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const INDEX = join(DIST, "index.html");

// --- 1. the Telegram loader ------------------------------------------------
const before = readFileSync(INDEX, "utf8");
// The whole <script> element that mentions telegram.org, however it is
// laid out — matched on the element, not on its exact text, so a reworded
// bootstrap is still removed instead of silently shipping.
const loader = /[ \t]*<script\b[^>]*>(?:(?!<\/script>)[\s\S])*?telegram\.org(?:(?!<\/script>)[\s\S])*?<\/script>\s*/gi;
// The comment that explains it goes too: it names the same third-party URL,
// and a reviewer reads the file rather than executing it.
const explanation = /[ \t]*<!--(?:(?!-->)[\s\S])*?[Tt]elegram(?:(?!-->)[\s\S])*?-->\s*/g;
const found = before.match(loader)?.length ?? 0;
if (found === 0) {
  console.error("drop-remote-scripts: no Telegram loader in dist/index.html.");
  console.error("  If app.html no longer has one, delete this step; if it moved, teach this script where.");
  process.exit(1);
}
writeFileSync(INDEX, before.replace(loader, "\n").replace(explanation, ""));
console.log(`drop-remote-scripts: removed ${found} Telegram loader script tag(s) from index.html`);

// --- 2. nothing else reaches out -------------------------------------------
// Only the ways a browser is actually told to run someone else's code:
// a script tag with a remote src, an assignment to .src, importScripts,
// and a dynamic import of a URL. A remote address in a comment, a link,
// or a privacy page is not code and is left alone.
const RUNS_REMOTE_CODE = [
  /<script\b[^>]*\bsrc\s*=\s*["']https?:\/\//i,
  /\.src\s*=\s*["']https?:\/\//i,
  /importScripts\s*\(\s*["']https?:\/\//i,
  /\bimport\s*\(\s*["']https?:\/\//i,
];
const TEXT = /\.(html|js|mjs|cjs|css|json)$/i;

function* files(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* files(path);
    else if (TEXT.test(entry)) yield path;
  }
}

const offenders = [];
for (const path of files(DIST)) {
  const text = readFileSync(path, "utf8");
  for (const pattern of RUNS_REMOTE_CODE) {
    const hit = text.match(pattern);
    if (hit) offenders.push(`${path.slice(DIST.length + 1)}: ${hit[0].trim()}`);
  }
}
if (offenders.length > 0) {
  console.error("drop-remote-scripts: the package would load code from the internet:");
  for (const o of offenders) console.error(`  ${o}`);
  console.error("  A store asks whether the extension uses remote code; this is what makes the answer yes.");
  process.exit(1);
}
console.log("drop-remote-scripts: no remote code in dist/ — every script it runs is in the package");
