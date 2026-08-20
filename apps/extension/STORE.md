# Chrome Web Store listing — OpenPdfEdit

Phase 5 Task 3 (packaging). This is the store-listing draft plus the
submission checklist. Nothing here has been submitted anywhere yet — see
the checklist at the bottom for what's still a human step.

## Listing copy

### Short description (≤132 characters — Chrome Web Store's own limit)

```
Edit, merge, compare, and sign PDFs 100% locally in your browser. No upload, no account, no tracking.
```

(101 characters.)

### Long description

```
OpenPdfEdit is a full PDF editor that runs entirely inside your browser
tab. There is no server: every PDF you open is processed by an
in-browser WASM engine (PDFium + OpenPdfEdit's own Rust crates, compiled
to WebAssembly) and never leaves your machine.

What you can do:
- View and annotate — highlight, underline, strikeout, freehand ink,
  sticky-note comments
- Edit text runs and move images directly on the page
- Fill and create form fields
- Draw and place signatures
- Organize pages — rotate, delete, reorder, crop, extract a page range
  into a new document
- Redact — permanently remove sensitive content, not just paint over it
- Merge multiple PDFs into one
- Compare two PDFs — see both text and rendered-pixel differences
- Undo/redo across your whole editing session

Privacy, by construction, not by policy:
- 100% local PDF processing — your document is never uploaded anywhere,
  opened and edited entirely by an in-browser WASM engine
- No account required to edit — every editing feature above works with
  no sign-in at all
- An optional account panel (for credits/purchases, not editing) talks to
  OpenApps' own account server only if and when you choose to sign in —
  see the privacy declaration below for exactly what that does and
  doesn't send
- No analytics, no tracking, no telemetry — nothing about your usage is
  ever collected
- No remote code — everything the extension runs shipped inside the
  extension package you installed; it does not fetch or `eval` code
  from anywhere at runtime (this is also enforced by its Content-Security-
  Policy, not just a claim: `script-src 'self' 'wasm-unsafe-eval'`)

One current gap: OCR (making a scanned PDF searchable) needs a local
Tesseract install and isn't available in this browser build — it's the
one feature still exclusive to the OpenPdfEdit desktop app. Everything
else above is fully live in the extension.
```

### Category

**Productivity** (Chrome Web Store's category for document/office
tooling — PDF editors from other vendors list here too).

### Privacy declaration (for the Store's "Privacy practices" tab)

This has two genuinely different parts, and the declaration below is
deliberately precise about which is which — a blanket "no data leaves the
browser" claim would be false and is a real rejection/compliance risk, not
just imprecise wording: the optional Account panel is a real network
surface, even though the PDF editor itself is not.

- **PDF/document processing: 100% local, always, no exceptions.** Every
  editing feature — view, annotate, edit text, fill/create form fields,
  sign, redact, reorganize pages, merge, compare — runs against an
  in-browser WASM PDF engine. The document you open, and everything you
  do to it, never leaves the browser: no upload, no server round-trip, no
  exception for any editing feature. Opened PDFs are held in the
  extension's own in-memory WASM engine and, if saved, written back to
  disk via the browser's File System Access API (a direct user-initiated
  file write, not a network request). This holds regardless of whether
  the optional account feature below is used at all.
- **Optional account panel — network activity, but not document data.**
  The account/credits UI (sign in, view balance, buy credits) is a
  separate, optional surface, not part of PDF editing. **With no session
  (not signed in — the default state), it renders a single "Sign in"
  button and makes no network requests at all**: the three underlying
  components (`<openapps-account>`, `<openapps-credits
  poll-seconds="30">`, `<openapps-buy>`) are not even mounted into the
  page unless the app's own `loggedIn` check is already true (see
  `apps/desktop/src/lib/AccountPanel.svelte`'s `{#if loggedIn}` gate —
  confirmed by reading that component's template, not assumed), and each
  of those components independently no-ops instead of calling the network
  when there is no signed-in session (confirmed by reading
  `openapps-credits.ts`'s `refresh()`, which checks `sdk.isLoggedIn`
  before ever calling `sdk.credits.balance()`). Configuring the shared
  SDK client at app startup (`configure({ baseUrl:
  "https://accounts.openapps.network" })` in `+layout.svelte`) only
  constructs a local client object and reads a local token store — it
  does not itself make a network request either (confirmed by reading
  `OpenApps`'s constructor in the SDK). **If and only if you choose to
  sign in**, that panel communicates with
  `accounts.openapps.network` solely for account/credit-balance/purchase
  functionality (checking your session, showing your credit balance,
  processing a credit purchase). No document content or metadata is ever
  sent to it — the PDF engine and the account client are two unconnected
  code paths that never pass document data to each other.
- **Data collected: none.** No analytics, no crash reporting, no
  telemetry, no remote logging — from either the PDF editor or the
  account panel.
- **Remote code:** none. The CSP (`content_security_policy` in
  `manifest.json`) disallows inline scripts and restricts script sources
  to `'self'` plus `'wasm-unsafe-eval'` (required to instantiate the
  bundled WASM module) — there is no `eval`, no remotely-fetched script,
  and no third-party script host permitted, enforced by Chrome itself,
  not just declared. This applies to the account panel's network calls
  too: they're plain `fetch()` API calls for JSON data, not script
  loading.
- **Permissions requested:** none beyond what MV3 grants any extension by
  default (this manifest declares no `permissions` array at all — no
  `tabs`, `storage`, `activeTab`, host permissions, etc.). File access
  goes through the File System Access API's native browser file picker,
  which the user drives directly; the extension never gets ambient
  access to the filesystem.

## Submission checklist

Automated / already done by this task:
- [x] `npm run build` produces a current `dist/`
- [x] `npm run package` zips `dist/` into `openpdfedit-dist.zip`
      (run it fresh right before uploading — see below)

Human-only — nothing in this repo can do these:
- [ ] **Chrome Web Store developer account** — register at
      https://chrome.google.com/webstore/devconsole (one-time $5 registration
      fee at time of writing) if not already done for the OpenApps org
- [ ] **Screenshots** — the Store requires at least one 1280x800 or
      640x400 screenshot; capture the extension actually open in a real
      Chrome tab with a PDF loaded (a promotional tile image is optional
      but recommended). Nothing in this repo can drive a *visible* browser
      and capture a polished marketing screenshot — the e2e suite runs
      headless and is not meant to produce store assets.
- [ ] **Store listing icon** — the Store dashboard also wants a
      standalone 128x128 icon upload separate from the manifest's own
      icon; `apps/extension/public/icons/128.png` (generated by
      `scripts/generate-icons.sh`) can be reused directly for this.
- [ ] **Privacy policy URL** — the Store requires a hosted privacy policy
      page if any permission implies data access; given the "no data
      collection" declaration above this may qualify for the Store's
      simplified flow, but the dashboard will say definitively once the
      listing form is actually filled in — read what it asks for at
      submission time rather than guessing here.
- [ ] **Fill in the dashboard's listing form** with the copy above (short
      description, long description, category, privacy practices tab)
- [ ] **Upload `openpdfedit-dist.zip`** via the dashboard's package
      upload step
- [ ] **Submit for review** and monitor the review outcome (Chrome Web
      Store reviews typically take hours to a few days; a rejection needs
      a human to read the specific reason and decide the fix — not
      something to pre-guess here)

## Producing the upload zip

```bash
npm run package
```

This runs `npm run build` (fresh `dist/`) then zips `dist/`'s *contents*
(not a nested `dist/` folder — Chrome's uploader expects `manifest.json`
at the zip's root) into `openpdfedit-dist.zip` in this package's
root. See `package.json`'s `package` script and
`scripts/package-zip.sh` for the exact zip invocation.
