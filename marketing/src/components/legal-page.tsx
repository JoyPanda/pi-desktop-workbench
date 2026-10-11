import type { ReactNode } from "react";

import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, SectionLabel, Shell } from "@/components/ui";

/**
 * Shared shell for legal and policy pages: a titled header band and a single
 * measured prose column. Content is authored as plain markup and styled by
 * `.prose-orbit` in globals.css.
 */
export function LegalPage({
  label,
  title,
  updated,
  intro,
  children,
}: {
  label: string;
  title: string;
  updated: string;
  intro?: string;
  children: ReactNode;
}) {
  return (
    <>
      <SiteNav />
      <main className="flex-1">
        <Shell>
          <Band dashed className="relative overflow-hidden py-16 sm:py-20">
            <div className="relative z-[1]">
              <SectionLabel className="mb-5">{label}</SectionLabel>
              <h1 className="max-w-[20ch] text-[clamp(32px,4.4vw,52px)] font-[250] leading-[1.08] tracking-[-0.02em] text-ink">
                {title}
              </h1>
              {intro ? (
                <p className="mt-5 max-w-[56ch] text-[15.5px] leading-[1.7] text-ink-2">
                  {intro}
                </p>
              ) : null}
              <p className="mt-6 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                Last updated {updated}
              </p>
            </div>
          </Band>

          <Band className="py-14 sm:py-16">
            <article className="prose-orbit max-w-[74ch]">{children}</article>
          </Band>
        </Shell>
      </main>
      <SiteFooter />
    </>
  );
}
