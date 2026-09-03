// Nothing in the extension may lead somewhere the extension cannot go.
//
// This exists because an Edge review failed on exactly that. Signing in
// opened `/login`; the web app's server rewrites unknown paths to the SPA
// fallback, so it works there, while `chrome-extension://` has no server
// and answers a literal file lookup. The reviewer got Chrome's "File not
// found. It may have been moved, edited, or deleted." and reported the
// product's primary functions as unusable.
//
// Every existing test missed it. `boot.spec.ts` navigates straight to
// `index.html`, and every flow test stubs the file picker, so nothing
// ever followed a link the app offers and checked that it resolves.
//
// Two things are asserted, because fixing only the first would move the
// failure rather than remove it:
//
//   1. No account surface is offered in the extension at all. The
//      sign-in page redirects the whole window out to an OAuth provider
//      and back, and a `chrome-extension://` redirect URI is not
//      something those providers accept — so a working `/login` would
//      still dead-end, one step later and less legibly.
//   2. Every same-origin URL the page does offer resolves to something
//      real.
import { expect, test } from "./fixtures";

/** Tools that need the account, and so cannot work here. Named by their
 * accessible names, which is what a reviewer clicking around sees. */
const ACCOUNT_GATED = ["Account", "OCR document", "Watermark document"];

test("the extension offers nothing that needs an account", async ({ context, extensionId }) => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/index.html`);
  await page
    .locator("header.topbar")
    .getByRole("button", { name: "Open PDF…" })
    .waitFor({ state: "visible", timeout: 30_000 });

  for (const name of ACCOUNT_GATED) {
    await expect(
      page.getByRole("button", { name, exact: true }),
      `"${name}" is offered in the extension, where signing in cannot work`,
    ).toHaveCount(0);
  }
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
test("the sign-in path the review hit is not reachable", async ({ context, extensionId }) => {
  const page = await context.newPage();
  const response = await page.goto(`chrome-extension://${extensionId}/login`).catch(() => null);

  // Chrome serves a real error page for a missing extension resource
  // rather than throwing, so "did it load" is not the question — "is
  // there anything there" is.
  const missing =
    response === null ||
    response.status() >= 400 ||
    (await page.evaluate(() => document.body.innerText.includes("File not found")));

  expect(missing, "if /login now resolves, this test's premise has changed — re-read it").toBe(true);
});
