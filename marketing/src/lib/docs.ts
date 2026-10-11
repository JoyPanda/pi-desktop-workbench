/**
 * The docs table of contents. This is the single source of truth for the
 * sidebar, the prev/next pager, and the cards on the `/docs` index — add a
 * page here and link it from `DOCS_NAV`, and it appears everywhere.
 */
export type DocLink = {
  label: string;
  href: string;
  /** One line shown on the docs index cards. */
  description?: string;
};

export type DocGroup = {
  title: string;
  links: DocLink[];
};

export const DOCS_NAV: DocGroup[] = [
  {
    title: "Get started",
    links: [
      {
        label: "Overview",
        href: "/docs",
        description: "What Orbit is, how it works, and where your data lives.",
      },
      {
        label: "Install",
        href: "/docs/getting-started",
        description:
          "Download for your platform, clear the first-run checks, and send a first prompt.",
      },
    ],
  },
  {
    title: "Workbench",
    links: [
      {
        label: "Sessions",
        href: "/docs/sessions",
        description:
          "Projects, sessions, navigation, workflow and access modes, and notifications.",
      },
      {
        label: "Worktrees",
        href: "/docs/worktrees",
        description:
          "Run parallel branches as first-class workspaces, with an optional setup script.",
      },
      {
        label: "MCP servers",
        href: "/docs/mcp",
        description:
          "Manage the Model Context Protocol servers the agent can call, with a local secret store.",
      },
    ],
  },
  {
    title: "Safeguards",
    links: [
      {
        label: "Access modes",
        href: "/docs/access-modes",
        description:
          "Supervised, Auto-accept edits, and Full access: what runs without asking, and how approvals work.",
      },
    ],
  },
  {
    title: "Reference",
    links: [
      {
        label: "Keyboard shortcuts",
        href: "/docs/shortcuts",
        description: "Every shortcut, grouped by the surface it works in.",
      },
    ],
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
