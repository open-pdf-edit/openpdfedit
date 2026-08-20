# Extension Port Phase 5 (Merge/Compare + Packaging) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the port: the extension gains merge and compare (the two
multi-file flows), the UI hides what the extension genuinely cannot do,
the Phase 4 one-liners land, the parked Phase-1 cleanup minors are swept,
and the extension is packaged to a store-submittable state.

**Global Constraints:** identical to Phase 4's plan (real-PDFium
ran-vs-skipped; both e2e specs green after every task; never touch other
projects' /tmp; DTO freeze applies to EXISTING shapes — new byte-flow
request DTOs for merge/compare are additive and get camelCase serde like
their siblings; no async/Workers/single-init changes).

## Carried-forward inputs (from Phase 4's closing ledger)
- Phase 4 one-liners (Task 1 below).
- Merge byte-plumbing semantic: desktop's `MergeRequest.open_handle`
  merges the LIVE working copy of the open doc — the byte variant must
  preserve that (read bytes via store, not a re-read of the original).
- Compare: desktop takes two arbitrary paths (docs-map-independent);
  wasm variant = two byte buffers via two picks.
- Open store/session residuals (double-rotation; fill lost-update) are
  NOT Phase 5 scope (candidate fix: per-key lock spanning the op —
  post-port follow-up); ocr.rs atomicity ticket likewise.
- Packaging must include: hide/gate backend-unsupported commands
  (OCR button; anything left dead), extension icons, store listing
  text, manifest polish, the packaging checklist.
- Non-goals stay: OCR, Firefox.

---

### Task 1: Phase 4 one-liners + UI gating sweep

**Files:** `apps/desktop/src/routes/+page.svelte`, `apps/desktop/src/lib/PagesPanel.svelte` (maybe), ledger

- `handleSave`/`handleSaveAs`/⌘S entry-gated on `mutationBusy` (closes the
  stale-handle doc-clobber hazard).
- `mutationBusy` surfaced visually: OR it into the relevant `disabled=`/
  `busy=` bindings (toolbar undo/redo/save, panels) so gated input isn't
  silently swallowed.
- Fix the stale `logRefreshFailure` comment; fix the Phase-4 ledger's
  line-43 imprecision (report-side note, no code).
- Battery: desktop check + both flavors; extension build + both e2e.

---

### Task 2: Merge + compare byte plumbing (session + wasm + wasm.ts)

**Files:** `crates/openpdfedit-session/src/{pages,compare}.rs`, `crates/openpdfedit-wasm/src/lib.rs`, `apps/desktop/src/lib/backend/wasm.ts`

- Session: additive byte-flow entry points — merge: portable
  `merge_open_doc_with_bytes(state-parts, open_handle, sources: Vec<Vec<u8>>) -> bytes`
  (preserving the live-working-copy semantic: the open doc's bytes come
  from `store.read`, per the carried-forward note; reuse `merge_bytes`'s
  core); compare: `compare_bytes` already exists and is portable (verify)
  — wasm needs only the marshaling.
- Wasm: `mergeDocuments(requestJson-with-handle + source bytes — wasm-bindgen
  can't take Vec<Vec<u8>> directly; design the boundary: e.g. a two-step
  API (beginMerge(handle) + addMergeSource(bytes) + finishMerge() -> bytes)
  or a single call with a length-prefixed concatenated buffer — pick the
  simplest that stays synchronous, document it), `compareDocuments(bytesA, bytesB, requestJson-options) -> CompareReportDto JSON`.
- wasm.ts: `mergeDocuments` picks source files (pickOpenPaths exists),
  reads their bytes, drives the wasm API, saves the merged output via
  save picker, opens it (mirroring extractPages' landed pattern);
  `compareDocuments`: the desktop flow picks two paths — read types.ts's
  CompareRequest shape and adapt (the extension picks two files, passes
  bytes; the REPORT rendering is shared UI and must work unchanged).
- TDD session-side; arg shapes verified against reality; migrateOpenDoc
  where the open doc rotates (merge doesn't rotate the open doc on
  desktop — verify and match); glue regenerated + .d.ts diff; on-paper
  traces both flows.
- Battery incl. both e2e specs.

---

### Task 3: Packaging to store-submittable

**Files:** `apps/extension/public/manifest.json`, `apps/extension/public/icons/*` (new), `apps/extension/README.md`, `apps/extension/STORE.md` (new), `apps/desktop/src/routes/+page.svelte` (gating)

- Hide/gate backend-unsupported commands: OCR button hidden on the wasm
  backend (a `backend.capabilities` or a simple `isExtension` export from
  the backend module — read how VITE_BACKEND selection works and pick the
  minimal mechanism; document). Merge/Extract/Compare become LIVE in
  Task 2 so only OCR (and the Tauri-only login popup, already toasting
  honestly) remain gated.
- Extension icons: generate from the existing brand assets
  (apps/desktop/src-tauri/icons/ has the app icon; the openapps-design
  system's brand mark — 16/48/128 PNGs; a simple script or one-time
  generation, committed).
- manifest.json polish: name ("OpenPdfEdit"), description, version 0.1.0,
  icons wired; verify CSP/minimum_chrome_version stay.
- STORE.md: the store-listing text (short + long description, category,
  privacy declaration — local-only processing, no data collection) + the
  submission checklist (zip dist/, store dashboard steps, screenshots
  needed — the parts only a human can do stay clearly marked as such).
- Battery + both e2e; a fresh `dist/` zip built and its byte size noted.

---

### Task 4: Phase-1 parked minors sweep + port-closing battery + final ledger

- Sweep the parked Phase-1 adjudications (from ../2026-08-15-extension-port-phase1/progress.md):
  error-mapping convention unification (From impls vs inline map_err),
  AnnotationSummaryDto rename_all, contradictory "unlike other modules"
  module docs, handle-resolution boilerplate helper, duplicated test
  fixtures into test_support, session Cargo.toml comment. Judgment on
  each: fix if mechanical, park-with-reason if risky.
- Full port-closing battery; ledger: PORT COMPLETE entry with the
  definitive feature matrix (what works in the extension, what's gated,
  what's follow-up) — this becomes the input for the user's one manual
  test pass.
- Commit.
