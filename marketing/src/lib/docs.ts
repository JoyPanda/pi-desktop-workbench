/**
 * The docs table of contents. This is the single source of truth for the
 * sidebar, the prev/next pager, and the cards on the `/docs` index — add a page
 * here and link it from `DOCS_NAV`, and it appears everywhere.
 *
 * Only structure lives here. Every `id` is a key in the `docs.nav` messages
 * (`<id>` for the label, `<id>Description` for the one-liner), so the sidebar is
 * translated without duplicating the tree in ten dictionaries.
 */
export type DocLink = {
  id: string;
  href: string;
};

export type DocGroup = {
  id: string;
  links: DocLink[];
};

export const DOCS_NAV: DocGroup[] = [
  {
    id: "getStarted",
    links: [
      { id: "overview", href: "/docs" },
      { id: "install", href: "/docs/getting-started" },
    ],
  },
  {
    id: "workbench",
    links: [
      { id: "sessions", href: "/docs/sessions" },
      { id: "worktrees", href: "/docs/worktrees" },
      { id: "mcp", href: "/docs/mcp" },
    ],
  },
  {
    id: "safeguards",
    links: [{ id: "accessModes", href: "/docs/access-modes" }],
  },
  {
    id: "reference",
    links: [{ id: "shortcuts", href: "/docs/shortcuts" }],
  },
];

/** Every page in reading order, used by the prev/next pager. */
export const DOCS_FLAT: DocLink[] = DOCS_NAV.flatMap((group) => group.links);

export function docsNeighbours(href: string): {
  prev?: DocLink;
  next?: DocLink;
} {
  const i = DOCS_FLAT.findIndex((link) => link.href === href);
  if (i === -1) return {};
  return {
    prev: i > 0 ? DOCS_FLAT[i - 1] : undefined,
    next: i < DOCS_FLAT.length - 1 ? DOCS_FLAT[i + 1] : undefined,
  };
}
