"use client";

import { Fragment, useSyncExternalStore, type ReactNode } from "react";
import { ArrowDown01Icon, Download01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { LinuxIcon, MacosIcon, WindowsIcon } from "@/components/brand-icons";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ButtonLink, buttonLinkClass, type ButtonIntent } from "@/components/ui";
import {
  PLATFORMS,
  preferredDownload,
  type Downloads,
  type Platform,
} from "@/lib/downloads";
import { cn } from "@/lib/utils";

const LABELS: Record<Platform, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};

const ICONS: Record<Platform, ReactNode> = {
  macos: <MacosIcon className="size-3.5" />,
  windows: <WindowsIcon className="size-3.5" />,
  linux: <LinuxIcon className="size-3.5" />,
};

/**
 * Best-effort OS detection from the user agent. Returns `null` for phones,
 * tablets, and anything we don't ship a build for, so the button falls back to
 * the generic releases page.
 *
 * Architecture is deliberately not detected: browsers do not agree on how to
 * report it, and the menu lists every build anyway.
 */
function detectPlatform(): Platform | null {
  if (typeof navigator === "undefined") return null;
  const ua = navigator.userAgent;

  if (/iphone|ipad|ipod/i.test(ua)) return null;
  // iPadOS 13+ reports itself as "Macintosh"; the touch points give it away.
  if (/macintosh/i.test(ua) && navigator.maxTouchPoints > 1) return null;
  if (/android/i.test(ua)) return null;
  if (/cros/i.test(ua)) return null;

  if (/windows/i.test(ua)) return "windows";
  if (/macintosh|mac os x/i.test(ua)) return "macos";
  if (/linux|x11/i.test(ua)) return "linux";
  return null;
}

const subscribe = () => () => {};
const getSnapshot = (): Platform | null => detectPlatform();
const getServerSnapshot = (): Platform | null => null;

/** The visitor's OS, or `null` before the client has hydrated. */
export function usePlatform(): Platform | null {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}

function PlatformIcon({ platform }: { platform: Platform | null }) {
  if (platform === "macos") {
    return <MacosIcon data-icon="inline-start" />;
  }
  if (platform === "windows") {
    return <WindowsIcon data-icon="inline-start" />;
  }
  if (platform === "linux") {
    return <LinuxIcon data-icon="inline-start" />;
  }
  return (
    <HugeiconsIcon
      icon={Download01Icon}
      data-icon="inline-start"
      strokeWidth={1.8}
    />
  );
}

/**
 * The download CTA: a split button whose main half hands over the build for the
 * visitor's OS and whose menu half lists every other platform, architecture,
 * and package format the release ships.
 *
 * Server-rendered generic ("Download"); the platform, label, and target resolve
 * on the client, so there is no hydration mismatch.
 */
export function OsDownloadButton({
  downloads,
  fallback,
  variant = "primary",
  short = false,
  menu = true,
  className,
}: {
  downloads: Downloads;
  fallback: string;
  variant?: ButtonIntent;
  short?: boolean;
  /** Set false where a split control will not fit, e.g. the header. */
  menu?: boolean;
  className?: string;
}) {
  const platform = usePlatform();
  const primary = platform ? preferredDownload(downloads, platform) : undefined;
  const label =
    platform && !short ? `Download for ${LABELS[platform]}` : "Download";

  const groups = PLATFORMS.map((platform) => ({
    platform,
    items: downloads[platform],
  })).filter((group) => group.items.length > 0);

  // One plain button when there is nothing to choose between (no release
  // resolved, or it shipped no assets) and wherever the split control will not
  // fit (the header). The target is still the visitor's platform build.
  if (groups.length === 0 || !menu) {
    return (
      <ButtonLink
        href={primary?.url ?? fallback}
        variant={variant}
        className={className}
        external
      >
        <PlatformIcon platform={platform} />
        {label}
      </ButtonLink>
    );
  }

  // `inline-flex`: content-sized inside a block (the changelog card), but as a
  // flex item it blockifies and stretches to the row, which is what the hero's
  // stacked CTAs need. `flex-1` on the primary makes it fill that stretched row.
  return (
    <div className="inline-flex items-stretch">
      <a
        href={primary?.url ?? fallback}
        title={primary?.file}
        className={buttonLinkClass(
          variant,
          cn("flex-1 rounded-r-none", className),
        )}
        {...(primary ? {} : { target: "_blank", rel: "noreferrer" })}
      >
        <PlatformIcon platform={platform} />
        {label}
      </a>
      <DropdownMenu>
        <DropdownMenuTrigger
          aria-label="Other platforms and formats"
          className={buttonLinkClass(
            variant,
            // One visual control split in two: the menu half cancels the
            // radius the shared intent adds, sits flush over the primary's
            // seam, stretches to the primary's height, and draws its own
            // divider in the foreground colour so it reads in both themes.
            "relative -ml-px h-auto self-stretch rounded-l-none border-l border-l-primary-foreground/20 px-2.5 shadow-none",
          )}
        >
          <HugeiconsIcon icon={ArrowDown01Icon} strokeWidth={1.8} className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="end"
          sideOffset={6}
          className="w-[286px] p-1.5 ring-edge-default"
        >
          {groups.map(({ platform, items }, index) => (
            <Fragment key={platform}>
              {index > 0 ? <DropdownMenuSeparator /> : null}
              <DropdownMenuGroup>
                <DropdownMenuLabel className="flex items-center gap-2 px-1.5 pb-1 font-mono text-[10px] uppercase tracking-[0.09em] text-ink-3">
                  {ICONS[platform]}
                  {LABELS[platform]}
                </DropdownMenuLabel>
                {items.map((download) => (
                  <DropdownMenuItem
                    key={download.file}
                    title={download.file}
                    className="gap-2 px-1.5 py-1.5"
                    render={
                      <a
                        href={download.url}
                        target="_blank"
                        rel="noreferrer"
                      />
                    }
                  >
                    <span className="flex min-w-0 items-baseline gap-2">
                      <span className="truncate text-[12.5px] text-ink">
                        {download.label}
                      </span>
                      <span className="shrink-0 font-mono text-[10px] uppercase tracking-[0.08em] text-ink-3">
                        {download.detail}
                      </span>
                    </span>
                    <span className="ml-auto shrink-0 font-mono text-[9.5px] uppercase tracking-[0.1em] text-ink-3">
                      {download.format}
                    </span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuGroup>
            </Fragment>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
