import type { Metadata } from "next";

import { Callout, DocsPage } from "@/components/docs-page";

export const metadata: Metadata = {
  title: "Access modes",
  description:
    "How Orbit decides which tool calls the agent may run without asking: Supervised, Auto-accept edits, and Full access, the allowlist, and how approvals work.",
  alternates: { canonical: "/docs/access-modes" },
};

export default function AccessModesPage() {
  return (
    <DocsPage
      current="/docs/access-modes"
      title="Access modes"
      intro="An access mode is how much latitude the agent has. You choose one per session from the composer, and a bundled pi extension enforces it on every tool call the agent makes."
    >
      <h2>The three modes</h2>
      <p>
        Pick the mode from the access chip in the composer, next to the model and
        thinking selectors. The choice is remembered and can be changed at any
        time — including mid-session.
      </p>
      <table>
        <thead>
          <tr>
            <th>Mode</th>
            <th>Runs without asking</th>
            <th>Still asks before</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>
              <strong>Supervised</strong>
            </td>
            <td>Reads only.</td>
            <td>Every command and every file change.</td>
          </tr>
          <tr>
            <td>
              <strong>Auto-accept edits</strong>
            </td>
            <td>Reads and file edits.</td>
            <td>Commands, MCP calls, and other tools.</td>
          </tr>
          <tr>
            <td>
              <strong>Full access</strong>
            </td>
            <td>Everything.</td>
            <td>Nothing — the agent is never prompted.</td>
          </tr>
        </tbody>
      </table>
      <Callout>
        New sessions start in <strong>Full access</strong>. That is the agent
        behaving as it would in the terminal, with no guard in the way; tighten
        it to <strong>Supervised</strong> or{" "}
        <strong>Auto-accept edits</strong> whenever you want a confirmation step.
      </Callout>

      <h2>How a tool call is classified</h2>
      <p>
        The guard looks at the tool name and sorts each call into one of five
        kinds. Reads never mutate anything, so they always pass.
      </p>
      <table>
        <thead>
          <tr>
            <th>Kind</th>
            <th>Examples</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Read</td>
            <td>
              <code>read</code>, <code>grep</code>, <code>find</code>,{" "}
              <code>ls</code>, <code>glob</code>, <code>list</code>,{" "}
              <code>webfetch</code>
            </td>
          </tr>
          <tr>
            <td>Edit</td>
            <td>
              <code>edit</code>, <code>write</code>, <code>multiedit</code>,{" "}
              <code>apply_patch</code>, <code>str_replace_editor</code>
            </td>
          </tr>
          <tr>
            <td>Exec</td>
            <td>
              <code>bash</code>, <code>powershell</code>, <code>shell</code>,{" "}
              <code>exec</code>, <code>terminal</code>
            </td>
          </tr>
          <tr>
            <td>MCP</td>
            <td>
              any tool named <code>mcp__&lt;server&gt;__&lt;tool&gt;</code>
            </td>
          </tr>
          <tr>
            <td>Other</td>
            <td>
              pi extensions and custom tools — treated conservatively, so they
              ask in the confined modes.
            </td>
          </tr>
        </tbody>
      </table>
      <p>The decision for each kind:</p>
      <table>
        <thead>
          <tr>
            <th>Kind</th>
            <th>Supervised</th>
            <th>Auto-accept edits</th>
            <th>Full access</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Read</td>
            <td>Allow</td>
            <td>Allow</td>
            <td>Allow</td>
          </tr>
          <tr>
            <td>Edit</td>
            <td>Ask</td>
            <td>Allow</td>
            <td>Allow</td>
          </tr>
          <tr>
            <td>Exec</td>
            <td>Ask</td>
            <td>Ask</td>
            <td>Allow</td>
          </tr>
          <tr>
            <td>MCP</td>
            <td>Ask</td>
            <td>Ask</td>
            <td>Allow</td>
          </tr>
          <tr>
            <td>Other</td>
            <td>Ask</td>
            <td>Ask</td>
            <td>Allow</td>
          </tr>
        </tbody>
      </table>
      <p>
        There is intentionally no <em>block</em>. The guard prompts rather than
        silently denying, and a prompt that is dismissed counts as a denial.
      </p>

      <h2>Approving a call</h2>
      <p>
        When a call needs approval, a compact bar appears above the composer —
        not a blocking modal — so you can keep reading the transcript. It offers
        three choices:
      </p>
      <ul>
        <li>
          <strong>Allow once</strong> — run this call, and only this one.
        </li>
        <li>
          <strong>Always allow this tool</strong> — run it and stop asking for
          the same tool in this mode.
        </li>
        <li>
          <strong>Deny</strong> — refuse the call.
        </li>
      </ul>
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
              <kbd>↑</kbd> / <kbd>↓</kbd>
            </td>
            <td>Move between the three choices.</td>
          </tr>
          <tr>
            <td>
              <kbd>Enter</kbd>
            </td>
            <td>Confirm the highlighted choice.</td>
          </tr>
          <tr>
            <td>
              <kbd>Esc</kbd>
            </td>
            <td>Dismiss, which counts as a denial.</td>
          </tr>
        </tbody>
      </table>
      <p>
        After you answer, focus returns to the composer so you can keep typing.
        pi blocks one request at a time, so if a dialog or another approval is
        already open the new request is declined rather than queueing behind a
        prompt you cannot see.
      </p>

      <h2>The always-allow list</h2>
      <p>
        <strong>Always allow this tool</strong> records the exact tool name in{" "}
        <code>~/.orbit-pi/access-allow.json</code>, keyed by the active mode. Two
        things follow from that:
      </p>
      <ul>
        <li>
          The list is <strong>per mode</strong>: allowing a tool in Auto-accept
          edits does not allow it in Supervised.
        </li>
        <li>
          It matches on the <strong>tool name</strong>, not the arguments, so
          allowing <code>bash</code> lets the agent run any command without
          asking. Prefer <strong>Allow once</strong> for one-off commands.
        </li>
      </ul>
      <p>
        To start asking again, remove the tool from the list for that mode — the
        file is plain JSON:
      </p>
      <pre>{`{
  "auto-accept-edits": ["bash"],
  "supervised": []
}`}</pre>

      <h2>Where the mode lives</h2>
      <p>
        The active mode is stored in <code>~/.orbit-pi/access.json</code>:
      </p>
      <pre>{`{ "mode": "auto-accept-edits" }`}</pre>
      <p>
        The guard extension reads that file fresh on <em>every</em> tool call, so
        changing the mode re-arms sessions that are already running — there is no
        restart and no per-session copy to update. An unknown or malformed value
        falls back to the default, so a hand-edited file can never widen access by
        accident.
      </p>

      <h2>Access mode versus workflow mode</h2>
      <p>
        These are separate controls and both apply to a session. A{" "}
        <a href="/docs/sessions">workflow mode</a> decides which tools exist at
        all; the access mode decides which of the remaining ones can run without
        a prompt.
      </p>
      <table>
        <thead>
          <tr>
            <th>Control</th>
            <th>Question it answers</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Workflow mode (Plan / Build / Ask)</td>
            <td>Which tools is the agent allowed to use?</td>
          </tr>
          <tr>
            <td>Access mode (Supervised / Auto-accept / Full)</td>
            <td>Which of those tools can run without asking?</td>
          </tr>
        </tbody>
      </table>
      <p>
        In Plan and Ask the write tools are removed and <code>bash</code> is
        gated to a read-only allowlist, so a Supervised Build session and a Plan
        session differ in what the guard is even asked about.
      </p>

      <h2>What it is not</h2>
      <ul>
        <li>
          <strong>It is not a sandbox.</strong> It is a confirmation guard around
          tool calls. pi ships no sandbox, and Orbit does not add one. Full
          access, or an always-allowed <code>bash</code>, gives the agent
          everything your user account can do.
        </li>
        <li>
          <strong>The guard has to be installed.</strong> Orbit ships it as a pi
          extension and loads it into every session. If it could not be
          installed, changing the mode has no effect and the app says so instead
          of implying it took.
        </li>
        <li>
          <strong>Dismissing is denying.</strong> A prompt that is closed,
          cancelled, or lost when a session is replaced is treated as a refusal,
          never as approval.
        </li>
      </ul>
    </DocsPage>
  );
}
