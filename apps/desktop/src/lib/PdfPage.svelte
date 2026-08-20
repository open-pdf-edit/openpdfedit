<script lang="ts">
  // One page in the virtualized scroll list. Reserves correctly-
  // proportioned layout space immediately (from PDF-point page size, no
  // tile needed), and only fetches+paints its tile once an
  // IntersectionObserver says it's actually near the viewport. That's
  // what makes a 2,000-page document scroll instantly instead of
  // rendering (and holding in memory) every page up front.
  //
  // When a markup tool is active, a transparent overlay captures pointer
  // gestures and converts them from CSS pixels (screen space, y-down)
  // into PDF page-space points (origin bottom-left, y-up) before handing
  // them to the parent via `onCreateAnnotation` — this component never
  // calls the backend itself, because a successful annotation save
  // rotates the whole document's engine handle (see
  // apps/desktop/src-tauri/src/annotations.rs), which only the page that
  // owns `doc` state (`+page.svelte`) can react to correctly.

  import type { Tool } from "./tools";
  import { showPrompt } from "./dialog.svelte";
  import { backend } from "./backend";
  import type { AnnotationKindDto } from "./backend/types";

  interface Props {
    handle: number;
    pageIndex: number;
    widthPt: number;
    heightPt: number;
    /** CSS px per PDF point at 100% zoom, before the zoom multiplier. */
    basePxPerPt: number;
    zoom: number;
    activeTool: Tool;
    color: [number, number, number];
    /** A mutating backend call is in flight elsewhere (`+page.svelte`'s
     * `mutationBusy`) — while true, the interaction layer stops accepting
     * pointer gestures (`pointer-events: none` below), so a drag/click
     * started here can't dispatch a second mutating call that races
     * whatever's already in flight (including a self-race: starting a
     * second drag on *this* page before the first one's backend call has
     * even returned). Purely a CSS guard, not a JS one — `onPointerDown`/
     * `onPointerMove`/`onPointerUp` below are unchanged; the browser
     * simply never delivers those events to this element while busy. */
    busy: boolean;
    onCreateAnnotation: (pageIndex: number, payload: AnnotationPayload) => void;
    /** Same drag-rectangle gesture as highlight/underline/strikeout, but
     * for the "redact" tool — routed separately since redaction mutates
     * the document's content stream directly rather than creating an
     * annotation (see redact.rs's module doc). */
    onRedact: (pageIndex: number, rect: [number, number, number, number]) => void;
    /** Fired on a plain click (not a drag) for "editText"/"moveImage" —
     * both need to find *which existing thing* was clicked, which only
     * the parent (`+page.svelte`, which owns the current document/tool
     * state and talks to the backend) can resolve. `x`/`y` are PDF
     * page-space points. */
    onToolClick: (pageIndex: number, x: number, y: number) => void;
    /** Same drag-rectangle gesture again, for "addTextField"/"addCheckbox" —
     * places a new AcroForm field's `/Rect` (see field_create.rs's module
     * doc for why this is a separate write path from annotations/forms
     * fill). */
    onCreateField: (pageIndex: number, rect: [number, number, number, number], kind: "text" | "checkbox") => void;
    /** Same drag-rectangle gesture again, for "signature": `rect` is
     * where the currently-armed saved signature should be stamped (see
     * SignaturesPanel.svelte). */
    onPlaceSignature: (pageIndex: number, rect: [number, number, number, number]) => void;
    /** Drag-to-move for "moveText"/"moveImage": `x`/`y` are where the
     * drag started (which identifies *what* is being moved) and `dx`/`dy`
     * are how far it travelled, all in PDF page-space points. Direct
     * manipulation replaced typing a "dx,dy" offset into a prompt —
     * quite apart from being easier, the prompt gave no feedback about
     * which direction was which. */
    onMoveObject: (pageIndex: number, x: number, y: number, dx: number, dy: number) => void;
  }

  export interface AnnotationPayload {
    rect: [number, number, number, number];
    color: [number, number, number];
    opacity: number;
    contents?: string;
    annotation: AnnotationKindDto;
  }

  let {
    handle,
    pageIndex,
    widthPt,
    heightPt,
    basePxPerPt,
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
  }: Props = $props();

  const MOVE_TOOLS: Tool[] = ["moveText", "moveImage"];

  let containerEl = $state<HTMLDivElement | null>(null);
  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let isNearViewport = $state(false);
  let loadedFor = $state<{ handle: number; width: number } | null>(null);
  let renderError = $state<string | null>(null);

  const cssWidth = $derived(widthPt * basePxPerPt * zoom);
  const cssHeight = $derived(heightPt * basePxPerPt * zoom);
  const pxPerPt = $derived(basePxPerPt * zoom);

  // Render at device-pixel resolution so text stays crisp on HiDPI
  // displays; rounded because the tile:// path parses this as an integer.
  const targetWidthPx = $derived(Math.round(cssWidth * (typeof devicePixelRatio !== "undefined" ? devicePixelRatio : 1)));

  $effect(() => {
    if (!containerEl) return;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          isNearViewport = entry.isIntersecting;
          // The `{#if isNearViewport}` block below destroys `<canvas>`
          // once a page scrolls far enough away (that's the whole point
          // — a 2,000-page doc can't hold every page's pixels in memory
          // at once). `loadedFor` must not survive that: without this
          // reset, scrolling a page back into view creates a brand-new,
          // blank canvas, but the paint effect below sees loadedFor
          // still matching this handle/width and skips repainting it —
          // exactly the "pages go white again after scrolling" bug.
          if (!entry.isIntersecting) {
            loadedFor = null;
          }
        }
      },
      // Preload roughly one screen-height above/below so pages are
      // already painted by the time they scroll into view.
      { rootMargin: "800px 0px" },
    );
    observer.observe(containerEl);
    return () => observer.disconnect();
  });

  $effect(() => {
    const width = targetWidthPx;
    const currentHandle = handle;
    if (!isNearViewport || !canvasEl || width <= 0) return;
    if (loadedFor && loadedFor.handle === currentHandle && loadedFor.width === width) return;

    const controller = new AbortController();
    (async () => {
      const { width: tileWidth, height: tileHeight, rgba } = await backend.getPageBitmap(currentHandle, pageIndex, width, controller.signal);
      if (!canvasEl) return;

      canvasEl.width = tileWidth;
      canvasEl.height = tileHeight;
      const ctx = canvasEl.getContext("2d");
      ctx?.putImageData(new ImageData(rgba, tileWidth, tileHeight), 0, 0);
      renderError = null;
      loadedFor = { handle: currentHandle, width };
    })().catch((e) => {
      if (e?.name !== "AbortError") {
        renderError = `tile fetch threw: ${e?.message ?? e}`;
        console.error("tile fetch failed", e);
      }
    });

    return () => controller.abort();
  });

  // --- Pointer-gesture capture for markup tools ---

  let dragStart = $state<{ x: number; y: number } | null>(null);
  let dragCurrent = $state<{ x: number; y: number } | null>(null);
  let inkStroke = $state<[number, number][]>([]);

  function cssToPdfPoint(clientX: number, clientY: number): [number, number] {
    const box = containerEl!.getBoundingClientRect();
    const cssX = clientX - box.left;
    const cssY = clientY - box.top;
    return [cssX / pxPerPt, heightPt - cssY / pxPerPt];
  }

  function normalizedRect(a: { x: number; y: number }, b: { x: number; y: number }): [number, number, number, number] {
    return [Math.min(a.x, b.x), Math.min(a.y, b.y), Math.max(a.x, b.x), Math.max(a.y, b.y)];
  }

  async function onPointerDown(e: PointerEvent) {
    e.currentTarget && (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    const [x, y] = cssToPdfPoint(e.clientX, e.clientY);

    if (activeTool === "select" || activeTool === "erase") {
      // Click-to-select-and-delete an existing annotation — no drag
      // gesture of its own, same click-based shape as editText/moveImage
      // below, just resolved against annotations instead of text
      // runs/image placements (see +page.svelte's handleToolClick).
      onToolClick(pageIndex, x, y);
      return;
    }

    if (activeTool === "note") {
      const text = await showPrompt("Note text:", { title: "Add note", confirmLabel: "Add" });
      if (text && text.trim().length > 0) {
        const rect: [number, number, number, number] = [x, y - 60, x + 220, y];
        onCreateAnnotation(pageIndex, {
          rect,
          color,
          opacity: 1,
          annotation: { kind: "freeText", text: text.trim(), fontSize: 12 },
        });
      }
      return;
    }

    if (activeTool === "ink") {
      inkStroke = [[x, y]];
      return;
    }

    if (activeTool === "editText") {
      onToolClick(pageIndex, x, y);
      return;
    }

    // moveText / moveImage: press on the thing, drag, release. Handled
    // by the same dragStart/dragCurrent pair as the rectangle tools, but
    // interpreted as an offset rather than an area (see onPointerUp).
    dragStart = { x, y };
    dragCurrent = { x, y };
  }

  function onPointerMove(e: PointerEvent) {
    if (activeTool === "ink" && inkStroke.length > 0) {
      const [x, y] = cssToPdfPoint(e.clientX, e.clientY);
      inkStroke = [...inkStroke, [x, y]];
      return;
    }
    if (dragStart) {
      const [x, y] = cssToPdfPoint(e.clientX, e.clientY);
      // Shift constrains a move to a single axis — snap whichever axis
      // has travelled *less* back to the start point, re-evaluated on
      // every move so the locked axis tracks wherever the cursor has
      // gone furthest, the same convention design tools use. Recomputed
      // fresh each event rather than fixed at drag-start, since which
      // axis the user "meant" usually isn't obvious from the first
      // couple of pixels of travel.
      if (e.shiftKey && MOVE_TOOLS.includes(activeTool)) {
        const dx = x - dragStart.x;
        const dy = y - dragStart.y;
        dragCurrent = Math.abs(dx) >= Math.abs(dy) ? { x, y: dragStart.y } : { x: dragStart.x, y };
      } else {
        dragCurrent = { x, y };
      }
    }
  }

  function onPointerUp() {
    if (activeTool === "ink" && inkStroke.length > 1) {
      const xs = inkStroke.map((p) => p[0]);
      const ys = inkStroke.map((p) => p[1]);
      const rect: [number, number, number, number] = [Math.min(...xs) - 2, Math.min(...ys) - 2, Math.max(...xs) + 2, Math.max(...ys) + 2];
      onCreateAnnotation(pageIndex, { rect, color, opacity: 1, annotation: { kind: "ink", strokes: [inkStroke] } });
      inkStroke = [];
      return;
    }

    if (dragStart && dragCurrent && MOVE_TOOLS.includes(activeTool)) {
      const dx = dragCurrent.x - dragStart.x;
      const dy = dragCurrent.y - dragStart.y;
      // A press with no travel is a mis-click, not a request to move
      // something by zero — reporting it would make an undo entry for a
      // no-op edit.
      if (Math.hypot(dx, dy) > 1) {
        onMoveObject(pageIndex, dragStart.x, dragStart.y, dx, dy);
      }
      dragStart = null;
      dragCurrent = null;
      return;
    }

    if (dragStart && dragCurrent) {
      const rect = normalizedRect(dragStart, dragCurrent);
      const width = rect[2] - rect[0];
      const height = rect[3] - rect[1];
      // Ignore accidental clicks-without-drag, but measure "did the user
      // actually drag" by total travel, NOT by requiring meaningful
      // extent on *both* axes. Dragging horizontally across a line of
      // text — the natural way to select text, and the whole point of
      // the highlight/underline/strikeout tools — moves almost zero
      // vertical distance, so a `width > 2 && height > 2` test silently
      // threw the entire gesture away and nothing happened at all: no
      // annotation, no error, no feedback. That was the real cause of
      // "I can't select text."
      //
      // The area-defining tools (redact, addTextField, addCheckbox,
      // signature) do genuinely need a rectangle with both dimensions,
      // so they keep the stricter check; a zero-height redaction or
      // form field (or a signature squashed to a line) would be
      // meaningless.
      const traveled = Math.hypot(width, height) > 3;
      const hasArea = width > 2 && height > 2;
      const isAreaTool = activeTool === "redact" || activeTool === "addTextField" || activeTool === "addCheckbox" || activeTool === "signature";
      if (isAreaTool ? hasArea : traveled) {
        if (activeTool === "redact") {
          onRedact(pageIndex, rect);
        } else if (activeTool === "addTextField" || activeTool === "addCheckbox") {
          onCreateField(pageIndex, rect, activeTool === "addTextField" ? "text" : "checkbox");
        } else if (activeTool === "signature") {
          onPlaceSignature(pageIndex, rect);
        } else {
          const kind = activeTool === "highlight" ? "highlight" : activeTool === "underline" ? "underline" : "strikeOut";
          onCreateAnnotation(pageIndex, {
            rect,
            color,
            opacity: activeTool === "highlight" ? 0.4 : 1,
            annotation: { kind, quads: [rect] },
          });
        }
      }
    }
    dragStart = null;
    dragCurrent = null;
  }

  const dragRectStyle = $derived.by(() => {
    if (!dragStart || !dragCurrent) return "";
    const [x0, y0, x1, y1] = normalizedRect(dragStart, dragCurrent);
    // Flip y back to CSS (top-down) space for the preview box.
    const top = (heightPt - y1) * pxPerPt;
    const left = x0 * pxPerPt;
    const width = (x1 - x0) * pxPerPt;
    const height = (y1 - y0) * pxPerPt;
    return `left:${left}px; top:${top}px; width:${width}px; height:${height}px;`;
  });

  // Underline/strikeout render as a *thin line* in the saved annotation
  // (see openpdfedit-annot's build_appearance — a stroked `m`/`l`/`S`, not
  // a filled box), but the drag gesture used to place them is the same
  // freehand rectangle as highlight/redact. Without this, the live
  // preview shows the same translucent box for every tool, which reads
  // as "this creates a highlight" regardless of which tool is active —
  // reported as underline/strikeout "feeling like a highlight box." This
  // mirrors build_appearance's own line-position math (underline: a hair
  // above the bottom edge; strikeout: the vertical midline) so the
  // preview honestly shows where the line will land before you release.
  const dragLineStyle = $derived.by(() => {
    if (!dragStart || !dragCurrent) return "";
    const [x0, y0, x1, y1] = normalizedRect(dragStart, dragCurrent);
    const top = (heightPt - y1) * pxPerPt;
    const left = x0 * pxPerPt;
    const width = (x1 - x0) * pxPerPt;
    const height = (y1 - y0) * pxPerPt;
    const lineTop = activeTool === "strikeOut" ? top + height * 0.5 : top + height * 0.92;
    return `left:${left}px; top:${lineTop}px; width:${width}px;`;
  });

  // Live feedback for the move tools: a line from where the drag started
  // to the cursor, plus the offset in PDF points. Without it there is no
  // way to tell how far something is about to move until after it moves.
  const moveIndicator = $derived.by(() => {
    if (!dragStart || !dragCurrent || !MOVE_TOOLS.includes(activeTool)) return null;
    const x0 = dragStart.x * pxPerPt;
    const y0 = (heightPt - dragStart.y) * pxPerPt;
    const x1 = dragCurrent.x * pxPerPt;
    const y1 = (heightPt - dragCurrent.y) * pxPerPt;
    const dx = dragCurrent.x - dragStart.x;
    const dy = dragCurrent.y - dragStart.y;
    const length = Math.hypot(x1 - x0, y1 - y0);
    const angle = (Math.atan2(y1 - y0, x1 - x0) * 180) / Math.PI;
    return {
      lineStyle: `left:${x0}px; top:${y0}px; width:${length}px; transform: rotate(${angle}deg);`,
      labelStyle: `left:${x1 + 10}px; top:${y1 + 10}px;`,
      label: `${dx >= 0 ? "→" : "←"} ${Math.abs(dx).toFixed(0)}  ${dy >= 0 ? "↑" : "↓"} ${Math.abs(dy).toFixed(0)} pt`,
    };
  });
</script>

<div class="page" bind:this={containerEl} style="width: {cssWidth}px; height: {cssHeight}px;">
  {#if isNearViewport}
    <canvas bind:this={canvasEl} style="width: 100%; height: 100%;"></canvas>
    {#if renderError}
      <div class="render-error" title={renderError}>⚠ page failed to render</div>
    {/if}
    <div
      class="interaction-layer"
      class:active={activeTool !== "select"}
      class:busy
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      role="presentation"
    ></div>
    {#if moveIndicator}
      <div class="move-arrow" style={moveIndicator.lineStyle}></div>
      <div class="move-label" style={moveIndicator.labelStyle}>{moveIndicator.label}</div>
    {:else if dragStart && dragCurrent}
      {#if activeTool === "underline" || activeTool === "strikeOut"}
        <div class="drag-preview-line" style={dragLineStyle}></div>
      {:else}
        <div
          class="drag-preview"
          class:redact-preview={activeTool === "redact"}
          class:signature-preview={activeTool === "signature"}
          style={dragRectStyle}
        ></div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .page {
    background: var(--white);
    box-shadow: var(--shadow-md);
    margin: 0 auto 16px;
    position: relative;
  }

  canvas {
    display: block;
  }

  .render-error {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: repeating-linear-gradient(
      45deg,
      var(--danger-bg),
      var(--danger-bg) 10px,
      color-mix(in oklab, var(--danger-bg) 60%, var(--danger-fg)) 10px,
      color-mix(in oklab, var(--danger-bg) 60%, var(--danger-fg)) 20px
    );
    color: var(--danger-fg);
    font: var(--type-caption);
    font-size: var(--text-sm);
    text-align: center;
    padding: var(--space-2);
    pointer-events: none;
  }

  .drag-preview-line {
    position: absolute;
    height: 2px;
    background: color-mix(in oklab, var(--red-500) 85%, transparent);
    pointer-events: none;
  }

  .interaction-layer {
    position: absolute;
    inset: 0;
    touch-action: none;
  }

  .interaction-layer.active {
    cursor: crosshair;
  }

  /* A mutating backend call is in flight — see `busy` prop's doc above.
     `pointer-events: none` makes the browser skip this element entirely
     during hit-testing, so pointerdown/move/up never fire here at all
     (not merely ignored inside the handler) for as long as it's set. */
  .interaction-layer.busy {
    pointer-events: none;
  }

  .drag-preview {
    position: absolute;
    background: color-mix(in oklab, var(--yellow) 35%, transparent);
    border: var(--border-width) solid color-mix(in oklab, var(--yellow) 80%, var(--gray-800));
    pointer-events: none;
  }

  .move-arrow {
    position: absolute;
    height: 2px;
    background: var(--info-fg);
    transform-origin: 0 50%;
    pointer-events: none;
  }

  .move-label {
    position: absolute;
    background: var(--surface-inverse);
    color: var(--text-inverse);
    font: var(--type-caption);
    padding: 2px 6px;
    border-radius: var(--radius-xs);
    white-space: nowrap;
    pointer-events: none;
  }

  .drag-preview.redact-preview {
    background: var(--black-60);
    border: var(--border-width) solid var(--black-90);
  }

  .drag-preview.signature-preview {
    background: color-mix(in oklab, var(--app-pdfedit) 12%, transparent);
    border: var(--border-width) dashed var(--app-pdfedit);
  }
</style>
