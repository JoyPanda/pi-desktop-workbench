import Link from "next/link";
import { Download01Icon, GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { OrbitIcon } from "@/components/brand";
import { OsDownloadButton } from "@/components/download-button";
import { Band, ButtonLink, Shell } from "@/components/ui";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";
import { NO_DOWNLOADS, preferredDownload } from "@/lib/downloads";
import { getT } from "@/i18n/t";
import { downloadLabels } from "@/i18n/labels";
import type { Locale } from "@/i18n/config";

export async function Closer({ locale }: { locale: Locale }) {
  const t = getT(locale, "closer");
  const release = await getLatestRelease();
  const downloads = release?.downloads ?? NO_DOWNLOADS;
  const platforms = [
    {
      label: t("platforms.macos"),
      href: preferredDownload(downloads, "macos")?.url ?? LATEST_RELEASE_URL,
    },
    {
      label: t("platforms.windows"),
      href: preferredDownload(downloads, "windows")?.url ?? LATEST_RELEASE_URL,
    },
    {
      label: t("platforms.linux"),
      href: preferredDownload(downloads, "linux")?.url ?? LATEST_RELEASE_URL,
    },
  ];

  return (
    <Shell>
      <Band id="install" className="flex scroll-mt-20 flex-col items-center py-24 text-center sm:py-32">
        {/* The mark, held in a quiet pocket of ambient light. */}
        <div className="relative mb-8">
          <div
            aria-hidden
            className="pointer-events-none absolute left-1/2 top-1/2 h-[180px] w-[260px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(91,157,255,0.08),transparent)]"
          />
          <OrbitIcon
            alt=""
            className="relative h-12 w-12 rounded-[12px] shadow-card"
          />
        </div>

        <h2 className="max-w-[22ch] text-balance text-[clamp(30px,3.6vw,42px)] font-semibold leading-[1.14] tracking-[-0.026em] text-ink">
          {t("heading")}
        </h2>
        <p className="mt-4 max-w-[48ch] text-[15px] leading-[1.7] text-ink-2">
          {t("body")}
        </p>

        <div className="mt-9 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center">
          <OsDownloadButton
            downloads={downloads}
            fallback={LATEST_RELEASE_URL}
            className="min-h-12 px-6 text-sm sm:min-w-[208px]"
            labels={downloadLabels(locale)}
          />
          <ButtonLink
            href="https://github.com/imrj05/orbit"
            variant="secondary"
            className="min-h-12 px-6 text-sm"
            external
          >
            <HugeiconsIcon icon={GithubIcon} data-icon="inline-start" />
            {t("github")}
          </ButtonLink>
        </div>

        <div className="mt-6 flex flex-wrap items-center justify-center gap-x-5 gap-y-2 text-[12px] text-ink-3">
          {platforms.map(({ label, href }) => (
            <a
              key={label}
              href={href}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 no-underline transition-colors hover:text-ink-2"
            >
              <HugeiconsIcon
                icon={Download01Icon}
                className="size-3.5"
                strokeWidth={1.8}
              />
              {label}
            </a>
          ))}
          {release ? (
            <Link
              href={`/changelog/${release.tag}`}
              className="no-underline transition-colors hover:text-ink-2"
            >
              {t("releaseNotes", { tag: release.tag })}
            </Link>
          ) : null}
        </div>
      </Band>
    </Shell>
  );
}
