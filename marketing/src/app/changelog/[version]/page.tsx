import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import {
  ArrowLeft01Icon,
  ArrowRight01Icon,
  GithubIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { ReleaseNotes } from "@/components/changelog/release-notes";
import { OsDownloadButton } from "@/components/download-button";
import { JsonLd } from "@/components/json-ld";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Badge } from "@/components/ui/badge";
import { Band, Shell } from "@/components/ui";
import { formatDate, getChangelog, getRelease } from "@/lib/changelog";
import { SITE } from "@/lib/site";

/** Every release page is known at build time; unknown versions 404. */
export const dynamicParams = false;

function releaseDescription(tag: string, date: string): string {
  return `${tag} (${formatDate(date)}). Orbit Pi release notes — features, fixes, and downloads.`;
}

export async function generateStaticParams() {
  const entries = await getChangelog();
  return entries.map((entry) => ({ version: entry.tag }));
}

export async function generateMetadata({
  params,
}: PageProps<"/changelog/[version]">): Promise<Metadata> {
  const { version } = await params;
  const entry = await getRelease(version);
  if (!entry) return {};

  const title = `Orbit Pi ${entry.tag}`;
  const description = releaseDescription(entry.tag, entry.date);

  return {
    title,
    description,
    alternates: { canonical: `/changelog/${entry.tag}` },
    openGraph: {
      type: "article",
      siteName: SITE.name,
      url: `/changelog/${entry.tag}`,
      title: `${title} · Orbit`,
      description,
      publishedTime: entry.date,
    },
    twitter: {
      card: "summary_large_image",
      title: `${title} · Orbit`,
      description,
    },
  };
}

export default async function ReleasePage({
  params,
}: PageProps<"/changelog/[version]">) {
  const { version } = await params;
  const entry = await getRelease(version);
  if (!entry) notFound();

  const entries = await getChangelog();
  const index = entries.findIndex((item) => item.version === entry.version);
  const newer = index > 0 ? entries[index - 1] : null;
  const older = index < entries.length - 1 ? entries[index + 1] : null;

  const url = `${SITE.url}/changelog/${entry.tag}`;
  const jsonLd = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "BreadcrumbList",
        "@id": `${url}/#breadcrumb`,
        itemListElement: [
          { "@type": "ListItem", position: 1, name: "Home", item: SITE.url },
          {
            "@type": "ListItem",
            position: 2,
            name: "Changelog",
            item: `${SITE.url}/changelog`,
          },
          {
            "@type": "ListItem",
            position: 3,
            name: `Orbit Pi ${entry.tag}`,
            item: url,
          },
        ],
      },
      {
        "@type": "TechArticle",
        "@id": `${url}/#article`,
        headline: `Orbit Pi ${entry.tag}`,
        description: releaseDescription(entry.tag, entry.date),
        ...(entry.date
          ? { datePublished: entry.date, dateModified: entry.date }
          : {}),
        mainEntityOfPage: { "@id": `${url}/#webpage` },
        author: { "@id": `${SITE.url}/#organization` },
        publisher: { "@id": `${SITE.url}/#organization` },
        about: { "@id": `${SITE.url}/#softwareapplication` },
        isPartOf: { "@id": `${SITE.url}/#website` },
        inLanguage: "en",
      },
      {
        "@type": "WebPage",
        "@id": `${url}/#webpage`,
        url,
        name: `Orbit Pi ${entry.tag} · Orbit`,
        isPartOf: { "@id": `${SITE.url}/#website` },
        breadcrumb: { "@id": `${url}/#breadcrumb` },
      },
    ],
  };

  return (
    <>
      <JsonLd data={jsonLd} />
      <SiteNav />
      <main className="flex-1">
        <Shell>
          <Band
            dashed
            className="relative overflow-hidden pb-10 pt-10 sm:pt-14"
          >
            <span
              aria-hidden
              className="dither dither-tr right-0 top-0 h-[200px] w-[300px]"
            />

            <div className="relative z-[1]">
              <Link
                href="/changelog"
                className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3 no-underline transition-colors hover:text-ink"
              >
                <HugeiconsIcon icon={ArrowLeft01Icon} className="size-3.5" />
                All releases
              </Link>

              <div className="mt-7 flex flex-wrap items-center gap-2.5">
                <span className="font-mono text-[12px] uppercase tracking-[0.14em] text-brand">
                  Release notes
                </span>
                {index === 0 ? (
                  <Badge className="bg-brand/12 text-brand">Latest</Badge>
                ) : null}
                {entry.prerelease ? (
                  <Badge variant="outline" className="text-ink-3">
                    Pre-release
                  </Badge>
                ) : null}
              </div>

              <h1 className="mt-4 text-[clamp(30px,4.2vw,46px)] font-[250] leading-[1.1] tracking-[-0.025em] text-ink">
                Orbit Pi {entry.tag}
              </h1>

              <div className="mt-5 flex flex-wrap items-center gap-x-5 gap-y-2 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                <time dateTime={entry.date}>{formatDate(entry.date)}</time>
                <a
                  href={entry.url}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1.5 no-underline transition-colors hover:text-ink"
                >
                  <HugeiconsIcon icon={GithubIcon} className="size-3.5" />
                  Release on GitHub
                </a>
              </div>

              <div className="mt-7">
                <OsDownloadButton
                  downloads={entry.downloads}
                  fallback={entry.url}
                  className="min-h-11 gap-[9px] px-5 text-[13px] sm:min-w-[196px]"
                />
              </div>
            </div>
          </Band>

          <Band className="py-12 sm:py-14">
            <div className="max-w-[48rem]">
              <ReleaseNotes groups={entry.groups} />
            </div>
          </Band>

          <Band className="flex flex-wrap items-center justify-between gap-4 border-t border-hair py-8">
            {older ? (
              <Link
                href={`/changelog/${older.tag}`}
                className="group inline-flex items-center gap-3 no-underline"
              >
                <HugeiconsIcon
                  icon={ArrowLeft01Icon}
                  className="size-4 text-ink-3 transition-transform group-hover:-translate-x-0.5"
                />
                <span className="flex flex-col">
                  <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
                    Previous
                  </span>
                  <span className="text-[13.5px] text-ink-2 transition-colors group-hover:text-ink">
                    {older.tag}
                  </span>
                </span>
              </Link>
            ) : (
              <span />
            )}

            {newer ? (
              <Link
                href={`/changelog/${newer.tag}`}
                className="group ml-auto inline-flex items-center gap-3 text-right no-underline"
              >
                <span className="flex flex-col">
                  <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
                    Next
                  </span>
                  <span className="text-[13.5px] text-ink-2 transition-colors group-hover:text-ink">
                    {newer.tag}
                  </span>
                </span>
                <HugeiconsIcon
                  icon={ArrowRight01Icon}
                  className="size-4 text-ink-3 transition-transform group-hover:translate-x-0.5"
                />
              </Link>
            ) : null}
          </Band>
        </Shell>
      </main>
      <SiteFooter />
    </>
  );
}
