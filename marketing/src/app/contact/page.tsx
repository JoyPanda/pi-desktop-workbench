import type { Metadata } from "next";
import type { ReactNode } from "react";
import {
  Bug01Icon,
  GithubIcon,
  Mail01Icon,
  ShieldKeyIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import { Band, SectionLabel, Shell } from "@/components/ui";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Contact",
  description:
    "Get in touch about Orbit — general questions, bug reports, and private security disclosures.",
  alternates: { canonical: "/contact" },
};

const CHANNELS: {
  icon: ReactNode;
  title: string;
  body: string;
  action: string;
  href: string;
  external?: boolean;
}[] = [
  {
    icon: <HugeiconsIcon icon={Mail01Icon} className="size-4" />,
    title: "General & press",
    body: "Questions, feedback, partnerships, or anything that isn't a bug report.",
    action: SITE.email,
    href: `mailto:${SITE.email}`,
  },
  {
    icon: <HugeiconsIcon icon={Bug01Icon} className="size-4" />,
    title: "Bugs & feature requests",
    body: "Open an issue on GitHub. Search first in case it's already tracked.",
    action: "Open an issue",
    href: `${SITE.github}/issues`,
    external: true,
  },
  {
    icon: <HugeiconsIcon icon={ShieldKeyIcon} className="size-4" />,
    title: "Security",
    body: "Report vulnerabilities privately — please don't open a public issue.",
    action: "How to report",
    href: "/security",
  },
  {
    icon: <HugeiconsIcon icon={GithubIcon} className="size-4" />,
    title: "Source & releases",
    body: "Browse the code, star the repo, and follow releases on GitHub.",
    action: SITE.github.replace("https://", ""),
    href: SITE.github,
    external: true,
  },
];

export default function ContactPage() {
  return (
    <>
      <SiteNav />
      <main className="flex-1">
        <Shell>
          <Band dashed className="relative overflow-hidden py-16 sm:py-20">
            <div className="relative z-[1]">
              <SectionLabel className="mb-5">Contact</SectionLabel>
              <h1 className="max-w-[16ch] text-[clamp(32px,4.4vw,52px)] font-[250] leading-[1.08] tracking-[-0.02em] text-ink">
                Talk to us.
              </h1>
              <p className="mt-5 max-w-[54ch] text-[15.5px] leading-[1.7] text-ink-2">
                A question, some feedback, or a bug to report? Pick the channel
                that fits and it&apos;ll reach the right place.
              </p>
            </div>
          </Band>

          <Band className="py-14 sm:py-16">
            <ul className="grid gap-4 sm:grid-cols-2">
              {CHANNELS.map((channel) => (
                <li key={channel.title}>
                  <a
                    href={channel.href}
                    {...(channel.external
                      ? { target: "_blank", rel: "noreferrer" }
                      : {})}
                    className="group flex h-full flex-col rounded-[14px] border border-edge-subtle bg-surface-1 p-6 no-underline transition-colors duration-[120ms] hover:border-edge-strong hover:bg-surface-2"
                  >
                    <span className="flex size-9 items-center justify-center rounded-[10px] border border-edge-subtle bg-surface-2 text-ink-2">
                      {channel.icon}
                    </span>
                    <span className="mt-4 text-[15px] font-semibold text-ink">
                      {channel.title}
                    </span>
                    <span className="mt-1.5 flex-1 text-[13.5px] leading-[1.7] text-ink-2">
                      {channel.body}
                    </span>
                    <span className="mt-4 font-mono text-[11px] uppercase tracking-[0.1em] text-ink-3 transition-colors group-hover:text-brand">
                      {channel.action}
                    </span>
                  </a>
                </li>
              ))}
            </ul>

            <p className="mt-8 text-[12.5px] text-ink-3">
              For private security reports, email{" "}
              <a
                href={`mailto:${SITE.email}`}
                className="text-ink-2 underline underline-offset-4 transition-colors hover:text-ink"
              >
                {SITE.email}
              </a>
              .
            </p>
          </Band>
        </Shell>
      </main>
      <SiteFooter />
    </>
  );
}
