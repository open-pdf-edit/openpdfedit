// Offline support for the web app.
//
// The product's central claim is that documents never leave your
// machine. A page that needs the network to start is a weaker version of
// that claim than one that doesn't — with this, you can load the app
// once, disconnect entirely, and keep editing. That is the claim being
// demonstrable rather than merely asserted.
//
// Strategy: precache the app shell on install, then serve
// cache-first for everything same-origin. Cache-first (rather than
// network-first) because every asset here is content-hashed or
// versioned by CACHE — a stale hit is impossible for the hashed
// bundles, and for the handful of unhashed ones (index.html, the wasm
// binaries) a new CACHE name is what replaces them.
//
// Bumping CACHE is what ships an update: the activate handler deletes
// every other cache, so a released build never serves a mix of old and
// new chunks.
const CACHE = "openpdfedit-v0.1.4";

// The unhashed entry points. Everything else (hashed JS/CSS chunks,
// fonts, icons) is added on first use by the fetch handler below —
// listing them here would mean regenerating this file on every build.
const SHELL = [
  "./",
  "./index.html",
  "./pdfium.js",
  "./pdfium.wasm",
  "./wasm-gen/openpdfedit_wasm.js",
  "./wasm-gen/openpdfedit_wasm_bg.wasm",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      // Individually, not addAll: addAll rejects the whole install if a
      // single request fails, which would leave the app with no service
      // worker at all over one unlucky asset.
      .then((cache) =>
        Promise.all(
          SHELL.map((url) => cache.add(url).catch(() => undefined)),
        ),
      )
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(names.filter((n) => n !== CACHE).map((n) => caches.delete(n))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  // Only this origin. The app makes no cross-origin requests at all —
  // and if that ever changes, caching someone else's response here is
  // not the behaviour anyone would want.
  if (url.origin !== self.location.origin) return;

  event.respondWith(
    caches.match(request).then((hit) => {
      if (hit) return hit;
      return fetch(request)
        .then((response) => {
          // Opaque and error responses are not worth persisting; a
          // failed asset should be retried next time, not cached.
          if (!response.ok || response.type === "opaque") return response;
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
          return response;
        })
        .catch(() => {
          // Offline and not cached. For a navigation, fall back to the
          // shell so the SPA still boots and can route itself; anything
          // else genuinely fails.
          if (request.mode === "navigate") return caches.match("./index.html");
          throw new Error(`offline and not cached: ${url.pathname}`);
        });
    }),
  );
});
