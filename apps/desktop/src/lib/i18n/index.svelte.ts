/**
 * Translation, with no runtime dependency.
 *
 * ## Why the English text is the key
 *
 * `t("Save a copy")` rather than `t("toolbar.saveAs")`. That is gettext's
 * convention and Apple's, and it earns its place here for three reasons:
 *
 * - **A missing translation degrades to English**, not to `toolbar.saveAs`
 *   on a button. With 176 strings across eight locales there will always
 *   be a gap somewhere, and the failure has to be survivable.
 * - **The replacement is mechanical.** Wrapping a literal cannot change
 *   what the interface says, so this can be applied across 27 components
 *   without re-reading each one for intent.
 * - **The catalogue reads as prose**, so a translator sees the sentence
 *   rather than a key they have to go and look up.
 *
 * The cost is that editing English copy orphans its translations. That is
 * the right trade while English is the source of truth: a changed
 * sentence *should* be re-translated, and `missing()` below makes the gap
 * visible instead of silent.
 *
 * ## Why no library
 *
 * svelte-i18n and i18next both bring a store layer, an ICU parser and
 * async loading. This app is local-first and ships inside a browser
 * extension and an iOS bundle, where every kilobyte is one someone waits
 * for. Eight catalogues of 176 short strings are about 15 KB gzipped
 * altogether, so they are imported statically: no async, no loading
 * state, and no flash of untranslated text on a cold start with no
 * network — which is the state this app is designed to work in.
 */
import { DEFAULT_LOCALE, resolveLocale } from "./locales";
import de from "./de";
import es from "./es";
import ja from "./ja";
import ko from "./ko";
import pt from "./pt";
import zhHans from "./zh-Hans";
import zhHant from "./zh-Hant";

export { LOCALES, DEFAULT_LOCALE } from "./locales";
export type { LocaleDef } from "./locales";

export type Catalogue = Record<string, string>;

const CATALOGUES: Record<string, Catalogue> = {
  en: {},          // English is the key set; nothing to look up.
  de,
  es,
  ja,
  ko,
  pt,
  "zh-Hans": zhHans,
  "zh-Hant": zhHant,
};

const STORAGE_KEY = "openpdfedit.locale";

/** The active locale. `$state` so every `t()` call site re-renders when it
 * changes — a language picker that needs a reload is not a language
 * picker. */
let locale = $state(DEFAULT_LOCALE);

/** Reads the stored choice, else what the browser asks for.
 *
 * Called once from the root layout rather than at module scope: this
 * module is imported by the extension's service worker and by the iOS
 * shell's prerender, neither of which has a `window`. */
export function initLocale(): void {
  if (typeof window === "undefined") return;
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch {
    // Private browsing, or storage disabled. The browser's own
    // preference is a good enough answer.
  }
  const preferred = stored ? [stored] : [...(navigator.languages ?? [navigator.language])];
  setLocale(resolveLocale(preferred), { persist: false });
}

export function getLocale(): string {
  return locale;
}

export function setLocale(next: string, options: { persist?: boolean } = {}): void {
  if (!(next in CATALOGUES)) return;
  locale = next;
  if (typeof document !== "undefined") {
    // Not cosmetic: `lang` is what picks the right glyphs for Han
    // characters — the same codepoint is drawn differently in Japanese
    // and Chinese — and what a screen reader switches voice on.
    document.documentElement.lang = next;
  }
  if (options.persist !== false) {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // The choice still applies to this session.
    }
  }
}

/**
 * Translate `text`, falling back to the English it was written in.
 *
 * `vars` interpolates `{name}` placeholders. Kept deliberately small —
 * no plural rules, no date formats — because nothing in this interface
 * needs them yet, and `Intl.PluralRules` is there for the day it does.
 */
export function t(text: string, vars?: Record<string, string | number>): string {
  const table = CATALOGUES[locale];
  let out = (table && table[text]) || text;
  if (vars) {
    for (const [key, value] of Object.entries(vars)) {
      out = out.replaceAll(`{${key}}`, String(value));
    }
  }
  return out;
}

/** Which keys a locale has no translation for. Used by the i18n test, so
 * a new English string cannot quietly ship untranslated in seven
 * languages. */
export function missing(code: string, keys: readonly string[]): string[] {
  if (code === "en") return [];
  const table = CATALOGUES[code];
  if (!table) return [...keys];
  return keys.filter((k) => !(k in table));
}
