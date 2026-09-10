/**
 * The languages OpenPdfEdit ships in.
 *
 * Confirmed with jerry under APP-42. Deliberately a short list rather
 * than the thirty-odd a large competitor carries: every locale here is a
 * permanent commitment — each new string in the product needs a
 * translation in all of them, forever, and a half-translated interface
 * reads worse than an English one.
 *
 * `name` is the language's name *in that language*. A picker that lists
 * "Chinese (Simplified)" in English is no use to someone who cannot read
 * English, which is the entire audience for the control.
 *
 * All eight are left-to-right. Adding Arabic or Hebrew later is a layout
 * change (`dir="rtl"`, logical CSS properties) and not just a
 * translation — worth knowing before anyone promises one.
 */
export interface LocaleDef {
  /** BCP 47 tag. Script subtags on Chinese, because zh-CN/zh-TW encode a
   * region and the difference here is the writing system: a reader in
   * Singapore wants Hans, one in Hong Kong wants Hant. */
  code: string;
  /** Endonym — the language's own name for itself. */
  name: string;
  /** Two or three characters for the topbar, where the full endonym does
   * not fit. Shown beside the globe, so the control says which language
   * is active as well as what it does — and so it has visible text at
   * all, which a phone requires: there is no pointer to hover a tooltip
   * with. */
  short: string;
}

export const LOCALES: readonly LocaleDef[] = [
  { code: "en", name: "English", short: "EN" },
  { code: "zh-Hans", name: "简体中文", short: "简" },
  { code: "zh-Hant", name: "繁體中文", short: "繁" },
  { code: "ja", name: "日本語", short: "日本語" },
  { code: "ko", name: "한국어", short: "한국어" },
  { code: "de", name: "Deutsch", short: "DE" },
  { code: "es", name: "Español", short: "ES" },
  { code: "pt", name: "Português", short: "PT" },
] as const;

export const DEFAULT_LOCALE = "en";

/** Maps whatever the browser reports onto a locale we actually have.
 *
 * `navigator.languages` is ordered by preference and full of tags we do
 * not ship — "en-GB", "zh-TW", "pt-BR". Matching is therefore widening:
 * exact tag, then the script variant Chinese needs, then the base
 * language. Without the middle step a Taiwanese reader whose browser says
 * "zh-TW" gets Simplified, which is worse than getting English.
 */
export function resolveLocale(preferred: readonly string[]): string {
  const have = new Set(LOCALES.map((l) => l.code));
  for (const raw of preferred) {
    const tag = raw.trim();
    if (!tag) continue;
    if (have.has(tag)) return tag;

    const lower = tag.toLowerCase();
    if (lower.startsWith("zh")) {
      // Hant regions, per CLDR's likely-subtags: everything else is Hans.
      const hant = /hant|-tw|-hk|-mo/.test(lower);
      return hant ? "zh-Hant" : "zh-Hans";
    }
    const base = lower.split("-")[0];
    if (have.has(base)) return base;
  }
  return DEFAULT_LOCALE;
}
