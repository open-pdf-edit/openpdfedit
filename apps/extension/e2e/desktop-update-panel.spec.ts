// APP-29, reopened by jerry: "点击更新后弹窗不能关闭" — after clicking
// update, the popup could not be closed.
//
// The update popup's only exit was clicking its own icon a second time.
// Every other menu and panel in the app closes on Escape, and the topbar
// menus on an outside click too, so a user reasonably tries those — and on
// a build whose update check fails, they were left with an error and a
// "Try again" button stuck over their document.
//
// Each exit is tested on its own, from a freshly opened popup, and so are
// the two things that must NOT close it. A single run that pressed Escape
// and then clicked outside would pass the second half for free, because
// the popup was already closed.
import { expect, test, type Page } from "@playwright/test";

import { DESKTOP_ORIGIN } from "./origin";

test.describe.configure({ timeout: 120_000 });

async function openPopup(page: Page) {
  const button = page.getByRole("button", { name: "Check for updates" }).first();
  // The first load compiles on demand under Vite, so it can be slow.
  await button.waitFor({ timeout: 90_000 });
  await button.click();
  const menu = page.locator(".update__menu");
  await expect(menu).toBeVisible();
  // Wait for the check to settle — here it fails, as it does on a build
  // whose release carries no update manifest.
  await expect(menu.getByRole("button", { name: "Try again" })).toBeVisible({ timeout: 10_000 });
  return { button, menu };
}

test.beforeEach(async ({ page }) => {
  await page.goto(DESKTOP_ORIGIN);
});

test("Escape closes the update popup", async ({ page }) => {
  const { menu } = await openPopup(page);
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
});

test("a click outside closes the update popup", async ({ page }) => {
  const { menu } = await openPopup(page);
  await page.mouse.click(400, 500);
  await expect(menu).toBeHidden();
});

test("clicking the icon again still closes it", async ({ page }) => {
  const { button, menu } = await openPopup(page);
  await button.click();
  await expect(menu).toBeHidden();
});

test("a click inside the popup does not close it", async ({ page }) => {
  // Try again is inside the popup: it must re-run the check, not dismiss
  // what it is about to report.
  const { menu } = await openPopup(page);
  await menu.getByRole("button", { name: "Try again" }).click();
  await expect(menu).toBeVisible();
});
