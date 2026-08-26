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
    /** Search matches falling on this page, painted as a translucent
     * overlay. Purely presentational — a search never touches the
     * document, so these are not annotations and vanish with the search. */
    searchMatches?: SearchMatchOverlay[];
    /** Where this page's current text selection is, as quads in
     * page-space points. Drawn, not interactive. */
    selectionQuads?: [number, number, number, number][];
    /** A drag with the Select tool: the rectangle dragged over, for the
     * parent to resolve into real characters. */
    onSelectText?: (pageIndex: number, rect: [number, number, number, number]) => void;
    /** Text fields on this page, rendered as real inputs positioned over
     * the page so they can be filled where they are. */
    formFields?: PageFormField[];
    /** Commits a field's new value. */
    onFillField?: (name: string, value: string) => void;
    /** A field to put the cursor in — set when one has just been drawn,
     * so it can be typed into without a second click. The `nonce` is
     * what makes re-focusing the same field work: a bare name wouldn't
     * change, so the effect wouldn't re-run. */
    focusField?: { name: string; nonce: number } | null;
    /** Bumped by the viewer when a gesture that started here has been
     * taken over by something bigger — a second finger arriving to
     * pinch. Whatever was being drawn is abandoned: the finger that
     * began it is now half of a zoom, and committing a highlight
     * because someone zoomed in on a word would be its own bug. */
    gestureCancel?: number;
  }

  /** One fillable text field, in PDF page-space points. */
  export interface PageFormField {
    name: string;
    value: string;
    rect: [number, number, number, number];
    readOnly: boolean;
  }

  /** One search hit's geometry, in PDF page-space points. `active` marks
   * the hit the result list is stepped to, drawn differently so it's
   * findable among the other matches on a busy page. */
  export interface SearchMatchOverlay {
    quads: [number, number, number, number][];
    active: boolean;
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
    searchMatches = [],
    selectionQuads = [],
    onSelectText,
    formFields = [],
    onFillField,
    focusField = null,
    gestureCancel = 0,
  }: Props = $props();

  /** The rendered inputs, keyed by field name, so a just-drawn field can
   * be focused without reaching into the DOM by selector. */
  const fieldInputs = new Map<string, HTMLInputElement>();

  // Focusing has to wait for the input to exist: the field is drawn, the
  // document reloads, and only then does this page render an input for
  // it. `focusField` changing is that signal — by the time the prop
  // arrives with a name this page owns, the `{#each}` above has already
  // run for the refreshed field list.
  $effect(() => {
    const request = focusField;
    if (!request) return;
    void request.nonce;
    const input = fieldInputs.get(request.name);
    if (!input) return;
    input.focus();
    input.select();
  });

  const MOVE_TOOLS: Tool[] = ["moveText", "moveImage"];

  /** Border thickness for a newly drawn shape, in PDF points. Thick
   * enough to read at a glance without dominating the page; the Rust
   * side accepts any width, so a properties panel can expose it later. */
  const SHAPE_LINE_WIDTH = 2;

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

  /** Tools that act where you press, with no gesture of their own. They
   * are the ones a finger has to be treated differently for: see
   * `pendingTap`. */
  const TAP_TOOLS: Tool[] = ["select", "erase", "note", "editText"];

  /** Tools whose drag selects text rather than drawing anything. Select
   * is both: a tap on an annotation still picks it, a drag over words
   * selects the words. Which one it was is only known on release, which
   * is why the tap is deferred for every pointer type here and not just
   * for touch. */
  const SELECTS_TEXT: Tool[] = ["select"];

  /** How far a finger may travel and still count as a tap rather than a
   * scroll, in CSS pixels. Roughly the wobble of a deliberate tap on a
   * phone; a scroll clears it within the first few pixels. */
  const TAP_SLOP_PX = 10;

  /** A finger resting on the page under a tap tool, not yet resolved.
   *
   * With a mouse, press *is* the click. With a finger it is ambiguous:
   * the same gesture starts every scroll of the document. Acting on
   * pointerdown would mean flicking through a document with the eraser
   * selected deletes whatever the flick started on. So a touch is held
   * until release and only counts if it stayed put — and the layer lets
   * the browser scroll in the meantime, rather than swallowing the
   * gesture with `touch-action: none`. */
  let pendingTap: { x: number; y: number; clientX: number; clientY: number } | null = null;

  async function tapAt(x: number, y: number) {
    if (activeTool === "select" || activeTool === "erase" || activeTool === "editText") {
      // Click-to-select-and-delete an existing annotation — no drag
      // gesture of its own, just resolved against annotations instead of
      // text runs/image placements (see +page.svelte's handleToolClick).
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
    }
  }

  async function onPointerDown(e: PointerEvent) {
    const [x, y] = cssToPdfPoint(e.clientX, e.clientY);

    if (TAP_TOOLS.includes(activeTool)) {
      if (SELECTS_TEXT.includes(activeTool)) {
        // Both possibilities at once: the start of a text drag, and a
        // tap that has not been ruled out yet.
        e.currentTarget && (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        pendingTap = { x, y, clientX: e.clientX, clientY: e.clientY };
        dragStart = { x, y };
        dragCurrent = { x, y };
        return;
      }
      if (e.pointerType === "touch") {
        // Deliberately no pointer capture: capturing here would take the
        // gesture away from the scroller, which is the thing the finger
        // is most likely to have meant.
        pendingTap = { x, y, clientX: e.clientX, clientY: e.clientY };
        return;
      }
      await tapAt(x, y);
      return;
    }

    e.currentTarget && (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);

    if (activeTool === "ink") {
      inkStroke = [[x, y]];
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

  function onPointerCancel() {
    // The browser took the gesture — it was a scroll, not a tap.
    pendingTap = null;
    abandonGesture();
  }

  /** Forget whatever was in progress, without committing it. */
  function abandonGesture() {
    dragStart = null;
    dragCurrent = null;
    inkStroke = [];
  }

  $effect(() => {
    // Read it so the effect subscribes; the value itself means nothing
    // beyond "it changed". Also runs once on mount, when there is
    // nothing in progress to abandon.
    void gestureCancel;
    abandonGesture();
  });

  function onPointerUp(e: PointerEvent) {
    const tap = pendingTap;
    if (tap) {
      pendingTap = null;
      const travelled = Math.hypot(e.clientX - tap.clientX, e.clientY - tap.clientY);
      const from = dragStart;
      const to = dragCurrent;
      dragStart = null;
      dragCurrent = null;
      if (travelled <= TAP_SLOP_PX) {
        void tapAt(tap.x, tap.y);
      } else if (SELECTS_TEXT.includes(activeTool) && from && to) {
        onSelectText?.(pageIndex, normalizedRect(from, to));
      }
      return;
    }

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
      const isAreaTool = activeTool === "redact" || activeTool === "addTextField" || activeTool === "addCheckbox" || activeTool === "signature" || activeTool === "rectangle" || activeTool === "ellipse";
      if (isAreaTool ? hasArea : traveled) {
        if (activeTool === "redact") {
          onRedact(pageIndex, rect);
        } else if (activeTool === "addTextField" || activeTool === "addCheckbox") {
          onCreateField(pageIndex, rect, activeTool === "addTextField" ? "text" : "checkbox");
        } else if (activeTool === "signature") {
          onPlaceSignature(pageIndex, rect);
        } else if (activeTool === "rectangle" || activeTool === "ellipse") {
          // The drag *is* the shape's bounds, so size is set by the
          // gesture — no separate size control needed.
          onCreateAnnotation(pageIndex, {
            rect,
            color,
            opacity: 1,
            annotation: {
              kind: activeTool === "rectangle" ? "square" : "circle",
              lineWidth: SHAPE_LINE_WIDTH,
            },
          });
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

<div
  class="page"
  bind:this={containerEl}
  data-page-index={pageIndex} style="width: {cssWidth}px; height: {cssHeight}px;">
  {#if isNearViewport}
    <canvas bind:this={canvasEl} style="width: 100%; height: 100%;"></canvas>
    <!-- Above the tile, below the interaction layer, and pointer-
         transparent, so a search overlay never intercepts a markup
         gesture aimed at the text underneath it. -->
    <!-- Real inputs sitting exactly over each field, so a form is
         filled where it is rather than in a side panel. They're above
         the tile but rendered before the interaction layer, and only
         accept pointer events while the select tool is active — a
         markup gesture must still pass through to the page. -->
    {#each formFields as field (field.name)}
      <input
        bind:this={
          () => fieldInputs.get(field.name) ?? null,
          (el) => {
            if (el) fieldInputs.set(field.name, el);
            else fieldInputs.delete(field.name);
          }
        }
        class="form-field"
        class:form-field--interactive={activeTool === "select" && !field.readOnly}
        value={field.value}
        readonly={field.readOnly}
        disabled={busy}
        tabindex={activeTool === "select" ? 0 : -1}
        aria-label={field.name}
        title={field.name}
        style="left: {field.rect[0] * pxPerPt}px; top: {(heightPt - field.rect[3]) * pxPerPt}px; width: {(field.rect[2] -
          field.rect[0]) * pxPerPt}px; height: {(field.rect[3] - field.rect[1]) * pxPerPt}px; font-size: {Math.max(
          9,
          Math.min(18, (field.rect[3] - field.rect[1]) * pxPerPt * 0.62),
        )}px;"
        onchange={(e) => onFillField?.(field.name, e.currentTarget.value)}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            e.currentTarget.blur();
          } else if (e.key === "Escape") {
            // Put the committed value back and get out of the way.
            e.currentTarget.value = field.value;
            e.currentTarget.blur();
          }
          // Editing text must not trigger the app's single-key
          // shortcuts, or typing a value starts changing tools.
          e.stopPropagation();
        }}
      />
    {/each}
    {#each selectionQuads as quad, q (q)}
      <div
        class="text-selection"
        style="left: {quad[0] * pxPerPt}px; top: {(heightPt - quad[3]) * pxPerPt}px; width: {(quad[2] - quad[0]) *
          pxPerPt}px; height: {(quad[3] - quad[1]) * pxPerPt}px;"
      ></div>
    {/each}
    {#each searchMatches as match, m (m)}
      {#each match.quads as quad, q (q)}
        <div
          class="search-hit"
          class:search-hit--active={match.active}
          style="left: {quad[0] * pxPerPt}px; top: {(heightPt - quad[3]) * pxPerPt}px; width: {(quad[2] - quad[0]) *
            pxPerPt}px; height: {(quad[3] - quad[1]) * pxPerPt}px;"
        ></div>
      {/each}
    {/each}
    {#if renderError}
      <div class="render-error" title={renderError}>⚠ page failed to render</div>
    {/if}
    <div
      class="interaction-layer"
      class:active={activeTool !== "select"}
      class:interaction-layer--tap={TAP_TOOLS.includes(activeTool)}
      class:interaction-layer--selects-text={SELECTS_TEXT.includes(activeTool)}
      class:busy
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerCancel}
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
          class:select-preview={activeTool === "select"}
          class:redact-preview={activeTool === "redact"}
          class:shape-preview={activeTool === "rectangle" || activeTool === "ellipse"}
          class:ellipse-preview={activeTool === "ellipse"}
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

  .form-field {
    position: absolute;
    margin: 0;
    padding: 0 3px;
    /* The page bitmap already shows this field's value: PDFium renders
       form data (FPDF_FFLDraw) as part of the page, so an input that
       also painted its value drew it twice — once at the PDF's own font
       size and once at this element's, which is what "the text appears
       twice, in two sizes" was. The input is the editor, not the
       display: it goes visible only while it has focus, where its opaque
       background covers the rendering underneath. */
    color: transparent;
    border: 1px solid color-mix(in oklab, var(--info-fg, #15b9eb) 55%, transparent);
    border-radius: 2px;
    background: color-mix(in oklab, var(--info-fg, #15b9eb) 8%, transparent);
    font-family: inherit;
    line-height: 1;
    /* Inert unless the select tool is active, so drawing a highlight
       over a form doesn't get swallowed by its fields. */
    pointer-events: none;
  }

  .form-field--interactive {
    pointer-events: auto;
  }

  .form-field:focus {
    outline: 2px solid var(--info-fg, #15b9eb);
    outline-offset: 0;
    background: var(--white, #fff);
    color: var(--black, #000);
  }

  /* The selection itself, in the colour every other application uses
     for one. Under the interaction layer, so it never eats a click. */
  .text-selection {
    position: absolute;
    background: color-mix(in oklab, var(--info-fg, #3b82f6) 30%, transparent);
    pointer-events: none;
  }

  .search-hit {
    position: absolute;
    background: color-mix(in oklab, var(--yellow, #f2e863) 55%, transparent);
    border-radius: 1px;
    pointer-events: none;
  }

  .search-hit--active {
    background: color-mix(in oklab, var(--orange, #ff8c42) 55%, transparent);
    outline: var(--border-width) solid var(--orange-600, #cc6a2f);
  }

  .interaction-layer {
    position: absolute;
    inset: 0;
    /* Drag tools own the gesture: a highlight drawn across a line must
       not scroll the document out from under itself. */
    touch-action: none;
  }

  /* Tap tools do not, so the page can be scrolled and pinched with a
     finger anywhere on it — which is most of the page. `onPointerUp`
     decides after the fact whether what happened was a tap. */
  .interaction-layer--tap {
    touch-action: auto;
  }

  .interaction-layer.active {
    cursor: crosshair;
  }

  /* Text can be dragged over here, and an I-beam is how every other
     application says so. */
  .interaction-layer--selects-text {
    cursor: text;
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

  /* A marquee, not a highlight. While a Select drag is under way there
     is nothing to show yet — the real selection only exists once the
     characters under the rectangle have been resolved — so this is a
     thin outline rather than the yellow block the markup tools draw. */
  .drag-preview.select-preview {
    background: color-mix(in oklab, var(--info-fg, #3b82f6) 12%, transparent);
    border-color: color-mix(in oklab, var(--info-fg, #3b82f6) 60%, transparent);
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

  .drag-preview.shape-preview {
    background: transparent;
    border: 2px solid color-mix(in oklab, var(--app-pdfedit, #e5484d) 80%, transparent);
  }

  /* Rounded to a full ellipse so the preview shows the shape being
     drawn, not the box it's inscribed in. */
  .drag-preview.ellipse-preview {
    border-radius: 50%;
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
