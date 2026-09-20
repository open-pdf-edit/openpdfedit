/**
 * `$lib/telegram` for every build made from this repository: not a Mini App.
 *
 * A Mini App is a webview Telegram points at an HTTPS page it controls, and
 * the bridge that makes one work is a <script> fetched from telegram.org.
 * That script has no place in a package anyone ships: MV3 forbids remote
 * code outright, an iOS bundle carrying it is a question at review, and the
 * desktop app makes no third-party request by design — which is the claim
 * privacy.html makes and a test asserts.
 *
 * So the bridge lives in the Telegram build's own private repository, and
 * this — "no, and nothing to wait for" — is what every caller here gets.
 * That build points TELEGRAM_BRIDGE at its own implementation; see
 * svelte.config.js.
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
