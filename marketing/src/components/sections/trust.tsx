import {
  FlashIcon,
  LaptopIcon,
  SecurityLockIcon,
  SourceCodeIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { Band, Shell } from "@/components/ui";

/**
 * The four objections a pi user has before downloading: is it free, where does
 * my code go, is it really native, and will it run on my machine. Sits directly
 * under the hero so the answers land before the feature tour.
 */
const ITEMS = [
  {
    icon: SourceCodeIcon,
    title: "Free & open source",
    copy: "Apache-2.0. No account, no lock-in.",
  },
  {
    icon: SecurityLockIcon,
    title: "Local-first",
    copy: "Runs your own pi install. Code stays on your machine.",
  },
  {
    icon: FlashIcon,
    title: "Native, not a webview",
    copy: "Rust + GPUI, drawn on the GPU.",
  },
  {
    icon: LaptopIcon,
    title: "macOS · Windows · Linux",
    copy: "One app, the same sessions as the terminal.",
  },
];

export function Trust() {
  return (
    <Shell>
      <Band className="pb-4 pt-2 sm:pb-6">
        <dl className="grid grid-cols-1 gap-px overflow-hidden rounded-[14px] border border-edge-default bg-edge-subtle sm:grid-cols-2 lg:grid-cols-4">
          {ITEMS.map(({ icon: Icon, title, copy }) => (
            <div key={title} className="bg-page px-5 py-6">
              <dt className="flex items-center gap-2 text-[13px] font-medium text-ink">
                <HugeiconsIcon
                  icon={Icon}
                  className="size-4 shrink-0 text-ink-3"
                  strokeWidth={1.8}
                />
                {title}
              </dt>
              <dd className="mt-2 text-[12.5px] leading-[1.65] text-ink-2">
                {copy}
              </dd>
            </div>
          ))}
        </dl>
      </Band>
    </Shell>
  );
}
