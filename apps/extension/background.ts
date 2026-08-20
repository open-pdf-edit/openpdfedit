// Opens the editor in its own tab on toolbar-icon click — mirrors
// opencapture's own background service worker's "open the editor
// in a real tab, not a popup" pattern (see that app's background.ts).
//
// Task 9 retired the hand-written editor.html/editor.ts walking skeleton
// in favor of packaging the shared desktop SPA (apps/desktop, built with
// VITE_BACKEND=wasm) as this extension's UI — see
// apps/extension/scripts/build-spa.sh. That build's SvelteKit
// adapter-static output has a fixed entry point name, index.html, not
// editor.html.
chrome.action.onClicked.addListener(() => {
  chrome.tabs.create({ url: chrome.runtime.getURL("index.html") });
});
