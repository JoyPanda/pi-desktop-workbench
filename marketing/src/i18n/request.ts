import { hasLocale } from "next-intl";
import { getRequestConfig } from "next-intl/server";

import { messagesFor } from "./messages";
import { routing } from "./routing";

/**
 * Resolves the locale for a request from the `[locale]` segment (next-intl's
 * proxy puts it there). `setRequestLocale()` in the layout pins it so the route
 * stays statically prerendered — no cookie or header is read during render.
 */
export default getRequestConfig(async ({ requestLocale }) => {
  const requested = await requestLocale;
  const locale = hasLocale(routing.locales, requested)
    ? requested
    : routing.defaultLocale;

  return { locale, messages: messagesFor(locale) };
});
