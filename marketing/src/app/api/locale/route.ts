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

  const safe = requested.startsWith("/") && !requested.startsWith("//");
  const response = NextResponse.redirect(
    new URL(safe ? requested : "/", request.url),
    307,
  );

  response.cookies.set(LOCALE_COOKIE, locale, {
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
  });

  return response;
}
