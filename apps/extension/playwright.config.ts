import { defineConfig } from "@playwright/test";

import { WEBAPP_PORT } from "./e2e/origin";

// Two kinds of spec live here. Most drive the extension's own
// `chrome-extension://` pages and need nothing served. Nine drive the
// *web app*, and they used to point at a hand-started server on port
// 8099 — which nothing in this repository started, and which another app
// in this suite also binds. With nothing listening those nine failed on
// 30s timeouts; with the wrong thing listening they drove a different
// product and blamed this one.
//
// So the suite starts its own, on a port of its own. `reuseExistingServer`
// is false on purpose: reusing whatever happens to answer is exactly how
// the wrong app got tested.
export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  webServer: {
    command: `python3 ../../scripts/serve-webapp.py ${WEBAPP_PORT}`,
    url: `http://127.0.0.1:${WEBAPP_PORT}/`,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
