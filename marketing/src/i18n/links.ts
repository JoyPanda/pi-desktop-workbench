import { DEFAULT_LOCALE, type Locale } from "./config";

/**
 * The same page in another language.
 *
 * English lives unprefixed (`/docs/mcp`), every other language sits behind its
 * own prefix (`/de/docs/mcp`), so moving between them is a matter of adding or
 * dropping one segment. The landing page is the case that needs care: `/` and
 * `/#agent` become `/de` and `/de#agent`, not `/de/`.
 *
 * External URLs and `mailto:` links are returned untouched — a language choice
 * applies to this site, not to GitHub.
 */
export function localeHref(locale: Locale, href: string): string {
  if (!href.startsWith("/")) return href;
  if (locale === DEFAULT_LOCALE) return href;

  const hashAt = href.indexOf("#");
  const path = hashAt === -1 ? href : href.slice(0, hashAt);
  const hash = hashAt === -1 ? "" : href.slice(hashAt);

  return `/${locale}${path === "/" ? "" : path}${hash}`;
}
