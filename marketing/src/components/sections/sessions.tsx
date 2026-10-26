import { SessionsMock } from "@/components/sections/mocks";
import { MockFrame, Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Sessions({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.sessions");

  return (
    <Story
      id="sessions"
      label={t("label")}
      title={t("title")}
      copy={t("copy")}
      flip
      visual={
        <MockFrame>
          <SessionsMock />
        </MockFrame>
      }
    />
  );
}
