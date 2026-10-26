"use client";

import { useState } from "react";
import { Link01Icon, Tick02Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

/**
 * Copies a permalink to a release. Hover-revealed on pointer devices.
 *
 * `label` is the whole accessible name, already translated (`Copy link to
 * v0.0.15`) — the string is not assembled here, because word order around the
 * tag differs by language.
 */
export function CopyLinkButton({
  path,
  label,
}: {
  path: string;
  label: string;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    const url = `${window.location.origin}${path}`;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  }

  return (
    <button
      type="button"
      onClick={copy}
      aria-label={label}
      className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-ink-3 opacity-40 transition-colors hover:bg-tint-05 hover:text-ink focus-visible:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
    >
      <HugeiconsIcon
        icon={copied ? Tick02Icon : Link01Icon}
        className="size-3.5"
      />
    </button>
  );
}
