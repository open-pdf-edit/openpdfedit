<script lang="ts">
  // Watermark dialog — a replication of OpenCapture's watermark tool for
  // PDF pages: tiled text/logo cells across a band of each page, 0° or
  // 45°, with a live preview. The preview here draws with the same cell
  // math the Rust side bakes into the page content streams
  // (openpdfedit-watermark ports the tiling formulas; keep the two in
  // lockstep when touching either).
  //
  // Like the original, this panel has no drawing gesture: it's opened
  // from the topbar (a document-level tool, like OCR/Compare), configured
  // here, and applied to every page in one command. The logo file is
  // decoded to raw RGBA via a canvas — image-format parsing never
  // reaches the Rust side — and capped to 512px on its longest side to
  // bound the payload that crosses the IPC/wasm boundary.
  import Icon from "./Icon.svelte";
  import type { WatermarkChoices } from "./backend/types";

  interface Props {
    open: boolean;
    busy: boolean;
    onApply: (choices: WatermarkChoices) => void;
    onClose: () => void;
  }

  let { open, busy, onApply, onClose }: Props = $props();

  let text = $state("CONFIDENTIAL");
  let location = $state<"top" | "bottom" | "top-bottom" | "full">("full");
  let orientationDeg = $state<0 | 45>(45);
  let opacity = $state(0.4);
  let textScale = $state(1);
  let logoName = $state("");
  let logoBitmap: ImageBitmap | null = $state(null);
  let logoRgbaBase64 = "";
  let logoWidth = 0;
  let logoHeight = 0;
  let previewEl: HTMLCanvasElement | undefined = $state();
  let fileEl: HTMLInputElement | undefined = $state();

  const canApply = $derived(text.trim().length > 0 || logoBitmap !== null);

  // --- the OpenCapture cell math, for the preview only (the applied
  // watermark re-derives the same numbers in Rust, in PDF points) ---

  function cellSize(basisWidth: number): { width: number; height: number } {
    const width = Math.max(40, Math.round(basisWidth * 0.16));
    const height = Math.max(24, Math.round(width * 0.5));
    return { width, height };
  }

  function drawCell(
    ctx: CanvasRenderingContext2D,
    rect: { x: number; y: number; width: number; height: number },
  ): void {
    ctx.save();
    ctx.globalAlpha = opacity;
    const trimmed = text.trim();
    const logoAreaHeight = logoBitmap && trimmed ? rect.height * 0.6 : rect.height;
    let textTop = rect.y;
    if (logoBitmap) {
      const scale = Math.min(rect.width / logoBitmap.width, logoAreaHeight / logoBitmap.height);
      const w = logoBitmap.width * scale;
      const h = logoBitmap.height * scale;
      ctx.drawImage(logoBitmap, rect.x + (rect.width - w) / 2, rect.y + (logoAreaHeight - h) / 2, w, h);
      textTop = rect.y + logoAreaHeight;
    }
    if (trimmed) {
      const textAreaHeight = rect.y + rect.height - textTop;
      const fontSize =
        Math.max(10, Math.min(textAreaHeight * 0.7, (rect.width / Math.max(1, trimmed.length)) * 1.7)) * textScale;
      ctx.font = `${fontSize}px system-ui, sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.lineWidth = Math.max(2, fontSize / 8);
      ctx.strokeStyle = "#000000";
      ctx.fillStyle = "#ffffff";
      const tx = rect.x + rect.width / 2;
      const ty = textTop + textAreaHeight / 2;
      ctx.strokeText(trimmed, tx, ty);
      ctx.fillText(trimmed, tx, ty);
    }
    ctx.restore();
  }

  function drawPreview(): void {
    if (!previewEl) return;
    const ctx = previewEl.getContext("2d");
    if (!ctx) return;
    const W = previewEl.width;
    const H = previewEl.height;

    // A neutral mock "page" so white-with-black-stroke cells read against
    // something — light paper with a few grey text bars.
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = "#fdfdfc";
    ctx.fillRect(0, 0, W, H);
    ctx.fillStyle = "#deded9";
    for (let i = 0; i < 5; i++) ctx.fillRect(24, 22 + i * 26, W - 48 - (i % 3) * 30, 9);

    const cell = cellSize(W * 0.5);
    const bands =
      location === "full"
        ? [{ x: 0, y: 0, width: W, height: H }]
        : location === "top"
          ? [{ x: 0, y: 0, width: W, height: Math.min(cell.height, H) }]
          : location === "bottom"
            ? [{ x: 0, y: H - Math.min(cell.height, H), width: W, height: Math.min(cell.height, H) }]
            : [
                { x: 0, y: 0, width: W, height: Math.min(cell.height, Math.floor(H / 2)) },
                { x: 0, y: H - Math.min(cell.height, Math.floor(H / 2)), width: W, height: Math.min(cell.height, Math.floor(H / 2)) },
              ];

    const gapX = cell.width * 0.5;
    const gapY = cell.height * 0.5;
    const strideX = cell.width + gapX;
    const strideY = cell.height + gapY;
    const angle = (-orientationDeg * Math.PI) / 180;
    for (const band of bands) {
      const cols = Math.max(1, Math.ceil(band.width / strideX));
      const rows = Math.max(1, Math.ceil(band.height / strideY));
      for (let row = 0; row < rows; row++) {
        for (let col = 0; col < cols; col++) {
          const cx = band.x + gapX / 2 + col * strideX + cell.width / 2;
          const cy = band.y + gapY / 2 + row * strideY + cell.height / 2;
          ctx.save();
          ctx.translate(cx, cy);
          if (angle) ctx.rotate(angle);
          drawCell(ctx, { x: -cell.width / 2, y: -cell.height / 2, width: cell.width, height: cell.height });
          ctx.restore();
        }
      }
    }
  }

  $effect(() => {
    // Reads of every control keep the preview live; `open` re-draws on
    // first mount of the canvas.
    void [open, text, location, orientationDeg, opacity, textScale, logoBitmap, previewEl];
    drawPreview();
  });

  async function chooseLogo(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    const bitmap = await createImageBitmap(file);
    // Cap the payload: logos render at cell size (well under 200pt), so
    // anything past 512px is invisible fidelity for real cost.
    const scale = Math.min(1, 512 / Math.max(bitmap.width, bitmap.height));
    const w = Math.max(1, Math.round(bitmap.width * scale));
    const h = Math.max(1, Math.round(bitmap.height * scale));
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(bitmap, 0, 0, w, h);
    const rgba = ctx.getImageData(0, 0, w, h).data;
    let binary = "";
    const CHUNK = 0x8000;
    for (let i = 0; i < rgba.length; i += CHUNK) {
      binary += String.fromCharCode(...rgba.subarray(i, Math.min(i + CHUNK, rgba.length)));
    }
    logoRgbaBase64 = btoa(binary);
    logoWidth = w;
    logoHeight = h;
    logoBitmap = bitmap;
    logoName = file.name;
  }

  function removeLogo(): void {
    logoBitmap = null;
    logoName = "";
    logoRgbaBase64 = "";
    logoWidth = 0;
    logoHeight = 0;
  }

  function apply(): void {
    if (!canApply || busy) return;
    onApply({
      text: text.trim(),
      location,
      orientationDeg,
      opacity,
      textScale,
      ...(logoBitmap ? { logoRgbaBase64, logoWidth, logoHeight } : {}),
    });
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="oa-dialog-scrim"
    role="dialog"
    aria-modal="true"
    aria-label="Watermark"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="oa-dialog watermark-dialog">
      <div class="oa-dialog__header">
        <div class="oa-dialog__header-text">
          <h2 class="oa-dialog__title">Watermark</h2>
          <p class="oa-dialog__subtitle">Tiled across every page, baked into the document on Apply. Undo works as usual.</p>
        </div>
        <button class="oa-icon-btn oa-icon-btn--sm" onclick={onClose} aria-label="Close">
          <Icon name="x" size={15} />
        </button>
      </div>
      <div class="oa-dialog__body watermark-body">
        <canvas bind:this={previewEl} width="360" height="200" class="watermark-preview"></canvas>

        <label class="oa-field">
          <span class="oa-field__label">Text</span>
          <input class="oa-input" type="text" bind:value={text} placeholder="CONFIDENTIAL" maxlength="64" />
        </label>

        <div class="watermark-row">
          <label class="oa-field">
            <span class="oa-field__label">Location</span>
            <select class="oa-input" bind:value={location}>
              <option value="full">Whole page</option>
              <option value="top">Top edge</option>
              <option value="bottom">Bottom edge</option>
              <option value="top-bottom">Top and bottom</option>
            </select>
          </label>
          <label class="oa-field">
            <span class="oa-field__label">Angle</span>
            <select class="oa-input" bind:value={orientationDeg}>
              <option value={45}>45° diagonal</option>
              <option value={0}>Horizontal</option>
            </select>
          </label>
        </div>

        <div class="watermark-row">
          <label class="oa-field">
            <span class="oa-field__label">Opacity {Math.round(opacity * 100)}%</span>
            <input type="range" min="0.05" max="1" step="0.05" bind:value={opacity} />
          </label>
          <label class="oa-field">
            <span class="oa-field__label">Text size ×{textScale.toFixed(1)}</span>
            <input type="range" min="0.5" max="2" step="0.1" bind:value={textScale} />
          </label>
        </div>

        <div class="watermark-logo-row">
          <input bind:this={fileEl} type="file" accept="image/*" onchange={chooseLogo} hidden />
          <button class="oa-btn oa-btn--secondary" onclick={() => fileEl?.click()} disabled={busy}>
            <Icon name="image" size={14} />
            {logoBitmap ? "Change logo" : "Add logo"}
          </button>
          {#if logoBitmap}
            <span class="watermark-logo-name">{logoName}</span>
            <button class="oa-icon-btn oa-icon-btn--sm" onclick={removeLogo} aria-label="Remove logo">
              <Icon name="x" size={13} />
            </button>
          {/if}
        </div>
      </div>
      <div class="oa-dialog__footer">
        <button class="oa-btn oa-btn--secondary" onclick={onClose} disabled={busy}>Cancel</button>
        <button class="oa-btn oa-btn--primary" onclick={apply} disabled={!canApply || busy}>
          <Icon name="stamp" size={14} spin={busy} />
          Apply to all pages
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .watermark-dialog {
    width: min(440px, calc(100vw - 48px));
  }
  .oa-dialog__subtitle {
    font-size: 12.5px;
    color: var(--text-muted, #656565);
    margin-top: 2px;
  }
  .watermark-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .watermark-preview {
    width: 100%;
    height: auto;
    border: 1px solid var(--border-hairline, #deded9);
    border-radius: 6px;
    display: block;
  }
  .watermark-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .oa-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .oa-field__label {
    font-size: 12px;
    color: var(--text-muted, #656565);
  }
  .watermark-logo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .watermark-logo-name {
    font-size: 12px;
    color: var(--text-muted, #656565);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
</style>
