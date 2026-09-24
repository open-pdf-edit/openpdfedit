# App Store Connect listing

Every field App Store Connect asks for, with the answer. Copy from here
rather than composing at the keyboard — several of these fields are
length-limited, one of them (App Privacy) is a legal declaration that has
to match what the app actually does, and one (Sign-In Information) is the
single most common cause of a first-submission rejection.

The screenshots live in `screenshots/`; regenerate them with
`../scripts/screenshots.sh`.

## What the app actually charges for

Get this right before writing a word of the listing, because three
different fields depend on it and overclaiming in any of them is a
rejection.

- Everything is free except **two** tools: **Watermark** and **OCR**.
- Those two are unlocked together, **once**, for **1,000 credits**. It is
  not a subscription and not a per-use charge. The entitlement is
  recorded on the server (`openpdfedit_supporter_unlock`), so
  reinstalling or switching devices cannot cost a second 1,000.
- **Buying credits requires an account.** Redemption is authenticated —
  see `$lib/iap.ts` — so there is no anonymous purchase path. This is why
  a demo account is required below.
- `credits_1000` at $4.99 is exactly the unlock. `credits_5000` is for
  someone who also uses credits in another OpenApps product.

## App information

| Field | Value |
| --- | --- |
| Name | `OpenPdfEdit` |
| Subtitle | `Edit PDFs on device` |
| Bundle ID | `com.openpdfedit.app` |
| SKU | `openpdfedit-ios` |
| Primary category | Productivity |
| Secondary category | Business |
| Primary language | English (U.S.) |
| Age rating | 4+ |
| Content rights | Does not contain, show, or access third-party content |

The subtitle is capped at 30 characters. `Edit PDFs on device` is 19 and
says the one thing separating this from the several hundred PDF apps that
upload your file to a server.

## Promotional text (170 max)

> Your PDFs never leave your device. Edit, annotate, fill, sign, merge
> and export entirely on device — no upload, no account needed, and all
> of it works in Airplane Mode.

Promotional text can be changed without submitting a new build, so this
is the field for anything time-sensitive later.

## Description (4000 max)

Every line below is a claim about the build, checked against its code on
2026-09-22 — three were not true until then: pages could not be
*inserted*, one document could not be *split into many* (the pages panel
extracts the selected pages into one new file), and the outline could be
*read* but not edited. Check a new line the same way before adding it.

> OpenPdfEdit is a complete PDF editor that runs entirely on your device.
> Not a viewer with a subscription attached, and not a front end for
> someone else's server — the whole editor is in the app.
>
> That has one consequence worth stating plainly: your documents are
> never uploaded anywhere. There is no server to leak them and no privacy
> policy you have to take on trust, because the file never leaves your
> device in the first place. Turn on Airplane Mode and everything below
> still works.
>
> WHAT YOU CAN DO
>
> • Annotate — highlight, underline, strike through, and add sticky notes
> • Draw freehand, or add rectangles and ellipses
> • Edit the text already in a document, and move text and images around
> • Redact — properly, by removing the content, not by drawing over it
> • Add text fields and checkboxes, fill in forms, and sign with a
>   signature you draw once and reuse
> • Reorder, rotate, crop and delete pages
> • Merge several PDFs into one, or pull the pages you need out into a new file
> • Number pages, and move around a long document by its outline
> • Compress a file that is too large to email
> • Extract pages and text
> • Encrypt a document with a password
> • Search inside a document
>
> OPENS FROM ANYWHERE
>
> OpenPdfEdit registers as a PDF editor, so it appears in the share sheet
> across the system. Send a PDF to it from Mail, Files, Safari, Messages
> or anywhere else and it opens ready to edit.
>
> WHAT COSTS MONEY
>
> Almost nothing, and we would rather say so than bury it. Every tool
> listed above is free, forever, with no account.
>
> Two tools — Watermark and OCR — are unlocked together for a one-time
> 1,000 credits. Not a subscription. Not per use. Once. Credits are
> bought in the app, never expire, and the unlock is remembered by your
> account, so reinstalling or moving to a new device does not charge you
> again.
>
> An account is needed only to buy credits. Everything else works signed
> out.
>
> OPEN SOURCE
>
> OpenPdfEdit is open source. The editor, the rendering engine and the
> app around them can all be read, built and audited by anyone.

## Mac description (4000 max)

The same product on the Mac App Store, without the one thing the Mac build
does not do: it does not appear in Finder's Open With, so the iOS listing's
share-sheet paragraph is left out rather than reworded into a claim.

> OpenPdfEdit is a complete PDF editor that runs entirely on your Mac.
> Not a viewer with a subscription attached, and not a front end for
> someone else's server — the whole editor is in the app.
>
> That has one consequence worth stating plainly: your documents are
> never uploaded anywhere. There is no server to leak them and no privacy
> policy you have to take on trust, because the file never leaves your
> Mac in the first place. Turn off Wi-Fi and everything below still
> works.
>
> WHAT YOU CAN DO
>
> • Annotate — highlight, underline, strike through, and add sticky notes
> • Draw freehand, or add rectangles and ellipses
> • Edit the text already in a document, and move text and images around
> • Redact — properly, by removing the content, not by drawing over it
> • Add text fields and checkboxes, fill in forms, and sign with a
>   signature you draw once and reuse
> • Reorder, rotate, crop and delete pages
> • Merge several PDFs into one, or pull the pages you need out into a new file
> • Number pages, and move around a long document by its outline
> • Compress a file that is too large to email
> • Extract pages and text
> • Encrypt a document with a password
> • Search inside a document
>
> WHAT COSTS MONEY
>
> Almost nothing, and we would rather say so than bury it. Every tool
> listed above is free, forever, with no account.
>
> Two tools — Watermark and OCR — are unlocked together for a one-time
> 1,000 credits. Not a subscription. Not per use. Once. OCR runs on your
> Mac too: it turns a scanned page into text you can select and search,
> with nothing to install. Credits are bought in the app, never expire,
> and the unlock is remembered by your account, so it follows you to
> OpenPdfEdit on iPhone and iPad as well.
>
> An account is needed only to buy credits. Everything else works signed
> out.
>
> OPEN SOURCE
>
> OpenPdfEdit is open source. The editor, the rendering engine and the
> app around them can all be read, built and audited by anyone.

## Mac promotional text (170 max)

> Your PDFs never leave your Mac. Edit, annotate, fill, sign, merge and
> export entirely on device — no upload, no account needed, and all of it
> works offline.

## Keywords (100 max, comma-separated, no spaces)

```
pdf,editor,annotate,sign,form,merge,split,compress,offline,redact,markup,fill
```

84 characters. Do not repeat the app name or the category names — Apple
already indexes those, and a duplicate spends the budget for nothing.

## URLs

| Field | Value |
| --- | --- |
| Marketing URL | `https://openpdfedit.com` |
| Support URL | `https://openpdfedit.com/support` |
| Privacy Policy URL | `https://openpdfedit.com/privacy` |

Both the support and privacy URLs must resolve before submission. Apple
follows them, and a 404 is a rejection. Check them:

```bash
for u in https://openpdfedit.com/support https://openpdfedit.com/privacy; do
  printf '%s %s\n' "$(curl -s -o /dev/null -w '%{http_code}' "$u")" "$u"
done
```

## App Privacy

- **Data used to track you:** none. The app has no analytics, no ad
  identifier and no third-party SDK.
- **Data linked to you:** *Contact Info → Email Address* and *Purchases →
  Purchase History*, both only for someone who creates an account.
  Purpose: **App Functionality**. Tick "used for tracking" on neither.
- **Data not linked to you:** none.
- **The documents a customer opens:** not collected. This is defensible
  rather than aspirational — there is no network call carrying document
  bytes anywhere in the app, which is the same claim the review notes
  invite the reviewer to verify with Airplane Mode.

## In-app purchases

Both **Consumable**. The identifiers must match
`Store.productIdentifiers` in the app and the `app_iap_products` rows on
the server exactly. Nothing checks this for you, and a mismatch presents
as a purchase that takes the money and grants nothing.

| Product ID | Reference name | Display name | Price |
| --- | --- | --- | --- |
| `credits_1000` | 1,000 Credits | 1,000 Credits | $4.99 |
| `credits_5000` | 5,000 Credits | 5,000 Credits | $19.99 |

Description for `credits_1000`:

> 1,000 credits for OpenPdfEdit — exactly what the one-time Watermark and
> OCR unlock costs. Credits never expire.

Description for `credits_5000`:

> 5,000 credits for OpenPdfEdit. Credits never expire and can be spent in
> any OpenApps product.

Each in-app purchase needs its own review screenshot showing where it is
bought. `screenshots/iap-review.png` serves for both — it is the credits
panel, where both packs appear together.

## Sign-In Information — required

Tick **Sign-in required** and supply the shared review account. The
reviewer will test the in-app purchase, purchases are authenticated, and a
reviewer who cannot reach the buy button rejects the build.

OpenApps has no email or password sign-in — accounts are Google, Nostr or an
Ethereum wallet — so the review account is a **Nostr key**, which is a
credential a reviewer can simply paste. It is one account for every OpenApps
app, since they share an account system, and it was created on the
production server on 2026-09-22 with a zero balance.

- **The key** lives in `~/.config/openapps/app-review.txt` (mode 600) on the
  release Mac, and in the password manager. Never in this repository.
- **Username field:** `Nostr key (paste it — see notes)`
- **Password field:** the `nsec1…` key from that file.

And in the notes, the path, because it is four taps deep and a reviewer
will not guess it:

> Sign in with the Nostr key supplied in the password field. There is no
> email login. Tap Account → Continue with Nostr → "No extension? Use a
> remote signer" → "I only have a private key", paste the key, and sign in.
> The account has zero credits so that the in-app purchase can be tested.

Keep the balance at zero. The reviewer needs to *make* a purchase, and a
pre-credited account hides the thing under review.

## Review notes: sign-in (both platforms)

> SIGN IN: tap Account, then "Continue with Apple", and use your own
> Apple ID. That is the whole of it. We request no name and no email —
> only Apple's anonymous user identifier — so nothing personal reaches
> us, and the account can be deleted in the app at Account → Delete
> account.
>
> There is no email-and-password login in this app, which is why the
> Username field on this form holds a Nostr key instead (the Password
> field is unused). To use it: Account → Continue with Nostr → "No
> extension? Use a remote signer" → "I only have a private key", and
> paste the value from the Username field.
>
> IMPORTANT, for the in-app purchase step: please sign in with the Nostr
> demo account above, not with Apple. Our server credits a sandbox
> purchase only to accounts named in its configuration, and the demo
> account is the one named. A purchase made on any other account is
> refused with "this server credits Production purchases; that receipt
> is Sandbox" — not a bug you have found, but the guard that stops a
> sandbox receipt buying real credits. The demo account starts with zero
> credits so the purchase is a real test.
>
> Sign in with Apple is the right way to see the sign-in and account
> deletion flows; the Nostr account is the one to buy with.

## Review notes: about the app (both platforms)

Apple asked for these on the iOS 2.1 reply (2026-09-24) and said to keep
them in the Notes field "for reference on future submissions" — so they
are shared, not copied, or the two platforms drift.

> PURPOSE AND AUDIENCE
>
> A PDF editor for people who do not want their documents uploaded to be
> edited. Contracts, medical letters, ID scans: every other free PDF tool
> asks you to upload the file to a server, and this one does the work on
> the device. For anyone who signs, fills, redacts or annotates PDFs and
> would rather not hand them to a stranger.
>
> EXTERNAL SERVICES
>
> The editing uses none: PDFium and Tesseract are compiled into the
> bundle, not called over a network. The app contacts three hosts, all
> ours, and only if you sign in: auth.openpdfedit.com (accounts and
> balance), gateway.openpdfedit.com (the only service that can spend
> credits), openpdfedit.com (support and privacy pages). Authentication
> is Sign in with Apple and Nostr; payment is Apple in-app purchase and
> nothing else. No analytics, advertising, AI service or data provider.
>
> REGIONAL DIFFERENCES
>
> None. Features are identical in every territory and there is no
> geographic gating in the code. The interface is offered in eight
> languages and OCR in twelve, both chosen by the user, not by location.
> Only the displayed price varies, which StoreKit formats.
>
> REGULATED INDUSTRY AND THIRD-PARTY MATERIAL
>
> Neither applies. This is a document editor, not a regulated service.
> Two third-party components are bundled under permissive licences that
> allow redistribution: PDFium (BSD-3-Clause) and Tesseract with
> tesseract.js (Apache-2.0). No protected content ships with the app; it
> displays only files the user opens. Our source is public at
> github.com/open-pdf-edit/openpdfedit under AGPL-3.0-or-later.

## Review notes

Paste into "Notes for the reviewer". The first paragraph exists because
of guideline 4.2 and the third because of 3.1.1; a reviewer who taps
around the first screen sees a web view and will otherwise draw the wrong
conclusion.

> This is not a web wrapper. The complete PDF engine — PDFium and our
> Rust core, compiled to WebAssembly — ships inside the app bundle and
> runs on device. To confirm: enable Airplane Mode and every feature
> below still works, including opening, editing and exporting. Nothing is
> fetched at runtime and no document ever leaves the device.
>
> The app registers as a PDF editor, so "Open in OpenPdfEdit" appears in
> the share sheet system-wide. Sending a PDF from Files or Mail is the
> primary way it is used.
>
> Credits are digital content consumed inside the app and are sold only
> through in-app purchase. There is no link to an external payment page
> anywhere in the app.
>
> HOW TO TEST
>
> 1. Open any PDF from Files, or tap Open on the first screen.
> 2. Annotate it, edit its text, reorder pages, then export. All free,
>    and no account is needed for any of it.
> 3. Tap Watermark. A panel explains the one-time 1,000-credit unlock.
> 4. Sign in with the Nostr demo account, then buy the 1,000 Credits
>    pack. The unlock is permanent for that account.
>
> Almost the entire app is free. Only Watermark and OCR are paid, they
> unlock together, and the charge is once — not a subscription.


## Mac review notes

> This is not a web wrapper. The complete PDF engine — PDFium and our
> Rust core, compiled to WebAssembly — ships inside the app bundle and
> runs on the Mac. To confirm: turn off Wi-Fi and every feature below
> still works, including opening, editing, OCR and exporting. Nothing is
> fetched at runtime and no document ever leaves the Mac.
>
> Credits are digital content consumed inside the app and are sold only
> through in-app purchase. There is no link to an external payment page
> anywhere in the app.
>
> HOW TO TEST
>
> 1. Click Open PDF… on the first screen and choose any PDF.
> 2. Annotate it, edit its text, reorder pages, then save. All free, and
>    no account is needed for any of it.
> 3. Click Watermark or OCR. They are the only two paid tools, so a panel
>    appears explaining the one-time 1,000-credit unlock.
> 4. Sign in with the demo account supplied above, then buy the 1,000
>    Credits pack. The unlock is then permanent for that account, on the
>    Mac and on iPhone and iPad.
>
> Almost the entire app is free. Only Watermark and OCR are paid, they
> are unlocked together, and the charge is once — not a subscription and
> not per use.

## Export compliance

`ITSAppUsesNonExemptEncryption` is `false` on both targets, so App Store
Connect does not ask at upload time. The app's only use of cryptography is
HTTPS and PDF password encryption via the platform and PDFium — both
exempt.

**The Mac target only got this on 2026-09-23**, after the 1.0.1 submission
was refused with "This build is missing export compliance information".
iOS had carried the key in `apps/ios/OpenPdfEdit/Info.plist` since the
start; the Tauri Mac build had no `Info.plist` of its own at all, so
nothing answered the question. Build 3537358 was answered through the API
after the fact (`PATCH /builds/{id}` with `usesNonExemptEncryption`), and
`apps/desktop/src-tauri/Info.plist` now carries the key so no later build
asks again. A key set per project rather than per config, so the
Developer ID build gets it too.

## Before you press Submit

- [ ] Support and privacy URLs return 200.
- [ ] Both in-app purchases are **Ready to Submit**, and attached to this
      build. A first submission must carry its IAPs with it; submitted
      separately they sit in limbo.
- [ ] The demo account exists on the production server, is signed-in-able
      from the app, and has zero credits.
- [ ] `app_iap_products` has the two `apple` rows, on the production
      database (`../store/products.sql`).
- [ ] The server's App Store Server Notifications V2 URL is set.
- [ ] A sandbox purchase has been made end to end through TestFlight and
      the credits appeared.
