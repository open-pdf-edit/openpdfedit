// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { createRequire } from "node:module";

const { version } = createRequire(import.meta.url)("./package.json");

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({
      fallback: "index.html",
    }),
    // SvelteKit's default asset dir is "_app", but Chrome REFUSES to load
    // an unpacked extension containing any top-level file/dir starting
    // with "_" ("reserved for use by the system") — and the same build
    // output ships as the extension SPA. Playwright's --load-extension
    // path never enforces that check, so only a human load-unpacked
    // catches it; build-spa.sh now guards against any _-prefixed root
    // entry for the same reason. Harmless on the Tauri side.
    appDir: "app",

    // Pinned, because SvelteKit's default is `Date.now()` — which lands
    // in a chunk, changes its content hash, and cascades into every
    // importer's hash. Two builds of identical source would then emit
    // wholly different filenames, which is what makes the web app's
    // service worker treat an unchanged rebuild as a new release and
    // re-download ~9 MB of WebAssembly for every returning visitor (see
    // apps/webapp/scripts/build.sh, which derives its cache name from a
    // digest of the build). Nothing here uses SvelteKit's own
    // version-change detection, which is what that default exists for.
    version: { name: version },
  },
};

export default config;
