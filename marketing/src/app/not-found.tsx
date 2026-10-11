import type { Metadata } from "next";
import { ArrowLeft01Icon, Download01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitField } from "@/components/orbit-field";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, ButtonLink, SectionLabel, Shell } from "@/components/ui";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Page not found",
  description: "That page drifted off course. Head back to Orbit.",
};

export default function NotFound() {
  return (
    <>
      <SiteNav />
      <main className="flex-1">
        <Shell>
          <Band className="relative overflow-hidden py-24 text-center sm:py-32">
            <OrbitField />
            <div className="relative z-[1] mx-auto flex max-w-[620px] flex-col items-center">
              <SectionLabel>Error 404</SectionLabel>
              <h1 className="mt-5 text-balance text-[clamp(34px,5vw,54px)] font-semibold leading-[1.08] tracking-[-0.03em] text-ink">
                This page drifted off course.
              </h1>
              <p className="mt-5 max-w-[48ch] text-[15px] leading-[1.75] text-ink-2">
                The page you were looking for doesn&apos;t exist, moved, or was
                never in this orbit. Let&apos;s get you back on track.
              </p>

              <div className="mt-9 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center">
                <ButtonLink
                  href="/"
                  className="min-h-12 px-6 text-sm sm:min-w-[184px]"
                >
                  <HugeiconsIcon
                    icon={ArrowLeft01Icon}
                    data-icon="inline-start"
                  />
                  Back home
                </ButtonLink>
                <ButtonLink
                  href="/changelog"
                  variant="secondary"
                  className="min-h-12 px-6 text-sm"
                >
                  <HugeiconsIcon
                    icon={Download01Icon}
                    data-icon="inline-start"
                    strokeWidth={1.8}
                  />
                  Browse releases
                </ButtonLink>
              </div>

              <p className="mt-6 text-[12.5px] text-ink-3">
                Think this is a bug?{" "}
                <a
                  href={`${SITE.github}/issues`}
                  target="_blank"
                  rel="noreferrer"
                  className="text-ink-2 underline underline-offset-4 transition-colors hover:text-ink"
                >
                  Report an issue
                </a>
                .
              </p>
            </div>
          </Band>
        </Shell>
      </main>
      <SiteFooter />
    </>
  );
}
