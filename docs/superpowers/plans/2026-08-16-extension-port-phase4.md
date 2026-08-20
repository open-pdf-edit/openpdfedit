# Extension Port Phase 4 (Signatures, Redaction, Text/Image Editing) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The extension gains the desktop's remaining single-document
editing surface — signature listing/placement, page redaction, text-run
edit/move, image move — and the session crate gets per-key write
serialization (the named Phase 3 opener).

**Architecture:** the established pattern end-to-end. Three named
prerequisites from Phase 3's ledger are Tasks 1-2; the rest is wiring.

**Tech Stack / Global Constraints:** identical to Phase 3's plan
(real-PDFium ran-vs-skipped evidence; never touch other projects' /tmp;
DTO shapes frozen; no async/Workers/single-init-guard changes; both e2e
specs green after every task).

## Named prerequisites (from Phase 3's closing ledger)

1. **Per-key `store.write` serialization (opener):** `undo_impl`/
   `redo_impl`/`fill_form_fields_impl` release all locks before
   `store.write`; concurrent same-key writers are clean last-writer-wins
   today (unique tmp suffixes), but serialization makes the outcome
   deterministic. Design latitude: hold the `docs` lock across the write
   (matching `commit_mutation`'s discipline) OR a per-key lock map in
   `SessionState` — implementer investigates deadlock-freedom against
   existing lock ordering and documents the choice.
2. **`signatures::list_signatures_impl` store-routing:** currently
   cfg-fenced (does `std::fs::read(&path)`); the portable
   `list_signatures_in_bytes` exists — reroute reads through
   `store.read`, un-fence, genericize.
3. **Fill-invalidates-signatures:** already documented and the UI
   refetches after fill (Phase 3 fix wave); Phase 4's signature UI work
   must keep that behavior intact (no new work expected — verify).

---

### Task 1: Per-key write serialization (the opener)

**Files:** `crates/openpdfedit-session/src/lib.rs` (+ `forms.rs` if fill's flow changes)

Investigate and implement; failing test first if a deterministic test is
constructible (two threads racing undo vs a commit_mutation on one doc —
the session crate's tests use one shared engine; a loom-style test is NOT
expected, a best-effort deterministic interleaving via std threads +
barriers is acceptable, or if genuinely untestable deterministically,
document why and rely on the lock-discipline argument + existing suite).
Desktop behavior unchanged; wasm32 unaffected structurally (single-threaded)
but must still build.

- [ ] Investigate → (test) → implement → full session/engine suites + wasm32 + clippy/fmt → commit.

---

### Task 2: Portable signatures listing

**Files:** `crates/openpdfedit-session/src/signatures.rs`

Reroute `list_signatures_impl` through `store.read`, genericize, un-fence;
portable test (MemWorkingStore: open bytes of a signature-bearing fixture —
check what fixture the existing fs test uses and reuse its byte source).

- [ ] TDD → implement → suites + wasm32 → commit.

---

### Task 3: wasm + wasm.ts surface for signatures/redact/textedit/image

**Files:** `crates/openpdfedit-wasm/src/lib.rs`, `apps/desktop/src/lib/backend/wasm.ts`

WasmSession gains: `listSignatures(handle)`, `redactPage(requestJson)`,
`listTextRuns(handle, pageIndex)`, `editTextRun(requestJson)`,
`moveTextRun(requestJson)`, `listImagePlacements(handle, pageIndex)`,
`moveImage(requestJson)` — arg shapes VERIFIED per command against
tauri.ts + the desktop wrappers (the plan's shorthand has been wrong
twice; reality wins). Mutating returns = OpenedDocumentInfo +
migrateOpenDoc. Signature PLACEMENT: trace how the desktop places a drawn
signature (+page.svelte handlePlaceSignature → which backend method?) —
if it rides the already-live addAnnotation path, no new wasm work; trace
and document either way; if it needs a new method, add it following the
pattern. Glue regenerated + .d.ts diff. wasm.ts replaces the
corresponding notImplemented throws (incl. listSignatures — the
refetch-after-fill path goes live).

- [ ] Implement → battery (both e2e specs) + on-paper traces (redact flow, text-edit flow) → commit.

---

### Task 4: Phase 4 battery + e2e extension + ledger

- [ ] Extend `wasm-session.spec.ts` cheaply: redactPage + listTextRuns/editTextRun ops on the fixture (it has a text field; check what text RUNS exist on it — if the fixture lacks page text, extend the embedded fixture or add a second minimal one; document).
- [ ] Full battery; ledger with real numbers; Phase 5 notes (merge + compare multi-file flows; packaging/store checklist; the parked Phase-1 cleanup minors).
- [ ] Commit.
