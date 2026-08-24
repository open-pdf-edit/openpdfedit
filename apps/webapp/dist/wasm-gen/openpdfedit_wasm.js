/* @ts-self-types="./openpdfedit_wasm.d.ts" */

/**
 * Mirrors `openpdfedit-engine::RenderedTile` across the wasm boundary —
 * the JS side needs the actual pixel dimensions to size its canvas, not
 * just the raw bytes (a plain `Uint8Array` return can't carry both).
 *
 * Also carries the page's untransformed size in PDF points
 * (`pointWidth`/`pointHeight`, from `Engine::page_sizes`) alongside the
 * rendered pixel size — the coordinate transform between canvas pixels
 * and PDF points (pointer input, drag-to-highlight, etc.) needs both.
 */
export class RenderedPage {
    static __wrap(ptr) {
        const obj = Object.create(RenderedPage.prototype);
        obj.__wbg_ptr = ptr;
        RenderedPageFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        RenderedPageFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_renderedpage_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get height() {
        const ret = wasm.renderedpage_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get pointHeight() {
        const ret = wasm.renderedpage_pointHeight(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get pointWidth() {
        const ret = wasm.renderedpage_pointWidth(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Uint8Array}
     */
    get rgba() {
        const ret = wasm.renderedpage_rgba(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get width() {
        const ret = wasm.renderedpage_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) RenderedPage.prototype[Symbol.dispose] = RenderedPage.prototype.free;

/**
 * One open-documents session for the whole extension page, backed by
 * `openpdfedit-session`'s engine-generic core — the same DTOs
 * (`OpenedDocumentInfo`/`PageSize`) the desktop's Tauri commands emit,
 * so the shared Svelte UI's coordinate math (which reads `page_sizes`
 * off the JSON `OpenedDocument` shape) works unchanged against this
 * backend. See this module's doc comment for why there is no
 * `undo`/`redo` here.
 *
 * `DocHandle` (a plain `u64` in `openpdfedit-engine`) crosses the wasm
 * boundary as `u32` in every method below, not `u64` — wasm-bindgen maps
 * `u64` to a JS `bigint`, but `Backend`'s TypeScript surface
 * (`apps/desktop/src/lib/backend/types.ts`) types every handle as a
 * plain `number`, matching what Tauri's JSON-serialized `OpenedDocument.handle`
 * already is. `PdfiumEngine`'s handles are an in-process
 * `AtomicU64` counter starting at 1 and incrementing by 1 per
 * `open`/`open_bytes` call (see that crate's `next_handle` field) — a
 * single extension page opening/closing documents one at a time will
 * never come close to exhausting a `u32`, so widening on the way in
 * (`handle as u64`) and narrowing on the way out (the JSON `handle`
 * field is `u64` serialized as a plain JSON number, which JS's
 * `JSON.parse` already reads as a `number`, not a `bigint`) is safe in
 * practice without needing wasm-bindgen's `bigint` support at all.
 */
export class WasmSession {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmSessionFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmsession_free(ptr, 0);
    }
    /**
     * Adds one markup annotation (highlight/underline/strikeout/
     * free-text/ink — see `AnnotationInput`'s `kind` tag) to an open
     * document and returns the resulting `OpenedDocumentInfo` DTO
     * serialized as JSON, exactly like every other mutating method on
     * this type — see this module's doc comment for why `request_json`
     * alone (no separate `handle` argument) is this method's whole
     * input: `AddAnnotationRequest` already carries its own `handle`
     * field, and this is the same JSON `types.ts`'s `addAnnotation`
     * sends the Tauri `add_annotation_cmd`. Thin wrapper over
     * `openpdfedit_session::annotations::add_annotation_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    addAnnotation(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_addAnnotation(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Tiled text/logo watermark baked into the document's pages (see
     * `openpdfedit-watermark`'s module doc). Mutating: takes the same
     * camelCase `ApplyWatermarkRequest` JSON as the desktop command
     * (with the optional logo as base64 RGBA inside the request) and
     * returns the rotated `OpenedDocumentInfo` JSON, exactly like
     * [`Self::redact_page`]. Thin wrapper over
     * `openpdfedit_session::watermark::apply_watermark_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    applyWatermark(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_applyWatermark(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Releases `handle`'s engine-side document (and its owned byte
     * buffer — see `Engine::open_bytes`'s doc), drops the session's own
     * `docs` bookkeeping entry for it, and — as of Phase 2's final-review
     * fix wave (C1) — also removes its `MemWorkingStore` entry and
     * `DocHistory` undo/redo entry, via `openpdfedit_session::close_document_impl`.
     * Without ever calling this, a `WasmSession` that opens many
     * documents over a page's lifetime (e.g. one-at-a-time via repeated
     * "Open…") leaks every previous document's engine-side state, the
     * same shape of bug the old per-document `WasmDocument`'s `Drop` impl
     * existed to avoid (see this crate's pre-Task-8 history) —
     * `WasmSession` has no `Drop` equivalent of its own since it isn't a
     * per-document type, so a caller (`wasm.ts`, or whatever Task 9 wires
     * up) is responsible for calling this when a document is genuinely
     * done with, not just when a new one is opened. The store/history
     * cleanup matters even more here than it did before this fix:
     * [`openpdfedit_session::open_document_bytes`] mints a unique working
     * key per open now (see that function's doc), so this is what
     * actually reclaims a closed document's `MemWorkingStore` bytes and
     * `DocHistory` stacks instead of letting them accumulate for the rest
     * of the page's lifetime.
     * @param {number} handle
     */
    closeDocument(handle) {
        wasm.wasmsession_closeDocument(this.__wbg_ptr, handle);
    }
    /**
     * Compares two documents' bytes — text mode always, pixel mode too
     * if `optionsJson`'s `pixelTargetWidth` is present — and returns a
     * `CompareReportDto` JSON string. Thin marshaling over the
     * already-portable `openpdfedit_session::compare::compare_bytes`
     * (verified: no `#[cfg(not(target_arch = "wasm32"))]` gate anywhere
     * on it or its callees — only the path-based `CompareRequest`/
     * `compare_documents_impl` half of that module is desktop-only, see
     * that module's own doc). Neither document needs to already be open
     * in this session; this is a one-shot, read-only comparison, exactly
     * like the desktop's `compare_documents_cmd` — no handle, no
     * rotation, nothing persisted. `wasm.ts`'s `compareDocuments` is
     * responsible for turning `types.ts`'s `CompareDocumentsRequest`
     * (two path *strings*) into `bytesA`/`bytesB` before calling this —
     * see that method's own doc for how it resolves the currently-open
     * document's live working-copy bytes for one side and a
     * freshly-picked file's raw bytes for the other, with no change to
     * `types.ts`'s `Backend` interface at all.
     * @param {Uint8Array} bytes_a
     * @param {Uint8Array} bytes_b
     * @param {string} options_json
     * @returns {string}
     */
    compareDocuments(bytes_a, bytes_b, options_json) {
        const ptr0 = passArray8ToWasm0(bytes_a, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(bytes_b, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(options_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_compareDocuments(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v4 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v4;
    }
    /**
     * Creates a new AcroForm field (text or checkbox) on `request`'s
     * document and returns the resulting `OpenedDocumentInfo` DTO
     * serialized as JSON. Takes only `request_json` — see
     * [`Self::fill_form_fields`]'s doc for why. Thin wrapper over
     * `openpdfedit_session::forms::create_form_field_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    createFormField(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_createFormField(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Deletes one annotation (identified by its stable `lopdf` object id
     * — see `AnnotationSummaryDto::id`'s doc) from an open document and
     * returns the resulting `OpenedDocumentInfo` DTO serialized as JSON.
     * Thin wrapper over
     * `openpdfedit_session::annotations::delete_annotation_impl` — see
     * [`Self::add_annotation`]'s doc for why this takes only
     * `request_json`.
     * @param {string} request_json
     * @returns {string}
     */
    deleteAnnotation(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_deleteAnnotation(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Deletes `page_index` from `handle`'s document and returns the
     * resulting `OpenedDocumentInfo` DTO serialized as JSON. Plain
     * `handle`/`page_index` arguments — see [`Self::rotate_page`]'s doc
     * for why. Thin wrapper over
     * `openpdfedit_session::pages::delete_page_impl`.
     * @param {number} handle
     * @param {number} page_index
     * @returns {string}
     */
    deletePage(handle, page_index) {
        const ret = wasm.wasmsession_deletePage(this.__wbg_ptr, handle, page_index);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * The document's outline (bookmarks) as a flattened, depth-tagged
     * JSON array of `OutlineEntryDto`. Read-only: reads the parsed
     * object graph only — no engine, no working copy, no mutation.
     * @param {number} handle
     * @returns {string}
     */
    documentOutline(handle) {
        const ret = wasm.wasmsession_documentOutline(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Substitutes the text of one run (identified by `runIndex` into a
     * freshly re-listed [`Self::list_text_runs`] array — see
     * `openpdfedit-textedit`'s module doc for why index-based, not an
     * opaque id or re-sent coordinates) on `request`'s document, returning
     * the resulting `OpenedDocumentInfo` DTO serialized as JSON. Takes
     * only `request_json` — see [`Self::redact_page`]'s doc for why. Thin
     * wrapper over `openpdfedit_session::textedit::edit_text_run_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    editTextRun(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_editTextRun(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * The encrypted bytes for this document's working copy, for the
     * extension to hand to a download. Export, not mutation: the open
     * document is untouched and no handle rotates — see
     * `openpdfedit_session::encrypt`'s module doc for why encrypting in
     * place would be the wrong shape.
     * @param {number} handle
     * @param {string} choices_json
     * @returns {Uint8Array}
     */
    encryptDocumentBytes(handle, choices_json) {
        const ptr0 = passStringToWasm0(choices_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_encryptDocumentBytes(this.__wbg_ptr, handle, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * Every markup annotation on the document, serialized as XFDF —
     * returns an `ExportXfdfDto` JSON string carrying the XML plus a
     * suggested filename. Read-only. The extension hands the XML to a
     * download rather than writing a file, which is why the portable
     * half returns a string instead of taking an output path.
     * @param {number} handle
     * @returns {string}
     */
    exportXfdf(handle) {
        const ret = wasm.wasmsession_exportXfdf(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Extracts `page_indices` from the currently-open document identified
     * by `request_json`'s `handle` and returns the **extracted
     * document's raw bytes** (`Uint8Array`) — not an `OpenedDocumentInfo`
     * JSON string, unlike every other mutating method on this type. See
     * this module's doc comment ("Forms/pages surface") for the full
     * rationale: `openpdfedit_session::pages::ExtractRequest`/
     * `extract_pages_impl` are desktop-only (path-based, `#[cfg(not(
     * target_arch = "wasm32"))]`) and aren't even compiled into this
     * crate's wasm32 build, so this method instead reads the source
     * document's current working-copy bytes directly from
     * `self.state.store` (the same source [`Self::working_copy_bytes`]
     * reads from) and hands them to the portable byte-level
     * [`extract_pages_bytes`]. The source document at `handle` is left
     * completely untouched — no mutation, no handle rotation, mirroring
     * `extract_pages_impl`'s own behavior on the desktop. `wasm.ts`'s
     * `extractPages` is what turns these bytes into a real, newly-opened
     * `OpenedDocument` (see this module's doc comment for how).
     * @param {string} request_json
     * @returns {Uint8Array}
     */
    extractPages(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_extractPages(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Fills one or more AcroForm field values on `request`'s document and
     * returns the resulting `OpenedDocumentInfo` DTO serialized as JSON,
     * exactly like every other mutating method on this type. Takes only
     * `request_json` — `FillFormRequest` already carries its own `handle`
     * field, matching `tauri.ts`'s `fillFormFields({ request })` call
     * shape (see this module's doc comment). Thin wrapper over
     * `openpdfedit_session::forms::fill_form_fields_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    fillFormFields(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_fillFormFields(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Bakes markup (and optionally filled form values) into the page,
     * returning a `FlattenResultDto` JSON string. Mutating: rotates the
     * handle like every other mutating method here, and is undoable.
     * @param {string} request_json
     * @returns {string}
     */
    flattenDocument(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_flattenDocument(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Adds every annotation in `xml` this app can draw, returning an
     * `ImportXfdfDto` JSON string. Mutating: rotates the handle, and is
     * undoable.
     * @param {number} handle
     * @param {string} xml
     * @returns {string}
     */
    importXfdf(handle, xml) {
        const ptr0 = passStringToWasm0(xml, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_importXfdf(this.__wbg_ptr, handle, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Every AcroForm field on `handle`'s document, as a JSON array of
     * `FormFieldDto`. Read-only (no handle rotation), so — like
     * `listPageAnnotations` — this takes a plain `handle` argument
     * instead of a request DTO, matching `tauri.ts`'s
     * `listFormFields(handle)` call shape. Thin wrapper over
     * `openpdfedit_session::forms::list_form_fields_impl` — see this
     * module's doc comment ("Forms/pages surface") for the full argument-
     * shape rationale.
     * @param {number} handle
     * @returns {string}
     */
    listFormFields(handle) {
        const ret = wasm.wasmsession_listFormFields(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Every image placement on `page_index` of `handle`'s document, as a
     * JSON array of `ImagePlacementDto`. Read-only, plain
     * `handle`/`page_index` arguments, matching `tauri.ts`'s
     * `listImagePlacements(handle, pageIndex)` call shape — see this
     * module's doc comment. Thin wrapper over
     * `openpdfedit_session::textedit::list_image_placements_impl`, which
     * — like [`Self::list_text_runs`] — reads only the already-open
     * document's object graph.
     * @param {number} handle
     * @param {number} page_index
     * @returns {string}
     */
    listImagePlacements(handle, page_index) {
        const ret = wasm.wasmsession_listImagePlacements(this.__wbg_ptr, handle, page_index);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Every annotation on `page_index` of `handle`'s document, as a JSON
     * array of `AnnotationSummaryDto`. Read-only, so — like
     * `openpdfedit_session::annotations::list_page_annotations_impl`
     * itself — this takes plain `handle`/`page_index` parameters instead
     * of a request DTO, matching `tauri.ts`'s
     * `listPageAnnotations(handle, pageIndex)` call shape.
     * @param {number} handle
     * @param {number} page_index
     * @returns {string}
     */
    listPageAnnotations(handle, page_index) {
        const ret = wasm.wasmsession_listPageAnnotations(this.__wbg_ptr, handle, page_index);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Every signature found on `handle`'s document, as a JSON array of
     * `SignatureInfoDto` — structural inspection only, never a
     * cryptographic verdict (`isVerified` is always `false`; see
     * `openpdfedit_session::signatures`'s module doc). Read-only, plain
     * `handle` argument, matching `tauri.ts`'s `listSignatures(handle)`
     * call shape — see this module's doc comment ("Signatures/redact/
     * textedit/image surface") for the full argument-shape rationale.
     * Thin wrapper over `openpdfedit_session::signatures::list_signatures_impl`,
     * which — unlike every other method on this type — never touches
     * `self.state.engine` at all (it only reads working-copy bytes
     * through `self.state.store`).
     * @param {number} handle
     * @returns {string}
     */
    listSignatures(handle) {
        const ret = wasm.wasmsession_listSignatures(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Every text run on `page_index` of `handle`'s document, as a JSON
     * array of `TextRunDto`. Read-only, plain `handle`/`page_index`
     * arguments, matching `tauri.ts`'s `listTextRuns(handle, pageIndex)`
     * call shape — see this module's doc comment. Thin wrapper over
     * `openpdfedit_session::textedit::list_text_runs_impl`, which reads
     * only the already-open document's object graph (no engine, no
     * store).
     * @param {number} handle
     * @param {number} page_index
     * @returns {string}
     */
    listTextRuns(handle, page_index) {
        const ret = wasm.wasmsession_listTextRuns(this.__wbg_ptr, handle, page_index);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Marks `handle`'s document clean (`is_dirty: false` in the next
     * `OpenedDocumentInfo` refresh) — thin wrapper over
     * `openpdfedit_session::mark_saved`. **Caller contract** (see that
     * function's own doc for the full rationale): call this only after
     * `wasm.ts` has confirmed its own `FileSystemFileHandle` write of the
     * bytes `saveToBytes` returned actually succeeded — never before that
     * write, and never on a failed one. Takes `&self` even though it
     * mutates: `SessionState::docs` is a `Mutex`-guarded map (interior
     * mutability), the same pattern every other method on this type
     * already relies on for the engine's own `Mutex<HashMap<...>>`
     * document table.
     * @param {number} handle
     * @returns {string}
     */
    markSaved(handle) {
        const ret = wasm.wasmsession_markSaved(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Merges the currently-open document at `request_json`'s
     * `openHandle` (if given — its **live working-copy** bytes, read via
     * `self.state.store`, not a stale snapshot; see
     * [`merge_open_doc_with_bytes`]'s doc) ahead of every source packed
     * into `sources_buffer`, and returns the merged document's raw bytes
     * (`Uint8Array`) — not `OpenedDocumentInfo` JSON. Same "no filesystem
     * to open a new document from" situation [`Self::extract_pages`] is
     * in, for the same reason (see its doc): `wasm.ts`'s `mergeDocuments`
     * is what turns these bytes into a real, newly opened
     * `OpenedDocument`, mirroring `extractPages`' landed pattern. The
     * document at `openHandle`, if any, is left completely untouched —
     * no mutation, no handle rotation — matching
     * `merge_documents_impl`'s own behavior on the desktop (a merge never
     * rotates the *source* handle; only the brand-new merged document
     * gets opened, under its own fresh handle).
     *
     * **Wasm boundary for multiple source files.** wasm-bindgen cannot
     * marshal a `Vec<Vec<u8>>` parameter directly, so this method needed
     * a deliberate design for "N source files' worth of bytes, in one
     * call." Two shapes were on the table (see task-2-brief.md):
     *
     * 1. A stateful two-step API — `beginMerge(openHandle)`, then
     *    `addMergeSource(bytes)` once per source, then `finishMerge() ->
     *    bytes`. This would require `WasmSession` to grow a new mutable
     *    staging field (e.g. a `Mutex<Vec<Vec<u8>>>`) that has to live
     *    *between* otherwise-independent calls — real complexity for real
     *    hazards: a second merge started before the first one's
     *    `finishMerge`, or a caller that simply forgets to call it, would
     *    leave staged buffers stuck in session state with nothing to
     *    notice or clean them up.
     * 2. A single call carrying every source's bytes as one flat,
     *    length-prefixed buffer (**chosen**): `sources_buffer` is a
     *    concatenation of `[u32 length, little-endian][that many bytes]`
     *    records, one per source, decoded by
     *    [`parse_length_prefixed_sources`].
     *
     * (2) wins on the grounds this whole crate already runs on: every
     * method here is synchronous, single-threaded, on the one JS main
     * thread (see this module's doc, "Why no `async fn` exports and no
     * Workers") — there is no scenario where interleaving several calls
     * would help, so a stateful multi-call API buys nothing (2) doesn't
     * already give for free, while (2) adds no new mutable state to
     * `WasmSession` at all and can never be left half-finished. Building
     * the length-prefixed buffer on the JS side (`wasm.ts`) is a handful
     * of lines with `DataView`/`Uint8Array.set` — no heavier than the
     * JSON marshaling every other mutating method on this type already
     * does.
     * @param {string} request_json
     * @param {Uint8Array} sources_buffer
     * @returns {Uint8Array}
     */
    mergeDocuments(request_json, sources_buffer) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(sources_buffer, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_mergeDocuments(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Relocates one image placement (identified by `placementIndex` into
     * a freshly re-listed [`Self::list_image_placements`] array) by `(dx,
     * dy)` PDF-point offsets on `request`'s document, returning the
     * resulting `OpenedDocumentInfo` DTO serialized as JSON. Takes only
     * `request_json` — see [`Self::redact_page`]'s doc for why. Thin
     * wrapper over `openpdfedit_session::textedit::move_image_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    moveImage(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_moveImage(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Swaps `page_index` with its "up" or "down" neighbor (`direction`,
     * a plain `"Up"`/`"Down"` string — `MoveDirection`'s own
     * non-`rename_all`'d serde shape, matching `types.ts`'s
     * `PageMoveDirection`) in `handle`'s document, returning the
     * resulting `OpenedDocumentInfo` DTO serialized as JSON. Plain
     * `handle`/`page_index`/`direction` arguments, not a request DTO —
     * `move_page_cmd` takes bare `State` + scalar arguments on the
     * desktop side and `tauri.ts` calls it as `invoke("move_page_cmd", {
     * handle, pageIndex, direction })` — see this module's doc comment
     * for why this is the one place the brief's own request-JSON
     * shorthand didn't match the real desktop shape. Thin wrapper over
     * `openpdfedit_session::pages::move_page_impl`.
     * @param {number} handle
     * @param {number} page_index
     * @param {string} direction
     * @returns {string}
     */
    movePage(handle, page_index, direction) {
        const ptr0 = passStringToWasm0(direction, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_movePage(this.__wbg_ptr, handle, page_index, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Relocates one text run by `(dx, dy)` PDF-point offsets without
     * touching its content — see
     * `openpdfedit_session::textedit::move_text_run_impl`'s doc for why
     * this imposes no `isEditable` requirement, unlike
     * [`Self::edit_text_run`]. Returns the resulting `OpenedDocumentInfo`
     * DTO serialized as JSON. Takes only `request_json` — see
     * [`Self::redact_page`]'s doc for why. Thin wrapper over
     * `openpdfedit_session::textedit::move_text_run_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    moveTextRun(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_moveTextRun(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Layered guard against constructing more than one `WasmSession`
     * (and therefore more than one `PdfiumEngine`, hence more than one
     * `FPDF_InitLibrary` call) per process: `wasm.ts`'s `ensureSession()`
     * memoizes its own single call to `new WasmSession()` — the
     * friendly, everyday-operation path — but that's a JS-side
     * *convention*, not something this constructor can rely on; any JS
     * caller (a bug in `wasm.ts` itself, a future caller that doesn't
     * go through `ensureSession()`, hot-reloaded dev code, ...) could
     * still call `new WasmSession()` a second time. This
     * `OnceLock<()>`-backed check is the backstop that makes a second
     * call fail loudly with a clear error instead of silently
     * double-initializing PDFium's process-global state — restored
     * after a review found the previous design (a `OnceLock`-cached
     * `&'static PdfiumEngine` behind a free function every method went
     * through) had been dropped when `WasmSession` started owning its
     * engine by value, leaving the single-init invariant enforced only
     * by `wasm.ts`'s memoization. Once the slot is claimed, it stays
     * claimed even if `build_engine()` below then fails — matching the
     * old design's own no-retry-after-failure behavior (its
     * `get_or_init` closure ran at most once too, Ok or Err), not a new
     * regression: a second `WasmSession::new()` call always fails from
     * here on, whether or not the first call actually produced a
     * working engine.
     */
    constructor() {
        const ret = wasm.wasmsession_new();
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WasmSessionFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Stamps page numbers or Bates numbering into a margin of each
     * page, returning the resulting `OpenedDocumentInfo` DTO as JSON.
     * Mutating: rotates the handle, and is undoable.
     * @param {string} request_json
     * @returns {string}
     */
    numberPages(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_numberPages(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Opens a document from in-memory `bytes` (no filesystem — there is
     * none on `wasm32-unknown-unknown`) via
     * `openpdfedit_session::open_document_bytes`, registers it under
     * `display_name` (the extension's synthetic identity for a document
     * with no real on-disk path — typically the picked file's name), and
     * returns the resulting `OpenedDocumentInfo` DTO serialized as JSON.
     * Field names/casing are exactly what `#[derive(Serialize)]` emits
     * for that struct (no `rename_all`), which is also exactly what
     * `apps/desktop/src/lib/backend/types.ts`'s `OpenedDocument`
     * interface expects — see that crate's own doc comment inventory.
     * @param {string} display_name
     * @param {Uint8Array} bytes
     * @returns {string}
     */
    openDocument(display_name, bytes) {
        const ptr0 = passStringToWasm0(display_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_openDocument(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v3;
    }
    /**
     * Every page's size in PDF points, in reading order, as a JSON array
     * of `{width, height}` objects (`openpdfedit_session::PageSize`'s
     * own serde shape — the same one embedded in `OpenedDocumentInfo`'s
     * `page_sizes` field). Lets a caller lay out a virtualized scroll
     * container without a full `openDocument`/reopen round trip.
     * @param {number} handle
     * @returns {string}
     */
    pageSizes(handle) {
        const ret = wasm.wasmsession_pageSizes(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Permanently removes the content (text and images, not just a black
     * box painted over live data — see `openpdfedit-redact`'s module doc)
     * under `rect` on one page of `request`'s document, returning the
     * resulting `OpenedDocumentInfo` DTO serialized as JSON, exactly like
     * every other mutating method on this type. Takes only
     * `request_json` — `RedactPageRequest` already carries its own
     * `handle` field, matching `tauri.ts`'s `redactPage({ request })`
     * call shape (see this module's doc comment). Thin wrapper over
     * `openpdfedit_session::redact::redact_page_impl`.
     * @param {string} request_json
     * @returns {string}
     */
    redactPage(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_redactPage(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * The redo half of [`Self::undo`] — see that method's doc. Thin
     * wrapper over `openpdfedit_session::redo_impl`.
     * @param {number} handle
     * @returns {string}
     */
    redo(handle) {
        const ret = wasm.wasmsession_redo(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * Renders `page_index` at `target_width` pixels wide (aspect-ratio
     * preserved), returning both the rendered pixels and the page's
     * untransformed size in PDF points (see `RenderedPage`'s doc).
     * @param {number} handle
     * @param {number} page_index
     * @param {number} target_width
     * @returns {RenderedPage}
     */
    renderPage(handle, page_index, target_width) {
        const ret = wasm.wasmsession_renderPage(this.__wbg_ptr, handle, page_index, target_width);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return RenderedPage.__wrap(ret[0]);
    }
    /**
     * Rotates `page_index` of `handle`'s document by `delta_degrees` and
     * returns the resulting `OpenedDocumentInfo` DTO serialized as JSON.
     * Plain `handle`/`page_index`/`delta_degrees` arguments, not a
     * request DTO — `rotate_page_cmd` takes bare `State` + scalar
     * arguments on the desktop side (no request DTO exists for it) and
     * `tauri.ts` calls it as `invoke("rotate_page_cmd", { handle,
     * pageIndex, deltaDegrees })` — see this module's doc comment.  Thin
     * wrapper over `openpdfedit_session::pages::rotate_page_impl`.
     * @param {number} handle
     * @param {number} page_index
     * @param {number} delta_degrees
     * @returns {string}
     */
    rotatePage(handle, page_index, delta_degrees) {
        const ret = wasm.wasmsession_rotatePage(this.__wbg_ptr, handle, page_index, delta_degrees);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * The engine-side bytes of the currently-open document at `handle`
     * — i.e. whatever `Engine::save_to_bytes` (a full PDFium rewrite of
     * the in-memory document, not a copy of the original opened bytes)
     * produces *right now*. Phase 1 has no mutation surface at all (see
     * this module's doc comment), so today this is always byte-for-byte
     * equivalent to a straight PDFium round-trip of whatever was opened
     * — but the method itself doesn't assume that; it always asks the
     * engine for its current bytes, so it stays correct once Phase 2
     * adds real edits.
     *
     * **Deliberately does not mark the document clean.** Takes `&self`
     * (no interior mutation happens here either) because the bytes this
     * returns aren't durably saved *anywhere* yet — this method only asks
     * PDFium to serialize the in-memory document; the actual "save",
     * writing those bytes to the file the user opened, is `wasm.ts`'s job
     * (a `FileSystemFileHandle` write, entirely outside this crate and
     * this wasm module). If this method flipped `dirty` to `false` before
     * that write even started, a failed `FileSystemWritableFileStream`
     * write (disk full, permission revoked mid-session, ...) would leave
     * the document showing clean while the user's file on disk was never
     * actually updated — silently discarding the "you have unsaved
     * changes" signal exactly when it matters most. [`Self::mark_saved`]
     * exists as the separate call `wasm.ts` makes *after* its own write
     * resolves successfully, so the dirty flag's truth always tracks
     * what's actually durable, not what's merely been computed.
     *
     * **Not what `wasm.ts`'s own save path calls** (Phase 2 final-review
     * I2): calling this unconditionally would re-derive a *fresh* full
     * engine-side rewrite of the in-memory document right now, which can
     * diverge from whatever the working copy's *last* store-routed write
     * actually produced — see [`Self::working_copy_bytes`]'s doc (and its
     * I3 correction) for the method that actually byte-matches the
     * desktop's save output, and for why "the mutating commands preserve
     * signatures" stopped being universally true once form-filling's own
     * full-PDFium-rewrite write path became reachable here too. Kept
     * around for any caller that genuinely wants a full PDFium rewrite of
     * the in-memory document rather than the working copy's own bytes;
     * today nothing in this crate is that caller.
     * @param {number} handle
     * @returns {Uint8Array}
     */
    saveToBytes(handle) {
        const ret = wasm.wasmsession_saveToBytes(this.__wbg_ptr, handle);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Finds every occurrence of `query` in the open document, returning
     * a `SearchResultsDto` serialized as JSON. Read-only: no mutation,
     * no handle rotation, nothing written — see
     * `openpdfedit_session::search`'s module doc for why this one is
     * engine-only and needed no `WorkingStore` plumbing to become
     * portable.
     * @param {number} handle
     * @param {string} query
     * @param {boolean} match_case
     * @param {boolean} whole_word
     * @returns {string}
     */
    searchDocument(handle, query, match_case, whole_word) {
        const ptr0 = passStringToWasm0(query, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_searchDocument(this.__wbg_ptr, handle, ptr0, len0, match_case, whole_word);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Sets `page_index`'s crop box to `rect` (`[x0, y0, x1, y1]` in PDF
     * page-space points, a JS `Float32Array` at the call site) in
     * `handle`'s document, returning the resulting `OpenedDocumentInfo`
     * DTO serialized as JSON. Plain `handle`/`page_index`/`rect`
     * arguments — `set_crop_box_cmd` takes bare `State` + scalar
     * arguments (including a bare `rect: [f32; 4]`) on the desktop side —
     * see this module's doc comment. Thin wrapper over
     * `openpdfedit_session::pages::set_crop_box_impl`.
     * @param {number} handle
     * @param {number} page_index
     * @param {Float32Array} rect
     * @returns {string}
     */
    setCropBox(handle, page_index, rect) {
        const ptr0 = passArrayF32ToWasm0(rect, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_setCropBox(this.__wbg_ptr, handle, page_index, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Snaps a drag gesture's PDF-point start/end coordinates to the
     * nearest character boundaries and returns the covered text's line
     * quads (a JSON array of `[x0, y0, x1, y1]`) — the same snapping
     * logic the highlight/underline/strikeout tools use to build their
     * `quads` input to [`Self::add_annotation`]. Read-only (no
     * `docs`/`history`/`store` involved), so unlike the mutating methods
     * above this never touches undo/redo history. Thin wrapper over
     * `openpdfedit_session::annotations::text_selection_quads_impl` —
     * see [`Self::add_annotation`]'s doc for why this takes only
     * `request_json`.
     * @param {string} request_json
     * @returns {string}
     */
    textSelectionQuads(request_json) {
        const ptr0 = passStringToWasm0(request_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmsession_textSelectionQuads(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v2;
    }
    /**
     * Undoes the most recent edit for `handle`'s document (restores the
     * working copy's pre-edit bytes via the `MemWorkingStore` this
     * session was constructed with, and rotates the render handle, same
     * as any other write) and returns the resulting `OpenedDocumentInfo`
     * DTO serialized as JSON. Errors if there's nothing to undo — the
     * front-end should already be disabling the Undo button via
     * `OpenedDocumentInfo::can_undo`, so this is a defensive backstop,
     * not the primary UX guard, mirroring `undo_cmd`'s own doc on the
     * desktop side. Thin wrapper over `openpdfedit_session::undo_impl`.
     * @param {number} handle
     * @returns {string}
     */
    undo(handle) {
        const ret = wasm.wasmsession_undo(this.__wbg_ptr, handle);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getCachedStringFromWasm0(ret[0], ret[1]);
        if (ret[0] !== 0) { wasm.__wbindgen_free(ret[0], ret[1], 1); }
        return v1;
    }
    /**
     * The **working-copy** bytes this session's `MemWorkingStore` already
     * holds for `handle`'s document — exactly what the *last* store-routed
     * write for it produced. This is what `wasm.ts`'s save path
     * (`writeToFileHandle`) must actually write to the
     * `FileSystemFileHandle` — see [`Self::save_to_bytes`]'s doc for why
     * that method is the wrong one for this job (it always re-derives a
     * fresh, full PDFium rewrite of the *current* in-memory document,
     * which may not even match what this method returns). This method
     * mirrors what the desktop backend does for the same reason:
     * `save_document_impl` copies the working copy's *bytes on disk* over
     * the original path (`copy_with_lock_retry`), never re-derives them.
     *
     * **Corrected (fix-wave re-review's I3):** this doc used to claim the
     * working copy is *always* an `openpdfedit-doc` lopdf incremental
     * save — true for `commit_mutation`/`undo_impl`/`redo_impl` (every
     * mutation routed through [`crate::commit_mutation`]: annotations,
     * pages, redact, textedit, form-field creation), but false once a
     * form has been filled. `openpdfedit_session::forms::fill_form_fields_impl`
     * writes a *different* way — `Engine::fill_form_fields` mutates
     * PDFium's in-memory document, then `Engine::save_to_bytes`
     * (PDFium's own `FPDF_SaveAsCopy`/`FPDF_SaveWithVersion`, a full
     * rewrite of the *entire* file) produces the bytes that get
     * `store.write`-ten — see that function's own doc for why filling
     * can't go through the lopdf incremental path at all (PDFium's own
     * form model, not `openpdfedit-doc`'s object graph, is what actually
     * updates field values/appearances). A full rewrite renumbers and
     * repositions every object in the file, which invalidates any
     * existing signature's `/ByteRange` — so **a fill invalidates
     * existing signature byte ranges**, exactly like every other
     * full-rewrite write path in this codebase. This is not new
     * wasm-specific behavior: the desktop's own `fill_form_fields_impl`
     * has written this same way since forms-fill landed in M4, on the
     * same `EngineHandle`/PDFium write path — Phase 3 Task 2 made that
     * code portable (this crate can now call it too), it didn't change
     * what it does to a signed document's byte ranges. A caller of this
     * method can't tell which write path produced the bytes it returns
     * just from the return value — see `apps/desktop/src/routes/
     * +page.svelte`'s `refreshSignatures` for how the desktop UI accounts
     * for this (re-fetching signatures after a fill, not assuming they
     * survived it).
     *
     * Errors if `handle` is unknown (mirrors every other handle-taking
     * method in this crate — an unknown handle is a caller bug, not a
     * normal path). Read-only against the store — like `saveToBytes`,
     * does **not** mark the document clean; that's still [`Self::mark_saved`]'s
     * job, called only after the caller's own write actually succeeds.
     * @param {number} handle
     * @returns {Uint8Array}
     */
    workingCopyBytes(handle) {
        const ret = wasm.wasmsession_workingCopyBytes(this.__wbg_ptr, handle);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
}
if (Symbol.dispose) WasmSession.prototype[Symbol.dispose] = WasmSession.prototype.free;

/**
 * Establishes a binding between an external Pdfium WASM module and `pdfium-render`'s WASM module.
 * This function should be called from Javascript once the external Pdfium WASM module has been loaded
 * into the browser. It is essential that this function is called _before_ initializing
 * `pdfium-render` from within Rust code. For an example, see:
 * <https://github.com/ajrcarey/pdfium-render/blob/master/examples/index.html>
 * @param {any} pdfium_wasm_module
 * @param {any} local_wasm_module
 * @param {boolean} debug
 * @returns {boolean}
 */
export function initialize_pdfium_render(pdfium_wasm_module, local_wasm_module, debug) {
    const ret = wasm.initialize_pdfium_render(pdfium_wasm_module, local_wasm_module, debug);
    return ret !== 0;
}

/**
 * A callback function that can be invoked by Pdfium's `FPDF_LoadCustomDocument()` function,
 * wrapping around `crate::utils::files::read_block_from_callback()` to shuffle data buffers
 * from our WASM memory heap to Pdfium's WASM memory heap as they are loaded.
 * @param {number} param
 * @param {number} position
 * @param {number} pBuf
 * @param {number} size
 * @returns {number}
 */
export function read_block_from_callback_wasm(param, position, pBuf, size) {
    const ret = wasm.read_block_from_callback_wasm(param, position, pBuf, size);
    return ret;
}

/**
 * A callback function that can be invoked by Pdfium's `FPDF_SaveAsCopy()` and `FPDF_SaveWithVersion()`
 * functions, wrapping around `crate::utils::files::write_block_from_callback()` to shuffle data buffers
 * from Pdfium's WASM memory heap to our WASM memory heap as they are written.
 * @param {number} param
 * @param {number} buf
 * @param {number} size
 * @returns {number}
 */
export function write_block_from_callback_wasm(param, buf, size) {
    const ret = wasm.write_block_from_callback_wasm(param, buf, size);
    return ret;
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_debug_string_c25d447a39f5578f: function(arg0, arg1) {
            const ret = debugString(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_object_a27215656b807791: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_undefined_c05833b95a3cf397: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_eq_e659fcf7b0e32763: function(arg0, arg1) {
            const ret = arg0 === arg1;
            return ret;
        },
        __wbg___wbindgen_number_get_394265ed1e1b84ee: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_b0ca35b86a603356: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            var v0 = getCachedStringFromWasm0(arg0, arg1);
            throw new Error(v0);
        },
        __wbg_apply_3ac86a26fdb56c05: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.apply(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_call_a6e5c5dce5018821: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_decode_025cf7f5108091dc: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.decode(getArrayU8FromWasm0(arg2, arg3));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            var v0 = getCachedStringFromWasm0(arg0, arg1);
            if (arg0 !== 0) { wasm.__wbindgen_free(arg0, arg1, 1); }
            console.error(v0);
        },
        __wbg_from_13e323c65fc8f464: function(arg0) {
            const ret = Array.from(arg0);
            return ret;
        },
        __wbg_getRandomValues_cc7f052a444bb2ce: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getTime_d6f070c088c9b5ed: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_get_5b0994f14acc7b27: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.get(arg1 >>> 0);
            return ret;
        }, arguments); },
        __wbg_get_78f252d074a84d0b: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_index_e68b01fac18aa799: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_get_unchecked_6e0ad6d2a41b06f6: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_isArray_0677c962b281d01a: function(arg0) {
            const ret = Array.isArray(arg0);
            return ret;
        },
        __wbg_length_1f0964f4a5e2c6d8: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_370319915dc99107: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_81804e6c5f144937: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_a93f98b282d687d7: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_new_0_3da9e97f24fc69be: function() {
            const ret = new Date();
            return ret;
        },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_new_32b398fb48b6d94a: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_from_slice_77cdfb7977362f3c: function(arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_with_label_725575f8e06eaf0c: function() { return handleError(function (arg0, arg1) {
            var v0 = getCachedStringFromWasm0(arg0, arg1);
            const ret = new TextDecoder(v0);
            return ret;
        }, arguments); },
        __wbg_new_with_length_f8cbc3a5b9ff9368: function(arg0) {
            const ret = new Array(arg0 >>> 0);
            return ret;
        },
        __wbg_of_2bf3ed8a776ff19a: function(arg0, arg1, arg2, arg3) {
            const ret = Array.of(arg0, arg1, arg2, arg3);
            return ret;
        },
        __wbg_of_5f1b88183ddb5d94: function(arg0, arg1) {
            const ret = Array.of(arg0, arg1);
            return ret;
        },
        __wbg_of_85f52f8b6491a7ca: function(arg0) {
            const ret = Array.of(arg0);
            return ret;
        },
        __wbg_of_b0cd2e09b31a9684: function(arg0, arg1, arg2) {
            const ret = Array.of(arg0, arg1, arg2);
            return ret;
        },
        __wbg_of_d1905c2e39225d15: function(arg0, arg1, arg2, arg3, arg4) {
            const ret = Array.of(arg0, arg1, arg2, arg3, arg4);
            return ret;
        },
        __wbg_prototypesetcall_4770620bbe4688a0: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_push_d2ae3af0c1217ae6: function(arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        },
        __wbg_set_5d8eaa6b2caf4444: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.set(arg1 >>> 0, arg2);
        }, arguments); },
        __wbg_set_61e45ae8061eca11: function(arg0, arg1, arg2) {
            arg0.set(arg1, arg2 >>> 0);
        },
        __wbg_set_8a16b38e4805b298: function(arg0, arg1, arg2) {
            arg0[arg1 >>> 0] = arg2;
        },
        __wbg_slice_ecaaa67ec7cf96c1: function(arg0, arg1, arg2) {
            const ret = arg0.slice(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_static_accessor_GLOBAL_4ef717fb391d88b7: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_8d1badc68b5a74f4: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_146583524fe1469b: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_f2829a2234d7819e: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_3ed232c8a6baee09: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            var v0 = getCachedStringFromWasm0(arg0, arg1);
            // Cast intrinsic for `Ref(CachedString) -> Externref`.
            const ret = v0;
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./openpdfedit_wasm_bg.js": import0,
    };
}

const RenderedPageFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_renderedpage_free(ptr, 1));
const WasmSessionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmsession_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

function getCachedStringFromWasm0(ptr, len) {
    if (ptr === 0) {
        return getFromExternrefTable0(len);
    } else {
        return getStringFromWasm0(ptr, len);
    }
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

function getFromExternrefTable0(idx) { return wasm.__wbindgen_externrefs.get(idx); }

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayF32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getFloat32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('openpdfedit_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
