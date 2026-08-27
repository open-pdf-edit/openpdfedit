<script lang="ts">
  import "../app.css";
  // Registers <openapps-login>/<openapps-account>/<openapps-credits>/
  // <openapps-buy> as custom elements and configures the one client they
  // all share. No @openapps/tokens import here — this app already vendors
  // the same design tokens under lib/styles/tokens.css (see that file's
  // header), and the elements fall back to those semantic CSS vars
  // (--surface-card, --text-muted, etc.) across the shadow boundary.
  import { configure } from "@openapps/ui";
  import { OPENAPPS_BASE_URL } from "$lib/openapps";
  // Resolves which Backend implementation (Tauri today; wasm/extension in
  // Task 8) the rest of the app should call into, and installs it as
  // `backend` in $lib/backend — see that module's `initBackend` doc for
  // why this can't just be a top-level await there instead. Gating
  // `children()` on it means every component below (which assumes
  // `backend` is already the right implementation the moment it runs)
  // never observes the pre-resolution default.
  import { initBackend } from "$lib/backend";
  // Running inside Telegram changes theme, viewport and the back button, and
  // all three want to be right before the first paint rather than corrected
  // after it. A no-op in an ordinary browser — see $lib/telegram.
  import { initTelegram } from "$lib/telegram";

  // One definition, imported — never a second literal here. See
  // $lib/openapps for what this host is and why it isn't the backend's
  // own name.
  configure({ baseUrl: OPENAPPS_BASE_URL });

  let { children } = $props();
  const backendReady = initBackend();
  if (typeof window !== "undefined") initTelegram();
</script>

{#await backendReady then}
  {@render children()}
{/await}
