<script lang="ts">
  import { showAlert, showConfirm, showPrompt } from "./dialog.svelte";
  import PageThumb from "./PageThumb.svelte";
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";

  interface PageSize {
    width: number;
    height: number;
  }

  interface Props {
    handle: number;
    pageSizes: PageSize[];
    busy: boolean;
    onRotate: (pageIndex: number, deltaDegrees: number) => void;
    onDelete: (pageIndex: number) => void;
    onMove: (pageIndex: number, direction: "Up" | "Down") => void;
    onCrop: (pageIndex: number, rect: [number, number, number, number]) => void;
    onExtractSelected: (pageIndices: number[]) => void;
    onMerge: () => void;
  }

  let { handle, pageSizes, busy, onRotate, onDelete, onMove, onCrop, onExtractSelected, onMerge }: Props = $props();

  let selected = $state<Set<number>>(new Set());

  function toggleSelected(pageIndex: number) {
    const next = new Set(selected);
    if (next.has(pageIndex)) {
      next.delete(pageIndex);
    } else {
      next.add(pageIndex);
    }
    selected = next;
  }

  function extractSelected() {
    onExtractSelected([...selected].sort((a, b) => a - b));
  }

  // Crop is the one page op with no natural single-click affordance (it
  // needs four numbers), so it's the one op here that falls back to a
  // prompt — same pattern PdfPage.svelte already uses for FreeText note
  // contents, rather than building a dedicated drag-a-rectangle mode.
  async function promptCrop(pageIndex: number, size: PageSize) {
    const input = await showPrompt(
      `Crop box for page ${pageIndex + 1}, as "left,bottom,right,top" in PDF points ` +
        `(page is ${Math.round(size.width)}×${Math.round(size.height)}pt):`,
      { title: "Set crop box", confirmLabel: "Crop" },
    );
    if (!input) return;
    const rect = input.split(",").map((part) => Number(part.trim()));
    if (rect.length !== 4 || rect.some((n) => Number.isNaN(n))) {
      await showAlert("Expected four comma-separated numbers, e.g. 10,10,400,500");
      return;
    }
    onCrop(pageIndex, rect as [number, number, number, number]);
  }

  async function confirmDelete(pageIndex: number) {
    if (await showConfirm(`Delete page ${pageIndex + 1}? You can undo this with ⌘Z.`, { title: "Delete page?", confirmLabel: "Delete", destructive: true })) {
      onDelete(pageIndex);
    }
  }
</script>

<aside class="oa-panel">
  <div class="oa-panel__header">
    <span class="oa-panel__title">Pages</span>
  </div>
  <div class="oa-panel__body">
    <div class="actions">
      <button class="oa-btn oa-btn--secondary" onclick={onMerge} disabled={busy}>
        <Icon name="combine" size={15} />
        Merge PDFs…
      </button>
      <button class="oa-btn oa-btn--secondary" onclick={extractSelected} disabled={busy || selected.size === 0}>
        <Icon name="file-output" size={15} />
        Extract selected ({selected.size})
      </button>
    </div>

    <ul class="oa-list">
      {#each pageSizes as size, i (i)}
        <li class="oa-list-item row">
          <label class="select-box">
            <input class="oa-checkbox" type="checkbox" checked={selected.has(i)} onchange={() => toggleSelected(i)} aria-label={`Select page ${i + 1}`} />
          </label>
          <PageThumb {handle} pageIndex={i} widthPt={size.width} heightPt={size.height} />
          <div class="row-body">
            <span class="oa-caption page-number">p.{i + 1}</span>
            <div class="row-actions">
              <button class="oa-icon-btn oa-icon-btn--sm" onclick={() => onMove(i, "Up")} disabled={busy || i === 0} aria-label="Move up" use:tooltip={"Move up"}>
                <Icon name="chevron-up" size={14} />
              </button>
              <button class="oa-icon-btn oa-icon-btn--sm" onclick={() => onMove(i, "Down")} disabled={busy || i === pageSizes.length - 1} aria-label="Move down" use:tooltip={"Move down"}>
                <Icon name="chevron-down" size={14} />
              </button>
              <button class="oa-icon-btn oa-icon-btn--sm icon-flip" onclick={() => onRotate(i, -90)} disabled={busy} aria-label="Rotate left" use:tooltip={"Rotate left"}>
                <Icon name="rotate-cw" size={14} />
              </button>
              <button class="oa-icon-btn oa-icon-btn--sm" onclick={() => onRotate(i, 90)} disabled={busy} aria-label="Rotate right" use:tooltip={"Rotate right"}>
                <Icon name="rotate-cw" size={14} />
              </button>
              <button class="oa-icon-btn oa-icon-btn--sm" onclick={() => promptCrop(i, size)} disabled={busy} aria-label="Crop" use:tooltip={"Crop"}>
                <Icon name="crop" size={14} />
              </button>
              <button
                class="oa-icon-btn oa-icon-btn--sm danger"
                onclick={() => confirmDelete(i)}
                disabled={busy || pageSizes.length <= 1}
                aria-label="Delete page"
                use:tooltip={"Delete page"}
              >
                <Icon name="trash-2" size={14} />
              </button>
            </div>
          </div>
        </li>
      {/each}
    </ul>
  </div>
</aside>

<style>
  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .select-box {
    display: flex;
    flex-shrink: 0;
  }

  .row-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  .row-actions {
    display: flex;
    gap: 2px;
    flex-wrap: wrap;
  }

  .row-actions .oa-icon-btn {
    width: 24px;
    height: 24px;
  }

  /* No separate "rotate left" glyph is vendored — mirror "rotate-cw"
     instead of shipping a second near-identical SVG. */
  .icon-flip :global(.oa-icon) {
    transform: scaleX(-1);
  }

  .danger:hover:not(:disabled) {
    color: var(--danger-fg);
    background: var(--danger-bg);
  }
</style>
