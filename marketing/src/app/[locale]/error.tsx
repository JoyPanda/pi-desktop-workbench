"use client";

import { ErrorView } from "@/components/error-view";
import { DEFAULT_LOCALE, isLocale, type Locale } from "@/i18n/config";
import { useLocale } from "next-intl";

/**
 * Error boundary for every localized page. It sits inside `[locale]/layout`, so
 * the next-intl provider is above it and the locale is still known here — a
 * German page fails in German.
 */
export default function LocaleError({
  error,
  retry,
}: {
  error: Error & { digest?: string };
  retry: () => void;
}) {
  const current = useLocale();

  return (
    <ErrorView
      locale={(isLocale(current) ? current : DEFAULT_LOCALE) as Locale}
      error={error}
      retry={retry}
    />
  );
}
