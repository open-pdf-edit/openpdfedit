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
picker, writes the bytes and resolves `true`; cancelling resolves `false`
rather than rejecting, because dismissing a picker is not an error.

## Not done

- **Sign-in.** Deliberately absent from `bridge.js` rather than present and
  failing, so `nativeShell()?.signIn` reads false and the web layer takes its
  own path. It wants a Custom Tab and a callback the accounts server is
  configured to redirect to.
- **Play Billing.** The server side already exists
  (`openapps-payments/src/google.rs`, live at `/v1/payments/google/redeem`);
  the client half does not. Until it does, `storeKit()` returns null and no
  purchase UI is offered — which is why that check moved off "is there a
  shell" and onto "can it actually buy something".
- **A launcher icon**, and a pass over the toolbar: at phone width the top bar
  overlaps itself.
