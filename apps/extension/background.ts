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

/**
 * Relays a finished sign-in from the web app to the editor tab.
 *
 * Signing in cannot happen inside the extension: these pages are served
 * from `chrome-extension://`, which has no server, so the OAuth redirect
 * has nowhere to land. The editor opens the web app's own login page at
 * `https://app.openpdfedit.com/login` instead, and that page hands the
 * session back — once it has one — to the extension id it was given.
 *
 * This worker is the destination, because a page cannot message another
 * page directly. `externally_connectable` in the manifest is what lets
 * that origin reach us at all; it is a manifest key rather than a
 * permission, so it adds no install-time warning.
 *
 * Only that origin is trusted, and it is checked here rather than assumed
 * from the manifest: this handler forwards a credential, and a manifest
 * edit should not be able to widen who can supply one without the check
 * changing too.
 */
const WEBAPP_ORIGIN = "https://app.openpdfedit.com";
const SIGNIN_DONE_MESSAGE = "openpdfedit-signin-done";

chrome.runtime.onMessageExternal.addListener((message, sender, sendResponse) => {
  if (sender.origin !== WEBAPP_ORIGIN) return;
  const payload = message as { type?: string; session?: string } | null;
  if (payload?.type !== SIGNIN_DONE_MESSAGE || typeof payload.session !== "string") return;

  // Broadcast to the extension's own pages. The editor tab picks it up;
  // if none is open there is nothing to update, and the session will be
  // read from storage the next time one is.
  chrome.runtime.sendMessage(payload).catch(() => {
    // No receiver. Not a failure — the editor tab may simply be closed.
  });
  sendResponse({ received: true });
});
