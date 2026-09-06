# OpenPdfEdit for iOS

A native shell around the same web app that runs at
[app.openpdfedit.com](https://app.openpdfedit.com), bundled rather than
fetched.

## Why a shell, and why bundled

The editor already works: PDFium and the Rust core are compiled to
WebAssembly and run entirely in the page. Rewriting that in Swift would
mean maintaining a second implementation of 28 editing commands to reach
the same place.

What the shell adds is the three things a web page on iOS cannot do:

1. **Work with no network at all.** The whole editor is in the app
   bundle. That is the product's central claim, and a wrapper around a
   remote URL could not honour it — nor would it pass App Review, which
   treats a repackaged website as failing guideline 4.2.
2. **Take a PDF from another app.** "Open in OpenPdfEdit" from Files,
   Mail or any share sheet.
3. **Sell credits.** Guideline 3.1.1 requires digital content used in an
   app to be sold through in-app purchase, so the card checkout the web
   app offers is not merely redundant here — it is the thing that must
   not be there.

## Layout

```
OpenPdfEdit/
  OpenPdfEditApp.swift       @main, and the state that outlives the web view
  AppWebView.swift           WebRuntime: the web view, its delegates, the DEBUG probe
  BundleSchemeHandler.swift  serves www/ at openpdfedit://localhost
  WebBridge.swift            the one channel between page and shell
  bridge.js                  its JavaScript half, injected at document start
  Store.swift                StoreKit 2
  AuthSession.swift          sign-in, via ASWebAuthenticationSession
OpenPdfEditTests/            31 tests, all runnable with no developer account
Products.storekit            local product catalogue, for testing purchases
www/                         generated: a copy of apps/webapp/dist (gitignored)
```

## Building

```sh
apps/ios/scripts/sync-web.sh --build   # build the web app and copy it in
apps/ios/scripts/build-sim.sh --run    # build, install and launch a simulator
apps/ios/scripts/test.sh               # 31 tests
```

`sync-web.sh` without `--build` reuses `apps/webapp/dist` if it is there.

## Three decisions worth knowing

**`openpdfedit://localhost`, not `file://` and not an embedded server.**
A `file://` page has an opaque origin, so every request it makes to the
account server carries `Origin: null` — not something a CORS allowlist
can name without naming every sandboxed document on the internet. A
custom scheme gives a real origin; `localhost` as the host is what makes
it a secure context, which the account SDK needs for `crypto.subtle`. An
embedded HTTP server would give both, and a different port — so a
different origin — on every launch. The origin is asserted in
`BridgeTests`, because if it ever changes, every account call starts
failing CORS a long way from here.

**A transaction is finished only after the server grants the credits.**
StoreKit re-delivers an unfinished transaction on every launch, which is
what makes a purchase survive a crash, a dead network or a force-quit
between paying and being credited. Finishing eagerly — which is what the
obvious code does — throws that away and leaves someone charged for
credits nobody granted, with no record left to retry from. The order is
enforced in `IapPanel.svelte`, and tested from both sides:
`StoreTests.testAPurchaseIsNotFinishedUntilItIsTold` and the Playwright
test `the receipt is redeemed before the transaction is finished`.

**Sign-in goes through ASWebAuthenticationSession.** There are no popups
in a WKWebView, and pushing the OAuth page into this web view would
replace the editor and lose whatever is open behind it. Apple's sheet is
also the better answer: it shows the real address bar, so someone typing
a Google password can see whose page they are on, and it shares Safari's
cookies, so an account already signed in on the device usually needs one
tap. The session comes back in the callback URL's *fragment*, which is
never sent to a server.

## Testing purchases with no Apple Developer account

`Products.storekit` is a local catalogue. Running the app from Xcode uses
it (the shared scheme references it), so the purchase sheet works in the
simulator with no account and no money. The tests use it through
`SKTestSession`, which is real StoreKit with a test backend — the
transactions are signed, and `BridgeTests` asserts the receipt is the
same JWS shape the server verifies.

`scripts/test.sh` pins an **iOS 17** simulator. The StoreKit test service
in the iOS 26.5 runtime accepts a configuration and then answers product
requests from the real Media API anyway, so every purchase test sees an
empty catalogue — visible in `storekitd`'s own log, which shows the
configuration saved and the request going out to the network regardless.
This is the simulator's test harness, not StoreKit: the app is
unaffected, and the same tests pass on iOS 17, which is also the
deployment target.

## Before the first submission

Everything below needs the paid Apple Developer account, and nothing
above does.

- Set `DEVELOPMENT_TEAM` in the project (it is empty; the simulator does
  not need it and a device does).
- Create `credits_1000` and `credits_5000` in App Store Connect as
  **Consumable** in-app purchases. The ids must match
  `Store.productIdentifiers` and the `app_iap_products` rows on the
  server — three places, and the server is the one that decides what a
  product is worth.
- Configure the server: `docs/PRODUCTION.md` §3 (the origin), §3b (the
  Apple rail).
- Set the Server Notifications V2 URL. Without it Apple grants refunds
  and the credits stay granted.

## Size

The app bundle is about 85 MB, of which 70 MB is OCR: the Tesseract
engine variants and twelve languages' trained data. Trimming it means
fetching languages on demand from `app.openpdfedit.com`, which trades the
offline claim for download size. Bundled is the honest default; the split
is worth revisiting only if the size becomes a real objection.
