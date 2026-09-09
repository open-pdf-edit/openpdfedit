import { expect, test } from "./fixtures";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

// Firefox and Safari have no `showOpenFilePicker`, and neither plans to
// ship one. A document opened there arrives as a `File` from an
// `<input>` — a snapshot that cannot outlive the page — so there was no
// handle to keep and the Recent list stayed empty in those browsers.
// Reported as "the Firefox version does not have history like Chrome".
//
// They do have the Origin Private File System, so a copy goes there and
// the row reopens the document instead of reopening a picker.
//
// Run against the extension page rather than the web app: the browsers
// this is about cannot run a Chrome extension, but the code path is
// selected at runtime by feature detection, so deleting the two pickers
// reaches exactly the same branch — and it needs no external server,
// which the web-app specs do.
// The first document opened after a fresh build pays for compiling
// PDFium and the Rust core from a cold cache — measured at 32s against
// 5s once warm, which straddles Playwright's 30s default and made these
// fail only ever on the run right after `npm run build`. `test.slow()`
// triples the budget rather than papering over a real hang: a genuine
// stall still fails, just not at the one moment the engine is cold.
test.slow();

test("with no file picker API, a document is still remembered and reopens", async ({
  page,
  extensionId,
}) => {
  await page.addInitScript(() => {
    delete (window as unknown as Record<string, unknown>).showOpenFilePicker;
    delete (window as unknown as Record<string, unknown>).showSaveFilePicker;
  });
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const chooser = page.waitForEvent("filechooser");
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await (await chooser).setFiles({
    name: "remembered.pdf",
    mimeType: "application/pdf",
    buffer: Buffer.from(TEXT_PDF_BASE64, "base64"),
  });
  await expect(page.locator(".path-bar__text")).toHaveText("remembered.pdf", { timeout: 30_000 });

  // The row is recorded only after the copy is safely on disk — a row
  // that cannot be reopened is the thing this path exists to avoid — so
  // wait for the copy rather than for the row. Reloading mid-write is
  // how this test failed first, and the answer was correct: nothing had
  // been promised yet.
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          try {
            const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("recents");
            let n = 0;
            for await (const _ of (dir as unknown as { keys(): AsyncIterable<string> }).keys()) n += 1;
            return n;
          } catch {
            return 0;
          }
        }),
      { timeout: 30_000 },
    )
    .toBeGreaterThan(0);

  // The list is re-read only while no document is open (+page.svelte's
  // effect reads `doc` and returns early when there is one), so the row
  // is not expected to appear over the document that just created it —
  // true in Chrome as well.
  //
  // Reload, which is the real test anyway: neither a handle nor a File
  // survives it. Only the copy does.
  await page.reload();
  const row = page.getByRole("button", { name: /remembered\.pdf/ }).first();
  await expect(
    row,
    "a browser with no handle to keep must still remember the document",
  ).toBeVisible({ timeout: 30_000 });

  await row.click();
  await expect(
    page.locator(".path-bar__text"),
    "the row must reopen the document, not a picker",
  ).toHaveText("remembered.pdf", { timeout: 30_000 });
  await expect(page.locator("canvas")).toBeVisible({ timeout: 30_000 });
});

test("clearing the list deletes the copies it kept", async ({ page, extensionId }) => {
  // "Clear" is the control that promises the copies are gone. If it only
  // emptied localStorage the bytes would stay in OPFS, invisible and
  // unreachable, which is the worst of both.
  await page.addInitScript(() => {
    delete (window as unknown as Record<string, unknown>).showOpenFilePicker;
    delete (window as unknown as Record<string, unknown>).showSaveFilePicker;
  });
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const chooser = page.waitForEvent("filechooser");
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await (await chooser).setFiles({
    name: "throwaway.pdf",
    mimeType: "application/pdf",
    buffer: Buffer.from(TEXT_PDF_BASE64, "base64"),
  });
  await expect(page.locator(".path-bar__text")).toHaveText("throwaway.pdf", { timeout: 30_000 });

  const copiesNow = async () =>
    page.evaluate(async () => {
      try {
        const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("recents");
        let n = 0;
        for await (const _ of (dir as unknown as { keys(): AsyncIterable<string> }).keys()) n += 1;
        return n;
      } catch {
        return 0;
      }
    });

  await expect.poll(copiesNow, { timeout: 30_000 }).toBeGreaterThan(0);

  // Back to the start screen, where the list is re-read and Clear lives.
  await page.reload();
  await expect(page.getByRole("button", { name: /throwaway\.pdf/ }).first()).toBeVisible({
    timeout: 30_000,
  });
  await page.getByRole("button", { name: "Clear" }).click();
  await expect.poll(copiesNow, { timeout: 30_000 }).toBe(0);
});
