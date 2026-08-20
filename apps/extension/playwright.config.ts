import { defineConfig } from "@playwright/test";

// No `webServer` entry — unlike opencapture's suite, this boot smoke
// test only ever opens the extension's own `chrome-extension://` pages,
// never a cross-origin fixture page, so there's nothing to serve.
export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
});
