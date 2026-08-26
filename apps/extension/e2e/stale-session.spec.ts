import { expect, test } from "@playwright/test";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const ORIGIN = "http://localhost:8099";

/**
 * A session that has been sitting in storage since yesterday.
 *
 * Access tokens are short-lived; refresh tokens are not. After a reload
 * the app has a session that looks fine and whose access token the
 * server refuses. The SDK handles that — it refreshes on a 401 and
 * retries — but only for requests it makes itself, and the entitlement
 * check and the unlock were plain `fetch` calls with a bearer header.
 * They got the 401 and reported "not unlocked" and "couldn't unlock —
 * please try again", to someone who had already paid.
 *
 * Opening the account panel fixed it, because the panel asks for a
 * balance and that goes through the SDK. Which is a diagnosis, not a
 * workflow.
 *
 * These pin the outcome rather than the mechanism, and there are two
 * mechanisms behind it: the session is refreshed once when the app
 * starts, and the entitlement check refreshes and asks again if it is
 * refused. Either alone is enough for a given case, which is why
 * removing one of them leaves these passing — removing both does not.
 */
async function withStaleSession(
  page: import("@playwright/test").Page,
  options: { refreshWorks: boolean },
) {
  await page.addInitScript(
    ([base64, refreshWorks]: [string, boolean]) => {
      localStorage.setItem(
        "openapps.session",
        JSON.stringify({ accessToken: "stale-token", refreshToken: "good-refresh" }),
      );
      (window as unknown as Record<string, unknown>).__refreshWorks = refreshWorks;

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
    },
    [TEXT_PDF_BASE64, options.refreshWorks] as [string, boolean],
  );

  // The server, in miniature: the stale token is refused everywhere, the
  // refreshed one is accepted, and the account is a supporter.
  await page.route("**/v1/auth/refresh", (route) =>
    options.refreshWorks
      ? route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ access_token: "fresh-token", refresh_token: "next-refresh" }),
        })
      : route.fulfill({ status: 401, contentType: "application/json", body: "{}" }),
  );
  const authorized = (route: import("@playwright/test").Route, body: string) =>
    route.request().headers()["authorization"] === "Bearer fresh-token"
      ? route.fulfill({ status: 200, contentType: "application/json", body })
      : route.fulfill({ status: 401, contentType: "application/json", body: "{}" });
  await page.route("**/v1/credits/entitlement*", (route) => authorized(route, '{"unlocked":true}'));
  await page.route("**/v1/credits/balance*", (route) => authorized(route, '{"balance":4200}'));
}

async function openDocument(page: import("@playwright/test").Page) {
  await page.goto(ORIGIN);
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 30_000 });
}

test("a paid tool opens without a trip through the account panel", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await withStaleSession(page, { refreshWorks: true });
  await openDocument(page);

  await page.getByRole("button", { name: "OCR document" }).click();

  // No gate: the entitlement check was refused, the session refreshed,
  // and the second answer was "unlocked". What appears is the language
  // dialog — OCR itself.
  await expect(page.locator(".oa-dialog")).toContainText("Which language", { timeout: 20_000 });
  await expect(page.locator(".oa-dialog")).not.toContainText("1,000 credits");

  await ctx.close();
});

test("a session that cannot be refreshed says so, rather than offering to charge again", async ({
  browser,
}) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();
  await withStaleSession(page, { refreshWorks: false });
  await openDocument(page);

  await page.getByRole("button", { name: "OCR document" }).click();

  // Signed out is the honest answer here, and it is the one with a next
  // step. Asserted on the button rather than the prose, because the
  // locked gate also has the words "sign in" in it and would pass a
  // looser check while offering to charge for something already bought.
  const gate = page.locator(".oa-dialog");
  await expect(gate).toBeVisible({ timeout: 20_000 });
  await expect(gate.getByRole("button", { name: "Sign in" })).toBeVisible();
  await expect(gate.getByRole("button", { name: /Unlock for/ })).toHaveCount(0);

  await ctx.close();
});

test("the account button says whether there is a session", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();

  await page.goto(ORIGIN);
  const account = page.getByRole("button", { name: "Account", exact: true });
  await expect(account, "no session, no mark").toBeVisible();

  await page.evaluate(() =>
    localStorage.setItem(
      "openapps.session",
      JSON.stringify({ accessToken: "a", refreshToken: "b" }),
    ),
  );
  await page.reload();

  await expect(
    page.getByRole("button", { name: "Account — signed in" }),
    "signed in, and the interface says so without being opened",
  ).toBeVisible();

  await ctx.close();
});
