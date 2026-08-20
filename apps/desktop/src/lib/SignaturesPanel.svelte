<script lang="ts">
  // Manage saved signatures and pick which one is "armed" for placement.
  // Drawing itself happens in SignaturePad.svelte (opened from here);
  // placing an armed signature onto a page is a drag-rectangle gesture
  // handled by PdfPage.svelte/+page.svelte, the same shape addTextField/
  // addCheckbox already use.
  import { savedSignatures, removeSignature, type SavedSignature } from "./signatures.svelte";
  import { showConfirm } from "./dialog.svelte";
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";

  interface Props {
    armedId: string | null;
    onArm: (id: string) => void;
    onNew: () => void;
  }

  let { armedId, onArm, onNew }: Props = $props();

  const signatures = $derived(savedSignatures());

  // A small inline SVG preview from the normalized strokes — cheap and
  // needs no per-row canvas/render loop.
  function previewPath(sig: SavedSignature): string {
    const h = 100 / sig.aspect;
    return sig.strokes
      .map((stroke) => stroke.map(([x, y], i) => `${i === 0 ? "M" : "L"}${(x * 100).toFixed(1)},${(y * h).toFixed(1)}`).join(" "))
      .join(" ");
  }
  function previewViewBox(sig: SavedSignature): string {
    return `0 0 100 ${(100 / sig.aspect).toFixed(1)}`;
  }

  async function confirmRemove(sig: SavedSignature) {
    if (await showConfirm(`Delete the saved signature "${sig.name}"?`, { title: "Delete signature?", confirmLabel: "Delete", destructive: true })) {
      removeSignature(sig.id);
    }
  }
</script>

<aside class="oa-panel">
  <div class="oa-panel__header">
    <span class="oa-panel__title">Signatures</span>
  </div>
  <div class="oa-panel__body">
    <button class="oa-btn oa-btn--secondary new-btn" onclick={onNew}>
      <Icon name="plus" size={15} />
      New signature
    </button>

    {#if signatures.length === 0}
      <p class="oa-empty">
        No saved signatures yet. Draw one, then drag on the page anywhere you want it to appear — reuse it on any document from here on.
      </p>
    {:else}
      <ul class="oa-list">
        {#each signatures as sig (sig.id)}
          <li class="oa-list-item row" class:armed={armedId === sig.id}>
            <button
              class="preview-btn"
              onclick={() => onArm(sig.id)}
              aria-label={`Use signature "${sig.name}"`}
              use:tooltip={"Use this signature — then drag on the page to place it"}
            >
              <svg class="preview" viewBox={previewViewBox(sig)} preserveAspectRatio="xMidYMid meet">
                <path d={previewPath(sig)} fill="none" stroke="#020202" stroke-width="4" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
            <div class="row-body">
              <span class="oa-caption sig-name">{sig.name}</span>
              {#if armedId === sig.id}
                <span class="oa-tag armed-tag">selected</span>
              {/if}
            </div>
            <button class="oa-icon-btn oa-icon-btn--sm danger" onclick={() => confirmRemove(sig)} aria-label="Delete signature" use:tooltip={"Delete signature"}>
              <Icon name="trash-2" size={14} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</aside>

<style>
  .new-btn {
    width: 100%;
    margin-bottom: var(--space-2);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .row.armed {
    border-color: var(--app-pdfedit);
    box-shadow: 0 0 0 1px var(--app-pdfedit);
  }

  .preview-btn {
    flex: 0 0 auto;
    width: 64px;
    height: 40px;
    padding: 4px;
    background: var(--white);
    border: var(--border-width) solid var(--border-hairline);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .preview {
    width: 100%;
    height: 100%;
    display: block;
  }

  .row-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .sig-name {
    color: var(--text-strong);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .armed-tag {
    align-self: flex-start;
    color: var(--app-pdfedit);
    border-color: color-mix(in oklab, var(--app-pdfedit) 50%, transparent);
  }

  .danger:hover:not(:disabled) {
    color: var(--danger-fg);
    background: var(--danger-bg);
  }
</style>
