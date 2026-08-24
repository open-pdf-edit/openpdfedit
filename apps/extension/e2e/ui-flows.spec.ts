// Permanent third e2e spec (Phase 5 final-review fix round, I3): drives
// the REAL +page.svelte UI end to end — open a fixture PDF, paint a page,
// drag a highlight annotation, save, delete/undo/redo it, merge, and
// compare — against a real (headless) Chromium loaded against the packaged
// dist/ as an unpacked extension, the same fixture boot.spec.ts and
// wasm-session.spec.ts both use.
//
// Neither of those two specs exercises the UI itself: boot.spec.ts only
// confirms the empty-state shell mounts (it explicitly defers driving the
// real File System Access picker headlessly as "genuinely hard" — see its
// own comment), and wasm-session.spec.ts bypasses the UI and the picker
// entirely, calling `WasmSession` directly via `page.evaluate`. That
// combination left the actual UI <-> Backend wiring — the picker, the
// dialogs, the drag gestures, `+page.svelte`'s own state management —
// completely unexercised by anything in this repo's automated pipeline.
// It also means this is the ONE place `wasm.ts`'s pick-key/file_path
// namespace-unification fix (C1 in the Phase 5 final review — see
// wasm.ts's `openFromFileHandle` doc) can actually be verified end to end:
// the bug it fixed only manifests through the real `pickOpenPath(s)` /
// `pickSavePath` UI call sequence, never through wasm-session.spec.ts's
// direct `WasmSession` calls (which never touch `pendingOpenPicks` at
// all — that bookkeeping lives entirely in wasm.ts, not the Rust crate).
//
// The technique: `page.addInitScript` (must run before `page.goto`, so it
// exists before wasm.ts's own module-scope code ever runs) replaces
// `window.showOpenFilePicker`/`showSaveFilePicker` with in-memory
// `FileSystemFileHandle`-like stand-ins (`getFile()`/`createWritable()`
// backed by a plain `Map<string, Uint8Array>` on `window`) that a test
// pre-seeds/queues via `page.evaluate`, then drives the real UI with
// `page.mouse`/`page.locator` — nothing here bypasses `+page.svelte` or
// `wasm.ts`. This is the only way to headlessly script the File System
// Access API at all: it has no other test-mode hook, and Chromium refuses
// to grant it to a page without a real user gesture behind a real dialog.
import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import { FORM_PDF_BASE64, TEXT_PDF_BASE64 } from "./pdf-fixtures";

// `text_selection_quads_impl` (crates/openpdfedit-session/src/annotations.rs)
// is NOT a bounding-box-overlap test — it's a click-drag character-range
// selection, the same model a text editor's click-drag-to-select uses:
// `nearest_char_index` resolves each of (x0,y0)/(x1,y1) independently to
// the geometrically nearest real character PDFium found on the page, then
// `near_enough` requires that point to actually be within
// `SELECTION_SNAP_MARGIN` (20 PDF-point units) of *that specific
// character's* own box — not merely "inside a big rectangle somewhere
// over the text". A drag that starts or ends more than ~20pt from any
// real glyph resolves to zero quads (`+page.svelte` then shows a "No text
// found" alert instead of creating an annotation — this spec's own
// `noTextAlert` race exists because an earlier, over-generous rect here
// hit exactly that). TEXT_PDF_BASE64's one real page-content text run
// ("CONFIDENTIAL") sits at PDF-point (50, 50), 24pt Helvetica — so these
// two points are chosen well inside its actual ink (not at its outer
// edges, where a few points of estimation error in the word's total
// advance width could push a point outside the 20pt margin): `start`
// lands on/near an early character, `end` on/near a later one, both at
// y=58 (comfortably inside the glyphs' baseline-to-cap-height span,
// baseline 50 + Helvetica's ~17.2pt cap height at 24pt).
const HIGHLIGHT_DRAG_START_PT = { x: 70, y: 58 };
const HIGHLIGHT_DRAG_END_PT = { x: 190, y: 58 };
// A point inside the resulting annotation's rect (the bounding box of
// whatever character range the drag above actually resolved to) — used
// to click-to-select the annotation once it exists, for the
// delete/undo/redo leg below. Same y as the drag itself; x roughly
// centered between HIGHLIGHT_DRAG_START_PT/END_PT.
const HIGHLIGHT_CLICK_POINT_PT = { x: 130, y: 58 };

// PdfPage.svelte's own CSS-px-per-PDF-point constant at 100% zoom
// (Viewer.svelte's `BASE_PX_PER_PT = 96 / 72`) — duplicated here (not
// importable: it's a private module-scope const, not exported) so this
// spec can convert its own PDF-point drag targets into the CSS pixel
// coordinates `page.mouse` actually needs. `zoom` starts at 1 (see
// +page.svelte's `let zoom = $state(1)`), so no zoom multiplier applies.
const BASE_PX_PER_PT = 96 / 72;
// TEXT_PDF_BASE64's MediaBox is Letter-sized ([0 0 612 792]) — see
// pdf-fixtures.ts's own comment on that fixture.
const PAGE_HEIGHT_PT = 792;

/** PDF page-space (origin bottom-left, y-up) -> CSS pixels relative to the
 * page container's own top-left corner — the exact inverse of
 * PdfPage.svelte's own `cssToPdfPoint`. */
function pdfPointToLocalCssPx(x: number, y: number): { x: number; y: number } {
  return { x: x * BASE_PX_PER_PT, y: (PAGE_HEIGHT_PT - y) * BASE_PX_PER_PT };
}

/** Installs in-memory stand-ins for `window.showOpenFilePicker`/
 * `showSaveFilePicker` before any page script runs, so `wasm.ts`'s picker
 * calls resolve against a fake in-page filesystem instead of throwing
 * (there is no headless File System Access picker to script for real —
 * see this file's header comment). Exposes three globals a test drives
 * via `page.evaluate`:
 *
 * - `__e2eFiles: Map<string, Uint8Array>` — the fake filesystem's
 *   contents, keyed by filename. Seed a fixture's bytes here before a
 *   pick that's supposed to read them; read a save's written bytes back
 *   out here afterward.
 * - `__e2eOpenQueue: string[][]` — one entry per expected
 *   `showOpenFilePicker` call, shifted off in call order. An empty array
 *   (`[]`) simulates the user cancelling that pick (an `AbortError`,
 *   matching the real API's contract on Cancel).
 * - `__e2eSaveQueue: (string | null)[]` — one entry per expected
 *   `showSaveFilePicker` call, shifted off in call order. `null`
 *   simulates cancelling that save dialog.
 *
 * Queues, not fixed return values, because a single test drives several
 * picker calls in sequence (merge's source pick then its save-target
 * pick, each open/compare's own pick, ...) and each needs an independently
 * controllable outcome, including — the whole point of this spec's C1
 * regression coverage — a cancel on one call with real picks before and
 * after it. */
async function installFilePickerStubs(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const files = new Map<string, Uint8Array>();
    const openQueue: string[][] = [];
    const saveQueue: (string | null)[] = [];
    (window as unknown as { __e2eFiles: typeof files }).__e2eFiles = files;
    (window as unknown as { __e2eOpenQueue: typeof openQueue }).__e2eOpenQueue = openQueue;
    (window as unknown as { __e2eSaveQueue: typeof saveQueue }).__e2eSaveQueue = saveQueue;

    class FakeWritable {
      private chunks: Uint8Array[] = [];
      constructor(private readonly name: string) {}
      async write(data: Uint8Array): Promise<void> {
        this.chunks.push(data);
      }
      async close(): Promise<void> {
        const total = this.chunks.reduce((sum, c) => sum + c.length, 0);
        const buf = new Uint8Array(total);
        let offset = 0;
        for (const chunk of this.chunks) {
          buf.set(chunk, offset);
          offset += chunk.length;
        }
        files.set(this.name, buf);
      }
    }

    class FakeFileHandle {
      readonly kind = "file";
      constructor(readonly name: string) {}
      async getFile(): Promise<File> {
        const bytes = files.get(this.name) ?? new Uint8Array();
        return new File([bytes], this.name, { type: "application/pdf" });
      }
      async createWritable(): Promise<FakeWritable> {
        return new FakeWritable(this.name);
      }
    }

    function abort(): never {
      throw new DOMException("cancelled by e2e stub", "AbortError");
    }

    (window as unknown as { showOpenFilePicker: () => Promise<FakeFileHandle[]> }).showOpenFilePicker = async () => {
      const names = openQueue.shift();
      if (!names || names.length === 0) abort();
      return names.map((name) => new FakeFileHandle(name));
    };

    (window as unknown as { showSaveFilePicker: () => Promise<FakeFileHandle> }).showSaveFilePicker = async () => {
      const name = saveQueue.shift();
      if (name === undefined || name === null) abort();
      return new FakeFileHandle(name);
    };
  });
}

async function seedFile(page: Page, name: string, base64: string): Promise<void> {
  await page.evaluate(
    ({ name, base64 }) => {
      const binary = atob(base64);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      (window as unknown as { __e2eFiles: Map<string, Uint8Array> }).__e2eFiles.set(name, bytes);
    },
    { name, base64 },
  );
}

async function queueOpenPick(page: Page, names: string[]): Promise<void> {
  await page.evaluate((names) => {
    (window as unknown as { __e2eOpenQueue: string[][] }).__e2eOpenQueue.push(names);
  }, names);
}

async function queueSavePick(page: Page, name: string | null): Promise<void> {
  await page.evaluate((name) => {
    (window as unknown as { __e2eSaveQueue: (string | null)[] }).__e2eSaveQueue.push(name);
  }, name);
}

/** Length + `%PDF-` header of whatever the fake filesystem currently holds
 * for `name` (or `null` if nothing was ever written there) — the cheapest
 * real proof a save actually wrote real PDF bytes to the picked target,
 * without needing to parse the file. */
async function fakeFileHeader(page: Page, name: string): Promise<{ length: number; header: string } | null> {
  return page.evaluate((name) => {
    const bytes = (window as unknown as { __e2eFiles: Map<string, Uint8Array> }).__e2eFiles.get(name);
    if (!bytes) return null;
    return { length: bytes.length, header: new TextDecoder().decode(bytes.slice(0, 5)) };
  }, name);
}

test("the real UI: open, paint, highlight, save, delete/undo/redo, merge (incl. the C1 cancel-then-reopen repro), compare", async ({
  context,
  extensionId,
}) => {
  test.setTimeout(60_000);

  const page = await context.newPage();
  // Must run before page.goto below: addInitScript's callback re-runs on
  // every subsequent navigation this page makes, but has to be registered
  // before the *first* one so it exists before wasm.ts's own module-scope
  // code (and any picker call it makes) ever runs.
  await installFilePickerStubs(page);
  // A tall, wide viewport so the whole page (a Letter page at 100% zoom is
  // 816x1056 CSS px — 612x792pt * 96/72) is on-screen with no scrolling
  // needed: `page.mouse` drives absolute viewport coordinates and does not
  // auto-scroll the way a locator-based `.click()` would.
  await page.setViewportSize({ width: 1200, height: 1400 });
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  await seedFile(page, "a.pdf", TEXT_PDF_BASE64);
  await seedFile(page, "b.pdf", FORM_PDF_BASE64);

  // Scoped to the topbar: with no document open, the empty-state view
  // *also* renders an "Open PDF…" button (same text, different element),
  // so an unscoped `getByRole("button", { name: "Open PDF…" })` is a
  // strict-mode violation the first time this is used, before any
  // document is open. The topbar's own copy renders unconditionally
  // (unlike the empty-state one, which only exists while `!doc`), so
  // scoping to it works at every point in this test, not just the first.
  const openPdfButton = page.locator("header.topbar").getByRole("button", { name: "Open PDF…" });

  // --- 1. Open a fixture PDF, confirm it actually painted ------------------
  await queueOpenPick(page, ["a.pdf"]);
  await openPdfButton.click();

  const pathBarText = page.locator(".path-bar__text");
  await expect(pathBarText).toHaveText("a.pdf");

  const canvas = page.locator("canvas");
  await expect(canvas).toBeVisible();
  // A real, opaque painted pixel (PDFium always renders an opaque page
  // background) — not just "the canvas element exists and is sized",
  // which `canvas.width > 0` alone wouldn't distinguish from a canvas
  // that was resized but never actually got a `putImageData` call.
  await page.waitForFunction(() => {
    const el = document.querySelector("canvas") as HTMLCanvasElement | null;
    if (!el || el.width === 0 || el.height === 0) return false;
    const ctx = el.getContext("2d");
    if (!ctx) return false;
    return ctx.getImageData(0, 0, 1, 1).data[3] === 255;
  });

  // --- 2. Drag a highlight over the fixture's real text run ----------------
  await page.getByRole("button", { name: "Highlight", exact: true }).click();

  const interactionLayer = page.locator(".interaction-layer");
  const box = await interactionLayer.boundingBox();
  if (!box) throw new Error("interaction-layer has no bounding box — page didn't render?");
  const start = pdfPointToLocalCssPx(HIGHLIGHT_DRAG_START_PT.x, HIGHLIGHT_DRAG_START_PT.y);
  const end = pdfPointToLocalCssPx(HIGHLIGHT_DRAG_END_PT.x, HIGHLIGHT_DRAG_END_PT.y);
  await page.mouse.move(box.x + start.x, box.y + start.y);
  await page.mouse.down();
  await page.mouse.move(box.x + (start.x + end.x) / 2, box.y + (start.y + end.y) / 2, { steps: 5 });
  await page.mouse.move(box.x + end.x, box.y + end.y, { steps: 5 });
  await page.mouse.up();

  // handleCreateAnnotation resolves the drag against real text via
  // textSelectionQuads, then addAnnotation — both real backend round trips
  // (not instant), so wait for their combined effect rather than a fixed
  // sleep. If the drag rect somehow missed the text, +page.svelte shows a
  // "No text found..." alert dialog instead of ever going dirty — race the
  // two outcomes so a miss fails fast with a clear message instead of
  // just timing out on ".path-bar__dirty" with no clue why.
  const noTextAlert = page.getByRole("dialog").filter({ hasText: "No text found in that area" });
  await Promise.race([
    expect(page.locator(".path-bar__dirty")).toBeVisible({ timeout: 10_000 }),
    expect(noTextAlert)
      .toBeVisible({ timeout: 10_000 })
      .then(() => {
        throw new Error(
          "the highlight drag missed the fixture's text run — HIGHLIGHT_DRAG_RECT_PT no longer covers it",
        );
      }),
  ]);

  // --- 3. Comments panel shows the new annotation ---------------------------
  await page.getByRole("button", { name: "Toggle comments panel" }).click();
  const commentsPanel = page
    .locator("aside.oa-panel")
    .filter({ has: page.locator(".oa-panel__title", { hasText: "Comments" }) });
  await expect(commentsPanel.locator(".oa-list-item")).toHaveCount(1);
  await expect(commentsPanel.locator(".subtype")).toHaveText("Highlight");

  // --- 4. Save: dirty clears, real bytes land in the fake filesystem -------
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator(".path-bar__dirty")).toHaveCount(0);
  const savedA = await fakeFileHeader(page, "a.pdf");
  expect(savedA?.header).toBe("%PDF-");
  expect(savedA?.length).toBeGreaterThan(0);

  // --- 5. Delete the annotation, undo, redo ---------------------------------
  // CommentsPanel itself has no delete affordance of its own — deletion in
  // this app goes through the Select tool: click the thing on the canvas,
  // confirm the dialog (see +page.svelte's handleToolClick).
  await page.getByRole("button", { name: "Select", exact: true }).click();
  const clickPt = pdfPointToLocalCssPx(HIGHLIGHT_CLICK_POINT_PT.x, HIGHLIGHT_CLICK_POINT_PT.y);
  await interactionLayer.click({ position: clickPt });
  const deleteDialog = page.getByRole("dialog", { name: "Delete annotation?" });
  await expect(deleteDialog).toBeVisible();
  await deleteDialog.getByRole("button", { name: "Delete" }).click();
  await expect(commentsPanel.locator(".oa-list-item")).toHaveCount(0);

  const undoButton = page.getByRole("button", { name: "Undo", exact: true });
  await expect(undoButton).toBeEnabled();
  await undoButton.click();
  await expect(commentsPanel.locator(".oa-list-item")).toHaveCount(1);

  const redoButton = page.getByRole("button", { name: "Redo", exact: true });
  await expect(redoButton).toBeEnabled();
  await redoButton.click();
  await expect(commentsPanel.locator(".oa-list-item")).toHaveCount(0);
  await expect(redoButton).toBeDisabled();

  // --- 6. Merge: the C1 repro — pick b.pdf as a source, then cancel the ----
  //        save-target dialog. Pre-fix, that leaked "b.pdf" forever in
  //        wasm.ts's pendingOpenPicks: the next real open of b.pdf got
  //        keyed "b.pdf (2)" (uniquePickKey only suffixes on a live
  //        collision), and worse, doc.file_path stayed the raw "b.pdf"
  //        (openFromFileHandle used to read fileHandle.name, not the pick
  //        key) — so `filePath !== doc.file_path`, and compareDocuments'
  //        pathA scan could never find the open document again. Both
  //        halves of the fix are exercised here: releasePicks (so the
  //        pick key never gets stolen) and the pick-key-as-display-name
  //        threading (so filePath and doc.file_path agree even if a
  //        suffix ever were needed for some other reason).
  await page.getByRole("button", { name: "Toggle pages panel" }).click();
  await queueOpenPick(page, ["b.pdf"]);
  await queueSavePick(page, null); // simulates the user cancelling Save As
  await page.getByRole("button", { name: /Merge PDFs/ }).click();
  const mergeDialog = page.getByRole("dialog", { name: "Merge PDFs" });
  await expect(mergeDialog).toBeVisible();
  await mergeDialog.getByRole("button", { name: "Cancel" }).click();
  // handleMerge's own picker calls (pickOpenPaths then pickSavePath) run
  // and resolve without any further dialog — the save picker itself was
  // stubbed to reject above, so the whole merge attempt quietly no-ops.
  // Nothing to await on the page for that beyond what the next step's
  // own picker call already serializes after.
  await expect(page.locator(".banner")).toHaveCount(0);

  // Now actually open b.pdf through the normal picker — this is the
  // moment that either shows "b.pdf" (fixed) or "b.pdf (2)" (the bug).
  await queueOpenPick(page, ["b.pdf"]);
  await openPdfButton.click();

  // Opening a document adds a tab rather than replacing the open one, so
  // there is no unsaved-changes prompt to answer — a.pdf is still open,
  // still dirty from the delete/undo/redo chain above, and nothing about
  // it was at risk. (This leg used to answer a "Save and continue"
  // dialog; the tab strip is what removed it. See `pickAndOpen`.)
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.nth(0)).toContainText("a.pdf");
  await expect(tabs.nth(1)).toContainText("b.pdf");
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  // The dot on a.pdf's tab: its unsaved edits survived the open, which is
  // the actual guarantee that made dropping the prompt safe.
  await expect(tabs.nth(0).locator(".tab__dirty")).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await expect(pathBarText).toHaveText("b.pdf");

  // --- 7. Compare b.pdf (now open) against a.pdf — proves compareDocuments's
  //        pathA scan finds the just-reopened document by its now-correct
  //        file_path, and the report dialog renders. ----------------------
  await queueOpenPick(page, ["a.pdf"]);
  await page.getByRole("button", { name: "Compare documents" }).click();
  const compareDialog = page.getByRole("dialog", { name: "Compare result" });
  await expect(compareDialog).toBeVisible({ timeout: 10_000 });
  await expect(compareDialog).toContainText("Comparing the open document against");
  await expect(page.locator(".banner")).toHaveCount(0);
  await compareDialog.getByRole("button", { name: "OK" }).click();

  // --- 8. Compress: confirm dialog -> picked target -> real full-rewrite
  //        bytes land in the fake filesystem, and the toast reports sizes.
  //        Exercises wasm.ts's compressDocument (saveToBytes +
  //        workingCopyBytes + pendingSavePicks) through the real UI. ------
  await queueSavePick(page, "compressed.pdf");
  await page.getByRole("button", { name: "Save a compressed copy" }).click();
  const compressDialog = page.getByRole("dialog", { name: "Save a compressed copy?" });
  await expect(compressDialog).toBeVisible();
  await compressDialog.getByRole("button", { name: "Choose where to save" }).click();
  await expect(page.getByText(/Compressed copy saved:/)).toBeVisible({ timeout: 10_000 });
  const compressed = await fakeFileHeader(page, "compressed.pdf");
  expect(compressed).not.toBeNull();
  expect(compressed!.header).toBe("%PDF-");
  expect(compressed!.length).toBeGreaterThan(0);
  await expect(page.locator(".banner")).toHaveCount(0);

  // --- 9. Print: the hand-off, which is as far as a test can follow -------
  //
  // No browser exposes what happens after the print dialog opens, so what
  // is checkable is everything up to it: that Print builds a print target
  // from the *working copy* (edits included), that the target holds real
  // PDF bytes, and that nothing errors. That covers the whole of
  // `printBytes` except the one call this side of the boundary can't
  // observe — see wasm.ts's `printDocument`.
  //
  // Chromium is what runs here, so this takes the embedded path
  // (`embeddedPrintIsReliable`) and the frame is the observable.
  await page.getByRole("button", { name: "Print" }).click();

  const printFrame = page.locator('iframe[src^="blob:"]');
  await expect(printFrame).toHaveCount(1, { timeout: 10_000 });

  // The bytes actually behind that blob, not merely the fact of a frame:
  // a frame pointing at an empty or non-PDF blob would print a blank
  // sheet, which is the failure this is here to catch.
  const printed = await page.evaluate(async () => {
    const frame = document.querySelector('iframe[src^="blob:"]') as HTMLIFrameElement;
    const bytes = new Uint8Array(await (await fetch(frame.src)).arrayBuffer());
    return { length: bytes.length, header: new TextDecoder().decode(bytes.slice(0, 5)) };
  });
  expect(printed.header).toBe("%PDF-");
  expect(printed.length).toBeGreaterThan(0);
  await expect(page.locator(".banner")).toHaveCount(0);

  await page.close();
});
