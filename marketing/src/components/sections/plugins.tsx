import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Plugins({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.plugins");

  return (
    <Story
      id="plugins"
      label={t("label")}
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      visual={
        <Shot
          src="/screens/plugins.png"
          srcLight="/screens/plugins-light.png"
          alt={t("shotAlt")}
          sizes="(max-width: 1024px) 100vw, 560px"
        />
      }
    />
  );
}
