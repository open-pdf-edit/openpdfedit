<script lang="ts">
  import PdfPage, { type AnnotationPayload } from "./PdfPage.svelte";
  import type { Tool } from "./tools";

  interface PageSize {
    width: number;
    height: number;
  }

  interface Props {
    handle: number;
    pageSizes: PageSize[];
    zoom: number;
    activeTool: Tool;
    color: [number, number, number];
    /** A mutating backend call is in flight somewhere in +page.svelte
     * (`mutationBusy`) — forwarded straight through to every `PdfPage` so
     * its gesture overlay can stop accepting new gestures for the
     * duration. See `+page.svelte`'s `mutationBusy` doc comment for why. */
    busy: boolean;
    onCreateAnnotation: (pageIndex: number, payload: AnnotationPayload) => void;
    onRedact: (pageIndex: number, rect: [number, number, number, number]) => void;
    onToolClick: (pageIndex: number, x: number, y: number) => void;
    onCreateField: (pageIndex: number, rect: [number, number, number, number], kind: "text" | "checkbox") => void;
    onPlaceSignature: (pageIndex: number, rect: [number, number, number, number]) => void;
    onMoveObject: (pageIndex: number, x: number, y: number, dx: number, dy: number) => void;
  }

  let { handle, pageSizes, zoom, activeTool, color, busy, onCreateAnnotation, onRedact, onToolClick, onCreateField, onPlaceSignature, onMoveObject }: Props =
    $props();

  // 96 CSS px per inch, 72 PDF points per inch — the standard point-to-CSS-px
  // conversion at 100% zoom, matching how browsers render a "physical" inch.
  const BASE_PX_PER_PT = 96 / 72;
</script>

<div class="scroll-container">
  {#each pageSizes as size, i (i)}
    <PdfPage
      {handle}
      pageIndex={i}
      widthPt={size.width}
      heightPt={size.height}
      basePxPerPt={BASE_PX_PER_PT}
      {zoom}
      {activeTool}
      {color}
      {busy}
      {onCreateAnnotation}
      {onRedact}
      {onToolClick}
      {onCreateField}
      {onPlaceSignature}
      {onMoveObject}
    />
  {/each}
</div>

<style>
  .scroll-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 16px 0;
    background: var(--bg-sunken);
  }
</style>
