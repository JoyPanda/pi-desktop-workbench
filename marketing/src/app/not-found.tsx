import type { Metadata } from "next";

import { NotFoundView } from "@/components/not-found-view";
import { DEFAULT_LOCALE } from "@/i18n/config";

export const metadata: Metadata = {
  title: "Page not found",
  description: "That page drifted off course. Head back to Orbit.",
};

/**
 * A URL that matches no route has no locale segment to read a language from, and
 * reading the cookie here would make every route dynamic — so the fallback is the
 * default language. Localized 404s inside the site are rendered by the pages that
 * call `notFound()` themselves.
 */
export default function NotFound() {
  return <NotFoundView locale={DEFAULT_LOCALE} />;
}
