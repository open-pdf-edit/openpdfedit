# Extension store listing — OpenPdfEdit

The listing copy for the two stores that take this package: the Chrome
Web Store and Microsoft Edge Add-ons. Both accept the same MV3 zip
unmodified, and the copy below serves both — only the dashboards differ.

Submitted to Edge once and rejected; see the note below. For the
operational steps — registering, uploading, and the automation that
handles every release after the first — see `docs/STORES.md`.

**Keep this honest.** Reviewers compare what a listing claims against
what the bundle does, and a claim that is merely out of date reads
exactly like one that was never true. Check claims against the *built*
`dist/`, not against the source tree — the two differ. The listing
briefly claimed OCR worked here because `ocr-browser.ts` exists and the
web app ships it; the extension build does not copy the `/ocr` assets,
so it does not. Corrected below.

Also corrected: "no account required" stopped being true when
watermarking became a Supporter tool. If the gate in `lib/openapps.ts`
moves again, this file moves with it.

The Edge rejection was the same class of error one level down, and in
code rather than copy: Sign in opened a *relative* `/login`, which the
web app resolves against its own origin and `chrome-extension://` cannot
resolve at all. Sign-in now opens the web app's login page at its real
origin and the finished session is handed back to the extension. Worth
knowing when writing the privacy declaration below: the extension is
reachable from `openpdfedit.com` for that hand-back, declared as
`externally_connectable` — a manifest key, not a permission.

## Listing copy

### Short description (≤132 characters — both stores' own limit)

```
Edit, read, merge, redact and OCR PDFs entirely in your browser. Add watermarks, fill forms, sign, compress and more.
```

(101 characters.)

### Long description and search terms — nineteen languages

In [`store/listings/`](store/listings/), one folder per catalogue in
`public/_locales/`:

| File | Pastes into |
|---|---|
| `description-chrome.txt` | Chrome Web Store → Store listing → Description, for that language |
| `description-edge.txt` | Edge Partner Center → Store listings → Details for that language → Description |
| `search-terms-edge.txt` | the same Edge page → Search terms, one term per line |

[`store/listings/locales.json`](store/listings/locales.json) maps each folder to
the code or label each store's language picker uses. One of them differs:
the package's `pt` catalogue is Brazilian Portuguese, and the Chrome Web
Store lists only `pt_BR` and `pt_PT`, so that listing goes under `pt_BR`.

Neither store takes these through an API — they reach a store only by
being pasted into a form — so **run `npm run store:check` before pasting
any of them**. It checks coverage against the catalogues, Edge's
documented limits (description 250–10,000 characters; at most seven
search terms, 30 characters each, 21 words in total), and every claim
against the packaged zip rather than the source.

The copy is the translation team's (received as `overviews/*-242.txt`),
with two corrections applied on import, both in every locale:

- **The first search term, "convert document to image", was dropped.** The
  extension has no image export — Markdown and plain text only. It was also
  the term that took ca, en, en_GB, es and fr over Edge's 21-word limit.
- **The Chrome description said "Requires Edge 103".** Now "Chrome 103".
  "Edge" appeared exactly once per locale, as the browser name.

The Edge/Firefox description is filed as Edge only: there is no Firefox
build of this extension.

### OCR — the one thing blocking submission

**`npm run store:check` fails, on purpose, until this is decided.** The
extension cannot do OCR: the zip contains no recogniser and no language
data, and the OCR button is hidden in the extension build
(`{#if !isBrowserExtension}` in `+page.svelte`). The web app and the
desktop app both do it.

Every one of the nineteen listings says otherwise, in four places:

- the Edge description's first sentence — "…with built-in OCR"
- both descriptions' Supporter line — "Watermarking and OCR are optional
  Supporter tools, unlocked together with a one-time payment". Watermarking
  can be unlocked from the extension; OCR, once paid for, still cannot run
  there.
- the manifest `name` — "PDF Editor, Reader & OCR Watermark Tool"
- the manifest short `description` — "…OCR PDFs entirely in your browser"

The last two are already in the packaged zip. Edge requires a description
to "not contain any misleading" content and has rejected this extension
once, so these would not pass review as written. Either the copy says the
extension does not do OCR (and that the web and desktop apps do), or the
extension ships OCR. Deciding between them is the product owner's call;
the check stays red until one of them is true.

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
  SDK client at app startup (`configure({ baseUrl: OPENAPPS_BASE_URL })`
  in `+layout.svelte`) only constructs a local client object and reads a
  local token store — it does not itself make a network request either
  (confirmed by reading the SDK client's constructor). **If and
  only if you choose to sign in**, that panel communicates with exactly
  two hosts, both named in `apps/desktop/src/lib/openapps.ts`:
  `auth.openpdfedit.com` for the session, credit balance and the
  Supporter entitlement check, and `gateway.openpdfedit.com` for the
  one route that can actually spend credits — the Supporter unlock. No
  document content or metadata is ever
  sent to either — the PDF engine and the account client are two unconnected
  code paths that never pass document data to each other.
- **Signing in happens on the web app, not in the extension.** Choosing
  Sign in opens `https://openpdfedit.com/app/login` in a tab. The whole
  sign-in exchange happens there, on that origin; when it finishes, that
  page hands the resulting session back to this extension and nothing
  else. The manifest's `externally_connectable` entry names that one
  origin as the only thing permitted to message the extension. It is a
  manifest key rather than a permission, and it grants the extension no
  access to that site — only the reverse, and only from that site.
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
- [ ] **`npm run store:check` passes.** Until it does, the listings are
      not submittable. It fails today — see "OCR" below.
- [ ] **Fill in each dashboard's listing form** from `store/listings/`, one
      language at a time, and the privacy declaration below in the
      privacy-practices tab
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
