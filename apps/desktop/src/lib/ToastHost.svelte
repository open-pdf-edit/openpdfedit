<script lang="ts">
  // Renders whatever toast.svelte.ts currently has active. Mounted once
  // at the app root, same pattern as DialogHost — see that file's header.
  import { activeToast, dismissToast, type ToastTone } from "./toast.svelte";
  import Icon from "./Icon.svelte";

  const toast = $derived(activeToast());

  const TONE_ICON: Record<ToastTone, string> = {
    neutral: "info",
    success: "circle-check",
    warning: "triangle-alert",
    danger: "circle-x",
  };
  const TONE_CLASS: Record<ToastTone, string> = {
    neutral: "",
    success: "tone-success",
    warning: "tone-warning",
    danger: "tone-danger",
  };
</script>

{#if toast}
  {#key toast.id}
    <div class="toast {TONE_CLASS[toast.tone]}" role="status">
      <Icon name={TONE_ICON[toast.tone]} size={16} />
      <div class="toast__body">
        {#if toast.title}<span class="toast__title">{toast.title}</span>{/if}
        <span class="toast__message">{toast.message}</span>
      </div>
      <button class="oa-icon-btn oa-icon-btn--sm" onclick={dismissToast} aria-label="Dismiss">
        <Icon name="x" size={15} />
      </button>
    </div>
  {/key}
{/if}

<style>
  .toast {
    position: fixed;
    left: var(--space-4);
    bottom: var(--space-4);
    z-index: 900;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    width: 340px;
    max-width: calc(100vw - 2 * var(--space-4));
    padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    background: var(--surface-card);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    color: var(--text-muted);
    animation: oa-fade-up var(--duration-slow) var(--ease-out);
  }

  .toast :global(.oa-icon) {
    margin-top: 1px;
    flex: 0 0 auto;
  }

  .tone-success {
    color: var(--success-fg);
  }
  .tone-warning {
    color: var(--warning-fg);
  }
  .tone-danger {
    color: var(--danger-fg);
  }

  .toast__body {
    display: grid;
    gap: 3px;
    flex: 1;
    min-width: 0;
  }

  .toast__title {
    font: var(--type-ui);
    color: var(--text-strong);
  }

  .toast__message {
    font: var(--type-caption);
    color: var(--text-muted);
  }
</style>
