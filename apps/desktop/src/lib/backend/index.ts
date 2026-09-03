// Chooses which `Backend` implementation the app runs against. Default
// (and the only option today) is the Tauri desktop backend; `wasm.ts`
// (currently a Task-8 placeholder — see that file's doc) is the Chrome
// extension build, selected at build time via `VITE_BACKEND=wasm`. See
// `initBackend`'s doc below for exactly how the desktop build ends up
// shipping none of `wasm.ts` while the wasm build ships all of it —
// verified empirically, not just asserted, because the first mechanism
// tried here (a variable-routed specifier + `/* @vite-ignore */`) looked
// reasonable but was actually broken for the wasm build (see git history
// on this file/task-7-report.md for what that looked like and why).
import type { Backend } from "./types";
import { tauriBackend } from "./tauri";

export type {
  AddAnnotationRequest,
  AnnotationKindDto,
  AnnotationSummaryDto,
  Backend,
  CompareDocumentsRequest,
  CompareReportDto,
  CreateFormFieldRequest,
  DeleteAnnotationRequest,
  EditTextRunRequest,
  EncryptChoices,
  EncryptStats,
  ExportXfdfResult,
  ExtractPagesRequest,
  ImportXfdfResult,
  FillFormFieldsRequest,
  FlattenDocumentRequest,
  FlattenResultDto,
  FormFieldDto,
  FormFieldKindDto,
  FormFieldOptionDto,
  ImagePlacementDto,
  MergeDocumentsRequest,
  MoveImageRequest,
  MoveTextRunRequest,
  OcrDocumentRequest,
  OpenedDocument,
  PageBitmap,
  PageMoveDirection,
  PageSize,
  PixelPageDiffDto,
  NumberPagesChoices,
  OutlineEntryDto,
  RedactPageRequest,
  SearchHitDto,
  SearchRequest,
  SearchResultsDto,
  SignatureInfoDto,
  TextPageDiffDto,
  TextRunDto,
  TextSelectionQuadsRequest,
} from "./types";

// `backend` defaults to the Tauri implementation synchronously (no async
// step needed for that branch at all) so every module that does
// `import { backend } from "$lib/backend"` gets a working value even if
// somehow evaluated before `initBackend()` below runs. It's a mutable
// `let`, not a `const`: ES module bindings are live, so once
// `initBackend()` reassigns it (only the `VITE_BACKEND=wasm` branch ever
// does), every importer sees the new value on their next property access
// — there's no re-import needed, as long as call sites read `backend.foo`
// live rather than destructuring it into a local at import time (every
// call site in this app does).
/** Whether a failed `openDocument` was refused for want of a password,
 * as opposed to genuinely failing. The Tauri backend serializes
 * `CommandError::PasswordRequired` as the string `"password required"`;
 * matching on that rather than on any message the parser produced keeps
 * the check from drifting when unrelated error text changes. */
export function isPasswordRequired(error: unknown): boolean {
  const text =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : typeof error === "object" && error !== null
          ? Object.values(error as Record<string, unknown>).find((v) => typeof v === "string") ?? ""
          : "";
  return String(text).toLowerCase().includes("password required");
}

export let backend: Backend = tauriBackend;

/** Which `Backend` implementation this build ships, decided at build time
 * from the same `VITE_BACKEND` env var `initBackend()` below branches on
 * — a plain compile-time-constant `const`, not a live reflection of
 * `backend`'s current value, so call sites that only need to gate UI on
 * backend *capability* (e.g. hiding the OCR button, which `wasm.ts`
 * genuinely can't support — see that file's doc comment) don't need to
 * wait for `initBackend()` to resolve first, unlike `backend` itself.
 * `backendKind !== "wasm"` is deliberately not `=== "tauri"`: an entry
 * that's neither would (rightly) hide wasm-unsupported UI rather than
 * show it. */
export const backendKind: "tauri" | "wasm" =
  import.meta.env.VITE_BACKEND === "wasm" ? "wasm" : "tauri";

/**
 * Running as the packaged browser extension rather than the web app.
 *
 * `backendKind` cannot answer this: both are built with
 * `VITE_BACKEND=wasm` from the same source and ship the same bundle. Only
 * the origin they are loaded from tells them apart, so this is a runtime
 * check and not a compile-time constant.
 *
 * What it gates is the account surface. Signing in works on the web app
 * and cannot work in the extension: the flow opens `/login`, which the
 * web app's server rewrites to the SPA fallback, while
 * `chrome-extension://` has no server and answers a literal file lookup —
 * Chrome's "File not found. It may have been moved, edited, or deleted."
 * That is what an Edge reviewer hit, and it failed the submission.
 *
 * Making the page resolve would only move the failure: the sign-in page
 * redirects the whole window out to the OAuth provider and back, and a
 * `chrome-extension://` redirect URI is not something those providers
 * accept. The honest answer for this build is not to offer an account at
 * all — which costs it only the two Supporter tools, one of which
 * (OCR) the extension cannot perform anyway, since its engine and
 * language data are not packaged.
 */
export const isBrowserExtension =
  typeof location !== "undefined" &&
  (location.protocol === "chrome-extension:" || location.protocol === "moz-extension:");

/** Resolves which `Backend` implementation to use and installs it as
 * `backend` above. Must be awaited once, before the app mounts anything
 * that might call into `backend` — see +layout.svelte, which awaits this
 * and only renders `children()` once it settles. Top-level await in this
 * module would do the same thing more directly, but esbuild's configured
 * target for the desktop build (chrome87/edge88/es2020/firefox78/
 * safari14) doesn't support it (`npm run build` fails with "Top-level
 * await is not available in the configured target environment" if tried)
 * — this is the brief's documented fallback for that case.
 *
 * `import("./wasm")` uses a LITERAL specifier (no `/* @vite-ignore *\/`,
 * no variable indirection) so Vite/Rollup can actually resolve it into a
 * real, separately-chunked module — that's what makes the wasm build
 * ship working code instead of a raw, never-resolved `import("./wasm")`
 * call that would 404 at runtime (empirically confirmed: that's exactly
 * what a `/* @vite-ignore *\/`-routed specifier produces, since it tells
 * Vite not to touch the import at all, in *every* build, not just the
 * desktop one). With a literal specifier instead, Rollup's own
 * tree-shaking sees `import.meta.env.VITE_BACKEND === "wasm"` reduce to
 * a compile-time-constant `false` whenever `VITE_BACKEND` isn't set to
 * exactly `"wasm"`, and dead-code-eliminates the whole `if` block —
 * `import()` call included — before chunking, so the desktop build
 * contains zero bytes of `wasm.ts` (verified: `grep`ping the desktop
 * build output for `wasm.ts`'s content finds nothing, and no `wasm`-named
 * chunk is emitted). With `VITE_BACKEND=wasm` at build time, the same
 * literal specifier resolves for real and `wasm.ts` is emitted as its
 * own chunk (verified: it shows up as `chunks/wasm.js` server-side and
 * as its own client chunk, containing `wasm.ts`'s actual compiled code).
 *
 * The one requirement this trades in for that: `./wasm` has to exist as
 * a real, resolvable module at all times, even before Task 8 — Rollup
 * resolves every `import()` call while building the module graph,
 * *before* any dead-code elimination of the branch around it runs, so a
 * literal specifier pointing at a nonexistent module fails the build
 * outright (empirically confirmed too) regardless of whether that
 * branch is ever taken. `wasm.ts` is therefore a real (if placeholder)
 * file starting now rather than only once Task 8 lands — see its own
 * doc comment. */
export async function initBackend(): Promise<Backend> {
  if (import.meta.env.VITE_BACKEND === "wasm") {
    const mod = await import("./wasm");
    backend = mod.wasmBackend as Backend;
  }
  return backend;
}
