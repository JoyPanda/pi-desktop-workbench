"use client";

import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

export type VersionIndexItem = {
  version: string;
  tag: string;
  dateLabel: string;
};

/**
 * Release navigation that tracks the entry currently in view. Keeps the
 * changelog scannable without a full client-side router.
 */
export function VersionIndex({
  entries,
  label,
}: {
  entries: VersionIndexItem[];
  /** Accessible name for the release list ("Jump to a release"). */
  label: string;
}) {
  const [active, setActive] = useState(entries[0]?.version ?? "");

  useEffect(() => {
    const articles = entries
      .map((entry) => document.getElementById(`v${entry.version}`))
      .filter((element): element is HTMLElement => element !== null);
    if (articles.length === 0) return;

    const observer = new IntersectionObserver(
      (observed) => {
        const visible = observed
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (visible[0]) setActive(visible[0].target.id.replace(/^v/, ""));
      },
      { rootMargin: "-96px 0px -62% 0px", threshold: [0, 1] },
    );

    articles.forEach((article) => observer.observe(article));
    return () => observer.disconnect();
  }, [entries]);

  return (
    <nav aria-label={label}>
      <ul className="flex flex-col gap-0.5">
        {entries.map((entry) => {
          const isActive = entry.version === active;
          return (
            <li key={entry.version}>
              <a
                href={`#v${entry.version}`}
                aria-current={isActive ? "true" : undefined}
                className={cn(
                  "group relative flex items-baseline justify-between gap-3 rounded-md px-3 py-2 text-[13px] no-underline transition-colors",
                  isActive
                    ? "bg-tint-05 text-ink"
                    : "text-ink-2 hover:bg-tint-03 hover:text-ink",
                )}
              >
                <span
                  aria-hidden
                  className={cn(
                    "absolute left-0 top-1/2 h-4 w-[2px] -translate-y-1/2 rounded-full bg-brand transition-opacity duration-200",
                    isActive ? "opacity-100" : "opacity-0",
                  )}
                />
                <span className="inline-flex items-center gap-2 font-mono">
                  <span
                    aria-hidden
                    className={cn(
                      "size-[5px] rounded-full transition-colors",
                      isActive ? "bg-brand" : "bg-ink-3/50",
                    )}
                  />
                  {entry.tag}
                </span>
                <span className="font-mono text-[10.5px] tracking-[0.04em] text-ink-3">
                  {entry.dateLabel}
                </span>
              </a>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
