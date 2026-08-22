<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { OutlineEntryDto } from "./backend";

  interface Props {
    entries: OutlineEntryDto[];
    loading: boolean;
    /** The page currently in view, so the reader can see where they are
     * in the document's own structure without hunting for it. */
    currentPage: number;
    onGoToPage: (pageIndex: number) => void;
  }

  let { entries, loading, currentPage, onGoToPage }: Props = $props();

  /** The deepest entry at or before the current page — i.e. the section
   * the reader is actually inside. Entries with no page can't bound a
   * section, so they're skipped. */
  const activeIndex = $derived.by(() => {
    let best = -1;
    entries.forEach((entry, i) => {
      if (entry.pageIndex !== null && entry.pageIndex <= currentPage) best = i;
    });
    return best;
  });

  /** Indent per nesting level, in px. Enough to read as a hierarchy
   * without pushing deep entries off the panel. */
  const INDENT = 12;
</script>

<aside class="oa-panel">
  <div class="oa-panel__header">
    <span class="oa-panel__title">Contents</span>
  </div>
  <div class="oa-panel__body">
    {#if loading}
      <p class="oa-empty">Loading…</p>
    {:else if entries.length === 0}
      <p class="oa-empty">This document has no bookmarks.</p>
    {:else}
      <ul class="oa-list tight">
        {#each entries as entry, i (i)}
          <li>
            <button
              class="entry"
              class:entry--active={i === activeIndex}
              class:entry--section={entry.hasChildren}
              class:entry--dead={entry.pageIndex === null}
              style="padding-left: {8 + entry.depth * INDENT}px;"
              disabled={entry.pageIndex === null}
              onclick={() => entry.pageIndex !== null && onGoToPage(entry.pageIndex)}
              title={entry.pageIndex === null ? "This bookmark doesn't point at a page in this document" : undefined}
            >
              <span class="entry__title">{entry.title}</span>
              {#if entry.pageIndex !== null}
                <span class="oa-caption entry__page">{entry.pageIndex + 1}</span>
              {:else}
                <Icon name="info" size={12} />
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</aside>

<style>
  .tight {
    gap: 0;
  }

  .entry {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font: var(--type-body);
    color: var(--text-strong);
    transition: var(--transition-control);
  }
  .entry:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  .entry--active {
    background: var(--surface-selected);
  }
  .entry--section {
    font: var(--type-ui);
    color: var(--text-strong);
  }
  .entry--dead {
    cursor: default;
    color: var(--text-faint);
  }

  .entry__title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry__page {
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
</style>
