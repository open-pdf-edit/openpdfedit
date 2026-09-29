# The Android shell

The same web build the iOS app ships, in a WebView. Nothing about the PDF
engine is Android-specific: PDFium and the Rust core are already compiled to
WebAssembly, single-threaded, so there is no NDK, no native PDFium and no
native Tesseract here — only a shell that hosts the page and carries files in
and out.

## Building

    OCR=0 apps/android/scripts/sync-web.sh --build   # web bundle into assets
    cd apps/android && ./gradlew :app:assembleDebug

`sync-web.sh` is a thin wrapper around the iOS one, called with a different
destination. That script's real content is the list of things that must come
*out* of the web build before it goes inside an app binary — the service
worker, whose cache would outlive an app update; the crawler copy, which reads
as "a repackaged website" to a reviewer. None of that is Apple-specific, so
there is one copy of it rather than two that drift.

`OCR=0` drops 70 of the bundle's 84 MB. Shipping all twelve languages and six
Tesseract cores is a poor download for a feature most people never open;
fetching them on first use is the proper fix and is not written yet.

## What the shell does

| Piece | Where |
|---|---|
| Serves the bundle at `https://appassets.androidplatform.net` | `MainActivity.assetLoader` |
| Documents in, from VIEW/SEND intents, at `/__incoming/<n>` | `MainActivity.stage` |
| Documents out, through the SAF create-document picker | `MainActivity.startSave` |
| The one JS channel | `WebBridge.kt` + `assets/bridge.js` |
| Play Billing | `Billing.kt` |

An https origin rather than a custom scheme because the page must be a secure
context or `crypto.subtle` is undefined and the account SDK cannot start —
the same reason iOS serves from `openpdfedit://localhost`. **That origin has
to be in the accounts server's CORS allowlist**, next to
`openpdfedit://localhost` and `tauri://localhost`.

## Testing the bridge

Debug builds enable WebView inspection, so the bridge can be driven from the
command line rather than by tapping at coordinates:

    adb forward tcp:9333 localabstract:$(adb shell cat /proc/net/unix |
      grep -o 'webview_devtools_remote_[0-9]*' | head -1)

then evaluate against the page target from `http://localhost:9333/json`
(suppress the `Origin` header — CDP rejects it otherwise). Verified this way:
the bridge is injected with `platform: "android"`; `saveFile` opens the
picker, writes the bytes and resolves `true` — read back off the device to
confirm; cancelling resolves `false` rather than rejecting, because
dismissing a picker is not an error. A VIEW intent carrying a `content://`
URI opens the document through `/__incoming/`, checked after `pm clear` so
nothing could have been restored from a previous run.

## Releasing

The upload keystore lives in `~/.config/openpdfedit-android/`, never in the
repository, and `app/build.gradle.kts` reads it from there — a missing file
is not an error, so debug builds and CI are unaffected. **Play identifies the
app by that key for as long as it exists, so back that directory up somewhere
other than this machine.**

    ./gradlew :app:bundleRelease        # app/build/outputs/bundle/release/

To see what Play will actually generate from the bundle — which is not the
same as the bundle:

    bundletool build-apks --bundle=…/app-release.aab --output=/tmp/o.apks \
      --mode=universal --ks=… --ks-key-alias=upload

Worth doing once: `BundleConfig.pb` does **not** list the `noCompress`
extensions, which looks like the setting was lost. It was not — the generated
APK has every `.wasm` `Stored`. Judge it from the APK, not the bundle.

## Not done

- **Sign-in.** Deliberately absent from `bridge.js` rather than present and
  failing, so `nativeShell()?.signIn` reads false and the web layer takes its
  own path. It wants a Custom Tab and a callback the accounts server is
  configured to redirect to.
- **Play Billing end to end.** The client is written (`Billing.kt`) and the
  server side already existed (`openapps-payments/src/google.rs`, live at
  `/v1/payments/google/redeem`). What is untested is a real purchase: that
  needs the app in the Play Console, the two products created, a build on a
  test track and a licence tester. On a device with no Play Store every call
  degrades rather than hangs — measured: `products` → `[]`, `purchase` →
  `cancelled`, `finish` → `false`, all under 10 ms.
- **Server configuration for Play.** `OPENAPPS_GOOGLE_IAP_SERVICE_ACCOUNT_JSON`
  (a service account from Play Console → API access), and rows in
  `app_iap_products` for platform `google` with bundle id
  `com.openpdfedit.app` — without them redemption answers "no credit package
  is mapped to product credits_1000".
- **A launcher icon.**
- **A toolbar pass.** At phone width the top bar fits until the Recent button
  appears, and then Open/Recent and the language/Account buttons overlap each
  other. It lays out correctly with no history, which is why it looks fine on
  a first run and wrong on a second.
