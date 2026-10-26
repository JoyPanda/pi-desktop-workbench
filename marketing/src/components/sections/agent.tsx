import { ComposerMock, ToolsMock, TranscriptMock } from "@/components/sections/mocks";
import { MockFrame, Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Agent({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.agent");

  return (
    <Story
      id="agent"
      label={t("label")}
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      visual={
        <MockFrame>
          <div className="flex flex-col gap-5">
            <ComposerMock />
            <div className="h-px bg-edge-subtle" />
            <TranscriptMock />
            <div className="h-px bg-edge-subtle" />
            <ToolsMock />
          </div>
        </MockFrame>
      }
    />
  );
}
