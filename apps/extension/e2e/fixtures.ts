// Mirrors opencapture/apps/extension/e2e/fixtures.ts — same
// launchPersistentContext + --load-extension mechanism, adapted for this
// extension's own dist/ layout and manifest. See that file if this one
// needs to grow (a static-server fixture for cross-origin pages, etc.).
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium, test as base, type BrowserContext, type Worker } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
const extensionDist = join(here, "..", "dist");

export const test = base.extend<{
  context: BrowserContext;
  extensionId: string;
  serviceWorker: Worker;
}>({
  // eslint-disable-next-line no-empty-pattern
  context: async ({}, use) => {
    if (!existsSync(join(extensionDist, "manifest.json"))) {
      throw new Error(`${extensionDist}/manifest.json not found — run "npm run build" before the e2e suite.`);
    }

    // channel: 'chromium' is required (not just "omit channel") — it's
    // the specific literal value that opts into the extension-capable
    // headless implementation. Real Chrome's plain `headless: true` uses
    // "old" headless, which has never supported extensions at all, and
    // Playwright's default bundled-Chromium launch (no channel) doesn't
    // get the same treatment either — only channel: 'chromium' does. See
    // opencapture's fixtures.ts, same finding.
    const context = await chromium.launchPersistentContext("", {
      channel: "chromium",
      headless: true,
      args: [`--disable-extensions-except=${extensionDist}`, `--load-extension=${extensionDist}`],
    });
    await use(context);
    await context.close();
  },

  serviceWorker: async ({ context }, use) => {
    let [worker] = context.serviceWorkers();
    worker ??= await context.waitForEvent("serviceworker", { timeout: 20_000 });
    await use(worker);
  },

  extensionId: async ({ serviceWorker }, use) => {
    const id = new URL(serviceWorker.url()).host;
    await use(id);
  },
});

export const expect = test.expect;
