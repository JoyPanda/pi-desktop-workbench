import { defineRouting } from "next-intl/routing";

import { DEFAULT_LOCALE, LOCALES, LOCALE_COOKIE } from "./config";

/**
 * next-intl's routing for every page.
 *
 * `as-needed` keeps the default language unprefixed (`/docs/mcp`) and puts every
 * other one behind its own path (`/de/docs/mcp`), which is what makes a shared
 * link resolve independently of the reader's browser settings.
 *
 * `localeDetection` is off on purpose: next-intl's detection would redirect a
 * German browser away from `/docs/mcp`, and an unprefixed URL should mean the
 * English page it names. Only `/` negotiates — the proxy does that itself, from
 * `NEXT_LOCALE` then `Accept-Language`.
 */
export const routing = defineRouting({
  locales: LOCALES,
  defaultLocale: DEFAULT_LOCALE,
  localePrefix: "as-needed",
  localeDetection: false,
  localeCookie: {
    name: LOCALE_COOKIE,
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
  },
});
