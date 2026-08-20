// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

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
  },
};

export default config;
