<script lang="ts">
  import { base } from "$app/paths";

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

  // The mask URL is built at runtime, so it does not go through the
  // compiler's asset rewriting the way `src="..."` would -- it has to
  // prepend `base` itself. Empty for the desktop app and the extension,
  // "/app" for the web build, which is served from a path rather than the
  // root of its host. Without this every icon 404s there, and a CSS mask
  // that 404s is not an error anyone sees: the glyph is simply invisible.
</script>

<span
  class="oa-icon"
  class:oa-icon--spin={spin}
  aria-hidden="true"
  style="width: {size}px; height: {size}px; -webkit-mask-image: url('{base}/icons/{name}.svg'); mask-image: url('{base}/icons/{name}.svg');"
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
