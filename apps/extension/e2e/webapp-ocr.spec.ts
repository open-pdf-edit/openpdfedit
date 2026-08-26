import { expect, test } from "@playwright/test";
import { TEXT_PDF_BASE64 } from "./pdf-fixtures";

const ORIGIN = "http://localhost:8099";

/**
 * The OCR assets, as the built web app actually serves them.
 *
 * The risk in this feature is not recognition — Tesseract has been
 * reading text for twenty years. It is that its engine, its worker and
 * ~4 MB of trained data are served from *our* origin instead of the CDN
 * tesseract.js reaches for by default. Get one path wrong and the
 * failure mode is a silent request to someone else's server at the exact
 * moment a user OCRs a private document, which is the one thing this
 * product promises cannot happen.
 */
test("every OCR asset is served from our own origin", async ({ request }) => {
  // Every variant tesseract.js may ask for, not the ones this machine
  // happens to pick. It chooses at runtime from what the browser
  // supports — plain, SIMD or relaxed SIMD, each with an LSTM-only twin
  // — so a subset passes wherever it was tested and fails elsewhere with
  // "failed to load". That is not hypothetical: the first version of
  // this shipped four of them and current Chrome asked for a fifth.
  const cores = ["", "-simd", "-relaxedsimd"].flatMap((simd) =>
    ["", "-lstm"].map((lstm) => `tesseract-core${simd}${lstm}`),
  );
  for (const [path, minBytes] of [
    ["/ocr/worker.min.js", 10_000],
    ["/ocr/eng.traineddata", 3_000_000],
    ...cores.flatMap((core) => [
      [`/ocr/${core}.wasm`, 1_000_000] as const,
      [`/ocr/${core}.wasm.js`, 1_000_000] as const,
    ]),
  ] as const) {
    const response = await request.get(`${ORIGIN}${path}`);
    expect(response.status(), `${path} must be served`).toBe(200);
    expect((await response.body()).byteLength, `${path} looks truncated`).toBeGreaterThan(minBytes);
  }
});

test("the app pins OCR to its own origin", async () => {
  // Every emitted chunk, not only the ones the page loads: the OCR
  // module is lazily imported, so a plain page open never fetches it —
  // which is the desired behaviour, and would have made a load-time
  // check pass without inspecting the code that matters.
  const { readdirSync, readFileSync } = await import("node:fs");
  const { join } = await import("node:path");
  const root = join(process.cwd(), "..", "webapp", "dist", "app");

  const jsFiles: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith(".js")) jsFiles.push(full);
    }
  };
  walk(root);

  const bodies = jsFiles.map((f) => readFileSync(f, "utf8"));

  // The override that keeps the engine local has to actually be in the
  // shipped code, not just in the source.
  expect(
    bodies.some((b) => b.includes("/ocr")),
    "no chunk pins the OCR assets to this origin",
  ).toBe(true);

  // The trained data is the big fetch and the one tesseract.js would
  // otherwise pull per language. Nothing may reference a remote copy.
  for (const remote of ["tessdata.projectnaptha.com", "tessdata_fast", "tessdata_best"]) {
    expect(
      bodies.some((b) => b.includes(remote)),
      `a chunk references ${remote}, so language data could come from off-origin`,
    ).toBe(false);
  }

  // Known and deliberate: tesseract.js carries a jsdelivr URL as its
  // *default* workerPath, which our options replace at call time. The
  // string therefore survives into the bundle and cannot be asserted
  // away without patching the dependency. What can be checked is that it
  // is only ever the default — one occurrence, in its config module,
  // not something we point at ourselves.
  const jsdelivrHits = bodies.filter((b) => b.includes("cdn.jsdelivr.net")).length;
  expect(jsdelivrHits, "jsdelivr appears in more chunks than tesseract.js's own default").toBe(1);
});

/**
 * Every mutating backend method must re-key its open-document map.
 *
 * Committing a mutation rotates the handle — the session reopens the
 * document and returns a fresh one — so a method that updates the
 * document without moving the map entry leaves it under the old key.
 * The call that hit it then arrives with the new handle and is told the
 * document does not exist. That is "ocrDocument: unknown document handle
 * 5", reported after OCR had already worked once.
 *
 * This reads the source rather than exercising the behaviour, which is
 * usually the weaker kind of test. It earns its place here because the
 * invariant genuinely is structural — `migrateOpenDoc` exists precisely
 * so no method has to remember the two-step — and because the behavioural
 * version costs five minutes of real recognition to reach a bug that
 * only shows on the second run. This catches it in milliseconds.
 */
test("every mutating backend method migrates the open-document map", async () => {
  const { readFileSync } = await import("node:fs");
  const { join } = await import("node:path");
  const source = readFileSync(
    join(process.cwd(), "..", "desktop", "src", "lib", "backend", "wasm.ts"),
    "utf8",
  );

  // Discovered, not listed. The first version of this test named the
  // mutators it knew about, which is a list that goes stale the moment
  // one is added — `numberPages` and `flattenDocument` were both missing
  // it and both broke undo, after this test was written and passing.
  //
  // So the default is inverted: every session call must either migrate
  // the map or be named here as one that cannot rotate the handle. A new
  // mutator fails until someone decides which it is, which is the only
  // arrangement that survives people forgetting.
  const CANNOT_ROTATE = new Set([
    // Read-only.
    "renderPage",
    "pageSizes",
    "listSignatures",
    "searchDocument",
    "documentOutline",
    "workingCopyBytes",
    "saveToBytes",
    "exportXfdf",
    "compareDocuments",
    // Lifecycle: these establish or end a handle rather than rotate one.
    "openDocument",
    "closeDocument",
    "markSaved",
    // Produce a *new* document, opened separately — nothing to migrate.
    "extractPages",
    "mergeDocuments",
    // Writes a copy without touching the open document.
    "encryptDocumentBytes",
  ]);

  const offenders: string[] = [];
  for (const match of source.matchAll(/session\.(\w+)\(/g)) {
    const name = match[1];
    if (CANNOT_ROTATE.has(name)) continue;
    const around = source.slice(Math.max(0, match.index! - 700), match.index! + 700);
    if (!around.includes("migrateOpenDoc(")) offenders.push(name);
  }

  expect(
    offenders,
    "these rotate the handle but leave openDocs keyed by the old one — add migrateOpenDoc, " +
      "or list them in CANNOT_ROTATE if they genuinely cannot rotate",
  ).toEqual([]);
});

/**
 * OCR, end to end, through the real recogniser.
 *
 * Everything above this checks assets and structure. None of it would
 * have caught what was actually wrong: the code read the words off
 * `data.words`, which tesseract.js has not returned for several major
 * versions — they live under blocks → paragraphs → lines, and that whole
 * tree is null unless `blocks` is asked for. `data.words ?? []` turned
 * that into an empty list, so OCR ran for half a minute, reported
 * success and added nothing at all. In any language. Since the feature
 * costs 1,000 credits, that is the worst possible way for it to fail.
 *
 * So this runs it: recognise a page and confirm the text comes back out
 * through PDFium's own extraction, which is what search uses.
 */
test("OCR puts text into the document that search can find", async ({ browser }) => {
  test.setTimeout(180_000);
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await ctx.newPage();

  // Past the Supporter gate: a session in the store the SDK reads, and
  // an entitlement the gate believes. Neither touches the OCR path — the
  // point is to reach it.
  await page.route("**/v1/credits/entitlement*", (route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: '{"unlocked":true}' }),
  );
  await page.addInitScript((b64: string) => {
    localStorage.setItem(
      "openapps.session",
      JSON.stringify({ accessToken: "test-token", refreshToken: "test-refresh" }),
    );
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    (window as unknown as Record<string, unknown>).showOpenFilePicker = async () => [
      {
        name: "scan.pdf",
        async getFile() {
          return new File([bytes], "scan.pdf", { type: "application/pdf" });
        },
        async createWritable() {
          return { async write() {}, async close() {} };
        },
      },
    ];
  }, TEXT_PDF_BASE64);

  await page.goto(ORIGIN);
  await page.locator("header.topbar").getByRole("button", { name: "Open PDF…" }).click();
  await expect(page.locator("canvas").first()).toBeVisible({ timeout: 60_000 });

  // What the page already has. The fixture carries real text, so OCR of
  // its *rendering* — which is what a scan would give — must add a
  // second, findable copy on top.
  const find = page.getByRole("textbox", { name: "Find in document" });
  const count = page.locator(".find-bar__count");
  const hitsFor = async (query: string) => {
    await page.keyboard.press("Control+f");
    await find.fill(query);
    await expect(count).not.toHaveText("", { timeout: 30_000 });
    const text = await count.innerText();
    const match = text.match(/of (\d+)/);
    return match ? Number(match[1]) : 0;
  };

  const before = await hitsFor("CONFIDENTIAL");
  expect(before, "the fixture should have text to compare against").toBeGreaterThan(0);
  await page.keyboard.press("Escape");

  await page.getByRole("button", { name: "OCR document" }).click();
  // Language, because Tesseract reads the script it has data for and
  // nothing else. English here; the document is English.
  await page.locator(".oa-dialog select").selectOption("eng");
  await page.getByRole("button", { name: "Run OCR" }).click();

  // Done when the document has unsaved changes to show for it — or
  // sooner, with whatever the banner says. Waiting only on the enabled
  // Save turns every failure into a two-minute timeout that reports the
  // button rather than the reason.
  const banner = page.locator(".banner");
  const save = page.getByRole("button", { name: /^(Save|Download a copy)$/ });
  // A race rather than a poll: an error banner is a final answer, and
  // polling for one would keep asking for two more minutes before
  // reporting a result it already had. The loser of the race is parked
  // rather than left to reject after the test has moved on.
  const never = new Promise<string>(() => {});
  const outcome = await Promise.race([
    banner
      .waitFor({ state: "visible", timeout: 150_000 })
      .then(async () => `OCR failed: ${await banner.innerText()}`)
      .catch(() => never),
    expect(save)
      .toBeEnabled({ timeout: 150_000 })
      .then(() => "done")
      .catch(() => never),
  ]);
  expect(outcome).toBe("done");

  expect(
    await hitsFor("CONFIDENTIAL"),
    "OCR added no findable text — the recogniser returned nothing, or the layer is unreadable",
  ).toBeGreaterThan(before);

  await ctx.close();
});
