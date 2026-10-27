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
import { getT, type RichTags } from "@/i18n/t";
import { localeHref } from "@/i18n/links";
import { localeAlternates } from "@/i18n/meta";
import { pageLocale } from "@/i18n/page";
import type { Locale } from "@/i18n/config";

export async function generateMetadata({
  params,
}: PageProps<"/[locale]/contact">): Promise<Metadata> {
  const locale = await pageLocale(params);

  return {
    title: "Contact",
    description:
      "Get in touch about Orbit — general questions, bug reports, and private security disclosures.",
    alternates: localeAlternates(locale, "/contact"),
  };
}

/** Only the parts that are not text: the wording lives in `contact.channels`. */
const CHANNELS: {
  id: "general" | "bugs" | "security" | "source";
  icon: ReactNode;
  href: (locale: Locale) => string;
  meta?: string;
  external?: boolean;
}[] = [
  {
    id: "general",
    icon: <HugeiconsIcon icon={Mail01Icon} className="size-4" />,
    href: () => `mailto:${SITE.email}`,
    meta: SITE.email,
  },
  {
    id: "bugs",
    icon: <HugeiconsIcon icon={Bug01Icon} className="size-4" />,
    href: () => `${SITE.github}/issues`,
    external: true,
  },
  {
    id: "security",
    icon: <HugeiconsIcon icon={ShieldKeyIcon} className="size-4" />,
    href: (locale) => localeHref(locale, "/security"),
  },
  {
    id: "source",
    icon: <HugeiconsIcon icon={GithubIcon} className="size-4" />,
    href: () => SITE.github,
    meta: SITE.github.replace("https://", ""),
    external: true,
  },
];

export default async function ContactPage({
  params,
}: PageProps<"/[locale]/contact">) {
  const locale = await pageLocale(params);
  const t = getT(locale, "contact");
  const rich: RichTags = {
    link: (chunks) => (
      <a
        href={`mailto:${SITE.email}`}
        className="text-ink-2 underline underline-offset-4 transition-colors hover:text-ink"
      >
        {chunks}
      </a>
    ),
  };

  return (
    <>
      <SiteNav locale={locale} path="/contact" />
      <main className="flex-1">
        <Shell>
          <Band dashed className="relative overflow-hidden py-16 sm:py-20">
            <div className="relative z-[1]">
              <SectionLabel className="mb-5">{t("label")}</SectionLabel>
              <h1 className="max-w-[16ch] text-[clamp(32px,4.4vw,52px)] font-[250] leading-[1.08] tracking-[-0.02em] text-ink">
                {t("heading")}
              </h1>
              <p className="mt-5 max-w-[54ch] text-[15.5px] leading-[1.7] text-ink-2">
                {t("body")}
              </p>
            </div>
          </Band>

          <Band className="py-14 sm:py-16">
            <ul className="grid gap-4 sm:grid-cols-2">
              {CHANNELS.map((channel) => (
                <li key={channel.id}>
                  <a
                    href={channel.href(locale)}
                    {...(channel.external
                      ? { target: "_blank", rel: "noreferrer" }
                      : {})}
                    className="group flex h-full flex-col rounded-[14px] border border-edge-subtle bg-surface-1 p-6 no-underline transition-colors duration-[120ms] hover:border-edge-strong hover:bg-surface-2"
                  >
                    <span className="flex size-9 items-center justify-center rounded-[10px] border border-edge-subtle bg-surface-2 text-ink-2">
                      {channel.icon}
                    </span>
                    <span className="mt-4 text-[15px] font-semibold text-ink">
                      {t(`channels.${channel.id}.title`)}
                    </span>
                    <span className="mt-1.5 flex-1 text-[13.5px] leading-[1.7] text-ink-2">
                      {t(`channels.${channel.id}.body`)}
                    </span>
                    <span className="mt-4 font-mono text-[11px] uppercase tracking-[0.1em] text-ink-3 transition-colors group-hover:text-brand">
                      {channel.meta ?? t(`channels.${channel.id}.action`)}
                    </span>
                  </a>
                </li>
              ))}
            </ul>

            <p className="mt-8 text-[12.5px] text-ink-3">
              {t.rich("private", { ...rich, email: SITE.email })}
            </p>
          </Band>
        </Shell>
      </main>
      <SiteFooter locale={locale} />
    </>
  );
}
