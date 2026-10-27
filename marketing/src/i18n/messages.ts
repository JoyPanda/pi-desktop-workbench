import type { Locale } from "./config";

import en from "../../messages/en.json";
import de from "../../messages/de.json";
import es from "../../messages/es.json";
import fr from "../../messages/fr.json";
import it from "../../messages/it.json";
import ja from "../../messages/ja.json";
import ko from "../../messages/ko.json";
import ptBR from "../../messages/pt-BR.json";
import ru from "../../messages/ru.json";
import zhCN from "../../messages/zh-CN.json";

/**
 * Every message file, imported statically. A missing `messages/<locale>.json` is
 * a build error here rather than an English page served under a German URL, and
 * `translatedLocales` — what the switcher offers and what `generateStaticParams`
 * prerenders — is derived from this map, so the three cannot disagree.
 *
 * Static imports (not `import()` per request) are what keep the pages
 * prerenderable: the locale comes from the URL, so no request state is read.
 */
export const MESSAGES = {
  en,
  de,
  es,
  fr,
  it,
  ja,
  ko,
  "pt-BR": ptBR,
  ru,
  "zh-CN": zhCN,
} as const;

export type Messages = (typeof en);

export const translatedLocales = Object.keys(MESSAGES) as Locale[];

export function hasMessages(locale: Locale): boolean {
  return locale in MESSAGES;
}

type Dict = Record<string, unknown>;

function isDict(value: unknown): value is Dict {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Every key present in English, overwritten key by key with the locale's own
 * translation. A dictionary that has not been filled in yet therefore serves the
 * English sentence for exactly the keys it is missing instead of throwing
 * `MISSING_MESSAGE` and rendering the raw key path.
 */
function merge(base: Dict, over: Dict): Dict {
  const merged: Dict = { ...base };
  for (const [key, value] of Object.entries(over)) {
    const previous = merged[key];
    merged[key] = isDict(previous) && isDict(value) ? merge(previous, value) : value;
  }
  return merged;
}

/** Merged once per locale — a request should not pay for this twice. */
const MERGED = new Map<Locale, Dict>();

export function messagesFor(locale: Locale): Messages {
  const cached = MERGED.get(locale);
  if (cached) return cached as Messages;

  const merged = merge(
    MESSAGES.en as unknown as Dict,
    (MESSAGES[locale] ?? MESSAGES.en) as unknown as Dict,
  );
  MERGED.set(locale, merged);
  return merged as Messages;
}

/**
 * `fill("Release notes · {tag}", { tag })`. Only for copy that reaches a client
 * component, where `next-intl`'s translator is not available without shipping the
 * messages to the browser; server components interpolate through `t` instead.
 */
export function fill(
  template: string,
  values: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in values ? String(values[name]) : match,
  );
}
