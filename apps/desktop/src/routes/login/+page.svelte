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

  let containerEl: HTMLDivElement | null = null;

  async function finish(): Promise<void> {
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
  <openapps-login variant="panel"></openapps-login>
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
