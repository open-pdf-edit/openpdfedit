import { expect, test, devices } from "@playwright/test";

const ORIGIN = "http://localhost:8099";

/**
 * Installability: the manifest, the icons, and the offer.
 *
 * The manifest used to declare `favicon.png` as 512x512 when the file is
 * 256 — a claim no browser can check without fetching it, and one that
 * stops Chrome treating the app as installable at all. So these assert
 * the *files*, not just the declarations.
 */
test("the manifest declares icons that exist at the sizes it claims", async ({
  request,
}) => {
  const manifest = await (
    await request.get(`${ORIGIN}/manifest.webmanifest`)
  ).json();

  expect(manifest.display).toBe("standalone");
  expect(manifest.start_url).toBeTruthy();

  // Chromium requires at least a 192 and a 512 to offer installation.
  const anySizes = manifest.icons
    .filter((i: { purpose?: string }) => i.purpose === "any")
    .map((i: { sizes: string }) => i.sizes);
  expect(anySizes).toContain("192x192");
  expect(anySizes).toContain("512x512");

  // Android masks icons to a circle. Without a maskable one it crops the
  // artwork instead of the background, which on an icon with transparent
  // corners means cutting into the mark itself.
  const maskable = manifest.icons.filter(
    (i: { purpose?: string }) => i.purpose === "maskable",
  );
  expect(
    maskable.length,
    "no maskable icon — Android will crop the artwork",
  ).toBeGreaterThan(0);

  // Every declared icon must exist and actually be the size claimed.
  for (const icon of manifest.icons) {
    const url = `${ORIGIN}/${String(icon.src).replace(/^\.\//, "")}`;
    const response = await request.get(url);
    expect(response.status(), `${icon.src} is declared but missing`).toBe(200);

    const bytes = Buffer.from(await response.body());
    // PNG: width and height are big-endian u32 at offsets 16 and 20.
    expect(bytes.subarray(1, 4).toString(), `${icon.src} is not a PNG`).toBe(
      "PNG",
    );
    const width = bytes.readUInt32BE(16);
    const height = bytes.readUInt32BE(20);
    expect(`${width}x${height}`, `${icon.src} claims ${icon.sizes}`).toBe(
      icon.sizes,
    );
  }
});

test("iOS gets the tags it reads, since it reads none of the manifest's icons", async ({
  request,
}) => {
  const html = await (await request.get(`${ORIGIN}/`)).text();
  expect(html).toContain('rel="apple-touch-icon"');
  expect(html).toContain('name="apple-mobile-web-app-capable"');
  expect(html).toContain('name="theme-color"');

  // And the standard spelling beside it. Chrome warns that the Apple one
  // is deprecated and reads `mobile-web-app-capable` instead, while iOS
  // reads only the Apple one — so neither can be dropped for the other.
  expect(html, "Android's installability tag is missing").toContain(
    'name="mobile-web-app-capable"',
  );

  const icon = await request.get(`${ORIGIN}/icons/apple-touch-icon.png`);
  expect(icon.status()).toBe(200);
  const bytes = Buffer.from(await icon.body());
  expect(bytes.readUInt32BE(16)).toBe(180);
});

test("the install offer appears when the browser says it can, and not before", async ({
  browser,
}) => {
  const ctx = await browser.newContext({ ...devices["Pixel 5"] });
  const page = await ctx.newPage();
  await page.goto(ORIGIN);

  // Headless Chromium does not fire beforeinstallprompt, so the offer is
  // driven the way a real browser would drive it. What is under test is
  // this app's handling — that it defers the event rather than letting
  // the browser's own infobar take it, and only offers once it has one.
  await expect(
    page.getByRole("button", { name: "Install as an app" }),
  ).toHaveCount(0);

  const defaultPrevented = await page.evaluate(() => {
    const event = new Event("beforeinstallprompt", {
      cancelable: true,
    }) as Event & {
      prompt?: () => Promise<void>;
      userChoice?: Promise<unknown>;
    };
    event.prompt = async () => {};
    event.userChoice = Promise.resolve({ outcome: "accepted" });
    window.dispatchEvent(event);
    return event.defaultPrevented;
  });

  // Not preventing it leaves Chromium free to show its own bar as well.
  expect(defaultPrevented, "the event must be taken over, not observed").toBe(
    true,
  );

  const install = page.getByRole("button", { name: "Install as an app" });
  await expect(install).toBeVisible();

  // Chromium never replays the same event, so the offer must go once
  // used — otherwise it leaves a button that silently does nothing.
  await install.click();
  await expect(install).toHaveCount(0);

  await ctx.close();
});

test("an already-installed window is not asked to install again", async ({
  browser,
}) => {
  const ctx = await browser.newContext({ ...devices["Pixel 5"] });
  const page = await ctx.newPage();
  // What the browser reports once the app is launched from a home screen.
  await page.emulateMedia({ media: "screen" });
  await page.addInitScript(() => {
    const real = window.matchMedia.bind(window);
    window.matchMedia = ((q: string) =>
      q.includes("display-mode: standalone")
        ? ({
            matches: true,
            media: q,
            addEventListener() {},
            removeEventListener() {},
          } as never)
        : real(q)) as typeof window.matchMedia;
  });
  await page.goto(ORIGIN);

  await page.evaluate(() => {
    const event = new Event("beforeinstallprompt", {
      cancelable: true,
    }) as Event & {
      prompt?: () => Promise<void>;
    };
    event.prompt = async () => {};
    window.dispatchEvent(event);
  });

  await expect(
    page.getByRole("button", { name: "Install as an app" }),
  ).toHaveCount(0);
  await ctx.close();
});

test("an event fired before the app hydrates is not lost", async ({
  browser,
}) => {
  // Chromium fires beforeinstallprompt once it decides a page qualifies —
  // which needs only the manifest and the service worker, not the app —
  // and never replays it. A listener that exists only after hydration can
  // therefore miss it entirely and the offer never appears. That is what
  // happened: the test above passed while a screenshot of the same page
  // showed no install bar, because the test waited before dispatching and
  // the screenshot did not.
  const ctx = await browser.newContext({ ...devices["Pixel 5"] });
  const page = await ctx.newPage();

  // Hold the app's own bundle back. Over localhost the whole thing loads
  // in a few milliseconds, so without this the window being tested has
  // already closed by the time a dispatch can be issued — the test would
  // pass on the component's own listener and prove nothing. A phone on a
  // real network gives that window for free.
  let releaseBundle: () => void = () => {};
  const bundleHeld = new Promise<void>((resolve) => (releaseBundle = resolve));
  await page.route(/assets\/.*\.js$/, async (route) => {
    await bundleHeld;
    await route.continue();
  });

  await page.goto(ORIGIN, { waitUntil: "commit" });

  // The head script initialises the stash to null, so the property
  // existing means the capture is armed.
  await page.waitForFunction(() => "__installPromptEvent" in window);
  expect(
    await page.evaluate(
      () => document.body.textContent?.includes("Open a PDF") ?? false,
    ),
    "the app hydrated first — this test would not exercise the race",
  ).toBe(false);

  const captured = await page.evaluate(() => {
    const event = new Event("beforeinstallprompt", {
      cancelable: true,
    }) as Event & {
      prompt?: () => Promise<void>;
      userChoice?: Promise<unknown>;
    };
    event.prompt = async () => {};
    event.userChoice = Promise.resolve({ outcome: "accepted" });
    window.dispatchEvent(event);
    return {
      prevented: event.defaultPrevented,
      stashed:
        (window as { __installPromptEvent?: unknown }).__installPromptEvent ===
        event,
    };
  });
  // Chromium's own infobar must be headed off here too, not only later.
  expect(captured.prevented, "the head script must take the event over").toBe(
    true,
  );
  expect(
    captured.stashed,
    "the head script must keep the event for the app",
  ).toBe(true);

  releaseBundle();
  await expect(
    page.getByRole("button", { name: "Install as an app" }),
  ).toBeVisible({
    timeout: 15_000,
  });
  await ctx.close();
});
