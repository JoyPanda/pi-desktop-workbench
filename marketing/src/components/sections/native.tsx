import { ApprovalMock } from "@/components/sections/mocks";
import { MockFrame, Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Native({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.native");

  return (
    <Story
      id="native"
      label={t("label")}
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      visual={
        <MockFrame>
          <ApprovalMock />
        </MockFrame>
      }
    />
  );
}
