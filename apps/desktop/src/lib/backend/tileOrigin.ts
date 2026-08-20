// WebView2 (Windows) can't `fetch()` an arbitrary custom scheme like
// `tile://` — being Chromium-based, it only allows fetch against
// http(s)/blob/data/etc, so Tauri exposes registered custom protocols
// there as `<scheme>.localhost` instead (the same trick
// `@tauri-apps/api/core`'s `convertFileSrc` uses internally). It's
// `http`, not `https`: wry's `use_https_scheme` defaults to `false`
// and this app never opts in (see wry's `WebViewBuilderExtWindows`),
// so the app's own window loads over plain http on Windows too — an
// `https://tile.localhost` request doesn't match what WebView2 is
// listening for and falls through to a real (failing) DNS lookup,
// confirmed live via `ERR_CONNECTION_REFUSED` in a packaged build.
// WKWebView (macOS) has no such restriction and serves the scheme
// directly — that path is what lib.rs's TILE_CORS_HEADER comment
// already verified against a real packaged build.
//
// Every `tile://` fetch site (PdfPage.svelte, PageThumb.svelte) must use
// this instead of hardcoding the scheme — PageThumb.svelte hardcoded
// `tile://localhost` directly for a while, which worked on macOS but
// produced a real "URL scheme 'tile' is not supported" failure on
// Windows, since fetch() there never falls through to WebView2's
// `.localhost` handling the way an <img src> or <video src> does.
export const TILE_ORIGIN =
  typeof navigator !== "undefined" && navigator.userAgent.includes("Windows")
    ? "http://tile.localhost"
    : "tile://localhost";
