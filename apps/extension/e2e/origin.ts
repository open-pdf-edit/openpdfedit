/**
 * Where the web-app specs point.
 *
 * Nine specs in this suite drive the *web app* rather than the
 * extension's own pages, and each used to hardcode
 * `http://localhost:8099`. Two things were wrong with that.
 *
 * Nothing started the server, so the suite depended on someone having
 * run one by hand; with nothing listening, nine specs fail with 30s
 * timeouts that read like product bugs. `playwright.config.ts` now
 * starts and stops one.
 *
 * And 8099 was not ours alone — another app in this suite binds
 * 127.0.0.1:8099 on the same machine. Chromium resolves "localhost" to
 * IPv4, so the specs silently drove a different product and reported
 * this one as broken. Hence a port of our own, and 127.0.0.1 rather than
 * "localhost": the literal address cannot resolve to the other family.
 */
export const WEBAPP_PORT = Number(process.env.WEBAPP_PORT ?? 8137);
export const ORIGIN = process.env.WEBAPP_ORIGIN ?? `http://127.0.0.1:${WEBAPP_PORT}`;

/**
 * Where the desktop-UI specs point: the Svelte app built as the desktop
 * build is — `backendKind` "tauri" — served by Vite, in a plain browser.
 *
 * Components that exist only in the desktop build (the updater, above
 * all) render nowhere else, so until this server existed they had no
 * browser coverage at all. APP-29's popup that could not be closed is
 * what that gap shipped. With no Tauri runtime underneath, their plugin
 * calls fail — which is the same state an update check reaches on a build
 * whose release has no update manifest, so it is a real path, not a stub.
 */
export const DESKTOP_PORT = Number(process.env.DESKTOP_PORT ?? 8138);
export const DESKTOP_ORIGIN = `http://127.0.0.1:${DESKTOP_PORT}`;
