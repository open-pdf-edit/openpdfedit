<script lang="ts">
  // Save a password-protected copy. Export-shaped, like Compress: it
  // writes a new file and leaves the open document alone — encrypting in
  // place would immediately require the password to render the document
  // still on screen.
  import Icon from "./Icon.svelte";
  import type { EncryptChoices } from "./backend/types";

  interface Props {
    open: boolean;
    busy: boolean;
    onApply: (choices: EncryptChoices) => void;
    onClose: () => void;
  }

  let { open, busy, onApply, onClose }: Props = $props();

  let userPassword = $state("");
  let confirmPassword = $state("");
  let useSeparateOwner = $state(false);
  let ownerPassword = $state("");
  let reveal = $state(false);

  let allowPrint = $state(true);
  let allowModify = $state(true);
  let allowCopy = $state(true);
  let allowAnnotate = $state(true);

  const mismatch = $derived(confirmPassword.length > 0 && confirmPassword !== userPassword);
  const canApply = $derived(
    !busy && userPassword.length > 0 && confirmPassword === userPassword,
  );

  /** Rough strength feedback. Deliberately not a score out of 100: the
   * only thing worth communicating is "this is short enough to guess",
   * and a precise-looking number invites treating it as a guarantee. */
  const strength = $derived.by(() => {
    const pw = userPassword;
    if (pw.length === 0) return null;
    const classes = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^a-zA-Z0-9]/].filter((r) => r.test(pw)).length;
    if (pw.length < 8) return { label: "Short — easy to guess", tone: "weak" };
    if (pw.length >= 12 && classes >= 3) return { label: "Strong", tone: "strong" };
    return { label: "Reasonable", tone: "ok" };
  });

  function submit() {
    if (!canApply) return;
    onApply({
      userPassword,
      ownerPassword: useSeparateOwner ? ownerPassword : "",
      allowPrint,
      allowModify,
      allowCopy,
      allowAnnotate,
    });
  }

  function onKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Enter") {
      e.preventDefault();
      submit();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="oa-dialog-scrim"
    role="dialog"
    aria-modal="true"
    aria-label="Protect with a password"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="oa-dialog">
      <div class="oa-dialog__header">
        <h2 class="oa-dialog__title">Protect with a password</h2>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={onClose} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      </div>

      <div class="oa-dialog__body">
        <label class="field">
          <span class="field__label">Password</span>
          <input
            class="oa-input"
            type={reveal ? "text" : "password"}
            bind:value={userPassword}
            autocomplete="new-password"
            spellcheck="false"
          />
        </label>

        <label class="field">
          <span class="field__label">Confirm password</span>
          <input
            class="oa-input"
            class:input--invalid={mismatch}
            type={reveal ? "text" : "password"}
            bind:value={confirmPassword}
            autocomplete="new-password"
            spellcheck="false"
          />
        </label>

        <div class="meta">
          <label class="checkbox">
            <input type="checkbox" bind:checked={reveal} />
            Show password
          </label>
          {#if mismatch}
            <span class="mismatch">The passwords don't match.</span>
          {:else if strength}
            <span class="strength strength--{strength.tone}">{strength.label}</span>
          {/if}
        </div>

        <p class="note warn">
          <Icon name="triangle-alert" size={14} />
          There is no way to recover this password. If it's lost, the document can't be opened —
          by you or anyone else.
        </p>

        <label class="checkbox separate">
          <input type="checkbox" bind:checked={useSeparateOwner} />
          Use a separate owner password for full permissions
        </label>
        {#if useSeparateOwner}
          <label class="field">
            <span class="field__label">Owner password</span>
            <input
              class="oa-input"
              type={reveal ? "text" : "password"}
              bind:value={ownerPassword}
              autocomplete="new-password"
              spellcheck="false"
            />
          </label>
        {/if}

        <fieldset class="field">
          <legend class="field__label">Allow the recipient to</legend>
          <label class="checkbox"><input type="checkbox" bind:checked={allowPrint} /> Print</label>
          <label class="checkbox"><input type="checkbox" bind:checked={allowCopy} /> Copy text</label>
          <label class="checkbox"><input type="checkbox" bind:checked={allowModify} /> Change the document</label>
          <label class="checkbox"><input type="checkbox" bind:checked={allowAnnotate} /> Comment and fill in forms</label>
        </fieldset>
        <p class="note">
          These are honoured by convention: readers are asked to respect them, nothing enforces
          them. The password is what actually protects the file. Text extraction stays available to
          screen readers regardless.
        </p>
      </div>

      <div class="oa-dialog__footer">
        <button class="oa-btn oa-btn--secondary" onclick={onClose}>Cancel</button>
        <button class="oa-btn oa-btn--primary" onclick={submit} disabled={!canApply}>
          <Icon name="key" size={15} spin={busy} />
          {busy ? "Encrypting…" : "Save protected copy…"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .field {
    display: grid;
    gap: var(--space-1);
    border: 0;
    padding: 0;
    margin: 0 0 var(--space-3);
  }
  .field__label {
    font: var(--type-caption);
    color: var(--text-muted);
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--type-body);
    color: var(--text-strong);
  }
  .separate {
    margin: var(--space-3) 0;
  }

  .input--invalid {
    border-color: var(--danger-fg);
  }

  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .strength {
    font: var(--type-caption);
  }
  .strength--weak {
    color: var(--danger-fg);
  }
  .strength--ok {
    color: var(--warning-fg);
  }
  .strength--strong {
    color: var(--success-fg, var(--text-muted));
  }

  .mismatch {
    font: var(--type-caption);
    color: var(--danger-fg);
  }

  .note {
    margin: 0 0 var(--space-3);
    font: var(--type-caption);
    color: var(--text-muted);
  }

  .note.warn {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--warning-bg);
    color: var(--warning-fg);
  }
</style>
