<script lang="ts">
  // Small fixed-width thumbnail for the Pages panel — same `tile://` raw-RGBA
  // fetch as PdfPage.svelte's full-size render, just without the
  // IntersectionObserver machinery (panel rows are few enough, and only
  // rendered while the panel itself is open, that eager-loading is fine;
  // see +page.svelte's refreshAnnotations comment for the same
  // not-a-hypothetical-yet reasoning applied there).
  import { backend } from "./backend";

  interface Props {
    handle: number;
    pageIndex: number;
    widthPt: number;
    heightPt: number;
  }

  let { handle, pageIndex, widthPt, heightPt }: Props = $props();

  const THUMB_WIDTH_PX = 64;

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let loadedForHandle = $state<number | null>(null);
  let renderError = $state<string | null>(null);

  const aspectRatio = $derived(widthPt / Math.max(heightPt, 1));

  $effect(() => {
    const currentHandle = handle;
    if (!canvasEl || loadedForHandle === currentHandle) return;

    const controller = new AbortController();
    const targetWidth = Math.round(THUMB_WIDTH_PX * (typeof devicePixelRatio !== "undefined" ? devicePixelRatio : 1));
    (async () => {
      const { width: tileWidth, height: tileHeight, rgba } = await backend.getPageBitmap(currentHandle, pageIndex, targetWidth, controller.signal);
      if (!canvasEl) return;

      canvasEl.width = tileWidth;
      canvasEl.height = tileHeight;
      canvasEl.getContext("2d")?.putImageData(new ImageData(rgba, tileWidth, tileHeight), 0, 0);
      renderError = null;
      loadedForHandle = currentHandle;
    })().catch((e) => {
      if (e?.name !== "AbortError") {
        renderError = `fetch threw: ${e?.message ?? e}`;
        console.error("thumbnail fetch failed", e);
      }
    });

    return () => controller.abort();
  });
</script>

<div class="thumb" style="width: {THUMB_WIDTH_PX}px; aspect-ratio: {aspectRatio};">
  <canvas bind:this={canvasEl}></canvas>
  {#if renderError}
    <div class="render-error" title={renderError}>⚠</div>
  {/if}
</div>

<style>
  .thumb {
    flex-shrink: 0;
    background: var(--white);
    border: var(--border-width) solid var(--border-hairline);
    box-shadow: var(--shadow-xs);
    overflow: hidden;
    position: relative;
  }

  .render-error {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--danger-bg);
    color: var(--danger-fg);
    font-size: var(--text-sm);
  }

  canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
