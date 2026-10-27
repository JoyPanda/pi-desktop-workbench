import Link from "next/link";
import { GithubIcon, StarIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitWordmark } from "@/components/brand";
import { OsDownloadButton } from "@/components/download-button";
import { LanguageSwitcher } from "@/components/language-switcher";
import { ThemeToggle } from "@/components/theme-toggle";
import { Band, Shell } from "@/components/ui";
import {
  getLatestRelease,
  getRepoStars,
  LATEST_RELEASE_URL,
} from "@/lib/releases";
import { NO_DOWNLOADS } from "@/lib/downloads";
import { getT } from "@/i18n/t";
import { DEFAULT_LOCALE, type Locale } from "@/i18n/config";
import { downloadLabels } from "@/i18n/labels";
import { localeHref } from "@/i18n/links";

const LINKS = [
  { id: "agent", href: "/#agent" },
  { id: "sessions", href: "/#sessions" },
  { id: "git", href: "/#git" },
  { id: "usage", href: "/#usage" },
  { id: "skills", href: "/#skills" },
  { id: "mcp", href: "/#mcp" },
  { id: "docs", href: "/docs" },
  { id: "changelog", href: "/changelog" },
] as const;

/** 13 → "13", 1234 → "1.2k", 123456 → "123k". */
function formatStars(count: number): string {
  if (count < 1000) return String(count);
  const thousands = count / 1000;
  const rounded =
    thousands >= 100 ? Math.round(thousands) : Number(thousands.toFixed(1));
  return `${rounded}k`;
}

export async function SiteNav({
  locale = DEFAULT_LOCALE,
  path = "/",
}: {
  locale?: Locale;
  /** The page this nav sits on, without a locale prefix. */
  path?: string;
} = {}) {
  const t = getT(locale, "nav");
  const switcher = getT(locale, "switcher");
  const labels = downloadLabels(locale);
  const [release, stars] = await Promise.all([
    getLatestRelease(),
    getRepoStars(),
  ]);
  const downloads = release?.downloads ?? NO_DOWNLOADS;

  return (
    <header className="sticky top-0 z-50 bg-page/80 backdrop-blur-md">
      <Shell>
        <Band dashed>
          <nav className="nav-in flex h-16 items-center justify-between">
            <Link
              href={localeHref(locale, "/")}
              aria-label={t("home")}
              className="inline-flex items-center no-underline"
            >
              <OrbitWordmark className="h-[30px] w-auto" alt="" priority />
            </Link>

            <div className="hidden items-center gap-[24px] text-[13px] md:flex">
              {LINKS.map((l) => (
                <a
                  key={l.href}
                  href={localeHref(locale, l.href)}
                  className="text-ink-2 no-underline transition-colors duration-[120ms] hover:text-ink"
                >
                  {t(`links.${l.id}`)}
                </a>
              ))}

              <a
                href="https://github.com/imrj05/orbit"
                target="_blank"
                rel="noreferrer"
                aria-label={
                  stars != null
                    ? t("stars", { count: stars })
                    : t("github")
                }
                className="group inline-flex items-center gap-2 rounded-[10px] border border-edge-subtle bg-surface-1 py-[5px] pl-2.5 pr-3 text-[12.5px] text-ink-2 no-underline transition-colors duration-[120ms] hover:border-edge-strong hover:bg-surface-2 hover:text-ink"
              >
                <HugeiconsIcon icon={GithubIcon} className="size-4" />
                {stars != null ? (
                  <span className="inline-flex items-center gap-1.5">
                    <HugeiconsIcon
                      icon={StarIcon}
                      fill="currentColor"
                      className="size-3.5 text-gold transition-transform duration-200 group-hover:scale-110"
                    />
                    <span className="font-mono text-[11.5px] tabular-nums text-ink">
                      {formatStars(stars)}
                    </span>
                  </span>
                ) : (
                  <span>{t("github")}</span>
                )}
              </a>

              <LanguageSwitcher locale={locale} label={switcher("label")} path={path} />

              <ThemeToggle />

              <OsDownloadButton
                downloads={downloads}
                fallback={LATEST_RELEASE_URL}
                variant="nav"
                short
                menu={false}
                labels={labels}
              />
            </div>

            <div className="flex items-center gap-2.5 md:hidden">
              <LanguageSwitcher locale={locale} label={switcher("label")} path={path} />
              <ThemeToggle />
              <OsDownloadButton
                downloads={downloads}
                fallback={LATEST_RELEASE_URL}
                variant="nav"
                short
                menu={false}
                labels={labels}
              />
            </div>
          </nav>
        </Band>
      </Shell>
    </header>
  );
}
