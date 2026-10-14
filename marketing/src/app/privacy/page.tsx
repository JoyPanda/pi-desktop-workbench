import type { Metadata } from "next";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Privacy",
  description:
    "How Orbit handles your data — local-first by default. The desktop app collects no telemetry; this website uses self-hosted, cookie-free analytics.",
  alternates: { canonical: "/privacy" },
};

export default function PrivacyPage() {
  return (
    <LegalPage
      label="Legal"
      title="Privacy"
      updated="October 14, 2026"
      intro="Orbit is local-first. Your source code, projects, terminal, and AI conversations are yours and stay on your machine. The desktop app collects no telemetry; this page explains what leaves your machine when you use Orbit and the small amount of analytics this website uses."
    >
      <h2>The short version</h2>
      <ul>
        <li>
          The desktop app collects no telemetry — no analytics client, no
          installation or session identifier, and no usage reporting.
        </li>
        <li>
          Orbit never intentionally collects your source code, file names,
          terminal contents, prompts, conversations, or credentials.
        </li>
        <li>
          This website uses self-hosted, cookie-free analytics loaded only in
          production.
        </li>
        <li>
          There are no ads, no third-party trackers, and no cross-site
          profiling.
        </li>
      </ul>

      <h2>Desktop app</h2>
      <p>
        Orbit ships no telemetry. There is no analytics crate or client, no
        installation or session identifier, no usage event stream, and no
        third-party tracking SDK. Nothing about how you use the app is sent to
        the project or to anyone else.
      </p>
      <p>
        Orbit talks to the network only for things you asked for: checking for
        and downloading updates, fetching GitHub metadata shown in the Git and
        GitHub panels, and sending prompts to the model providers your{" "}
        <code>pi</code> agent is configured to use. None of these carry a
        telemetry identifier.
      </p>

      <h3>What Orbit does not intentionally collect</h3>
      <ul>
        <li>source code, file names, file contents, or project and repository paths;</li>
        <li>terminal commands or terminal output, or shell history;</li>
        <li>AI prompts or responses, or conversation contents;</li>
        <li>API keys, access tokens, passwords, MCP credentials, or auth cookies;</li>
        <li>environment variables or clipboard contents;</li>
        <li>
          your IP address as an analytics property, exact location, or private
          URLs;
        </li>
        <li>machine hostname, MAC address, serial numbers, or disk identifiers.</li>
      </ul>
      <p>
        Prompts and tool output you send are handled by the model provider you
        configured and are governed by that provider&apos;s policy, not this
        one.
      </p>

      <h3>Local data</h3>
      <p>
        Orbit keeps its settings and state under <code>~/.orbit-pi/</code> and
        reads session data that <code>pi</code> writes. These stay on your
        machine. Settings → Privacy holds session retention, which archives or
        deletes old sessions on a schedule you choose; it never uploads them.
      </p>

      <h2>This website</h2>
      <p>
        {SITE.url.replace("https://", "")} loads a small, self-hosted analytics
        script <strong>only in production builds</strong> — never in
        development. It is used to understand, in aggregate, which pages and
        releases people read.
      </p>
      <p>The script records:</p>
      <ul>
        <li>the page path, query string, and referrer;</li>
        <li>your browser language and screen size;</li>
        <li>
          a random visitor identifier stored in <code>localStorage</code> — not
          a cookie.
        </li>
      </ul>
      <p>
        It does not use advertising cookies, does not fingerprint your device,
        and does not track you across other websites. If you set your browser to
        block scripts or clear local storage, the site works normally without
        it.
      </p>

      <h2>Third-party services</h2>
      <ul>
        <li>
          <strong>Downloads and issue tracking</strong> run on GitHub. Visiting
          GitHub links is governed by GitHub&apos;s own privacy policy.
        </li>
        <li>
          <strong>Model providers</strong> you configure in Orbit are your own
          accounts. Prompts you send to them are governed by those providers&apos;
          terms and privacy policies, not this one.
        </li>
      </ul>

      <h2>Retention and sharing</h2>
      <p>
        Website analytics are self-hosted on infrastructure operated by the
        project and are not sold, rented, or shared with advertisers or data
        brokers. Data is retained only as long as it is useful for aggregate
        product decisions. Because the desktop app collects nothing, there is no
        app data to retain or share.
      </p>

      <h2>Changes to this policy</h2>
      <p>
        If this policy changes in a material way, the date at the top of this
        page will change. Continued use of Orbit after an update means you
        accept the revised policy.
      </p>

      <h2>Contact</h2>
      <p>
        Questions about privacy? Email{" "}
        <a href={`mailto:${SITE.email}`}>{SITE.email}</a>.
      </p>
    </LegalPage>
  );
}
