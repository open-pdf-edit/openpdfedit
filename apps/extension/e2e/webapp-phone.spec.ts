import { expect, test, devices } from "@playwright/test";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const ORIGIN = "http://localhost:8099";

/**
 * The phone layout, and the promise that it changes nothing on a desktop.
 *
 * Measured on an iPhone 13 before this existed: 24 buttons in a 430px
 * topbar wrapping over the path bar, a second tool row off the right
 * edge, a 56px rail eating the width, and a page rendered at 816px in a
 * 390px window — half a document with nothing to say the rest was there.
 *
 * Nothing is duplicated to fix it. The same buttons are drawn elsewhere,
 * which is why the desktop case below matters as much as the phone ones:
 * a regression here would most likely show up as the desktop quietly
 * inheriting a phone arrangement.
 */
async function openDocument(page: import("@playwright/test").Page) {
  await page.addInitScript((base64: string) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      {
        name: "doc.pdf",
        async getFile() {
          return new File([bytes], "doc.pdf", { type: "application/pdf" });
        },
        async createWritable() {
          return { async write() {}, async close() {} };
        },
      },
    ];
  }, TEXT_PDF_BASE64);
  await page.goto(ORIGIN);
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator(".path-bar__text")).toHaveText("doc.pdf", { timeout: 30_000 });
  await expect(page.locator("canvas")).toBeVisible({ timeout: 30_000 });
}

for (const [label, device] of [
  ["iPhone 13", devices["iPhone 13"]],
  ["Pixel 5", devices["Pixel 5"]],
] as const) {
  test(`${label}: the page fits, the tools reach the thumb`, async ({ browser }) => {
    const ctx = await browser.newContext({ ...device });
    const page = await ctx.newPage();
    await openDocument(page);

    const m = await page.evaluate(() => {
      const rail = document.querySelector(".rail") as HTMLElement;
      const canvas = document.querySelector("canvas") as HTMLElement;
      const more = document.querySelector(".topbar__more") as HTMLElement;
      return {
        pageOverflows: document.documentElement.scrollWidth > window.innerWidth + 1,
        canvasW: Math.round(canvas.getBoundingClientRect().width),
        winW: window.innerWidth,
        winH: window.innerHeight,
        railDirection: getComputedStyle(rail).flexDirection,
        railBottom: Math.round(rail.getBoundingClientRect().bottom),
        moreShown: getComputedStyle(more).display !== "none",
      };
    });

    expect(m.pageOverflows, "nothing may push the page sideways").toBe(false);
    // Fitted, not 100%: a Letter page at 100% is 816px, twice a phone.
    expect(m.canvasW, "the page should fill the width it has").toBeLessThanOrEqual(m.winW);
    expect(m.canvasW).toBeGreaterThan(m.winW * 0.85);
    // Tools along the bottom, where a thumb reaches, not down the side.
    expect(m.railDirection).toBe("row");
    expect(m.railBottom).toBe(m.winH);
    expect(m.moreShown, "the document tools need somewhere to live").toBe(true);

    // And the sheet actually opens, above the tool bar rather than over it.
    await page.locator(".topbar__more").click();
    const sheet = page.locator(".topbar__group--overflow.is-open");
    await expect(sheet).toBeVisible();
    const box = (await sheet.boundingBox())!;
    expect(Math.round(box.y + box.height), "the sheet must sit above the tool bar").toBeLessThanOrEqual(
      m.winH - 50,
    );

    await ctx.close();
  });
}

test("a desktop window is untouched by any of it", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await openDocument(page);

  const m = await page.evaluate(() => {
    const rail = document.querySelector(".rail") as HTMLElement;
    const canvas = document.querySelector("canvas") as HTMLElement;
    const more = document.querySelector(".topbar__more") as HTMLElement;
    return {
      railDirection: getComputedStyle(rail).flexDirection,
      railWidth: Math.round(rail.getBoundingClientRect().width),
      canvasW: Math.round(canvas.getBoundingClientRect().width),
      moreShown: getComputedStyle(more).display !== "none",
    };
  });

  expect(m.railDirection, "the rail stays down the side").toBe("column");
  expect(m.railWidth).toBe(56);
  // 100%, not fitted: on a desktop a page fits anyway, and a predictable
  // scale is worth more than filling the window.
  expect(m.canvasW, "a Letter page at 100% is 816px").toBe(816);
  expect(m.moreShown, "a desktop must not get a button for a sheet it never renders").toBe(false);

  await ctx.close();
});

/**
 * Touch on the page itself.
 *
 * The annotation layer covers every page edge to edge and used to carry
 * `touch-action: none` unconditionally, so on a phone a finger dragged
 * anywhere over a page scrolled nothing and pinching zoomed nothing —
 * the document was reachable only through the scrollbar it does not
 * have. The tools that need the gesture still take it; the ones that
 * only need a point no longer do.
 */
test("a finger can scroll the document", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  await openDocument(page);

  const touchAction = await page.evaluate(
    () => getComputedStyle(document.querySelector(".interaction-layer")!).touchAction,
  );
  expect(touchAction, "the select tool must not swallow scrolling").not.toBe("none");

  // A drawing tool is the other half: it has to keep the gesture, or a
  // highlight drawn across a line scrolls the line away mid-stroke.
  await page.getByRole("button", { name: "Highlight", exact: true }).click();
  const drawing = await page.evaluate(
    () => getComputedStyle(document.querySelector(".interaction-layer")!).touchAction,
  );
  expect(drawing, "a drawing tool must own the gesture").toBe("none");

  await ctx.close();
});

test("a finger dragged under a tap tool scrolls instead of acting", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  await openDocument(page);

  // Note is the visible one of the tap tools: it opens a dialog, so both
  // outcomes can be seen. Erase behaves identically and is the one that
  // would hurt — flicking through a document would delete whatever the
  // flick happened to start on.
  await page.getByRole("button", { name: "Note", exact: true }).click();

  const layer = page.locator(".interaction-layer");
  const box = (await layer.boundingBox())!;
  const x = Math.round(box.x + box.width / 2);
  const y = Math.round(box.y + box.height / 2);

  const touch = (type: string, cx: number, cy: number) =>
    layer.dispatchEvent(type, {
      pointerId: 1,
      pointerType: "touch",
      isPrimary: true,
      bubbles: true,
      clientX: cx,
      clientY: cy,
    });

  // A scroll: down, well away, up.
  await touch("pointerdown", x, y);
  await touch("pointermove", x, y - 120);
  await touch("pointerup", x, y - 120);
  await expect(page.getByRole("dialog"), "a scroll must not add a note").toHaveCount(0);

  // A tap: down and up in the same place.
  await touch("pointerdown", x, y);
  await touch("pointerup", x + 2, y + 1);
  await expect(page.getByRole("dialog")).toBeVisible();

  await ctx.close();
});

/**
 * Pinching changes the app's zoom, not the browser's.
 *
 * A browser pinch scales the visual viewport: the page gets bigger by
 * being a 100% bitmap enlarged, so it blurs, and the toolbar is enlarged
 * with it. Zooming the app instead re-renders the page at the new size.
 */
test("pinching zooms the document, sharply", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  await openDocument(page);

  // Reserved, or the browser takes the gesture and none of this runs.
  const reserved = await page.evaluate(
    () => getComputedStyle(document.querySelector(".scroll-container")!).touchAction,
  );
  expect(reserved, "the browser must not keep pinch for itself").toBe("pan-y");

  const zoomLabel = page.locator(".zoom-level");
  const before = await zoomLabel.innerText();
  const canvasBefore = (await page.locator("canvas").first().boundingBox())!.width;

  const container = page.locator(".scroll-container");
  const box = (await container.boundingBox())!;
  const cx = Math.round(box.x + box.width / 2);
  const cy = Math.round(box.y + box.height / 2);

  const finger = (type: string, id: number, x: number, y: number) =>
    container.dispatchEvent(type, {
      pointerId: id,
      pointerType: "touch",
      isPrimary: id === 1,
      bubbles: true,
      clientX: x,
      clientY: y,
    });

  // Two fingers 40px apart, spread to 160px: four times the zoom, less
  // whatever the clamp allows.
  await finger("pointerdown", 1, cx - 20, cy);
  await finger("pointerdown", 2, cx + 20, cy);
  await finger("pointermove", 1, cx - 80, cy);
  await finger("pointermove", 2, cx + 80, cy);
  await finger("pointerup", 1, cx - 80, cy);
  await finger("pointerup", 2, cx + 80, cy);

  await expect(zoomLabel).not.toHaveText(before);
  const after = Number.parseInt(await zoomLabel.innerText(), 10);
  expect(after, "spreading must zoom in").toBeGreaterThan(Number.parseInt(before, 10));

  // The rendered page really is bigger — not a scaled-up bitmap, which
  // would leave the canvas' own width alone.
  await expect
    .poll(async () => (await page.locator("canvas").first().boundingBox())!.width)
    .toBeGreaterThan(canvasBefore * 1.5);

  await ctx.close();
});

test("a pinch abandons whatever the first finger had started", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  await openDocument(page);

  // Highlight keeps the gesture, so one finger down on a page begins
  // drawing a rectangle. A second finger means the first was the start
  // of a zoom, and the half-drawn highlight must not survive it.
  await page.getByRole("button", { name: "Highlight", exact: true }).click();

  const layer = page.locator(".interaction-layer").first();
  const lbox = (await layer.boundingBox())!;
  const x = Math.round(lbox.x + lbox.width / 2);
  const y = Math.round(lbox.y + lbox.height / 2);

  const at = (target: typeof layer, type: string, id: number, px: number, py: number) =>
    target.dispatchEvent(type, {
      pointerId: id,
      pointerType: "touch",
      isPrimary: id === 1,
      bubbles: true,
      clientX: px,
      clientY: py,
    });

  await at(layer, "pointerdown", 1, x, y);
  await at(layer, "pointermove", 1, x + 60, y + 10);
  await expect(page.locator(".drag-preview"), "a drag should be under way").toHaveCount(1);

  const container = page.locator(".scroll-container");
  await at(container, "pointerdown", 2, x - 60, y);
  await expect(page.locator(".drag-preview"), "the pinch must take the gesture").toHaveCount(0);

  await ctx.close();
});
