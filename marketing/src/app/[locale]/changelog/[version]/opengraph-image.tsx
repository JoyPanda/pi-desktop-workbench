import { formatDate, getChangelog, getRelease } from "@/lib/changelog";
import { renderOgCard } from "@/lib/og";

export const runtime = "nodejs";
export const alt = "Orbit Pi release notes";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export async function generateStaticParams() {
  const entries = await getChangelog();
  return entries.map((entry) => ({ version: entry.tag }));
}

export default async function Image({
  params,
}: {
  params: Promise<{ version: string }>;
}) {
  const { version } = await params;
  const entry = await getRelease(version);

  return renderOgCard({
    eyebrow: "Release notes",
    title: entry ? `Orbit Pi ${entry.tag}` : "Orbit Pi release notes",
    meta: entry ? formatDate(entry.date) : undefined,
  });
}
