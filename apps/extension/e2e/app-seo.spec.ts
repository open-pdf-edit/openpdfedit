// What a crawler, and a link preview, get from openpdfedit.com/app/.
//
// This was site-seo.spec.ts, which covered the marketing page and this
// together. The page moved to its own private repository on 11 September
// 2026 and its tests went with it; what stays here is about the app's
// build.
//
// APP-96 is most of it. The served HTML used to be an empty <div> under a
// <title> that was only the brand — nothing for a crawler that runs no
// JavaScript to read, and a bare URL wherever the link was shared — and
// the build emitted a second sitemap that listed /app/ with a different
// lastmod from the site's own.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";
import { ORIGIN } from "./origin";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const dist = join(process.cwd(), "..", "webapp", "dist");
const html = () => readFileSync(join(dist, "index.html"), "utf8");

test("the app is one indexable page, not unlimited copies of one", () => {
  // Every path under /app/ answers with the same HTML and a 200 — that is
  // what makes a single-page app work, and it also means a crawler that
  // guesses a URL is told the page exists. The canonical is what
  // collapses them back into one.
  //
  // It names the apex under /app, not app.openpdfedit.com: that host 301s
  // here, and a canonical pointing at a redirect is a mixed signal the
  // search engine resolves by picking a URL itself.
  expect(html()).toContain('<link rel="canonical" href="https://openpdfedit.com/app/">');
  expect(html()).toContain('<meta name="description"');
});

test("a crawler that runs no JavaScript reads a heading and a page of text", () => {
  // In the HTML itself, not added by a script: GPTBot, ClaudeBot and
  // PerplexityBot run none, and Googlebot queues rendering behind
  // everything else.
  const page = html();
  const body = page.slice(page.indexOf("<body")).replace(/<script[\s\S]*?<\/script>/g, "");
  const words = body.replace(/<[^>]*>/g, " ").split(/\s+/).filter(Boolean).length;
  expect(words, "words of static body text").toBeGreaterThanOrEqual(350);
  expect(words, "words of static body text").toBeLessThanOrEqual(450);
  expect(page.match(/<h1[\s>]/g) ?? []).toHaveLength(1);
  expect(page).toContain("<h1 id=\"landing-copy-title\">Free Online PDF Editor &amp; Viewer</h1>");
  // Only what the web app does: none of the conversions, unlocking or
  // watermark removal its competitors are known for.
  expect(body).not.toMatch(/\b(Word|Excel|PowerPoint|JPG|PNG)\b|remove (a )?watermark|unlock/i);
});

test("the title and a shared link say what the page is", () => {
  const page = html();
  const title = page.match(/<title>([^<]*)<\/title>/)?.[1].replace(/&amp;/g, "&");
  expect(title).toBe("Free Online PDF Editor & Viewer — No Upload | OpenPdfEdit");
  expect(title!.length).toBeLessThanOrEqual(60);
  for (const property of ["og:type", "og:url", "og:title", "og:description", "og:image"]) {
    expect(page, property).toContain(`<meta property="${property}"`);
  }
  expect(page).toContain('<meta property="og:image" content="https://openpdfedit.com/og.png">');
  expect(page).toContain('<meta name="twitter:card" content="summary_large_image">');
});

test("the build publishes no sitemap of its own, and no analytics", () => {
  // /app/ is an ordinary URL on openpdfedit.com, listed by the site's own
  // sitemap; a second one here gave the same URL a different lastmod.
  expect(existsSync(join(dist, "sitemap.xml"))).toBe(false);
  expect(readFileSync(join(dist, "robots.txt"), "utf8")).not.toMatch(/^Sitemap:/m);
  // privacy.html promises the app contains no analytics.
  expect(html()).not.toMatch(/umami|googletagmanager|gtag\(/i);
});

test("a person sees the same copy under Open PDF, and it gives way to a document", async ({ page }) => {
  await page.addInitScript((base64: string) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      {
        name: "doc.pdf",
        async getFile() { return new File([bytes], "doc.pdf", { type: "application/pdf" }); },
        async createWritable() { return { async write() {}, async close() {} }; },
      },
    ];
  }, TEXT_PDF_BASE64);
  await page.goto(ORIGIN);

  // Moved into the empty state, visible, and not duplicated.
  const copy = page.locator(".empty-state #landing-copy");
  await expect(copy).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("#landing-copy")).toHaveCount(1);
  await expect(copy.getByRole("heading", { level: 1 })).toHaveText("Free Online PDF Editor & Viewer");
  // The copy makes the column taller than the screen; the button above it
  // must keep its height rather than shrink to make room.
  const button = page.locator(".empty-state").getByRole("button", { name: "Open PDF…" });
  const box = await button.boundingBox();
  expect(box!.height, "Open PDF button height").toBeGreaterThanOrEqual(28);

  // With a document open, the document has the screen.
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas")).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("#landing-copy")).toHaveCount(0);
});

test("under a translated interface the English copy is taken out, not hidden", async ({ browser }) => {
  const context = await browser.newContext({ locale: "zh-CN" });
  const page = await context.newPage();
  await page.goto(ORIGIN);
  await expect(page.locator(".empty-state")).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("#landing-copy")).toHaveCount(0);
  await context.close();
});
