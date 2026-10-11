"use client";

import { useEffect } from "react";
import { RefreshIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { Button } from "@/components/ui/button";
import { FONT_CLASSES } from "@/lib/fonts";
import { THEME_SCRIPT } from "@/lib/theme";
import "./globals.css";

/**
 * Global error boundary. Replaces the root layout when it (or a dependency it
 * renders) throws, so it must define its own <html>/<body>, fonts, and global
 * styles — none of which the root layout can provide here.
 */
export default function GlobalError({
  error,
  retry,
}: {
  error: Error & { digest?: string };
  retry: () => void;
}) {
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <html lang="en" className={`dark ${FONT_CLASSES}`} suppressHydrationWarning>
      <head>
        <title>Something went wrong · Orbit</title>
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
      </head>
      <body className="min-h-screen bg-page text-ink">
        <main className="flex min-h-screen flex-col items-center justify-center px-6 text-center">
          <p className="flex items-center gap-[9px] font-mono text-[10px] uppercase tracking-[0.16em] text-ink-2">
            <i className="size-[5px] rounded-full bg-brand" />
            Error
          </p>
          <h1 className="mt-5 max-w-[16ch] text-balance text-[clamp(30px,4.4vw,46px)] font-semibold leading-[1.1] tracking-[-0.028em] text-ink">
            Something went wrong.
          </h1>
          <p className="mt-5 max-w-[46ch] text-[15px] leading-[1.75] text-ink-2">
            Orbit hit an unexpected error while rendering this page. Trying
            again usually clears it up.
          </p>

          <div className="mt-9 flex flex-col items-center gap-3 sm:flex-row">
            <Button
              variant="default"
              size="lg"
              onClick={() => retry()}
              className="min-h-12 gap-2.5 rounded-[10px] px-6 text-sm font-semibold shadow-[0_1px_2px_rgba(0,0,0,0.18)] dark:border-white/20 light:border-black/10"
            >
              <HugeiconsIcon icon={RefreshIcon} data-icon="inline-start" />
              Try again
            </Button>
            {/* A plain anchor, not next/link: this document replaces the root
             * layout, so it must not depend on the client router context. */}
            {/* eslint-disable-next-line @next/next/no-html-link-for-pages */}
            <a
              href="/"
              className="inline-flex min-h-12 items-center justify-center gap-2.5 rounded-[10px] border border-edge-default px-6 text-sm text-ink no-underline transition-colors hover:bg-surface-2"
            >
              Back home
            </a>
          </div>

          {error.digest ? (
            <p className="mt-7 font-mono text-[10.5px] uppercase tracking-[0.12em] text-ink-3">
              Reference {error.digest}
            </p>
          ) : null}
        </main>
      </body>
    </html>
  );
}
