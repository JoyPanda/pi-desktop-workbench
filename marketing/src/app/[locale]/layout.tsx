import { NextIntlClientProvider } from "next-intl";
import { getMessages, setRequestLocale } from "next-intl/server";
import { notFound } from "next/navigation";

import { isLocale, type Locale } from "@/i18n/config";
import { hasMessages, translatedLocales } from "@/i18n/messages";

/**
 * Every route on the site, in every language that has a dictionary. `/[locale]`
 * is the root segment of the whole site — `[locale]/docs`, `[locale]/changelog`,
 * and the legal pages sit inside it — so this layout is where the language is
 * pinned and the messages are provided.
 */
export function generateStaticParams() {
  return translatedLocales.map((locale) => ({ locale }));
}

/**
 * Pins the locale for the whole subtree so the pages can be prerendered, and
 * provides messages to client components (the OS download button and the error
 * boundary are the two that need them). Server components read messages through
 * `getT` in `@/i18n/t`, which takes the locale explicitly.
 */
export default async function LocaleLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ locale: string }>;
}) {
  const { locale } = await params;
  if (!isLocale(locale) || !hasMessages(locale)) notFound();

  setRequestLocale(locale as Locale);
  const messages = await getMessages();

  return (
    // `<html lang>` is set once by the root layout, which every route shares, so
    // the language of this subtree is declared here instead — the nearest
    // ancestor wins for screen readers, hyphenation, and spellcheck.
    <div lang={locale} className="flex flex-1 flex-col">
      <NextIntlClientProvider locale={locale} messages={messages}>
        {children}
      </NextIntlClientProvider>
    </div>
  );
}
