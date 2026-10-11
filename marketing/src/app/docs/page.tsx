import type { Metadata } from "next";
import Link from "next/link";

import { DocsPage } from "@/components/docs-page";
import { DOCS_NAV } from "@/lib/docs";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Docs",
  description:
    "Guides for Orbit, the native desktop workbench for the pi coding agent: install, sessions, worktrees, MCP servers, and keyboard shortcuts.",
  alternates: { canonical: "/docs" },
};

export default function DocsOverviewPage() {
  return (
    <DocsPage
      current="/docs"
      title="Orbit documentation"
      intro="Everything you need to install Orbit, run agent sessions, work across Git worktrees, connect MCP servers, and learn the keyboard."
    >
      <h2>What Orbit is</h2>
      <p>
        Orbit is a native desktop workbench for the{" "}
        <a href="https://github.com/earendil-works/pi" target="_blank" rel="noreferrer">
          pi coding agent
        </a>
        . It is a Rust app rendered with GPUI, the GPU-accelerated UI framework
        behind Zed — no browser, no web view, no Node UI layer.
      </p>
      <p>
        Orbit is not a chat wrapper around a model API. It spawns the{" "}
        <code>pi</code> CLI as a child process and speaks pi&apos;s own RPC
        protocol over stdio. The model catalog, tools, thinking levels, and
        sessions stay pi&apos;s; Orbit is the window onto them. A session you
        create in Orbit and one you create in the terminal are the same file.
      </p>

      <h2>How it works</h2>
      <pre>{`Orbit (Rust · GPUI)
   │  newline-delimited JSON RPC over stdio
   ▼
pi CLI (one child process per open session)
   │
   ▼
~/.pi/agent/   sessions · config · usage`}</pre>
      <p>
        Requests go to pi as JSON on stdin; pi streams events back as JSON
        lines on stdout — text and thinking deltas, tool calls, question
        dialogs, and settle signals. Because the runtime is pi itself, anything
        the CLI can do, Orbit can drive, and nothing is reimplemented twice.
      </p>

      <h2>What you need</h2>
      <ul>
        <li>
          <strong>pi</strong> — the coding agent CLI that does the work. Install
          it from the{" "}
          <a href="https://github.com/earendil-works/pi" target="_blank" rel="noreferrer">
            pi repository
          </a>
          .
        </li>
        <li>
          <strong>Node.js</strong> — needed by many MCP servers (run through{" "}
          <code>npx</code>) and by pi packages.
        </li>
        <li>
          <strong>Git</strong> — powers the Review pane, the Git page, and
          worktrees.
        </li>
      </ul>
      <p>
        On first launch Orbit checks for all three and reports anything missing
        along with the command to install it. You can revisit this any time from
        Settings.
      </p>

      <h2>Where your data lives</h2>
      <table>
        <thead>
          <tr>
            <th>Path</th>
            <th>What is there</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>
              <code>~/.pi/agent/sessions/</code>
            </td>
            <td>
              Your sessions — the same files the <code>pi</code> CLI reads and
              writes.
            </td>
          </tr>
          <tr>
            <td>
              <code>~/.pi/agent/</code>
            </td>
            <td>
              pi configuration, including its MCP config and credentials.
            </td>
          </tr>
          <tr>
            <td>
              <code>~/.orbit-pi/</code>
            </td>
            <td>
              Orbit preferences: layout, access mode, workflow mode,
              notifications, the worktree root, and the MCP secret store.
            </td>
          </tr>
          <tr>
            <td>
              <code>&lt;project&gt;/.pi/</code>
            </td>
            <td>
              Project-scoped pi configuration, including a project{" "}
              <code>mcp.json</code>.
            </td>
          </tr>
        </tbody>
      </table>

      <h2>Guides</h2>
      {DOCS_NAV.map((group) => (
        <div key={group.title} className="mt-8">
          <h3 style={{ marginTop: 0 }}>{group.title}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            {group.links
              .filter((link) => link.href !== "/docs")
              .map((link) => (
                <Link
                  key={link.href}
                  href={link.href}
                  className="group flex flex-col gap-1 rounded-[14px] border border-edge-subtle bg-surface-1 p-4 no-underline transition-colors hover:border-edge-strong hover:bg-surface-2"
                >
                  <span className="text-[14px] font-medium text-ink">
                    {link.label}
                  </span>
                  <span className="text-[13px] leading-[1.6] text-ink-2">
                    {link.description}
                  </span>
                </Link>
              ))}
          </div>
        </div>
      ))}

      <p className="mt-10 text-[13px] text-ink-3">
        Missing something? Open an issue on{" "}
        <a href={`${SITE.github}/issues`} target="_blank" rel="noreferrer">
          GitHub
        </a>
        .
      </p>
    </DocsPage>
  );
}
