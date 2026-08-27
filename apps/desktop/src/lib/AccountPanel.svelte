<script lang="ts">
  // Account/credits surface. Deliberately never mounts <openapps-login> in
  // this window — its Google button does a full-page
  // `window.location.href` redirect out to accounts.google.com and back,
  // which would reload this whole SPA and discard any unsaved edit.
  // Sign-in happens in a separate popup window (routes/login/+page.svelte)
  // that has no document state to lose, and reports back over the
  // "openapps-session-changed" Tauri event once done. Viewing the
  // balance, buying credits and signing out are all plain API calls with
  // no redirect, so those stay inline here via <openapps-account> /
  // <openapps-credits> / <openapps-buy> / <openapps-referral> /
  // <openapps-signout>.
  //
  // NOT routed through the Backend adapter (Task 7, extension-port
  // Phase 1): this and routes/login/+page.svelte are the one place left
  // still importing @tauri-apps/api directly. Multi-window
  // creation/emit/listen (WebviewWindow, emit, getCurrentWindow) has no
  // one-to-one analog in a Chrome extension — there's no second "window"
  // to spawn the same way — so collapsing it into a same-shaped Backend
  // method would be pretend-portability, not real portability. This is
  // account/login chrome, not the 28-command PDF editing surface Task 7
  // covers; it needs its own small "start login, get told when it's
  // done" abstraction designed deliberately, tracked as a follow-up for
  // whichever task builds the extension's login flow (after Task 8).
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getClient, onChange, notify } from "@openapps/ui";
  import { SESSION_STORAGE_KEY, SIGNIN_DONE_MESSAGE, signInWithTelegram } from "$lib/openapps";
  import { initData as telegramInitData, isTelegram } from "$lib/telegram";
  import Icon from "./Icon.svelte";
  import { showToast } from "./toast.svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  // `@tauri-apps/api` calls (both `listen` below and `WebviewWindow` in
  // `signIn`) reach into `window.__TAURI_INTERNALS__` the moment they're
  // *called*, not at import time — importing the package is harmless in
  // the extension build (no such global there), but calling into it
  // throws "Cannot read properties of undefined (reading
  // 'transformCallback')" on every single page load, unhandled, because
  // this component's own `$effect` below used to call `listen()` with no
  // `.catch`. This check is this file's one addition of real
  // extension-awareness (everything else here is deliberately NOT
  // Backend-routed — see the module doc above) — it exists purely to
  // avoid that crash, not to offer any extension-side login flow.
  const tauriAvailable = "__TAURI_INTERNALS__" in window;

  let loggedIn = $state(getClient()?.isLoggedIn ?? false);

  function refresh(): void {
    loggedIn = getClient()?.isLoggedIn ?? false;
  }

  // Runs for the component's whole lifetime, not just while `open` — the
  // balance/login state should stay current even while the panel is
  // closed, so reopening it never shows a stale sign-in prompt.
  $effect(() => {
    const stopLocal = onChange(refresh);
    let stopRemote: UnlistenFn | undefined;
    let cancelled = false;
    // Skip registering the Tauri-event listener entirely outside Tauri —
    // see `tauriAvailable`'s doc above for why calling `listen()` there
    // throws rather than rejecting (so a `.catch` alone wouldn't help).
    if (tauriAvailable) {
      void listen("openapps-session-changed", refresh).then((un) => {
        if (cancelled) un();
        else stopRemote = un;
      });
    }

    // The browser equivalent of that Tauri event, in two parts because
    // neither alone is enough. `storage` fires in *other* same-origin
    // windows when one writes — that catches the sign-in popup finishing,
    // and equally a sign-out in another tab — but never in the window
    // that did the writing. The `message` is what the popup sends
    // explicitly, and arrives immediately rather than waiting for a
    // storage write to land.
    const onStorage = (e: StorageEvent) => {
      if (e.key === null || e.key === SESSION_STORAGE_KEY) sessionChangedElsewhere();
    };
    const onMessage = (e: MessageEvent) => {
      if (e.origin !== window.location.origin) return;
      if ((e.data as { type?: string } | null)?.type === SIGNIN_DONE_MESSAGE) sessionChangedElsewhere();
    };
    window.addEventListener("storage", onStorage);
    window.addEventListener("message", onMessage);

    return () => {
      cancelled = true;
      stopLocal();
      stopRemote?.();
      window.removeEventListener("storage", onStorage);
      window.removeEventListener("message", onMessage);
    };
  });

  /** A session written by another window of this origin. The client here
   * needs no re-hydration — its store reads localStorage live — but
   * nothing has told the rest of the page to look again, so `notify()`
   * does: every other element that shows a balance or a name re-reads. */
  function sessionChangedElsewhere(): void {
    refresh();
    notify();
  }

  function signIn(): void {
    // Inside Telegram the session is already on the page — no popup, no
    // redirect, no injected signer, none of which a Mini App webview
    // handles well anyway. Falls through to the ordinary flow if the
    // initData has expired, which is a normal thing to happen rather than
    // an error worth showing.
    const tg = isTelegram() ? telegramInitData() : null;
    if (tg) {
      void signInWithTelegram(tg).then((ok) => {
        if (ok) {
          sessionChangedElsewhere();
        } else {
          showToast("Telegram sign-in expired. Reopen the app and try again.", {
            tone: "warning",
            title: "Sign-in failed",
          });
        }
      });
      return;
    }
    if (!tauriAvailable) {
      // The browser builds (web app, extension page) get a real popup
      // rather than the short-circuit that used to live here. Same
      // reasoning as the Tauri window: <openapps-login>'s Google button
      // navigates the whole window out to accounts.google.com and back,
      // so it needs a window with no document state to lose.
      //
      // Nothing has to be handed back through it. The popup shares this
      // origin, so it shares this localStorage, and the SDK's store
      // reads from there on every access — the session it writes is
      // already visible here. The message below is only a nudge to
      // re-render; the `storage` listener in the effect above is the
      // backstop if it never arrives (a popup blocker, or someone
      // finishing sign-in in a tab they opened themselves).
      // A tab, not a sized popup. Passing width/height makes a popup
      // window, and wallet and Nostr extensions do not work reliably in
      // one: their own approval window takes focus, and the provider
      // treats a request from a backgrounded popup as abandoned —
      // reported by the user as OKX and Nostr both refusing to sign on
      // this page while both worked on the server's own /signin, which
      // OpenCapture opens as a tab (`ext.tabs.create`). Google was
      // unaffected either way, because a full-page OAuth redirect never
      // involves an extension.
      const signinTab = window.open("/login", "openpdfedit-signin");
      if (!signinTab) {
        showToast("Allow pop-ups for this site to sign in.", { tone: "warning", title: "Pop-up blocked" });
      }
      return;
    }
    // A fresh label each time — a closed WebviewWindow can't be reused,
    // and a reused label throws.
    new WebviewWindow(`login-${Date.now()}`, {
      url: "/login",
      title: "Sign in — OpenPdfEdit",
      width: 420,
      height: 640,
      resizable: false,
      parent: "main",
    });
  }

  function onKeydown(e: KeyboardEvent): void {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="oa-dialog-scrim"
    role="dialog"
    aria-modal="true"
    aria-label="Account"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="oa-dialog">
      <div class="oa-dialog__header">
        <div class="oa-dialog__header-text">
          <h2 class="oa-dialog__title">Account</h2>
        </div>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={onClose} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      </div>

      <div class="oa-dialog__body account-body">
        {#if loggedIn}
          <openapps-account></openapps-account>
          <openapps-credits poll-seconds="30"></openapps-credits>
          <openapps-buy></openapps-buy>
          <!-- Same reason the extension sets one: this is a Tauri webview,
               so the page's own URL is a tauri:// (or localhost) origin that
               means nothing to whoever the link is sent to. `app-id` lets
               the registered domain win once the server knows it, so this
               attribute only has to be right until then. -->
          <openapps-referral
            app-id="openpdfedit"
            invite-url="https://app.openpdfedit.com/"
          ></openapps-referral>
          <!-- Last, deliberately. It used to live inside
               <openapps-account>, which put it beside the balance — the
               one control here that ends a session, sitting where a
               stray click lands. Its own element means the platform
               still decides what it looks like and what it does, while
               the host decides where it goes. -->
          <openapps-signout></openapps-signout>
        {:else}
          <p class="message">Sign in to see your credits and buy more.</p>
          <button class="oa-btn oa-btn--primary" onclick={signIn}>Sign in</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .account-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .message {
    font: var(--type-body);
    color: var(--text-muted);
  }
</style>
