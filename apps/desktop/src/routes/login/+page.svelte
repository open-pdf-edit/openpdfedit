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
  import { SIGNIN_DONE_MESSAGE } from "$lib/openapps";

  let containerEl: HTMLDivElement | null = null;

  // Same check AccountPanel makes, and for the same reason: these calls
  // reach into `window.__TAURI_INTERNALS__` when invoked, so in a browser
  // they throw rather than reject and no `.catch` would help.
  const tauriAvailable = "__TAURI_INTERNALS__" in window;

  async function finish(): Promise<void> {
    if (!tauriAvailable) {
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
    // races with the main window's own check) — nothing to do here.
    if (getClient()?.isLoggedIn) {
      void finish();
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
