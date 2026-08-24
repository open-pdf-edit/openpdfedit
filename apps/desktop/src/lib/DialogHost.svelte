<script lang="ts">
  // Renders whatever dialog `dialog.svelte.ts` currently has open. Mounted
  // once, at the app root — see that module's header for why these exist
  // at all (native window.prompt/confirm do not function in this webview).
  import { activeDialog, resolveDialog } from "./dialog.svelte";
  import Icon from "./Icon.svelte";

  const dialog = $derived(activeDialog());
  let value = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);

  // Seed the field each time a new prompt opens, and focus it so the user
  // can type immediately (and press Enter/Escape) without reaching for
  // the mouse.
  $effect(() => {
    if (dialog?.kind === "prompt") {
      value = dialog.defaultValue;
      queueMicrotask(() => inputEl?.select());
    }
  });

  function confirm() {
    resolveDialog(dialog?.kind === "prompt" ? value : "");
  }
  function cancel() {
    resolveDialog(null);
  }
  function onKeydown(e: KeyboardEvent) {
    // Stopping propagation keeps the app's global shortcuts (⌘Z and
    // friends) from firing while a modal owns the keyboard.
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      confirm();
    }
  }
</script>

{#if dialog}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="oa-dialog-scrim"
    role="dialog"
    aria-modal="true"
    aria-label={dialog.title}
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="oa-dialog">
      <div class="oa-dialog__header">
        <div class="oa-dialog__header-text">
          <h2 class="oa-dialog__title">{dialog.title}</h2>
        </div>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={cancel} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      </div>

      <div class="oa-dialog__body">
        <p class="message">{dialog.message}</p>

        {#if dialog.kind === "prompt"}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="oa-input"
            bind:this={inputEl}
            bind:value
            type={dialog.masked ? "password" : "text"}
            autocomplete={dialog.masked ? "current-password" : "off"}
            placeholder={dialog.placeholder}
            autofocus
            spellcheck="false"
          />
        {/if}
      </div>

      <div class="oa-dialog__footer">
        {#if dialog.kind !== "alert"}
          <button class="oa-btn oa-btn--secondary" onclick={cancel}>Cancel</button>
        {/if}
        <button class="oa-btn" class:oa-btn--danger={dialog.destructive} class:oa-btn--primary={!dialog.destructive} onclick={confirm}>
          {dialog.confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .oa-dialog__header-text {
    flex: 1;
    display: grid;
    gap: 6px;
  }

  .message {
    font: var(--type-body);
    color: var(--text-muted);
    white-space: pre-wrap;
  }

  .oa-dialog__body .oa-input {
    margin-top: var(--space-3);
  }
</style>
