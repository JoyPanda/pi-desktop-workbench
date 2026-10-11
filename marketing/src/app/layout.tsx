import type { Metadata, Viewport } from "next";
import Script from "next/script";
import { JsonLd } from "@/components/json-ld";
import { LightboxProvider } from "@/components/lightbox";
import { FONT_CLASSES } from "@/lib/fonts";
import { getLatestRelease } from "@/lib/releases";
import { SITE } from "@/lib/site";
import { THEME_SCRIPT } from "@/lib/theme";
import "./globals.css";

/**
 * Search-engine ownership tokens. Google's is stable (and mirrored in DNS), so
 * it lives in code. Bing and Yandex tokens are read from env and only rendered
 * when set, so the build never ships an empty `content=""` meta tag. Set
 * `BING_VERIFICATION` (Bing Webmaster Tools) and `YANDEX_VERIFICATION`
 * (Yandex Webmaster) in the deployment environment.
 */
const BING_VERIFICATION = process.env.BING_VERIFICATION?.trim();
const YANDEX_VERIFICATION = process.env.YANDEX_VERIFICATION?.trim();

export const metadata: Metadata = {
  metadataBase: new URL(SITE.url),
  title: {
    default: SITE.title,
    template: "%s · Orbit",
  },
  description: SITE.description,
  applicationName: SITE.name,
  generator: "Next.js",
  keywords: [...SITE.keywords],
  authors: [{ name: SITE.name, url: SITE.github }],
  creator: SITE.name,
  publisher: SITE.name,
  category: "Developer Tools",
  referrer: "origin-when-cross-origin",
  alternates: {
    canonical: "/",
    types: { "application/rss+xml": `${SITE.url}/changelog/rss.xml` },
  },
  openGraph: {
    type: "website",
    siteName: SITE.name,
    url: "/",
    title: SITE.title,
    description: SITE.ogDescription,
    locale: "en_US",
  },
  twitter: {
    card: "summary_large_image",
    site: SITE.twitter,
    creator: SITE.twitter,
    title: SITE.title,
    description: SITE.ogDescription,
  },
  robots: {
    index: true,
    follow: true,
    googleBot: {
      index: true,
      follow: true,
      "max-image-preview": "large",
      "max-snippet": -1,
      "max-video-preview": -1,
    },
  },
  formatDetection: { email: false, address: false, telephone: false },
  manifest: "/manifest.webmanifest",
  // Confirms site ownership in Google Search Console. The same token is also
  // published as a DNS TXT record on the domain.
  verification: {
    google: "BXytVhKqWEHfSTa-uxejpCPRbyhlN_9RAY2eiz0B2hk",
    ...(BING_VERIFICATION || YANDEX_VERIFICATION
      ? {
          other: {
            ...(BING_VERIFICATION
              ? { "msvalidate.01": BING_VERIFICATION }
              : {}),
            ...(YANDEX_VERIFICATION
              ? { "yandex-verification": YANDEX_VERIFICATION }
              : {}),
          },
        }
      : {}),
  },
  appleWebApp: {
    capable: true,
    title: SITE.name,
    statusBarStyle: "black-translucent",
  },
};

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f6f6f7" },
    { media: "(prefers-color-scheme: dark)", color: "#09090b" },
  ],
  colorScheme: "dark light",
  width: "device-width",
  initialScale: 1,
};

/** Site-wide structured data for rich results. */
function buildJsonLd(version?: string) {
  return {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "WebSite",
        "@id": `${SITE.url}/#website`,
        url: SITE.url,
        name: SITE.name,
        description: SITE.description,
        inLanguage: "en",
        publisher: { "@id": `${SITE.url}/#organization` },
      },
      {
        "@type": "Organization",
        "@id": `${SITE.url}/#organization`,
        name: SITE.name,
        url: SITE.url,
        logo: {
          "@type": "ImageObject",
          url: `${SITE.url}/icon.png`,
          width: 256,
          height: 256,
        },
        sameAs: [
          SITE.github,
          `https://x.com/${SITE.twitter.replace(/^@/, "")}`,
        ],
        email: SITE.email,
        contactPoint: {
          "@type": "ContactPoint",
          email: SITE.email,
          contactType: "customer support",
        },
      },
      {
        "@type": "SoftwareApplication",
        "@id": `${SITE.url}/#softwareapplication`,
        name: SITE.name,
        alternateName: "Orbit Pi",
        applicationCategory: "DeveloperApplication",
        applicationSubCategory: "AI Coding Agent Client",
        operatingSystem: "macOS, Windows, Linux",
        description: SITE.description,
        url: SITE.url,
        downloadUrl: `${SITE.github}/releases/latest`,
        installUrl: `${SITE.github}/releases/latest`,
        softwareHelp: `${SITE.github}#readme`,
        // Only present once the release resolves; the key is dropped otherwise.
        softwareVersion: version,
        codeRepository: SITE.github,
        license: "https://www.apache.org/licenses/LICENSE-2.0",
        author: { "@id": `${SITE.url}/#organization` },
        publisher: { "@id": `${SITE.url}/#organization` },
        offers: {
          "@type": "Offer",
          price: "0",
          priceCurrency: "USD",
          availability: "https://schema.org/InStock",
        },
        featureList: [
          "Native desktop app built with Rust and GPUI",
          "Multiple pi sessions and workspaces in one window",
          "Inline diff review with per-turn checkpoints",
          "Git and GitHub issues, pull requests, and merges",
          "Model providers: Anthropic, OpenAI, Bedrock, Ollama, and custom",
          "Usage and quota analytics",
          "MCP servers, skills, and plugins",
          "Integrated terminal and keyboard-first navigation",
        ],
        screenshot: [
          "/screens/session.png",
          "/screens/review.png",
          "/screens/git.png",
          "/screens/usage.png",
        ].map((path) => ({
          "@type": "ImageObject",
          url: `${SITE.url}${path}`,
        })),
        sameAs: [
          SITE.github,
          `https://x.com/${SITE.twitter.replace(/^@/, "")}`,
        ],
      },
    ],
  };
}

export default async function RootLayout({ children }: LayoutProps<"/">) {
  const release = await getLatestRelease();

  return (
    <html
      lang="en"
      data-scroll-behavior="smooth"
      suppressHydrationWarning
      className={`dark ${FONT_CLASSES} h-full`}
    >
      <body className="min-h-full flex flex-col bg-page text-ink">
        <Script
          id="theme-init"
          strategy="beforeInteractive"
          dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }}
        />
        <JsonLd data={buildJsonLd(release?.version)} />
        <LightboxProvider>{children}</LightboxProvider>
      </body>
    </html>
  );
}
