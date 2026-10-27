import type { MetadataRoute } from "next";
import { translatedLocales } from "@/i18n/messages";
import { DEFAULT_LOCALE } from "@/i18n/config";
import { localeHref } from "@/i18n/links";
import { getChangelog } from "@/lib/changelog";
import { SITE } from "@/lib/site";

/** Every page, in every language, with the full `hreflang` set on each entry. */
const PAGES = [
  { path: "/", priority: 1, changeFrequency: "weekly" as const },
  { path: "/changelog", priority: 0.8, changeFrequency: "daily" as const },
  { path: "/docs", priority: 0.7, changeFrequency: "monthly" as const },
  { path: "/docs/getting-started", priority: 0.6, changeFrequency: "monthly" as const },
  { path: "/docs/sessions", priority: 0.6, changeFrequency: "monthly" as const },
  { path: "/docs/worktrees", priority: 0.6, changeFrequency: "monthly" as const },
  { path: "/docs/mcp", priority: 0.6, changeFrequency: "monthly" as const },
  { path: "/docs/access-modes", priority: 0.6, changeFrequency: "monthly" as const },
  { path: "/docs/shortcuts", priority: 0.6, changeFrequency: "monthly" as const },
  { path: "/privacy", priority: 0.4, changeFrequency: "yearly" as const },
  { path: "/terms", priority: 0.3, changeFrequency: "yearly" as const },
  { path: "/security", priority: 0.4, changeFrequency: "yearly" as const },
  { path: "/contact", priority: 0.4, changeFrequency: "yearly" as const },
];

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const releases = await getChangelog();
  // The newest release is the last time the marketing pages actually changed;
  // using `new Date()` here would claim every URL changed on every regeneration.
  const latestDate = releases.find((entry) => entry.date)?.date;
  const lastModified = latestDate
    ? new Date(`${latestDate}T00:00:00Z`)
    : new Date();

  const pages = PAGES.flatMap((page) => {
    // Every language of one page points at all the others, so a search engine
    // indexes them as translations rather than near-duplicates.
    const languages = Object.fromEntries([
      ...translatedLocales.map((locale) => [
        locale,
        `${SITE.url}${localeHref(locale, page.path)}`,
      ]),
      ["x-default", `${SITE.url}${page.path}`],
    ]);

    return translatedLocales.map((locale) => ({
      url: `${SITE.url}${localeHref(locale, page.path)}`,
      lastModified,
      changeFrequency: page.changeFrequency,
      priority:
        locale === DEFAULT_LOCALE ? page.priority : page.priority * 0.9,
      alternates: { languages },
    }));
  });

  // Release notes are written in the repository, in English, and are the same
  // document under every prefix — so they are listed once, unprefixed.
  const releasePages = releases.map((entry) => ({
    url: `${SITE.url}/changelog/${entry.tag}`,
    lastModified: entry.date ? new Date(`${entry.date}T00:00:00Z`) : lastModified,
    changeFrequency: "monthly" as const,
    priority: 0.6,
  }));

  return [...pages, ...releasePages];
}
