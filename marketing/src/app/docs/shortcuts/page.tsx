import type { Metadata } from "next";

import { DocsPage } from "@/components/docs-page";

export const metadata: Metadata = {
  title: "Keyboard shortcuts",
  description:
    "Every Orbit keyboard shortcut, grouped by the surface it works in — sessions, panels, the composer, the review pane, and the Git page.",
  alternates: { canonical: "/docs/shortcuts" },
};

export default function ShortcutsPage() {
  return (
    <DocsPage
      current="/docs/shortcuts"
      title="Keyboard shortcuts"
      intro="Orbit is keyboard-first. Shortcuts are contextual: the same key does the sensible thing for whatever surface has focus."
    >
      <p>
        Where a shortcut shows <kbd>⌘</kbd>, use <kbd>Ctrl</kbd> on Windows and
        Linux. <kbd>Ctrl</kbd>+<kbd>Tab</kbd> is literal on every platform.
        Press <kbd>⌘</kbd>+<kbd>/</kbd> (<kbd>Ctrl</kbd>+<kbd>/</kbd>) inside
        the app to open this reference as a page.
      </p>

      <h2>General</h2>
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
              <kbd>⌘</kbd>+<kbd>P</kbd> or <kbd>⌘</kbd>+<kbd>K</kbd>
            </td>
            <td>Open the command palette (sessions, commands, settings).</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>,</kbd>
            </td>
            <td>Open Settings.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>F</kbd>
            </td>
            <td>Find in the transcript.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>C</kbd>
            </td>
            <td>Copy the newest assistant response.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>/</kbd>
            </td>
            <td>Open this shortcut reference.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>U</kbd>
            </td>
            <td>Check for updates.</td>
          </tr>
          <tr>
            <td>
              <kbd>Esc</kbd> or <kbd>⌘</kbd>+<kbd>.</kbd>
            </td>
            <td>Close the top surface; with the composer focused, abort the run.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>Q</kbd>
            </td>
            <td>Quit Orbit.</td>
          </tr>
        </tbody>
      </table>

      <h2>Sessions</h2>
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
              <kbd>⌘</kbd>+<kbd>N</kbd>
            </td>
            <td>New session.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>R</kbd>
            </td>
            <td>Reload the session list from disk.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>B</kbd>
            </td>
            <td>Move focus to the sessions list.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>1</kbd> … <kbd>⌘</kbd>+<kbd>9</kbd>
            </td>
            <td>Open the Nth visible session.</td>
          </tr>
          <tr>
            <td>
              <kbd>Ctrl</kbd>+<kbd>Tab</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>
              +<kbd>Tab</kbd>
            </td>
            <td>Next / previous session.</td>
          </tr>
        </tbody>
      </table>

      <h2>Panels and navigation</h2>
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
              <kbd>⌘</kbd>+<kbd>B</kbd>
            </td>
            <td>Toggle the sidebar.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>E</kbd>
            </td>
            <td>Toggle the project panel (Explorer).</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>J</kbd>
            </td>
            <td>Toggle the terminal.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>R</kbd>
            </td>
            <td>Open the Review pane.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>G</kbd>
            </td>
            <td>Open the Git page.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>U</kbd>
            </td>
            <td>Open Usage.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>↑</kbd> / <kbd>⌘</kbd>+<kbd>↓</kbd>
            </td>
            <td>Jump to the previous / next user turn.</td>
          </tr>
        </tbody>
      </table>

      <h2>Agent</h2>
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
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>M</kbd>
            </td>
            <td>Choose the model.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>T</kbd>
            </td>
            <td>Choose the thinking level.</td>
          </tr>
          <tr>
            <td>
              <kbd>Esc</kbd>
            </td>
            <td>Abort the running turn.</td>
          </tr>
        </tbody>
      </table>

      <h2>Composer</h2>
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
              <kbd>Enter</kbd>
            </td>
            <td>Send. While the agent works, queue or steer — your default.</td>
          </tr>
          <tr>
            <td>
              <kbd>Alt</kbd>+<kbd>Enter</kbd>
            </td>
            <td>Use the other sending mode for this message.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>Enter</kbd>
            </td>
            <td>Always steer.</td>
          </tr>
          <tr>
            <td>
              <kbd>Shift</kbd>+<kbd>Enter</kbd>
            </td>
            <td>Insert a newline.</td>
          </tr>
          <tr>
            <td>
              <kbd>Tab</kbd>
            </td>
            <td>Accept the autocomplete suggestion.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>Z</kbd> / <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>Z</kbd>
            </td>
            <td>Undo / redo.</td>
          </tr>
        </tbody>
      </table>

      <h2>Review pane</h2>
      <p>These apply while the diff tree has focus.</p>
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
            <td>Move between rows.</td>
          </tr>
          <tr>
            <td>
              <kbd>Enter</kbd>
            </td>
            <td>Open a file or fold a folder.</td>
          </tr>
          <tr>
            <td>
              <kbd>n</kbd> / <kbd>p</kbd>
            </td>
            <td>Next / previous changed file.</td>
          </tr>
          <tr>
            <td>
              <kbd>]</kbd> / <kbd>[</kbd>
            </td>
            <td>Next / previous hunk.</td>
          </tr>
          <tr>
            <td>
              <kbd>e</kbd> / <kbd>c</kbd>
            </td>
            <td>Expand / collapse all files.</td>
          </tr>
          <tr>
            <td>
              <kbd>Esc</kbd>
            </td>
            <td>Back to the chat.</td>
          </tr>
        </tbody>
      </table>

      <h2>Git page tabs</h2>
      <table>
        <thead>
          <tr>
            <th>Keys</th>
            <th>Tab</th>
          </tr>
        </thead>
        <tbody>
          {[
            ["1", "Changes"],
            ["2", "History"],
            ["3", "Graph"],
            ["4", "Stashes"],
            ["5", "Issues"],
            ["6", "Pull requests"],
          ].map(([n, tab]) => (
            <tr key={n}>
              <td>
                <kbd>⌘</kbd>+<kbd>⌥</kbd>+<kbd>{n}</kbd>
              </td>
              <td>{tab}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2>Files and the editor</h2>
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
              <kbd>⌘</kbd>+<kbd>W</kbd>
            </td>
            <td>Close the active file tab (the last one closes the Files surface).</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>W</kbd>
            </td>
            <td>Close the Files surface.</td>
          </tr>
          <tr>
            <td>
              <kbd>⌘</kbd>+<kbd>S</kbd>
            </td>
            <td>Save now (autosave is on by default).</td>
          </tr>
        </tbody>
      </table>

      <h2>Pickers, menus, and dialogs</h2>
      <p>
        Every picker — model, thinking level, branches, the composer&apos;s plus
        menu, the access-mode menu, and dialogs — shares one set of keys:
      </p>
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
            <td>Move the selection.</td>
          </tr>
          <tr>
            <td>
              <kbd>Enter</kbd>
            </td>
            <td>Confirm.</td>
          </tr>
          <tr>
            <td>
              <kbd>Esc</kbd>
            </td>
            <td>Cancel.</td>
          </tr>
        </tbody>
      </table>
      <p>
        The access-approval bar above the composer uses the same keys, with{" "}
        <kbd>Enter</kbd> to allow and <kbd>Esc</kbd> to deny.
      </p>
    </DocsPage>
  );
}
