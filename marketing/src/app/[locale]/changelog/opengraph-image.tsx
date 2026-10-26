import { renderOgCard } from "@/lib/og";

export const runtime = "nodejs";
export const alt = "Orbit changelog — every release, as it shipped";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function Image() {
  return renderOgCard({
    eyebrow: "Changelog",
    title: "Every release, as it shipped.",
    meta: "Release notes, pulled live from the repository.",
  });
}
