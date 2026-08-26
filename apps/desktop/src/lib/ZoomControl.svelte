<script lang="ts">
  // The zoom stepper: out, the current percentage, in.
  //
  // A component rather than markup in the topbar because it is rendered
  // in two places — the topbar on a desktop, the file strip on a phone,
  // where a 390px toolbar has no room for it. Only one of the two is
  // ever visible (see the media query in `+page.svelte`), and both drive
  // the same handlers and the same `zoom`, so there is no second state
  // to keep in step. Duplicating the markup instead would mean two
  // places to fix when the control changes.
  import Icon from "./Icon.svelte";
  import { tooltip } from "./tooltip";

  interface Props {
    zoom: number;
    canZoomOut: boolean;
    canZoomIn: boolean;
    onZoomOut: () => void;
    onZoomIn: () => void;
    onReset: () => void;
  }

  let { zoom, canZoomOut, canZoomIn, onZoomOut, onZoomIn, onReset }: Props = $props();
</script>

<button
  class="oa-icon-btn oa-icon-btn--sm zoom-step"
  onclick={onZoomOut}
  disabled={!canZoomOut}
  aria-label="Zoom out"
>
  <Icon name="zoom-out" size={15} />
</button>
<button class="zoom-level oa-mono" onclick={onReset} use:tooltip={"Reset zoom"}
  >{Math.round(zoom * 100)}%</button
>
<button
  class="oa-icon-btn oa-icon-btn--sm zoom-step"
  onclick={onZoomIn}
  disabled={!canZoomIn}
  aria-label="Zoom in"
>
  <Icon name="zoom-in" size={15} />
</button>

<style>
  .zoom-level {
    min-width: 3.2rem;
    height: var(--control-h-sm);
    padding: 0 4px;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: center;
    font-variant-numeric: tabular-nums;
    border-radius: var(--radius-sm);
    transition: var(--transition-control);
  }

  .zoom-level:hover {
    background: var(--surface-hover);
  }

  /* Fingers, not a cursor. The stepper is secondary to pinching, but a
     32px target next to a 44px one still reads as broken rather than
     secondary. */
  @media (max-width: 720px) {
    .zoom-step {
      width: 34px;
      height: 34px;
    }

    .zoom-level {
      height: 34px;
      min-width: 3.4rem;
    }
  }
</style>
