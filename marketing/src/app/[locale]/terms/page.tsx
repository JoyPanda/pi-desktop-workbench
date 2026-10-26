import type { Metadata } from "next";
import Link from "next/link";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

import { pageLocale } from "@/i18n/page";
import { localeAlternates } from "@/i18n/meta";
import { localeHref } from "@/i18n/links";
import { getT, type RichTags } from "@/i18n/t";
import { RICH } from "@/i18n/rich";

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/terms">): Promise<Metadata> {
  const locale = await pageLocale(params);
  const t = getT(locale, "legal.terms");

  return {
    title: t("metaTitle"),
    description: t("description"),
    alternates: localeAlternates(locale, "/terms"),
  };
}

export default async function TermsPage({
  params,
}: PageProps<"/[locale]/terms">) {
  const locale = await pageLocale(params);
  const t = getT(locale, "legal.terms");
  const tags: RichTags = {
    ...RICH,
    license: (chunks) => (
      <a
        href={`${SITE.github}/blob/main/LICENSE`}
        target="_blank"
        rel="noreferrer"
      >
        {chunks}
      </a>
    ),
    privacy: (chunks) => (
      <Link href={localeHref(locale, "/privacy")}>{chunks}</Link>
    ),
  };

  return (
    <LegalPage
      locale={locale}
      path="/terms"
      title={t("title")}
      updated={t("updated")}
      intro={t("intro")}
    >
      <h2>{t("headings.overview")}</h2>
      <p>
        {t.rich("overview.body", {
          ...tags,
          site: SITE.url.replace("https://", ""),
        })}
      </p>

      <h2>{t("headings.software")}</h2>
      <p>{t.rich("software.body", tags)}</p>

      <h2>{t("headings.responsible")}</h2>
      <p>{t("responsible.body")}</p>
      <ul>
        <li>{t.rich("responsible.action", tags)}</li>
        <li>{t.rich("responsible.accessModes", tags)}</li>
        <li>{t.rich("responsible.backUp", tags)}</li>
      </ul>

      <h2>{t("headings.providers")}</h2>
      <p>{t("providers.body")}</p>

      <h2>{t("headings.website")}</h2>
      <p>{t.rich("website.body", tags)}</p>

      <h2>{t("headings.acceptableUse")}</h2>
      <p>{t("acceptableUse.body")}</p>
      <ul>
        <li>{t("acceptableUse.law")}</li>
        <li>{t("acceptableUse.access")}</li>
        <li>{t("acceptableUse.interfere")}</li>
      </ul>

      <h2>{t("headings.intellectualProperty")}</h2>
      <p>{t("intellectualProperty.body")}</p>

      <h2>{t("headings.disclaimer")}</h2>
      <p>{t.rich("disclaimer.body", tags)}</p>

      <h2>{t("headings.liability")}</h2>
      <p>{t("liability.body")}</p>

      <h2>{t("headings.changes")}</h2>
      <p>{t("changes.body")}</p>

      <h2>{t("headings.contact")}</h2>
      <p>{t.rich("contact.body", { ...tags, email: SITE.email })}</p>
    </LegalPage>
  );
}
