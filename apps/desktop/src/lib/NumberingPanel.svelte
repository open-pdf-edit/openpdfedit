<script lang="ts">
  // Page numbers and Bates numbering. A document-level tool opened from
  // the topbar and applied to every page in one command, the same shape
  // as WatermarkPanel — and deliberately separate from it: that one
  // tiles a repeating cell across whole pages, this one puts a single
  // label in a margin and changes it page to page.
  import Icon from "./Icon.svelte";
  import type { NumberPagesChoices } from "./backend/types";

  interface Props {
    open: boolean;
    busy: boolean;
    pageCount: number;
    onApply: (choices: NumberPagesChoices) => void;
    onClose: () => void;
  }

  let { open, busy, pageCount, onApply, onClose }: Props = $props();

  const MODES = [
    { id: "pageNumbers", label: "Page numbers" },
    { id: "bates", label: "Bates" },
  ] as const;

  const ANCHORS = [
    { id: "topLeft", label: "Top left" },
    { id: "topCenter", label: "Top centre" },
    { id: "topRight", label: "Top right" },
    { id: "bottomLeft", label: "Bottom left" },
    { id: "bottomCenter", label: "Bottom centre" },
    { id: "bottomRight", label: "Bottom right" },
  ];

  let mode = $state<(typeof MODES)[number]["id"]>("pageNumbers");
  let prefix = $state("");
  let suffix = $state("");
  let startAt = $state(1);
  let digits = $state(0);
  let anchor = $state("bottomCenter");
  let font = $state("helvetica");
  let fontSize = $state(10);
  let colorHex = $state("#000000");
  let margin = $state(24);

  /** Each mode's whole preset, applied on switch — picking "Bates" and
   * getting an unpadded number centred at the foot would be nobody's
   * intent. */
  function selectMode(next: (typeof MODES)[number]["id"]) {
    mode = next;
    if (next === "bates") {
      anchor = "bottomRight";
      fontSize = 9;
      digits = 6;
      margin = 24;
    } else {
      anchor = "bottomCenter";
      fontSize = 10;
      digits = 0;
      prefix = "";
      margin = 24;
    }
  }

  function hexToRgb(hex: string): [number, number, number] {
    const value = hex.replace("#", "");
    const n = parseInt(value.length === 3 ? value.replace(/./g, (c) => c + c) : value, 16);
    if (Number.isNaN(n)) return [0, 0, 0];
    return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
  }

  /** What the first numbered page will actually read. Four interacting
   * fields is more than anyone should have to simulate in their head. */
  const preview = $derived(`${prefix}${String(startAt).padStart(digits, "0")}${suffix}`);

  const canApply = $derived(!busy && preview.trim().length > 0);

  function submit() {
    if (!canApply) return;
    onApply({
      prefix,
      suffix,
      startAt,
      digits,
      anchor,
      font,
      fontSize,
      color: hexToRgb(colorHex),
      opacity: 1,
      margin,
      pages: null,
    });
  }

  function onKeydown(e: KeyboardEvent) {
    // A modal owns the keyboard while open, same as DialogHost.
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
    aria-label="Page numbers"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="oa-dialog">
      <div class="oa-dialog__header">
        <h2 class="oa-dialog__title">Number pages</h2>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={onClose} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      </div>

      <div class="oa-dialog__body">
        <div class="modes" role="tablist">
          {#each MODES as m (m.id)}
            <button
              class="mode"
              class:mode--on={mode === m.id}
              role="tab"
              aria-selected={mode === m.id}
              onclick={() => selectMode(m.id)}
            >
              {m.label}
            </button>
          {/each}
        </div>

        <div class="row">
          <label class="field">
            <span class="field__label">Prefix</span>
            <input class="oa-input" bind:value={prefix} spellcheck="false" placeholder={mode === "bates" ? "ACME-" : ""} />
          </label>
          <label class="field narrow">
            <span class="field__label">Start at</span>
            <input class="oa-input" type="number" min="0" bind:value={startAt} />
          </label>
          <label class="field narrow">
            <span class="field__label">Digits</span>
            <input class="oa-input" type="number" min="0" max="20" bind:value={digits} />
          </label>
          <label class="field">
            <span class="field__label">Suffix</span>
            <input class="oa-input" bind:value={suffix} spellcheck="false" placeholder={mode === "pageNumbers" ? ` of ${pageCount}` : ""} />
          </label>
        </div>

        <p class="preview">First page will read <strong>{preview || "—"}</strong></p>

        <div class="row">
          <label class="field">
            <span class="field__label">Position</span>
            <select class="oa-input" bind:value={anchor}>
              {#each ANCHORS as a (a.id)}
                <option value={a.id}>{a.label}</option>
              {/each}
            </select>
          </label>
          <label class="field">
            <span class="field__label">Font</span>
            <select class="oa-input" bind:value={font}>
              <option value="helvetica">Helvetica</option>
              <option value="helveticaBold">Helvetica Bold</option>
              <option value="timesRoman">Times</option>
              <option value="timesBold">Times Bold</option>
              <option value="courier">Courier</option>
            </select>
          </label>
          <label class="field narrow">
            <span class="field__label">Size</span>
            <input class="oa-input" type="number" min="4" max="96" bind:value={fontSize} />
          </label>
          <label class="field narrow">
            <span class="field__label">Margin</span>
            <input class="oa-input" type="number" min="0" max="200" bind:value={margin} />
          </label>
          <label class="field colour">
            <span class="field__label">Colour</span>
            <input class="oa-input colour-input" type="color" bind:value={colorHex} />
          </label>
        </div>

        <p class="note">
          Numbers are added as real page content, so they stay put wherever the file is opened.
          Built-in PDF fonts, which cover Latin characters only. ⌘Z undoes it.
        </p>
      </div>

      <div class="oa-dialog__footer">
        <button class="oa-btn oa-btn--secondary" onclick={onClose}>Cancel</button>
        <button class="oa-btn oa-btn--primary" onclick={submit} disabled={!canApply}>
          <Icon name="hash" size={15} spin={busy} />
          {busy ? "Numbering…" : `Number all ${pageCount} pages`}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modes {
    display: flex;
    gap: 2px;
    margin-bottom: var(--space-3);
  }
  .mode {
    flex: 1;
    height: var(--control-h-sm);
    border: var(--border-width) solid var(--border-hairline);
    background: transparent;
    color: var(--text-muted);
    font: var(--type-ui);
    cursor: pointer;
    transition: var(--transition-control);
  }
  .mode:first-child {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }
  .mode:last-child {
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }
  .mode:hover {
    background: var(--surface-hover);
  }
  .mode--on {
    background: var(--surface-selected);
    color: var(--text-strong);
  }

  .row {
    display: flex;
    gap: var(--space-3);
    align-items: flex-end;
  }
  .row > .field {
    flex: 1;
    min-width: 0;
  }
  .narrow {
    flex: 0 0 5.5rem !important;
  }
  .colour {
    flex: 0 0 4rem !important;
  }
  .colour-input {
    padding: 2px;
    height: var(--control-h-sm);
  }

  .field {
    display: grid;
    gap: var(--space-1);
    margin: 0 0 var(--space-3);
    min-width: 0;
  }
  .field__label {
    font: var(--type-caption);
    color: var(--text-muted);
  }

  .preview {
    margin: 0 0 var(--space-3);
    font: var(--type-caption);
    color: var(--text-muted);
  }
  .preview strong {
    color: var(--text-strong);
    font-variant-numeric: tabular-nums;
  }

  .note {
    margin: var(--space-3) 0 0;
    font: var(--type-caption);
    color: var(--text-muted);
  }
</style>
