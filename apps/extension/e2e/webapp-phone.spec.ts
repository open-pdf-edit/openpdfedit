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
        topbarH: Math.round(document.querySelector("header.topbar")!.getBoundingClientRect().height),
        // Every control in the two top strips that is actually on screen,
        // measured on its shorter side.
        tapTargets: [...document.querySelectorAll("header.topbar button, .path-bar button")]
          .map((b) => b.getBoundingClientRect())
          .filter((r) => r.width > 0 && r.height > 0)
          .map((r) => Math.round(Math.min(r.width, r.height))),
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

    // Zoom and the page number don't fit a 390px topbar, so they live in
    // the file strip beside the name of the document they describe.
    // Neither was shown at all before, which left no way to read the
    // zoom a pinch had just changed.
    await expect(page.locator(".path-bar__meta")).toBeVisible();
    await expect(page.locator(".path-bar__pages")).toHaveText(/^\d+ \/ \d+$/);
    await expect(page.locator(".path-bar__meta .zoom-level")).toHaveText(/%$/);

    // One row, and targets a thumb can hit. The topbar used to pack its
    // controls 2px apart at 28px each.
    expect(m.topbarH, "the topbar must not wrap onto a second row").toBeLessThanOrEqual(60);
    expect(Math.min(...m.tapTargets), "every visible control in the chrome").toBeGreaterThanOrEqual(
      34,
    );

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
      zoomInTopbar: getComputedStyle(document.querySelector(".topbar__group--zoom")!).display,
      stripMeta: getComputedStyle(document.querySelector(".path-bar__meta")!).display,
    };
  });

  expect(m.railDirection, "the rail stays down the side").toBe("column");
  // Wider than the 56px it was: the tools carry their names now, which
  // is the whole point of the change and needs the room.
  expect(m.railWidth).toBe(150);
  // 100%, not fitted: on a desktop a page fits anyway, and a predictable
  // scale is worth more than filling the window.
  expect(m.canvasW, "a Letter page at 100% is 816px").toBe(816);
  expect(m.moreShown, "a desktop must not get a button for a sheet it never renders").toBe(false);
  // Zoom and the page number are rendered twice — topbar and file strip
  // — with CSS choosing which. A desktop takes the topbar copy, and must
  // not show both.
  expect(m.zoomInTopbar, "the desktop keeps zoom in the topbar").not.toBe("none");
  expect(m.stripMeta, "the phone's copy must stay hidden here").toBe("none");

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

  // The file strip's copy — the topbar's is hidden on a phone, and both
  // exist in the DOM.
  const zoomLabel = page.locator(".path-bar__meta .zoom-level");
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

/**
 * Opening and saving on a phone.
 *
 * The File System Access API — `showOpenFilePicker`, and the writable
 * handle that lets Save write back over the original — is desktop
 * Chromium only. Neither iOS Safari nor Android Chrome has it, so a
 * phone always takes the fallback path: an `<input type="file">` to get
 * bytes in, and a download to get them out. That path exists for the
 * extension; this is the phone using it, and the check that the Save
 * control admits which of the two it is doing rather than promising a
 * write it cannot perform.
 */
test("a phone without the file system API can still open and save", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["Pixel 5"], acceptDownloads: true });
  const page = await ctx.newPage();

  await page.addInitScript(() => {
    delete (window as unknown as Record<string, unknown>).showOpenFilePicker;
    delete (window as unknown as Record<string, unknown>).showSaveFilePicker;
  });
  await page.goto(ORIGIN);

  const chooserPromise = page.waitForEvent("filechooser");
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  const chooser = await chooserPromise;
  await chooser.setFiles({
    name: "doc.pdf",
    mimeType: "application/pdf",
    buffer: Buffer.from(TEXT_PDF_BASE64, "base64"),
  });

  await expect(page.locator(".path-bar__text")).toHaveText("doc.pdf", { timeout: 30_000 });
  await expect(page.locator("canvas")).toBeVisible({ timeout: 30_000 });

  // Saying "Save" here would promise writing back over a file this
  // browser cannot reopen for writing.
  const save = page.getByRole("button", { name: "Download a copy" });
  await expect(save).toBeVisible();

  // Nothing to save until something has been changed. A rectangle, not
  // a highlight: a highlight needs words under it, and where they are
  // depends on the fixture.
  await page.getByRole("button", { name: "Rectangle", exact: true }).click();
  const layer = page.locator(".interaction-layer").first();
  const box = (await layer.boundingBox())!;
  for (const [type, dx, dy] of [
    ["pointerdown", 40, 40],
    ["pointermove", 160, 120],
    ["pointerup", 160, 120],
  ] as const) {
    await layer.dispatchEvent(type, {
      pointerId: 1,
      pointerType: "touch",
      isPrimary: true,
      bubbles: true,
      clientX: Math.round(box.x + dx),
      clientY: Math.round(box.y + dy),
    });
  }
  await expect(save).toBeEnabled({ timeout: 30_000 });

  const downloadPromise = page.waitForEvent("download");
  await save.click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toContain(".pdf");

  await ctx.close();
});

/**
 * The tools say what they are.
 *
 * Sixteen glyphs in a rail and seventeen more in the document tools,
 * distinguishable only by hovering each one in turn, is a memory test —
 * and on a phone there is no hover at all, so an unlabelled icon there
 * is a guess. Both sets are named and grouped now, and the groups are a
 * claim about what the tools do, so a wrong one is worth catching.
 */
test("every tool is named, and the names are grouped", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await openDocument(page);

  // The rail: a name beside every icon, under a heading.
  const railLabels = page.locator(".rail__label");
  expect(await railLabels.count(), "one label per tool").toBe(16);
  for (const name of ["Select", "Erase", "Highlight", "Redact", "Signature"]) {
    await expect(page.locator(".rail__label", { hasText: new RegExp(`^${name}$`) })).toBeVisible();
  }
  await expect(page.locator(".rail__heading")).toHaveText([
    "Select",
    "Mark up",
    "Draw",
    "Edit content",
    "Fill & sign",
  ]);

  // The document tools are named too, in their own band under the
  // topbar: seventeen icons crowded into the topbar's right-hand end had
  // nowhere to put the names, and the difference between Flatten and
  // Compress is not something an icon can carry.
  await expect(page.locator(".tools__heading")).toHaveText([
    "Panels",
    "Document",
    "Markup",
    "Save & protect",
  ]);
  for (const name of ["Comments", "OCR", "Watermark", "Flatten", "Compress", "Remove markup", "Text"]) {
    await expect(page.locator(".tools__label", { hasText: new RegExp(`^${name}$`) })).toBeVisible();
  }

  // And the band must not push the window sideways, whatever it holds.
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
  ).toBe(true);

  await ctx.close();
});

test("a phone names the document tools too, since it has no hover", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  await openDocument(page);

  await page.locator(".topbar__more").click();
  const sheet = page.locator(".topbar__group--overflow.is-open");
  await expect(sheet).toBeVisible();

  await expect(sheet.locator(".tools__heading").first()).toBeVisible();
  await expect(sheet.locator(".tools__label", { hasText: /^OCR$/ })).toBeVisible();
  await expect(sheet.locator(".tools__label", { hasText: /^Watermark$/ })).toBeVisible();
  await expect(sheet.locator(".tools__label", { hasText: /^Remove markup$/ })).toBeVisible();

  // The sheet is taller than the room it has, so it must scroll rather
  // than wrap into a second column running off the right edge — which is
  // what it did the first time the headings went in.
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
    "the sheet must not push the page sideways",
  ).toBe(true);

  await ctx.close();
});

/**
 * What a phone actually shows.
 *
 * The existing tests here checked that things exist and that the page
 * does not overflow, and everything below was wrong while they passed:
 * the rail's group headings were `display: none`, every topbar button
 * carried a name only in `aria-label` — which a phone never renders,
 * having no hover to trigger a tooltip — and the third colour swatch
 * wrapped past the bottom edge of a 62px bar and could not be tapped.
 *
 * Reachability and legibility, then, not presence.
 */
test("iPhone 13: the phone names its controls and can reach all of them", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  await openDocument(page);

  // Sixteen tools in a strip a third their width: the group names are
  // what makes scrolling past Draw to reach Edit content deliberate
  // rather than exploratory.
  await expect(page.locator(".rail__heading")).toHaveText([
    "Select",
    "Mark up",
    "Draw",
    "Edit content",
    "Fill & sign",
  ]);
  for (const heading of await page.locator(".rail__heading").all()) {
    await expect(heading).toBeVisible();
  }

  // Every topbar button says what it is, in text on the screen. A
  // tooltip is not a label where there is no pointer to hover with.
  const unnamed = await page.evaluate(() =>
    [...document.querySelectorAll("header.topbar button")]
      .filter((b) => (b as HTMLElement).offsetParent !== null)
      .filter((b) => !(b as HTMLElement).innerText.trim())
      .map((b) => b.getAttribute("aria-label") ?? "(no aria-label either)"),
  );
  // Zoom in and out are the exception: a magnifier with a plus in it is
  // universal, and the zoom percentage sits between them saying what
  // they act on.
  expect(unnamed.filter((n) => !/^Zoom (in|out)$/.test(n))).toEqual([]);

  // Every swatch has to be inside the bar. `flex-wrap` inherited from
  // the desktop's column rail stacked them, and the third fell off the
  // bottom of the screen — present in the DOM, unreachable by a thumb.
  const swatches = await page.evaluate(() => {
    const rail = document.querySelector(".rail")!.getBoundingClientRect();
    return [...document.querySelectorAll(".swatch")].map((s) => {
      const r = s.getBoundingClientRect();
      return { inside: r.top >= rail.top - 1 && r.bottom <= rail.bottom + 1, size: r.width };
    });
  });
  expect(swatches.length).toBeGreaterThan(0);
  expect(swatches.filter((s) => !s.inside), "swatches outside the tool bar").toEqual([]);
  for (const s of swatches) {
    expect(s.size, "a swatch smaller than a fingertip").toBeGreaterThanOrEqual(24);
  }

  await ctx.close();
});
