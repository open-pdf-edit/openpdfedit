<script lang="ts">
  // Draw-once, reuse-everywhere signature capture, opened from
  // SignaturesPanel. Captures plain (x, y) strokes with the pointer —
  // the exact same shape the "Draw" tool already sends as an Ink
  // annotation (see PdfPage.svelte's inkStroke) — so saving one here and
  // placing it later both go through code this app has already tested,
  // not a new drawing engine.
  import Icon from "./Icon.svelte";

  interface Props {
    onSave: (name: string, strokes: [number, number][][], aspect: number) => void;
    onCancel: () => void;
  }

  let { onSave, onCancel }: Props = $props();

  const PAD_WIDTH = 480;
  const PAD_HEIGHT = 220;

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let strokes = $state<[number, number][][]>([]);
  let currentStroke: [number, number][] = [];
  let drawing = false;
  let name = $state("");

  const isEmpty = $derived(strokes.length === 0);

  function pointerPos(e: PointerEvent): [number, number] {
    const rect = canvasEl!.getBoundingClientRect();
    return [e.clientX - rect.left, e.clientY - rect.top];
  }

  function redraw() {
    const ctx = canvasEl?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, PAD_WIDTH, PAD_HEIGHT);
    // A faint baseline, like a signature line on paper — purely a
    // drawing guide, never part of the saved strokes.
    ctx.strokeStyle = "#d8d8d3";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(24, PAD_HEIGHT - 40);
    ctx.lineTo(PAD_WIDTH - 24, PAD_HEIGHT - 40);
    ctx.stroke();

    ctx.strokeStyle = "#020202";
    ctx.lineWidth = 2.5;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const stroke of [...strokes, currentStroke]) {
      if (stroke.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(stroke[0][0], stroke[0][1]);
      for (const [x, y] of stroke.slice(1)) ctx.lineTo(x, y);
      ctx.stroke();
    }
  }

  function onPointerDown(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drawing = true;
    currentStroke = [pointerPos(e)];
    redraw();
  }

  function onPointerMove(e: PointerEvent) {
    if (!drawing) return;
    currentStroke = [...currentStroke, pointerPos(e)];
    redraw();
  }

  function onPointerUp() {
    if (!drawing) return;
    drawing = false;
    if (currentStroke.length > 1) strokes = [...strokes, currentStroke];
    currentStroke = [];
    redraw();
  }

  function clear() {
    strokes = [];
    currentStroke = [];
    redraw();
  }

  // Crops to the ink's own bounding box (not the whole pad) and scales
  // it to 0..1, so a small tight signature doesn't place onto the page
  // surrounded by dead margin the way normalizing against the full
  // canvas would.
  function save() {
    if (isEmpty) return;
    const allPoints = strokes.flat();
    const xs = allPoints.map((p) => p[0]);
    const ys = allPoints.map((p) => p[1]);
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    const minY = Math.min(...ys);
    const maxY = Math.max(...ys);
    const width = Math.max(maxX - minX, 1);
    const height = Math.max(maxY - minY, 1);
    const normalized: [number, number][][] = strokes.map((stroke) =>
      stroke.map(([x, y]) => [(x - minX) / width, (y - minY) / height] as [number, number]),
    );
    onSave(name, normalized, width / height);
  }

  function onKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="oa-dialog-scrim" role="dialog" aria-modal="true" aria-label="Draw your signature" tabindex="-1" onkeydown={onKeydown}>
  <div class="oa-dialog pad-dialog">
    <div class="oa-dialog__header">
      <div class="oa-dialog__header-text">
        <h2 class="oa-dialog__title">Draw your signature</h2>
      </div>
      <button class="oa-icon-btn oa-icon-btn--sm" onclick={onCancel} aria-label="Close">
        <Icon name="x" size={15} />
      </button>
    </div>

    <div class="oa-dialog__body">
      <canvas
        bind:this={canvasEl}
        width={PAD_WIDTH}
        height={PAD_HEIGHT}
        class="pad"
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={onPointerUp}
      ></canvas>

      <div class="pad-row">
        <input class="oa-input" type="text" placeholder="Name this signature (optional)" bind:value={name} maxlength={60} />
        <button class="oa-btn oa-btn--secondary" onclick={clear} disabled={isEmpty}>
          <Icon name="eraser" size={15} />
          Clear
        </button>
      </div>
    </div>

    <div class="oa-dialog__footer">
      <button class="oa-btn oa-btn--secondary" onclick={onCancel}>Cancel</button>
      <button class="oa-btn oa-btn--primary" onclick={save} disabled={isEmpty}>Save signature</button>
    </div>
  </div>
</div>

<style>
  .pad-dialog {
    max-width: 540px;
  }

  .pad {
    display: block;
    width: 100%;
    height: 220px;
    background: var(--white);
    border: var(--border-width) solid var(--border-strong);
    border-radius: var(--radius-md);
    cursor: crosshair;
    touch-action: none;
  }

  .pad-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .pad-row .oa-input {
    flex: 1;
  }

  .pad-row .oa-btn {
    flex: 0 0 auto;
  }
</style>
