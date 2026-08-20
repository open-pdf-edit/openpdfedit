# Extension Port Phase 2 (Annotations Parity) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The extension gains the desktop's full annotation feature set —
all markup tools, the comments panel, delete, text-selection quads, and
working undo/redo/dirty-tracking — by giving the shared session crate a
storage abstraction that works without a filesystem.

**Architecture:** Phase 1 left one structural gap between the desktop and
wasm builds: every mutation flows through `commit_mutation` →
`std::fs::write(working_copy)` → `reopen_after_write` → path-based
reopen, and undo/redo replay those on-disk snapshots. wasm32 has no
filesystem, so Phase 2 introduces a `WorkingStore` abstraction inside
`openpdfedit-session` — the ONE new design element of this phase — with
two implementations: the existing on-disk behavior (desktop, byte-for-byte
identical semantics) and an in-memory store (wasm). Everything else is
wiring: wasm-bindgen exports for the annotation/undo surface, `wasm.ts`
methods replacing "not yet ported" throws, and the dirty-flag becoming
real in the extension (un-disabling Save).

**Tech Stack:** unchanged from Phase 1 (Rust workspace, wasm-bindgen
=0.2.126 crate+CLI, Svelte 5/SvelteKit shared SPA, Playwright boot e2e).

## Global Constraints (inherited from Phase 1 + spec; binding on every task)

- Desktop behavior must not change. Guards: full existing test battery
  (`cargo test -p openpdfedit-session -p openpdfedit-engine`,
  `cargo check --workspace --tests`), `npm run check` + both build
  flavors, and the extension boot e2e (`apps/extension npm run e2e`).
- `openpdfedit-session` stays tauri-free and must build for
  `wasm32-unknown-unknown` after every task
  (`cargo build -p openpdfedit-session --target wasm32-unknown-unknown`).
- `openpdfedit-wasm`: no `async fn` exports, no Workers, the
  `SESSION_CREATED` single-init guard stays (its safety argument depends
  on these).
- `export CARGO_TARGET_DIR=/tmp/openpdfedit-target` before every cargo
  command; disk is chronically tight — prune `/tmp/openpdfedit-target/debug`
  between heavy steps, prefer `cargo check --tests` substitutions on
  ENOSPC and document them.
- No real PDFium in this sandbox: PDFium-touching tests skip gracefully;
  green = compile-integrity + non-PDFium logic + skip paths. The user's
  end-of-port manual Chrome test is the runtime gate.
- Session tests share ONE engine via `test_support::shared_handle()`.
- DTO serde shapes are frozen (snake_case as landed; `types.ts` is the
  reference; a silent shape change blanks the extension viewer).

## Phase 2 inputs from Phase 1's final review (must-honor)

- Save is currently disabled in the extension because `is_dirty` can
  never become true — this phase makes dirty-tracking real and MUST
  re-enable the Save affordance (final-review Minor 15).
- Document-close lifecycle: when `openDocument` succeeds, close the
  previous document's handle in BOTH backends (final-review finding 7 —
  fixes the desktop's own pre-existing accumulation too; behavior change
  is memory-only, not user-visible, and applies symmetrically).
- The fs-bound session functions compile clean-but-nonfunctional on
  wasm32 today (std fs stubs). After `WorkingStore` lands, the remaining
  genuinely-path-only entry points must be cfg-fenced so a future
  implementer cannot wire them to wasm and discover the problem at
  runtime (final-review Recommendation 4).

---

### Task 1: `WorkingStore` abstraction in `openpdfedit-session`

**Files:**
- Modify: `crates/openpdfedit-session/src/lib.rs`

**Interfaces:**
- Produces:

```rust
/// Where a document's working copy and undo/redo snapshots live.
/// Desktop: the on-disk scratch file (existing behavior, unchanged).
/// wasm: an in-memory byte store (no filesystem on wasm32).
pub trait WorkingStore: Send {
    /// Read the current working-copy bytes for `key`.
    fn read(&self, key: &Path) -> Result<Vec<u8>, SessionError>;
    /// Overwrite the working copy for `key`.
    fn write(&self, key: &Path, bytes: &[u8]) -> Result<(), SessionError>;
    /// Remove the working copy for `key` (close/cleanup).
    fn remove(&self, key: &Path);
}

#[cfg(not(target_arch = "wasm32"))]
pub struct FsWorkingStore; // read/write/remove = std::fs on the key path

pub struct MemWorkingStore(Mutex<HashMap<PathBuf, Vec<u8>>>); // portable
```

- `SessionState<E>` gains a `store: S` parameter (`SessionState<E, S: WorkingStore>`)
  OR holds `Box<dyn WorkingStore>` — implementer's choice; pick the one
  that disturbs the existing generic bounds and desktop wrapper code
  least, and document why. Desktop constructs with `FsWorkingStore`
  (behavior identical: it reads/writes the same paths `commit_mutation`
  already used); wasm constructs with `MemWorkingStore`.
- `commit_mutation`, `capture_pre_edit_snapshot`, `undo_impl`,
  `redo_impl`, `reopen_after_write` route ALL working-copy/snapshot
  reads+writes through the store. The engine-reopen step changes from
  path-based `engine.open(&path)` to bytes-based
  `engine.open_bytes(store.read(&path)?)` — verify this preserves
  desktop behavior (the engine re-reads the same bytes it would have
  read from disk; the store wrote them one line earlier) and note that
  `Document::open(&path)` inside `reopen_after_write` must likewise
  become `Document::from_bytes(&store.read(...)?)`.
- The `#[cfg(not(target_arch = "wasm32"))]` fence moves: path-based
  `open_document_impl`/`save_document_impl`/`save_document_as_impl` stay
  desktop-only (they genuinely touch the user's real files), but
  `commit_mutation`/`undo_impl`/`redo_impl` become fully portable.

- [ ] **Step 1: Write the failing test** — a `MemWorkingStore` round-trip
  in the session crate: open bytes → `commit_mutation` (delete a page,
  as the existing undo/redo tests do) → `undo_impl` → `redo_impl`,
  asserting page counts at each step — the EXISTING undo/redo test
  sequence, but running against `MemWorkingStore` with a byte-opened
  document (no temp files anywhere in the test).
- [ ] **Step 2: Verify it fails** (no WorkingStore yet).
- [ ] **Step 3: Implement** as specified above.
- [ ] **Step 4: Full verification battery** (constraints block) — the
  existing fs-based undo/redo tests must still pass unchanged (they now
  run through `FsWorkingStore`), plus the new mem test, plus wasm32 build.
- [ ] **Step 5: Commit** — `openpdfedit-session: WorkingStore abstraction — fs on desktop, in-memory for wasm`

---

### Task 2: Dirty tracking + document-close lifecycle

**Files:**
- Modify: `crates/openpdfedit-session/src/lib.rs` (dirty flag already
  exists in `OpenedDocumentInfo`; verify it becomes true after
  `commit_mutation` for byte-opened docs and false after save)
- Modify: `apps/desktop/src/lib/backend/tauri.ts`, `wasm.ts`, and the
  relevant session/wasm plumbing: on successful `openDocument`, close
  the PREVIOUS document's handle (both backends; final-review finding 7)
- Modify: `apps/desktop/src/lib/backend/wasm.ts`: `saveDocument` for a
  byte-opened doc = `saveToBytes` + write to the retained
  `FileSystemFileHandle`, then mark clean (this is where `is_dirty`
  returns to false; check what the session's save path does for the
  desktop and mirror the semantics)

- [ ] **Step 1: Failing test** — session crate: byte-open → mutate →
  `OpenedDocumentInfo.is_dirty == true`; save-to-bytes → the NEXT info
  refresh reports `is_dirty == false` (add a session-level
  `mark_saved`/equivalent if the current save path doesn't already do
  this for byte-opened docs — read the code first; the desktop's save
  already flips it, mirror that mechanism).
- [ ] **Step 2-4: implement, verify (battery incl. boot e2e), commit** —
  `openpdfedit-session: real dirty tracking for byte-opened docs + close previous doc on open`

---

### Task 3: wasm-bindgen annotation/undo surface

**Files:**
- Modify: `crates/openpdfedit-wasm/src/lib.rs`

**Interfaces:**
- `WasmSession` gains: `addAnnotation(handle, requestJson) → JSON OpenedDocumentInfo`,
  `deleteAnnotation(handle, requestJson) → JSON`, `listPageAnnotations(handle, pageIndex) → JSON`,
  `textSelectionQuads(handle, requestJson) → JSON`, `undo(handle) → JSON`,
  `redo(handle) → JSON` — thin serde_json marshaling over the session
  crate's `add_annotation_impl`/`delete_annotation_impl`/
  `list_page_annotations_impl`/`text_selection_quads_impl`/`undo_impl`/
  `redo_impl` (all landed in Phase 1; now portable via Task 1-2).
  Request DTOs deserialize from the same JSON `types.ts` sends the Tauri
  commands (the DTOs already derive Deserialize for Tauri — verify).
- Regenerate the wasm-gen glue (`apps/extension/scripts/build-wasm.sh`)
  and confirm the `.d.ts` gains the new methods.

- [ ] Steps: failing wasm32 build check → implement → battery (wasm32
  build REQUIRED; regenerated glue verified) → commit —
  `openpdfedit-wasm: annotation + undo/redo surface over the session crate`

---

### Task 4: `wasm.ts` annotation methods + Save re-enabled

**Files:**
- Modify: `apps/desktop/src/lib/backend/wasm.ts` — replace the
  "not yet ported" throws for: `addAnnotation`, `deleteAnnotation`,
  `listPageAnnotations`, `textSelectionQuads`, `undo`, `redo` with real
  calls through the (typed) session handle, JSON-parsing to the same DTO
  types `tauri.ts` returns. Update the hand-typed `WasmSessionHandle`
  interface from the regenerated `.d.ts` (diff them; a mismatch is a
  finding, not a patch-over).

- [ ] Steps: implement → verification battery (both build flavors,
  `npm run check`, boot e2e) → MANUAL-ADJACENT static check: trace one
  full annotation flow (+page.svelte `handleCreateAnnotation` →
  backend.addAnnotation → wasm → session → back to `doc` reassignment)
  on paper in the report, confirming handle rotation and annotation
  refresh both work with the wasm backend's return values → commit —
  `openpdfedit: extension annotation surface live (wasm.ts)`

---

### Task 5: Phase 2 battery + boot-e2e extension + ledger

- [ ] Extend `apps/extension/e2e/boot.spec.ts` minimally: after boot,
  assert the annotation toolbar renders (the `TOOLS` buttons exist).
  (A full open-PDF-and-annotate e2e needs a fixture PDF through the
  File System Access picker — genuinely hard headlessly; do NOT build
  a CDP-mocked picker in this phase; note it as future work.)
- [ ] Full battery (Phase 1's Task 10 list + boot e2e), prune-as-needed.
- [ ] Ledger: Phase 2 completion entry + notes for Phase 3 (Engine trait
  form-ops extension is the known prerequisite).
- [ ] Commit docs/ledger updates.

---

## Phase 3 preview (planned after Phase 2 lands)
Forms + page organization: extend the `Engine` trait with
`list_form_fields`/`fill_form_fields` (the verified Phase 3 boundary),
then the same wasm/wasm.ts wiring pattern for forms, field creation,
rotate/delete/move/crop/extract/merge (bytes cores already landed).
