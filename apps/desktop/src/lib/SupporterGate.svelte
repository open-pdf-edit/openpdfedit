<script lang="ts">
  // The only thing in OpenPdfEdit you pay for: a one-time, 1000-credit
  // unlock of the watermark tool. Every other tool works with no account
  // at all, and nothing here is checked anywhere else.
  //
  // "One-time" is a server property, not a promise made here: the charge
  // is idempotent per user (see $lib/openapps's `unlockSupporter`), and
  // what it buys is an entitlement the server records, not a flag this
  // client keeps. Reinstalling, switching machines, or clearing storage
  // therefore can't cost a second 1000 credits — the entitlement check
  // on the next sign-in finds it already redeemed.
  import Icon from "./Icon.svelte";
  import { SUPPORTER_COST } from "./openapps";

  /** What the gate is currently showing. `hidden` is the resting state —
   * the gate exists only while a decision is pending, and closes itself
   * the moment the tool is available. */
  export type GateState =
    | { kind: "hidden" }
    | { kind: "checking" }
    | { kind: "signed-out" }
    | { kind: "locked" }
    | { kind: "unlocking" }
    | { kind: "insufficient"; have: number; need: number }
    | { kind: "error"; message: string };

  interface Props {
    state: GateState;
    /** Which tool the user just reached for. One unlock covers both, so
     * this only changes what the panel is *about* — never what is
     * bought. */
    tool?: "watermark" | "ocr";
    /** Open the account panel — sign in, or top up. */
    onAccount: () => void;
    /** Spend the credits. */
    onUnlock: () => void;
    /** Try the whole check again after a failure. */
    onRetry: () => void;
    onClose: () => void;
  }

  let { state, tool = "watermark", onAccount, onUnlock, onRetry, onClose }: Props = $props();

  const title = $derived(tool === "ocr" ? "OCR" : "Watermark");

  const price = SUPPORTER_COST.toLocaleString();

  function onKeydown(e: KeyboardEvent): void {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

{#if state.kind !== "hidden"}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="oa-dialog-scrim"
    role="dialog"
    aria-modal="true"
    aria-label="Supporter feature"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="oa-dialog gate-dialog">
      <div class="oa-dialog__header">
        <div class="oa-dialog__header-text">
          <h2 class="oa-dialog__title">{title}</h2>
          <p class="oa-dialog__subtitle">A Supporter feature — {price} credits, once, for both Supporter tools.</p>
        </div>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={onClose} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      </div>

      <div class="oa-dialog__body gate-body">
        {#if state.kind === "checking"}
          <p class="gate-message">Checking your account…</p>
        {:else if state.kind === "signed-out"}
          <p class="gate-message">
            Sign in to unlock {title}. One {price}-credit purchase covers the watermark
            <em>and</em> OCR, on every device you sign in on.
          </p>
        {:else if state.kind === "locked"}
          <p class="gate-message">
            {tool === "ocr"
              ? "Make a scan searchable — select, search and copy its text."
              : "Tile text or a logo across every page."}
            {price} credits, charged once, and it unlocks both Supporter tools — not a subscription,
            and not per document.
          </p>
        {:else if state.kind === "unlocking"}
          <p class="gate-message">Unlocking…</p>
        {:else if state.kind === "insufficient"}
          <p class="gate-message">
            You have {state.have.toLocaleString()}
            {state.have === 1 ? "credit" : "credits"} — unlocking costs {state.need.toLocaleString()}.
          </p>
        {:else if state.kind === "error"}
          <p class="gate-message gate-message--error">{state.message}</p>
        {/if}
      </div>

      <div class="oa-dialog__footer">
        <button class="oa-btn oa-btn--secondary" onclick={onClose}>
          {state.kind === "unlocking" ? "Close" : "Not now"}
        </button>
        {#if state.kind === "signed-out"}
          <button class="oa-btn oa-btn--primary" onclick={onAccount}>Sign in</button>
        {:else if state.kind === "locked"}
          <button class="oa-btn oa-btn--primary" onclick={onUnlock}>Unlock for {price} credits</button>
        {:else if state.kind === "insufficient"}
          <button class="oa-btn oa-btn--primary" onclick={onAccount}>Buy credits</button>
        {:else if state.kind === "error"}
          <button class="oa-btn oa-btn--primary" onclick={onRetry}>Try again</button>
        {:else if state.kind === "unlocking"}
          <button class="oa-btn oa-btn--primary" disabled>
            <Icon name="loader-circle" size={14} spin={true} />
            Unlocking
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .gate-dialog {
    width: min(400px, calc(100vw - 48px));
  }
  .oa-dialog__subtitle {
    font-size: 12.5px;
    color: var(--text-muted, #656565);
    margin-top: 2px;
  }
  .gate-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .gate-message {
    font-size: 13.5px;
    line-height: 1.5;
    color: var(--text-body, #282828);
  }
  .gate-message--error {
    color: var(--danger-fg, #d43d3d);
  }
</style>
