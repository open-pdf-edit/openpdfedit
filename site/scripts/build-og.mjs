// Renders site/og.png — the 1200x630 card every share, preview and
// answer-engine result shows.
//
// The page declares `twitter:card: summary_large_image` and pointed at
// no image at all, so every link to it unfurled as a blank rectangle
// with a hostname under it.
//
// Drawn by screenshotting a page rather than by hand so it uses the
// site's own fonts and tokens, and so re-running it after a wording
// change costs nothing. Chromium comes from the e2e suite's Playwright.
//
//   node site/scripts/build-og.mjs
import { chromium } from "../../apps/extension/node_modules/playwright/index.mjs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const siteDir = resolve(here, "..");

const html = `<!doctype html><meta charset="utf-8">
<style>
  @font-face{font-family:"Geist";src:url("fonts/Geist-Light.woff2") format("woff2");font-weight:300}
  @font-face{font-family:"Geist";src:url("fonts/Geist-Medium.woff2") format("woff2");font-weight:500}
  @font-face{font-family:"Geist Mono";src:url("fonts/GeistMono-Regular.woff2") format("woff2");font-weight:400}
  *{margin:0;padding:0;box-sizing:border-box}
  body{
    width:1200px;height:630px;background:#111;color:#fff;
    font-family:"Geist",-apple-system,sans-serif;
    display:flex;flex-direction:column;justify-content:space-between;
    padding:72px 80px;-webkit-font-smoothing:antialiased;
  }
  .mark{font-weight:500;font-size:26px;letter-spacing:-.04em}
  .dot{color:#ff4d4d}
  h1{font-weight:300;font-size:88px;line-height:1.04;letter-spacing:-.045em}
  .sub{margin-top:26px;font-size:26px;line-height:1.45;color:rgba(255,255,255,.6);max-width:22ch}
  .foot{display:flex;justify-content:space-between;align-items:flex-end}
  .tags{font-family:"Geist Mono",monospace;font-size:15px;letter-spacing:.04em;
        text-transform:uppercase;color:rgba(255,255,255,.4)}
  .rule{height:1px;background:rgba(255,255,255,.1);margin-bottom:26px}
</style>
<div class="mark">OpenPdfEdit<span class="dot">.</span></div>
<div>
  <h1>Edit PDFs<span class="dot">.</span><br>Keep them yours<span class="dot">.</span></h1>
  <p class="sub">Every edit happens on your machine. Nothing is uploaded.</p>
</div>
<div>
  <div class="rule"></div>
  <div class="foot">
    <span class="tags">macOS · Windows · Browser · Chrome</span>
    <span class="tags">openpdfedit.com</span>
  </div>
</div>`;

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1200, height: 630 },
  deviceScaleFactor: 1,
});
// A file:// base so the @font-face URLs resolve to the real woff2 files;
// setContent alone would leave them unresolved and fall back to a system
// face, which is the one thing this is meant to avoid.
await page.goto(`file://${siteDir}/`);
await page.setContent(html);
await page.evaluate(() => document.fonts.ready);
await page.screenshot({ path: `${siteDir}/og.png` });
await browser.close();
console.log("wrote site/og.png (1200x630)");
