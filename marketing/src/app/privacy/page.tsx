import type { Metadata } from "next";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Privacy",
  description:
    "How Orbit handles your data — local-first by default, with optional, self-hosted, privacy-first analytics for the desktop app and this website.",
  alternates: { canonical: "/privacy" },
};

export default function PrivacyPage() {
  return (
    <LegalPage
      label="Legal"
      title="Privacy"
      updated="October 10, 2026"
      intro="Orbit is local-first. Your source code, projects, terminal, and AI conversations are yours and stay on your machine. This page explains the small amounts of usage data the desktop app and this website may collect, and how to turn it off."
    >
      <h2>The short version</h2>
      <ul>
        <li>
          Orbit never intentionally collects your source code, file names,
          terminal contents, prompts, conversations, or credentials.
        </li>
        <li>
          The desktop app sends a small, optional stream of anonymous product
          usage to a self-hosted server. It is on by default in release builds
          and can be switched off at any time.
        </li>
        <li>
          This website uses self-hosted, cookie-free analytics loaded only in
          production.
        </li>
        <li>
          There are no ads, no third-party trackers, and no cross-site
          profiling. Orbit works exactly the same with analytics disabled.
        </li>
      </ul>

      <h2>Desktop app analytics</h2>
      <p>
        Release builds can send a limited set of anonymous usage information to
        a self-hosted analytics server. This is <strong>on by default in
        release builds and off in development builds</strong>, and you can turn
        it off at any time in <strong>Settings → Privacy → Anonymous Usage
        Analytics</strong>.
      </p>
      <p>When enabled, Orbit may send:</p>
      <ul>
        <li>
          a <strong>pseudonymous installation identifier</strong> — a random
          UUID generated on first run and stored locally in{" "}
          <code>~/.orbit-pi/analytics.json</code>;
        </li>
        <li>
          a <strong>session identifier</strong> — a new random UUID for each
          launch;
        </li>
        <li>
          the app version, operating system, CPU architecture, and interface
          locale;
        </li>
        <li>
          a small, closed set of feature events: the app started or closed; a
          project was added, opened, or removed; the terminal was opened; an
          agent run started, completed, or failed; an MCP server connected,
          disconnected, or failed; the command palette or settings opened; the
          theme or a setting changed; and an update was available or installed.
        </li>
      </ul>
      <p>
        Event details are limited to coarse values — for example, a failure{" "}
        <em>category</em> such as <code>provider</code>, or a theme{" "}
        <em>mode</em> such as <code>dark</code>. No free-form text is ever sent.
      </p>

      <h3>The installation identifier is pseudonymous</h3>
      <p>
        The installation identifier is a random number. It is not derived from
        your email, username, account, hostname, MAC address, CPU serial, disk
        serial, or any hardware identifier. Because it is stable across launches
        it can be linked to one installation over time, so it is best described
        as <strong>pseudonymous</strong> rather than legally anonymous.
        Resetting it generates a new identifier and a new session and discards
        anything already queued.
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
        The analytics pipeline is designed so this is enforced structurally:
        every event is a variant of a closed type, and a privacy filter strips
        and detects sensitive keys as defence in depth.
      </p>

      <h3>Turning it off</h3>
      <p>
        Turning telemetry off in Settings stops new events from being collected,
        clears anything already queued, and persists the choice locally. Orbit
        never blocks, never errors when the network is down, and works exactly
        the same with analytics disabled. Data is sent over HTTPS only, and TLS
        verification is never disabled.
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
        Analytics is self-hosted on infrastructure operated by the project and
        is not sold, rented, or shared with advertisers or data brokers. Data is
        retained only as long as it is useful for aggregate product decisions.
        Because the data collected is coarse and pseudonymous, it is not used to
        identify you.
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
        <a href={`mailto:${SITE.email}`}>{SITE.email}</a>. For the implementation
        details behind the app telemetry, see the project&apos;s{" "}
        <code>docs/analytics.md</code>.
      </p>
    </LegalPage>
  );
}
