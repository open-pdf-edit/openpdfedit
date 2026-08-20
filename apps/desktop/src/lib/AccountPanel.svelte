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
  // <openapps-credits> / <openapps-buy>.
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
  import { getClient, onChange } from "@openapps/ui";
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
    return () => {
      cancelled = true;
      stopLocal();
      stopRemote?.();
    };
  });

  function signIn(): void {
    if (!tauriAvailable) {
      // Honest short-circuit rather than letting `new WebviewWindow(...)`
      // throw the same `transformCallback` TypeError `listen()` would
      // have. The extension's own sign-in flow is follow-up work (see
      // this module's doc comment) — this just says so instead of
      // crashing.
      showToast("Sign-in isn't available in the extension yet.", { tone: "warning", title: "Not available" });
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
