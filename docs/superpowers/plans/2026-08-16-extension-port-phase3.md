# Extension Port Phase 3 (Forms + Page Organization) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The extension gains the desktop's forms surface (list/fill/create
fields) and page organization (rotate/delete/move/crop/extract), by
extending the `Engine` trait with the two missing form operations and
porting the remaining fs-coupled impls onto the `WorkingStore`.

**Architecture:** Two known prerequisites from Phase 2's closing ledger,
then the established wiring pattern (wasm-bindgen methods → wasm.ts →
shared UI already works). Merge is EXCLUDED from the extension in this
phase: its desktop form consumes extra source *files* picked by path;
the wasm flow would need multi-document byte plumbing through
`MergeRequest` — deferred to Phase 5 alongside compare (same
multi-file-input shape); the desktop keeps its existing merge unchanged.

**Tech Stack:** unchanged (see Phase 2 plan).

## Global Constraints (all inherited from Phase 2's plan, plus)

- Real PDFium is present in this worktree — every battery reports
  ran-vs-skipped counts; a suite that skips is not green evidence.
- The subagent environment rule, verbatim in every dispatch: other
  projects' `/tmp/*` directories are NEVER to be deleted, even if they
  look stale — on ENOSPC, prune ONLY `/tmp/openpdfedit-target/debug`
  and this pipeline's own `/tmp/openpdfedit-*review*` dirs, then
  substitute `cargo check --tests` and document.
- DTO serde shapes frozen; boot e2e green after every task.

## Phase 3 prerequisites (from Phase 2's ledger, must land first)

1. `Engine` trait lacks `list_form_fields`/`fill_form_fields`
   (inherent-only on `PdfiumEngine`/`EngineHandle`) — extend the trait
   using the same recipe `open_bytes` used (trait method + both impls +
   `EngineHandle` request/match-arm plumbing).
2. `fill_form_fields_impl` is cfg-fenced off wasm32 because it writes
   the working copy via `fs::rename`, bypassing the store — its port
   must route the written bytes through `store.write` (PDFium saves to a
   temp byte buffer via `save_to_bytes` on wasm; on desktop the existing
   rename path stays, or unify both onto store.write if the desktop
   equivalence argument holds — implementer investigates and documents).

---

### Task 1: Extend the `Engine` trait with form operations

**Files:** `crates/openpdfedit-engine/src/lib.rs`, `crates/openpdfedit-engine/src/thread.rs`

**Interfaces:** trait gains `fn list_form_fields(&self, handle) -> Result<Vec<FormField>, EngineError>` and `fn fill_form_fields(&self, handle, values: HashMap<String,String>) -> Result<(), EngineError>` — signatures copied from the existing inherent methods (verify from source). `PdfiumEngine` impl forwards to its inherent methods; `EngineHandle` impl forwards through the existing inherent (channel) methods. Existing inherent methods stay (desktop callers unchanged).

- [ ] Failing test first (trait-object test in thread.rs's module via `shared_handle()`, exercising list through `&dyn Engine` on a form fixture built with lopdf as forms.rs's tests do) → implement → full engine+session suites + wasm32 build → commit.

---

### Task 2: Port `fill_form_fields_impl` + forms listing onto the portable surface

**Files:** `crates/openpdfedit-session/src/forms.rs`

**Interfaces:** `list_form_fields_impl`/`fill_form_fields_impl` genericized to `E: Engine` (un-fencing them from wasm32); the fill path's working-copy write routed through the store: on wasm, `engine.save_to_bytes(handle)` → `store.write(&path, bytes)`; investigate whether the desktop keeps `save_document`+rename (path-specific, `#[cfg]`) or unifies — either is acceptable with the equivalence documented; `normalize_button_states_in_file` becomes byte-based (`normalize_button_states_in_bytes`) or stays desktop-side cfg'd, implementer's documented call. `create_form_field_impl` is already portable (verify).

- [ ] Failing portable test first (MemWorkingStore: open bytes with form fixture → fill → list reflects value → undo → redo) → implement → suites incl. wasm32 → commit.

---

### Task 3: wasm-bindgen + wasm.ts forms/pages surface

**Files:** `crates/openpdfedit-wasm/src/lib.rs`, `apps/desktop/src/lib/backend/wasm.ts`

**Interfaces:** WasmSession gains: `listFormFields(handle)`, `fillFormFields(requestJson)`, `createFormField(requestJson)`, `rotatePage(handle, pageIndex)`, `deletePage(handle, pageIndex)`, `movePage(requestJson)`, `setCropBox(requestJson)`, `extractPages(requestJson → Uint8Array of the extracted doc's bytes — the wasm flow returns bytes for the UI to save via a picker rather than writing a path)` — match each Tauri command's request shape exactly (read tauri.ts + the session DTOs; the extract case deviates deliberately: desktop writes a file at a picked path, wasm returns bytes and wasm.ts saves via showSaveFilePicker; the UI's extract handler goes through backend.extractPages whose Backend-interface signature must already accommodate both — READ types.ts's current extractPages signature and adapt the WASM SIDE to it, not vice versa; if the interface genuinely can't express the wasm flow without a UI change, NEEDS_CONTEXT with specifics). wasm.ts replaces the corresponding notImplemented throws; migrateOpenDoc on every mutating return; glue regenerated + .d.ts diffed.

- [ ] Implement → full battery (both build flavors, e2e) + on-paper trace of one forms flow and one pages flow → commit.

---

### Task 4: Phase 3 battery + ledger

- [ ] Full battery (Phase 2 Task 5's list); ledger entry with real numbers; Phase 4 notes (signatures/redact/textedit wiring is the same pattern; image-move needs list_image_placements which is already portable — verify and note).
- [ ] Commit docs/ledger.
