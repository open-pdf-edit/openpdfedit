import { expect, test } from "@playwright/test";

test("sign-in opens a tab, not a sized popup", async ({ page, context }) => {
  // A sized popup is what broke wallet and Nostr signing: an extension
  // cannot reliably prompt from one, because its own approval window
  // takes focus and the provider treats the request as abandoned.
  // Asserted on the window features, since the difference is invisible
  // in a screenshot and easy to reintroduce by tidying the call.
  await page.goto("http://localhost:8099/");
  await page.evaluate(() => {
    (window as unknown as Record<string, unknown>).__opened = null;
    (window as unknown as Record<string, unknown>).__called = false;
    window.open = ((url?: string | URL, target?: string, features?: string) => {
      (window as unknown as Record<string, unknown>).__called = true;
      (window as unknown as Record<string, unknown>).__opened = features ?? null;
      return null; // never actually navigate
    }) as typeof window.open;
  });

  await page.getByRole("button", { name: "Account" }).click();
  const signIn = page.getByRole("button", { name: "Sign in", exact: true });
  await expect(signIn, "signed-out account panel should offer sign-in").toBeVisible({
    timeout: 15_000,
  });
  await signIn.click();

  const { called, features } = await page.evaluate(() => ({
    called: (window as unknown as Record<string, unknown>).__called as boolean,
    features: (window as unknown as Record<string, unknown>).__opened as string | null,
  }));

  // Without this the assertion below passes whenever the click misses,
  // which is exactly how the first version of this test managed to pass
  // against the bug it was written for.
  expect(called, "sign-in never called window.open — the test missed the button").toBe(true);
  expect(features, "sign-in must pass no window features; features make a popup").toBeNull();

  await context.close();
});
