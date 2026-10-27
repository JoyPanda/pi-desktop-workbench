import { GitCompareIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

function GitCard() {
  return (
    <div className="absolute -bottom-5 right-6 hidden rounded-[12px] border border-edge-default bg-window/95 px-3.5 py-3 shadow-card backdrop-blur md:block">
      <p className="mb-1 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
        <HugeiconsIcon icon={GitCompareIcon} className="size-3" />
        Git
      </p>
      <p className="text-[12px] text-ink-2">
        <span className="text-ink">4 files changed</span>
        {" · "}
        <span className="font-mono text-[11px]">+355 −34</span>
      </p>
    </div>
  );
}

export async function Git({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.git");

  return (
    <Story
      id="git"
      label={t("label")}
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      flip
      visual={
        <div className="relative">
          <Shot
            src="/screens/review.png"
            srcLight="/screens/review-light.png"
            alt={t("shotAlt")}
            sizes="(max-width: 1024px) 100vw, 560px"
          />
          <GitCard />
        </div>
      }
    />
  );
}
