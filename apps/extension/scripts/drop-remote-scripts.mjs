// Proves a built package loads no code from the internet — and takes out
// the Telegram loader if a build ever puts one back.
//
//   node drop-remote-scripts.mjs [dist-directory]
//
// The Mini App bridge used to live in apps/desktop/src/app.html, so every
// target carried a <script src="https://telegram.org/…"> and this script
// cut it out of the extension's copy. It is gone at the source now: the
// bridge belongs to the Telegram build, in its own private repository,
// and nothing built here ships it (svelte.config.js, $telegram).
//
// So the removal is a safety net and the *assertion* is the point. A
// store review reads the code rather than what the code can reach: a
// <script> pointing at another origin is the Chrome Web Store's own
// definition of remote code, MV3 forbids it, and an iOS bundle carrying
// one is a question at review. With none present the honest answer to
// "are you using remote code?" is no — everything these packages run is
// in them: PDFium and the Rust core as WebAssembly, the SPA's own chunks.
//
// Called by the extension build, the web app build and the iOS bundle
// sync, each with its own directory. It fails the build rather than
// warn: a quiet pass here is discovered in a review weeks later.
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DIST = process.argv[2]
  ? resolve(process.argv[2])
  : join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const INDEX = join(DIST, "index.html");
const label = DIST.split("/").slice(-2).join("/");

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
if (found > 0) {
  writeFileSync(INDEX, before.replace(loader, "\n").replace(explanation, ""));
  console.log(`drop-remote-scripts: removed ${found} Telegram loader script tag(s) from ${label}/index.html`);
}

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
console.log(`drop-remote-scripts: no remote code in ${label} — every script it runs is in the package`);
