import type { Metadata } from "next";
import { LegalPage } from "@/components/legal-page";
import { SITE } from "@/lib/site";

export const metadata: Metadata = {
  title: "Terms",
  description:
    "The terms for using the Orbit desktop app and this website — open source under Apache-2.0, provided as-is.",
  alternates: { canonical: "/terms" },
};

export default function TermsPage() {
  return (
    <LegalPage
      label="Legal"
      title="Terms of use"
      updated="October 10, 2026"
      intro="Plain-language terms for using the Orbit desktop app and this website. The software itself is open source under the Apache License 2.0, which governs your use of the code."
    >
      <h2>Overview</h2>
      <p>
        These terms cover your use of the Orbit desktop application
        (&ldquo;Orbit&rdquo;, &ldquo;the app&rdquo;) and the website at{" "}
        {SITE.url.replace("https://", "")} (&ldquo;the site&rdquo;). By
        downloading, installing, or using Orbit, or by using the site, you agree
        to them. If you do not agree, do not use Orbit or the site.
      </p>

      <h2>The software</h2>
      <p>
        Orbit is free and open-source software released under the{" "}
        <a href={`${SITE.github}/blob/main/LICENSE`} target="_blank" rel="noreferrer">
          Apache License 2.0
        </a>
        . That license governs your rights to use, modify, and redistribute the
        code, and it includes its own disclaimer of warranties and limitation of
        liability. Where these terms and the license conflict regarding the
        software, the license controls.
      </p>

      <h2>Using the app responsibly</h2>
      <p>
        Orbit is a local desktop client that runs an AI coding agent with the
        same local access you would have in a terminal. That means:
      </p>
      <ul>
        <li>
          <strong>You are responsible for what the agent does.</strong> It can
          read, edit, and run commands in your workspace as your user.
        </li>
        <li>
          <strong>Access modes are a confirmation guard, not a sandbox.</strong>{" "}
          They decide which actions prompt first, but they are not an isolation
          boundary.
        </li>
        <li>
          Back up your work and review changes before accepting them, especially
          in <strong>Full access</strong> mode.
        </li>
      </ul>

      <h2>Model providers</h2>
      <p>
        Orbit connects to model and tool providers that you configure with your
        own credentials. Your use of those services is governed by their terms
        and pricing, not by these terms. You are responsible for any costs you
        incur and for complying with each provider&apos;s acceptable-use rules.
      </p>

      <h2>This website</h2>
      <p>
        The site describes Orbit and links to downloads and release notes. It is
        provided for information only. Downloads are hosted by GitHub; your use
        of GitHub is governed by GitHub&apos;s terms. See the{" "}
        <a href="/privacy">Privacy</a> page for how the site handles analytics.
      </p>

      <h2>Acceptable use</h2>
      <p>You agree not to:</p>
      <ul>
        <li>
          use Orbit or the site to break any law or to infringe the rights of
          others;
        </li>
        <li>
          attempt to gain unauthorized access to the site, its infrastructure,
          or any connected service;
        </li>
        <li>
          interfere with the site or its analytics, or scrape it at a rate that
          degrades service for others.
        </li>
      </ul>

      <h2>Intellectual property</h2>
      <p>
        The Orbit name, logo, and wordmark identify the project and are not
        granted for use by the software license. The source code is licensed
        under Apache-2.0 as described above.
      </p>

      <h2>Disclaimer of warranties</h2>
      <p>
        Orbit and the site are provided <strong>&ldquo;as is&rdquo;</strong>,
        without warranties or conditions of any kind, whether express or
        implied, including, without limitation, any warranties of
        merchantability, fitness for a particular purpose, or
        non-infringement. You use Orbit at your own risk.
      </p>

      <h2>Limitation of liability</h2>
      <p>
        To the maximum extent permitted by law, the maintainers and contributors
        of Orbit will not be liable for any indirect, incidental, special,
        consequential, or punitive damages, or any loss of data, profits, or
        goodwill, arising from or related to your use of Orbit or the site —
        including any change made to your files or systems by the agent.
      </p>

      <h2>Changes to these terms</h2>
      <p>
        These terms may be updated from time to time; the date at the top of the
        page will change when they are. Continued use after an update means you
        accept the revised terms.
      </p>

      <h2>Contact</h2>
      <p>
        Questions about these terms? Email{" "}
        <a href={`mailto:${SITE.email}`}>{SITE.email}</a>.
      </p>
    </LegalPage>
  );
}
