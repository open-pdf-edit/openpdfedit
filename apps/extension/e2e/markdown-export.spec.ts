import { expect, test } from "@playwright/test";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const ORIGIN = "http://localhost:8099";

/**
 * PDF to Markdown.
 *
 * The conversion is Firecrawl's `anydoc`, loaded as WebAssembly from
 * this origin when someone asks for it — not linked into the bundle,
 * where it added eight megabytes to a four-megabyte download, and not
 * called over the network, which would mean uploading the document.
 * Both of those are what this test is really checking: that the
 * conversion happens, and that nothing leaves the origin to make it
 * happen.
 */
test("converts the open document, without asking anyone else", async ({ browser }) => {
  const ctx = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    acceptDownloads: true,
  });
  const page = await ctx.newPage();

  const offOrigin: string[] = [];
  page.on("request", (request) => {
    if (!request.url().startsWith(ORIGIN) && !request.url().startsWith("data:")) {
      offOrigin.push(request.url());
    }
  });

  await page.addInitScript((base64: string) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      {
        name: "report.pdf",
        async getFile() {
          return new File([bytes], "report.pdf", { type: "application/pdf" });
        },
        async createWritable() {
          return { async write() {}, async close() {} };
        },
      },
    ];
  }, TEXT_PDF_BASE64);

  await page.goto(ORIGIN);
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "Export as Markdown" }).click();

  // Chromium can write into a folder, so it is offered the choice.
  const choice = page.locator(".oa-dialog select");
  if (await choice.count()) {
    await choice.selectOption("file");
    await page.locator(".oa-dialog").getByRole("button", { name: "Export", exact: true }).click();
  }

  const download = await page.waitForEvent("download", { timeout: 60_000 });
  expect(download.suggestedFilename(), "named after the document, not the format").toBe(
    "report.md",
  );

  const path = await download.path();
  const { readFileSync } = await import("node:fs");
  const markdown = readFileSync(path!, "utf8");
  expect(markdown, "the document's own text has to be in it").toContain("CONFIDENTIAL");

  expect(offOrigin, "converting must not involve anyone else").toEqual([]);
  await ctx.close();
});

test("a vault is offered only where one can be written to", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();

  // Firefox and Safari have no directory picker, so there is nowhere to
  // put a vault and nothing to ask about — the file is downloaded and
  // the dialog never appears.
  await page.addInitScript((base64: string) => {
    delete (window as unknown as Record<string, unknown>).showDirectoryPicker;
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      {
        name: "report.pdf",
        async getFile() {
          return new File([bytes], "report.pdf", { type: "application/pdf" });
        },
        async createWritable() {
          return { async write() {}, async close() {} };
        },
      },
    ];
  }, TEXT_PDF_BASE64);

  await page.goto(ORIGIN);
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "Export as Markdown" }).click();
  await expect(page.locator(".oa-dialog select")).toHaveCount(0);

  await ctx.close();
});

test("a chosen folder is written into, and remembered", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();

  // A stand-in for the folder picker. Playwright cannot drive the real
  // one — there is no way to answer an OS directory dialog — so what is
  // under test is everything after it: storing the handle, asking for
  // permission on the click that writes, and writing the file.
  await page.addInitScript((base64: string) => {
    const written: Record<string, string> = {};
    (window as unknown as Record<string, unknown>).__written = written;
    (window as unknown as Record<string, unknown>).showDirectoryPicker = async () => ({
      name: "Notes",
      kind: "directory",
      requestPermission: async () => "granted",
      async getFileHandle(name: string) {
        return {
          async createWritable() {
            let text = "";
            return {
              async write(chunk: string) {
                text += chunk;
              },
              async close() {
                written[name] = text;
              },
            };
          },
        };
      },
    });

    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      {
        name: "report.pdf",
        async getFile() {
          return new File([bytes], "report.pdf", { type: "application/pdf" });
        },
        async createWritable() {
          return { async write() {}, async close() {} };
        },
      },
    ];
  }, TEXT_PDF_BASE64);

  await page.goto(ORIGIN);
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "Export as Markdown" }).click();
  await page.locator(".oa-dialog select").selectOption("pick");
  await page.locator(".oa-dialog").getByRole("button", { name: "Export", exact: true }).click();

  // `toHaveProperty` reads a dot as a path separator, so a file name is
  // the one kind of key it cannot be given.
  await expect
    .poll(() =>
      page.evaluate(() =>
        Object.keys((window as { __written?: Record<string, string> }).__written ?? {}),
      ),
    )
    .toContain("report.md");
  const contents = await page.evaluate(
    () => (window as { __written?: Record<string, string> }).__written!["report.md"],
  );
  expect(contents).toContain("CONFIDENTIAL");

  // And it is offered by name next time, which is the whole point of a
  // vault as opposed to a save dialog.
  await page.getByRole("button", { name: "Export as Markdown" }).click();
  await expect(page.locator(".oa-dialog")).toContainText("Save to Notes");

  await ctx.close();
});
