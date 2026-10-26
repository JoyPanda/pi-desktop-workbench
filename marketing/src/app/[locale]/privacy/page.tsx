import type { Metadata } from "next";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

import { pageLocale } from "@/i18n/page";
import { localeAlternates } from "@/i18n/meta";
import { getT } from "@/i18n/t";
import { RICH } from "@/i18n/rich";

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/privacy">): Promise<Metadata> {
  const locale = await pageLocale(params);
  const t = getT(locale, "legal.privacy");

  return {
    title: t("title"),
    description: t("description"),
    alternates: localeAlternates(locale, "/privacy"),
  };
}

export default async function PrivacyPage({
  params,
}: PageProps<"/[locale]/privacy">) {
  const locale = await pageLocale(params);
  const t = getT(locale, "legal.privacy");

  return (
    <LegalPage
      locale={locale}
      path="/privacy"
      title={t("title")}
      updated={t("updated")}
      intro={t("intro")}
    >
      <h2>{t("headings.shortVersion")}</h2>
      <ul>
        <li>{t("shortVersion.telemetry")}</li>
        <li>{t("shortVersion.never")}</li>
        <li>{t("shortVersion.analytics")}</li>
        <li>{t("shortVersion.ads")}</li>
      </ul>

      <h2>{t("headings.desktopApp")}</h2>
      <p>{t("desktop.telemetry")}</p>
      <p>{t.rich("desktop.network", RICH)}</p>

      <h3>{t("headings.notCollected")}</h3>
      <ul>
        <li>{t("notCollected.source")}</li>
        <li>{t("notCollected.terminal")}</li>
        <li>{t("notCollected.ai")}</li>
        <li>{t("notCollected.credentials")}</li>
        <li>{t("notCollected.env")}</li>
        <li>{t("notCollected.ip")}</li>
        <li>{t("notCollected.machine")}</li>
      </ul>
      <p>{t("notCollected.note")}</p>

      <h3>{t("headings.localData")}</h3>
      <p>{t.rich("localData.body", RICH)}</p>

      <h2>{t("headings.website")}</h2>
      <p>
        {t.rich("website.analytics", {
          ...RICH,
          site: SITE.url.replace("https://", ""),
        })}
      </p>
      <p>{t("website.records")}</p>
      <ul>
        <li>{t("website.recordPath")}</li>
        <li>{t("website.recordBrowser")}</li>
        <li>{t.rich("website.recordId", RICH)}</li>
      </ul>
      <p>{t("website.note")}</p>

      <h2>{t("headings.thirdParty")}</h2>
      <ul>
        <li>{t.rich("thirdParty.downloads", RICH)}</li>
        <li>{t.rich("thirdParty.providers", RICH)}</li>
      </ul>

      <h2>{t("headings.retention")}</h2>
      <p>{t("retention.body")}</p>

      <h2>{t("headings.changes")}</h2>
      <p>{t("changes.body")}</p>

      <h2>{t("headings.contact")}</h2>
      <p>{t.rich("contact.body", { ...RICH, email: SITE.email })}</p>
    </LegalPage>
  );
}
