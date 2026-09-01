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
import { readFileSync } from "node:fs";
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

/** Chrome's limit, which Edge enforces at upload and Chrome does not
 * enforce at load — so a manifest can be over it for months. */
const DESCRIPTION_LIMIT = 132;

test("the manifest description fits what a store will accept", () => {
  const { description } = manifest();
  expect(
    description.length,
    `manifest description is ${description.length} characters; stores cap it at ${DESCRIPTION_LIMIT}`,
  ).toBeLessThanOrEqual(DESCRIPTION_LIMIT);
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
  expect(manifest().description).toBe(declared);
});
