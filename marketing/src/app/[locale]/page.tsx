import type { Metadata } from "next";
import { notFound } from "next/navigation";

import { Landing } from "@/components/landing";
import { getT } from "@/i18n/t";
import { hasMessages, translatedLocales } from "@/i18n/messages";
import { isLocale, localePath } from "@/i18n/config";

/**
 * The landing page in every language that has a dictionary. Only these paths
 * exist — `/fr/docs` and friends are English-only routes, so the nav on a
 * translated page keeps pointing at the unprefixed ones.
 */
export const dynamicParams = false;

export function generateStaticParams() {
  return translatedLocales.map((locale) => ({ locale }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ locale: string }>;
}): Promise<Metadata> {
  const { locale } = await params;
  if (!isLocale(locale) || !hasMessages(locale)) return {};

  const t = getT(locale, "meta");
  const languages = Object.fromEntries([
    ...translatedLocales.map((entry) => [entry, localePath(entry)]),
    ["x-default", "/"],
  ]);

  return {
    title: t("title"),
    description: t("description"),
    alternates: { canonical: localePath(locale), languages },
    openGraph: {
      title: t("title"),
      description: t("description"),
      locale: locale.replace("-", "_"),
      url: localePath(locale),
    },
    // The root layout's Twitter card is English; a localized page must not
    // ship an English card next to German copy.
    twitter: {
      title: t("title"),
      description: t("description"),
    },
  };
}

export default async function LocalizedHome({
  params,
}: {
  params: Promise<{ locale: string }>;
}) {
  const { locale } = await params;
  if (!isLocale(locale) || !hasMessages(locale)) notFound();

  return <Landing locale={locale} />;
}
