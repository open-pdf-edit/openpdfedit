import { expect, test } from "./fixtures";

// Two defects reported from the field, both of which reproduce here.
//
//   1. Print printed the *editor* — toolbar, tool rail, and whatever
//      slice of the page was on screen — instead of the document.
//   2. A page wider than the viewport could not be scrolled sideways.
//
// They are unrelated in cause and share a spec only because they share a
// fixture: a 1200pt-wide landscape page with unmistakable content at each
// edge, so "can the reader reach the right-hand side?" is answerable by
// looking rather than by reading a number.
const WIDE_PDF_BASE64 = "JVBERi0xLjQKMSAwIG9iajw8L1R5cGUvQ2F0YWxvZy9QYWdlcyAyIDAgUj4+ZW5kb2JqCjIgMCBvYmo8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PmVuZG9iagozIDAgb2JqPDwvVHlwZS9QYWdlL1BhcmVudCAyIDAgUi9NZWRpYUJveFswIDAgMTIwMCA2MDBdL1Jlc291cmNlczw8L0ZvbnQ8PC9GMSA1IDAgUj4+Pj4vQ29udGVudHMgNCAwIFI+PmVuZG9iago0IDAgb2JqPDwvTGVuZ3RoIDI0ND4+c3RyZWFtCkJUIC9GMSA0OCBUZiA0MCA1MDAgVGQgKExFRlQgRURHRSkgVGogRVQKQlQgL0YxIDQ4IFRmIDkwMCA1MDAgVGQgKFJJR0hUIEVER0UpIFRqIEVUCkJUIC9GMSAyMCBUZiA0MCA0MzAgVGQgKFRoaXMgcGFnZSBpcyAxMjAwcHQgd2lkZS4pIFRqIEVUCkJUIC9GMSAyMCBUZiA5MDAgNDMwIFRkIChZb3UgYXJlIGF0IHRoZSBmYXIgcmlnaHQuKSBUaiBFVAo0IHcgNDAgNDAgbSAxMTYwIDQwIGwgUwo0MCA1NjAgbSAxMTYwIDU2MCBsIFMKZW5kc3RyZWFtZW5kb2JqCjUgMCBvYmo8PC9UeXBlL0ZvbnQvU3VidHlwZS9UeXBlMS9CYXNlRm9udC9IZWx2ZXRpY2E+PmVuZG9iagp4cmVmCjAgNgowMDAwMDAwMDAwIDY1NTM1IGYgCjAwMDAwMDAwMDkgMDAwMDAgbiAKMDAwMDAwMDA1MiAwMDAwMCBuIAowMDAwMDAwMTAxIDAwMDAwIG4gCjAwMDAwMDAyMTIgMDAwMDAgbiAKMDAwMDAwMDUwMiAwMDAwMCBuIAp0cmFpbGVyPDwvU2l6ZSA2L1Jvb3QgMSAwIFI+PgpzdGFydHhyZWYKNTYzCiUlRU9GCg==";

async function openWideDocument(page: import("@playwright/test").Page, extensionId: string) {
  await page.setViewportSize({ width: 1100, height: 800 });
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  await page.evaluate(() => {
    (window as unknown as Record<string, unknown>).__e2eFiles = new Map();
    (window as unknown as Record<string, unknown>).__e2eOpenQueue = [];
    class FakeFileHandle {
      constructor(public name: string) {}
      async getFile() {
        const bytes = (window as unknown as { __e2eFiles: Map<string, Uint8Array> }).__e2eFiles.get(this.name);
        return new File([bytes as BlobPart], this.name, { type: "application/pdf" });
      }
      queryPermission = async () => "granted";
      requestPermission = async () => "granted";
    }
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => {
      const names = (window as unknown as { __e2eOpenQueue: string[][] }).__e2eOpenQueue.shift() ?? [];
      return names.map((n) => new FakeFileHandle(n));
    };
  });
  await page.evaluate((b64) => {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    (window as unknown as { __e2eFiles: Map<string, Uint8Array> }).__e2eFiles.set("wide.pdf", bytes);
    (window as unknown as { __e2eOpenQueue: string[][] }).__e2eOpenQueue.push(["wide.pdf"]);
  }, WIDE_PDF_BASE64);

  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator(".path-bar__text")).toHaveText("wide.pdf");
  await page.waitForFunction(() => {
    const el = document.querySelector("canvas") as HTMLCanvasElement | null;
    if (!el || el.width === 0) return false;
    return el.getContext("2d")!.getImageData(0, 0, 1, 1).data[3] === 255;
  });
}

test("a page wider than the viewport can be scrolled to", async ({ page, extensionId }) => {
  await openWideDocument(page, extensionId);

  const zoomIn = page.getByRole("button", { name: "Zoom in" });
  for (let i = 0; i < 2; i++) await zoomIn.click();
  await page.waitForTimeout(400);

  const before = await page.evaluate(() => {
    const sc = document.querySelector(".scroll-container") as HTMLElement;
    return { container: sc.clientWidth, content: sc.scrollWidth };
  });
  expect(before.content, "the fixture must actually overflow, or this proves nothing")
    .toBeGreaterThan(before.container);

  // The gesture a reader would make, not a scrollLeft assignment:
  // "overflow-x: hidden" still accepts a programmatic scroll, so setting
  // it directly would have passed against the bug it is meant to catch.
  const box = await page.locator(".scroll-container").boundingBox();
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await page.mouse.wheel(1200, 0);
  await page.waitForTimeout(400);

  const scrollLeft = await page.evaluate(
    () => (document.querySelector(".scroll-container") as HTMLElement).scrollLeft,
  );
  expect(scrollLeft, "a horizontal wheel gesture must reach the right-hand side").toBeGreaterThan(0);

  // Touch is the other half of the same bug: pan-y alone leaves a wide
  // page unreachable on a tablet even once overflow-x allows scrolling.
  const touchAction = await page.evaluate(
    () => getComputedStyle(document.querySelector(".scroll-container") as HTMLElement).touchAction,
  );
  expect(touchAction, "touch panning must be allowed on both axes").toContain("pan-x");
});

test("printing the page does not print the editor", async ({ page, extensionId }) => {
  await openWideDocument(page, extensionId);

  // What the printer would actually be handed. Before the print
  // stylesheet existed this rendered the whole editor — the topbar, all
  // sixteen tool names down the rail, and the visible slice of the page.
  await page.emulateMedia({ media: "print" });
  await page.waitForTimeout(200);

  for (const chrome of ["Highlight", "Redact", "Compress", "Page numbers"]) {
    await expect(
      page.getByText(chrome, { exact: true }),
      `"${chrome}" is editor furniture and must not reach the printer`,
    ).toBeHidden();
  }

  // The replacement line lives in body::after, so innerText cannot see
  // it — read the generated content itself.
  const printed = await page.evaluate(
    () => getComputedStyle(document.body, "::after").content,
  );
  expect(printed, "print should explain where the Print command is").toContain("Print button");

  await page.emulateMedia({ media: "screen" });
});
