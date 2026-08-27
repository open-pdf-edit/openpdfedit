/**
 * Running as a Telegram Mini App.
 *
 * A Mini App is a webview pointed at an HTTPS URL with Telegram's bridge on
 * the page — so the web build already *is* one, and everything here is the
 * wrapper rather than a port: tell Telegram we have painted, take the theme
 * and the safe areas it offers, and make its back button mean what our own
 * back means.
 *
 * ## Files stay on the device
 *
 * The one decision that matters, and it is a decision rather than a
 * limitation. Two ways a Mini App can get a document:
 *
 * - **A file input in the webview.** The bytes go straight into WASM here.
 *   Nothing is uploaded, and the only size limit is what the device holds.
 * - **Through the bot.** The user sends the PDF to the bot, which fetches it
 *   by `file_id`. The document goes to Telegram's servers, and the Bot API
 *   caps downloads at 20 MB.
 *
 * Most Telegram document bots do the second. It would quietly invert the one
 * claim this product is built on — everything runs on your machine — so this
 * app does the first, and `openFileFromTelegram` deliberately does not exist.
 *
 * A consequence worth knowing rather than discovering: "open this PDF from a
 * chat" is therefore not available. It needs the bot path, and the
 * attachment menu that would make it pleasant is restricted to major
 * Telegram advertisers anyway.
 */

/** The slice of Telegram's API this app uses. */
interface TelegramWebApp {
  initData: string;
  colorScheme: "light" | "dark";
  themeParams: Record<string, string>;
  isExpanded: boolean;
  viewportStableHeight?: number;
  ready(): void;
  expand(): void;
  disableVerticalSwipes?(): void;
  onEvent(event: string, handler: () => void): void;
  offEvent(event: string, handler: () => void): void;
  BackButton: {
    show(): void;
    hide(): void;
    onClick(handler: () => void): void;
    offClick(handler: () => void): void;
  };
  HapticFeedback?: {
    impactOccurred(style: "light" | "medium" | "heavy"): void;
  };
}

declare global {
  interface Window {
    Telegram?: { WebApp?: TelegramWebApp };
  }
}

function webApp(): TelegramWebApp | null {
  if (typeof window === "undefined") return null;
  // `initData` is empty when the same URL is opened in an ordinary browser,
  // even though Telegram's script has loaded. Presence of the object is not
  // the question; presence of a session is.
  const app = window.Telegram?.WebApp;
  return app && app.initData ? app : null;
}

/** Whether this page is running inside Telegram. */
export function isTelegram(): boolean {
  return webApp() !== null;
}

/**
 * The `initData` string, for sign-in.
 *
 * Handed to `/v1/auth/verify` as `{type: "telegram_init_data", init_data}`.
 * Never logged and never put in a URL: it carries no server nonce, so it is
 * a bearer credential for its whole `auth_date` window, and anything that
 * can read it can replay it until the server's single-use guard fires.
 */
export function initData(): string | null {
  return webApp()?.initData ?? null;
}

/**
 * Copy Telegram's theme onto our own CSS custom properties.
 *
 * The app already themes through semantic tokens, so this maps rather than
 * restyles: a Mini App that ignores the user's Telegram theme looks like a
 * website someone embedded, which is exactly what it is and exactly what it
 * should not feel like.
 */
function applyTheme(app: TelegramWebApp): void {
  const root = document.documentElement;
  root.classList.toggle("oa-dark", app.colorScheme === "dark");

  const map: Record<string, string> = {
    bg_color: "--bg-page",
    secondary_bg_color: "--surface-card",
    text_color: "--text-strong",
    hint_color: "--text-muted",
    link_color: "--brand",
    button_color: "--brand",
  };
  for (const [from, to] of Object.entries(map)) {
    const value = app.themeParams[from];
    if (value) root.style.setProperty(to, value);
  }
}

/**
 * Keep the layout inside the visible viewport.
 *
 * Telegram's own chrome overlaps the webview, and `100vh` is wrong by
 * exactly that much — a full-height editor ends up with its toolbar under
 * the drag handle. `--tg-viewport-height` is what layout should use when
 * this is running in Telegram.
 */
function applyViewport(app: TelegramWebApp): void {
  const set = () => {
    const height = app.viewportStableHeight;
    if (height) {
      document.documentElement.style.setProperty("--tg-viewport-height", `${height}px`);
    }
  };
  set();
  app.onEvent("viewportChanged", set);
}

/**
 * Wire up Telegram's hardware-ish back button.
 *
 * Returns a function that registers what "back" should do. Telegram's back
 * button replaces the browser's, and leaving it unhandled means the only way
 * out of a panel is to close the whole Mini App.
 */
export function onBack(handler: (() => void) | null): void {
  const app = webApp();
  if (!app) return;
  const button = app.BackButton;
  // Only one handler at a time; a stale one from a closed panel would fire
  // alongside the live one.
  if (backHandler) button.offClick(backHandler);
  backHandler = handler ?? undefined;
  if (backHandler) {
    button.onClick(backHandler);
    button.show();
  } else {
    button.hide();
  }
}
let backHandler: (() => void) | undefined;

/** A short tap, where the platform offers one. Silently absent elsewhere. */
export function tap(): void {
  webApp()?.HapticFeedback?.impactOccurred("light");
}

/**
 * Initialise, if we are in Telegram. Safe and cheap to call anywhere else.
 *
 * Returns whether it did anything, so a caller can branch without repeating
 * the detection.
 */
export function initTelegram(): boolean {
  const app = webApp();
  if (!app) return false;

  // Tells Telegram the page has painted; until this is called the user sees
  // its loading placeholder over a rendered app.
  app.ready();
  if (!app.isExpanded) app.expand();
  // An editor is full of drag gestures, and Telegram reads a downward drag
  // as "close the Mini App" unless asked not to. Optional-chained because
  // it postdates the older clients this may open in.
  app.disableVerticalSwipes?.();

  applyTheme(app);
  applyViewport(app);
  app.onEvent("themeChanged", () => applyTheme(app));

  document.documentElement.classList.add("in-telegram");
  return true;
}
