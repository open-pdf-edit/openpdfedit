// The limits a store enforces on manifest.json, checked here instead.
//
// These are not style rules. Edge rejected a v0.1.7 upload outright —
// "The string ... has exceeded the maximum length of 132" — for a
// description that had been in the manifest since the first import and
// that nothing local objected to: it is valid JSON, Chrome loads the
// unpacked extension happily, the typecheck has no opinion about it,
// and the e2e suite drove the whole app through it without noticing.
// The first thing that ever measured it was a store, after a fifteen-
// minute build and an upload.
//
// So the cost of getting this wrong is a round trip through a
// dashboard, which is exactly the kind of feedback worth pulling back
// into the test suite.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";

const EXTENSION = process.cwd();

interface Manifest {
  name: string;
  version: string;
  description: string;
}

function manifest(): Manifest {
  return JSON.parse(
    readFileSync(join(EXTENSION, "public", "manifest.json"), "utf8"),
  ) as Manifest;
}

const LOCALES = join(EXTENSION, "public", "_locales");

interface Catalogue {
  name: { message: string };
  description: { message: string };
}

function catalogue(locale: string): Catalogue {
  return JSON.parse(
    readFileSync(join(LOCALES, locale, "messages.json"), "utf8"),
  ) as Catalogue;
}

function locales(): string[] {
  return readdirSync(LOCALES).filter((d) => !d.startsWith("."));
}

/** Chrome's limits, which Edge enforces at upload and Chrome does not
 * enforce at load — so a listing can be over them for months. */
const DESCRIPTION_LIMIT = 132;
const NAME_LIMIT = 75;

// The manifest no longer carries either string. Both are message keys now,
// and the text a store measures is in nineteen catalogues — where one
// overlong translation rejects the submission just as surely as an overlong
// English one did, and is nineteen times easier to miss. Measuring
// manifest.description here would measure "__MSG_description__", which is
// nineteen characters and always passes.
test("every listing name fits what a store will accept", () => {
  const over = locales()
    .map((l) => ({ l, n: catalogue(l).name.message.length }))
    .filter(({ n }) => n > NAME_LIMIT);
  expect(over, `stores cap the listing name at ${NAME_LIMIT} characters`).toEqual([]);
});

test("every listing description fits what a store will accept", () => {
  const over = locales()
    .map((l) => ({ l, n: catalogue(l).description.message.length }))
    .filter(({ n }) => n > DESCRIPTION_LIMIT);
  expect(over, `stores cap the description at ${DESCRIPTION_LIMIT} characters`).toEqual([]);
});

test("the manifest points at the catalogues", () => {
  const m = manifest() as Manifest & { default_locale?: string };
  // Message keys without default_locale are a hard load error; default_locale
  // without a folder of that name is the same. Neither shows up in a build.
  expect(m.name).toBe("__MSG_name__");
  expect(m.description).toBe("__MSG_description__");
  expect(m.default_locale, "default_locale is what resolves the keys").toBe("en");
  expect(locales()).toContain("en");
});

test("the version is a number a store will take", () => {
  // Letters are the trap: "0.2.0-beta" is a perfectly ordinary version
  // everywhere else in software and is refused at upload.
  expect(manifest().version).toMatch(/^\d+(\.\d+){0,3}$/);
});

test("the manifest version matches the package it is built from", () => {
  const pkg = JSON.parse(readFileSync(join(EXTENSION, "package.json"), "utf8")) as {
    version: string;
  };
  expect(manifest().version, "run scripts/set-version.sh rather than editing by hand").toBe(
    pkg.version,
  );
});

// The manifest description and STORE.md's short description are the same
// sentence in two places: one shipped in the package, one pasted into the
// dashboard. A reviewer sees both. Letting them drift is how a listing
// comes to describe a product the bundle is not — which is what happened
// to the "no account required" claim, true when written and false once
// OCR and watermarking became Supporter tools.
test("the store listing and the manifest describe the same product", () => {
  const store = readFileSync(join(EXTENSION, "STORE.md"), "utf8");
  const heading = store.indexOf("### Short description");
  expect(heading, "STORE.md has no short-description section").toBeGreaterThan(-1);

  const fence = store.indexOf("```", heading);
  const close = store.indexOf("```", fence + 3);
  const declared = store.slice(fence + 3, close).trim();

  expect(
    declared.length,
    "STORE.md's short description is over the store's limit too",
  ).toBeLessThanOrEqual(DESCRIPTION_LIMIT);
  // English is the one a reviewer reads beside STORE.md, and _locales/en is
  // now where the shipped copy of it lives.
  expect(catalogue("en").description.message).toBe(declared);
});
