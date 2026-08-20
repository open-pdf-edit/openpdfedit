// Boot smoke test: loads the built dist/ as a real unpacked extension in
// a real (headless) Chromium and opens its actual boot page — exactly
// what a user's freshly-installed extension does — then asserts the
// editor UI actually mounted. This is the one check nothing else in this
// repo's build/typecheck pipeline can catch: `npm run check`/`vite build`
// both stay green whether or not `chrome-extension://<id>/index.html`
// resolves to the editor or to SvelteKit's own "404 — Not found" page,
// because the mismatch is a client-side routing decision made at
// runtime, once the SvelteKit adapter-static SPA fallback boots at a
// non-root pathname — see apps/desktop/src/hooks.ts's `reroute` hook,
// which this test is the actual regression guard for.
import { expect, test } from "./fixtures";

test("the packaged extension's boot page renders the editor, not SvelteKit's 404", async ({ context, extensionId }) => {
  const page = await context.newPage();

  // Capture console errors from the moment navigation starts, not after —
  // a boot-time error logged before this listener attaches would
  // otherwise go unnoticed.
  const consoleErrors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });

  // `console` alone is blind to an unhandled exception or unhandled
  // promise rejection thrown in page script — nothing calls
  // `console.error` for those, the browser just reports them as an
  // uncaught error. That's exactly the shape of the AccountPanel.svelte
  // boot bug this test is the regression guard for: its `$effect` used to
  // call `void listen(...).then(...)` with no `.catch`, throwing an
  // unhandled "Cannot read properties of undefined (reading
  // 'transformCallback')" TypeError on every load in the extension (no
  // `__TAURI_INTERNALS__` there) — and this spec stayed green through
  // that the whole time, because `consoleErrors` never saw it.
  // `page.on("pageerror", ...)` is what actually observes it.
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => {
    pageErrors.push(err.message);
  });

  await page.goto(`chrome-extension://${extensionId}/index.html`);

  // The empty-state "Open PDF…" button is what the editor's root route
  // renders with no document open yet — present as soon as the SPA has
  // actually mounted at `/`, not stuck on SvelteKit's own not-found page
  // (which the pre-fix bug produced: `url.pathname === "/index.html"`
  // matches neither `/` nor `/login` in the route table).
  await expect(page.locator(".empty-state button", { hasText: "Open PDF…" })).toBeVisible();

  // Belt-and-suspenders: the specific failure mode this guards against
  // renders literally the text "404" (SvelteKit's default not-found
  // page body), so a direct negative assertion catches it even if some
  // future markup change ever drops the `.empty-state` class/copy above.
  await expect(page.locator("body")).not.toContainText("404");

  // The app shell (topbar) renders unconditionally — unlike the
  // annotation TOOLS toolbar in +page.svelte, which lives inside
  // `{#if doc}` and therefore cannot render until a document is open.
  // Headlessly opening one would require driving the real File System
  // Access picker (or a CDP-injected mock of it), which is genuinely
  // hard and out of scope for this phase — deferred to future work, see
  // docs/superpowers/plans/2026-08-16-extension-port-phase2.md Task 5.
  // In the meantime, asserting the shell mounted plus a clean console
  // (no runtime errors during the mount/wasm-init sequence) is the
  // strongest cheap signal available without that fixture.
  await expect(page.locator("header.topbar")).toBeVisible();

  expect(consoleErrors, `expected no console errors during boot, got: ${consoleErrors.join("; ")}`).toEqual([]);
  expect(pageErrors, `expected no uncaught page errors during boot, got: ${pageErrors.join("; ")}`).toEqual([]);

  await page.close();
});
