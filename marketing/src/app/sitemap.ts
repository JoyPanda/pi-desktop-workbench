import type { MetadataRoute } from "next";
import { getChangelog } from "@/lib/changelog";
import { SITE } from "@/lib/site";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const releases = await getChangelog();
  // The newest release is the last time the marketing pages actually changed;
  // using `new Date()` here would claim every URL changed on every regeneration.
  const latestDate = releases.find((entry) => entry.date)?.date;
  const lastModified = latestDate
    ? new Date(`${latestDate}T00:00:00Z`)
    : new Date();

  return [
    {
      url: `${SITE.url}/`,
      lastModified,
      changeFrequency: "weekly",
      priority: 1,
    },
    {
      url: `${SITE.url}/changelog`,
      lastModified,
      changeFrequency: "daily",
      priority: 0.8,
    },
    ...[
      { path: "/docs", priority: 0.7 },
      { path: "/docs/getting-started", priority: 0.6 },
      { path: "/docs/sessions", priority: 0.6 },
      { path: "/docs/worktrees", priority: 0.6 },
      { path: "/docs/mcp", priority: 0.6 },
      { path: "/docs/access-modes", priority: 0.6 },
      { path: "/docs/shortcuts", priority: 0.6 },
    ].map((page) => ({
      url: `${SITE.url}${page.path}`,
      lastModified,
      changeFrequency: "monthly" as const,
      priority: page.priority,
    })),
    ...[
      { path: "/privacy", priority: 0.4 },
      { path: "/terms", priority: 0.3 },
      { path: "/security", priority: 0.4 },
      { path: "/contact", priority: 0.4 },
    ].map((page) => ({
      url: `${SITE.url}${page.path}`,
      lastModified,
      changeFrequency: "yearly" as const,
      priority: page.priority,
    })),
    ...releases.map((entry) => ({
      url: `${SITE.url}/changelog/${entry.tag}`,
      lastModified: entry.date
        ? new Date(`${entry.date}T00:00:00Z`)
        : lastModified,
      changeFrequency: "monthly" as const,
      priority: 0.6,
    })),
  ];
}
