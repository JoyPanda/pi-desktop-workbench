import type { Metadata } from "next";
import Link from "next/link";

import { Callout, DocsPage } from "@/components/docs-page";
import { SITE } from "@/lib/site";

import { pageLocale } from "@/i18n/page";
import { localeAlternates } from "@/i18n/meta";
import { localeHref } from "@/i18n/links";
import { getT, type RichTags } from "@/i18n/t";
import { RICH } from "@/i18n/rich";

const RELEASES = `${SITE.github}/releases/latest`;

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/docs/getting-started">): Promise<Metadata> {
  const locale = await pageLocale(params);
  const t = getT(locale, "docs.install");

  return {
    title: t("metaTitle"),
    description: t("description"),
    alternates: localeAlternates(locale, "/docs/getting-started"),
  };
}

export default async function GettingStartedPage({
  params,
}: PageProps<"/[locale]/docs/getting-started">) {
  const locale = await pageLocale(params);
  const t = getT(locale, "docs.install");
  const tags: RichTags = {
    ...RICH,
    release: (chunks) => (
      <a href={RELEASES} target="_blank" rel="noreferrer">
        {chunks}
      </a>
    ),
    sessions: (chunks) => (
      <Link href={localeHref(locale, "/docs/sessions")}>{chunks}</Link>
    ),
    shortcuts: (chunks) => (
      <Link href={localeHref(locale, "/docs/shortcuts")}>{chunks}</Link>
    ),
  };

  return (
    <DocsPage
      locale={locale}
      current="/docs/getting-started"
      title={t("title")}
      intro={t("intro")}
    >
      <h2>{t("headings.download")}</h2>
      <p>{t.rich("download.body", tags)}</p>
      <table>
        <thead>
          <tr>
            <th>{t("download.platform")}</th>
            <th>{t("download.formats")}</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>macOS</td>
            <td>{t.rich("download.macos", tags)}</td>
          </tr>
          <tr>
            <td>Windows</td>
            <td>{t.rich("download.windows", tags)}</td>
          </tr>
          <tr>
            <td>Linux</td>
            <td>{t.rich("download.linux", tags)}</td>
          </tr>
        </tbody>
      </table>

      <Callout>{t.rich("callout", tags)}</Callout>

      <h2>{t("headings.requirements")}</h2>
      <p>{t("requirements.body")}</p>
      <ul>
        <li>{t.rich("requirements.pi", tags)}</li>
        <li>{t.rich("requirements.node", tags)}</li>
        <li>{t.rich("requirements.git", tags)}</li>
      </ul>

      <h2>{t("headings.firstLaunch")}</h2>
      <ol>
        <li>{t.rich("firstLaunch.open", tags)}</li>
        <li>{t.rich("firstLaunch.provider", tags)}</li>
        <li>{t.rich("firstLaunch.workspace", tags)}</li>
        <li>{t.rich("firstLaunch.model", tags)}</li>
        <li>{t.rich("firstLaunch.accessMode", tags)}</li>
        <li>{t.rich("firstLaunch.send", tags)}</li>
      </ol>

      <h2>{t("headings.updates")}</h2>
      <p>{t.rich("updates.body", tags)}</p>

      <h2>{t("headings.next")}</h2>
      <p>{t.rich("next.body", tags)}</p>
    </DocsPage>
  );
}
