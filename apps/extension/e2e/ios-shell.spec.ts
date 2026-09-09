import { expect, test } from "@playwright/test";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

import { ORIGIN } from "./origin";

/**
 * The web app running inside the iOS shell.
 *
 * The shell itself is tested from Swift (apps/ios/OpenPdfEditTests). What
 * these cover is the half that lives here: that the account panel sells
 * through the App Store and not through a card checkout, and that it
 * redeems a receipt *before* telling StoreKit the purchase is finished.
 *
 * That order is the one thing in this flow that costs real money to get
 * wrong. StoreKit re-delivers an unfinished transaction on every launch,
 * which is what makes a purchase survive a crash between paying and being
 * credited; finishing first throws the safety net away and leaves someone
 * charged for credits nobody granted.
 *
 * `window.OpenPdfEditNative` is stubbed, because the real one exists only
 * inside a WKWebView. The stub records the order it is called in — which is
 * the assertion.
 */
async function inTheShell(
  page: import("@playwright/test").Page,
  options: {
    redeem?: "ok" | "fails" | "stale-token-then-ok";
    outstanding?: { transactionId: string; productId: string }[];
    openPanel?: boolean;
  } = {},
) {
  const { redeem = "ok", outstanding = [], openPanel = true } = options;

  await page.addInitScript((owed: { transactionId: string; productId: string }[]) => {
    localStorage.setItem(
      "openapps.session",
      JSON.stringify({ accessToken: "good-token", refreshToken: "good-refresh" }),
    );

    const calls: string[] = [];
    (window as unknown as Record<string, unknown>).__calls = calls;
    (window as unknown as Record<string, unknown>).OpenPdfEditNative = {
      platform: "ios",
      on: () => () => {},
      ready: async () => ({ platform: "ios" }),
      readDocument: async () => new File([], "x.pdf"),
      signIn: async () => ({ status: "cancelled" }),
      products: async () => {
        calls.push("products");
        return [
          {
            id: "credits_1000",
            name: "1,000 Credits",
            description: "Credits never expire.",
            price: "$4.99",
          },
          {
            id: "credits_5000",
            name: "5,000 Credits",
            description: "Credits never expire.",
            price: "$19.99",
          },
        ];
      },
      outstanding: async () => {
        calls.push("outstanding");
        return owed.map((o) => ({
          status: "purchased",
          transactionId: o.transactionId,
          productId: o.productId,
          receipt: "eyJ.header.signature",
          verifiedLocally: true,
        }));
      },
      purchase: async (productId: string) => {
        calls.push(`purchase:${productId}`);
        return {
          status: "purchased",
          transactionId: "2000000900000001",
          productId,
          receipt: "eyJ.header.signature",
          verifiedLocally: true,
        };
      },
      finish: async (transactionId: string) => {
        calls.push(`finish:${transactionId}`);
        return { finished: true };
      },
    };
  }, outstanding);

  // The account server, in miniature.
  await page.route("**/v1/auth/refresh", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ access_token: "good-token", refresh_token: "good-refresh" }),
    }),
  );
  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ user_id: "u1", identities: [] }),
    }),
  );
  await page.route("**/v1/credits/balance", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ balance: 0 }),
    }),
  );
  let redeemAttempts = 0;
  await page.route("**/v1/payments/apple/redeem", async (route) => {
    redeemAttempts += 1;
    const attempt = redeemAttempts;
    await page.evaluate(
      (n) => (window as unknown as { __calls: string[] }).__calls.push(`redeem:${n}`),
      attempt,
    );
    if (redeem === "fails") {
      return route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({ error: "the store could not be reached" }),
      });
    }
    if (redeem === "stale-token-then-ok" && attempt === 1) {
      return route.fulfill({
        status: 401,
        contentType: "application/json",
        body: JSON.stringify({ error: "unauthorized" }),
      });
    }
    return route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ topup_id: "iap_apple_1", credits: 1000, status: "credited" }),
    });
  });

  await page.goto(ORIGIN + "/");
  if (openPanel) await page.getByRole("button", { name: /^Account/ }).click();
}

async function calls(page: import("@playwright/test").Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __calls: string[] }).__calls);
}

test("inside the shell, credits are sold through the App Store and not by card", async ({
  page,
}) => {
  await inTheShell(page);

  await expect(page.getByRole("heading", { name: "Buy credits" })).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByRole("button", { name: "$4.99" })).toBeVisible();
  await expect(page.getByRole("button", { name: "$19.99" })).toBeVisible();

  // Guideline 3.1.1: a card checkout for digital content used in the app
  // is not a second option, it is grounds for rejection. <openapps-buy> is
  // the element that would offer one.
  expect(await page.locator("openapps-buy").count()).toBe(0);
});

test("the receipt is redeemed before the transaction is finished", async ({ page }) => {
  await inTheShell(page);
  await expect(page.getByRole("button", { name: "$4.99" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("button", { name: "$4.99" }).click();
  await expect
    .poll(async () => (await calls(page)).some((c) => c.startsWith("finish:")), {
      timeout: 15_000,
    })
    .toBe(true);

  const sequence = await calls(page);
  const purchased = sequence.indexOf("purchase:credits_1000");
  const redeemed = sequence.findIndex((c) => c.startsWith("redeem:"));
  const finished = sequence.indexOf("finish:2000000900000001");

  expect(purchased, "the purchase never reached the shell").toBeGreaterThanOrEqual(0);
  expect(redeemed, "the receipt was never sent to the server").toBeGreaterThan(purchased);
  expect(
    finished,
    "the transaction was finished before the server granted the credits",
  ).toBeGreaterThan(redeemed);
});

test("a receipt the server could not honour is not finished", async ({ page }) => {
  await inTheShell(page, { redeem: "fails" });
  await expect(page.getByRole("button", { name: "$4.99" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("button", { name: "$4.99" }).click();
  await expect(page.getByRole("alert")).toBeVisible({ timeout: 15_000 });

  // The receipt is the customer's only evidence they paid. Finishing it
  // after a failed redemption destroys that evidence and the money is gone.
  const sequence = await calls(page);
  expect(sequence.some((c) => c.startsWith("finish:"))).toBe(false);
});

test("a purchase left owing from an earlier run is collected at startup", async ({ page }) => {
  await inTheShell(page, {
    outstanding: [{ transactionId: "2000000900000009", productId: "credits_1000" }],
    // Nobody opens anything. Not a "Restore purchases" button, and not
    // even the account panel: someone short of credits they paid for
    // should not have to go looking, and the case this recovers from —
    // a redemption interrupted by a crash — is one they cannot describe.
    openPanel: false,
  });

  await expect
    .poll(async () => await calls(page), { timeout: 15_000 })
    .toContain("finish:2000000900000009");

  const sequence = await calls(page);
  expect(sequence.indexOf("redeem:1")).toBeLessThan(
    sequence.indexOf("finish:2000000900000009"),
  );
});

test("an access token that aged out does not lose a purchase", async ({ page }) => {
  // `redeemAppleReceipt` is a plain fetch, so the SDK's refresh-on-401
  // does not cover it. Left unhandled, a stale token tells someone who has
  // just paid to sign in again — the same trap the entitlement check fell
  // into before it learned to refresh and ask once more.
  await inTheShell(page, { redeem: "stale-token-then-ok" });
  await expect(page.getByRole("button", { name: "$4.99" })).toBeVisible({ timeout: 15_000 });

  await page.getByRole("button", { name: "$4.99" }).click();

  await expect
    .poll(async () => await calls(page), { timeout: 15_000 })
    .toContain("finish:2000000900000001");
  const sequence = await calls(page);
  expect(sequence).toContain("redeem:1");
  expect(sequence, "the refused redemption was never retried").toContain("redeem:2");
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("a PDF handed over by another app opens in the editor", async ({ page }) => {
  // The whole chain: iOS stages a file, the bridge announces it, the web
  // app fetches it as a File, registers it with the WebAssembly backend and
  // opens it — through real PDFium, not a stub. This is what makes the app
  // more than a bookmark, which is the test App Review applies under
  // guideline 4.2.
  await page.addInitScript((base64: string) => {
    const listeners: Record<string, ((detail: unknown) => void)[]> = {};
    (window as unknown as Record<string, unknown>).OpenPdfEditNative = {
      platform: "ios",
      on: (event: string, fn: (detail: unknown) => void) => {
        (listeners[event] ||= []).push(fn);
        return () => {};
      },
      ready: async () => {
        // Only once the page says it can take one — the same handover the
        // shell performs after a cold start.
        setTimeout(() => {
          for (const fn of listeners.document ?? []) {
            fn({ path: "/__incoming/1", name: "handed-over.pdf" });
          }
        }, 0);
        return { platform: "ios" };
      },
      readDocument: async () =>
        new File([Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))], "handed-over.pdf", {
          type: "application/pdf",
        }),
      signIn: async () => ({ status: "cancelled" }),
      products: async () => [],
      outstanding: async () => [],
      purchase: async () => ({ status: "cancelled" }),
      finish: async () => ({ finished: true }),
    };
  }, TEXT_PDF_BASE64);

  await page.goto(ORIGIN + "/");

  await expect(page.locator(".path-bar__text")).toHaveText("handed-over.pdf", {
    timeout: 20_000,
  });
  await expect(page.locator("canvas").first()).toBeVisible();
  // Rendered, not merely present: PDFium always paints an opaque page
  // background, so an opaque pixel is the difference between a canvas that
  // was sized and one that was actually drawn into. A blank one is what a
  // wasm binary that failed to load looks like.
  await page.waitForFunction(() => {
    const el = document.querySelector("canvas") as HTMLCanvasElement | null;
    if (!el || el.width === 0 || el.height === 0) return false;
    const context = el.getContext("2d");
    return context ? context.getImageData(0, 0, 1, 1).data[3] === 255 : false;
  });
});
