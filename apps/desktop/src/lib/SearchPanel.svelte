<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { SearchHitDto } from "./backend";

  interface Props {
    hits: SearchHitDto[];
    activeIndex: number;
    /** True while a query is in flight — the list keeps showing the
     * previous results rather than blanking, so typing another character
     * doesn't flash the panel empty. */
    busy: boolean;
    /** True once a query has actually been run, so "no matches" is only
     * shown after a real search rather than on an empty box. */
    searched: boolean;
    /** True when the backend hit its result cap. */
    truncated: boolean;
    onSelect: (index: number) => void;
  }

  let { hits, activeIndex, busy, searched, truncated, onSelect }: Props = $props();
</script>

<aside class="oa-panel">
  <div class="oa-panel__header">
    <span class="oa-panel__title">Results</span>
    {#if hits.length > 0}
      <span class="oa-caption">{hits.length}{truncated ? "+" : ""}</span>
    {/if}
  </div>
  <div class="oa-panel__body">
    {#if hits.length === 0}
      <p class="oa-empty">
        {#if busy}Searching…{:else if searched}No matches.{:else}Type to search this document.{/if}
      </p>
    {:else}
      {#if truncated}
        <p class="oa-caption capped">
          Showing the first {hits.length} matches. Narrow the search to see the rest.
        </p>
      {/if}
      <ul class="oa-list">
        {#each hits as hit, i (i)}
          <li>
            <button
              class="result"
              class:result--active={i === activeIndex}
              onclick={() => onSelect(i)}
            >
              <span class="oa-caption page">p.{hit.pageIndex + 1}</span>
              <span class="snippet"
                >{hit.contextBefore}<mark>{hit.contextMatch}</mark>{hit.contextAfter}</span
              >
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  {#if busy && hits.length > 0}
    <div class="oa-caption footer"><Icon name="loader-circle" size={12} spin={true} /> Searching…</div>
  {/if}
</aside>

<style>
  .capped {
    margin: 0 0 var(--space-2);
    color: var(--text-muted);
  }

  .result {
    display: block;
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: var(--transition-control);
  }

  .result:hover {
    background: var(--surface-hover);
  }

  .result--active {
    background: var(--surface-selected);
  }

  .page {
    display: block;
    margin-bottom: 2px;
  }

  .snippet {
    display: block;
    font: var(--type-caption);
    color: var(--text-muted);
    /* Three lines is enough for a clause to be recognisable; more turns
       the list into a wall of text nobody scans. */
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }

  mark {
    background: color-mix(in oklab, var(--yellow) 60%, transparent);
    color: var(--text-strong);
    border-radius: 2px;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-2);
    border-top: var(--border-width) solid var(--border-hairline);
    color: var(--text-muted);
  }
</style>
