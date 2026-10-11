import Link from "next/link";
import { GithubIcon, StarIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitWordmark } from "@/components/brand";
import { OsDownloadButton } from "@/components/download-button";
import { ThemeToggle } from "@/components/theme-toggle";
import { Band, Shell } from "@/components/ui";
import {
  getLatestRelease,
  getRepoStars,
  LATEST_RELEASE_URL,
} from "@/lib/releases";

const links = [
  { label: "Agent", href: "/#agent" },
  { label: "Sessions", href: "/#sessions" },
  { label: "Git", href: "/#git" },
  { label: "Usage", href: "/#usage" },
  { label: "Skills", href: "/#skills" },
  { label: "MCP", href: "/#mcp" },
  { label: "Docs", href: "/docs" },
  { label: "Changelog", href: "/changelog" },
];

/** 13 → "13", 1234 → "1.2k", 123456 → "123k". */
function formatStars(count: number): string {
  if (count < 1000) return String(count);
  const thousands = count / 1000;
  const rounded =
    thousands >= 100 ? Math.round(thousands) : Number(thousands.toFixed(1));
  return `${rounded}k`;
}

export async function SiteNav() {
  const [release, stars] = await Promise.all([
    getLatestRelease(),
    getRepoStars(),
  ]);
  const assets = {
    macos: release?.macos,
    windows: release?.windows,
    linux: release?.linux,
  };

  return (
    <header className="sticky top-0 z-50 bg-page/80 backdrop-blur-md">
      <Shell>
        <Band dashed>
          <nav className="nav-in flex h-16 items-center justify-between">
            <Link
              href="/"
              aria-label="Orbit home"
              className="inline-flex items-center no-underline"
            >
              <OrbitWordmark className="h-[30px] w-auto" alt="" priority />
            </Link>

            <div className="hidden items-center gap-[24px] text-[13px] md:flex">
              {links.map((l) => (
                <a
                  key={l.href}
                  href={l.href}
                  className="text-ink-2 no-underline transition-colors duration-[120ms] hover:text-ink"
                >
                  {l.label}
                </a>
              ))}

              <a
                href="https://github.com/imrj05/orbit"
                target="_blank"
                rel="noreferrer"
                aria-label={
                  stars != null ? `GitHub — ${stars} stars` : "GitHub"
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
                  <span>GitHub</span>
                )}
              </a>

              <ThemeToggle />

              <OsDownloadButton
                assets={assets}
                fallback={LATEST_RELEASE_URL}
                variant="nav"
                short
              />
            </div>

            <div className="flex items-center gap-2.5 md:hidden">
              <ThemeToggle />
              <OsDownloadButton
                assets={assets}
                fallback={LATEST_RELEASE_URL}
                variant="nav"
                short
              />
            </div>
          </nav>
        </Band>
      </Shell>
    </header>
  );
}
