"use client";

import { ErrorView } from "@/components/error-view";
import { DEFAULT_LOCALE } from "@/i18n/config";

/**
 * Route-level error boundary for everything outside `[locale]` — the root
 * layout itself. There is no locale to read at this level, so it renders the
 * default language rather than guessing.
 */
export default function Error({
  error,
  retry,
}: {
  error: Error & { digest?: string };
  retry: () => void;
}) {
  return <ErrorView locale={DEFAULT_LOCALE} error={error} retry={retry} />;
}
