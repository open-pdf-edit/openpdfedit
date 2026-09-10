<script lang="ts">
  // The OpenPdfEdit mark — ported from the OpenApps design system's
  // BrandMark component (components/core/BrandMark.jsx). Family rule:
  // every product name is "Open" + one word, set in Geist Medium at
  // -0.04em; the "Open" prefix is muted, the product word full strength,
  // and the trailing period carries the product's own accent — red for
  // OpenPdfEdit (--app-pdfedit). One implementation, reading the
  // --logo-* tokens, so re-pointing them re-skins every surface at once.
  import Icon from "./Icon.svelte";

  interface Props {
    variant?: "wordmark" | "monogram" | "lockup";
    size?: number;
  }

  let { variant = "wordmark", size = 20 }: Props = $props();

  const tileSize = $derived(variant === "lockup" ? size : size);
  const wordSize = $derived(variant === "lockup" ? size * 0.56 : size);
</script>

<span class="oa-brandmark" role="img" aria-label="OpenPdfEdit">
  {#if variant !== "wordmark"}
    <span class="tile" style="width: {tileSize}px; height: {tileSize}px;">
      <Icon name="file-pen" size={Math.round(tileSize * 0.55)} />
    </span>
  {/if}
  {#if variant !== "monogram"}
    <span class="word" style="font-size: {wordSize}px;">
      <span class="prefix">Open</span>PdfEdit<span class="dot">.</span>
    </span>
  {/if}
</span>

<style>
  .oa-brandmark {
    display: inline-flex;
    align-items: center;
    gap: 0.28em;
  }

  .tile {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    border-radius: var(--logo-tile-radius);
    background: var(--logo-tile-bg);
    color: var(--logo-tile-fg);
  }

  .word {
    font: var(--weight-medium) 1em/1 var(--font-display);
    letter-spacing: var(--logo-tracking);
    color: var(--logo-fg);
    white-space: nowrap;
  }

  .prefix {
    color: var(--text-muted);
  }

  .dot {
    color: var(--logo-dot);
  }
</style>
