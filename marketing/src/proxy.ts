import createMiddleware from "next-intl/middleware";
import { NextResponse, type NextRequest } from "next/server";

import {
  DEFAULT_LOCALE,
  LOCALE_COOKIE,
  isLocale,
  localePath,
  negotiateLocale,
  type Locale,
} from "@/i18n/config";
import { routing } from "@/i18n/routing";

const handle = createMiddleware(routing);

/** `zh-TW`, `zh-HK`, `zh-MO`, `zh-Hant*` — written in Traditional characters. */
const TRADITIONAL_CHINESE = /^zh-(tw|hk|mo|hant)/i;

/**
 * The reader's most preferred language, when it is Chinese.
 *
 * next-intl's negotiation folds every `zh-*` tag into `zh-CN`, so a Traditional
 * reader would be handed Simplified text. Serving English instead is the lesser
 * evil: it is still not their language, but it is not the wrong script either.
 * An explicit choice (`NEXT_LOCALE`, set by the switcher) always wins.
 */
function prefersTraditionalChinese(request: NextRequest): boolean {
  if (request.cookies.has(LOCALE_COOKIE)) return false;

  const ranked = (request.headers.get("accept-language") ?? "")
    .split(",")
    .map((part) => {
      const [tag = "", ...params] = part.trim().split(";");
      const q = params.find((p) => p.trim().startsWith("q="));
      return { tag: tag.trim(), q: q ? Number(q.slice(2)) : 1 };
    })
    .filter((entry) => entry.tag.length > 0 && Number.isFinite(entry.q))
    .sort((a, b) => b.q - a.q);

  const preferredZh = ranked.find((entry) => /^zh\b/i.test(entry.tag));
  return preferredZh !== undefined && TRADITIONAL_CHINESE.test(preferredZh.tag);
}

/**
 * The language `/` should land on: the reader's own choice first (`NEXT_LOCALE`,
 * written by the switcher), then the browser's `Accept-Language`.
 */
function preferredLocale(request: NextRequest): Locale {
  const chosen = request.cookies.get(LOCALE_COOKIE)?.value;
  if (chosen && isLocale(chosen)) return chosen;
  if (prefersTraditionalChinese(request)) return DEFAULT_LOCALE;
  return negotiateLocale(request.headers.get("accept-language"));
}

/**
 * Locale routing for every page.
 *
 * `/` is the only URL that does not state a language, so it is the only one that
 * negotiates — by cookie, then by `Accept-Language` — and it redirects to the
 * prefixed path when the answer is not English. Everything else means what it
 * says: `/docs/mcp` is English, `/de/docs/mcp` is German, and the switcher links
 * straight at the one the reader picked. Unprefixed routes are therefore stable,
 * shareable English URLs, never a redirect through the browser's settings.
 */
export function proxy(request: NextRequest) {
  if (request.nextUrl.pathname === "/") {
    const locale = preferredLocale(request);
    if (locale !== DEFAULT_LOCALE) {
      return NextResponse.redirect(new URL(localePath(locale), request.url));
    }
  }

  return handle(request);
}

export const config = {
  // Every page, minus the paths that cannot have a locale: `/api/locale` records
  // the choice, Next's internals, and anything with a file extension (sitemap,
  // RSS feed, manifest, icons, OG cards).
  matcher: ["/((?!api|_next|_vercel|.*\\..*).*)"],
};
