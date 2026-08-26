<script lang="ts">
  import PdfPage, {
    type AnnotationPayload,
    type PageFormField,
    type SearchMatchOverlay,
  } from "./PdfPage.svelte";
  import type { FormFieldDto, SearchHitDto } from "./backend";
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
    /** Every hit of the current search, in document order. */
    searchHits?: SearchHitDto[];
    /** Index into `searchHits` of the hit the user stepped to, or -1. */
    activeHitIndex?: number;
    /** The current text selection, if it is on one of these pages. */
    selection?: { pageIndex: number; quads: [number, number, number, number][] } | null;
    onSelectText?: (pageIndex: number, rect: [number, number, number, number]) => void;
    /** A request to scroll to a page. The `nonce` is what makes clicking
     * the same bookmark twice scroll again — a bare page number wouldn't
     * change, so the effect wouldn't re-run. */
    scrollToPage?: { pageIndex: number; nonce: number } | null;
    /** Fired when the page at the top of the viewport changes. */
    onCurrentPageChange?: (pageIndex: number) => void;
    /** Every fillable field in the document; grouped per page here. */
    formFields?: FormFieldDto[];
    onFillField?: (name: string, value: string) => void;
    /** A field to put the cursor in, once it exists — see
     * `PdfPage`'s own prop. Passed to every page; only the one that
     * renders a field by that name acts on it. */
    focusField?: { name: string; nonce: number } | null;
    /** A pinch asked for this zoom. Unclamped — the parent owns the
     * limits, and applying them here would mean two places to change. */
    onZoom?: (zoom: number) => void;
  }

  let {
    handle,
    pageSizes,
    zoom,
    activeTool,
    color,
    busy,
    onCreateAnnotation,
    onRedact,
    onToolClick,
    onCreateField,
    onPlaceSignature,
    onMoveObject,
    searchHits = [],
    activeHitIndex = -1,
    selection = null,
    onSelectText,
    scrollToPage = null,
    onCurrentPageChange,
    formFields = [],
    onFillField,
    focusField = null,
    onZoom,
  }: Props = $props();

  // 96 CSS px per inch, 72 PDF points per inch — the standard point-to-CSS-px
  // conversion at 100% zoom, matching how browsers render a "physical" inch.
  const BASE_PX_PER_PT = 96 / 72;

  /** How far down the viewport a jumped-to match is parked. A third from
   * the top keeps the lines *above* the match visible too, which makes a
   * hit readable in context instead of pinned to the top edge with its
   * lead-in scrolled off. */
  const HIT_VIEWPORT_FRACTION = 1 / 3;

  let scrollEl = $state<HTMLDivElement | null>(null);

  // --- Pinch to zoom ---
  //
  // Not the browser's own. A browser pinch scales the visual viewport,
  // which magnifies the rendered bitmap — a page zoomed to 300% that way
  // is a 100% page enlarged, blurred, with the rest of the interface
  // enlarged along with it. Pinching here changes the app's zoom, so
  // pages are re-rendered at the new size and stay sharp, and the topbar
  // stays where it was. `.scroll-container` sets `touch-action: pan-y`
  // to reserve the gesture: panning stays with the browser, pinching
  // does not.

  /** Every finger currently down, by pointer id. */
  const touches = new Map<number, { x: number; y: number }>();

  /** What the pinch started from, so the zoom follows the *total* spread
   * of the fingers rather than accumulating per-frame ratios, which
   * drift. `midpoint` is relative to the container's top; `scrollTop` is
   * where it sat then. Together they anchor the point between the
   * fingers, so the document appears to zoom around it. */
  let pinch: { distance: number; zoom: number; midpoint: number; scrollTop: number } | null = null;

  /** Bumped when a pinch begins, to tell the page under the first finger
   * that its gesture has been taken over. */
  let gestureCancel = $state(0);

  /** Where the zoom effect should scroll to once the pages have been
   * laid out at their new size. */
  let pendingScrollTop: number | null = null;

  function spread(): { distance: number; midpoint: number } {
    const [a, b] = [...touches.values()];
    return {
      distance: Math.hypot(a.x - b.x, a.y - b.y),
      midpoint: (a.y + b.y) / 2,
    };
  }

  function onPointerDown(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (touches.size !== 2 || !scrollEl) return;

    const { distance, midpoint } = spread();
    const top = scrollEl.getBoundingClientRect().top;
    pinch = { distance, zoom, midpoint: midpoint - top, scrollTop: scrollEl.scrollTop };
    // The first finger may have started drawing something. It is now
    // half of a zoom.
    gestureCancel += 1;
  }

  function onPointerMove(e: PointerEvent) {
    if (e.pointerType !== "touch" || !touches.has(e.pointerId)) return;
    touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (!pinch || touches.size !== 2 || !scrollEl) return;

    const { distance } = spread();
    // A pinch that starts as a tap can report a distance of nearly zero
    // for a frame, which would send the zoom to infinity.
    if (pinch.distance < 1) return;

    const next = pinch.zoom * (distance / pinch.distance);
    // Keep the point between the fingers where it is: everything above
    // it grows by the same factor, so the scroll offset must too.
    pendingScrollTop = (pinch.scrollTop + pinch.midpoint) * (next / pinch.zoom) - pinch.midpoint;
    onZoom?.(next);
  }

  function endTouch(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    touches.delete(e.pointerId);
    // A pinch is over as soon as it stops being two fingers. Lifting one
    // of three does not resume anything: the geometry it started from is
    // gone.
    if (touches.size < 2) {
      pinch = null;
      // A pinch against the zoom limit changes nothing, so the effect
      // that consumes this never runs. Left set, it would jump the
      // document the next time the zoom buttons were used.
      pendingScrollTop = null;
    }
  }

  $effect(() => {
    // Runs after the pages have been re-laid out at the new zoom, which
    // is the only moment `scrollTop` can be set without being clamped to
    // the old, shorter document.
    void zoom;
    const target = pendingScrollTop;
    if (target === null || !scrollEl) return;
    pendingScrollTop = null;
    scrollEl.scrollTop = Math.max(0, target);
  });

  // Grouped once per search rather than filtered inside every page: a
  // 500-hit result set across a long document would otherwise cost a full
  // scan of the hit list per page, per render.
  const hitsByPage = $derived.by(() => {
    const byPage = new Map<number, SearchMatchOverlay[]>();
    searchHits.forEach((hit, index) => {
      const overlay: SearchMatchOverlay = { quads: hit.quads, active: index === activeHitIndex };
      const existing = byPage.get(hit.pageIndex);
      if (existing) existing.push(overlay);
      else byPage.set(hit.pageIndex, [overlay]);
    });
    return byPage;
  });

  // Grouped once, for the same reason search hits are: a per-page scan
  // of the whole field list on every render is wasted work on a long
  // form.
  //
  // Only text fields get an on-page input. A checkbox needs a control
  // that toggles rather than a box you type into, and radio groups need
  // to coordinate across widgets — both are still handled in the panel
  // rather than half-done here.
  const fieldsByPage = $derived.by(() => {
    const byPage = new Map<number, PageFormField[]>();
    for (const field of formFields) {
      if (field.kind !== "text") continue;
      const entry: PageFormField = {
        name: field.name,
        value: field.value ?? "",
        rect: field.rect,
        readOnly: field.isReadOnly,
      };
      const existing = byPage.get(field.pageIndex);
      if (existing) existing.push(entry);
      else byPage.set(field.pageIndex, [entry]);
    }
    return byPage;
  });

  function pageElement(pageIndex: number): HTMLElement | null {
    return scrollEl?.querySelector<HTMLElement>(`[data-page-index="${pageIndex}"]`) ?? null;
  }

  // Scrolling lives here rather than in the parent because this component
  // owns the scroll container; the parent only knows which hit is active.
  // Measured against live rects rather than `offsetTop` so it stays
  // correct regardless of which ancestor is the offset parent.
  $effect(() => {
    const hit = activeHitIndex >= 0 ? searchHits[activeHitIndex] : undefined;
    const container = scrollEl;
    if (!hit || !container) return;
    const pageEl = pageElement(hit.pageIndex);
    const pageSize = pageSizes[hit.pageIndex];
    if (!pageEl || !pageSize) return;

    // Quads are `[x0, y0, x1, y1]` with a bottom-left origin, so the top
    // of the whole match is the largest y1 among them.
    const topPt = Math.max(...hit.quads.map((quad) => quad[3]));
    const offsetWithinPage = (pageSize.height - topPt) * BASE_PX_PER_PT * zoom;
    const pageOffset = pageEl.getBoundingClientRect().top - container.getBoundingClientRect().top;
    const target =
      container.scrollTop + pageOffset + offsetWithinPage - container.clientHeight * HIT_VIEWPORT_FRACTION;
    container.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
  });

  $effect(() => {
    const request = scrollToPage;
    const container = scrollEl;
    if (!request || !container) return;
    // Read the nonce so re-selecting the same page still re-runs this.
    void request.nonce;
    const pageEl = pageElement(request.pageIndex);
    if (!pageEl) return;
    const offset = pageEl.getBoundingClientRect().top - container.getBoundingClientRect().top;
    container.scrollTo({ top: Math.max(0, container.scrollTop + offset - 8), behavior: "smooth" });
  });

  /** Which page sits at the top of the viewport, reported on scroll so a
   * contents panel can highlight the section being read. Throttled to one
   * measurement per frame: scroll fires far more often than the answer
   * can change, and each measurement reads layout. */
  let currentPage = -1;
  let measurementQueued = false;

  function measureCurrentPage() {
    measurementQueued = false;
    const container = scrollEl;
    if (!container || !onCurrentPageChange) return;
    const containerTop = container.getBoundingClientRect().top;
    let best = 0;
    for (const pageEl of container.querySelectorAll<HTMLElement>("[data-page-index]")) {
      // The topmost page whose bottom edge is still below the fold.
      if (pageEl.getBoundingClientRect().bottom > containerTop + 1) {
        best = Number(pageEl.dataset.pageIndex ?? 0);
        break;
      }
    }
    if (best !== currentPage) {
      currentPage = best;
      onCurrentPageChange(best);
    }
  }

  function onScroll() {
    if (measurementQueued) return;
    measurementQueued = true;
    requestAnimationFrame(measureCurrentPage);
  }
</script>

<div
  class="scroll-container"
  bind:this={scrollEl}
  onscroll={onScroll}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={endTouch}
  onpointercancel={endTouch}
  role="presentation"
>
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
      searchMatches={hitsByPage.get(i) ?? []}
      selectionQuads={selection?.pageIndex === i ? selection.quads : []}
      {onSelectText}
      formFields={fieldsByPage.get(i) ?? []}
      {onFillField}
      {focusField}
      {gestureCancel}
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
    /* Panning stays with the browser; pinching is handled above, and
       would otherwise scale the whole interface instead of the page.
       Set here rather than on the pages because touch-action is
       intersected up the ancestor chain — the pages can only give away
       more, never take this back. */
    touch-action: pan-y;
  }
</style>
