// The app's own machine-readable half.
//
// This was site-seo.spec.ts, which covered the marketing page and this
// together. The page moved to its own private repository on 11 September
// 2026 and its tests went with it; what stays here is the one assertion
// that is about the app's build rather than the page.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";

test("the app is one indexable page, not unlimited copies of one", () => {
  // Every path under /app/ answers with the same HTML and a 200 — that is
  // what makes a single-page app work, and it also means a crawler that
  // guesses a URL is told the page exists. The canonical is what
  // collapses them back into one.
  //
  // It names the apex under /app, not app.openpdfedit.com: that host 301s
  // here, and a canonical pointing at a redirect is a mixed signal the
  // search engine resolves by picking a URL itself.
  const dist = join(process.cwd(), "..", "webapp", "dist");
  const html = readFileSync(join(dist, "index.html"), "utf8");
  expect(html).toContain('<link rel="canonical" href="https://openpdfedit.com/app/">');
  expect(html).toContain('<meta name="description"');

  const robots = readFileSync(join(dist, "robots.txt"), "utf8");
  expect(robots).toContain("Sitemap: https://openpdfedit.com/app/sitemap.xml");

  const sitemap = readFileSync(join(dist, "sitemap.xml"), "utf8");
  expect(sitemap).toContain("<loc>https://openpdfedit.com/app/</loc>");
});
