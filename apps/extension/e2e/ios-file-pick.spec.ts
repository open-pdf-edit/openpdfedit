import { expect, test } from "./fixtures";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

// APP-30 — "iphone上不能打开pdf文档，safara和chrome都不行".
//
// Every browser on iOS is WebKit, which is why Safari and Chrome failed
// together: one engine, one bug. There is no `showOpenFilePicker` there,
// so opening goes through an `<input>`, and the helper driving it
// decided a pick had been cancelled when `change` had not fired 500ms
// after focus returned. A PDF in iCloud Drive is not on the device yet —
// it downloads first — so a real pick routinely landed after that and
// was thrown away. The user tapped Open, chose a file, and nothing
// happened.
//
// Driven against the extension page rather than a WebKit build of the
// web app: the branch is chosen by feature detection, so deleting the
// pickers reaches the same code, and it needs no server. What is
// simulated is the timing — focus returning, then the file arriving well
// after the old window had closed.
// The first document opened after a fresh build pays for compiling
// PDFium and the Rust core from a cold cache — measured at 32s against
// 5s once warm, which straddles Playwright's 30s default and made these
// fail only ever on the run right after `npm run build`. `test.slow()`
// triples the budget rather than papering over a real hang: a genuine
// stall still fails, just not at the one moment the engine is cold.
test.slow();

test("a file that takes a moment to arrive is still opened", async ({ page, extensionId }) => {
  await page.addInitScript(() => {
    delete (window as unknown as Record<string, unknown>).showOpenFilePicker;
    delete (window as unknown as Record<string, unknown>).showSaveFilePicker;
  });
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const chooser = page.waitForEvent("filechooser");
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  const fc = await chooser;

  // The sheet closes — iOS fires focus on the window — and only then does
  // the file finish downloading. 1.2s is unremarkable for iCloud, and is
  // what the old 500ms window discarded.
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await page.waitForTimeout(1200);
  await fc.setFiles({
    name: "slow.pdf",
    mimeType: "application/pdf",
    buffer: Buffer.from(TEXT_PDF_BASE64, "base64"),
  });

  await expect(
    page.locator(".path-bar__text"),
    "a slow pick must not be discarded as a cancellation",
  ).toHaveText("slow.pdf", { timeout: 30_000 });
  await expect(page.locator("canvas")).toBeVisible({ timeout: 30_000 });
});

test("the picker input is in the document", async ({ page, extensionId }) => {
  // Safari on iOS does not reliably open a picker for an input that was
  // never in the DOM. Asserted separately because the test above passes
  // either way under Playwright, which drives the chooser directly.
  await page.addInitScript(() => {
    delete (window as unknown as Record<string, unknown>).showOpenFilePicker;
    delete (window as unknown as Record<string, unknown>).showSaveFilePicker;
    (window as unknown as Record<string, unknown>).__attached = [];
    const realClick = HTMLInputElement.prototype.click;
    HTMLInputElement.prototype.click = function (this: HTMLInputElement) {
      if (this.type === "file") {
        (window as unknown as { __attached: boolean[] }).__attached.push(
          document.body.contains(this),
        );
      }
      return realClick.call(this);
    };
  });
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const chooser = page.waitForEvent("filechooser");
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await chooser;

  const attached = await page.evaluate(
    () => (window as unknown as { __attached: boolean[] }).__attached,
  );
  expect(attached, "the file input must be clicked while in the document").toEqual([true]);
});
