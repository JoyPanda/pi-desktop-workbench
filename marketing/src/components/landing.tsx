import Script from "next/script";

import { Agent } from "@/components/sections/agent";
import { Appearance } from "@/components/sections/appearance";
import { Closer } from "@/components/sections/closer";
import { Contributors } from "@/components/sections/contributors";
import { Faq } from "@/components/sections/faq";
import { Git } from "@/components/sections/git";
import { Hero } from "@/components/sections/hero";
import { Mcp } from "@/components/sections/mcp";
import { Models } from "@/components/sections/models";
import { Native } from "@/components/sections/native";
import { NewTask } from "@/components/sections/new-task";
import { Plugins } from "@/components/sections/plugins";
import { Reviews } from "@/components/sections/reviews";
import { Sessions } from "@/components/sections/sessions";
import { Skills } from "@/components/sections/skills";
import { Trust } from "@/components/sections/trust";
import { Usage } from "@/components/sections/usage";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import type { Locale } from "@/i18n/config";

/**
 * The landing page, shared by `/` (English) and `/{locale}`. The language of the
 * subtree is declared by `[locale]/layout`, so nothing here needs a `lang`.
 */
export async function Landing({ locale }: { locale: Locale }) {
  // Analytics is loaded only in production builds, never in `next dev`, so
  // local browsing does not pollute the dashboard.
  const analyticsEnabled = process.env.NODE_ENV === "production";

  return (
    <>
      <SiteNav locale={locale} path="/" />
      <main className="flex-1">
        <Hero locale={locale} />
        <Trust locale={locale} />
        <Agent locale={locale} />
        <Sessions locale={locale} />
        <NewTask locale={locale} />
        <Models locale={locale} />
        <Skills locale={locale} />
        <Git locale={locale} />
        <Usage locale={locale} />
        <Mcp locale={locale} />
        <Plugins locale={locale} />
        <Appearance locale={locale} />
        <Native locale={locale} />
        <Contributors locale={locale} />
        <Reviews locale={locale} />
        <Faq locale={locale} />
        <Closer locale={locale} />
      </main>
      <SiteFooter locale={locale} />
      {analyticsEnabled ? (
        <Script
          src="https://tracking.rajeshwar.tech/api/script.js"
          data-site-id="f08dd17d42ca"
          strategy="afterInteractive"
        />
      ) : null}
    </>
  );
}
