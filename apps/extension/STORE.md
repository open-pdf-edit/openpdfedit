# Extension store listing — OpenPdfEdit

The listing copy for the two stores that take this package: the Chrome
Web Store and Microsoft Edge Add-ons. Both accept the same MV3 zip
unmodified, and the copy below serves both — only the dashboards differ.

Submitted to Edge and rejected once; see the note below and
`docs/STORES.md` for the operational steps.

**Keep this honest.** Reviewers compare what a listing claims against
what the bundle does, and a claim that is merely out of date reads
exactly like one that was never true. Check claims against the *built*
`dist/`, not against the source tree — the two differ. The listing
briefly claimed OCR worked here because `ocr-browser.ts` exists and the
web app ships it; the extension build does not copy the `/ocr` assets,
so it does not.

An Edge review then failed the submission over the same class of
mistake, one level down: the account panel offered a Sign in that opened
`/login`, a path only a server can resolve, so the reviewer got "File
not found" and reported the primary functions as unusable. The extension
has no account surface at all now, which makes "no account required"
true again without qualification — see `isBrowserExtension`.

## Listing copy

### Short description (≤132 characters — both stores' own limit)

```
Edit, merge, compare, sign, and redact PDFs right in your browser. Your documents never leave your machine.
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

Free, with no exceptions:
- Everything above is free. There is no account to create, nothing to
  sign in to, nothing time-limited, nothing watermarked, and no caps.

Two things this extension does not do, both of which the web app at
app.openpdfedit.com and the desktop app do: OCR, and watermarking. OCR
needs Tesseract's engine and language data — about 70 MB — which is the
wrong thing to put in an extension. Watermarking is a paid Supporter
tool, and paid tools need an account, which needs a sign-in flow a
browser extension cannot host. Rather than ship either as a button that
leads nowhere, this build leaves them out.

Privacy, by construction, not by policy:
- 100% local PDF processing — your document is never uploaded anywhere,
  opened and edited entirely by an in-browser WASM engine
- No analytics, no tracking, no telemetry — nothing about your usage is
  ever collected
- No remote code — everything the extension runs shipped inside the
  extension package you installed; it does not fetch or `eval` code
  from anywhere at runtime (this is also enforced by its Content-Security-
  Policy, not just a claim: `script-src 'self' 'wasm-unsafe-eval'`)

Everything in the feature list above is live in this extension. OCR and
watermarking, as noted, are not in it at all.
```

### Category

**Productivity** (Chrome Web Store's category for document/office
tooling — PDF editors from other vendors list here too).

### Privacy declaration (for the Store's "Privacy practices" tab)

This used to need care, because the account panel was a real network
surface even though the PDF editor was not. That panel is gone from this
build, so the simple claim is now the true one: nothing here talks to
anything.

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
- **No network activity at all.** This build has no account surface: no
  sign-in, no credits, no purchases. Those exist in the web app and the
  desktop app, and are deliberately absent here, because the sign-in flow
  they need cannot be hosted inside an extension. Nothing in this package
  contacts any server, for any reason, at any point. Verified rather than
  asserted: with every non-extension request blocked outright, the
  extension still loads and opens, edits and saves a document, because it
  never asks for one.
- **Data collected: none.** No analytics, no crash reporting, no
  telemetry, no remote logging.
- **Remote code:** none. The CSP (`content_security_policy` in
  `manifest.json`) disallows inline scripts and restricts script sources
  to `'self'` plus `'wasm-unsafe-eval'` (required to instantiate the
  bundled WASM module) — there is no `eval`, no remotely-fetched script,
  and no third-party script host permitted, enforced by Chrome itself,
  not just declared.
- **Permissions requested:** none beyond what MV3 grants any extension by
  default (this manifest declares no `permissions` array at all — no
  `tabs`, `storage`, `activeTab`, host permissions, etc.). File access
  goes through the File System Access API's native browser file picker,
  which the user drives directly; the extension never gets ambient
  access to the filesystem.

## Submission checklist

Already true of the repository:
- [x] `npm run package` produces a store-shaped zip with `manifest.json`
      at its root — run it fresh immediately before uploading, since the
      zip is gitignored and a stale one looks identical to a current one
- [x] **Zero permissions.** The manifest declares no `permissions` and
      no `host_permissions`, so both dashboards' "justify each
      permission" step is empty. This is the single biggest thing in
      this submission's favour and it is worth not spending: think hard
      before any future change adds one.
- [x] **Privacy policy URL** — <https://openpdfedit.com/privacy.html>,
      live and current
- [x] **128×128 listing icon** — `apps/extension/public/icons/128.png`,
      which both dashboards want uploaded separately from the manifest's
      own icon

Human-only — nothing in this repo can do these:
- [ ] **Screenshots.** At least one at 1280×800 or 640×400, showing the
      extension open on a real PDF. Nothing here can produce them: the
      e2e suite is headless and is not a marketing tool. The same set
      serves both stores.
- [ ] **Chrome Web Store developer account** —
      <https://chrome.google.com/webstore/devconsole>, one-time
      registration fee
- [ ] **Microsoft Partner Center account** for Edge Add-ons — free to
      register for the Edge program, and the same account is later used
      for the Microsoft Store desktop submission
- [ ] **Fill in each dashboard's listing form** with the copy above, and
      the privacy declaration below in the privacy-practices tab
- [ ] **Upload `openpdfedit-dist.zip`** and submit

After the first submission of each, releases are automated: see
`docs/STORES.md` and `scripts/publish-edge.sh`. Neither store's API can
make the *first* submission — Edge's has no endpoint that creates a
product at all — so the list above is genuinely once-only, not a
process.

## Producing the upload zip

```bash
npm run package
```

This runs `npm run build` (fresh `dist/`) then zips `dist/`'s *contents*
(not a nested `dist/` folder — Chrome's uploader expects `manifest.json`
at the zip's root) into `openpdfedit-dist.zip` in this package's
root. See `package.json`'s `package` script and
`scripts/package-zip.sh` for the exact zip invocation.
