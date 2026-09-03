<script lang="ts">
  // Dedicated popup window for signing in — see AccountPanel.svelte's
  // comment for why this can't happen in the main editor window.
  // <openapps-login>'s Google button does a full-page
  // `window.location.href` redirect out to accounts.google.com and back;
  // this window has no document state to lose, so it's the one that takes
  // that redirect. On success it tells the main window over a Tauri event
  // and closes itself — the main window never mounts <openapps-login> at
  // all, so that redirect can never happen to it.
  import { emit } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getClient } from "@openapps/ui";
  import {
    OPENER_EXTENSION_PARAM,
    SESSION_STORAGE_KEY,
    SIGNIN_DONE_MESSAGE,
    isExtensionId,
  } from "$lib/openapps";

  let containerEl: HTMLDivElement | null = null;

  // Same check AccountPanel makes, and for the same reason: these calls
  // reach into `window.__TAURI_INTERNALS__` when invoked, so in a browser
  // they throw rather than reject and no `.catch` would help.
  const tauriAvailable = "__TAURI_INTERNALS__" in window;

  /** Hands the finished session to the browser extension that sent us
   * here, if one did.
   *
   * The extension cannot run this page: `chrome-extension://` has no
   * server, so it opens the web app's copy instead — which means the
   * session lands in *this* origin's localStorage and not in the one the
   * extension reads. It has to be carried across deliberately.
   *
   * Two routes, because neither alone is reliable. `postMessage` to the
   * opener is immediate but depends on the opener relationship, which a
   * Cross-Origin-Opener-Policy header anywhere in the OAuth round trip
   * severs. `chrome.runtime.sendMessage` does not depend on it at all,
   * and works because the extension names this origin in
   * `externally_connectable` — a manifest key, not a permission, so it
   * costs the extension no warning at install.
   *
   * The id is validated before either: it arrives in a query parameter,
   * so it is untrusted input, and it decides who receives a credential. */
  function handOverToExtension(): void {
    const extensionId = new URLSearchParams(window.location.search).get(OPENER_EXTENSION_PARAM);
    if (!isExtensionId(extensionId)) return;

    const session = localStorage.getItem(SESSION_STORAGE_KEY);
    if (!session) return;
    const payload = { type: SIGNIN_DONE_MESSAGE, session };

    window.opener?.postMessage(payload, `chrome-extension://${extensionId}`);

    // `chrome.runtime` is injected into this page only because the
    // extension names this origin in `externally_connectable`; on every
    // other browser, and for every visitor without the extension, it is
    // simply absent.
    const runtime = (
      globalThis as {
        chrome?: {
          runtime?: {
            sendMessage?: (id: string, message: unknown, cb: () => void) => void;
            lastError?: unknown;
          };
        };
      }
    ).chrome?.runtime;
    if (typeof runtime?.sendMessage === "function") {
      try {
        // An extension that is not installed simply never answers. The
        // callback form is what keeps that from surfacing as an
        // unhandled rejection — reading `lastError` marks it handled.
        runtime.sendMessage(extensionId, payload, () => void runtime.lastError);
      } catch {
        // Nothing to do: the postMessage above is the other half.
      }
    }
  }

  async function finish(): Promise<void> {
    if (!tauriAvailable) {
      handOverToExtension();
      // The browser build. The session is already in this origin's
      // localStorage, which the opener shares and reads live, so there
      // is nothing to hand over — this only says "look again", so the
      // opener doesn't have to wait for a storage event or a refocus.
      window.opener?.postMessage({ type: SIGNIN_DONE_MESSAGE }, window.location.origin);
      window.close();
      return;
    }
    await emit("openapps-session-changed");
    await getCurrentWindow().close();
  }

  $effect(() => {
    // Reopening this window while already signed in (a stale window, or a
    // race with the main window's own check) — nothing to do here.
    //
    // `isLoggedIn` is only "a session object exists in storage"; it makes
    // no claim that the token still works. Taking it at face value is
    // what produced the worst version of this bug: an expired session
    // from an earlier visit meant this page never offered Google, Nostr
    // or a wallet at all — it closed immediately and handed the dead
    // token on, leaving an account panel that says "Not signed in" four
    // times over a "Sign out" button.
    //
    // So ask the server whether the session is real before acting on it.
    // A session that cannot fetch its own account is not one to keep, or
    // to pass to an extension.
    const client = getClient();
    if (client?.isLoggedIn) {
      void client.auth
        .me()
        .then(() => finish())
        .catch(() => {
          // Expired, revoked, or the server disowned it. Drop it and let
          // the sign-in buttons below render, which is what the person
          // opening this window came for.
          client.clearSession();
        });
      return;
    }
    if (!containerEl) return;
    const onLogin = () => void finish();
    containerEl.addEventListener("openapps-login", onLogin);
    return () => containerEl?.removeEventListener("openapps-login", onLogin);
  });
</script>

<div bind:this={containerEl} class="login-shell">
  <!-- The panel's framing is ours, not the platform's. The element
       defaults to "Sign in to OpenApps", which is right on OpenApps' own
       pages and wrong in a window titled OpenPdfEdit — a heading naming
       a service the user has never heard of, asking for a password,
       reads exactly like the thing people are told to be suspicious of.
       The account behind it is the same one either way; only the framing
       changes. Sign-in itself already runs against auth.openpdfedit.com
       (see $lib/openapps), so the address bar agrees with the heading. -->
  <openapps-login
    variant="panel"
    mark="P"
    heading="Sign in to OpenPdfEdit"
    description="Your account unlocks Supporter features and carries them to every device you sign in on. Everything else works without it."
  ></openapps-login>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
  }

  .login-shell {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    padding: var(--space-4);
    background: var(--bg-page);
  }
</style>
