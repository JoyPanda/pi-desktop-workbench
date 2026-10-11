import Link from "next/link";
import { ArrowRight01Icon, GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { LinuxIcon, MacosIcon, WindowsIcon } from "@/components/brand-icons";
import { OrbitField } from "@/components/orbit-field";
import { OsDownloadButton } from "@/components/download-button";
import { Shot } from "@/components/shot";
import { Band, ButtonLink, Shell } from "@/components/ui";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";

export async function Hero() {
  const release = await getLatestRelease();
  const assets = {
    macos: release?.macos,
    windows: release?.windows,
    linux: release?.linux,
  };

  const platforms = [
    {
      icon: <MacosIcon className="size-3.5" />,
      label: "macOS 13+",
      href: release?.macos,
    },
    {
      icon: <WindowsIcon className="size-3.5" />,
      label: "Windows",
      href: release?.windows,
    },
    {
      icon: <LinuxIcon className="size-3.5" />,
      label: "Linux",
      href: release?.linux,
    },
  ];

  return (
    <Shell>
      <Band className="relative overflow-hidden pb-24 pt-16 sm:pb-32 sm:pt-24">
        <OrbitField />
        <div className="relative z-[1] mx-auto flex max-w-[760px] flex-col items-center text-center">
          {release ? (
            <Link
              href={`/changelog/${release.tag}`}
              className="rise group inline-flex items-center gap-2 rounded-full border border-edge-subtle bg-surface-1 py-1 pl-2.5 pr-3 text-[12px] no-underline transition-colors hover:border-edge-strong hover:bg-surface-2"
            >
              <span className="inline-flex items-center gap-1.5 font-mono text-[10.5px] uppercase tracking-[0.1em] text-brand">
                <span className="pulse-dot size-1.5 rounded-full bg-brand" />
                {release.tag}
              </span>
              <span className="text-ink-2 transition-colors group-hover:text-ink">
                Release notes
              </span>
              <HugeiconsIcon
                icon={ArrowRight01Icon}
                className="size-3.5 text-ink-3 transition-transform duration-200 group-hover:translate-x-0.5 group-hover:text-ink"
              />
            </Link>
          ) : null}

          <h1
            className="rise mt-8 text-balance text-[clamp(36px,5vw,56px)] font-semibold leading-[1.08] tracking-[-0.032em] text-ink"
            style={{ animationDelay: "80ms" }}
          >
            A native workbench for the pi coding agent.
          </h1>

          <p
            className="rise mt-6 max-w-[52ch] text-[15px] leading-[1.75] text-ink-2"
            style={{ animationDelay: "140ms" }}
          >
            Your sessions, your models, your machine. Chat, review, and commit
            in one window — the same sessions as the terminal, drawn natively.
          </p>

          <div
            className="rise mt-10 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center"
            style={{ animationDelay: "200ms" }}
          >
            <OsDownloadButton
              assets={assets}
              fallback={LATEST_RELEASE_URL}
              className="min-h-12 px-6 text-sm sm:min-w-[208px]"
            />
            <ButtonLink
              href="https://github.com/imrj05/orbit"
              variant="secondary"
              className="min-h-12 px-6 text-sm"
              external
            >
              <HugeiconsIcon icon={GithubIcon} data-icon="inline-start" />
              View on GitHub
            </ButtonLink>
          </div>

          <div
            className="rise mt-6 flex flex-wrap items-center justify-center gap-x-5 gap-y-2"
            style={{ animationDelay: "260ms" }}
          >
            {platforms.map(({ icon, label, href }) => {
              const inner = (
                <>
                  <span className="text-ink-3 transition-colors group-hover:text-ink-2">
                    {icon}
                  </span>
                  <span>{label}</span>
                </>
              );
              const className =
                "group inline-flex items-center gap-1.5 text-[12px] text-ink-3 no-underline transition-colors hover:text-ink-2";

              return href ? (
                <a
                  key={label}
                  href={href}
                  target="_blank"
                  rel="noreferrer"
                  className={className}
                >
                  {inner}
                </a>
              ) : (
                <span key={label} className={className}>
                  {inner}
                </span>
              );
            })}
          </div>
        </div>

        {/* The product is the hero. A floor of ambient light sits behind the
         * frame and dissolves into the page; the frame carries the only deep
         * shadow in the whole design. */}
        <div className="relative z-[1] mt-16 sm:mt-20">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-0 top-1/2 mx-auto h-[420px] w-[min(1020px,110%)] -translate-y-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(91,157,255,0.07),transparent)]"
          />
          <div className="rise-window relative mx-auto w-full max-w-[1180px]">
            <Shot
              variant="product"
              src="/screens/session.png"
              srcLight="/screens/session-light.png"
              priority
              alt="A completed Orbit session: the agent's notes on the Windows IME composer fix and a Changed 4 files card with per-file line counts, above the composer."
            />
          </div>
        </div>
      </Band>
    </Shell>
  );
}
