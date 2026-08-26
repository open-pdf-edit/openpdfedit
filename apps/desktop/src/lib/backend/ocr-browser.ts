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
// to edit a PDF that isn't a scan. The same is true of the language
// data: several languages are served, and a run fetches only the ones it
// was asked for.
//
// **Language is not a detail here.** Tesseract recognises the script it
// was given data for and nothing else, and it does not fail when handed
// something else — it returns confident nonsense, or nothing at all.
// This shipped English-only, silently, which meant OCR on a Chinese
// document appeared to run and produced an empty text layer.

/** Where the build puts the engine, the worker and the trained data. */
const OCR_ASSET_DIR = "/ocr";

/** What tesseract.js calls a word, narrowed to what is used here. */
interface RecognisedWord {
  text: string;
  confidence: number;
  bbox: { x0: number; y0: number; x1: number; y1: number };
  /** One per character, when the engine reports them. This is the whole
   * reason the layout tree is asked for rather than the plain text: a
   * word box alone leaves the characters inside it to be spaced by
   * arithmetic, and a search highlight drawn from that arithmetic sits
   * beside the word it found rather than on it. */
  symbols?: { text: string; bbox: { x0: number; y0: number; x1: number; y1: number } }[];
}

/** Where the words actually are.
 *
 * tesseract.js used to put them on the result as `data.words`. It does
 * not any more: they live three levels down, under blocks → paragraphs →
 * lines, and the whole tree is `null` unless `blocks` is asked for.
 *
 * Reading `data.words` therefore found `undefined`, and this code turned
 * that into an empty list — so OCR ran, took its time, reported success
 * and added nothing, in every language. That is the failure this shape
 * exists to prevent: it is why `recognisePage` throws when the tree is
 * missing rather than treating it as "no text found". */
interface RecognisedPage {
  blocks: { paragraphs: { lines: { words: RecognisedWord[] }[] }[] }[] | null;
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
  /** Each character's own horizontal extent, in the same pixel space.
   * Empty when the engine did not say. */
  chars: { text: string; left: number; width: number }[];
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

/** Which language the cached worker was built for. A worker is bound to
 * its trained data at creation, so asking it for a different language
 * would quietly keep recognising in the old one. */
let workerLang: string | null = null;

/** Tesseract's own default, and the only language whose data was
 * shipped before this was configurable. */
export const DEFAULT_OCR_LANG = "eng";

interface TesseractWorker {
  recognize(
    image: unknown,
    options?: Record<string, unknown>,
    output?: Record<string, boolean>,
  ): Promise<{ data: RecognisedPage }>;
  terminate(): Promise<unknown>;
}

async function getWorker(lang: string, onLoading?: () => void): Promise<TesseractWorker> {
  if (workerPromise && workerLang !== lang) {
    // Language changed. The old worker holds the wrong trained data and
    // several megabytes of heap, so it goes rather than lingering.
    void releaseOcr();
  }
  if (!workerPromise) {
    onLoading?.();
    workerLang = lang;
    workerPromise = (async () => {
      const { createWorker } = await import("tesseract.js");
      // `chi_sim+eng` and friends: tesseract loads several sets of
      // trained data and recognises a page that mixes them, which is
      // what a Chinese document with Latin numerals and headings needs.
      const langs = lang.split("+").filter(Boolean);
      return (await createWorker(langs, 1, {
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
      workerLang = null;
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
  lang: string = DEFAULT_OCR_LANG,
  onLoading?: () => void,
): Promise<OcrWordDto[]> {
  const worker = await getWorker(lang, onLoading);

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

  // `blocks: true` is what makes the word tree exist at all — the
  // default output is the plain text of the page and nothing else, which
  // has no positions and so cannot be turned into a text layer.
  const { data } = await worker.recognize(canvas, {}, { blocks: true });

  if (!data.blocks) {
    throw new Error(
      "the recogniser returned no layout for this page — OCR cannot place text without it",
    );
  }

  const words: OcrWordDto[] = [];
  for (const block of data.blocks) {
    for (const paragraph of block.paragraphs ?? []) {
      for (const line of paragraph.lines ?? []) {
        for (const word of line.words ?? []) {
          // Empty and whitespace-only "words" are common on noisy scans
          // and would each become a text-showing operator positioning
          // nothing.
          if (!word.text.trim()) continue;
          words.push({
            text: word.text,
            left: word.bbox.x0,
            top: word.bbox.y0,
            width: word.bbox.x1 - word.bbox.x0,
            height: word.bbox.y1 - word.bbox.y0,
            confidence: word.confidence,
            chars: (word.symbols ?? []).map((symbol) => ({
              text: symbol.text,
              left: symbol.bbox.x0,
              width: symbol.bbox.x1 - symbol.bbox.x0,
            })),
          });
        }
      }
    }
  }
  return words;
}

/** Release the engine. Worth doing when a document closes: the worker
 * holds the wasm heap and the trained data for as long as it lives. */
export async function releaseOcr(): Promise<void> {
  const pending = workerPromise;
  workerPromise = null;
  workerLang = null;
  if (!pending) return;
  try {
    await (await pending).terminate();
  } catch {
    /* already gone, or never started */
  }
}
