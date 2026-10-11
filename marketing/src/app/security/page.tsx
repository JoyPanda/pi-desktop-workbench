import type { Metadata } from "next";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Security",
  description:
    "How to report a vulnerability in Orbit, what is in scope, and the security model behind the app's local agent access.",
  alternates: { canonical: "/security" },
};

export default function SecurityPage() {
  return (
    <LegalPage
      label="Trust"
      title="Security"
      updated="October 10, 2026"
      intro="Orbit runs an AI agent with local tool access on your machine, so its security model is worth stating plainly. Here is how to report a problem and what counts as one."
    >
      <h2>Supported versions</h2>
      <p>
        Orbit is pre-1.0. Security fixes are made against the latest release and
        the <code>main</code> branch.
      </p>
      <ul>
        <li>
          <strong>main</strong> (latest) — supported
        </li>
        <li>
          <strong>Latest tagged release</strong> — supported
        </li>
        <li>
          <strong>Older releases</strong> — not supported
        </li>
      </ul>

      <h2>Reporting a vulnerability</h2>
      <p>
        Please report vulnerabilities <strong>privately</strong> — do not open a
        public issue. Use GitHub&apos;s{" "}
        <a
          href={`${SITE.github}/security/advisories/new`}
          target="_blank"
          rel="noreferrer"
        >
          private vulnerability reporting
        </a>
        , or email{" "}
        <a href={`mailto:${SITE.email}`}>{SITE.email}</a>.
      </p>

      <h3>What to include</h3>
      <ul>
        <li>a description of the issue and its impact;</li>
        <li>steps to reproduce, or a proof of concept;</li>
        <li>
          the Orbit version, your OS version, and how Orbit was installed (
          <code>cargo run</code>, a <code>.app</code> bundle, or a DMG);
        </li>
        <li>
          the <code>pi</code> CLI version (<code>pi --version</code>).
        </li>
      </ul>

      <h3>What to expect</h3>
      <p>
        You can expect an acknowledgement within a few days and a status update
        as the report is triaged. Please give a reasonable window to ship a fix
        before any public disclosure.
      </p>

      <h2>Scope and security model</h2>
      <p>
        Orbit is a local desktop client. It spawns the <code>pi</code> coding
        agent as a child process and gives it the same local tool access you
        would have in a terminal. A few properties are <strong>by design</strong>{" "}
        and are not vulnerabilities on their own:
      </p>
      <ul>
        <li>
          <strong>Orbit runs the agent with full local tool access.</strong> The
          agent can read, edit, and run commands in your workspace as your user.
          This is the point of the tool.
        </li>
        <li>
          <strong>Access modes are a confirmation guard, not a sandbox.</strong>{" "}
          Supervised, Auto-accept edits, and Full access decide which mutating
          tool calls prompt first. <code>pi</code> ships no sandbox and Orbit
          does not add one. A mode that auto-approves a call is not an isolation
          boundary.
        </li>
        <li>
          <strong>Session data lives in pi&apos;s own store</strong> (
          <code>~/.pi/agent/</code>, <code>~/.orbit-pi/</code>). Orbit reads and
          writes the same files the <code>pi</code> CLI does.
        </li>
      </ul>
      <p>Reports we do want:</p>
      <ul>
        <li>memory-safety bugs;</li>
        <li>
          command or prompt injection that crosses a boundary Orbit claims to
          enforce;
        </li>
        <li>credentials leaking across the RPC surface;</li>
        <li>the signed updater accepting an unverifiable artifact.</li>
      </ul>
    </LegalPage>
  );
}
