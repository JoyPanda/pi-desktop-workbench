import type { Metadata } from "next";

import { Callout, DocsPage } from "@/components/docs-page";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Install",
  description:
    "Download Orbit for macOS, Windows, or Linux, clear the first-run dependency check, connect a provider, and send your first prompt.",
  alternates: { canonical: "/docs/getting-started" },
};

const RELEASES = `${SITE.github}/releases/latest`;

export default function GettingStartedPage() {
  return (
    <DocsPage
      current="/docs/getting-started"
      title="Install and first run"
      intro="Orbit ships signed builds for macOS, Windows, and Linux, with signed in-app updates. This page takes you from download to a first prompt."
    >
      <h2>Download</h2>
      <p>
        Grab the build for your platform from the{" "}
        <a href={RELEASES} target="_blank" rel="noreferrer">
          latest release
        </a>
        . The <strong>Download</strong> button in the header of this site picks
        the right one automatically.
      </p>
      <table>
        <thead>
          <tr>
            <th>Platform</th>
            <th>Formats</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>macOS</td>
            <td>
              <code>.dmg</code> (signed and notarized, universal) and{" "}
              <code>.tar.gz</code>
            </td>
          </tr>
          <tr>
            <td>Windows</td>
            <td>
              <code>.exe</code> and <code>.zip</code>
            </td>
          </tr>
          <tr>
            <td>Linux</td>
            <td>
              <code>.deb</code> and <code>.tar.gz</code>
            </td>
          </tr>
        </tbody>
      </table>

      <Callout>
        Orbit bundles none of the agent runtime. It drives the <code>pi</code>{" "}
        CLI you already have, so the two update independently.
      </Callout>

      <h2>Requirements</h2>
      <p>
        Orbit drives tools that live outside the app. Install these first, or
        let the first-run check tell you what is missing:
      </p>
      <ul>
        <li>
          <strong>pi</strong> — the coding agent CLI. Orbit spawns it as a child
          process for every session.
        </li>
        <li>
          <strong>Node.js</strong> — many MCP servers run through{" "}
          <code>npx</code>, and pi packages run on Node.
        </li>
        <li>
          <strong>Git</strong> — for the Review pane, the Git page, and
          worktrees.
        </li>
      </ul>

      <h2>First launch</h2>
      <ol>
        <li>
          <strong>Open Orbit.</strong> On first run it checks for{" "}
          <code>pi</code>, <code>node</code>, and <code>git</code> and lists
          anything it cannot find with the command to install it.
        </li>
        <li>
          <strong>Connect a provider.</strong> Open{" "}
          <strong>Settings → Providers</strong>, then sign in with an API key or
          OAuth. Orbit reads pi&apos;s live model catalog, so this list matches
          what the CLI can reach.
        </li>
        <li>
          <strong>Choose a workspace.</strong> A workspace is just a project
          folder. <strong>New task</strong> has a workspace picker with recent
          folders and a native browse button.
        </li>
        <li>
          <strong>Pick a model and thinking level.</strong> Settings → Agent
          sets the default model and thinking level every new session starts
          on.
        </li>
        <li>
          <strong>Choose an access mode.</strong> Supervised asks before every
          mutating call; Auto-accept edits applies file edits but still asks
          before commands; Full access runs without prompting.
        </li>
        <li>
          <strong>Send a prompt.</strong> Press <kbd>Enter</kbd> to send, or{" "}
          <kbd>Shift</kbd>+<kbd>Enter</kbd> for a new line.
        </li>
      </ol>

      <h2>Updates</h2>
      <p>
        Release builds check for updates in the background and install them with
        a signed in-app updater. Run <strong>Check for updates</strong> manually
        with <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>U</kbd> (
        <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>U</kbd> on Windows and Linux), or
        open the Version History from the updater dialog.
      </p>

      <h2>Next</h2>
      <p>
        With the app running, the{" "}
        <a href="/docs/sessions">Sessions</a> guide covers the day-to-day
        workbench, and <a href="/docs/shortcuts">Keyboard shortcuts</a> has the
        full keymap.
      </p>
    </DocsPage>
  );
}
