// `removeMarkup` driven in-page against the real PDFium wasm build, on a
// document whose markup was flattened into the page before it ever
// reached this app.
//
// The unit tests in `openpdfedit-unmark` prove the detection rules; this
// proves the whole path works where it has to — real wasm, real handle
// rotation, real undo — and that the guard which keeps a page from being
// emptied survives the crossing.
//
// Worth its own spec because the interesting document here has no
// annotations at all. Everything else in this suite that touches markup
// works through `/Annots`, so nothing else would notice if the
// content-stream half stopped working.
import { expect, test } from "./fixtures";
import { MARKED_UP_PDF_BASE64 } from "./pdf-fixtures";

test("removeMarkup takes off a flattened pen layer, keeps the page, and undoes", async ({
  context,
  extensionId,
}) => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const result = await page.evaluate(async (base64: string) => {
    // Init sequence: verbatim from wasm.ts's initSession, as in
    // wasm-session.spec.ts — see that file for why each step is needed.
    await new Promise<void>((resolve, reject) => {
      const script = document.createElement("script");
      script.src = "/pdfium.js";
      script.onload = () => resolve();
      script.onerror = () => reject(new Error("failed to load /pdfium.js"));
      document.head.appendChild(script);
    });
    const win = window as unknown as { PDFiumModule?: () => Promise<unknown> };
    if (!win.PDFiumModule) throw new Error("/pdfium.js never defined window.PDFiumModule");
    const pdfiumModule = await win.PDFiumModule();

    const mod = (await import(/* @vite-ignore */ "/wasm-gen/openpdfedit_wasm.js")) as {
      default: () => Promise<unknown>;
      initialize_pdfium_render: (a: unknown, b: unknown, debug: boolean) => boolean;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      WasmSession: new () => any;
    };
    const rustModule = await mod.default();
    if (!mod.initialize_pdfium_render(pdfiumModule, rustModule, false)) {
      throw new Error("initialize_pdfium_render failed");
    }
    const session = new mod.WasmSession();

    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);

    const opened = JSON.parse(session.openDocument("marked-up.pdf", bytes));
    const textOf = (handle: number) =>
      (JSON.parse(session.listTextRuns(handle, 0)) as Array<{ text: string }>)
        .map((run) => run.text)
        .join(" ");
    const before = textOf(opened.handle);

    const removed = JSON.parse(
      session.removeMarkup(JSON.stringify({ handle: opened.handle })),
    );
    const afterText = textOf(removed.document.handle);

    const undone = JSON.parse(session.undo(removed.document.handle));

    return {
      before,
      annotations: removed.annotations,
      layers: removed.layers,
      rotated: removed.document.handle !== opened.handle,
      dirty: removed.document.is_dirty,
      afterText,
      undoneRotated: undone.handle !== removed.document.handle,
    };
  }, MARKED_UP_PDF_BASE64);

  expect(result.before).toContain("THE DOCUMENT");

  // No annotation in the file at all — the layer is page content, which
  // is exactly the case that made this tool necessary.
  expect(result.annotations).toBe(0);
  expect(result.layers).toBe(1);

  // The document under the markup has to be left alone. A rule that
  // removed the overlay by removing the page would pass a "the markup
  // is gone" assertion just as well.
  expect(result.afterText).toContain("THE DOCUMENT");

  expect(result.rotated, "a mutating call must rotate the handle").toBe(true);
  expect(result.dirty).toBe(true);
  expect(result.undoneRotated, "undo rotates the handle back to a fresh one").toBe(true);
});
