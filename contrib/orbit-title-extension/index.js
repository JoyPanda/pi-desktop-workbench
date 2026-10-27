/**
 * Orbit auto-title (pi extension).
 *
 * Orbit loads this file with `pi --extension <path>` on every session it
 * spawns. When a turn settles it asks a model for a short title from the
 * session's first exchange and sets it with `pi.setSessionName`. pi persists a
 * `session_info` entry and emits `session_info_changed`, so Orbit shows the
 * title live and it survives reloads.
 *
 * The setting lives in `~/.orbit-pi/auto-title.json` and is read when a turn
 * settles, so a change in Orbit reaches sessions started afterwards (and any
 * session whose turn has not settled yet). The model is Orbit's choice
 * (`{provider, id}`) or, by default, the session's active model. The pure
 * pieces are in `title.js`.
 *
 * Which sessions get a title: **any session that does not have a name yet.**
 * An existing name is the user's (or one set earlier), so it is never replaced
 * without an explicit `/generate-title`. Deciding by *name* rather than by
 * "this is the first turn of a fresh session" is what makes it dependable:
 * - a session resumed from disk with no name still gets one,
 * - a steering message or follow-up queued during the first run does not
 *   disqualify the session (its first exchange is already there to describe),
 * - a settle that arrives before the assistant has produced any text is
 *   retried on the next turn instead of writing the session off for the life
 *   of the process.
 *
 * Failure is always quiet: a title is a nicety, never a reason to disturb a
 * run. A transient failure (a provider hiccup, a model that is briefly
 * unavailable) is retried on the next settle, up to `MAX_ASK_ATTEMPTS`; a
 * completed ask that produced no text is final, because asking again would
 * only repeat the same empty reply.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { randomUUID } from "node:crypto";

import {
  CONFIG_FILE,
  buildTitlePrompt,
  firstExchange,
  isStaleContext,
  isTitleable,
  parseConfig,
  resolveModel,
  titleFromResponse,
} from "./title.js";

// Reasoning models bill thinking against the same output budget, and a title
// is only a few tokens. 64 was consumed entirely by thinking — the model hit
// `stopReason: "length"` and emitted no text — so leave room to think and
// still answer. (Some models declare thinking unsupported (`"off": null`) and
// cannot be asked to skip it, so the budget has to cover it.)
const MAX_TITLE_TOKENS = 1024;

/**
 * How many transient failures a session gets before it is left with its
 * first-message title. One attempt was too few: a single provider hiccup cost
 * that session its title for good, because the retry only ever ran on a later
 * settle. It is still bounded so a permanently broken model is not asked on
 * every turn forever.
 */
const MAX_ASK_ATTEMPTS = 3;

/** In-flight title request; Orbit's tests await it to stay deterministic. */
let inflight = Promise.resolve();

export function pendingTitle() {
  return inflight;
}

/** A title was asked for and answered: this session is finished with titling. */
const ASKED = "asked";
/** Nothing to do right now — disabled, already named, or the ctx went stale. */
const SKIP = "skip";
/** No exchange to describe yet — a later settle may still title this session. */
const WAITING = "waiting";
/** Transient failure: worth another attempt on the next settle. */
const RETRY = "retry";

/** Config path resolved per call, so a changed `HOME` (tests) is honored. */
function configPath() {
  return path.join(os.homedir(), ".orbit-pi", CONFIG_FILE);
}

/** Read Orbit's setting; a missing/broken file means "on, active model". */
function readConfig() {
  try {
    return parseConfig(fs.readFileSync(configPath(), "utf8"));
  } catch {
    return parseConfig(null);
  }
}

/**
 * Generate and set the title. Never throws. `manual` (the `/generate-title`
 * command Orbit's details popover runs) titles any session with an exchange to
 * describe — even one already named, and even when automatic titling is off.
 */
async function generate(pi, ctx, { manual = false } = {}) {
  const config = readConfig();
  if (!config.enabled && !manual) {
    return SKIP;
  }
  // Named means done: the name is either the user's or a title set earlier, and
  // only an explicit request may replace it.
  if (pi.getSessionName() && !manual) {
    return SKIP;
  }
  const { user, assistant } = firstExchange(ctx.sessionManager?.getBranch?.() ?? []);
  // A session whose first exchange has no assistant text yet is not finished
  // being written — leave it open for the next settle rather than deciding now
  // that it can never be titled.
  if (!isTitleable(user, assistant)) {
    return WAITING;
  }
  const model = resolveModel(ctx, config);
  if (!model) {
    return RETRY;
  }
  try {
    const response = await ctx.modelRegistry.complete(
      model,
      {
        messages: [
          {
            role: "user",
            content: [{ type: "text", text: buildTitlePrompt(user, assistant) }],
            timestamp: Date.now(),
          },
        ],
      },
      {
        maxTokens: MAX_TITLE_TOKENS,
        cacheRetention: "none",
        sessionId: randomUUID(),
      },
    );
    const title = titleFromResponse(response);
    // Never clobber a name the user set while the request was in flight; the
    // automatic pass also never replaces an existing name. A manual request
    // owns the name it asked to regenerate.
    if (title && (manual || !pi.getSessionName())) {
      pi.setSessionName(title);
    }
    // A completed ask is not retried: an empty title here is the model's
    // answer (for example it spent the whole budget thinking), and asking
    // again would only repeat it. Transient failures still retry via the
    // catch below.
    return ASKED;
  } catch (error) {
    if (isStaleContext(error)) {
      return SKIP;
    }
    // Quiet by design: titling must never surface as a session error.
    return RETRY;
  }
}

export default function activate(pi) {
  // One title request per session: once the model has answered (with a title or
  // with nothing) the session is done here, so a session that keeps settling is
  // never asked twice. `attempts` counts transient failures only, so a broken
  // model is not asked on every turn forever; a pass that found nothing to
  // describe yet costs nothing and leaves the session open.
  let answered = false;
  let attempts = 0;
  let asking = false;

  // Orbit's session-details popover runs this to (re)name a session on
  // demand. An extension command is not added to the transcript, and it
  // executes even while a turn is running.
  pi.registerCommand("generate-title", {
    description: "Generate a title for this session",
    handler: async (_args, ctx) => {
      await generate(pi, ctx, { manual: true });
    },
  });

  pi.on("agent_settled", (_event, ctx) => {
    if (answered || asking || attempts >= MAX_ASK_ATTEMPTS) {
      return;
    }
    asking = true;
    // pi waits for a promise only when the handler returns one; the title is
    // not worth delaying the settled state, so this runs detached. Orbit reads
    // the resulting `session_info_changed` from the event stream, and the next
    // settle asks again while this session still has no name.
    inflight = generate(pi, ctx)
      .then((outcome) => {
        if (outcome === ASKED) {
          answered = true;
        } else if (outcome === RETRY) {
          attempts += 1;
        }
      })
      .catch(() => {
        attempts += 1;
      })
      .finally(() => {
        asking = false;
      });
  });
}
