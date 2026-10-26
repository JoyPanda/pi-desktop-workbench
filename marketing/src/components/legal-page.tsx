import type { ReactNode } from "react";

import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, SectionLabel, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import { longDate } from "@/i18n/format";
import type { Locale } from "@/i18n/config";

/**
 * Shared shell for legal and policy pages: a titled header band and a single
 * measured prose column. Content is authored as plain markup and styled by
 * `.prose-orbit` in globals.css.
 */
export function LegalPage({
  locale,
  label,
  path,
  title,
  updated,
  intro,
  children,
}: {
  locale: Locale;
  label?: string;
  /** The page's own route, so the language switcher can keep the reader here. */
  path: string;
  title: string;
  /** ISO date (`2026-10-14`); formatted in the reader's language. */
  updated: string;
  intro?: string;
  children: ReactNode;
}) {
  const t = getT(locale, "legal");

  return (
    <>
      <SiteNav locale={locale} path={path} />
      <main className="flex-1">
        <Shell>
          <Band dashed className="relative overflow-hidden py-16 sm:py-20">
            <div className="relative z-[1]">
              <SectionLabel className="mb-5">{label ?? t("label")}</SectionLabel>
              <h1 className="max-w-[20ch] text-[clamp(32px,4.4vw,52px)] font-[250] leading-[1.08] tracking-[-0.02em] text-ink">
                {title}
              </h1>
              {intro ? (
                <p className="mt-5 max-w-[56ch] text-[15.5px] leading-[1.7] text-ink-2">
                  {intro}
                </p>
              ) : null}
              <p className="mt-6 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                {t("updated", { date: longDate(locale, updated) })}
              </p>
            </div>
          </Band>

          <Band className="py-14 sm:py-16">
            <article className="prose-orbit max-w-[74ch]">{children}</article>
          </Band>
        </Shell>
      </main>
      <SiteFooter locale={locale} />
    </>
  );
}
