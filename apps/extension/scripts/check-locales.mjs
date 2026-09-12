// Every listing catalogue is complete and inside the store's limits.
//
// `name` and `description` are what the Edge Add-ons and Chrome Web Store
// listings show. Neither is visible in a build or in review: a name three
// characters too long surfaces as a rejected submission, at the point where
// somebody is waiting on it, and a locale folder Chrome does not recognise is
// simply ignored — the listing silently falls back to English for that
// language and nothing says so.
//
// Takes the directory holding manifest.json and _locales/, defaulting to the
// source in public/. package-zip.sh points it at dist/ instead, so what gets
// measured is the artifact being uploaded rather than the files it was built
// from — copy-vendor and the build have both been able to drop a file before.
//
//   node scripts/check-locales.mjs [dir]
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ext = dirname(dirname(fileURLToPath(import.meta.url)));
const base = process.argv[2] ? resolve(process.argv[2]) : join(ext, "public");
const root = join(base, "_locales");
if (!existsSync(root)) {
  console.error(`check-locales: no _locales directory in ${base}`);
  process.exit(1);
}
const manifest = JSON.parse(readFileSync(join(base, "manifest.json"), "utf8"));

// https://developer.chrome.com/docs/webstore/cws-dashboard-listing
const NAME_MAX = 75;
const DESCRIPTION_MAX = 132;

let failures = 0;
const check = (label, ok, detail = "") => {
  if (ok) console.log(`  ok   ${label}`);
  else { failures++; console.log(`  FAIL ${label}${detail ? ` — ${detail}` : ""}`); }
};

console.log(`${base}\n${manifest.name} / ${manifest.description}, default_locale=${manifest.default_locale}\n`);

// The three have to agree or the catalogues are dead weight: message keys in
// the manifest do nothing without default_locale, and default_locale does
// nothing without a folder of that name.
check("manifest name is a message key", manifest.name === "__MSG_name__", manifest.name);
check("manifest description is a message key", manifest.description === "__MSG_description__", manifest.description);
check("default_locale is set", !!manifest.default_locale, "absent — Chrome rejects the package");
check(`default_locale (${manifest.default_locale}) has a catalogue`,
  existsSync(join(root, manifest.default_locale ?? "", "messages.json")));

const locales = readdirSync(root).filter((d) => !d.startsWith("."));
console.log(`\n${locales.length} locales`);

const tooLongName = [];
const tooLongDescription = [];
const malformed = [];
for (const locale of locales) {
  const file = join(root, locale, "messages.json");
  if (!existsSync(file)) { malformed.push(`${locale}: no messages.json`); continue; }
  let m;
  try { m = JSON.parse(readFileSync(file, "utf8")); }
  catch (e) { malformed.push(`${locale}: ${e.message}`); continue; }
  const name = m.name?.message;
  const description = m.description?.message;
  if (typeof name !== "string" || !name.trim()) { malformed.push(`${locale}: no name`); continue; }
  if (typeof description !== "string" || !description.trim()) { malformed.push(`${locale}: no description`); continue; }
  if (name.length > NAME_MAX) tooLongName.push(`${locale}:${name.length}`);
  if (description.length > DESCRIPTION_MAX) tooLongDescription.push(`${locale}:${description.length}`);
}

check("every catalogue has a name and a description", malformed.length === 0, malformed.join("; "));
check(`every name within ${NAME_MAX} chars`, tooLongName.length === 0, tooLongName.join(", "));
check(`every description within ${DESCRIPTION_MAX} chars`, tooLongDescription.length === 0, tooLongDescription.join(", "));

// A folder whose name Chrome does not know is not an error at build time and
// not an error in review — it just never loads.
// https://developer.chrome.com/docs/extensions/reference/api/i18n#locales
const CHROME_LOCALES = new Set(["ar","am","bg","bn","ca","cs","da","de","el","en","en_AU","en_GB","en_US","es","es_419","et","fa","fi","fil","fr","gu","he","hi","hr","hu","id","it","ja","kn","ko","lt","lv","ml","mr","ms","nl","no","nb","pl","pt","pt_BR","pt_PT","ro","ru","sk","sl","sr","sv","sw","ta","te","th","tr","uk","vi","zh_CN","zh_TW"]);
const unknown = locales.filter((l) => !CHROME_LOCALES.has(l));
check("every folder is a locale Chrome recognises", unknown.length === 0, unknown.join(", "));

console.log(failures ? `\n${failures} check(s) FAILED` : "\nall locale checks passed");
process.exit(failures ? 1 : 0);
