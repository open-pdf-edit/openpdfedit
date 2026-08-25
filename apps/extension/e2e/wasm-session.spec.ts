// In-browser probe of the WasmSession surface, driven entirely from
// inside a real (headless) Chromium loaded against the packaged dist/ as
// an unpacked extension — the same fixture boot.spec.ts uses, but this
// spec drives `WasmSession` IN-PAGE via `page.evaluate`, bypassing the
// file picker entirely (there is no way to script the real File System
// Access picker headlessly — see boot.spec.ts's own comment on that).
// This is the permanent regression guard for the whole
// open -> list/fill forms -> rotate/crop -> extract -> undo -> save-to-
// -bytes pipeline actually working against the real, vendored PDFium wasm
// build and the real wasm-bindgen glue — nothing else in this repo's
// build/typecheck/unit-test pipeline instantiates PDFium's wasm build at
// all, so a broken pdfium.js/pdfium.wasm vendor drop or a wasm-bindgen
// version mismatch (see scripts/build-wasm.sh's own version-pin comment)
// would otherwise only surface the first time a real user opened a file.
//
// The init sequence below (inject <script src="/pdfium.js"> -> await
// window.PDFiumModule() -> dynamic import("/wasm-gen/openpdfedit_wasm.js")
// -> mod.default() -> mod.initialize_pdfium_render(...) -> new
// mod.WasmSession()) is copy-verbatim what
// apps/desktop/src/lib/backend/wasm.ts's own `initSession` does — see
// that file for the full rationale behind each step, in particular why
// `initialize_pdfium_render` has to run before any PDFium call and why
// this whole sequence can only run inside a real extension-origin page,
// not apps/desktop's own dev server (no /pdfium.js, no /wasm-gen/ there).
import { expect, test } from "./fixtures";
// FORM_PDF_BASE64/TEXT_PDF_BASE64 moved to pdf-fixtures.ts (Phase 5
// final-review fix round, I3) so ui-flows.spec.ts can reuse the exact same
// bytes rather than embedding a second copy — see that file for how each
// was built and what each one is for.
import { FORM_PDF_BASE64, TEXT_PDF_BASE64 } from "./pdf-fixtures";

test("WasmSession, driven in-page, round-trips open/fill/mutate/undo/save against real PDFium wasm", async ({ context, extensionId }) => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const result = await page.evaluate(async ({ formBase64, textBase64 }) => {
    // --- init sequence: verbatim copy of wasm.ts's initSession ---------
    await new Promise<void>((resolve, reject) => {
      const script = document.createElement("script");
      script.src = "/pdfium.js";
      script.onload = () => resolve();
      script.onerror = () => reject(new Error("failed to load /pdfium.js"));
      document.head.appendChild(script);
    });
    const win = window as unknown as { PDFiumModule?: () => Promise<unknown> };
    if (!win.PDFiumModule) throw new Error("/pdfium.js loaded but never defined window.PDFiumModule");
    const pdfiumModule = await win.PDFiumModule();

    const mod = (await import(/* @vite-ignore */ "/wasm-gen/openpdfedit_wasm.js")) as {
      default: () => Promise<unknown>;
      initialize_pdfium_render: (pdfiumModule: unknown, rustModule: unknown, debug: boolean) => boolean;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      WasmSession: new () => any;
    };
    const rustModule = await mod.default();
    const ok = mod.initialize_pdfium_render(pdfiumModule, rustModule, false);
    if (!ok) throw new Error("initialize_pdfium_render failed");

    const session = new mod.WasmSession();

    // --- bytes: base64 -> Uint8Array ------------------------------------
    const toBytes = (base64: string) => {
      const binary = atob(base64);
      const out = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
      return out;
    };
    const bytes = toBytes(formBase64);

    // --- the actual probe -------------------------------------------------
    const opened = JSON.parse(session.openDocument("form.pdf", bytes));
    const handle: number = opened.handle;

    // Phase 4 surface, part 1: confirm (rather than assume) that the
    // AcroForm fixture's page content has no page-content text runs (its
    // field lives only in an annotation appearance — see TEXT_PDF_BASE64's
    // comment above) and that an unsigned document reports no signatures.
    const formRunsBeforeAnything = JSON.parse(session.listTextRuns(handle, 0));
    const signaturesOnUnsignedFixture = JSON.parse(session.listSignatures(handle));

    const fieldsBefore = JSON.parse(session.listFormFields(handle));
    const fieldBefore = fieldsBefore.find((f: { name: string }) => f.name === "full_name");

    const afterFill = JSON.parse(
      session.fillFormFields(JSON.stringify({ handle, values: { full_name: "Ada Lovelace" } })),
    );
    const fieldsAfterFill = JSON.parse(session.listFormFields(afterFill.handle));
    const fieldAfterFill = fieldsAfterFill.find((f: { name: string }) => f.name === "full_name");

    const afterRotate = JSON.parse(session.rotatePage(afterFill.handle, 0, 90));
    const afterCrop = JSON.parse(
      session.setCropBox(afterRotate.handle, 0, Float32Array.from([0, 0, 500, 700])),
    );

    const extracted = session.extractPages(
      JSON.stringify({ handle: afterCrop.handle, pageIndices: [0] }),
    ) as Uint8Array;
    const extractedHeader = new TextDecoder().decode(extracted.slice(0, 5));

    const afterUndo = JSON.parse(session.undo(afterCrop.handle));

    // The FPDF_SaveAsCopy function-table canary: saveToBytes exercises
    // pdfium-render's name-installed write-callback function
    // ("write_block_from_callback_wasm", see wasm.ts's initSession doc) —
    // nothing else in this suite calls the write path, so this is the one
    // thing that would actually catch a pdfium.js/pdfium.wasm re-vendor
    // that silently changed or dropped that export.
    const saved = session.saveToBytes(afterUndo.handle) as Uint8Array;
    const savedHeader = new TextDecoder().decode(saved.slice(0, 5));

    // --- Phase 4 surface, part 2: real text runs (TEXT_PDF_BASE64) ------
    // A second, independent document — WasmSession's `docs` map holds
    // both open documents at once, exactly like the desktop's multi-tab
    // usage; this doesn't disturb the FORM_PDF probe above.
    const textOpened = JSON.parse(session.openDocument("text.pdf", toBytes(textBase64)));
    const textHandle: number = textOpened.handle;

    const runsBeforeEdit = JSON.parse(session.listTextRuns(textHandle, 0)) as Array<{
      text: string;
      isEditable: boolean;
      rect: [number, number, number, number];
    }>;

    // editTextRun: mutating, must rotate the handle and mark dirty, and
    // the new text must show up in a fresh listTextRuns call.
    const afterEdit = JSON.parse(
      session.editTextRun(
        JSON.stringify({ handle: textHandle, pageIndex: 0, runIndex: 0, newText: "EDITED TEXT" }),
      ),
    );
    const runsAfterEdit = JSON.parse(session.listTextRuns(afterEdit.handle, 0)) as Array<{
      text: string;
      rect: [number, number, number, number];
    }>;

    // redactPage: mutating, must rotate the handle and mark dirty; permanently
    // removes the run's underlying content (not a black box painted over
    // live data — see openpdfedit-redact's module doc), which a fresh
    // listTextRuns call after the redaction must reflect as an empty page.
    // The redaction rect is the just-edited run's own bbox (padded by a
    // couple of points), read fresh off the document rather than
    // hardcoded, since edit_text_run keeps the run at its original
    // position/font but rescales its width to approximately match the new
    // text.
    const editedRect = runsAfterEdit[0].rect;
    const redactRect: [number, number, number, number] = [
      editedRect[0] - 2,
      editedRect[1] - 2,
      editedRect[2] + 2,
      editedRect[3] + 2,
    ];
    const afterRedact = JSON.parse(
      session.redactPage(
        JSON.stringify({ handle: afterEdit.handle, pageIndex: 0, rect: redactRect }),
      ),
    );
    const runsAfterRedact = JSON.parse(session.listTextRuns(afterRedact.handle, 0));

    // applyWatermark: mutating, must rotate the handle and mark dirty.
    // Applied to the just-redacted (now text-free) page, the tiled
    // watermark text becomes this page's ONLY text content, so a fresh
    // listTextRuns call must find real "WATERMARK" runs where the
    // redaction just left zero — proving the stamp landed in the page's
    // real content stream, not merely that the call returned.
    const afterWatermark = JSON.parse(
      session.applyWatermark(
        JSON.stringify({
          handle: afterRedact.handle,
          text: "WATERMARK",
          location: "full",
          orientationDeg: 45,
          opacity: 0.4,
          textScale: 1,
        }),
      ),
    );
    const runsAfterWatermark = JSON.parse(
      session.listTextRuns(afterWatermark.handle, 0),
    ) as Array<{ text: string }>;

    // --- Phase 5 Task 2 surface: mergeDocuments/compareDocuments --------
    // Exercised against two fresh, independent single-page opens (not the
    // already-mutated/closed chains above) so this block stays isolated
    // from this spec's own execution order.
    const mergeOpenDoc = JSON.parse(session.openDocument("merge-open.pdf", toBytes(formBase64)));
    const mergeOpenHandle: number = mergeOpenDoc.handle;

    // The length-prefixed sources buffer wasm.ts's own
    // `packLengthPrefixedSources` builds — one extra source (the text
    // fixture), built by hand here since this spec bypasses wasm.ts
    // entirely (see this file's header comment).
    const mergeSourceBytes = toBytes(textBase64);
    const sourcesBuffer = new Uint8Array(4 + mergeSourceBytes.length);
    new DataView(sourcesBuffer.buffer).setUint32(0, mergeSourceBytes.length, true);
    sourcesBuffer.set(mergeSourceBytes, 4);

    const mergedBytes = session.mergeDocuments(
      JSON.stringify({ openHandle: mergeOpenHandle }),
      sourcesBuffer,
    ) as Uint8Array;
    const mergedHeader = new TextDecoder().decode(mergedBytes.slice(0, 5));

    // Round-trip the merged result through a real openDocument call —
    // the most convincing proof this is a real, reparseable merged PDF
    // (both sources' pages present) rather than just non-empty bytes.
    const mergedOpened = JSON.parse(session.openDocument("merged.pdf", mergedBytes));

    // Merging must never rotate or otherwise touch the *source* open
    // document's handle (matching the desktop's `merge_documents_impl`) —
    // it must still resolve, unrotated, to its own single-page size list.
    const mergeOpenDocStillPageSizes = JSON.parse(session.pageSizes(mergeOpenHandle));

    // compareDocuments: read-only, no handle/open-document involvement at
    // all — bytesA (form fixture, no page-content text) vs bytesB (text
    // fixture, one real text run) should report a text diff on page 0 and
    // one pixel-diff entry (both are single-page).
    const compareReport = JSON.parse(
      session.compareDocuments(
        toBytes(formBase64),
        toBytes(textBase64),
        JSON.stringify({ pixelTargetWidth: 200 }),
      ),
    );

    // Close every document's final live handle — this probe opens several
    // independent documents in the same WasmSession and none was closed
    // as it went, leaking them out of `WasmSession`'s `docs`/`history`
    // maps for the rest of this page's lifetime. Harmless for a single
    // test run in a throwaway page (see `page.close()` below), but
    // leaving them open was sloppy hygiene for what this spec otherwise
    // treats as a real, representative session — a real caller (wasm.ts's
    // `closeDocument`) always closes a document once done with it.
    // `afterUndo.handle`/`afterRedact.handle` are each earlier chain's
    // last-rotated (still-live) handle.
    session.closeDocument(afterUndo.handle);
    session.closeDocument(afterWatermark.handle);
    session.closeDocument(mergeOpenHandle);
    session.closeDocument(mergedOpened.handle);

    return {
      fieldBeforeValue: fieldBefore?.value ?? null,
      afterFillIsDirty: afterFill.is_dirty,
      afterFillCanUndo: afterFill.can_undo,
      fieldAfterFillValue: fieldAfterFill?.value ?? null,
      afterCropPageCount: afterCrop.page_count,
      extractedHeader,
      extractedLength: extracted.length,
      afterUndoIsDirty: afterUndo.is_dirty,
      savedHeader,
      savedLength: saved.length,
      formRunsBeforeAnythingLength: formRunsBeforeAnything.length,
      signaturesOnUnsignedFixtureLength: signaturesOnUnsignedFixture.length,
      runsBeforeEditLength: runsBeforeEdit.length,
      runBeforeEditText: runsBeforeEdit[0]?.text ?? null,
      runBeforeEditIsEditable: runsBeforeEdit[0]?.isEditable ?? null,
      textHandle,
      afterEditHandle: afterEdit.handle,
      afterEditIsDirty: afterEdit.is_dirty,
      runAfterEditText: runsAfterEdit[0]?.text ?? null,
      afterRedactHandle: afterRedact.handle,
      afterRedactIsDirty: afterRedact.is_dirty,
      runsAfterRedactLength: runsAfterRedact.length,
      afterWatermarkHandle: afterWatermark.handle,
      afterWatermarkIsDirty: afterWatermark.is_dirty,
      runsAfterWatermarkLength: runsAfterWatermark.length,
      watermarkRunTexts: [...new Set(runsAfterWatermark.map((r) => r.text))],
      mergedHeader,
      mergedBytesLength: mergedBytes.length,
      mergedPageCount: mergedOpened.page_count,
      mergeOpenDocStillPageSizesLength: mergeOpenDocStillPageSizes.length,
      compareReport,
    };
  }, { formBase64: FORM_PDF_BASE64, textBase64: TEXT_PDF_BASE64 });

  // list -> fill: value round-trips through PDFium's own form model.
  expect(result.fieldBeforeValue).toBeNull();
  expect(result.afterFillIsDirty).toBe(true);
  expect(result.afterFillCanUndo).toBe(true);
  expect(result.fieldAfterFillValue).toBe("Ada Lovelace");

  // rotate + crop landed (rotate/crop don't change page count).
  expect(result.afterCropPageCount).toBe(1);

  // extractPages returns real PDF bytes, not an error/empty buffer.
  expect(result.extractedHeader).toBe("%PDF-");
  expect(result.extractedLength).toBeGreaterThan(0);

  // undo reverted the crop/rotate/fill chain but the working copy is
  // still dirty relative to the original bytes (undoing isn't saving —
  // see openpdfedit-session's own undo_impl tests for the same
  // assertion).
  expect(result.afterUndoIsDirty).toBe(true);

  // saveToBytes (the FPDF_SaveAsCopy canary) produced real PDF bytes.
  expect(result.savedHeader).toBe("%PDF-");
  expect(result.savedLength).toBeGreaterThan(0);

  // The AcroForm fixture's page content genuinely has no text runs (its
  // field lives only in an annotation appearance) — a real empty-case
  // assertion, not an assumption, and the reason this spec adds a second
  // fixture (TEXT_PDF_BASE64) for the redact/text-run surface below.
  expect(result.formRunsBeforeAnythingLength).toBe(0);

  // An unsigned document reports zero signatures — real empty-case
  // assertion (the signed-fixture path is covered by
  // openpdfedit-session's own Rust tests, e.g.
  // `list_signatures_impl_finds_a_signature_via_the_open_documents_working_copy`).
  expect(result.signaturesOnUnsignedFixtureLength).toBe(0);

  // listTextRuns finds the one real page-content text run and correctly
  // marks it editable (plain Helvetica/WinAnsiEncoding, not a CID-encoded
  // subset font).
  expect(result.runsBeforeEditLength).toBe(1);
  expect(result.runBeforeEditText).toBe("CONFIDENTIAL");
  expect(result.runBeforeEditIsEditable).toBe(true);

  // editTextRun is mutating: it must rotate to a fresh handle and mark
  // the working copy dirty, and the substituted text must show up in a
  // fresh listTextRuns call against the rotated handle.
  expect(result.afterEditHandle).not.toBe(result.textHandle);
  expect(result.afterEditIsDirty).toBe(true);
  expect(result.runAfterEditText).toBe("EDITED TEXT");

  // redactPage is mutating: it must rotate to yet another fresh handle
  // and mark the working copy dirty, and — because this is true content
  // removal, not a black box painted over live data — a fresh
  // listTextRuns call against the rotated handle must find zero runs
  // left on the page.
  expect(result.afterRedactHandle).not.toBe(result.afterEditHandle);
  expect(result.afterRedactIsDirty).toBe(true);
  expect(result.runsAfterRedactLength).toBe(0);

  // applyWatermark is mutating: fresh handle, dirty, and the tiled stamp
  // text is now really in the page's content stream — the page that had
  // zero runs after redaction now lists a full grid of "WATERMARK" runs.
  expect(result.afterWatermarkHandle).not.toBe(result.afterRedactHandle);
  expect(result.afterWatermarkIsDirty).toBe(true);
  expect(result.runsAfterWatermarkLength).toBeGreaterThan(1);
  expect(result.watermarkRunTexts).toEqual(["WATERMARK"]);

  // mergeDocuments produced a real, reparseable PDF combining both
  // sources' pages (the form fixture's 1 page + the text fixture's 1
  // page = 2), and never touched the source open document's own handle.
  expect(result.mergedHeader).toBe("%PDF-");
  expect(result.mergedBytesLength).toBeGreaterThan(0);
  expect(result.mergedPageCount).toBe(2);
  expect(result.mergeOpenDocStillPageSizesLength).toBe(1);

  // compareDocuments found the expected page-0 text diff (the form
  // fixture has no page-content text; the text fixture has one real run)
  // and one pixel-diff entry (both fixtures are single-page).
  expect(result.compareReport.pageCountA).toBe(1);
  expect(result.compareReport.pageCountB).toBe(1);
  expect(result.compareReport.textPages).toHaveLength(1);
  expect(result.compareReport.textPages[0].added).toEqual(["CONFIDENTIAL"]);
  expect(result.compareReport.textPages[0].removed).toEqual([]);
  expect(result.compareReport.pixelPages).toHaveLength(1);
  expect(result.compareReport.pixelPages[0].differingPixels).toBeGreaterThan(0);

  await page.close();
});

/**
 * Two OCR passes in a row.
 *
 * Committing a text layer rotates the document handle — `commit_mutation`
 * reopens and hands back a fresh one. The backend keeps its own map of
 * open documents keyed by handle, so a mutating call that updates the
 * document but not the key leaves the map under the old handle: the next
 * call arrives with the new one and is told the document does not exist.
 * Reported as "ocrDocument: unknown document handle 5", after OCR had
 * already succeeded once.
 *
 * Driven against `WasmSession` directly rather than through the UI so it
 * needs no recogniser: what rotates the handle is the commit, not the
 * reading, and a stub word exercises the same path in a fraction of the
 * time.
 */
test("addOcrTextLayer rotates the handle, and the new one is usable", async ({
  context,
  extensionId,
}) => {
  const page = await context.newPage();
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const result = await page.evaluate(async (textBase64) => {
    await new Promise<void>((resolve, reject) => {
      const script = document.createElement("script");
      script.src = "/pdfium.js";
      script.onload = () => resolve();
      script.onerror = () => reject(new Error("failed to load /pdfium.js"));
      document.head.appendChild(script);
    });
    const win = window as unknown as { PDFiumModule?: () => Promise<unknown> };
    const pdfiumModule = await win.PDFiumModule!();
    const mod = (await import(/* @vite-ignore */ "/wasm-gen/openpdfedit_wasm.js")) as {
      default: () => Promise<unknown>;
      initialize_pdfium_render: (a: unknown, b: unknown, c: boolean) => boolean;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      WasmSession: new () => any;
    };
    const rustModule = await mod.default();
    mod.initialize_pdfium_render(pdfiumModule, rustModule, false);
    const session = new mod.WasmSession();

    const bytes = Uint8Array.from(atob(textBase64), (c) => c.charCodeAt(0));
    const opened = JSON.parse(session.openDocument("scan.pdf", bytes));

    const layer = (handle: number) =>
      JSON.parse(
        session.addOcrTextLayer(
          JSON.stringify({
            handle,
            pages: [
              {
                page_index: 0,
                page_width_pt: 612,
                page_height_pt: 792,
                image_width_px: 1275,
                image_height_px: 1650,
                words: [
                  { text: "SCANNED", left: 150, top: 200, width: 400, height: 50, confidence: 90 },
                ],
              },
            ],
          }),
        ),
      );

    const first = layer(opened.handle);
    // The handle the second pass must use is the one the first returned.
    const second = layer(first.handle);
    return { opened: opened.handle, first: first.handle, second: second.handle };
  }, TEXT_PDF_BASE64);

  expect(result.first).not.toBe(result.opened);
  expect(result.second).not.toBe(result.first);
});
