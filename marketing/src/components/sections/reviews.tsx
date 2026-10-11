import { NewTwitterIcon, RedditIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { UserAvatar } from "@/components/avatar";
import { Band, SectionLabel, Shell } from "@/components/ui";

const SUBREDDIT = "https://www.reddit.com/r/PiCodingAgent";
const THREAD_LAUNCH = `${SUBREDDIT}/comments/1wjvoka/built_a_native_desktop_client_for_pi_agent/`;
const THREAD_RELEASE = `${SUBREDDIT}/comments/1woela9/orbit_pi_v0015_is_out/`;
const X_HANDLE = "imrj05";

type Platform = "reddit" | "x";

type Review = {
  author: string;
  quote: string;
  platform: Platform;
  url: string;
};

/**
 * Verbatim quotes from the r/PiCodingAgent launch threads, unedited so the
 * praise reads as it was written. Give an entry `platform: "x"` (and an X post
 * URL) to have the X glyph render instead of the Reddit one — the card, column
 * layout, and scroller are platform-agnostic.
 */
const REVIEWS: Review[] = [
  {
    author: "Sharp-Everything",
    platform: "reddit",
    url: THREAD_RELEASE,
    quote:
      "Cool stuff dude, thanks. Just tried and I think I'll gonna stick for a while. Was using Herdr, but well, desktop app just feels better. Love your work, gonna be using for my daily work unless its gonna be overbloated with things I dont need (it seems like always happens nowadays xD)",
  },
  {
    author: "piousp",
    platform: "reddit",
    url: THREAD_RELEASE,
    quote:
      "I am currently testing it and have to say that I'm impressed! Works really really well.",
  },
  {
    author: "OneMoreName1",
    platform: "reddit",
    url: THREAD_LAUNCH,
    quote:
      "Hey, I actually started giving this a go, and I must say, good job on the ui and so on! I really like it. Very nice that it also shows me the usage.",
  },
  {
    author: "tulwio",
    platform: "reddit",
    url: THREAD_RELEASE,
    quote:
      "Cool, I'll give it a try! I currently use Herdr with Pi, so it's nice to see another app that's more focused specifically on Pi.",
  },
  {
    author: "adamshand",
    platform: "reddit",
    url: THREAD_RELEASE,
    quote: "Looks nice and love that it runs on Mac & Linux.",
  },
  {
    author: "Upstairs_Toe_3560",
    platform: "reddit",
    url: THREAD_RELEASE,
    quote:
      "On my M5 it works very smooth and brings back my codex experience.",
  },
  {
    author: "Altruistic_Ad8462",
    platform: "reddit",
    url: THREAD_LAUNCH,
    quote:
      "dig it, nice job. I'm only going off your screenshots because Im not in the market for another GUI, but it looks good.",
  },
  {
    author: "Upstairs_Toe_3560",
    platform: "reddit",
    url: THREAD_RELEASE,
    quote:
      "first impressisons good. I'm a heavy work pi user with herdr. I'll test more and feedback the weakness.",
  },
  {
    author: "OneMoreName1",
    platform: "reddit",
    url: THREAD_LAUNCH,
    quote:
      "I was actually looking for something like this for my not very technical gf!",
  },
];

/** Copies of each column's set. Three keeps the loop gapless at any speed. */
const COPIES = 3;
const COLUMNS = 3;

/** Reddit ships seven default snoo avatars at `avatar_default_1..7`. */
const SNOO_COUNT = 7;

/** A stable per-handle pick from Reddit's default snoo set. */
function snooAvatar(username: string): string {
  let hash = 0;
  for (let i = 0; i < username.length; i += 1) {
    hash =
      (username.charCodeAt(i) + (hash << 6) + (hash << 16) - hash) >>> 0;
  }
  return `https://www.redditstatic.com/avatars/defaults/v2/avatar_default_${
    (hash % SNOO_COUNT) + 1
  }.png`;
}

function ReviewCard({
  review,
  dupe = false,
}: {
  review: Review;
  dupe?: boolean;
}) {
  const isReddit = review.platform === "reddit";
  const Icon = isReddit ? RedditIcon : NewTwitterIcon;
  const handle = isReddit ? `u/${review.author}` : `@${review.author}`;

  return (
    <figure
      data-dupe={dupe || undefined}
      aria-hidden={dupe || undefined}
      className="mb-3 rounded-[14px] border border-edge-default bg-surface-1 p-5 shadow-card"
    >
      <blockquote className="text-[13.5px] leading-[1.7] text-ink-2">
        {review.quote}
      </blockquote>
      <figcaption className="mt-5 flex items-center gap-2.5">
        <UserAvatar
          src={isReddit ? snooAvatar(review.author) : undefined}
          handle={handle}
        />
        <span className="truncate text-[12.5px] text-ink">{handle}</span>
        {dupe ? (
          <HugeiconsIcon
            icon={Icon}
            className="ml-auto size-4 shrink-0 text-ink-3"
            strokeWidth={1.8}
          />
        ) : (
          <a
            href={review.url}
            target="_blank"
            rel="noreferrer"
            aria-label={`Read ${handle}'s review on ${isReddit ? "Reddit" : "X"}`}
            className="ml-auto inline-flex shrink-0 items-center text-ink-3 no-underline outline-none transition-colors hover:text-ink focus-visible:text-ink"
          >
            <HugeiconsIcon icon={Icon} className="size-4" strokeWidth={1.8} />
          </a>
        )}
      </figcaption>
    </figure>
  );
}

function ReviewColumn({
  reviews,
  down = false,
  className = "",
}: {
  reviews: Review[];
  down?: boolean;
  className?: string;
}) {
  return (
    <div className={`reviews-col ${className}`}>
      <div className={`reviews-track${down ? " reviews-track--down" : ""}`}>
        {Array.from({ length: COPIES }).map((_, copy) =>
          reviews.map((review) => (
            <ReviewCard
              key={`${copy}-${review.author}-${review.quote.slice(0, 16)}`}
              review={review}
              dupe={copy > 0}
            />
          )),
        )}
      </div>
    </div>
  );
}

export function Reviews() {
  const columns = Array.from({ length: COLUMNS }, (_, column) =>
    REVIEWS.filter((_, index) => index % COLUMNS === column),
  );

  return (
    <Shell>
      <Band id="reviews" dashed className="scroll-mt-20 py-20 sm:py-28">
        <div className="flex flex-col items-center text-center">
          <SectionLabel>Reviews</SectionLabel>
          <h2 className="mt-4 max-w-[24ch] text-balance text-[clamp(26px,2.8vw,34px)] font-semibold leading-[1.18] tracking-[-0.024em] text-ink">
            What people are saying.
          </h2>
          <p className="mt-4 max-w-[48ch] text-[14.5px] leading-[1.75] text-ink-2">
            Unedited comments from the r/PiCodingAgent launch threads.
          </p>
        </div>

        <div
          className="reviews-scroller mt-12"
          role="region"
          aria-label="Reviews"
        >
          <div className="reviews-grid">
            <ReviewColumn reviews={columns[0]} />
            <ReviewColumn
              reviews={columns[1]}
              down
              className="hidden sm:block"
            />
            <ReviewColumn reviews={columns[2]} className="hidden lg:block" />
          </div>
        </div>

        <div className="mt-10 flex flex-wrap items-center justify-center gap-x-5 gap-y-2 text-[12px] text-ink-3">
          <a
            href={SUBREDDIT}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1.5 no-underline transition-colors hover:text-ink-2"
          >
            <HugeiconsIcon
              icon={RedditIcon}
              className="size-3.5"
              strokeWidth={1.8}
            />
            r/PiCodingAgent
          </a>
          <a
            href={`https://x.com/${X_HANDLE}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1.5 no-underline transition-colors hover:text-ink-2"
          >
            <HugeiconsIcon
              icon={NewTwitterIcon}
              className="size-3.5"
              strokeWidth={1.8}
            />
            @{X_HANDLE}
          </a>
        </div>
      </Band>
    </Shell>
  );
}
