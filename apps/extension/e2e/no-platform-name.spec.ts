// The platform is never named on a product surface.
//
// A reader of openpdfedit.com has never heard of OpenApps. A heading
// that says "Sign in to OpenApps" in a window titled OpenPdfEdit asks
// for a password under a name the person does not recognise, which reads
// exactly like the thing people are told to be suspicious of.
//
// It also undoes deliberate work: every account hostname is masked
// behind this product's own domain (auth.openpdfedit.com,
// gateway.openpdfedit.com) so the platform never appears in an address
// bar or a permission prompt. Naming it in prose gives that back.
//
// The live risk is the account SDK: `<openapps-login>` defaults its
// heading to "Sign in to OpenApps", so any page that mounts it without
// an explicit `heading` shows the platform's name. login/+page.svelte
// overrides it; this is what keeps that true.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "./fixtures";

const SRC = join(import.meta.dirname, "..", "..", "desktop", "src");

test("no surface renders the platform's name", async ({ page, extensionId }) => {
  await page.goto(`chrome-extension://${extensionId}/index.html`);
  await page.waitForTimeout(500);
  // Shadow roots too: the account elements render inside them, so a
  // plain innerText check would not see what they paint.
  const found = await page.evaluate(() => {
    const out: string[] = [];
    const scan = (root: Document | ShadowRoot) => {
      const t = (root as Document).body?.innerText ?? (root as ShadowRoot).textContent ?? "";
      for (const m of String(t).matchAll(/.{0,40}OpenApps.{0,40}/g)) out.push(m[0].replace(/\s+/g, " "));
      for (const el of root.querySelectorAll("*")) {
        const sr = (el as HTMLElement & { shadowRoot?: ShadowRoot }).shadowRoot;
        if (sr) scan(sr);
      }
    };
    scan(document);
    return [...new Set(out)];
  });
  expect(found, "the app must not print the platform's name").toEqual([]);
});

test("every mounted account element that has a heading sets its own", () => {
  // The source check the rendered one cannot do: the sign-in page is a
  // full-window redirect flow, so it is not reachable from the packaged
  // build above, and its heading is exactly the one that defaults wrong.
  const login = readFileSync(join(SRC, "routes", "login", "+page.svelte"), "utf8");
  expect(login, "<openapps-login> must be mounted").toContain("<openapps-login");
  const heading = /heading="([^"]*)"/.exec(login)?.[1];
  expect(heading, "it must carry an explicit heading").toBeTruthy();
  expect(heading).not.toMatch(/OpenApps/);

  // And nothing else in the app's own copy may print the word.
  for (const f of ["routes/+page.svelte", "lib/AccountPanel.svelte", "lib/SupporterGate.svelte"]) {
    const src = readFileSync(join(SRC, f), "utf8");
    const rendered = src
      .replace(/<!--[\s\S]*?-->/g, "")          // comments are notes to us
      .replace(/\/\/[^\n]*/g, "")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/from "[^"]*"/g, "");             // import paths are identifiers
    expect(rendered, `${f} must not print the platform's name`).not.toMatch(/OpenApps/);
  }
});
