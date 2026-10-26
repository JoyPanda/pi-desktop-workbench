import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

function ModelCard() {
  const rows = [
    { name: "Claude", meta: "Anthropic" },
    { name: "GPT", meta: "OpenAI" },
    { name: "DeepSeek", meta: "Ollama · local" },
  ];
  return (
    <div className="absolute -left-5 top-8 hidden w-[188px] rounded-[12px] border border-edge-default bg-window/95 p-3 shadow-card backdrop-blur md:block">
      <p className="mb-2 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
        Model
      </p>
      <div className="flex flex-col gap-1">
        {rows.map((r, i) => (
          <div
            key={r.name}
            className={`flex items-baseline justify-between gap-2 rounded-[7px] px-2 py-1.5 text-[12px] ${
              i === 0 ? "bg-surface-2 text-ink" : "text-ink-2"
            }`}
          >
            <span>{r.name}</span>
            <span className="font-mono text-[10px] text-ink-3">{r.meta}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export async function Models({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.providers");

  return (
    <Story
      id="providers"
      label={t("label")}
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      flip
      visual={
        <div className="relative">
          <Shot
            src="/screens/models.png"
            srcLight="/screens/models-light.png"
            alt={t("shotAlt")}
            sizes="(max-width: 1024px) 100vw, 560px"
          />
          <ModelCard />
        </div>
      }
    />
  );
}
