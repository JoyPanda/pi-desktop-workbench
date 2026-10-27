import type { Metadata } from "next";
import Link from "next/link";

import { DocsPage } from "@/components/docs-page";
import { DOCS_NAV } from "@/lib/docs";
import { SITE } from "@/lib/site";

import { pageLocale } from "@/i18n/page";
import { localeAlternates } from "@/i18n/meta";
import { localeHref } from "@/i18n/links";
import { getT, type RichTags } from "@/i18n/t";
import { RICH } from "@/i18n/rich";

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/docs">): Promise<Metadata> {
  const locale = await pageLocale(params);
  const t = getT(locale, "docs.index");

  return {
    title: t("metaTitle"),
    description: t("description"),
    alternates: localeAlternates(locale, "/docs"),
  };
}

export default async function DocsOverviewPage({
  params,
}: PageProps<"/[locale]/docs">) {
  const locale = await pageLocale(params);
  const t = getT(locale, "docs.index");
  const nav = getT(locale, "docs");
  const tags: RichTags = {
    ...RICH,
    pi: (chunks) => (
      <a
        href="https://github.com/earendil-works/pi"
        target="_blank"
        rel="noreferrer"
      >
        {chunks}
      </a>
    ),
    github: (chunks) => (
      <a href={`${SITE.github}/issues`} target="_blank" rel="noreferrer">
        {chunks}
      </a>
    ),
  };

  return (
    <DocsPage
      locale={locale}
      current="/docs"
      title={t("title")}
      intro={t("intro")}
    >
      <h2>{t("headings.what")}</h2>
      <p>{t.rich("what.p1", tags)}</p>
      <p>{t.rich("what.p2", tags)}</p>

      <h2>{t("headings.how")}</h2>
      <pre>{t("how.diagram")}</pre>
      <p>{t("how.body")}</p>

      <h2>{t("headings.need")}</h2>
      <ul>
        <li>{t.rich("need.pi", tags)}</li>
        <li>{t.rich("need.node", tags)}</li>
        <li>{t.rich("need.git", tags)}</li>
      </ul>
      <p>{t("need.note")}</p>

      <h2>{t("headings.data")}</h2>
      <table>
        <thead>
          <tr>
            <th>{t("data.path")}</th>
            <th>{t("data.what")}</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>
              <code>~/.pi/agent/sessions/</code>
            </td>
            <td>{t.rich("data.sessions", tags)}</td>
          </tr>
          <tr>
            <td>
              <code>~/.pi/agent/</code>
            </td>
            <td>{t("data.config")}</td>
          </tr>
          <tr>
            <td>
              <code>~/.orbit-pi/</code>
            </td>
            <td>{t("data.orbit")}</td>
          </tr>
          <tr>
            <td>
              <code>&lt;project&gt;/.pi/</code>
            </td>
            <td>{t.rich("data.project", tags)}</td>
          </tr>
        </tbody>
      </table>

      <h2>{t("headings.guides")}</h2>
      {DOCS_NAV.map((group) => (
        <div key={group.id} className="mt-8">
          <h3 style={{ marginTop: 0 }}>{nav(`nav.groups.${group.id}`)}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            {group.links
              .filter((link) => link.href !== "/docs")
              .map((link) => (
                <Link
                  key={link.href}
                  href={localeHref(locale, link.href)}
                  className="group flex flex-col gap-1 rounded-[14px] border border-edge-subtle bg-surface-1 p-4 no-underline transition-colors hover:border-edge-strong hover:bg-surface-2"
                >
                  <span className="text-[14px] font-medium text-ink">
                    {nav(`nav.links.${link.id}`)}
                  </span>
                  <span className="text-[13px] leading-[1.6] text-ink-2">
                    {nav(`nav.links.${link.id}Description`)}
                  </span>
                </Link>
              ))}
          </div>
        </div>
      ))}

      <p className="mt-10 text-[13px] text-ink-3">{t.rich("missing", tags)}</p>
    </DocsPage>
  );
}
