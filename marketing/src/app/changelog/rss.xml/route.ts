import { getChangelog } from "@/lib/changelog";
import { SITE } from "@/lib/site";

/**
 * RSS 2.0 feed of release notes at `/changelog/rss.xml`. Reuses the changelog
 * data (already cached for a day), so it costs nothing extra at the edge and
 * gives aggregators a fresh-content signal on every release.
 */
export const revalidate = 3600;

const XML_ENTITIES: Record<string, string> = {
  "<": "&lt;",
  ">": "&gt;",
  "&": "&amp;",
  "'": "&apos;",
  '"': "&quot;",
};

function escapeXml(value: string): string {
  return value.replace(/[<>&'"]/g, (char) => XML_ENTITIES[char] ?? char);
}

/** Flatten the changelog's light markdown into plain feed text. */
function toPlainText(value: string): string {
  return value
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/[*`_]+/g, "")
    .trim();
}

export async function GET() {
  const releases = await getChangelog();

  const items = releases
    .slice(0, 30)
    .map((release) => {
      const link = `${SITE.url}/changelog/${release.tag}`;
      const summary = release.groups
        .flatMap((group) => group.items)
        .slice(0, 12)
        .map((line) => `• ${toPlainText(line)}`)
        .join("\n");
      const pubDate = release.date
        ? new Date(`${release.date}T00:00:00Z`).toUTCString()
        : "";

      return [
        "    <item>",
        `      <title>${escapeXml(`Orbit Pi ${release.tag}`)}</title>`,
        `      <link>${escapeXml(link)}</link>`,
        `      <guid isPermaLink="true">${escapeXml(link)}</guid>`,
        pubDate ? `      <pubDate>${pubDate}</pubDate>` : "",
        `      <description>${escapeXml(
          summary || `Release notes for Orbit Pi ${release.tag}.`,
        )}</description>`,
        "    </item>",
      ]
        .filter(Boolean)
        .join("\n");
    })
    .join("\n");

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>${escapeXml(`${SITE.name} releases`)}</title>
    <link>${SITE.url}/changelog</link>
    <description>${escapeXml(
      "Release notes for Orbit, a native desktop workbench for the pi coding agent.",
    )}</description>
    <language>en</language>
    <atom:link href="${
      SITE.url
    }/changelog/rss.xml" rel="self" type="application/rss+xml" />
${items}
  </channel>
</rss>
`;

  return new Response(xml, {
    headers: {
      "Content-Type": "application/rss+xml; charset=utf-8",
      "Cache-Control":
        "public, max-age=3600, s-maxage=86400, stale-while-revalidate=86400",
    },
  });
}
