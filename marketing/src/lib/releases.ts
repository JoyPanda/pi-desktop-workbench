const LATEST_RELEASE_API =
  "https://api.github.com/repos/imrj05/orbit/releases/latest";
const REPO_API = "https://api.github.com/repos/imrj05/orbit";
const CONTRIBUTORS_API = `${REPO_API}/contributors?per_page=100`;
const RELEASES_PAGE = "https://github.com/imrj05/orbit/releases";

/** Where to send someone when we cannot resolve a concrete asset. */
export const LATEST_RELEASE_URL = `${RELEASES_PAGE}/latest`;

export type Release = {
  tag: string;
  version: string;
  name: string;
  publishedAt: string;
  macos?: string;
  windows?: string;
  linux?: string;
};

type GithubAsset = { name?: string; browser_download_url?: string };

function assetUrl(assets: GithubAsset[], pattern: RegExp) {
  return assets.find((a) => a.name && pattern.test(a.name))?.browser_download_url;
}

/**
 * Latest published (non-prerelease) GitHub release, cached for a day. This
 * fetch gates the marketing pages' ISR window, so a longer TTL keeps the
 * homepage strongly cacheable at the CDN edge.
 * Returns `null` when the API is unavailable so callers can fall back to the
 * releases page instead of shipping a stale hard-coded version.
 */
export async function getLatestRelease(): Promise<Release | null> {
  try {
    const res = await fetch(LATEST_RELEASE_API, {
      next: { revalidate: 86400 },
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return null;

    const data = (await res.json()) as {
      tag_name?: string;
      name?: string;
      published_at?: string;
      assets?: GithubAsset[];
    };

    const tag = data.tag_name ?? "";
    if (!tag) return null;

    const assets = Array.isArray(data.assets) ? data.assets : [];
    return {
      tag,
      version: tag.replace(/^v/, ""),
      name: data.name || `Orbit ${tag}`,
      publishedAt: data.published_at ?? "",
      macos: assetUrl(assets, /\.dmg$/i),
      // Prefer the Windows installer, then the portable single-file build,
      // then the zip — so Windows is a direct .exe download.
      windows:
        assetUrl(assets, /setup\.exe$/i) ??
        assetUrl(assets, /^(?!.*setup).*\.exe$/i) ??
        assetUrl(assets, /windows.*\.zip$/i) ??
        assetUrl(assets, /\.zip$/i),
      // Prefer the native package over the tarball.
      linux:
        assetUrl(assets, /\.deb$/i) ??
        assetUrl(assets, /linux.*\.tar\.gz$/i) ??
        assetUrl(assets, /\.tar\.gz$/i),
    };
  } catch {
    return null;
  }
}

/**
 * Stargazer count for the repository, cached for a day (same window as the
 * release fetch, so it does not widen the page's ISR window). Returns `null`
 * when the API is unavailable so the header can fall back to a plain link.
 */
export async function getRepoStars(): Promise<number | null> {
  try {
    const res = await fetch(REPO_API, {
      next: { revalidate: 86400 },
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return null;

    const data = (await res.json()) as { stargazers_count?: number };
    return typeof data.stargazers_count === "number"
      ? data.stargazers_count
      : null;
  } catch {
    return null;
  }
}

export type Contributor = {
  login: string;
  avatarUrl: string;
  contributions: number;
  profileUrl: string;
};

/**
 * Repository contributors, cached for a day (the same window as the other
 * GitHub fetches, so it does not widen the page's ISR window). Bot accounts are
 * dropped, since they are not people we are thanking. Returns an empty array
 * when the API is unavailable so the section can hide rather than render an
 * empty shell.
 */
export async function getContributors(): Promise<Contributor[]> {
  try {
    const res = await fetch(CONTRIBUTORS_API, {
      next: { revalidate: 86400 },
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return [];

    const data = (await res.json()) as Array<{
      login?: string;
      avatar_url?: string;
      contributions?: number;
      html_url?: string;
      type?: string;
    }>;
    if (!Array.isArray(data)) return [];

    return data
      .filter((c) => c.login && c.type !== "Bot" && !c.login.endsWith("[bot]"))
      .map((c) => ({
        login: c.login as string,
        avatarUrl: c.avatar_url ?? "",
        contributions: c.contributions ?? 0,
        profileUrl: c.html_url ?? `https://github.com/${c.login}`,
      }));
  } catch {
    return [];
  }
}
