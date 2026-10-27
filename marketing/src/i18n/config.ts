/**
 * Locale registry for the landing page. The marketing site is English-only
 * except for `/` — the landing page is also served at `/{locale}` for each
 * entry in `TRANSLATED_LOCALES`, and nothing else is localized (docs, changelog,
 * and the legal pages stay English).
 *
 * Adding a language is two steps: add its tag here, then drop a dictionary in
 * `./dictionaries`. The switcher, the redirect, and `generateStaticParams` all
 * read from this list, so nothing else needs touching.
 */

export const LOCALES = [
  "en",
  "de",
  "es",
  "fr",
  "it",
  "ja",
  "ko",
  "pt-BR",
  "ru",
  "zh-CN",
] as const;

export type Locale = (typeof LOCALES)[number];

export const DEFAULT_LOCALE: Locale = "en";

/**
 * Remembers an explicit choice so a later visit to `/` does not re-redirect.
 * Written by `/api/locale`, read by next-intl's locale detection.
 */
export const LOCALE_COOKIE = "NEXT_LOCALE";

/** Endonyms — a switcher that says "German" is useless to a German speaker. */
export const LOCALE_NAMES: Record<Locale, string> = {
  en: "English",
  de: "Deutsch",
  es: "Español",
  fr: "Français",
  it: "Italiano",
  ja: "日本語",
  ko: "한국어",
  "pt-BR": "Português (Brasil)",
  ru: "Русский",
  "zh-CN": "简体中文",
};

export function isLocale(value: string): value is Locale {
  return (LOCALES as readonly string[]).includes(value);
}

/**
 * Best supported locale for an `Accept-Language` header, highest q first.
 *
 * Two deliberate wrinkles: a regional tag falls back to its language
 * (`de-AT` → `de`, `pt-PT` → `pt-BR`), and Traditional Chinese tags are *not*
 * folded into `zh-CN` — handing a Traditional reader Simplified text is worse
 * than leaving them on English.
 */
export function negotiateLocale(header: string | null): Locale {
  if (!header) return DEFAULT_LOCALE;

  const ranked = header
    .split(",")
    .map((part) => {
      const [tag = "", ...params] = part.trim().split(";");
      const q = params.find((p) => p.trim().startsWith("q="));
      return { tag: tag.trim().toLowerCase(), q: q ? Number(q.slice(2)) : 1 };
    })
    .filter((entry) => entry.tag.length > 0 && Number.isFinite(entry.q))
    .sort((a, b) => b.q - a.q);

  for (const { tag } of ranked) {
    const exact = LOCALES.find((locale) => locale.toLowerCase() === tag);
    if (exact) return exact;

    const primary = tag.split("-")[0];
    if (primary === "zh" && !tag.startsWith("zh-cn")) continue;

    const sameLanguage = LOCALES.find(
      (locale) => locale.toLowerCase().split("-")[0] === primary,
    );
    if (sameLanguage) return sameLanguage;
  }

  return DEFAULT_LOCALE;
}

/** `/` for the default language, `/{locale}` for everything else. */
export function localePath(locale: Locale): string {
  return locale === DEFAULT_LOCALE ? "/" : `/${locale}`;
}
