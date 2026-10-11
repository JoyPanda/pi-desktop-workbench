import Link from "next/link";
import type { ReactNode } from "react";
import { ArrowLeft01Icon, ArrowRight01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, SectionLabel, Shell } from "@/components/ui";
import { DOCS_NAV, docsNeighbours } from "@/lib/docs";
import { cn } from "@/lib/utils";

/**
 * Shared shell for every `/docs` page: a titled header band, a sticky sidebar
 * built from `DOCS_NAV`, one measured prose column, and a prev/next pager.
 * Content is authored as plain markup and styled by `.prose-orbit`.
 */
export function DocsPage({
  current,
  label = "Docs",
  title,
  intro,
  children,
}: {
  current: string;
  label?: string;
  title: string;
  intro?: string;
  children: ReactNode;
}) {
  const { prev, next } = docsNeighbours(current);

  return (
    <>
      <SiteNav />
      <main className="flex-1">
        <Shell>
          <Band dashed className="relative overflow-hidden py-14 sm:py-16">
            <span
              aria-hidden
              className="dither dither-tl left-0 top-0 h-[180px] w-[280px]"
            />
            <div className="relative z-[1]">
              <SectionLabel className="mb-5">{label}</SectionLabel>
              <h1 className="max-w-[22ch] text-[clamp(30px,4vw,46px)] font-[250] leading-[1.1] tracking-[-0.02em] text-ink">
                {title}
              </h1>
              {intro ? (
                <p className="mt-5 max-w-[58ch] text-[15.5px] leading-[1.7] text-ink-2">
                  {intro}
                </p>
              ) : null}
            </div>
          </Band>

          <Band className="py-12 sm:py-14">
            <div className="grid grid-cols-1 gap-10 lg:grid-cols-[210px_minmax(0,1fr)] lg:gap-16">
              <DocsSidebar current={current} />
              <div className="min-w-0">
                <article className="prose-orbit max-w-[72ch]">
                  {children}
                </article>
                <DocsPager prev={prev} next={next} />
              </div>
            </div>
          </Band>
        </Shell>
      </main>
      <SiteFooter />
    </>
  );
}

function DocsSidebar({ current }: { current: string }) {
  return (
    <nav aria-label="Documentation" className="lg:sticky lg:top-24 lg:self-start">
      {/* Below `lg` the full list scrolls horizontally above the article. */}
      <ul className="-mx-1 flex gap-2 overflow-x-auto px-1 pb-3 lg:hidden">
        {DOCS_NAV.flatMap((group) => group.links).map((link) => (
          <li key={link.href} className="shrink-0">
            <Link
              href={link.href}
              aria-current={link.href === current ? "page" : undefined}
              className={cn(
                "inline-flex rounded-full border px-3 py-1.5 text-[12.5px] no-underline transition-colors",
                link.href === current
                  ? "border-edge-strong bg-surface-2 text-ink"
                  : "border-edge-subtle bg-surface-1 text-ink-2 hover:border-edge-strong hover:text-ink",
              )}
            >
              {link.label}
            </Link>
          </li>
        ))}
      </ul>

      <div className="hidden flex-col gap-8 lg:flex">
        {DOCS_NAV.map((group) => (
          <div key={group.title}>
            <p className="mb-3 font-mono text-[10.5px] uppercase tracking-[0.16em] text-ink-3">
              {group.title}
            </p>
            <ul className="flex flex-col gap-0.5">
              {group.links.map((link) => {
                const active = link.href === current;
                return (
                  <li key={link.href}>
                    <Link
                      href={link.href}
                      aria-current={active ? "page" : undefined}
                      className={cn(
                        "-ml-px block border-l py-1.5 pl-3.5 text-[13.5px] no-underline transition-colors",
                        active
                          ? "border-brand font-medium text-ink"
                          : "border-transparent text-ink-2 hover:border-edge-strong hover:text-ink",
                      )}
                    >
                      {link.label}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </div>
    </nav>
  );
}

function DocsPager({
  prev,
  next,
}: {
  prev?: { label: string; href: string };
  next?: { label: string; href: string };
}) {
  if (!prev && !next) return null;

  return (
    <div className="mt-14 grid gap-3 border-t border-edge-subtle pt-8 sm:grid-cols-2">
      {prev ? (
        <Link
          href={prev.href}
          className="group flex flex-col gap-1 rounded-[14px] border border-edge-subtle bg-surface-1 p-4 no-underline transition-colors hover:border-edge-strong hover:bg-surface-2"
        >
          <span className="inline-flex items-center gap-1.5 font-mono text-[10.5px] uppercase tracking-[0.14em] text-ink-3">
            <HugeiconsIcon icon={ArrowLeft01Icon} className="size-3.5" />
            Previous
          </span>
          <span className="text-[14px] text-ink-2 transition-colors group-hover:text-ink">
            {prev.label}
          </span>
        </Link>
      ) : (
        <span />
      )}
      {next ? (
        <Link
          href={next.href}
          className="group flex flex-col items-end gap-1 rounded-[14px] border border-edge-subtle bg-surface-1 p-4 text-right no-underline transition-colors hover:border-edge-strong hover:bg-surface-2 sm:col-start-2"
        >
          <span className="inline-flex items-center gap-1.5 font-mono text-[10.5px] uppercase tracking-[0.14em] text-ink-3">
            Next
            <HugeiconsIcon icon={ArrowRight01Icon} className="size-3.5" />
          </span>
          <span className="text-[14px] text-ink-2 transition-colors group-hover:text-ink">
            {next.label}
          </span>
        </Link>
      ) : null}
    </div>
  );
}

/** A quiet aside for notes and warnings inside a docs article. */
export function Callout({
  children,
  tone = "note",
}: {
  children: ReactNode;
  tone?: "note" | "warn";
}) {
  return (
    <aside
      className={cn(
        "my-6 rounded-[12px] border px-4 py-3 text-[13.5px] leading-[1.7]",
        tone === "warn"
          ? "border-[color-mix(in_oklab,var(--gold)_35%,transparent)] bg-[color-mix(in_oklab,var(--gold)_8%,transparent)] text-ink-2"
          : "border-edge-subtle bg-surface-1 text-ink-2",
      )}
    >
      {children}
    </aside>
  );
}
