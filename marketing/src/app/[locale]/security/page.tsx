import type { Metadata } from "next";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

import { pageLocale } from "@/i18n/page";
import { localeAlternates } from "@/i18n/meta";
import { getT, type RichTags } from "@/i18n/t";
import { RICH } from "@/i18n/rich";

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/security">): Promise<Metadata> {
  const locale = await pageLocale(params);
  const t = getT(locale, "legal.security");

  return {
    title: t("title"),
    description: t("description"),
    alternates: localeAlternates(locale, "/security"),
  };
}

export default async function SecurityPage({
  params,
}: PageProps<"/[locale]/security">) {
  const locale = await pageLocale(params);
  const t = getT(locale, "legal.security");
  const tags: RichTags = {
    ...RICH,
    reporting: (chunks) => (
      <a
        href={`${SITE.github}/security/advisories/new`}
        target="_blank"
        rel="noreferrer"
      >
        {chunks}
      </a>
    ),
  };

  return (
    <LegalPage
      locale={locale}
      label={t("label")}
      path="/security"
      title={t("title")}
      updated={t("updated")}
      intro={t("intro")}
    >
      <h2>{t("headings.supported")}</h2>
      <p>{t.rich("supported.body", tags)}</p>
      <ul>
        <li>{t.rich("supported.main", tags)}</li>
        <li>{t.rich("supported.tagged", tags)}</li>
        <li>{t.rich("supported.older", tags)}</li>
      </ul>

      <h2>{t("headings.reporting")}</h2>
      <p>{t.rich("reporting.body", { ...tags, email: SITE.email })}</p>

      <h3>{t("headings.include")}</h3>
      <ul>
        <li>{t("include.issue")}</li>
        <li>{t("include.steps")}</li>
        <li>{t.rich("include.version", tags)}</li>
        <li>{t.rich("include.pi", tags)}</li>
      </ul>

      <h3>{t("headings.expect")}</h3>
      <p>{t("expect.body")}</p>

      <h2>{t("headings.scope")}</h2>
      <p>{t.rich("scope.body", tags)}</p>
      <ul>
        <li>{t.rich("scope.access", tags)}</li>
        <li>{t.rich("scope.accessModes", tags)}</li>
        <li>{t.rich("scope.store", tags)}</li>
      </ul>
      <p>{t("scope.wanted")}</p>
      <ul>
        <li>{t("scope.memory")}</li>
        <li>{t("scope.injection")}</li>
        <li>{t("scope.credentials")}</li>
        <li>{t("scope.updater")}</li>
      </ul>
    </LegalPage>
  );
}
