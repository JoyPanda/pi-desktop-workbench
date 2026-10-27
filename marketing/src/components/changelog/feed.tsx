import Link from "next/link";
import { GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { CopyLinkButton } from "@/components/changelog/copy-link-button";
import { ReleaseNotes } from "@/components/changelog/release-notes";
import { VersionIndex } from "@/components/changelog/version-index";
import { Badge } from "@/components/ui/badge";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Eyebrow } from "@/components/ui";
import {
  formatDate,
  getChangelog,
  RELEASES_PAGE,
  type ChangelogEntry,
} from "@/lib/changelog";
import { SITE } from "@/lib/site";
import { cn } from "@/lib/utils";
import { getT } from "@/i18n/t";
import { localeHref } from "@/i18n/links";
import type { Locale } from "@/i18n/config";

function ReleaseHeader({
  locale,
  entry,
  latest = false,
  featured = false,
}: {
  locale: Locale;
  entry: ChangelogEntry;
  latest?: boolean;
  featured?: boolean;
}) {
  const t = getT(locale, "changelog");

  return (
    <header className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2.5">
      <div className="flex min-w-0 flex-wrap items-center gap-2.5">
        <Link
          href={localeHref(locale, `/changelog/${entry.tag}`)}
          className={cn(
            "font-mono tracking-[-0.01em] text-ink no-underline transition-colors hover:text-brand",
            featured ? "text-[17px]" : "text-[15px]",
          )}
        >
          {entry.tag}
        </Link>
        {latest ? (
          <Badge className="bg-brand/12 text-brand">{t("latest")}</Badge>
        ) : null}
        {entry.prerelease ? (
          <Badge variant="outline" className="text-ink-3">
            {t("prerelease")}
          </Badge>
        ) : null}
        <CopyLinkButton
          path={localeHref(locale, `/changelog/${entry.tag}`)}
          label={t("copyLink", { tag: entry.tag })}
        />
      </div>
      <time
        dateTime={entry.date}
        className="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3"
      >
        {formatDate(entry.date)}
      </time>
    </header>
  );
}

/**
 * Streams the release feed in behind the page's static header: the newest
 * release as a featured panel, the rest as a dated timeline, with a
 * scroll-spy index beside them.
 */
export async function ChangelogFeed({ locale }: { locale: Locale }) {
  const t = getT(locale, "changelog");
  const entries = await getChangelog();

  if (entries.length === 0) {
    return (
      <Empty className="border border-hair py-16">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <HugeiconsIcon icon={GithubIcon} className="size-4" />
          </EmptyMedia>
          <EmptyTitle>{t("unavailableTitle")}</EmptyTitle>
          <EmptyDescription>
            {t.rich("unavailableBody", {
              link: (chunks) => (
                <a href={RELEASES_PAGE} target="_blank" rel="noreferrer">
                  {chunks}
                </a>
              ),
            })}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  const [latest, ...rest] = entries;
  const index = entries.map((entry) => ({
    version: entry.version,
    tag: entry.tag,
    dateLabel: formatDate(entry.date),
  }));

  return (
    <div className="grid gap-12 lg:grid-cols-[200px_minmax(0,1fr)] lg:gap-16">
      <aside className="hidden lg:block">
        <div className="sticky top-24">
          <div className="mb-4 flex items-baseline justify-between gap-2 px-3">
            <Eyebrow>{t("releases")}</Eyebrow>
            <span className="font-mono text-[10.5px] text-ink-3">
              {entries.length}
            </span>
          </div>
          <VersionIndex entries={index} label={t("jumpToRelease")} />

          <div className="mt-5 border-t border-hair pt-4 pl-3 text-[12.5px]">
            <a
              href={SITE.github}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 text-ink-2 no-underline transition-colors hover:text-ink"
            >
              <HugeiconsIcon icon={GithubIcon} className="size-3.5" />
              GitHub
            </a>
          </div>
        </div>
      </aside>

      <div className="min-w-0 max-w-[48rem]">
        <article id={`v${latest.version}`} className="group scroll-mt-24">
          <div className="relative overflow-hidden rounded-2xl bg-brand/[0.035] p-6 ring-1 ring-brand/15 sm:p-7">
            <span
              aria-hidden
              className="absolute inset-x-0 top-0 h-px bg-linear-to-r from-transparent via-brand/45 to-transparent"
            />
            <ReleaseHeader locale={locale} entry={latest} latest featured />
            <ReleaseNotes groups={latest.groups} className="mt-6" />
          </div>
        </article>

        <div className="mt-8 flex flex-col divide-y divide-hair">
          {rest.map((entry) => (
            <article
              key={entry.version}
              id={`v${entry.version}`}
              className="group scroll-mt-24 py-10 first:pt-0"
            >
              <ReleaseHeader locale={locale} entry={entry} />
              <ReleaseNotes groups={entry.groups} className="mt-6" />
            </article>
          ))}
        </div>
      </div>
    </div>
  );
}
