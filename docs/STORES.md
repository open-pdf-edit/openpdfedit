# Publishing to the stores

Three products, three storefronts:

| | What is submitted | Package | Where it comes from |
|---|---|---|---|
| **Edge Add-ons** | the browser extension | `openpdfedit-dist.zip` | `cd apps/extension && npm run package` |
| **Microsoft Store** | the desktop app | `OpenPdfEdit_<version>_x64.msix` | the `msix` workflow, or `scripts/build-msix.ps1` |
| **App Store** | the iOS app | `OpenPdfEdit.ipa` | Xcode Organizer, from `apps/ios` |

Edge and the Microsoft Store share one Partner Center account; the App
Store needs a separate Apple Developer membership.

Register once at
<https://partner.microsoft.com/dashboard>. The Edge program is free to
join; the Windows developer program charges a one-time individual
registration fee. Both live under the same sign-in.

## The one thing to understand before starting

**Neither store's API can make a first submission.** Microsoft is
explicit about the Edge one: *"To initially publish a new extension, you
use Partner Center"*, and there are no endpoints for creating a product
or editing its listing text. The Store submission API is the same shape
— it updates a product that a human has already created and named.

So the division of labour is fixed, and it is not a limitation of this
repository:

- **A human, once per product:** register, reserve the name, write the
  listing, upload the first package, answer the review.
- **Automated, every time after that:** build the package and push it.

Everything below is written around that split.

---

## Edge Add-ons — the extension

### 1. First submission, by hand

1. Partner Center → **Microsoft Edge** → **Create new extension**.
2. Upload `apps/extension/openpdfedit-dist.zip`. Build it fresh first:

   ```bash
   cd apps/extension && npm run package
   ```

   The zip is a gitignored build artifact, so nothing about a fresh
   clone or a `git pull` refreshes it, and a stale one is
   indistinguishable from a current one at the moment it matters.
3. Fill in the listing from `apps/extension/STORE.md` — short
   description, long description, category **Productivity**, the 128×128
   icon, screenshots, and the privacy declaration.
4. Privacy policy URL: <https://openpdfedit.com/privacy.html>.
5. Submit. Edge's review is typically slower than Chrome's; days rather
   than hours.

The declaration in `STORE.md` draws a line that matters and should not
be smoothed over: **document processing is local without exception, and
the account panel is a real network surface.** Both halves are true, and
a reviewer who finds the account call after reading an unqualified "100%
local, nothing leaves your machine" will treat the claim as a
misrepresentation rather than as shorthand.

### 2. Enable the API

Partner Center → **Microsoft Edge** → **Publish API**. If the page still
shows *Access token URL* and *Secrets*, click **Enable** next to "enable
the new experience" first — that is v1, whose token flow was retired at
the end of 2024. Then **Create API credentials**.

Write down three things:

- **Product ID** — a GUID, on the extension's overview page (also in the
  dashboard URL, between `microsoftedge/` and `/packages`)
- **Client ID**
- **API key** — shown once, at creation

### 3. Store them as repository secrets

Set them from your own shell so the values never pass through anything
else. `gh secret set` with no value prompts for it and does not echo:

```bash
gh secret set EDGE_PRODUCT_ID --repo open-pdf-edit/OpenPdfEdit
gh secret set EDGE_CLIENT_ID  --repo open-pdf-edit/OpenPdfEdit
gh secret set EDGE_API_KEY    --repo open-pdf-edit/OpenPdfEdit
```

The API key expires. When it does the upload fails with 401 and the fix
is to mint a new one in Partner Center and re-run `gh secret set` —
nothing in the repository changes.

### 4. Every release after that

From the Actions tab, run **publish-edge**, or:

```bash
gh workflow run publish-edge.yml --repo open-pdf-edit/OpenPdfEdit \
  -f notes="What changed in this version."
```

It builds the extension from the current commit, checks the credentials
are present, uploads, waits for the package to be accepted, publishes,
and waits again. `-f dry_run=true` does everything except the upload.

Locally, the same thing:

```bash
export EDGE_PRODUCT_ID=… EDGE_CLIENT_ID=… EDGE_API_KEY=…
./scripts/publish-edge.sh                 # upload and publish
./scripts/publish-edge.sh --no-publish    # upload to the draft only
```

The script refuses to upload a zip built before the current commit,
which is the failure this whole class of tooling exists to prevent.

---

## Microsoft Store — the desktop app

### Why MSIX rather than submitting the `.exe`

Partner Center accepts an unpackaged EXE or MSI: you reserve the name
and point it at a stable HTTPS URL serving your installer. It is less
work, and it costs money every year.

**The Store does not sign EXE or MSI submissions.** They must be
Authenticode-signed by the publisher first, with a certificate from a CA
in the Microsoft Trusted Root Program. Azure Trusted Signing is the
affordable route at roughly $10/month — and it is limited to verified
US, Canadian, EU and UK businesses and self-employed individuals, which
does not cover a Singapore-based publisher. That leaves a traditional OV
certificate at a few hundred dollars a year.

An MSIX submitted through the Store is re-signed by Microsoft during
onboarding. No certificate, ever. It also gets clean uninstall,
automatic updates, and no SmartScreen prompt.

The two are not interchangeable later: Partner Center has no supported
path from a published EXE/MSI product to an MSIX one. Choosing MSIX now
avoids reserving the name twice.

### 1. Prove the packaging works — before any account exists

```bash
gh workflow run msix.yml --repo open-pdf-edit/OpenPdfEdit
```

With no inputs it packs with a placeholder identity. That package would
be rejected on upload, which is the point: it proves `makeappx` runs,
the manifest is valid, and every file the manifest names is really in
the package, without needing Partner Center yet. The job opens the
finished `.msix` and asserts the contents rather than trusting that
`makeappx` exiting zero means the payload is complete.

### 2. Reserve the name, get the identity

Partner Center → **Windows & Xbox** → **Create a new app** → reserve
**OpenPdfEdit**. Then open the app → **Product management** → **Product
identity**, and copy three values:

| Partner Center label | Goes into |
|---|---|
| Package/Identity/Name | `MSIX_IDENTITY_NAME` |
| Package/Identity/Publisher (the full `CN=…` string) | `MSIX_PUBLISHER` |
| Package/Properties/PublisherDisplayName | `MSIX_PUBLISHER_DISPLAY_NAME` |

These are **not secrets** — they are printed on the product page and
embedded in every copy of the shipped package. Store them as repository
*variables*:

```bash
gh variable set MSIX_IDENTITY_NAME         --repo open-pdf-edit/OpenPdfEdit
gh variable set MSIX_PUBLISHER             --repo open-pdf-edit/OpenPdfEdit
gh variable set MSIX_PUBLISHER_DISPLAY_NAME --repo open-pdf-edit/OpenPdfEdit
```

They must match Partner Center exactly. An identity mismatch is the most
common first-upload rejection and the error message is not especially
clear about which of the three is wrong.

### 3. Build the real package

```bash
gh workflow run msix.yml --repo open-pdf-edit/OpenPdfEdit
```

With the variables set it now produces a submittable package; the run
summary says which of the two it built. Download the `msix-package`
artifact.

On a Windows machine directly:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 `
  -IdentityName "…" -Publisher "CN=…" -PublisherDisplayName "…"
```

Add `-SelfSign` to sideload and test it locally. **Never** submit a
self-signed package — the Store signs it, and one carrying another
signature is rejected.

### 4. First submission, by hand

In the reserved app: **Pricing and availability**, **Properties**
(category *Productivity*), **Age ratings** (an IARC questionnaire),
**Packages** (upload the `.msix`), **Store listings** (description,
screenshots at 1366×768 or larger, the privacy policy URL), then submit.

Reviews typically take a few days.

---

---

## App Store — the iOS app

The app is documented in full in `apps/ios/README.md`, including how to
build, test and buy credits with no developer account at all. What
belongs here is the submission itself.

### What Apple will look for

Two guidelines decide this submission, and both were designed for.

**4.2, minimum functionality.** A repackaged website is rejected. This
app bundles the entire editor — PDFium and the Rust core, compiled to
WebAssembly — so it works in aeroplane mode, and it registers as a PDF
editor so "Open in OpenPdfEdit" appears in Files, Mail and every share
sheet. Say both in the review notes; a reviewer who only taps around the
first screen sees something that looks like a web view.

**3.1.1, in-app purchase.** Credits are digital content used in the app,
so they must be sold through the App Store. Inside the shell the account
panel swaps the card checkout for a StoreKit one — this is enforced in
code, not by convention, and asserted by a test (`inside the shell,
credits are sold through the App Store and not by card`). Nothing in the
app links out to a payment page.

The related trap is 3.1.3(b): an account created elsewhere may be used in
the app, and the app may not *tell* anyone where to create one. The
sign-in sheet is fine. A "sign up on our website" link is not.

### Order of operations

1. **Buy the membership** ($99/yr). Everything below needs it; nothing in
   `apps/ios/README.md` does.
2. **Register the bundle id** `com.openpdfedit.app`, and set
   `DEVELOPMENT_TEAM` in the Xcode project.
3. **Create the two in-app purchases** in App Store Connect, both
   **Consumable**: `credits_1000` and `credits_5000`. The ids must match
   `Store.productIdentifiers` and the `app_iap_products` rows on the
   server. The server decides what a pack is *worth*; App Store Connect
   decides what it *costs*.
4. **Configure the server** — `docs/PRODUCTION.md` §3 (the app's origin)
   and §3b (the Apple rail, and the Server Notifications URL). Do this
   before the first sandbox purchase, not after: without the origin, an
   account cannot be reached from the app at all.
5. **Archive and upload** from Xcode's Organizer.
6. **Sandbox-test a real purchase** through TestFlight before submitting.
   This is the first moment the whole chain runs end to end — StoreKit,
   the receipt, the server's verifier, the ledger — and the first moment
   Apple's real certificate chain is parsed by anything of ours.

### What is not automated

Nothing about the App Store, deliberately. `xcodebuild archive` plus
`altool` is a well-trodden path, but it needs an App Store Connect API
key and a signing identity in CI, and neither exists yet. It is worth
doing once releases are frequent enough for the Organizer to be
annoying — not before the first one has ever been made.

## Version numbers

One command sets all five places the version is written:

```bash
./scripts/set-version.sh          # show
./scripts/set-version.sh 0.1.8    # set
```

The MSIX version is derived from `tauri.conf.json` and given the fourth
part the Store requires, so `0.1.8` becomes `0.1.8.0`. The Store
reserves that revision field and rejects a non-zero value.

Bump before every submission. Every storefront refuses a version number
it has already accepted, and none of them release one back after a
withdrawal — a duplicate costs a review cycle to discover.

## What is deliberately not here

**Chrome Web Store automation.** Chrome has a publish API too, but the
extension is not on Chrome yet either, and adding a second untested
publish path before the first submission has ever been made would be
building on a guess. Worth doing straight after the Chrome listing goes
live, reusing the shape of `publish-edge.sh`.

**App Store automation.** See above — same reasoning, different account.

**Google Play.** The backend rail is built and tested (`docs/PRODUCTION.md`
§3c), but there is no Android app yet. The iOS shell is the pattern it
will follow.

**Microsoft Store submission automation.** The Store's submission API
needs an Azure AD directory associated with the Partner Center account —
a real piece of setup that is only worth doing once releases are
frequent enough to be annoying. Until then, uploading a `.msix` in the
dashboard is a two-minute job a few times a year.
