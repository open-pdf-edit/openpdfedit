/**
 * `$lib/telegram` for the browser-extension build: nothing at all.
 *
 * A Mini App is a webview Telegram points at an HTTPS page, so only the web
 * build can ever be one. An extension page is opened as
 * chrome-extension://…/index.html, where Telegram's bridge is never present
 * and its sign-in proof can never arrive.
 *
 * It is swapped in at build time (apps/desktop/vite.config.js, when
 * VITE_TARGET=extension) rather than left to run harmlessly, because a store
 * review reads the package: Telegram code in an extension that cannot use it
 * invites questions — about remote code, about what the extension talks to —
 * that are better not raised than answered. The callers are unchanged; they
 * ask whether this is a Mini App and are told no.
 */
export function isTelegram(): boolean {
  return false;
}

export function initData(): string | null {
  return null;
}

export function onBack(_handler: (() => void) | null): void {}

export function tap(): void {}

export function initTelegram(): boolean {
  return false;
}
