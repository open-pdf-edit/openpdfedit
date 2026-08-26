import { expect, test } from "@playwright/test";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const ORIGIN = "http://localhost:8099";

/**
 * Selecting text with the Select tool.
 *
 * The tool was named Select and could not select anything: it
 * hit-tested annotations and offered to delete them, and a drag over
 * words did nothing at all. In a PDF reader that is close to the first
 * thing anyone tries.
 *
 * Both behaviours have to survive, because a click and a drag are the
 * same gesture until the mouse comes up.
 */
async function openDocument(page: import("@playwright/test").Page) {
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
}

/** Where a word is on screen, found through search rather than assumed —
 * the fixture's layout is not this test's business.
 *
 * The find bar stays open, and the hits stay drawn under the layer that
 * takes the drag. Closing it moves everything below it up by its own
 * height, and clearing the query scrolls back — either way a coordinate
 * measured beforehand points somewhere else by the time it is used, and
 * the drag quietly selects nothing. Both were tried; this is what
 * survived.
 */
async function boxOf(page: import("@playwright/test").Page, word: string) {
  await page.keyboard.press("Control+f");
  const find = page.getByRole("textbox", { name: "Find in document" });
  await find.fill(word);
  const hit = page.locator(".search-hit").first();
  await expect(hit).toBeVisible({ timeout: 15_000 });
  // Into view before measuring: the viewer scrolls to a hit, and a box
  // read before that settles can be below the fold, where a mouse
  // cannot reach it.
  await hit.scrollIntoViewIfNeeded();
  const box = (await hit.boundingBox())!;
  return box;
}

/** Drag from one side of a box to the other, along its middle. */
async function dragAcross(
  page: import("@playwright/test").Page,
  box: { x: number; y: number; width: number; height: number },
) {
  const y = box.y + box.height / 2;
  await page.mouse.move(box.x + 3, y);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 3, y, { steps: 12 });
  await page.mouse.up();
}

test("dragging over words selects them, and ⌘C copies them", async ({ browser }) => {
  const ctx = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    permissions: ["clipboard-read", "clipboard-write"],
  });
  const page = await ctx.newPage();
  await openDocument(page);

  const box = await boxOf(page, "CONFIDENTIAL");
  await dragAcross(page, box);

  // Drawn where the characters are, not where the mouse went: the
  // backend snaps the drag onto the real glyphs PDFium reports.
  const drawn = page.locator(".text-selection");
  await expect(drawn).toHaveCount(1, { timeout: 15_000 });
  const selected = (await drawn.boundingBox())!;
  expect(Math.abs(selected.x - box.x), "the selection sits on the word").toBeLessThan(8);

  // Focus was in the find box; the drag moved it back to the page, which
  // is what lets ⌘C mean "copy the selection" rather than "copy what I
  // just typed into the find box".
  await page.keyboard.press("Control+c");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("CONFIDENTIAL");

  await ctx.close();
});

test("a click is still a click, and switching tools clears the selection", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await openDocument(page);

  const box = await boxOf(page, "CONFIDENTIAL");

  // A click on bare page: no selection, and no dialog — there is no
  // annotation under it to offer to delete.
  await page.mouse.click(box.x + box.width + 120, box.y + box.height / 2);
  await expect(page.locator(".text-selection")).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // Select something, then pick another tool: the quads must go, or they
  // sit over text they no longer describe.
  await dragAcross(page, box);
  await expect(page.locator(".text-selection")).toHaveCount(1, { timeout: 15_000 });

  await page.getByRole("button", { name: "Highlight", exact: true }).click();
  await expect(page.locator(".text-selection")).toHaveCount(0);

  await ctx.close();
});
