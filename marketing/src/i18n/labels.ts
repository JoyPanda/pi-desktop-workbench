import { getT } from "./t";

import type { Locale } from "./config";

/**
 * The three strings the OS-aware download button needs. It is a client component
 * (it sniffs the platform), so the labels are resolved on the server and handed
 * down as data.
 *
 * `forPlatform` is taken with `raw()` on purpose: it is the only one holding an
 * ICU variable, and the value (`macOS`, `Windows`, …) is not known until the
 * browser reports its platform. Formatting it here would ask next-intl to
 * interpolate `{platform}` with nothing — a `FORMATTING_ERROR` in development and
 * a fallback key in production. The raw template travels down and `fill()`
 * substitutes it in the browser.
 */
export function downloadLabels(locale: Locale) {
  const t = getT(locale, "download");
  return {
    download: t("download"),
    forPlatform: t.raw("forPlatform") as string,
    otherPlatforms: t("otherPlatforms"),
  };
}
