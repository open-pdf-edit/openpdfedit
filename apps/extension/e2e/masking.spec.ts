import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

// The suite's backend answers as accounts.openapps.network, and no
// shipped client is ever pointed there. Each product reaches the same
// server through its own auth. and gateway. hostnames — same service,
// same ledger, different name in the address bar and in the permission
// prompt a browser puts in front of someone deciding whether to trust an
// install.
//
// This exists because the masking is invisible when it breaks. Nothing
// throws, no other test goes red, and the only symptom is a stranger's
// domain appearing at the worst possible moment. OpenPdfEdit shipped a
// masked auth. host beside a bare gateway.openapps.network for months
// for precisely that reason: the hostname, the nginx server_name and the
// certificate were all already in place, and one client constant was
// never changed.

const REPO = fileURLToPath(new URL("../../../", import.meta.url));
const MODULE = "apps/desktop/src/lib/openapps.ts";
const PLATFORM = /accounts\.openapps\.network|gateway\.openapps\.network/;

function read(path: string): string {
  return readFileSync(join(REPO, path), "utf8");
}

function walk(dir: string, out: string[] = []): string[] {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}

test("both hostnames are the product's own, not the platform's", () => {
  const source = read(MODULE);

  // Asserted together, because masking only auth. is the specific way
  // this goes wrong. auth. is a URL someone may glance at; gateway. is
  // the one a browser interrupts them to ask about, so it is the half
  // that matters more and the half that gets forgotten.
  expect(source, "OPENAPPS_BASE_URL must be the product's own auth host").toContain(
    'export const OPENAPPS_BASE_URL = "https://auth.openpdfedit.com"',
  );
  expect(source, "OPENAPPS_GATEWAY_URL must be the product's own gateway host").toContain(
    'export const OPENAPPS_GATEWAY_URL = "https://gateway.openpdfedit.com"',
  );
});

test("no shipped source outside that module names the platform", () => {
  // The regression test for a literal that got copied. OpenCapture's
  // base URL lived in two places, so moving to its custom domain fixed
  // one and silently left the other on the old host — a break that shows
  // up only in whichever flow used the second copy.
  const offenders = ["apps/desktop/src", "apps/extension/src", "apps/webapp/src"]
    .flatMap((dir) => walk(join(REPO, dir)))
    .filter((path) => /\.(ts|js|svelte|html)$/.test(path))
    .filter((path) => !path.endsWith(join("lib", "openapps.ts")))
    .filter((path) => PLATFORM.test(readFileSync(path, "utf8")))
    .map((path) => path.slice(REPO.length));

  expect(offenders, `${MODULE} must be the only place these hosts appear`).toEqual([]);
});

test("the built extension ships neither platform hostname", () => {
  // The one that asserts what actually reaches a user. The two above
  // read the source; a bundler, a vendored dependency or a stale build
  // can still put the name back, and this is the only check that would
  // notice.
  const dist = join(REPO, "apps/extension/dist");
  test.skip(!existsSync(dist), "no dist/ — run `npm run build` in apps/extension first");

  const offenders = walk(dist)
    .filter((path) => /\.(js|html|json|css)$/.test(path))
    .filter((path) => PLATFORM.test(readFileSync(path, "utf8")))
    .map((path) => path.slice(REPO.length));

  expect(offenders, "the shipped bundle must not name the platform").toEqual([]);
});
