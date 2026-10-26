import { SITE } from "@/lib/site";

import type { RichTags } from "./t";

/**
 * The inline markup a translated string may carry, as next-intl rich-text tags.
 *
 * Keeping one shared map means a paragraph stays one translatable sentence —
 * `<code>pi</code>` and `<strong>only in production builds</strong>` can move
 * wherever the target language needs them — instead of being chopped into
 * fragments whose word order only works in English.
 */
export const RICH: RichTags = {
  code: (chunks) => <code>{chunks}</code>,
  strong: (chunks) => <strong>{chunks}</strong>,
  em: (chunks) => <em>{chunks}</em>,
  kbd: (chunks) => <kbd>{chunks}</kbd>,
  /** `SITE.email`, as a mailto link. */
  mail: (chunks) => <a href={`mailto:${SITE.email}`}>{chunks}</a>,
};
