import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

test("ocr highlight placement", async ({ browser }) => {
  test.setTimeout(300_000);
  const bytes = readFileSync("/Users/dariuskohsg/Downloads/sharing_folder/ocr-test.pdf");
  const ctx = await browser.newContext({ viewport: { width: 1600, height: 950 } });
  const page = await ctx.newPage();
  await page.route("**/v1/credits/entitlement*", (r) =>
    r.fulfill({ status: 200, contentType: "application/json", body: '{"unlocked":true}' }));
  await page.addInitScript((b64: string) => {
    localStorage.setItem("openapps.session", JSON.stringify({ accessToken: "t", refreshToken: "r" }));
    const arr = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      { name: "ocr-test.pdf", async getFile() { return new File([arr], "ocr-test.pdf", { type: "application/pdf" }); }, async createWritable() { return { async write() {}, async close() {} }; } },
    ];
  }, bytes.toString("base64"));

  await page.goto("http://localhost:8099");
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await page.locator("canvas").first().waitFor({ timeout: 60_000 });
  await page.getByRole("button", { name: "OCR document" }).click();
  await page.locator(".oa-dialog select").selectOption("chi_sim+eng");
  await page.getByRole("button", { name: "Run OCR" }).click();
  await expect(page.getByRole("button", { name: /^(Save|Download a copy)$/ })).toBeEnabled({ timeout: 250_000 });

  await page.keyboard.press("Control+f");
  const find = page.getByRole("textbox", { name: "Find in document" });
  // Recognised at image x 741..900 of a 4000px-wide render, i.e. 18.5%
  // to 22.5% across the page.
  for (const [q, from, to] of [["四年级", 0.185, 0.225], ["暑期", 0.23, 0.258], ["模拟卷", 0.379, 0.417]] as const) {
    await find.fill(q);
    await page.waitForTimeout(2500);
    const count = await page.locator(".find-bar__count").innerText();
    const hit = page.locator(".search-hit").first();
    if (!(await hit.count())) { console.log(`>>> ${q}: ${count} (no hit box)`); continue; }
    const box = (await hit.boundingBox())!;
    const pageBox = (await page.locator("[data-page-index='0']").boundingBox())!;
    const x0 = (box.x - pageBox.x) / pageBox.width;
    const x1 = (box.x + box.width - pageBox.x) / pageBox.width;
    console.log(`>>> ${q}: ${count} at ${x0.toFixed(3)}..${x1.toFixed(3)} (expected ~${from}..${to})`);
    if (q === "四年级") {
      await hit.scrollIntoViewIfNeeded();
      await page.waitForTimeout(400);
      const b = (await hit.boundingBox())!;
      await page.screenshot({
        path: "/tmp/ocr-highlight.png",
        clip: { x: Math.max(0, b.x - 260), y: Math.max(0, b.y - 60), width: 900, height: 170 },
      });
    }
  }
});
