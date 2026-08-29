// The recent-documents list on the landing screen.
//
// Two halves worth pinning. The visible one: the list shows what was
// opened, newest first, and can be pruned. The invisible one: an entry
// whose file is gone must not sit there forever pretending to work —
// what makes the list worth having is that every row does something.
import { expect, test } from "./fixtures";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const ORIGIN = "http://localhost:8099";
const KEY = "openpdfedit.recents";

/** Seeds the list the way a backend would, then reloads so the landing
 *  screen reads it. Ages are relative to now so the wording is stable. */
async function seed(page: import("@playwright/test").Page, entries: [string, number][]) {
  await page.goto(ORIGIN);
  await page.evaluate(
    ({ key, entries }) => {
      const now = Date.now();
      localStorage.setItem(
        key,
        JSON.stringify(entries.map(([name, ageMs]) => ({ id: name, name, openedAt: now - ageMs }))),
      );
    },
    { key: KEY, entries },
  );
  await page.reload();
}

test("nothing is shown until something has been opened", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await page.goto(ORIGIN);
  await expect(page.locator(".empty-state")).toBeVisible();
  // A "Recent" heading over blank space on first run promises a feature
  // nobody has asked about yet.
  await expect(page.locator(".recents")).toHaveCount(0);
  await ctx.close();
});

test("recents list newest first, with how long ago", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await seed(page, [
    ["yesterday.pdf", 26 * 3600_000],
    ["fresh.pdf", 30_000],
    ["earlier.pdf", 25 * 60_000],
  ]);

  const names = page.locator(".recent__name");
  await expect(names).toHaveText(["fresh.pdf", "earlier.pdf", "yesterday.pdf"]);
  await expect(page.locator(".recent__when")).toHaveText([
    "just now",
    "25 minutes ago",
    "yesterday",
  ]);
  await ctx.close();
});

test("only the most recent handful are kept", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  // Ten opened, oldest first. A landing screen listing all of them is a
  // file manager, and a worse one than the picker it saves you from.
  await seed(
    page,
    Array.from({ length: 10 }, (_, i) => [`doc-${i}.pdf`, (10 - i) * 60_000] as [string, number]),
  );

  const rows = page.locator(".recent");
  await expect(rows).toHaveCount(6);
  // Newest of the ten is doc-9; the six kept run back from there.
  await expect(page.locator(".recent__name").first()).toHaveText("doc-9.pdf");
  await ctx.close();
});

test("one can be removed, and all of them", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await seed(page, [
    ["keep.pdf", 60_000],
    ["remove-me.pdf", 120_000],
  ]);

  await expect(page.locator(".recent")).toHaveCount(2);
  await page
    .locator(".recent", { hasText: "remove-me.pdf" })
    .locator(".recent__forget")
    .click();
  await expect(page.locator(".recent__name")).toHaveText(["keep.pdf"]);

  await page.locator(".recents__clear").click();
  // The whole block goes, not an empty list with a heading over it.
  await expect(page.locator(".recents")).toHaveCount(0);

  // And it stays gone across a reload, rather than being a screen-only
  // effect over storage that still has the entries.
  await page.reload();
  await expect(page.locator(".recents")).toHaveCount(0);
  await ctx.close();
});

test("a row whose file is gone says so and stops being offered", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  // Seeded into localStorage with no matching handle in IndexedDB —
  // exactly the state left behind by a file that has been deleted, or
  // by storage that was partly cleared.
  await seed(page, [["vanished.pdf", 60_000]]);

  await page.locator(".recent__open").click();

  await expect(page.getByText(/couldn't reopen that one/i)).toBeVisible();
  // Dropped rather than left to be clicked again to the same effect.
  await expect(page.locator(".recents")).toHaveCount(0);
  await ctx.close();
});

/** Opens a document, so the start screen (and its copy of the list) is
 *  gone and the topbar's History button is the only way to the recents. */
async function openADocument(page: import("@playwright/test").Page) {
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
  // `addInitScript` only applies to navigations after it is added, and
  // the seeding above has already loaded the page.
  await page.reload();
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 30_000 });
}

test("History reaches the recents once the start screen is gone", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await seed(page, [
    ["board-pack.pdf", 40_000],
    ["lease.pdf", 5 * 3600_000],
  ]);
  await openADocument(page);

  // The start screen and its list are gone; the button is the way back.
  await expect(page.locator(".empty-state")).toHaveCount(0);
  await expect(page.locator(".history__menu")).toHaveCount(0);

  await page.getByRole("button", { name: "Recent documents" }).click();
  const menu = page.locator(".history__menu");
  await expect(menu).toBeVisible();
  await expect(menu.locator(".recent__name")).toHaveText(["board-pack.pdf", "lease.pdf"]);

  await ctx.close();
});

test("the History menu closes the ways a menu should", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await seed(page, [["board-pack.pdf", 40_000]]);
  await openADocument(page);

  const button = page.getByRole("button", { name: "Recent documents" });
  const menu = page.locator(".history__menu");

  await button.click();
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);

  await button.click();
  await expect(menu).toBeVisible();
  // A press anywhere else. Raw coordinates rather than a locator
  // click: the point is that a press outside dismisses, and
  // Playwright's actionability checks on whatever happens to be under
  // the cursor are not what is being tested.
  const size = page.viewportSize()!;
  await page.mouse.click(size.width - 60, size.height - 60);
  await expect(menu).toHaveCount(0);

  await ctx.close();
});

test("no History button when there is no history", async ({ browser }) => {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await page.goto(ORIGIN);
  await openADocument(page);
  // Absent, not disabled: a control that is permanently dead teaches
  // people to stop looking at that corner of the screen.
  await expect(page.getByRole("button", { name: "Recent documents" })).toHaveCount(0);
  await ctx.close();
});
