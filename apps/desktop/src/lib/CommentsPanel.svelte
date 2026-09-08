<script lang="ts">
  import Icon from "./Icon.svelte";

  interface AnnotationSummary {
    subtype: string;
    rect: [number, number, number, number];
    contents: string | null;
    pageIndex: number;
  }

  interface Props {
    annotations: AnnotationSummary[];
    loading: boolean;
    /** Dismiss the panel. Every rail panel can be opened by something
     * other than its toolbar toggle, so every one needs a way out that
     * does not require finding that toggle. */
    onClose: () => void;
  }

  let { annotations, loading, onClose }: Props = $props();
</script>

<aside class="oa-panel">
  <div class="oa-panel__header">
    <span class="oa-panel__title">Comments</span>
    <button class="oa-icon-btn oa-icon-btn--sm oa-panel__close" onclick={onClose} aria-label="Close panel">
      <Icon name="x" size={15} />
    </button>
  </div>
  <div class="oa-panel__body">
    {#if loading}
      <p class="oa-empty">Loading…</p>
    {:else if annotations.length === 0}
      <p class="oa-empty">No annotations yet. Pick a markup tool and drag on the page.</p>
    {:else}
      <ul class="oa-list">
        {#each annotations as a, i (i)}
          <li class="oa-list-item">
            <span class="subtype">{a.subtype}</span>
            <span class="oa-caption page">p.{a.pageIndex + 1}</span>
            {#if a.contents}<p class="contents">{a.contents}</p>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</aside>

<style>
  .subtype {
    font: var(--type-ui);
    color: var(--text-strong);
  }

  .page {
    margin-left: var(--space-2);
  }

  .contents {
    margin: var(--space-1) 0 0;
    font: var(--type-caption);
    color: var(--text-muted);
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
