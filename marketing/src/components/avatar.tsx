"use client";

import { useState } from "react";

/**
 * A reviewer's avatar, with the handle's initial as the fallback. Sources are
 * Reddit's default snoo images, loaded straight from their CDN; if a request
 * fails we keep the card intact with the initial.
 */
export function UserAvatar({
  src,
  handle,
  className = "",
}: {
  src?: string;
  handle: string;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const initial =
    handle.replace(/^(u\/|@)/, "").charAt(0).toUpperCase() || "?";

  if (src && !failed) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        src={src}
        alt=""
        width={28}
        height={28}
        loading="lazy"
        decoding="async"
        onError={() => setFailed(true)}
        className={`size-7 shrink-0 rounded-full border border-edge-default bg-surface-2 object-cover ${className}`}
      />
    );
  }

  return (
    <span
      className={`flex size-7 shrink-0 items-center justify-center rounded-full border border-edge-default bg-surface-2 font-mono text-[10px] text-ink-2 ${className}`}
    >
      {initial}
    </span>
  );
}
