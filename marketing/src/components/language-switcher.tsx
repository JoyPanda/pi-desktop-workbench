import { LanguageSkillIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { LOCALE_NAMES, type Locale } from "@/i18n/config";
import { localeHref } from "@/i18n/links";
import { translatedLocales } from "@/i18n/messages";

/**
 * Plain links inside a `<details>` — no JavaScript, no hydration, and every
 * language is a real crawlable URL. The proxy records the choice as a cookie
 * when the link is followed, so `/` stops redirecting afterwards.
 *
 * `locale` is the language of the page it sits on. It is optional: on a page
 * rendered without one the menu shows a globe instead of a current code.
 *
 * Renders nothing until a second dictionary exists, so the page never shows a
 * menu with a single entry.
 */
export function LanguageSwitcher({
  locale,
  label = "Language",
  path = "/",
}: {
  locale?: Locale;
  label?: string;
  /** The page you are on, without any locale prefix. */
  path?: string;
}) {
  if (translatedLocales.length < 2) return null;

  /**
   * Every page exists in every language, so a choice is the same page with a
   * different prefix — `/changelog` becomes `/de/changelog`, not `/de`.
   *
   * The link goes through `/api/locale` rather than straight to the page so the
   * choice is remembered even when the destination carries no prefix: `/` is
   * negotiated, and an English reader coming back later must not be redirected
   * to German again.
   */
  function hrefFor(option: Locale): string {
    const next = encodeURIComponent(localeHref(option, path));
    return `/api/locale?to=${option}&next=${next}`;
  }

  return (
    <details className="group relative [&[open]>summary]:border-edge-strong [&[open]>summary]:bg-surface-2 [&[open]>summary]:text-ink">
      <summary
        aria-label={locale ? `${label}: ${LOCALE_NAMES[locale]}` : label}
        title={label}
        className="inline-flex h-8 shrink-0 cursor-pointer list-none items-center gap-1.5 rounded-[9px] border border-edge-subtle bg-surface-1 px-2 text-ink-2 transition-colors hover:border-edge-strong hover:bg-surface-2 hover:text-ink [&::-webkit-details-marker]:hidden"
      >
        <HugeiconsIcon icon={LanguageSkillIcon} className="size-4" />
        {locale ? (
          <span className="font-mono text-[11px] uppercase tracking-[0.06em]">
            {locale}
          </span>
        ) : null}
      </summary>

      <ul className="absolute right-0 z-[60] mt-2 min-w-[188px] overflow-hidden rounded-[10px] border border-edge-default bg-page p-1 shadow-card">
        {translatedLocales.map((option) => (
          <li key={option}>
            <a
              href={hrefFor(option)}
              hrefLang={option}
              aria-current={option === locale ? "true" : undefined}
              className={`block rounded-[7px] px-2.5 py-1.5 text-[13px] no-underline transition-colors hover:bg-surface-2 hover:text-ink ${
                option === locale ? "text-ink" : "text-ink-2"
              }`}
            >
              {LOCALE_NAMES[option]}
            </a>
          </li>
        ))}
      </ul>
    </details>
  );
}
