"use client";

import { useSyncExternalStore, type ReactNode } from "react";
import { Download01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { LinuxIcon, MacosIcon, WindowsIcon } from "@/components/brand-icons";
import { ButtonLink } from "@/components/ui";

export type Platform = "macos" | "windows" | "linux";

const LABELS: Record<Platform, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};

/**
 * Best-effort OS detection from the user agent. Returns `null` for phones,
 * tablets, and anything we don't ship a build for, so the button falls back to
 * the generic releases page.
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
 * A download CTA that points at the build for the visitor's OS and shows that
 * platform's logo. Server-rendered generic ("Download"); the platform and icon
 * resolve on the client in an effect, so there is no hydration mismatch.
 */
export function OsDownloadButton({
  assets,
  fallback,
  variant = "primary",
  short = false,
  className,
}: {
  assets: Partial<Record<Platform, string>>;
  fallback: string;
  variant?: "primary" | "nav";
  short?: boolean;
  className?: string;
}) {
  const platform = usePlatform();

  const href = (platform && assets[platform]) || fallback;
  const label =
    platform && !short ? `Download for ${LABELS[platform]}` : "Download";

  return (
    <ButtonLink href={href} variant={variant} className={className} external>
      <PlatformIcon platform={platform} />
      {label}
    </ButtonLink>
  );
}

/** A link whose destination follows the visitor's OS. */
export function OsLink({
  assets,
  fallback,
  external = false,
  className,
  children,
}: {
  assets: Partial<Record<Platform, string>>;
  fallback: string;
  external?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const platform = usePlatform();
  const href = (platform && assets[platform]) || fallback;

  return (
    <a
      href={href}
      className={className}
      {...(external ? { target: "_blank", rel: "noreferrer" } : {})}
    >
      {children}
    </a>
  );
}
