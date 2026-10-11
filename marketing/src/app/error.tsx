"use client";

import { useEffect } from "react";
import Link from "next/link";
import { RefreshIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { OrbitWordmark } from "@/components/brand";
import { Band, ButtonLink, SectionLabel, Shell } from "@/components/ui";
import { Button } from "@/components/ui/button";

/**
 * Route-level error boundary. Catches runtime errors in the page segment and
 * offers a retry. Client Component by requirement.
 */
export default function Error({
  error,
  retry,
}: {
  error: Error & { digest?: string };
  retry: () => void;
}) {
  useEffect(() => {
    // Surface the error in the console; wire to a reporter if one is added.
    console.error(error);
  }, [error]);

  return (
    <main className="flex flex-1 flex-col">
      <Shell>
        <Band className="flex justify-center py-8">
          <Link
            href="/"
            aria-label="Orbit home"
            className="inline-flex items-center no-underline"
          >
            <OrbitWordmark className="h-[28px] w-auto" alt="" />
          </Link>
        </Band>
      </Shell>

      <Shell className="flex flex-1">
        <Band className="flex min-h-[58vh] flex-col items-center justify-center py-20 text-center">
          <SectionLabel>Error</SectionLabel>
          <h1 className="mt-5 text-balance text-[clamp(30px,4.4vw,46px)] font-semibold leading-[1.1] tracking-[-0.028em] text-ink">
            Something went wrong.
          </h1>
          <p className="mt-5 max-w-[46ch] text-[15px] leading-[1.75] text-ink-2">
            An unexpected error interrupted this page. Trying again usually
            clears it up.
          </p>

          <div className="mt-9 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center">
            <Button
              variant="default"
              size="lg"
              onClick={() => retry()}
              className="min-h-12 gap-2.5 rounded-[10px] px-6 text-sm font-semibold shadow-[0_1px_2px_rgba(0,0,0,0.18)] dark:border-white/20 light:border-black/10"
            >
              <HugeiconsIcon icon={RefreshIcon} data-icon="inline-start" />
              Try again
            </Button>
            <ButtonLink
              href="/"
              variant="secondary"
              className="min-h-12 px-6 text-sm"
            >
              Back home
            </ButtonLink>
          </div>

          {error.digest ? (
            <p className="mt-7 font-mono text-[10.5px] uppercase tracking-[0.12em] text-ink-3">
              Reference {error.digest}
            </p>
          ) : null}
        </Band>
      </Shell>
    </main>
  );
}
