// OCR in the browser: recognition only.
//
// The desktop shells out to the tesseract binary. A browser cannot, so
// this runs Tesseract compiled to WebAssembly (tesseract.js) in a worker
// and hands the words it finds to exactly the same Rust code the desktop
// path ends in — `openpdfedit_ocr::add_text_layer`, reached through
// `WasmSession.addOcrTextLayer`. The two paths differ in what reads the
// pixels and in nothing else, which is why a page OCR'd here and the
// same page OCR'd on the desktop produce the same kind of text layer.
//
// **Everything is served from this origin.** tesseract.js defaults to
// pulling its core and its language data off a CDN at first use, which
// would make "your documents never leave your machine" depend on a third
// party being contacted the moment you use the feature, and would make
// OCR the one tool that stops working offline. `scripts/
// fetch-tesseract-assets.sh` and the web app's build put those files
// under /ocr/ instead, and the paths below pin them there.
//
// The import is dynamic on purpose: the engine and the trained data are
// ~3 MB gzipped between them, which nobody should pay for on page load
// to edit a PDF that isn't a scan.

/** Where the build puts the engine, the worker and the trained data. */
const OCR_ASSET_DIR = "/ocr";

/** What tesseract.js calls a word, narrowed to what is used here. */
interface RecognisedWord {
  text: string;
  confidence: number;
  bbox: { x0: number; y0: number; x1: number; y1: number };
}

/** One word, in the shape `AddOcrTextLayerRequest` expects: pixel space,
 * top-left origin, width/height rather than a second corner. */
export interface OcrWordDto {
  text: string;
  left: number;
  top: number;
  width: number;
  height: number;
  confidence: number;
}

export interface OcrProgress {
  /** 0-based page currently being read. */
  pageIndex: number;
  pageCount: number;
  /** True while the engine and language data are still downloading —
   * the slow part of a first run, and worth saying so rather than
   * showing a progress bar that appears stuck. */
  loading: boolean;
}

/** Reused across pages and across runs: starting a worker means
 * instantiating the wasm engine and re-reading ~4 MB of trained data,
 * which is most of the cost of OCR'ing a short document. */
let workerPromise: Promise<TesseractWorker> | null = null;

interface TesseractWorker {
  recognize(image: unknown): Promise<{ data: { words?: RecognisedWord[] } }>;
  terminate(): Promise<unknown>;
}

async function getWorker(onLoading?: () => void): Promise<TesseractWorker> {
  if (!workerPromise) {
    onLoading?.();
    workerPromise = (async () => {
      const { createWorker } = await import("tesseract.js");
      return (await createWorker("eng", 1, {
        // Pinned to this origin — see the module doc. A missing file
        // here fails loudly rather than silently reaching a CDN.
        workerPath: `${OCR_ASSET_DIR}/worker.min.js`,
        corePath: `${OCR_ASSET_DIR}/`,
        langPath: OCR_ASSET_DIR,
        // The data is served plain, not gzipped-with-a-.gz-name.
        gzip: false,
      })) as unknown as TesseractWorker;
    })().catch((error) => {
      // Don't cache a failed start: a network blip on first use would
      // otherwise disable OCR for the rest of the session.
      workerPromise = null;
      throw error;
    });
  }
  return workerPromise;
}

/**
 * Recognise one page bitmap.
 *
 * `bitmap` is RGBA pixels as the renderer produced them. tesseract.js
 * accepts a canvas, so the conversion is a draw rather than an encode —
 * PNG-encoding each page first would cost more than the recognition on
 * short documents.
 */
export async function recognisePage(
  bitmap: { width: number; height: number; data: Uint8Array | Uint8ClampedArray },
  onLoading?: () => void,
): Promise<OcrWordDto[]> {
  const worker = await getWorker(onLoading);

  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("could not get a 2d context to hand the page to OCR");
  context.putImageData(
    new ImageData(new Uint8ClampedArray(bitmap.data), bitmap.width, bitmap.height),
    0,
    0,
  );

  const { data } = await worker.recognize(canvas);
  return (data.words ?? []).map((word) => ({
    text: word.text,
    left: word.bbox.x0,
    top: word.bbox.y0,
    width: word.bbox.x1 - word.bbox.x0,
    height: word.bbox.y1 - word.bbox.y0,
    confidence: word.confidence,
  }));
}

/** Release the engine. Worth doing when a document closes: the worker
 * holds the wasm heap and the trained data for as long as it lives. */
export async function releaseOcr(): Promise<void> {
  const pending = workerPromise;
  workerPromise = null;
  if (!pending) return;
  try {
    await (await pending).terminate();
  } catch {
    /* already gone, or never started */
  }
}
