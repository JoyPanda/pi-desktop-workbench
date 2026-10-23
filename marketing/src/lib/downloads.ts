/**
 * The builds a release ships, parsed from its GitHub asset filenames and
 * grouped by platform. The download menu renders straight from this, so the
 * site never hard-codes a version, a triple, or a filename.
 *
 * Order matters: the first entry for a platform is the one the primary
 * download button offers, and the rest are the menu's alternatives.
 */

export type Platform = "macos" | "windows" | "linux";

/** Groups render in this order, which is also the order the site names them. */
export const PLATFORMS: Platform[] = ["macos", "windows", "linux"];

export type Download = {
  platform: Platform;
  /** Row label, e.g. "Apple silicon" or "Debian package". */
  label: string;
  /** Short qualifier beside the label, e.g. "Arm64" or "x64". */
  detail: string;
  /** File type, shown at the end of the row. */
  format: string;
  /** Exact asset filename, surfaced as the row's tooltip. */
  file: string;
  url: string;
};

export type Downloads = Record<Platform, Download[]>;

/** A release with no assets (or no release at all) renders as links only. */
export const NO_DOWNLOADS: Downloads = { macos: [], windows: [], linux: [] };

export type GithubAsset = { name?: string; browser_download_url?: string };

type Meta = Pick<Download, "label" | "detail" | "format">;

function find(
  assets: GithubAsset[],
  platform: Platform,
  pattern: RegExp,
  meta: Meta,
): Download | null {
  const asset = assets.find((a) => a.name && pattern.test(a.name));
  if (!asset?.name || !asset.browser_download_url) return null;
  return { platform, ...meta, file: asset.name, url: asset.browser_download_url };
}

function compact(list: (Download | null)[]): Download[] {
  return list.filter((d): d is Download => d !== null);
}

/**
 * Map a release's assets onto per-platform download lists. Missing artifacts
 * (a platform that failed to build) simply drop out, and the menu hides the
 * groups that end up empty.
 */
export function getDownloads(input: GithubAsset[] | undefined): Downloads {
  const assets = Array.isArray(input) ? input : [];

  const macos = [
    // Universal first: it is the one DMG that cannot be the wrong
    // architecture, so it is also what the primary button hands over.
    find(assets, "macos", /-universal\.dmg$/i, {
      label: "Universal",
      detail: "Intel + Apple silicon",
      format: "DMG",
    }),
    find(assets, "macos", /-arm64\.dmg$/i, {
      label: "Apple silicon",
      detail: "Arm64",
      format: "DMG",
    }),
  ];

  // The zip holds the same binary as the portable .exe, so it is not offered.
  const windows = [
    find(assets, "windows", /-setup\.exe$/i, {
      label: "Installer",
      detail: "x64",
      format: "EXE",
    }),
    // The portable build carries the full target triple in its name
    // (`orbit-pi-<version>-x86_64-pc-windows-msvc.exe`), so match the suffix
    // rather than the arch, and leave the installer to the row above.
    find(assets, "windows", /^(?!.*-setup\.exe$).*\.exe$/i, {
      label: "Portable",
      detail: "x64",
      format: "EXE",
    }),
  ];

  // Package format first, architecture second: choosing .deb vs .tar.gz is the
  // decision a Linux user makes; the arch is a column they scan for.
  const linux = [
    find(assets, "linux", /_amd64\.deb$/i, {
      label: "Debian package",
      detail: "x64",
      format: "DEB",
    }),
    find(assets, "linux", /_arm64\.deb$/i, {
      label: "Debian package",
      detail: "Arm64",
      format: "DEB",
    }),
    find(assets, "linux", /-x86_64-unknown-linux-gnu\.tar\.gz$/i, {
      label: "Tarball",
      detail: "x64",
      format: "TAR.GZ",
    }),
    find(assets, "linux", /-aarch64-unknown-linux-gnu\.tar\.gz$/i, {
      label: "Tarball",
      detail: "Arm64",
      format: "TAR.GZ",
    }),
  ];

  return {
    macos: compact(macos),
    windows: compact(windows),
    linux: compact(linux),
  };
}

/** The build a plain "get it for <platform>" link should point at. */
export function preferredDownload(
  downloads: Downloads,
  platform: Platform,
): Download | undefined {
  return downloads[platform][0];
}
