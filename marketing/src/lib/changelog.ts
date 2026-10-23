import { getDownloads, type Downloads } from "@/lib/downloads";

const REPO = "imrj05/orbit";
const CHANGELOG_URL = `https://raw.githubusercontent.com/${REPO}/main/CHANGELOG.md`;
const RELEASES_API = `https://api.github.com/repos/${REPO}/releases?per_page=100`;

/** Canonical releases page, used as the fallback destination. */
export const RELEASES_PAGE = `https://github.com/${REPO}/releases`;

export type ChangeGroup = {
  /** "Added", "Changed", … or "" when the source carried no category. */
  kind: string;
  items: string[];
};

export type ChangelogEntry = {
  version: string;
  tag: string;
  /** Calendar date, `YYYY-MM-DD`. */
  date: string;
  url: string;
  prerelease: boolean;
  groups: ChangeGroup[];
  downloads: Downloads;
};

type GithubAsset = { name?: string; browser_download_url?: string };

type GithubRelease = {
  tag_name?: string;
  html_url?: string;
  published_at?: string;
  prerelease?: boolean;
  body?: string;
  assets?: GithubAsset[];
};

type RawEntry = { version: string; date: string; groups: ChangeGroup[] };

/** Parse a GitHub release body (or any markdown list) into change groups. */
function parseBody(body: string): ChangeGroup[] {
  const groups: ChangeGroup[] = [];
  let group: ChangeGroup | null = null;
  let last = -1;

  const ensure = (): ChangeGroup => {
    if (!group) {
      group = { kind: "", items: [] };
      groups.push(group);
    }
    return group;
  };

  for (const raw of body.replace(/\r\n/g, "\n").split("\n")) {
    const line = raw.trim();
    if (!line || /^see changelog\.md/i.test(line)) {
      last = -1;
      continue;
    }

    const heading = line.match(/^#{1,4}\s+(.+?)\s*$/);
    if (heading) {
      group = { kind: heading[1].replace(/#+$/, "").trim(), items: [] };
      groups.push(group);
      last = -1;
      continue;
    }

    const bullet = raw.match(/^\s*[-*]\s+(.*)$/);
    const target = ensure();
    if (bullet) {
      target.items.push(bullet[1].trim());
      last = target.items.length - 1;
      continue;
    }

    // Indented continuation of the previous bullet.
    if (last >= 0 && /^\s+\S/.test(raw)) {
      target.items[last] = `${target.items[last]} ${line}`;
      continue;
    }

    target.items.push(line);
    last = target.items.length - 1;
  }

  return groups.filter((g) => g.items.length > 0);
}

/** Parse `CHANGELOG.md` into dated entries in Keep a Changelog order. */
function parseChangelog(markdown: string): RawEntry[] {
  const entries: RawEntry[] = [];
  let entry: RawEntry | null = null;
  let group: ChangeGroup | null = null;
  let last = -1;

  for (const raw of markdown.replace(/\r\n/g, "\n").split("\n")) {
    const version = raw.match(/^##\s+\[([^\]]+)\](?:\s*-\s*(.+))?\s*$/);
    if (version) {
      const name = version[1].trim();
      entry = /^unreleased$/i.test(name)
        ? null
        : { version: name, date: (version[2] ?? "").trim(), groups: [] };
      if (entry) entries.push(entry);
      group = null;
      last = -1;
      continue;
    }
    if (!entry) continue;

    const kind = raw.match(/^###\s+(.+?)\s*$/);
    if (kind) {
      group = { kind: kind[1].replace(/#+$/, "").trim(), items: [] };
      entry.groups.push(group);
      last = -1;
      continue;
    }

    const bullet = raw.match(/^\s*[-*]\s+(.*)$/);
    if (bullet) {
      if (!group) {
        group = { kind: "", items: [] };
        entry.groups.push(group);
      }
      group.items.push(bullet[1].trim());
      last = group.items.length - 1;
      continue;
    }

    const trimmed = raw.trim();
    // Reference-style link definitions at the foot of the file.
    if (!trimmed || /^\[[^\]]+\]:\s+\S+/.test(trimmed)) {
      last = -1;
      continue;
    }
    if (group && last >= 0 && /^\s+\S/.test(raw)) {
      group.items[last] = `${group.items[last]} ${trimmed}`;
    }
  }

  return entries;
}

async function fetchText(url: string): Promise<string> {
  try {
    const res = await fetch(url, {
      next: { revalidate: 86400 },
      headers: { Accept: "text/plain" },
    });
    return res.ok ? await res.text() : "";
  } catch {
    return "";
  }
}

async function fetchReleases(): Promise<GithubRelease[]> {
  try {
    const res = await fetch(RELEASES_API, {
      next: { revalidate: 86400 },
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return [];
    const data: unknown = await res.json();
    return Array.isArray(data) ? (data as GithubRelease[]) : [];
  } catch {
    return [];
  }
}

function isoDate(value?: string): string {
  return value ? value.slice(0, 10) : "";
}

/** Numeric version segments, for newest-first sorting (0.0.10 > 0.0.9). */
function versionRank(version: string): number[] {
  return version
    .replace(/^v/i, "")
    .split(/[.+-]/)
    .map((part) => (/^\d+$/.test(part) ? Number(part) : 0));
}

function compareVersions(a: string, b: string): number {
  const av = versionRank(a);
  const bv = versionRank(b);
  for (let i = 0; i < Math.max(av.length, bv.length); i++) {
    const delta = (bv[i] ?? 0) - (av[i] ?? 0);
    if (delta !== 0) return delta;
  }
  return 0;
}

/**
 * The full changelog, newest first.
 *
 * `CHANGELOG.md` is the canonical source; the Releases API fills the gaps
 * (some releases ship a summary rather than a changelog section) and supplies
 * the GitHub URL, pre-release flag, and release-only versions. Cached for a
 * day, and degrades to `[]` if GitHub is unreachable.
 */
export async function getChangelog(): Promise<ChangelogEntry[]> {
  const [markdown, releases] = await Promise.all([
    fetchText(CHANGELOG_URL),
    fetchReleases(),
  ]);

  const byTag = new Map(
    releases.map((release) => [release.tag_name ?? "", release]),
  );
  const seen = new Set<string>();
  const merged: ChangelogEntry[] = [];

  for (const raw of parseChangelog(markdown)) {
    const tag = `v${raw.version}`;
    const release = byTag.get(tag);
    seen.add(tag);
    merged.push({
      version: raw.version,
      tag: release?.tag_name || tag,
      date: raw.date || isoDate(release?.published_at),
      url: release?.html_url || `${RELEASES_PAGE}/tag/${tag}`,
      prerelease: Boolean(release?.prerelease),
      groups: raw.groups.length
        ? raw.groups
        : parseBody(release?.body ?? ""),
      downloads: getDownloads(release?.assets),
    });
  }

  // A release published without a CHANGELOG entry still belongs here.
  for (const release of releases) {
    const tag = release.tag_name ?? "";
    if (!tag || seen.has(tag)) continue;
    merged.push({
      version: tag.replace(/^v/i, ""),
      tag,
      date: isoDate(release.published_at),
      url: release.html_url || RELEASES_PAGE,
      prerelease: Boolean(release.prerelease),
      groups: parseBody(release.body ?? ""),
      downloads: getDownloads(release.assets),
    });
  }

  return merged.sort((a, b) => compareVersions(a.version, b.version));
}

/** A single release by tag or version (with or without a leading `v`). */
export async function getRelease(
  version: string,
): Promise<ChangelogEntry | null> {
  const target = version.replace(/^v/i, "");
  const entries = await getChangelog();
  return entries.find((entry) => entry.version === target) ?? null;
}

/** "2026-09-20" → "Sep 20, 2026", formatted in UTC so it never shifts. */
export function formatDate(value: string): string {
  if (!value) return "Unreleased";
  const date = new Date(`${value.slice(0, 10)}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en-US", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}
