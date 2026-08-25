<script lang="ts">
  // "Install this" — shown only in the web build, only when the browser
  // will actually do something about it, and never once installed.
  //
  // Two browsers, two mechanisms. Chromium fires `beforeinstallprompt`,
  // which can be stashed and replayed from a click; Safari fires nothing
  // and has no API at all, so the only honest thing to offer an iPhone
  // is a sentence describing where the button is. Anything that looked
  // like an install button there would be a button that cannot install.
  import { backendKind } from "$lib/backend";
  import Icon from "./Icon.svelte";

  /** The event Chromium hands over so a page can ask later, at a moment
   * of its choosing rather than on load. Typed narrowly — it is not in
   * lib.dom, and only these two members are used. */
  interface InstallEvent extends Event {
    prompt(): Promise<void>;
    userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
  }

  let deferred = $state<InstallEvent | null>(null);
  let dismissed = $state(false);
  let installing = $state(false);

  /** Already running as an installed app, by either platform's reckoning.
   * `standalone` is Safari's own, non-standard flag, and is the only way
   * to know on iOS. */
  const alreadyInstalled =
    typeof window !== "undefined" &&
    (window.matchMedia("(display-mode: standalone)").matches ||
      (navigator as { standalone?: boolean }).standalone === true);

  /** iOS Safari, where Add to Home Screen exists but only by hand.
   *
   * Sniffing, reluctantly: there is nothing to feature-detect, because
   * the distinguishing trait is the *absence* of an API and absence
   * cannot be observed at a point in time. The iPad clause is the part
   * that is easy to get wrong — iPadOS has reported itself as a Mac
   * since version 13, so a touch count is what separates an iPad from a
   * desktop. Being wrong here costs a sentence shown to someone who
   * cannot act on it, which is why it is a sentence and not a button. */
  const isIosSafari = (() => {
    if (typeof navigator === "undefined") return false;
    const ua = navigator.userAgent;
    const isIos =
      /iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
    // Every iOS browser is WebKit underneath, but only Safari offers Add
    // to Home Screen — Chrome and Firefox for iOS do not.
    const isSafari = /Safari/.test(ua) && !/CriOS|FxiOS|EdgiOS|OPiOS/.test(ua);
    return isIos && isSafari;
  })();

  /** Where the page's own head script parks the event if it fires before
   * this component exists — which it usually does. See the web app's
   * build script: Chromium fires `beforeinstallprompt` as soon as it
   * decides the page qualifies, never replays it, and hydration is not
   * fast enough to rely on. Without this the offer appears only when the
   * app happens to win that race. */
  interface WindowWithEarlyPrompt extends Window {
    __installPromptEvent?: InstallEvent | null;
  }

  $effect(() => {
    if (backendKind !== "wasm" || alreadyInstalled) return;

    // Anything caught before this component existed.
    const early = (window as WindowWithEarlyPrompt).__installPromptEvent;
    if (early) deferred = early;

    // Fired by that same head script, for the case where it catches the
    // event between this effect being scheduled and running.
    const onEarly = () => {
      const stashed = (window as WindowWithEarlyPrompt).__installPromptEvent;
      if (stashed) deferred = stashed;
    };
    window.addEventListener("openpdfedit:installable", onEarly);

    const onPrompt = (event: Event) => {
      // Chromium would otherwise show its own mini-infobar; taking the
      // event lets the offer sit where it makes sense instead.
      event.preventDefault();
      deferred = event as InstallEvent;
    };
    const onInstalled = () => {
      deferred = null;
      (window as WindowWithEarlyPrompt).__installPromptEvent = null;
      dismissed = true;
    };
    window.addEventListener("beforeinstallprompt", onPrompt);
    window.addEventListener("appinstalled", onInstalled);
    return () => {
      window.removeEventListener("openpdfedit:installable", onEarly);
      window.removeEventListener("beforeinstallprompt", onPrompt);
      window.removeEventListener("appinstalled", onInstalled);
    };
  });

  async function install(): Promise<void> {
    const event = deferred;
    if (!event) return;
    installing = true;
    try {
      await event.prompt();
      await event.userChoice;
    } finally {
      // Single use: Chromium will not replay the same event, so holding
      // on to it would leave a button that silently does nothing.
      deferred = null;
      (window as WindowWithEarlyPrompt).__installPromptEvent = null;
      installing = false;
    }
  }

  // Only in the browser build, only when there is something to say, and
  // never after it has been waved away.
  const show = $derived(
    backendKind === "wasm" && !alreadyInstalled && !dismissed && (deferred !== null || isIosSafari),
  );
</script>

{#if show}
  <div class="install">
    {#if deferred}
      <button class="oa-btn oa-btn--secondary" onclick={install} disabled={installing}>
        <Icon name="plus" size={14} />
        Install as an app
      </button>
      <span class="install__why">Opens in its own window and works offline.</span>
    {:else}
      <!-- Safari has no install API, so this is a sentence, not a
           button. Describing the steps is the only thing that can
           actually help here. -->
      <span class="install__why">
        <Icon name="plus" size={14} />
        Add to your home screen: tap Share, then <strong>Add to Home Screen</strong>.
      </span>
    {/if}
    <button class="install__dismiss" onclick={() => (dismissed = true)} aria-label="Dismiss">
      <Icon name="x" size={13} />
    </button>
  </div>
{/if}

<style>
  .install {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-5);
    padding: var(--space-3) var(--space-4);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-lg);
    background: var(--bg-subtle);
    max-width: 34rem;
  }

  .install__why {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--text-muted);
    text-align: left;
  }

  .install__dismiss {
    display: inline-flex;
    align-items: center;
    padding: 4px;
    border: 0;
    border-radius: var(--radius-sm, 6px);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .install__dismiss:hover {
    color: var(--text-strong);
  }
</style>
