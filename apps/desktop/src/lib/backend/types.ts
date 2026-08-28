// The seam between the UI and whatever runs it: a Tauri desktop window
// today (`tauri.ts`), a Chrome extension WASM sandbox later (`wasm.ts`,
// Task 8). Every DTO here mirrors a Rust serde shape one-for-one — field
// names/casing are NOT "improved" or normalized even where they look
// inconsistent (e.g. `OpenedDocument` stays snake_case while most
// request/response DTOs are camelCase) — that's exactly what the Rust
// side emits today, and changing it would break wire compatibility with
// `tauri.ts` for zero benefit. See index.ts for how `backend` is chosen.

// --- Shared value types -----------------------------------------------

export interface PageSize {
  width: number;
  height: number;
}

/** Returned by every open/save/undo/redo/edit command — see lib.rs's
 * `OpenedDocument` struct. Field names are snake_case because the Rust
 * struct has no `#[serde(rename_all = ...)]` attribute. */
export interface OpenedDocument {
  handle: number;
  page_count: number;
  page_sizes: PageSize[];
  can_undo: boolean;
  can_redo: boolean;
  is_dirty: boolean;
  file_path: string;
}

export interface AnnotationSummaryDto {
  id: [number, number];
  subtype: string;
  rect: [number, number, number, number];
  contents: string | null;
}

// `isVerified` is always false in this build — see openpdfedit-sign's
// module doc: no cryptographic signature verification is implemented,
// only structural inspection (what the PDF declares about a signature,
// not whether it's genuine or trusted). Never render this as "signed and
// valid."
export interface ExportXfdfResult {
  exported: number;
}

export interface ImportXfdfResult {
  document: OpenedDocument;
  imported: number;
  /** Annotation kinds this app can't draw — counted rather than
   * approximated with something the sender didn't draw. */
  skipped: number;
  /** Annotations addressed to pages this document doesn't have. */
  outOfRange: number;
}

/** Everything the numbering panel decides. `handle` is added by the
 * backend, matching how `WatermarkChoices` is shaped. */
export interface NumberPagesChoices {
  prefix: string;
  suffix: string;
  startAt: number;
  /** Zero-pad the number to this width; 0 leaves it unpadded. */
  digits: number;
  anchor: string;
  font: string;
  fontSize: number;
  /** `[r, g, b]`, each 0..=1. */
  color: [number, number, number];
  opacity: number;
  margin: number;
  /** `null` numbers every page. 0-based page indices otherwise. */
  pages: number[] | null;
}

/** Everything the encryption dialog decides. */
export interface EncryptChoices {
  /** What a reader is prompted for. Required. */
  userPassword: string;
  /** Full-permission password; empty reuses `userPassword`. */
  ownerPassword: string;
  allowPrint: boolean;
  allowModify: boolean;
  allowCopy: boolean;
  allowAnnotate: boolean;
}

export interface EncryptStats {
  bytes: number;
}

export interface FlattenDocumentRequest {
  handle: number;
  annotations: boolean;
  formFields: boolean;
}

export interface FlattenResultDto {
  document: OpenedDocument;
  flattened: number;
  /** Left interactive — a link, or markup with no appearance to draw. */
  skipped: number;
  popupsRemoved: number;
}

import type { RecentDocument } from "$lib/recents";
export type { RecentDocument };

export interface RemoveMarkupRequest {
  handle: number;
}

export interface RemoveMarkupResultDto {
  document: OpenedDocument;
  /** Annotations deleted — highlights, notes, ink, stamps. */
  annotations: number;
  /** Pen-and-highlighter layers already flattened into the page,
   * dropped from its content. Usually one per marked-up page. */
  layers: number;
}

/** One row of the document's outline. Mirrors `OutlineEntryDto` in
 * `crates/openpdfedit-session/src/outline.rs`. */
export interface OutlineEntryDto {
  title: string;
  /** `null` for an entry whose destination isn't a page in this document
   * — still listed, just not clickable. */
  pageIndex: number | null;
  depth: number;
  hasChildren: boolean;
}

export interface SearchRequest {
  handle: number;
  query: string;
  matchCase: boolean;
  wholeWord: boolean;
}

/** One occurrence of the current search query. Mirrors `SearchHitDto` in
 * `crates/openpdfedit-session/src/search.rs`. */
export interface SearchHitDto {
  pageIndex: number;
  /** Inclusive character range of the match on its page, in the same
   * index space the annotation commands use. */
  charStart: number;
  charEnd: number;
  /** One `[x0, y0, x1, y1]` per visual line the match spans, in PDF
   * page-space points (origin bottom-left). */
  quads: [number, number, number, number][];
  contextBefore: string;
  contextMatch: string;
  contextAfter: string;
}

export interface SearchResultsDto {
  hits: SearchHitDto[];
  /** True when the backend stopped at its hit cap and the document holds
   * more matches than are listed. */
  truncated: boolean;
}

export interface SignatureInfoDto {
  subFilter: string | null;
  reason: string | null;
  name: string | null;
  signingTime: string | null;
  byteRangeIsStructurallySound: boolean;
  isVerified: boolean;
}

export interface TextRunDto {
  index: number;
  text: string;
  rect: [number, number, number, number];
  fontSize: number;
  isEditable: boolean;
}

export interface ImagePlacementDto {
  index: number;
  rect: [number, number, number, number];
}

export interface TextPageDiffDto {
  pageIndex: number;
  added: string[];
  removed: string[];
}

export interface PixelPageDiffDto {
  pageIndex: number;
  differingPixels: number;
  totalPixels: number;
  bbox: [number, number, number, number] | null;
}

export interface CompareReportDto {
  pageCountA: number;
  pageCountB: number;
  textPages: TextPageDiffDto[];
  pixelPages: PixelPageDiffDto[];
}

export interface FormFieldOptionDto {
  label: string | null;
  isSelected: boolean;
}

export type FormFieldKindDto = "text" | "checkbox" | "radioButton" | "comboBox" | "listBox" | "pushButton" | "signature" | "unknown";

export interface FormFieldDto {
  pageIndex: number;
  name: string;
  kind: FormFieldKindDto;
  /** `[x0, y0, x1, y1]` on the page, in PDF points (origin bottom-left). */
  rect: [number, number, number, number];
  value: string | null;
  isChecked: boolean | null;
  isReadOnly: boolean;
  options: FormFieldOptionDto[];
}

/** The wire shape of `add_annotation_cmd`'s `annotation` field — also
 * embedded (via PdfPage.svelte's `AnnotationPayload`) in the drag-gesture
 * payload the UI builds before it has a backend request to send. */
export type AnnotationKindDto =
  | { kind: "highlight" | "underline" | "strikeOut"; quads: [number, number, number, number][] }
  | { kind: "freeText"; text: string; fontSize: number }
  | { kind: "ink"; strokes: [number, number][][] }
  /** A rectangle outline filling the annotation's rect. `fill` omitted
   * draws an outline only; `lineWidth: 0` draws a hairline. */
  | { kind: "square"; lineWidth: number; fill?: [number, number, number] }
  /** An ellipse inscribed in the annotation's rect. */
  | { kind: "circle"; lineWidth: number; fill?: [number, number, number] };

export type PageMoveDirection = "Up" | "Down";

/** Raw RGBA page bitmap — the shared wire format both backends produce.
 * `tauri.ts` gets it from a `tile://` fetch; the wasm backend (Task 8)
 * will get it from a direct in-process render call. */
export interface PageBitmap {
  width: number;
  height: number;
  rgba: Uint8ClampedArray;
}

// --- Request DTOs (one per command whose args don't fit as plain
// positional parameters on the Backend method) --------------------------

export interface AddAnnotationRequest {
  handle: number;
  pageIndex: number;
  rect: [number, number, number, number];
  color: [number, number, number];
  opacity: number;
  contents: string | null;
  annotation: AnnotationKindDto;
}

export interface DeleteAnnotationRequest {
  handle: number;
  pageIndex: number;
  annotationId: [number, number];
}

export interface TextSelectionQuadsRequest {
  handle: number;
  pageIndex: number;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** What a drag over the page selected. */
export interface TextSelection {
  /** One bounding quad per visual line the selection spans, in PDF
   * page-space points. */
  quads: [number, number, number, number][];
  /** The selected characters, in reading order. */
  text: string;
}

export interface ExportMarkdownRequest {
  handle: number;
  /** What to call the file, without a directory: "report.md". */
  fileName: string;
  /** Where it should go, when the user has chosen somewhere — a
   * directory path on the desktop, or the key of a directory the
   * browser has granted access to. Absent means "ask, or download". */
  vault?: string | null;
}

export interface ExportMarkdownResult {
  /** Where it ended up, for saying so afterwards. Null when the file was
   * handed to the browser's downloads instead of written somewhere
   * nameable. */
  path: string | null;
  /** How much text there was, so an empty result can be explained
   * rather than looking like a failure. A scan that has not been OCR'd
   * has no text to convert. */
  characters: number;
}

export interface EditTextRunRequest {
  handle: number;
  pageIndex: number;
  runIndex: number;
  newText: string;
}

export interface MoveTextRunRequest {
  handle: number;
  pageIndex: number;
  runIndex: number;
  dx: number;
  dy: number;
}

export interface MoveImageRequest {
  handle: number;
  pageIndex: number;
  placementIndex: number;
  dx: number;
  dy: number;
}

export interface CreateFormFieldRequest {
  handle: number;
  pageIndex: number;
  rect: [number, number, number, number];
  kind: "text" | "checkbox";
  name: string;
}

export interface FillFormFieldsRequest {
  handle: number;
  values: Record<string, string>;
}

export interface RedactPageRequest {
  handle: number;
  pageIndex: number;
  rect: [number, number, number, number];
}

/** Tiled text/logo watermark options — the vocabulary of OpenCapture's
 * watermark tool, applied per PDF page (see `openpdfedit-watermark`'s
 * module doc). The optional logo travels as base64-encoded raw RGBA with
 * explicit dimensions (the UI decodes the picked image via a canvas);
 * all three logo fields must be present together. */
export interface ApplyWatermarkRequest {
  handle: number;
  /** May be empty when a logo is supplied. */
  text: string;
  location: "top" | "bottom" | "top-bottom" | "full";
  orientationDeg: 0 | 45;
  /** 0..=1, rides an ExtGState so it hits fills and strokes alike. */
  opacity: number;
  /** Multiplier on the automatic font size. */
  textScale: number;
  /** How many tiles fit across a page relative to the stock pattern:
   * 1.0 is that pattern, lower is sparser. */
  density: number;
  logoRgbaBase64?: string;
  logoWidth?: number;
  logoHeight?: number;
  /** 0-based page indexes; omitted = every page. */
  pages?: number[];
}

/** What the watermark dialog collects — everything in
 * `ApplyWatermarkRequest` except the handle, which the page supplies. */
export type WatermarkChoices = Omit<ApplyWatermarkRequest, "handle">;

export interface ExtractPagesRequest {
  handle: number;
  pageIndices: number[];
  outputPath: string;
}

/** Compress-copy export: writes a full-rewrite (PDFium FPDF_SaveAsCopy)
 * copy of the document's CURRENT state to `outputPath` — dropping the
 * incremental revision chain and orphaned objects, which is where the
 * size wins come from on an edited file. Export, not mutation: the open
 * document is untouched. Existing digital signatures do not carry over
 * into the compressed copy (full rewrite) — the UI says so before
 * running. `outputPath` follows extractPages' convention: a real path on
 * tauri, a pickSavePath() key on wasm. */
export interface CompressDocumentRequest {
  handle: number;
  outputPath: string;
}

export interface CompressStats {
  beforeBytes: number;
  afterBytes: number;
}

export interface MergeDocumentsRequest {
  openHandle: number | null;
  sourcePaths: string[];
  outputPath: string;
}

/** compare_documents_cmd takes two file PATHS, not open-document handles
 * — the desktop UI collects them via a picker (see tauri.ts's
 * `pickOpenPath`), unlike every other request DTO here which addresses an
 * already-open document by handle. */
export interface CompareDocumentsRequest {
  pathA: string;
  pathB: string;
  pixelTargetWidth: number;
}

export interface OcrDocumentRequest {
  handle: number;
  /** Tesseract language codes, `+`-joined for a page that mixes scripts
   * (`"chi_sim+eng"`). Omitted means English.
   *
   * Not cosmetic: Tesseract reads the script it has trained data for and
   * responds to anything else with silence or nonsense rather than an
   * error, so the wrong value here looks exactly like a feature that
   * does not work. The desktop passes this to a local `tesseract`, which
   * needs the matching language pack installed; the browser fetches it
   * from this origin. */
  lang?: string;
}

// --- The Backend interface ---------------------------------------------

/** Everything the UI needs from whatever's running it. One method per
 * `#[tauri::command]` the frontend calls today (see grep receipt in the
 * Task 7 brief), camelCased, plus a handful of higher-level conveniences
 * (`pickAndOpenDocument`, `saveDocumentAs`, the picker primitives, the
 * close-window handshake, `getPageBitmap`) that bundle a native picker or
 * a non-IPC data path (the `tile://` fetch) with its matching command —
 * those are exactly the touchpoints that must be re-implemented, not just
 * re-wired, for the wasm/extension backend in Task 8. */
export interface Backend {
  // --- document lifecycle ---
  /** Opens `path`. A password-protected document rejects a call with no
   * `password` by throwing something `isPasswordRequired` recognises, so
   * the caller can prompt and retry rather than surfacing a parser
   * message about something the user already knows. */
  openDocument(path: string, password?: string): Promise<OpenedDocument>;
  /** Open picker + open document in one step — path-based on desktop,
   * bytes-based in the extension. Desktop's own call site uses the
   * finer-grained `pickOpenPath` + `openDocument` pair instead (see
   * +page.svelte's `pickAndOpen`), so it can keep showing the attempted
   * path in the UI even when `openDocument` rejects — this method exists
   * on the interface for the wasm backend and any future caller that
   * doesn't need that. */
  pickAndOpenDocument(): Promise<OpenedDocument | null>;

  /** The documents opened most recently, newest first, for the landing
   * screen. Only ones this backend can actually reopen — see
   * `$lib/recents` for why the list is empty in Firefox and Safari
   * rather than full of rows that do nothing. */
  recentDocuments(): Promise<RecentDocument[]>;

  /** Reopens one. Resolves to `null` when the file has moved, been
   * deleted, or the browser refused permission to read it again — all
   * ordinary outcomes for a list of files last seen days ago, none of
   * them an error worth an alert. The entry is dropped when that
   * happens, so a dead row does not sit there forever. */
  openRecent(id: string): Promise<OpenedDocument | null>;

  /** Forgets one entry, or all of them. */
  forgetRecent(id: string): Promise<void>;
  clearRecents(): Promise<void>;
  /** True when this backend can only save by handing the browser a
   * download, rather than writing back over the file that was opened.
   * The desktop always writes back; a browser can only do so where the
   * File System Access API exists (Chromium today). The UI uses this to
   * label the action honestly. */
  savesByDownloading(handle: number): boolean;
  /** Whether this backend can print at all, checked once to decide
   * whether the Print control exists. False on the desktop today: the
   * webview it runs in has never implemented `window.print()`, so
   * printing there needs a native path (CUPS on macOS/Linux, the shell
   * verb on Windows) that this build doesn't ship yet. A browser needs
   * no such thing, which is the whole reason print is a web-first
   * feature here rather than a desktop-first one. */
  canPrint(): boolean;
  /** Sends the document as it stands — edits included, not the file on
   * disk — to the browser's print path. Resolves once the print dialog
   * has been handed off, not once anything has been printed: no browser
   * reports back what the user did with the dialog, so there is nothing
   * truthful to resolve *to*. Throws if `canPrint()` is false. */
  printDocument(handle: number): Promise<void>;

  saveDocument(handle: number): Promise<OpenedDocument>;
  /** Writes the working copy to an already-chosen `path` — the raw
   * command, no picker. Desktop's own call site uses this directly (via
   * `pickSavePath` first) rather than the `saveDocumentAs` convenience
   * below, for the same reason `pickAndOpen` bypasses
   * `pickAndOpenDocument`: it needs to run the picker *before* clearing
   * `error`/setting `saveBusy`, so canceling the dialog leaves any
   * existing error banner alone — the combo method can't expose that
   * ordering to its caller. */
  saveDocumentAtPath(handle: number, path: string): Promise<OpenedDocument>;
  /** Picker inside. `defaultPath` seeds the save dialog's suggested
   * filename (today: the document's current `file_path`); omit it for no
   * suggestion. Resolves to `null` if the user cancels the picker. Exists
   * for the wasm backend and any future caller that doesn't need the
   * finer-grained `pickSavePath` + `saveDocumentAtPath` ordering
   * `handleSaveAs` relies on (see that method's doc). */
  saveDocumentAs(handle: number, defaultPath?: string | null): Promise<OpenedDocument | null>;
  /** Releases a document's backend-side state (engine handle + session
   * bookkeeping) without touching any file on disk — the counterpart to
   * `open_document`/`openDocument`, not to closing the whole window. Added
   * in Phase 2 Task 2 so a document can be retired when the UI replaces it
   * with a newly-opened one (see +page.svelte's `pickAndOpen`, the sole
   * caller); best-effort on both backends — a failure here is logged, not
   * surfaced, since by the time this runs the *new* document has already
   * opened successfully. */
  closeDocument(handle: number): Promise<void>;
  /** Tells the backend the close-confirmation flow is done and the window
   * may actually close. */
  confirmClose(): Promise<void>;
  /** Registers the "the window/tab wants to close, decide first" hook;
   * resolves to an unsubscribe function. */
  onCloseRequested(cb: () => void): Promise<() => void>;
  undo(handle: number): Promise<OpenedDocument>;
  redo(handle: number): Promise<OpenedDocument>;

  // --- rendering ---
  /** `signal` lets a caller (PdfPage.svelte's IntersectionObserver-driven
   * paint effect) cancel an in-flight fetch when a page scrolls back out
   * of view before it finishes loading. */
  getPageBitmap(handle: number, pageIndex: number, targetWidth: number, signal?: AbortSignal): Promise<PageBitmap>;

  // --- annotations ---
  listPageAnnotations(handle: number, pageIndex: number): Promise<AnnotationSummaryDto[]>;
  addAnnotation(request: AddAnnotationRequest): Promise<OpenedDocument>;
  deleteAnnotation(request: DeleteAnnotationRequest): Promise<OpenedDocument>;
  textSelectionQuads(request: TextSelectionQuadsRequest): Promise<[number, number, number, number][]>;

  /** The Select tool's drag: where the selection is *and* what it says.
   *
   * Separate from `textSelectionQuads`, which the markup tools use to
   * snap a highlight onto real words and has no use for the characters.
   * Both come from one character range on the backend, so they cannot
   * disagree about what was selected. */
  selectText(request: TextSelectionQuadsRequest): Promise<TextSelection>;

  /** The open document converted to Markdown, and where it was put.
   *
   * The two backends differ in what "put" can mean, which is why this
   * takes a target rather than returning a string. The desktop writes to
   * a path; a browser has no paths, so it writes into a directory the
   * user picked (Chromium) or hands over a download (everywhere else).
   * See `Backend.savesByDownloading` for the same split on saving. */
  exportMarkdown(request: ExportMarkdownRequest): Promise<ExportMarkdownResult>;

  /** The same, as plain text. Not the Markdown renamed: Markdown ends
   * every line with two spaces to force a hard break, which is
   * invisible trailing whitespace in a `.txt`. See
   * `openpdfedit_session::markdown`. */
  exportText(request: ExportMarkdownRequest): Promise<ExportMarkdownResult>;

  /** Whether this backend can write into a folder the user names — an
   * Obsidian vault, most usefully. True on the desktop; in a browser,
   * true only where the File System Access API exists (Chromium), since
   * everywhere else a file can only be handed to the downloads folder. */
  supportsVault(): boolean;

  /** Ask the user for that folder. Returns something to pass back as
   * `ExportMarkdownRequest.vault`, plus a name to show them. Null if
   * they cancelled. Must be called from a click: a browser will not open
   * a directory picker otherwise. */
  pickVault(): Promise<{ key: string; name: string } | null>;

  // --- text/image editing ---
  listTextRuns(handle: number, pageIndex: number): Promise<TextRunDto[]>;
  editTextRun(request: EditTextRunRequest): Promise<OpenedDocument>;
  moveTextRun(request: MoveTextRunRequest): Promise<OpenedDocument>;
  listImagePlacements(handle: number, pageIndex: number): Promise<ImagePlacementDto[]>;
  moveImage(request: MoveImageRequest): Promise<OpenedDocument>;

  // --- forms ---
  listFormFields(handle: number): Promise<FormFieldDto[]>;
  fillFormFields(request: FillFormFieldsRequest): Promise<OpenedDocument>;
  createFormField(request: CreateFormFieldRequest): Promise<OpenedDocument>;

  // --- signatures ---
  listSignatures(handle: number): Promise<SignatureInfoDto[]>;

  /** Finds every occurrence of `query` across the whole document.
   * Read-only — no handle rotation. */
  searchDocument(request: SearchRequest): Promise<SearchResultsDto>;

  /** The document's outline (bookmarks), flattened depth-first with a
   * `depth` tag per entry. Read-only — no handle rotation. */
  documentOutline(handle: number): Promise<OutlineEntryDto[]>;

  /** Bakes markup (and optionally filled form values) into the page.
   * Mutating — rotates the handle, and is undoable. */
  flattenDocument(request: FlattenDocumentRequest): Promise<FlattenResultDto>;

  /** Takes markup off the document: annotations, and pen layers already
   * flattened into the page. Mutating — rotates the handle, and is
   * undoable. */
  removeMarkup(request: RemoveMarkupRequest): Promise<RemoveMarkupResultDto>;

  /** Writes a password-protected copy, picking a destination the way
   * this backend does. Resolves to `null` if the user cancelled.
   * Export — the open document is untouched. */
  encryptDocument(handle: number, choices: EncryptChoices): Promise<EncryptStats | null>;

  /** Stamps page numbers or Bates numbering into a margin of each page.
   * Mutating — rotates the handle, and is undoable. */
  numberPages(handle: number, choices: NumberPagesChoices): Promise<OpenedDocument>;

  /** Writes the document's markup out as an XFDF file, picking a
   * destination the way this backend does. Resolves to `null` if the
   * user cancelled. Read-only. */
  exportXfdf(handle: number): Promise<ExportXfdfResult | null>;

  /** Reads an XFDF file the user picks and adds every annotation this
   * app can draw. Resolves to `null` if the user cancelled. Mutating —
   * rotates the handle, and is undoable. */
  importXfdf(handle: number): Promise<ImportXfdfResult | null>;

  // --- pages ---
  rotatePage(handle: number, pageIndex: number, deltaDegrees: number): Promise<OpenedDocument>;
  deletePage(handle: number, pageIndex: number): Promise<OpenedDocument>;
  movePage(handle: number, pageIndex: number, direction: PageMoveDirection): Promise<OpenedDocument>;
  setCropBox(handle: number, pageIndex: number, rect: [number, number, number, number]): Promise<OpenedDocument>;
  extractPages(request: ExtractPagesRequest): Promise<OpenedDocument>;
  mergeDocuments(request: MergeDocumentsRequest): Promise<OpenedDocument>;
  redactPage(request: RedactPageRequest): Promise<OpenedDocument>;
  applyWatermark(request: ApplyWatermarkRequest): Promise<OpenedDocument>;

  // --- document-level tools ---
  compressDocument(request: CompressDocumentRequest): Promise<CompressStats>;
  compareDocuments(request: CompareDocumentsRequest): Promise<CompareReportDto>;
  ocrDocument(request: OcrDocumentRequest): Promise<OpenedDocument>;

  // --- file-picker primitives ---
  // Back every remaining plugin-dialog `open()`/`save()` call in the
  // desktop UI (merge's source files + output, extract's output, compare's
  // second document) that isn't already folded into `pickAndOpenDocument`
  // or `saveDocumentAs` above. Every call site in this app filters to PDF
  // only, so that filter is baked into the Tauri implementation rather
  // than threaded through every call site.
  pickOpenPath(): Promise<string | null>;
  pickOpenPaths(): Promise<string[]>;
  pickSavePath(defaultPath?: string): Promise<string | null>;

  /** Releases picks a `pickOpenPath`/`pickOpenPaths` call returned but
   * that never got consumed by `openDocument`/`mergeDocuments`/etc. — call
   * this on any early-return/cancel path *after* a successful pick whose
   * result is being abandoned, e.g. `handleMerge` picking merge sources
   * and then the user canceling the save-target dialog. Added in the C1
   * fix round: on the wasm backend, an abandoned pick otherwise sits
   * forever in `pendingOpenPicks` (see wasm.ts's "Open-document
   * bookkeeping" doc), which does not itself corrupt anything but does
   * permanently consume that filename's un-suffixed key — a later
   * legitimate pick of the same filename then gets a `" (2)"`-suffixed
   * key it wouldn't otherwise need. No-op on the desktop backend: paths
   * there are real filesystem paths, not synthetic keys, so there is no
   * pending-pick bookkeeping to release. */
  releasePicks(paths: string[]): Promise<void>;
}
