import { ArrowLeft01Icon, Download01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitField } from "@/components/orbit-field";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, ButtonLink, SectionLabel, Shell } from "@/components/ui";
import { SITE } from "@/lib/site";
import { getT } from "@/i18n/t";
import { localeHref } from "@/i18n/links";
import type { Locale } from "@/i18n/config";

/**
 * The 404 body, shared by the root `not-found.tsx` and the one inside
 * `[locale]`. The root fallback picks the language from `NEXT_LOCALE`, since a
 * URL that matches no route has no locale segment to read it from.
 */
export async function NotFoundView({ locale }: { locale: Locale }) {
  const t = getT(locale, "errors.notFound");

  return (
    <>
      <SiteNav locale={locale} path="/" />
      <main className="flex-1">
        <Shell>
          <Band className="relative overflow-hidden py-24 text-center sm:py-32">
            <OrbitField />
            <div className="relative z-[1] mx-auto flex max-w-[620px] flex-col items-center">
              <SectionLabel>{t("label")}</SectionLabel>
              <h1 className="mt-5 text-balance text-[clamp(34px,5vw,54px)] font-semibold leading-[1.08] tracking-[-0.03em] text-ink">
                {t("heading")}
              </h1>
              <p className="mt-5 max-w-[48ch] text-[15px] leading-[1.75] text-ink-2">
                {t("body")}
              </p>

              <div className="mt-9 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center">
                <ButtonLink
                  href={localeHref(locale, "/")}
                  className="min-h-12 px-6 text-sm sm:min-w-[184px]"
                >
                  <HugeiconsIcon
                    icon={ArrowLeft01Icon}
                    data-icon="inline-start"
                  />
                  {t("home")}
                </ButtonLink>
                <ButtonLink
                  href={localeHref(locale, "/changelog")}
                  variant="secondary"
                  className="min-h-12 px-6 text-sm"
                >
                  <HugeiconsIcon
                    icon={Download01Icon}
                    data-icon="inline-start"
                    strokeWidth={1.8}
                  />
                  {t("releases")}
                </ButtonLink>
              </div>

              <p className="mt-6 text-[12.5px] text-ink-3">
                {t("bug")}{" "}
                <a
                  href={`${SITE.github}/issues`}
                  target="_blank"
                  rel="noreferrer"
                  className="text-ink-2 underline underline-offset-4 transition-colors hover:text-ink"
                >
                  {t("report")}
                </a>
                .
              </p>
            </div>
          </Band>
        </Shell>
      </main>
      <SiteFooter locale={locale} />
    </>
  );
}
