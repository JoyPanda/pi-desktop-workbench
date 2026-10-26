import type { Locale } from "./config";

/**
 * A date in the reader's language — `October 14, 2026`, `14. Oktober 2026`,
 * `2026年10月14日`.
 *
 * Pages carry the machine-readable value (`2026-10-14`) and format it here, so
 * ten dictionaries never have to agree on how to spell a month. UTC is pinned
 * because the value is a calendar date, not an instant: without it a server west
 * of Greenwich renders the day before.
 */
export function longDate(locale: Locale, iso: string): string {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "long",
    timeZone: "UTC",
  }).format(new Date(`${iso}T00:00:00Z`));
}
