// Universal hook, run client-side before route matching (see SvelteKit's
// `reroute` docs). Needed for the packaged Chrome extension specifically:
// `apps/extension/scripts/build-spa.sh` ships this SPA's `adapter-static`
// fallback `index.html` as the extension's actual boot page, opened at
// `chrome-extension://<id>/index.html` — so the client router's very
// first navigation has `url.pathname === "/index.html"`, which matches
// neither `/` nor `/login` in the route table and renders SvelteKit's own
// "404 — Not found" instead of the editor. On desktop (Tauri loads
// `index.html` at a root-relative path, so `url.pathname` is already
// `/`) this is a no-op — `undefined` leaves the URL as SvelteKit found it.
import type { Reroute } from "@sveltejs/kit";

export const reroute: Reroute = ({ url }) => {
  if (url.pathname === "/index.html") {
    return "/";
  }
  return undefined;
};
