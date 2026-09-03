// Nothing in the extension may lead somewhere the extension cannot go.
//
// This exists because an Edge review failed on exactly that. Signing in
// opened `/login`, a *relative* path — which the web app resolves against
// its own origin, where a server rewrites unknown paths to the SPA
// fallback. `chrome-extension://` has no server and answers a literal
// file lookup, so the reviewer got Chrome's "File not found. It may have
// been moved, edited, or deleted." and reported the product's primary
// functions as unusable. The path had been relative since the flow was
// written; it had simply never been clicked in the extension.
//
// Every existing test missed it. `boot.spec.ts` navigates straight to
// `index.html`, and every flow test stubs the file picker, so nothing
// ever followed a link the app offers to see whether it resolves.
//
// The fix keeps sign-in and sends it somewhere that can serve it: the web
// app's own login page, at its real origin, with the session handed back
// afterwards. So what is asserted here is that sign-in *leaves* the
// extension, and that the route home exists.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";

const WEBAPP_ORIGIN = "https://app.openpdfedit.com";

test("sign-in leaves the extension for an origin that can serve it", async ({
  context,
  extensionId,
}) => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  // Capture what the Sign in button asks the browser to open, without
  // letting it actually open.
  await page.evaluate(() => {
    (window as unknown as { __opened: string | null }).__opened = null;
    window.open = ((url?: string | URL) => {
      (window as unknown as { __opened: string | null }).__opened = url ? String(url) : null;
      return null;
    }) as typeof window.open;
  });

  await page.getByRole("button", { name: "Account", exact: true }).click();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();

  const opened = await page.evaluate(
    () => (window as unknown as { __opened: string | null }).__opened,
  );

  expect(opened, "the Sign in button opened nothing").not.toBeNull();
  // The bug, asserted directly: a relative path here resolves against
  // chrome-extension://, which cannot serve it.
  expect(opened!, "sign-in must not resolve against chrome-extension://").not.toContain(
    "chrome-extension://",
  );
  expect(opened!).toContain(`${WEBAPP_ORIGIN}/login`);
  // The extension's own id has to ride along, or the login page has
  // nowhere to hand the finished session back to.
  expect(opened!, "no extension id for the hand-back").toContain(`ext=${extensionId}`);
});

test("the login origin can reach the extension back", () => {
  const manifest = JSON.parse(
    readFileSync(join(process.cwd(), "public", "manifest.json"), "utf8"),
  ) as { externally_connectable?: { matches?: string[] }; permissions?: string[] };

  // Without this the hand-back has only the opener `postMessage`, which
  // a Cross-Origin-Opener-Policy header anywhere in the OAuth round trip
  // is free to sever.
  expect(manifest.externally_connectable?.matches).toContain(`${WEBAPP_ORIGIN}/*`);

  // And it has to stay a manifest key rather than become a permission.
  // Requesting none at all is this submission's strongest point, and
  // `identity` — the other way to do sign-in — would spend it.
  expect(manifest.permissions ?? []).toEqual([]);
});

test("every in-extension link resolves to a file that exists", async ({ context, extensionId }) => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/index.html`);
  await page
    .locator("header.topbar")
    .getByRole("button", { name: "Open PDF…" })
    .waitFor({ state: "visible", timeout: 30_000 });

  // Anchors pointing back at the extension's own origin. An external
  // https:// link is somebody else's server and not this test's business.
  const internal = await page.evaluate(() =>
    [...document.querySelectorAll("a[href]")]
      .map((a) => (a as HTMLAnchorElement).href)
      .filter((href) => href.startsWith("chrome-extension://")),
  );

  for (const href of new Set(internal)) {
    const response = await page.request.get(href);
    expect(response.status(), `${href} does not resolve inside the package`).toBeLessThan(400);
  }
});

// The specific URL that failed, asserted directly: `/login` is not a file
// in this package and never will be, so nothing may navigate to it.
test("the sign-in path the review hit is still not a file in the package", async ({
  context,
  extensionId,
}) => {
  const page = await context.newPage();
  const response = await page.goto(`chrome-extension://${extensionId}/login`).catch(() => null);

  const missing =
    response === null ||
    response.status() >= 400 ||
    (await page.evaluate(() => document.body.innerText.includes("File not found")));

  expect(missing, "if /login now resolves, this test's premise has changed — re-read it").toBe(true);
});
