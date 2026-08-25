import { expect, test } from "@playwright/test";

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
