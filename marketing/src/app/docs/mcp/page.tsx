import type { Metadata } from "next";

import { Callout, DocsPage } from "@/components/docs-page";

export const metadata: Metadata = {
  title: "MCP servers",
  description:
    "Manage the Model Context Protocol servers the pi agent can call: scopes, secrets, exposure, OAuth sign-in, and how changes are applied.",
  alternates: { canonical: "/docs/mcp" },
};

export default function McpPage() {
  return (
    <DocsPage
      current="/docs/mcp"
      title="MCP servers"
      intro="MCP (Model Context Protocol) lets the agent call external tools. pi owns the protocol; Orbit owns the management surface — a native page, a local secret store, live status, and a safe way to apply changes."
    >
      <h2>Where MCP shows up</h2>
      <ul>
        <li>
          <strong>Settings → MCP</strong> — the full page: every server, its
          tools, errors, and actions.
        </li>
        <li>
          <strong>The usage pill</strong> in the top bar opens a popover with{" "}
          <strong>Providers</strong> and <strong>MCP</strong> tabs. The MCP pane
          is one row per server — status, scope, tool count, and a contextual
          sign-in button.
        </li>
        <li>
          <strong>The status bar</strong> carries a quiet <code>MCP n/N</code>{" "}
          chip whenever servers are enabled; click it to open the page.
        </li>
        <li>
          <strong>Transcript cards</strong> show the actual calls —{" "}
          <code>MCP · &lt;server&gt;</code> for direct and deferred tools, and{" "}
          <code>Run code</code> for codemode scripts.
        </li>
      </ul>

      <h2>Scopes and files</h2>
      <p>
        Orbit reads and writes exactly the files pi reads, so a config edited
        here is visible to the CLI and vice versa:
      </p>
      <table>
        <thead>
          <tr>
            <th>Scope</th>
            <th>File</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Global</td>
            <td>
              <code>~/.pi/agent/mcp.json</code> — available in every project.
            </td>
          </tr>
          <tr>
            <td>This project</td>
            <td>
              <code>&lt;project&gt;/.pi/mcp.json</code> — available only in that
              project. Project entries replace global entries with the same name.
            </td>
          </tr>
        </tbody>
      </table>

      <h2>Adding a server</h2>
      <p>
        Open <strong>Settings → MCP</strong> and choose{" "}
        <strong>Add MCP Server</strong>, then pick a transport:
      </p>
      <table>
        <thead>
          <tr>
            <th>Transport</th>
            <th>Fields</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>stdio</td>
            <td>
              <code>command</code> (a single executable, never a shell string),{" "}
              <code>args</code>, <code>env</code>, and <code>cwd</code>.
            </td>
          </tr>
          <tr>
            <td>HTTP</td>
            <td>
              <code>url</code>, <code>headers</code>, and optional{" "}
              <code>oauth</code>. The legacy SSE transport is not supported.
            </td>
          </tr>
        </tbody>
      </table>
      <p>
        You can also set <code>enabled</code> (off keeps the entry without
        connecting) and an <code>exposure</code> mode. Descriptions are not part
        of pi&apos;s schema, so Orbit keeps them in{" "}
        <code>~/.orbit-pi/mcp-metadata.json</code> and never writes them into{" "}
        <code>mcp.json</code>.
      </p>

      <h2>Secrets</h2>
      <p>
        <code>mcp.json</code> is configuration, not a credential store. Orbit
        writes <code>{"${NAME}"}</code> references and keeps the values in{" "}
        <code>~/.orbit-pi/mcp-secrets.json</code>:
      </p>
      <ul>
        <li>
          A literal value typed into an Environment or Headers field is stored
          under a generated name and the field is written as a reference. Global
          and per-project secrets never share a stored value, even for the same
          server and field.
        </li>
        <li>
          Values that already contain a <code>{"${NAME}"}</code> reference (or a{" "}
          <code>!command</code>) are written through verbatim.
        </li>
        <li>
          Every pi spawn and probe receives the store&apos;s values as
          environment variables, so pi expands references as it would for a
          shell export. Values are masked in the UI, and errors are redacted
          before they reach the screen or a log.
        </li>
        <li>
          The store is <code>0600</code> on Unix and lives in your user profile
          on Windows — the same local, user-scoped model as pi&apos;s own
          credential files.
        </li>
      </ul>

      <Callout>
        Project trust is the boundary. Orbit spawns sessions with{" "}
        <code>--approve</code>, so a project&apos;s configuration runs with the
        same access a terminal session would grant it. Keep credentials out of
        untrusted projects.
      </Callout>

      <h2>Applying changes</h2>
      <p>A configuration change follows one path:</p>
      <pre>{`validate → persist (atomic write) → re-read → probe → restart pi if stale
         → switch_session back to the same conversation`}</pre>
      <ul>
        <li>
          Writes are atomic and preserve unrelated entries and the file&apos;s
          indentation; a parse or I/O error aborts untouched.
        </li>
        <li>
          The probe runs <code>pi mcp list --json</code> on a background
          executor, debounced, so it never blocks rendering.
        </li>
        <li>
          Applying is coalesced and waits for an in-flight run to settle, so a
          restart can never abort work. pi is restarted only when the running
          process is actually stale, and the session is preserved with{" "}
          <code>switch_session</code>.
        </li>
        <li>
          External edits to <code>mcp.json</code> are detected while the page is
          open. Orbit shows a notice and applies them only when you ask — it
          never overwrites them.
        </li>
      </ul>

      <h2>Exposure modes</h2>
      <p>
        Exposure controls how a server&apos;s tools reach the model. Orbit
        writes the mode; pi implements the tools.
      </p>
      <table>
        <thead>
          <tr>
            <th>Mode</th>
            <th>Meaning</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Codemode (default)</td>
            <td>
              Tools stay out of the model&apos;s tool list and are called from
              pi&apos;s JavaScript <code>codemode</code> sandbox.
            </td>
          </tr>
          <tr>
            <td>Codemode-deferred</td>
            <td>Like codemode, loaded on demand.</td>
          </tr>
          <tr>
            <td>Deferred</td>
            <td>
              Tools are undeclared until pi&apos;s <code>tool_search</code> loads
              them.
            </td>
          </tr>
          <tr>
            <td>Direct</td>
            <td>Tools are declared to the model directly.</td>
          </tr>
          <tr>
            <td>Hidden</td>
            <td>The server connects but contributes no tools.</td>
          </tr>
        </tbody>
      </table>

      <h2>OAuth sign-in</h2>
      <p>
        pi is the OAuth client. An HTTP server without an{" "}
        <code>Authorization</code> header is OAuth-capable, and no credential
        belongs in <code>mcp.json</code>. When such a server needs a login, its
        row shows <strong>Sign-in required</strong> with a{" "}
        <strong>Sign in</strong> button:
      </p>
      <ul>
        <li>
          <strong>Sign in</strong> runs <code>pi mcp login &lt;server&gt;</code>.
          pi opens the authorization page, handles the loopback callback, and
          stores tokens in <code>~/.pi/agent/mcp-auth.json</code>. Orbit waits
          and offers <strong>Cancel</strong>.
        </li>
        <li>
          <strong>Sign out</strong> (in the expanded detail) runs{" "}
          <code>pi mcp logout &lt;server&gt;</code> and restarts a running
          session so live access is dropped too.
        </li>
        <li>
          For authorization servers without dynamic client registration, the
          form has collapsed OAuth client settings for a client id, secret,
          callback port or URL, and scopes.
        </li>
      </ul>

      <h2>Troubleshooting</h2>
      <ul>
        <li>
          <strong>Connection failed with ENOENT</strong> — the stdio command is
          not installed or not on pi&apos;s <code>PATH</code>. Install it (for
          example Node.js for <code>npx</code>) or use an absolute path.
        </li>
        <li>
          <strong>Connection failed with 401 or 403</strong> — check the{" "}
          <code>Authorization</code> header and the secret it references. Use{" "}
          <strong>Test connection</strong>; the toast carries pi&apos;s status.
        </li>
        <li>
          <strong>Sign-in required</strong> — an OAuth server. Use{" "}
          <strong>Sign in</strong>; the next probe picks up the credentials.
        </li>
        <li>
          <strong>Project servers missing</strong> — the standalone{" "}
          <code>pi mcp list</code> may not trust the project. Orbit sessions pass{" "}
          <code>--approve</code>, so they still load; the page shows pi&apos;s
          note.
        </li>
        <li>
          <strong>Tools do not reach the model</strong> — check the server&apos;s
          exposure and the global <code>autoEnableCodemode</code> setting.
        </li>
        <li>
          <strong>Logs</strong> — pi appends server logs to{" "}
          <code>~/.pi/agent/mcp.log</code>. Orbit adds no secret values to any
          log.
        </li>
      </ul>
    </DocsPage>
  );
}
