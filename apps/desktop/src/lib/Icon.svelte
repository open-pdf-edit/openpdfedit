<script lang="ts">
  // Lucide glyph rendered as a CSS mask so it inherits `currentColor`,
  // matching the OpenApps design system's `Icon` component. The SVGs are
  // vendored into static/icons/ (not fetched from the jsDelivr CDN the
  // design system defaults to) so icons keep working with no network —
  // this is a local-first desktop app, and a blank toolbar the moment
  // you're offline would be a real regression, not a cosmetic one.

  interface Props {
    name: string;
    size?: number;
    /** A genuine loading spinner — the one motion exception to "nothing
     * spins" (readme.md, Motion). Pass this instead of swapping in a
     * separate icon per call site. */
    spin?: boolean;
  }

  let { name, size = 16, spin = false }: Props = $props();
</script>

<span
  class="oa-icon"
  class:oa-icon--spin={spin}
  aria-hidden="true"
  style="width: {size}px; height: {size}px; -webkit-mask-image: url('/icons/{name}.svg'); mask-image: url('/icons/{name}.svg');"
></span>

<style>
  .oa-icon {
    display: inline-block;
    flex: 0 0 auto;
    background: currentColor;
    -webkit-mask-size: contain;
    mask-size: contain;
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
    -webkit-mask-position: center;
    mask-position: center;
  }

  .oa-icon--spin {
    animation: oa-spin 700ms linear infinite;
  }
</style>
