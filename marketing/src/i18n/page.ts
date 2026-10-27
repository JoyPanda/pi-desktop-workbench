import { notFound } from "next/navigation";

import { isLocale, type Locale } from "./config";

/**
 * The locale of the `[locale]` segment, narrowed to a `Locale`.
 *
 * The layout already 404s an unknown segment; this is what turns the raw
 * `string` from `params` into the type the translators and link helper need, so
 * every page starts with one line instead of its own guard.
 */
export async function pageLocale(
  params: Promise<{ locale: string }>,
): Promise<Locale> {
  const { locale } = await params;
  if (!isLocale(locale)) notFound();
  return locale;
}
