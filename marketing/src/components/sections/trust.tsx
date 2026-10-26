import {
  FlashIcon,
  LaptopIcon,
  SecurityLockIcon,
  SourceCodeIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { Band, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

/**
 * The four objections a pi user has before downloading: is it free, where does
 * my code go, is it really native, and will it run on my machine. Sits directly
 * under the hero so the answers land before the feature tour.
 */
const ICONS = [SourceCodeIcon, SecurityLockIcon, FlashIcon, LaptopIcon];

export async function Trust({ locale }: { locale: Locale }) {
  const t = getT(locale, "trust");
  const items = t.raw("items") as { title: string; copy: string }[];
  return (
    <Shell>
      <Band className="pb-4 pt-2 sm:pb-6">
        <dl className="grid grid-cols-1 gap-px overflow-hidden rounded-[14px] border border-edge-default bg-edge-subtle sm:grid-cols-2 lg:grid-cols-4">
          {items.map(({ title, copy }, index) => {
            const Icon = ICONS[index] ?? SourceCodeIcon;
            return (
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
            );
          })}
        </dl>
      </Band>
    </Shell>
  );
}
