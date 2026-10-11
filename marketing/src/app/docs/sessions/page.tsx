import type { Metadata } from "next";

import { DocsPage } from "@/components/docs-page";

export const metadata: Metadata = {
  title: "Sessions",
  description:
    "How Orbit organizes projects and sessions, the workflow and access modes, steering versus queueing, navigation, and background notifications.",
  alternates: { canonical: "/docs/sessions" },
};

export default function SessionsPage() {
  return (
    <DocsPage
      current="/docs/sessions"
      title="Sessions"
      intro="A session is one conversation with the agent. Orbit groups sessions by project, keeps them as plain pi session files, and gives you the keyboard to move between them."
    >
      <h2>Projects and sessions</h2>
      <p>
        The sidebar groups sessions under the project they belong to. A project
        is simply a workspace folder; Orbit keeps its own project list so you can
        add and remove workspaces without touching pi. Removing a project from
        the sidebar never deletes the sessions underneath it.
      </p>
      <p>
        Every session is a file under <code>~/.pi/agent/sessions/</code> — the
        same store the <code>pi</code> CLI uses. A session started in Orbit can
        be resumed from the terminal and vice versa.
      </p>

      <h2>Starting a session</h2>
      <ol>
        <li>
          Press <kbd>⌘</kbd>+<kbd>N</kbd> (<kbd>Ctrl</kbd>+<kbd>N</kbd>) or click{" "}
          <strong>New task</strong> in the sidebar.
        </li>
        <li>
          Pick a workspace. Recent folders are listed, and the browse button
          opens a native folder picker.
        </li>
        <li>
          Choose a model, a thinking level, and an access mode from the composer.
        </li>
        <li>
          Type a prompt and press <kbd>Enter</kbd>. Orbit spawns a{" "}
          <code>pi</code> process for the session and streams the reply.
        </li>
      </ol>

      <h2>Workflow modes</h2>
      <p>
        Scope a session to how you want the agent to behave. The mode is stored
        per session and enforced by a bundled pi extension.
      </p>
      <table>
        <thead>
          <tr>
            <th>Mode</th>
            <th>Behavior</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Plan</td>
            <td>
              Read-only. Write tools are dropped and <code>bash</code> is
              gated, so the agent can explore and propose without changing
              anything.
            </td>
          </tr>
          <tr>
            <td>Build</td>
            <td>The full toolset. Edits and commands run subject to the access mode.</td>
          </tr>
          <tr>
            <td>Ask</td>
            <td>
              Read-only, like Plan, but without the planning framing — good for
              questions and explanations.
            </td>
          </tr>
        </tbody>
      </table>

      <h2>Access modes</h2>
      <p>
        The access mode decides which tool calls run without asking. Choose it
        from the composer&apos;s access chip: <strong>Supervised</strong> asks
        before every command and file change,{" "}
        <strong>Auto-accept edits</strong> applies file edits but still asks
        before commands, and <strong>Full access</strong> never prompts. When a
        call needs approval, an inline bar above the composer offers Allow once,
        Always allow this tool, or Deny.
      </p>
      <p>
        <a href="/docs/access-modes">Access modes</a> has the full decision
        table, the per-mode allowlist, and why this is a guard rather than a
        sandbox.
      </p>

      <h2>Sending: queue or steer</h2>
      <p>
        While the agent is working, <kbd>Enter</kbd> either queues a follow-up
        or steers the running task. Pick the default in{" "}
        <strong>Settings → Agent → Behavior</strong>; <kbd>Alt</kbd>+<kbd>Enter</kbd>{" "}
        uses the other mode, and{" "}
        <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>Enter</kbd>{" "}
        (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd>) always steers.
      </p>
      <ul>
        <li>
          <strong>Queue</strong> delivers the message after the current turn
          settles.
        </li>
        <li>
          <strong>Steer</strong> takes effect once the current response and its
          tools finish — it does not cut a tool call off mid-flight.
        </li>
        <li>
          <kbd>Shift</kbd>+<kbd>Enter</kbd> inserts a newline, and when idle{" "}
          <kbd>Enter</kbd> sends normally.
        </li>
      </ul>

      <h2>Organizing</h2>
      <ul>
        <li>
          <strong>Rename</strong>, <strong>pin</strong>, <strong>clone</strong>,{" "}
          <strong>delete</strong>, or <strong>copy the id</strong> from a
          session&apos;s row menu. Pinned sessions sort first.
        </li>
        <li>
          <strong>Sort projects</strong> by last activity, recently added, name,
          or session count.
        </li>
        <li>
          Sessions are cross-workspace: one project can hold sessions from any
          folder, and switching workspace switches what the sidebar, Explorer,
          terminal, Git page, and agent operate on.
        </li>
      </ul>

      <h2>Moving between sessions</h2>
      <table>
        <thead>
          <tr>
            <th>Keys</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>1</kbd> … <kbd>⌘</kbd>+<kbd>9</kbd>
            </td>
            <td>Open the Nth visible session in the sidebar order.</td>
          </tr>
          <tr>
            <td>
              <kbd>Ctrl</kbd>+<kbd>Tab</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>
              +<kbd>Tab</kbd>
            </td>
            <td>Cycle to the next / previous session.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>B</kbd>
            </td>
            <td>Move keyboard focus to the sessions list.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>P</kbd> or <kbd>⌘</kbd>+<kbd>K</kbd>
            </td>
            <td>Open the command palette to jump to a session or command.</td>
          </tr>
        </tbody>
      </table>
      <p>
        On Windows and Linux, replace <kbd>⌘</kbd> with <kbd>Ctrl</kbd>. See{" "}
        <a href="/docs/shortcuts">Keyboard shortcuts</a> for the complete list.
      </p>

      <h2>Live updates and notifications</h2>
      <ul>
        <li>
          Sessions written by the CLI or another Orbit window refresh in the
          sidebar live. <kbd>⌘</kbd>+<kbd>R</kbd> reloads them from disk.
        </li>
        <li>
          When a turn settles and the window is not frontmost, Orbit shows a
          desktop banner and plays the alert sound. If Orbit is frontmost you
          get an in-app toast instead.
        </li>
        <li>
          If a run is waiting on an extension dialog, Orbit raises a heads-up so
          the prompt is never silently stuck.
        </li>
      </ul>
    </DocsPage>
  );
}
