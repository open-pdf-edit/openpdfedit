// The Tauri desktop implementation of `Backend` — every method here is
// today's exact `invoke("...")` call (same command name, same argument
// keys) or `tile://` fetch, just relocated out of the components/route
// that used to make it directly. See types.ts's `Backend` doc for the
// overall shape and PLAN.md for the wasm/extension counterpart (Task 8).

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { TILE_ORIGIN } from "./tileOrigin";
import { clearRecents, forgetRecent, listRecents, rememberRecent } from "$lib/recents";
import type {
  AddAnnotationRequest,
  AnnotationSummaryDto,
  Backend,
  CompareDocumentsRequest,
  CompareReportDto,
  CreateFormFieldRequest,
  DeleteAnnotationRequest,
  EditTextRunRequest,
  ExtractPagesRequest,
  FillFormFieldsRequest,
  FormFieldDto,
  ImagePlacementDto,
  MergeDocumentsRequest,
  MoveImageRequest,
  MoveTextRunRequest,
  OcrDocumentRequest,
  OpenedDocument,
  PageBitmap,
  PageMoveDirection,
  RedactPageRequest,
  ApplyWatermarkRequest,
  CompressDocumentRequest,
  CompressStats,
  EncryptStats,
  ExportXfdfResult,
  FlattenResultDto,
  RemoveMarkupResultDto,
  ImportXfdfResult,
  OutlineEntryDto,
  SearchResultsDto,
  SignatureInfoDto,
  TextRunDto,
  ExportMarkdownResult,
  TextSelection,
  TextSelectionQuadsRequest,
} from "./types";

// Every file picker in this app filters to PDF only.
const PDF_FILTERS = [{ name: "PDF", extensions: ["pdf"] }];

/** The last path segment, for display. Both separators, because a
 * Windows path arrives with backslashes and this runs in the same
 * webview either way. */
function basename(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut >= 0 ? path.slice(cut + 1) : path;
}

export const tauriBackend: Backend = {
  // --- document lifecycle ---

  async openDocument(path, password) {
    const opened = await invoke<OpenedDocument>("open_document", {
      path,
      password: password ?? null,
    });
    // Recorded here rather than at the call sites: a document can be
    // reached through the picker, a recent, a drop, or a command-line
    // argument, and every one of those should show up in the list.
    rememberRecent(path, basename(path), Date.now());
    return opened;
  },

  async pickAndOpenDocument() {
    const path = await tauriBackend.pickOpenPath();
    if (!path) return null;
    return tauriBackend.openDocument(path);
  },

  /** Always false: the desktop writes over the file that was opened. */
  savesByDownloading() {
    return false;
  },

  /** Always false today — see `Backend.canPrint`. Printing from the
   * desktop needs a native path, because the webview has no
   * `window.print()`; when that lands this flips to true and the UI
   * picks the control up with no change of its own. */
  canPrint() {
    return false;
  },

  async printDocument() {
    throw new Error("printDocument: the desktop build has no print path yet — see Backend.canPrint");
  },

  async saveDocument(handle) {
    return invoke<OpenedDocument>("save_document", { handle });
  },

  async saveDocumentAtPath(handle, path) {
    return invoke<OpenedDocument>("save_document_as", { handle, path });
  },

  async saveDocumentAs(handle, defaultPath) {
    const target = await tauriBackend.pickSavePath(defaultPath ?? undefined);
    if (!target) return null;
    return tauriBackend.saveDocumentAtPath(handle, target);
  },

  async closeDocument(handle) {
    await invoke("close_document", { handle });
  },

  async confirmClose() {
    await invoke("close_window");
  },

  async onCloseRequested(cb) {
    return listen("close-requested", cb);
  },

  async undo(handle) {
    return invoke<OpenedDocument>("undo_cmd", { handle });
  },

  async redo(handle) {
    return invoke<OpenedDocument>("redo_cmd", { handle });
  },

  // --- rendering ---

  async getPageBitmap(handle, pageIndex, targetWidth, signal) {
    const tileUrl = `${TILE_ORIGIN}/${handle}/${pageIndex}/${targetWidth}`;
    const res = await fetch(tileUrl, { signal });
    if (!res.ok) {
      const bodyText = await res.text().catch(() => "");
      console.error(`tile fetch returned ${res.status} for ${tileUrl}: ${bodyText}`);
      throw new Error(`tile request failed (${res.status}): ${bodyText || res.statusText}`);
    }

    const width = Number(res.headers.get("X-Tile-Width"));
    const height = Number(res.headers.get("X-Tile-Height"));
    const rgba = new Uint8ClampedArray(await res.arrayBuffer());
    if (rgba.length !== width * height * 4) {
      throw new Error(`tile response had unexpected size (got ${rgba.length} bytes for ${width}x${height})`);
    }

    const bitmap: PageBitmap = { width, height, rgba };
    return bitmap;
  },

  // --- annotations ---

  async listPageAnnotations(handle, pageIndex) {
    return invoke<AnnotationSummaryDto[]>("list_page_annotations", { handle, pageIndex });
  },

  async addAnnotation(request: AddAnnotationRequest) {
    return invoke<OpenedDocument>("add_annotation_cmd", { request });
  },

  async deleteAnnotation(request: DeleteAnnotationRequest) {
    return invoke<OpenedDocument>("delete_annotation_cmd", { request });
  },

  async textSelectionQuads(request: TextSelectionQuadsRequest) {
    return invoke<[number, number, number, number][]>("text_selection_quads_cmd", { request });
  },

  async selectText(request: TextSelectionQuadsRequest) {
    return invoke<TextSelection>("select_text_cmd", { request });
  },

  // --- text/image editing ---

  async listTextRuns(handle, pageIndex) {
    return invoke<TextRunDto[]>("list_text_runs_cmd", { handle, pageIndex });
  },

  async editTextRun(request: EditTextRunRequest) {
    return invoke<OpenedDocument>("edit_text_run_cmd", { request });
  },

  async moveTextRun(request: MoveTextRunRequest) {
    return invoke<OpenedDocument>("move_text_run_cmd", { request });
  },

  async listImagePlacements(handle, pageIndex) {
    return invoke<ImagePlacementDto[]>("list_image_placements_cmd", { handle, pageIndex });
  },

  async moveImage(request: MoveImageRequest) {
    return invoke<OpenedDocument>("move_image_cmd", { request });
  },

  // --- forms ---

  async listFormFields(handle) {
    return invoke<FormFieldDto[]>("list_form_fields_cmd", { handle });
  },

  async fillFormFields(request: FillFormFieldsRequest) {
    return invoke<OpenedDocument>("fill_form_fields_cmd", { request });
  },

  async createFormField(request: CreateFormFieldRequest) {
    return invoke<OpenedDocument>("create_form_field_cmd", { request });
  },

  // --- signatures ---

  async listSignatures(handle) {
    return invoke<SignatureInfoDto[]>("list_signatures_cmd", { handle });
  },

  async searchDocument(request) {
    return invoke<SearchResultsDto>("search_document_cmd", { request });
  },

  async documentOutline(handle) {
    return invoke<OutlineEntryDto[]>("document_outline_cmd", { handle });
  },

  // Recents, the desktop half. An id here is the file's own path: the
  // app has a filesystem, so nothing needs storing beyond what the
  // shared list already keeps, and dedupe on reopening the same file
  // falls out of it.
  async recentDocuments() {
    return listRecents();
  },

  async openRecent(id) {
    try {
      // `openDocument` records it, which also moves it back to the top.
      return await tauriBackend.openDocument(id);
    } catch {
      // Moved, renamed, deleted, or on a volume that is no longer
      // mounted. All ordinary for a path last seen days ago.
      forgetRecent(id);
      return null;
    }
  },

  async forgetRecent(id) {
    forgetRecent(id);
  },

  async clearRecents() {
    clearRecents();
  },

  async flattenDocument(request) {
    return invoke<FlattenResultDto>("flatten_document_cmd", { request });
  },

  async removeMarkup(request) {
    return invoke<RemoveMarkupResultDto>("remove_markup_cmd", { request });
  },

  async encryptDocument(handle, choices) {
    const outputPath = await tauriBackend.pickSavePath();
    if (!outputPath) return null;
    return invoke<EncryptStats>("encrypt_document_cmd", {
      request: { handle, outputPath, ...choices },
    });
  },

  async numberPages(handle, choices) {
    return invoke<OpenedDocument>("number_pages_cmd", { request: { handle, ...choices } });
  },

  // The picker lives in the backend rather than the page, because where
  // an XFDF file comes from and goes to is exactly what differs between
  // a desktop app with a filesystem and an extension without one.
  async exportXfdf(handle) {
    const outputPath = await save({
      filters: [{ name: "XFDF", extensions: ["xfdf"] }],
    });
    if (!outputPath) return null;
    return invoke<ExportXfdfResult>("export_xfdf_cmd", {
      request: { handle, outputPath },
    });
  },

  /** A directory on disk, so the vault is a path and writing into it is
   * ordinary. When one is given the file goes straight there without a
   * dialog — which is the point of remembering a vault. */
  async exportMarkdown({ handle, fileName, vault }) {
    let outputPath: string | null;
    if (vault) {
      outputPath = `${vault.replace(/\/+$/, "")}/${fileName}`;
    } else {
      outputPath = await save({
        defaultPath: fileName,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
    }
    if (!outputPath) return { path: null, characters: 0 };
    return invoke<ExportMarkdownResult>("export_markdown_cmd", {
      request: { handle, outputPath },
    });
  },

  async exportText({ handle, fileName, vault }) {
    let outputPath: string | null;
    if (vault) {
      outputPath = `${vault.replace(/\/+$/, "")}/${fileName}`;
    } else {
      outputPath = await save({
        defaultPath: fileName,
        filters: [{ name: "Plain text", extensions: ["txt"] }],
      });
    }
    if (!outputPath) return { path: null, characters: 0 };
    return invoke<ExportMarkdownResult>("export_text_cmd", {
      request: { handle, outputPath },
    });
  },

  supportsVault() {
    return true;
  },

  async pickVault() {
    const picked = await open({ directory: true, multiple: false });
    if (!picked || Array.isArray(picked)) return null;
    return { key: picked, name: picked.split("/").pop() || picked };
  },

  async importXfdf(handle) {
    const picked = await open({
      multiple: false,
      filters: [{ name: "XFDF", extensions: ["xfdf", "xml"] }],
    });
    if (!picked || Array.isArray(picked)) return null;
    return invoke<ImportXfdfResult>("import_xfdf_cmd", {
      request: { handle, inputPath: picked },
    });
  },

  // --- pages ---

  async rotatePage(handle, pageIndex, deltaDegrees) {
    return invoke<OpenedDocument>("rotate_page_cmd", { handle, pageIndex, deltaDegrees });
  },

  async deletePage(handle, pageIndex) {
    return invoke<OpenedDocument>("delete_page_cmd", { handle, pageIndex });
  },

  async movePage(handle, pageIndex, direction: PageMoveDirection) {
    return invoke<OpenedDocument>("move_page_cmd", { handle, pageIndex, direction });
  },

  async setCropBox(handle, pageIndex, rect) {
    return invoke<OpenedDocument>("set_crop_box_cmd", { handle, pageIndex, rect });
  },

  async extractPages(request: ExtractPagesRequest) {
    return invoke<OpenedDocument>("extract_pages_cmd", { request });
  },

  async compressDocument(request: CompressDocumentRequest) {
    return invoke<CompressStats>("compress_document_cmd", { request });
  },

  async mergeDocuments(request: MergeDocumentsRequest) {
    return invoke<OpenedDocument>("merge_documents_cmd", { request });
  },

  async redactPage(request: RedactPageRequest) {
    return invoke<OpenedDocument>("redact_page_cmd", { request });
  },

  async applyWatermark(request: ApplyWatermarkRequest) {
    return invoke<OpenedDocument>("apply_watermark_cmd", { request });
  },

  // --- document-level tools ---

  async compareDocuments(request: CompareDocumentsRequest) {
    return invoke<CompareReportDto>("compare_documents_cmd", { request });
  },

  async ocrDocument(request: OcrDocumentRequest) {
    return invoke<OpenedDocument>("ocr_document_cmd", { request });
  },

  // --- file-picker primitives ---

  async pickOpenPath() {
    const selected = await open({ multiple: false, filters: PDF_FILTERS });
    if (!selected || Array.isArray(selected)) return null;
    return selected;
  },

  async pickOpenPaths() {
    const picked = await open({ multiple: true, filters: PDF_FILTERS });
    return Array.isArray(picked) ? picked : [];
  },

  async pickSavePath(defaultPath) {
    const target = await save({ filters: PDF_FILTERS, defaultPath });
    return target ?? null;
  },

  /** No-op — see types.ts's `Backend.releasePicks` doc: this backend's
   * "paths" are real filesystem paths returned directly by the OS picker
   * dialog, with no synthetic-key bookkeeping of their own to release. */
  async releasePicks() {},
};
