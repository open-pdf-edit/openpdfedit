import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

// The catalogues are keyed by their English text (see i18n/index.svelte.ts
// for why), so "is every string translated?" is answerable statically —
// no browser, no build. Which matters: a missing entry is invisible at
// runtime, because `t()` deliberately falls back to English rather than
// showing a key. That fallback is what keeps the product usable; this is
// what stops it hiding a gap for a release or two.
const I18N = join(dirname(fileURLToPath(import.meta.url)), "../../desktop/src/lib/i18n");

function keysOf(file: string): Set<string> {
  const src = readFileSync(join(I18N, file), "utf8");
  // Only the object's own keys — the leading two spaces exclude anything
  // quoted inside a doc comment.
  return new Set([...src.matchAll(/^ {2}"((?:[^"\\]|\\.)*)":/gm)].map((m) => m[1]));
}

const CATALOGUES = readdirSync(I18N).filter(
  (f) => f.endsWith(".ts") && !["index.svelte.ts", "locales.ts"].includes(f),
);

test("every locale ships, and none is empty", () => {
  // Guards against a locale being added to LOCALES with no catalogue
  // behind it, which shows as an interface that silently stays English.
  const declared = [...readFileSync(join(I18N, "locales.ts"), "utf8")
    .matchAll(/code: "([^"]+)"/g)].map((m) => m[1]);
  expect(declared, "locales.ts should list the eight confirmed languages").toHaveLength(8);
  for (const code of declared) {
    if (code === "en") continue;
    expect(CATALOGUES, `${code} is offered in the picker but has no catalogue`).toContain(
      `${code}.ts`,
    );
  }
});

test("every locale covers the same strings as Simplified Chinese", () => {
  // zh-Hans is the reference rather than English: English is implicit
  // (it *is* the key set), so the first fully-written catalogue is what
  // every other one has to match.
  const reference = keysOf("zh-Hans.ts");
  expect(reference.size, "the reference catalogue should be substantial").toBeGreaterThan(150);

  for (const file of CATALOGUES) {
    if (file === "zh-Hans.ts") continue;
    const theirs = keysOf(file);
    const missing = [...reference].filter((k) => !theirs.has(k));
    const extra = [...theirs].filter((k) => !reference.has(k));
    expect(missing, `${file} is missing ${missing.length} string(s), e.g. ${missing.slice(0, 3)}`)
      .toEqual([]);
    expect(extra, `${file} has ${extra.length} string(s) nothing else does: ${extra.slice(0, 3)}`)
      .toEqual([]);
  }
});

test("no translation was left as its English source", () => {
  // A copy-paste that never got translated is invisible in review and
  // reads to a user as a half-finished product. Proper nouns and things
  // that genuinely do not translate are listed rather than guessed at.
  const SAME_ON_PURPOSE = new Set([
    "OpenPdfEdit", "Markdown", "OCR", "Helvetica", "Times", "Courier",
    "Horizontal", "Text", "Position", "Note", "Ellipse", "Signature",
    // Genuinely the same word in the target language, not an oversight:
    // "Suffix" is German, "Markdown" and the font families are names.
    "Suffix",
  ]);
  for (const file of CATALOGUES) {
    const src = readFileSync(join(I18N, file), "utf8");
    const untranslated = [...src.matchAll(/^ {2}"((?:[^"\\]|\\.)*)": "((?:[^"\\]|\\.)*)",/gm)]
      .filter(([, k, v]) => k === v && !SAME_ON_PURPOSE.has(k))
      .map(([, k]) => k);
    expect(untranslated, `${file} left these in English: ${untranslated.slice(0, 5)}`).toEqual([]);
  }
});
