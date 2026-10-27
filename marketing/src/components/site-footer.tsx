import Link from "next/link";
import { GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitWordmark } from "@/components/brand";
import { Band, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import { DEFAULT_LOCALE, type Locale } from "@/i18n/config";
import { localeHref } from "@/i18n/links";
import { getLatestRelease } from "@/lib/releases";
import { SITE } from "@/lib/site";

const COLUMNS = [
  {
    id: "product",
    links: [
      { id: "agent", href: "/#agent", external: false },
      { id: "sessions", href: "/#sessions", external: false },
      { id: "modelsProviders", href: "/#providers", external: false },
      { id: "git", href: "/#git", external: false },
      { id: "usage", href: "/#usage", external: false },
      { id: "reviews", href: "/#reviews", external: false },
      { id: "faq", href: "/#faq", external: false },
      { id: "download", href: "/#install", external: false },
    ],
  },
  {
    id: "resources",
    links: [
      { id: "docs", href: "/docs", external: false },
      { id: "install", href: "/docs/getting-started", external: false },
      { id: "worktrees", href: "/docs/worktrees", external: false },
      { id: "mcpServers", href: "/docs/mcp", external: false },
      { id: "shortcuts", href: "/docs/shortcuts", external: false },
      { id: "changelog", href: "/changelog", external: false },
    ],
  },
  {
    id: "elsewhere",
    links: [
      { id: "github", href: SITE.github, external: true },
      {
        id: "pi",
        href: "https://github.com/earendil-works/pi",
        external: true,
      },
      {
        id: "reportIssue",
        href: `${SITE.github}/issues`,
        external: true,
      },
      { id: "email", href: `mailto:${SITE.email}`, external: false },
    ],
  },
  {
    id: "legal",
    links: [
      { id: "privacy", href: "/privacy", external: false },
      { id: "terms", href: "/terms", external: false },
      { id: "security", href: "/security", external: false },
      { id: "contact", href: "/contact", external: false },
    ],
  },
] as const;

const PILL =
  "inline-flex items-center gap-2 rounded-[10px] border border-edge-subtle bg-surface-1 px-3 py-1.5 text-[12px] text-ink-2 no-underline transition-colors duration-[120ms] hover:border-edge-strong hover:bg-surface-2 hover:text-ink";

export async function SiteFooter({
  locale = DEFAULT_LOCALE,
}: {
  locale?: Locale;
} = {}) {
  const t = getT(locale, "footer");
  const release = await getLatestRelease();
  const year = new Date().getFullYear();

  return (
    <footer>
      <Shell>
        <Band className="relative overflow-hidden pb-12 pt-14 sm:pt-16">
          <div className="relative z-[1] grid gap-12 md:grid-cols-[minmax(0,1.6fr)_repeat(4,minmax(0,1fr))] md:gap-8">
            <div className="md:pr-10">
              <Link
                href={localeHref(locale, "/")}
                aria-label="Orbit home"
                className="inline-flex items-center no-underline"
              >
                <OrbitWordmark className="h-[28px] w-auto" alt="" />
              </Link>
              <p className="mt-5 max-w-[36ch] text-[13.5px] leading-[1.75] text-ink-2">
                {t("tagline")}
              </p>

              <div className="mt-6 flex flex-wrap items-center gap-2.5">
                <a
                  href={SITE.github}
                  target="_blank"
                  rel="noreferrer"
                  className={PILL}
                >
                  <HugeiconsIcon icon={GithubIcon} className="size-4" />
                  {t("links.github")}
                </a>
              </div>

              {release ? (
                <p className="mt-6 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                  {t("latest")}{" "}
                  <Link
                    href={localeHref(locale, `/changelog/${release.tag}`)}
                    className="text-ink-2 no-underline transition-colors hover:text-ink"
                  >
                    {release.tag}
                  </Link>
                </p>
              ) : null}
            </div>

            <div className="grid grid-cols-2 gap-8 sm:grid-cols-4 md:contents">
              {COLUMNS.map((column) => (
                <nav
                  key={column.id}
                  aria-label={t(`columns.${column.id}`)}
                >
                  <p className="mb-4 font-mono text-[10.5px] uppercase tracking-[0.16em] text-ink-3">
                    {t(`columns.${column.id}`)}
                  </p>
                  <ul className="flex flex-col gap-3">
                    {column.links.map((link) => (
                      <li key={link.id}>
                        <a
                          href={
                            link.external
                              ? link.href
                              : localeHref(locale, link.href)
                          }
                          {...(link.external
                            ? { target: "_blank", rel: "noreferrer" }
                            : {})}
                          className="text-[13.5px] text-ink-2 no-underline transition-colors duration-[120ms] hover:text-ink"
                        >
                          {t(`links.${link.id}`)}
                        </a>
                      </li>
                    ))}
                  </ul>
                </nav>
              ))}
            </div>
          </div>
        </Band>

        <Band className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-edge-subtle py-6 font-mono text-[11px] uppercase tracking-[0.1em] text-ink-3">
          <p>{t("copyright", { year })}</p>
          <p className="flex flex-wrap items-center gap-x-5 gap-y-2">
            {release ? (
              <Link
                href={localeHref(locale, `/changelog/${release.tag}`)}
                className="no-underline transition-colors hover:text-ink"
              >
                {t("release", { tag: release.tag })}
              </Link>
            ) : null}
            <a
              href={SITE.github}
              target="_blank"
              rel="noreferrer"
              className="no-underline transition-colors hover:text-ink"
            >
              {SITE.github.replace("https://", "")}
            </a>
          </p>
        </Band>
      </Shell>
    </footer>
  );
}
