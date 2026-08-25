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
