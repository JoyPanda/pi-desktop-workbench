import Link from "next/link";
import { GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitWordmark } from "@/components/brand";
import { Band, Shell } from "@/components/ui";
import { getLatestRelease } from "@/lib/releases";
import { SITE } from "@/lib/site";

const COLUMNS = [
  {
    title: "Product",
    links: [
      { label: "Agent", href: "/#agent", external: false },
      { label: "Sessions", href: "/#sessions", external: false },
      { label: "Models & providers", href: "/#providers", external: false },
      { label: "Git", href: "/#git", external: false },
      { label: "Usage", href: "/#usage", external: false },
      { label: "Reviews", href: "/#reviews", external: false },
      { label: "FAQ", href: "/#faq", external: false },
      { label: "Download", href: "/#install", external: false },
    ],
  },
  {
    title: "Resources",
    links: [
      { label: "Docs", href: "/docs", external: false },
      { label: "Install", href: "/docs/getting-started", external: false },
      { label: "Worktrees", href: "/docs/worktrees", external: false },
      { label: "MCP servers", href: "/docs/mcp", external: false },
      { label: "Shortcuts", href: "/docs/shortcuts", external: false },
      { label: "Changelog", href: "/changelog", external: false },
    ],
  },
  {
    title: "Elsewhere",
    links: [
      { label: "GitHub", href: SITE.github, external: true },
      {
        label: "pi coding agent",
        href: "https://github.com/earendil-works/pi",
        external: true,
      },
      {
        label: "Report an issue",
        href: `${SITE.github}/issues`,
        external: true,
      },
      { label: "Email", href: `mailto:${SITE.email}`, external: false },
    ],
  },
  {
    title: "Legal",
    links: [
      { label: "Privacy", href: "/privacy", external: false },
      { label: "Terms", href: "/terms", external: false },
      { label: "Security", href: "/security", external: false },
      { label: "Contact", href: "/contact", external: false },
    ],
  },
];

const PILL =
  "inline-flex items-center gap-2 rounded-[10px] border border-edge-subtle bg-surface-1 px-3 py-1.5 text-[12px] text-ink-2 no-underline transition-colors duration-[120ms] hover:border-edge-strong hover:bg-surface-2 hover:text-ink";

export async function SiteFooter() {
  const release = await getLatestRelease();
  const year = new Date().getFullYear();

  return (
    <footer>
      <Shell>
        <Band className="relative overflow-hidden pb-12 pt-14 sm:pt-16">
          <div className="relative z-[1] grid gap-12 md:grid-cols-[minmax(0,1.6fr)_repeat(4,minmax(0,1fr))] md:gap-8">
            <div className="md:pr-10">
              <Link
                href="/"
                aria-label="Orbit home"
                className="inline-flex items-center no-underline"
              >
                <OrbitWordmark className="h-[28px] w-auto" alt="" />
              </Link>
              <p className="mt-5 max-w-[36ch] text-[13.5px] leading-[1.75] text-ink-2">
                A native workbench for the pi coding agent. Your sessions, your
                models, your machine — chat, review, and commit in one window.
              </p>

              <div className="mt-6 flex flex-wrap items-center gap-2.5">
                <a
                  href={SITE.github}
                  target="_blank"
                  rel="noreferrer"
                  className={PILL}
                >
                  <HugeiconsIcon icon={GithubIcon} className="size-4" />
                  GitHub
                </a>
              </div>

              {release ? (
                <p className="mt-6 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                  Latest{" "}
                  <Link
                    href={`/changelog/${release.tag}`}
                    className="text-ink-2 no-underline transition-colors hover:text-ink"
                  >
                    {release.tag}
                  </Link>
                </p>
              ) : null}
            </div>

            <div className="grid grid-cols-2 gap-8 sm:grid-cols-4 md:contents">
              {COLUMNS.map((column) => (
                <nav key={column.title} aria-label={column.title}>
                  <p className="mb-4 font-mono text-[10.5px] uppercase tracking-[0.16em] text-ink-3">
                    {column.title}
                  </p>
                  <ul className="flex flex-col gap-3">
                    {column.links.map((link) => (
                      <li key={link.label}>
                        <a
                          href={link.href}
                          {...(link.external
                            ? { target: "_blank", rel: "noreferrer" }
                            : {})}
                          className="text-[13.5px] text-ink-2 no-underline transition-colors duration-[120ms] hover:text-ink"
                        >
                          {link.label}
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
          <p>© {year} Orbit · same sessions as the terminal</p>
          <p className="flex flex-wrap items-center gap-x-5 gap-y-2">
            {release ? (
              <Link
                href={`/changelog/${release.tag}`}
                className="no-underline transition-colors hover:text-ink"
              >
                Release {release.tag}
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
