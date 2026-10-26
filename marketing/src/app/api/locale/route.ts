import { NextResponse, type NextRequest } from "next/server";

import { DEFAULT_LOCALE, LOCALE_COOKIE, isLocale } from "@/i18n/config";

/**
 * Records a language choice and sends the reader on.
 *
 * The switcher cannot link straight at the destination: English lives at
 * unprefixed paths (`/changelog`, `/`), so there is no `/{locale}` page to visit
 * that would let next-intl write `NEXT_LOCALE`. One redirect, no JavaScript.
 *
 * `next` is constrained to a same-origin absolute path so this can never become
 * an open redirect.
 */
export function GET(request: NextRequest) {
  const to = request.nextUrl.searchParams.get("to") ?? DEFAULT_LOCALE;
  const requested = request.nextUrl.searchParams.get("next") ?? "/";
  const locale = isLocale(to) ? to : DEFAULT_LOCALE;

  /**
   * The `Location` is path-relative on purpose. `request.url` carries the
   * internal origin the server is reached on (this site is proxied from
   * `localhost:3000`), so an absolute redirect built from it would drag a
   * reader on production to localhost. A relative target is resolved by the
   * browser against the URL that was requested, which is public by definition.
   * Re-encoding keeps a decoded path (`/de/café`) legal in a header.
   */
  const safe = requested.startsWith("/") && !requested.startsWith("//");
  const response = new NextResponse(null, {
    status: 307,
    headers: { location: encodeURI(safe ? requested : "/") },
  });

  response.cookies.set(LOCALE_COOKIE, locale, {
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
  });

  return response;
}
