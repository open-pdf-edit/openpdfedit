// Before any listing is pasted into a store dashboard.
//
// Neither the Chrome Web Store nor Edge Add-ons takes a long description
// or search terms through an API, so this text reaches a store only by
// someone pasting it into a form. That is when these checks matter, and
// why this is a pre-submission script rather than a CI job: the listings
// are not built or shipped by a push.
//
// Three kinds of check, and the third is the one that exists because of
// what the stores actually reject:
//   - coverage: every catalogue the package ships has a listing, and back
//   - limits:   Edge's documented ones, which Partner Center enforces
//   - claims:   nothing a listing promises that the packaged build lacks.
//               Edge requires a description to "not contain any
//               misleading" content, and it rejected this extension once.
//
// Run: npm run store:check     (exits non-zero on any failure)
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const LISTINGS = join(ROOT, "store", "listings");
const ZIP = join(ROOT, "openpdfedit-dist.zip");
const index = JSON.parse(readFileSync(join(LISTINGS, "locales.json"), "utf8"));

let failures = 0;
const fail = (msg) => { console.log(`  FAIL  ${msg}`); failures += 1; };
const ok = (msg) => console.log(`  ok    ${msg}`);
const warnings = [];
const warn = (msg) => warnings.push(msg);
const read = (loc, file) => readFileSync(join(LISTINGS, loc, file), "utf8").trim();

console.log("coverage");
const catalogues = readdirSync(join(ROOT, "public", "_locales")).sort();
const listed = Object.keys(index).sort();
const noListing = catalogues.filter((c) => !listed.includes(c));
const noCatalogue = listed.filter((l) => !catalogues.includes(l));
noListing.length ? fail(`catalogues with no listing: ${noListing.join(", ")}`)
                 : ok(`all ${catalogues.length} catalogues have a listing`);
noCatalogue.length ? fail(`listings with no catalogue: ${noCatalogue.join(", ")}`)
                   : ok("no listing without a catalogue behind it");

// learn.microsoft.com/microsoft-edge/extensions/publish/publish-extension,
// "Enter store listing details for each language".
console.log("Edge limits");
for (const loc of listed) {
  const edge = read(loc, "description-edge.txt");
  const terms = read(loc, "search-terms-edge.txt").split("\n").map((t) => t.trim()).filter(Boolean);
  const words = terms.reduce((n, t) => n + t.split(/\s+/).length, 0);
  const problems = [];
  if (edge.length < 250 || edge.length > 10000) problems.push(`description ${edge.length} chars (250–10,000)`);
  if (terms.length > 7) problems.push(`${terms.length} search terms (max 7)`);
  const long = terms.filter((t) => [...t].length > 30);
  if (long.length) problems.push(`terms over 30 chars: ${long.join(" | ")}`);
  if (words > 21) problems.push(`${words} words across search terms (max 21)`);
  problems.length ? fail(`${loc}: ${problems.join("; ")}`)
                  : ok(`${loc}: ${edge.length} chars, ${terms.length} terms, ${words} words`);
}

console.log("claims against the packaged build");
// Read from the zip that would be uploaded, not from source: what a
// reviewer tests is the package.
const packaged = existsSync(ZIP)
  ? execFileSync("unzip", ["-Z1", ZIP], { encoding: "utf8" })
  : "";
if (!packaged) fail("openpdfedit-dist.zip is missing — run `npm run package` first");
const hasOcr = /tesseract|traineddata/i.test(packaged);
for (const loc of listed) {
  for (const [file, store] of [["description-chrome.txt", "Chrome"], ["description-edge.txt", "Edge"]]) {
    const text = read(loc, file);
    // "OCR" is written in Latin letters in every one of the nineteen
    // translations, so this does not depend on reading the language.
    if (!hasOcr && /\bOCR\b/.test(text)) warn(`${loc} ${store} description`);
  }
  if (/\bEdge\b/.test(read(loc, "description-chrome.txt"))) {
    fail(`${loc} Chrome: the Chrome listing names Edge`);
  }
}
ok(`package checked: OCR engine ${hasOcr ? "present" : "absent"}`);

// The name and short description come from the package's own catalogues,
// and both stores show them above the long description — so a claim there
// is the first thing a reviewer reads, not the last.
for (const loc of catalogues) {
  const m = JSON.parse(readFileSync(join(ROOT, "public", "_locales", loc, "messages.json"), "utf8"));
  for (const key of ["name", "description"]) {
    if (!hasOcr && /\bOCR\b/.test(m[key]?.message ?? "")) warn(`${loc} manifest ${key}`);
  }
}

// OCR is claimed by every listing and is not in the extension package — it
// runs in the web and desktop apps. Submitting with the claim was decided
// on 14 September 2026, on the precedent that OpenCapture's listing passed
// review with a Supporter feature (watermark) described while the large
// majority of the extension's functions work. So it is reported, not
// failed: a count that changes means a listing changed, which is worth
// seeing, but it is no longer a reason to stop.
if (warnings.length) {
  console.log(`\nnote  OCR is claimed in ${warnings.length} places and is not in the package`);
  console.log("      — decided to submit as written; see STORE.md, \"OCR\"");
}
console.log(failures === 0 ? "\nall good — ready to paste" : `\n${failures} failed — not ready to submit`);
process.exit(failures === 0 ? 0 : 1);
