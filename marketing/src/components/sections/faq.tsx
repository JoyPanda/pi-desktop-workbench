import { ArrowDown01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { JsonLd } from "@/components/json-ld";
import { Band, SectionLabel, Shell } from "@/components/ui";
import { FAQ_ITEMS } from "@/lib/faq";
import { SITE } from "@/lib/site";

/**
 * `FAQPage` structured data, built from the same items the section renders so
 * they can never fall out of sync. Eligible for FAQ rich results.
 */
const FAQ_JSON_LD = {
  "@context": "https://schema.org",
  "@type": "FAQPage",
  "@id": `${SITE.url}/#faq`,
  mainEntity: FAQ_ITEMS.map((item) => ({
    "@type": "Question",
    name: item.q,
    acceptedAnswer: { "@type": "Answer", text: item.a },
  })),
};

/**
 * Native `<details>` disclosures — no JavaScript, keyboard accessible, and the
 * answer text is in the initial HTML for crawlers.
 */
export function Faq() {
  return (
    <Shell>
      <Band id="faq" dashed className="scroll-mt-20 py-20 sm:py-28">
        <JsonLd data={FAQ_JSON_LD} />

        <div className="mx-auto max-w-[760px]">
          <div className="flex flex-col items-center text-center">
            <SectionLabel>FAQ</SectionLabel>
            <h2 className="mt-4 max-w-[24ch] text-balance text-[clamp(26px,2.8vw,34px)] font-semibold leading-[1.18] tracking-[-0.024em] text-ink">
              Questions, answered.
            </h2>
            <p className="mt-4 max-w-[48ch] text-[14.5px] leading-[1.75] text-ink-2">
              The things pi users ask before trading their terminal for a
              window.
            </p>
          </div>

          <div className="mt-12">
            {FAQ_ITEMS.map((item) => (
              <details
                key={item.q}
                className="group border-b border-edge-subtle first:border-t"
              >
                <summary className="flex cursor-pointer list-none items-center justify-between gap-5 py-5 text-left text-[15px] font-medium text-ink outline-none transition-colors duration-150 hover:text-ink-2 focus-visible:text-brand [&::-webkit-details-marker]:hidden">
                  {item.q}
                  <HugeiconsIcon
                    icon={ArrowDown01Icon}
                    className="size-4 shrink-0 text-ink-3 transition-transform duration-200 group-open:rotate-180"
                    strokeWidth={1.8}
                  />
                </summary>
                <p className="pb-6 pr-8 text-[14px] leading-[1.75] text-ink-2">
                  {item.a}
                </p>
              </details>
            ))}
          </div>
        </div>
      </Band>
    </Shell>
  );
}
