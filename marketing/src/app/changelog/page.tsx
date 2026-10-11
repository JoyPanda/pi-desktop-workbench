import type { Metadata } from "next";
import Link from "next/link";
import { Suspense } from "react";
import { Download01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { LinuxIcon, MacosIcon, WindowsIcon } from "@/components/brand-icons";

import { ChangelogFeed } from "@/components/changelog/feed";
import { ChangelogSkeleton } from "@/components/changelog/skeleton";
import { JsonLd } from "@/components/json-ld";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, ButtonLink, SectionLabel, Shell } from "@/components/ui";
import { formatDate, getChangelog } from "@/lib/changelog";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";
import { SITE } from "@/lib/site";

const DESCRIPTION =
  "Every Orbit Pi release, newest first — features, fixes, and downloads, pulled live from the repository.";

export const metadata: Metadata = {
  title: "Changelog",
  description: DESCRIPTION,
  keywords: [
    "Orbit changelog",
    "Orbit Pi releases",
    "Orbit updates",
    "release notes",
    "pi coding agent releases",
  ],
  alternates: { canonical: "/changelog" },
  openGraph: {
    type: "website",
    url: "/changelog",
    siteName: SITE.name,
    title: "Changelog · Orbit",
    description:
      "Every Orbit Pi release, newest first — pulled live from the repository.",
  },
  twitter: {
    card: "summary_large_image",
    title: "Changelog · Orbit",
    description:
      "Every Orbit Pi release, newest first — pulled live from the repository.",
  },
};

export default async function ChangelogPage() {
  const release = await getLatestRelease();
  const entries = await getChangelog();

  const jsonLd = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "BreadcrumbList",
        "@id": `${SITE.url}/changelog/#breadcrumb`,
        itemListElement: [
          { "@type": "ListItem", position: 1, name: "Home", item: SITE.url },
          {
            "@type": "ListItem",
            position: 2,
            name: "Changelog",
            item: `${SITE.url}/changelog`,
          },
        ],
      },
      {
        "@type": "CollectionPage",
        "@id": `${SITE.url}/changelog/#webpage`,
        url: `${SITE.url}/changelog`,
        name: "Changelog · Orbit",
        description: DESCRIPTION,
        isPartOf: { "@id": `${SITE.url}/#website` },
        breadcrumb: { "@id": `${SITE.url}/changelog/#breadcrumb` },
        mainEntity: {
          "@type": "ItemList",
          itemListElement: entries.map((entry, index) => ({
            "@type": "ListItem",
            position: index + 1,
            url: `${SITE.url}/changelog/${entry.tag}`,
            name: `Orbit Pi ${entry.tag}`,
          })),
        },
      },
    ],
  };

  const downloadHref = release?.macos ?? LATEST_RELEASE_URL;
  const platforms = [
    {
      icon: <MacosIcon className="size-3.5" />,
      label: "macOS",
      meta: ".dmg",
      href: release?.macos ?? LATEST_RELEASE_URL,
    },
    {
      icon: <WindowsIcon className="size-3.5" />,
      label: "Windows",
      meta: ".exe",
      href: release?.windows ?? LATEST_RELEASE_URL,
    },
    {
      icon: <LinuxIcon className="size-3.5" />,
      label: "Linux",
      meta: ".deb",
      href: release?.linux ?? LATEST_RELEASE_URL,
    },
  ];

  return (
    <>
      <JsonLd data={jsonLd} />
      <SiteNav />
      <main className="flex-1">
        <Shell>
          <Band dashed className="relative overflow-hidden py-16 sm:py-20">
            <span
              aria-hidden
              className="dither dither-tl left-0 top-0 h-[220px] w-[320px]"
            />
            <span
              aria-hidden
              className="dither dither-br bottom-0 right-0 h-[220px] w-[320px]"
            />

            <div className="relative z-[1]">
              <SectionLabel className="mb-5">Changelog</SectionLabel>
              <h1 className="max-w-[18ch] text-[clamp(32px,4.4vw,52px)] font-[250] leading-[1.08] tracking-[-0.02em] text-ink">
                Every release, as it shipped.
              </h1>
              <p className="mt-5 max-w-[54ch] text-[15.5px] leading-[1.7] text-ink-2">
                Orbit Pi moves fast. This page mirrors the repository&apos;s
                changelog and release feed, so the notes you read here are the
                ones that shipped.
              </p>

              <div className="mt-8 flex flex-wrap items-center gap-3.5">
                <ButtonLink href={downloadHref} variant="primary" external>
                  <HugeiconsIcon
                    icon={Download01Icon}
                    data-icon="inline-start"
                    strokeWidth={1.8}
                  />
                  {release ? `Download ${release.tag}` : "Download for macOS"}
                </ButtonLink>
              </div>

              <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                {platforms.map(({ icon, label, meta, href }) => (
                  <a
                    key={label}
                    href={href}
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center gap-1.5 no-underline transition-colors hover:text-ink"
                  >
                    {icon}
                    {label}
                    <span className="text-ink-3/70">{meta}</span>
                  </a>
                ))}
              </div>

              <dl className="mt-9 flex flex-wrap items-center gap-x-8 gap-y-3 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                <div className="flex items-center gap-2">
                  <dt>Latest</dt>
                  <dd className="text-ink-2">
                    {release ? (
                      <Link
                        href={`/changelog/${release.tag}`}
                        className="no-underline transition-colors hover:text-ink"
                      >
                        {release.tag} · {formatDate(release.publishedAt)}
                      </Link>
                    ) : (
                      "—"
                    )}
                  </dd>
                </div>
                <div className="flex items-center gap-2">
                  <dt>Source</dt>
                  <dd>
                    <a
                      href={SITE.github}
                      target="_blank"
                      rel="noreferrer"
                      className="no-underline transition-colors hover:text-ink"
                    >
                      {SITE.github.replace("https://", "")}
                    </a>
                  </dd>
                </div>
              </dl>
            </div>
          </Band>

          <Band id="releases" className="scroll-mt-20 py-14 sm:py-16">
            <Suspense fallback={<ChangelogSkeleton />}>
              <ChangelogFeed />
            </Suspense>
          </Band>
        </Shell>
      </main>
      <SiteFooter />
    </>
  );
}
