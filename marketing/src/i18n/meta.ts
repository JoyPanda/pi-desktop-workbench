import type { Metadata } from "next";

import { localeHref } from "./links";
import { translatedLocales } from "./messages";
import type { Locale } from "./config";

/**
 * `canonical` for the language the reader is on, plus an `hreflang` map pointing
 * at the same page in every other one — so ten copies of `/docs/mcp` are indexed
 * as translations of each other instead of ten near-duplicates.
 */
export function localeAlternates(
  locale: Locale,
  /** The page's route, without a locale prefix. */
  path: string,
): Metadata["alternates"] {
  return {
    canonical: localeHref(locale, path),
    languages: Object.fromEntries([
      ...translatedLocales.map((entry) => [entry, localeHref(entry, path)]),
      ["x-default", path],
    ]),
  };
}
