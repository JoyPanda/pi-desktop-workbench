import { createTranslator } from "next-intl";
import type { ReactNode } from "react";

import type { Locale } from "./config";
import { messagesFor } from "./messages";

/**
 * What a rich string may reference: an ICU variable to interpolate, or the
 * markup it carries inline (`<code>pi</code>`, `<link>releases</link>`).
 */
export type RichTags = Record<
  string,
  ((chunks: ReactNode) => ReactNode) | string | number
>;

export type Translator = ((
  key: string,
  values?: Record<string, string | number>,
) => string) & {
  raw(key: string): unknown;
  /** `t.rich("body", { code: (c) => <code>{c}</code> })` — see `RICH`. */
  rich(key: string, tags: RichTags): ReactNode;
};

export function getT(locale: Locale, namespace: string): Translator {
  return createTranslator({
    locale,
    messages: messagesFor(locale),
    // Namespaces are dotted paths (`sections.agent`); the generic would only
    // accept the literal union, which is not worth widening every call site for.
    namespace: namespace as never,
  }) as unknown as Translator;
}
