import Image from "next/image";
import { GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { Band, ButtonLink, SectionLabel, Shell } from "@/components/ui";
import { getContributors } from "@/lib/releases";
import { SITE } from "@/lib/site";

/** Avatars shown in the stack before the "+N" chip takes over. */
const MAX_AVATARS = 32;

const GRAPH_URL = `${SITE.github}/graphs/contributors`;

/**
 * Centered avatar stack of everyone who has contributed. Avatars overlap, the
 * most active contributor sits first, and each one links to their profile.
 * Renders nothing if GitHub is unavailable.
 */
export async function Contributors() {
  const contributors = await getContributors();
  if (contributors.length === 0) return null;

  const shown = contributors.slice(0, MAX_AVATARS);
  const extra = contributors.length - shown.length;

  return (
    <Shell>
      <Band
        id="contributors"
        dashed
        className="scroll-mt-20 py-20 text-center sm:py-28"
      >
        <div className="flex flex-col items-center">
          <SectionLabel>Contributors</SectionLabel>
          <h2 className="mt-4 max-w-[22ch] text-balance text-[clamp(26px,2.8vw,34px)] font-semibold leading-[1.18] tracking-[-0.024em] text-ink">
            Built in the open.
          </h2>
          <p className="mt-4 max-w-[46ch] text-[14.5px] leading-[1.75] text-ink-2">
            Orbit is shaped by everyone who files an issue, sends a patch, or
            reviews a release.
          </p>

          <ul className="isolate mt-12 flex flex-wrap items-center justify-center">
            {shown.map((c) => (
              <li
                key={c.login}
                className="relative -ml-2 first:ml-0 transition-transform duration-200 hover:z-10 hover:-translate-y-1"
              >
                <a
                  href={c.profileUrl}
                  target="_blank"
                  rel="noreferrer"
                  aria-label={c.login}
                  title={c.login}
                  className="block rounded-full outline-none ring-2 ring-page focus-visible:ring-brand"
                >
                  <Image
                    src={c.avatarUrl}
                    alt=""
                    width={32}
                    height={32}
                    className="size-8 rounded-full border border-edge-default"
                  />
                </a>
              </li>
            ))}

            {extra > 0 ? (
              <li className="relative -ml-2">
                <a
                  href={GRAPH_URL}
                  target="_blank"
                  rel="noreferrer"
                  aria-label={`${extra} more contributors on GitHub`}
                  className="flex size-8 items-center justify-center rounded-full border border-edge-default bg-surface-2 font-mono text-[10px] text-ink-2 ring-2 ring-page no-underline transition-colors hover:text-ink"
                >
                  +{extra}
                </a>
              </li>
            ) : null}
          </ul>

          <ButtonLink
            href={GRAPH_URL}
            variant="secondary"
            external
            className="mt-12 min-h-11 px-5 text-[13px]"
          >
            <HugeiconsIcon icon={GithubIcon} data-icon="inline-start" />
            View on GitHub
          </ButtonLink>
        </div>
      </Band>
    </Shell>
  );
}
