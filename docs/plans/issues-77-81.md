# Plan — Open issues #77–#81

**Status:** Proposed, except **#80 and #78 — implemented** (see §2 and §4).
**Scope:** Orbit desktop app (`crates/orbit-pi`, `crates/orbit-rpc`)
**Related:** `AGENT.md` (RPC surface), `app/settings.rs` (Settings → Agent /
Appearance), `auto_title.rs` (the `~/.orbit-pi/*.json` store pattern),
`app/sidebar.rs` (row menus, sidebar key context), `transcript_view.rs`
(activity-group expansion)

Issue numbers here are GitHub `imrj05/orbit` issue numbers (issues and PRs share
one sequence).

---

## 0. Ordering and shape

| # | Title | Kind | Depends on | Size |
|---|-------|------|-----------|------|
| 81 | bug: unable to connect to pi process | bug + diagnostics | — | S (diagnostics) / M (real fix, needs reporter data) |
| 80 | keep agent calls expanded / collapsed | feature | — | S |
| 79 | edit steer + queued messages, send now | feature | upstream RPC gap | M–L |
| 78 | distinct colors for project names / session tabs | feature | — | S |
| 77 | session 3-dots actions from the keyboard | feature (needs clarification) | 78 (menu gets a shortcut gutter) | M |

Suggested landing order: **81 diagnostics → 80 → 78 → 77 → 79**. 81 unblocks
the reporter (and likely other M-series/DMG users); 80 and 78 are small and
independent; 77 and 79 both touch interaction surfaces and benefit from 78's
color work being settled first.

---

## 1. Issue #81 — "unable to connect to pi process"

### What the report actually says

- Orbit v0.2.12 (DMG), pi CLI 1.4.0, macOS 27.0.1, M5 Pro.
- Launching Orbit, with no interaction, surfaces
  `pi process exited — restart it from Settings → Runtime`.
- The *same* pi works in Orbit's integrated terminal.

The second point is the important one: the binary exists and runs under a login
shell. So this is not "pi is not installed" — it is a **spawn environment**
problem (PATH / `node` / argv / cwd) or a **spawn-contract** problem (`--mode
rpc` refused, session dir unwritable, patch applied to the wrong package root).

Note that three fixes already landed *after* v0.2.12 that change this failure's
shape: automatic respawn (`bb8f79c`), dead-stdin detection by source
(`cb14af5`), and the stderr-bearing banner (`on_process_exited` →
`process_exited_banner`, `app/runtime.rs:654`). A v0.2.12 user sees the bare
"process exited" line; a current build shows pi's own last stderr line. **Step 1
is therefore to get the current build's stderr line from the reporter** — that
one line names the cause ("env: node: No such file or directory", "Error:
Cannot find module …", "unknown option --mode", a permissions error, …).

### Approach — two phases

**Phase 1 (do now, unblocks triage): make the failure self-describing.**

1. **Surface the resolved binary in Settings → Runtime.** `RuntimeStatus`
   (`app.rs:3118`) carries `started_at` / `alive` / `exited` / `error` but not
   *what* was spawned. Add `bin: Option<PathBuf>` set from the same value the
   spawn used, and render it as a selectable path row in the Runtime panel with
   a "Reveal in Finder" affordance. Today a user cannot tell whether Orbit found
   `/opt/homebrew/bin/pi`, `~/.pi/agent/bin/pi`, or nothing.
2. **Show the full last N stderr lines, not just the last one.** The banner
   keeps `clip_stderr_line` (it must; a stack trace cannot own the error bar),
   but the Runtime panel should list `client.recent_stderr(40)` verbatim in a
   copyable block, plus a "Copy diagnostics" button reusing the bug-report
   plumbing in `bug_report.rs` (OS, arch, Orbit version, pi version, resolved
   bin, resolved package root, `node --version`, augmented PATH, last stderr).
   That button is what makes the next report actionable in one click.
3. **Assert the spawn contract at startup.** `spawn_inner`
   (`orbit-rpc/src/client.rs:260`) currently treats a process that starts and
   immediately exits as "connected, then exited". Add a bounded handshake: after
   spawn, wait up to ~3 s for the first `state`/`stats` reply before declaring
   the runtime ready. If it exits inside that window, the error is a **spawn
   failure** ("could not start pi: <stderr>") rather than a mid-session exit, and
   the auto-restart budget is not spent on it (a crash-on-boot loop should not
   burn three retries and then show a mid-run banner). This alone probably
   resolves the *user-visible* half of #81.

**Phase 2 (only once step 1 names the cause): fix the specific resolution bug.**

Candidate causes, all cheap to test once we know which one it is:

- **`node` not on the child's PATH.** `augmented_path` (`client.rs:876`) and
  `home_bin_dirs` (`client.rs:900`) already cover homebrew/volta/mise/asdf/pnpm/bun/
  npm-global/pi-node/nvm/fnm. Missing: `~/.local/share/mise/installs/node/*/bin`
  (mise shims exist but a pinned-node install may not be shimmed), `~/.nodenv/versions/*/bin`,
  `~/Library/pnpm`, `~/n/bin`, and a user-installed `~/.local/state/fnm_multishells`.
  Add any that apply as `newest_version_bin` entries — no new dependency.
- **Wrong package root for the RPC patch.** `pi_package_root` handles npm globals
  and pi's managed installer. If a `PI_PACKAGE_ROOT` from a previous install is
  persisted and the user has since upgraded pi, the patch can be applied to a
  vanished package. Verify at spawn time that the persisted root still exists and
  still declares `@earendil-works/pi-coding-agent`; drop it and re-resolve when
  not (one re-resolve, then persisted again).
- **Session dir unwritable** (macOS 27 TCC/permissions on `~/.pi/agent/sessions`
  for a DMG app): pi exits on a refused write. Detect by `metadata().permissions()`
  before spawn and report a named error instead of an opaque exit.

### Files

- `crates/orbit-rpc/src/client.rs` — handshake, `bin` on the status, any added
  search dirs.
- `crates/orbit-pi/src/app.rs` — `RuntimeStatus.bin`.
- `crates/orbit-pi/src/app/runtime.rs` — `on_process_exited`, spawn-failure vs
  exit classification, auto-restart budget interaction.
- `crates/orbit-pi/src/app/settings.rs` — Runtime panel rows.
- `crates/orbit-pi/src/bug_report.rs` — diagnostics block.

### Tests

- `app/auto_restart_tests.rs`: a spawn that exits inside the handshake window
  raises a *spawn failure*, does not decrement the retry budget, and does not
  schedule a respawn.
- A transport test driving a scripted server that exits immediately after
  `--mode rpc` and writes one stderr line: the banner carries that line.
- `pi_bin_candidates` ordering test for any added search dir.

### Risks

- The handshake adds up to 3 s of launch latency in the worst case. Mitigate by
  returning as soon as the first reply arrives (normally < 300 ms) and only
  waiting the full budget when nothing arrives.
- Do **not** raise the auto-restart limit to paper over this; a respawn loop
  hides the real cause.

---

## 2. Issue #80 — keep agent calls expanded or collapsed

**Shipped.** `agent_call_display.rs` owns the preference (`FollowRun` / `Expanded`
/ `Collapsed`, persisted to `~/.orbit-pi/agent-calls.json`); `Transcript::render`
receives it and `RowPaint` applies it as the default for both the activity group
and the tool detail card, with an explicit toggle still winning. Settings →
Agent → **Agent call details** selects it. Answering the open question below:
the setting is scoped to activity groups and tool cards, **not** the "Thought"
cards — that map records *collapsed* state (the inverse of the group map), so a
shared preference would fight its semantics. Revisit if users ask for it.

### Root cause

Expansion state defaults to "follow the run": in `transcript_view.rs:2082` and
`:2161`, an activity group's open state is
`expanded_activities.get(&(ix, step)).unwrap_or(group_live)` — where
`group_live = activity_group_is_live(...)`. While a run streams the group is
open; when `live` flips false at run end, every group the reader did not
explicitly toggle **snaps shut**. That is exactly "keeps expanding and
collapsing".

### Approach

Add a third, explicit fallback ahead of `group_live`:

1. **Preference.** `AgentCallDisplay { FollowRun, AlwaysExpanded, AlwaysCollapsed }`,
   default `FollowRun` (current behavior — do not change what existing users see).
   Persist to `~/.orbit-pi/agent-call-display.json`, following `auto_title.rs`'s
   store shape exactly (best-effort read, atomic write, defaults on any error).
2. **Read it where the default lives.** Replace
   `.unwrap_or(group_live)` with
   `.unwrap_or_else(|| preference.fallback_open(group_live))`, where
   `AlwaysExpanded → true`, `AlwaysCollapsed → false`, `FollowRun → group_live`.
   Explicit per-group toggles in the map still win, so a reader can still open
   one group in `AlwaysCollapsed` mode. Apply the same fallback to the "thought"
   cards (`collapsed_thoughts`, `transcript_view.rs:153`) and to tool detail rows
   (`expanded_tools`) so the setting is not half-applied.
3. **Settings → Agent row.** A three-option select (the same `settings_toggle` /
   select-chip pattern as `SettingsSelect`), with a one-line explanation
   ("Follow run" = open while running, closed once finished).
4. **Do not reset the map on mode change.** Flipping the setting should
   immediately re-render with the new fallback; stale explicit toggles stay until
   the next session, which is the least surprising behavior.

### Files

- `crates/orbit-pi/src/agent_call_display.rs` (new, mirrors `auto_title.rs`).
- `crates/orbit-pi/src/transcript_view.rs` — the two `unwrap_or(group_live)`
  sites plus thought/tool fallbacks; thread the preference through the view
  struct like `nerd` is threaded.
- `crates/orbit-pi/src/app.rs` — field on `OrbitApp`, seeded at launch.
- `crates/orbit-pi/src/app/settings.rs` — Agent-page row.
- `crates/orbit-pi/locales/en.yml` — three labels + the explanation.

### Tests

- Store round-trip: write/read/default-on-garbage (unit, in the new module).
- Transcript render test (the pattern already exists in
  `transcript_view.rs` tests around line 9058): with `AlwaysCollapsed`, a
  finished group renders closed; with `AlwaysExpanded`, a finished group renders
  open; `FollowRun` still keys on liveness.
- An explicit toggle still flips a single group in either fixed mode.

### Risks

- Lowest of the five. Main trap: threading a new field through the paint struct
  (`TranscriptPaint`) touches many call sites — pass it where `nerd` already
  travels rather than adding a parameter to every `render_*`.

---

## 3. Issue #79 — edit queued/steer messages and send now

### The hard constraint (read this before scoping)

pi's RPC surface is `prompt`, `abort`, **`clear_queue` (all-or-nothing)**,
`steer`, `follow_up`, `set_steering_mode`, `set_follow_up_mode`, … (`AGENT.md`
§"Typed in `CommandBody`"). There is **no per-entry edit, remove, or promote**
command. `PendingQueue` (`orbit-rpc/src/types.rs:1329`) is a read-only mirror of
`queue_update`, and Orbit's only mutator today is `clear_queue`
(`app/view.rs:4900`, `app/session.rs:569`).

So this issue cannot be implemented as "mutate the queue". There are two honest
designs:

**Option A — clear-and-resend (no upstream change, ships now).**
"Send now" on row *i*: capture the current queue, send `clear_queue`, then
re-send the edited/other entries as `steer` (immediate, delivered after the
current step) or `follow_up` (after the turn), in their original order. "Edit"
is local-only: the text lives in a draft until the user commits, at which point
the same clear-and-resend applies. `clear_queue`'s response returns the queue
text, so the operation is recoverable if a `steer` then fails.

Tradeoffs, stated plainly: there is a visible window where the queue is empty
before the re-send lands, `queue_update` will echo twice, and the ordering of
"send now" relative to already-delivered steering is whatever pi decides. If pi
delivers a steer between the clear and the re-send, the user's "send now" lands
after it.

**Option B — upstream per-entry RPC (`queue_edit` / `queue_promote`).**
Correct, but it is a pi change first and an Orbit change second; Orbit must keep
working against pi builds that lack it (capability-probe via the command catalog,
fall back to Option A). This is the right long-term shape and the wrong thing to
block the issue on.

**Recommendation:** ship Option A behind the same UI we would want for Option B,
and file the upstream request. Have the settings/i18n copy say "sent as a new
message" so the behavior is not oversold as true queue mutation.

### UI

The queue bar (`app/view.rs:4706`) currently renders flat read-only rows via
`queue_row(kind, text, follow, theme)` (`app/helpers.rs:43`). Per row, revealed
on hover (the same `BUTTON_GROUP` hover-lift convention as the rest of the app):

- **Edit** → swap the row's text for a one-line `ComposerInput` seeded with the
  entry; ⏎ commits, Esc cancels. Add `queue_edit: Option<(QueueKind, usize)>`
  plus a `queue_edit_input: Entity<ComposerInput>` (mirrors the existing
  `session_name_input`).
- **Send now** → available on `follow_up` rows only. A `steer` row is already
  on its way; offering "send now" there would be a lie.
- Keep the existing header `X` (clear all) as-is.

`queue_row` needs `(kind, ix)` and listener callbacks, so it moves from a free
function in `helpers.rs` into a method on `OrbitApp` (or gains a
`cx: &Context<OrbitApp>` + index). Prefer the method: `helpers.rs` has no `cx`.

### Files

- `crates/orbit-pi/src/app/view.rs` — `queue_bar`, new row actions.
- `crates/orbit-pi/src/app/helpers.rs` — `queue_row` signature or relocation.
- `crates/orbit-pi/src/app/composer_ops.rs` — edit/send-now handlers,
  clear-and-resend sequencing with rollback into the composer on failure.
- `crates/orbit-pi/src/app.rs` — edit state fields.
- `crates/orbit-pi/locales/en.yml` — "Edit", "Send now", failure text.
- `AGENT.md` — record that queue mutation is emulated (clear + resend) until pi
  exposes per-entry commands.

### Tests

- Clear-and-resend ordering: a queue of `[steer A, follow_up B, follow_up C]`
  with "send now" on C produces `clear_queue` → `steer C` → `follow_up A,B`
  in that order, with no entry lost or duplicated.
- A failed `steer` after the clear restores the remaining entries into the
  composer and raises the banner (never silently drops text).
- "Send now" is not offered on a steering row.
- Edit + commit re-sends the edited text, not the original.

### Risks

- The clear-and-resend window is the real risk: a crash between the two leaves
  the user's queued text only in memory. Mitigate by holding the captured queue
  in a `pending_queue_restore` field (there is precedent:
  `restore_queue_on_clear`) and restoring on any error or on process exit.
- Do not send `steer` for a row while `busy == false` — idle has nothing to
  steer; it must be a plain `prompt`/`follow_up`.

---

## 4. Issue #78 — distinct colors for project names and session tabs

**Shipped.** See the note at the end of this section for what was actually
built, which departs from the draft below in one respect: no new theme tokens
were added.

### Root cause

Project (workspace) header label and session rows are visually near-identical:

- header label: `theme.text_secondary`, `TextSize::Default`, `MEDIUM`
  (`app/sidebar.rs`, the `SideRow::Workspace` arm),
- session row title (`session_title`): `text_primary` when active /
  `text_secondary` otherwise, `TextSize::Default` — so an *inactive* session title
  and a project header were the same size and the same color, differing only in
  weight (`sidebar.rs` `session_title`).

(The `text_muted`/`XSmall` text around them is the preview line, the age, and the
flat-list workspace chip — not the title.)

A per-project tint already exists and is already resolved against the theme
(`workspace_mark::tint_color`, `workspace_mark.rs` `TINTS`), but it colors **only
the icon**. The label stays grey, so the tint is invisible at a glance.

### What shipped

One function and one call site — `workspace_mark::label_ink(&theme, tint)` used
by the project header label:

- **untinted / `"default"` / garbage key** → the palette's **accent hue** at the
  body ink's lightness, saturation capped at `DEFAULT_NAME_SATURATION` (0.35).
  Measured against Orbit dark's `#E2E2E2` body ink that is `#ECDCD8`, a ±10 RGB
  step: a quiet warm cast that reads as color but never competes with the
  selected row. The literal "10% of the accent" ask (a 0.10 cap, or an alpha
  blend) moves the channels by ±3 and is invisible — the knob is saturation, and
  the constant documents the whole table.
- **a real tint** → the tint's **hue**, saturation capped at
  `TINT_NAME_SATURATION` (0.5). The user's pick beats the default.
- **a tint the palette collapsed onto body ink** (`OperationalLight` maps
  `syn_type` → `text_secondary`, `syn_function` → `text_primary`) → the default
  hue, since that tint cannot name anything.

**Why hue-only, at body-ink lightness:** `TINTS` were curated for *fills and
glyphs*, and several are too dark to be text — the first cut of this change
named tinted projects with the raw tint and `OrbitAzure` came in at **3.53:1**
on the sidebar, below AA. Borrowing `text_primary.l` makes the hue safe in every
palette with no contrast search at paint time, and gives all headers one
consistent brightness. A palette-wide test
(`theme::tests::the_project_name_ink_stays_readable_in_every_palette`) now holds
both the untinted and tinted inks at 4.5:1 on `surface` and `surface_hover` for
all 12 palettes.

Session titles are untouched. No `tab_fg` tokens were added: aliasing
`text_secondary` under a new name changes no pixel and would be speculative. The
hierarchy is now four steps — section label (`text_muted`, 12px) → project
(accent/tint hue at body brightness, 14px `MEDIUM`) → session (`text_secondary`,
14px `NORMAL`; `text_primary` on the selected fill when open).

**Open question, still open:** the issue says "session tabs"; this implements the
sidebar-session-row reading. If the reporter meant a different surface, the
header half still applies and only the session half would move.

### Approach (draft, superseded by "What shipped")

1. **Tint the project label, not just its icon.** In the header render, use
   `workspace_mark::tint_color(&theme, mark.tint.as_deref())` for the label's
   ink when a tint is set; fall back to `theme.text_secondary` when it is not
   (the default mark must not start shouting color). To keep legibility, resolve
   the tint through a new `workspace_mark::label_ink(&theme, tint)` that returns
   the tint for non-`default` keys and `text_secondary` for `default`.
2. **Give session rows a distinct, quieter identity.** Add one semantic token
   pair to `theme.rs` — `tab_fg` / `tab_fg_active` — instead of reusing
   `text_muted`/`text_secondary`. `tab_fg_active` should be the theme's
   `text_primary`-adjacent ink and `tab_fg` a step below `text_muted`, so the
   hierarchy becomes: coloured project name → primary session title → muted
   metadata. Every palette (`theme.rs` has both Classic and Operational) must
   define both, and the dark-mode pair needs checking for contrast.
3. **Leave the icon-only path alone** when no tint is set, so nothing changes for
   users who never opened the icon-and-color picker.
4. **Do not color the whole row background** — the active row already owns
   `surface_selected`; two competing fills would look like a bug.

This is a "make the existing mechanism visible" fix, not a new coloring system.

### Open question (needs a decision before coding)

The issue says "project names and session tabs". In this app the nearest thing to
"session tabs" is the **sidebar session row**; there is no top-of-window session
tab strip. If the reporter meant something else (the terminal tab strip, or an
editor-style tab bar they expect to exist), the fix targets a different file.
Confirm with the reporter, and default to the sidebar reading above.

### Files

- `crates/orbit-pi/src/workspace_mark.rs` — `label_ink`.
- `crates/orbit-pi/src/app/sidebar.rs` — header label ink, session row ink.
- `crates/orbit-pi/src/theme.rs` — `tab_fg` / `tab_fg_active` for both palettes.
- `crates/orbit-pi/src/app/theme_switch_tests.rs` — token presence per palette.

### Tests

- Every palette defines `tab_fg` / `tab_fg_active` (the existing
  `theme_switch_tests` pattern catches a missing token across all palettes).
- `label_ink` returns the tint for a tinted mark and `text_secondary` for
  `default`.
- A render snapshot test (the sidebar has several: `sidebar_group_tests.rs`)
  asserting a tinted project header's label ink differs from a session row's.

### Risks

- Low. Main trap is contrast: `syn_string` (pink) and `warning` (yellow) as
  label ink are risky on light backgrounds. If a tint fails a 4.5:1 check against
  `surface`, fall back to `text_secondary` for that key rather than shipping it.

---

## 5. Issue #77 — "app shortcuts sidebar session 3dots actions"

### Status: needs a description

The issue body repeats the title three times and says nothing else. `imrj05`
already asked for detail in a comment on 2026-10-25; there is no reply. **Do not
guess-and-ship a shortcut scheme off the title alone.** Plan the most likely
reading, confirm it in-thread, then build.

Most likely reading (and the one that matches the codebase's gaps): the session
row's `…` menu has six actions — Pin, Archive, Copy path, Reveal in Finder,
Clone session, Delete (`session_menu_popup`, `app/sidebar.rs:1251`) — and **none
of them is reachable from the keyboard**, nor is the menu itself.

What exists today:

- Sidebar cursor navigation: `⌘⇧B` to focus, `↑`/`↓`/`Home`/`End`/`⏎`/`Esc`
  (`main.rs:553–558`, handlers in `app/sidebar.rs:2570–2697`).
- Row menu opens only by click (`toggle_session_menu`, `app/sidebar.rs:2108`;
  right-click via `open_session_menu_at`, `:2138`).
- `menu_item` (`app/sidebar.rs:2010`) already reserves a keybinding gutter
  (`context_menu::keybinding_gap`) but renders nothing in it.

### Approach (pending confirmation)

1. **Open the menu from the keyboard.** New action `SidebarMenu` in the
   `Sidebar` context, bound to `shift-f10` (the platform convention) and
   `⌘⇧K`-style chord only if it collides with nothing in `commands.rs`. It
   resolves the cursor row to a `SessionMenu` (or `WorkspaceMenu` for a header
   row) and opens it anchored to the row — the same `SessionMenu` the `…`
   button builds, with `at: None`.
2. **Keyboard navigation inside the menu.** `↑`/`↓` move a highlight,
   `⏎` runs, `Esc` closes (already routed through `on_sidebar_close`). Requires
   a `highlight: usize` on `SessionMenu`/`WorkspaceMenu` and a `Menu` key
   context riding the popup's focus handle — no new mechanism, the approval bar
   (`approval_highlight`) and the model picker do exactly this.
3. **A shortcut per action**, shown in `menu_item`'s gutter:
   Pin `⌘⇧P`, Archive `⌘⇧A`, Copy path `⌥⌘C`, Reveal `⌥⌘R`, Clone `⌘⇧D`,
   Delete `⌫`/`⌦` (destructive, menu-scoped only). All are *menu-scoped first*
   (they only fire while the menu is focused) so nothing is stolen from the
   composer; promote the safe ones (Pin, Copy path) to global sidebar-scoped
   bindings only if the reporter asks.
4. **Register them in the command palette / shortcut help.** Any new
   `CommandId` must appear in `commands.rs`' registry or the Discoverability
   rule in `AGENT.md` is broken (the registry test at `commands.rs:1261` enforces
   unique, parseable chords).

### Files

- `crates/orbit-pi/src/main.rs` — action definitions + `KeyBinding`s.
- `crates/orbit-pi/src/commands.rs` — `CommandId` entries, chords, help text.
- `crates/orbit-pi/src/app/sidebar.rs` — `SidebarMenu` handler, highlight state,
  `menu_item` keybinding gutter.
- `crates/orbit-pi/src/app.rs` — `highlight` on the two menu structs.
- `crates/orbit-pi/locales/en.yml` — any new labels.

### Tests

- `SidebarMenu` on a session row opens that row's menu; on a collapsed workspace
  header it opens the workspace menu; with no cursor it is a no-op.
- `↑`/`↓` clamp inside the menu, `⏎` invokes the highlighted item, `Esc` closes
  without touching the session.
- Every new chord passes the registry test (unique + parseable on both platform
  modifiers).

### Risks

- `shift-f10`/menu chords can be swallowed by the terminal or composer focus
  contexts. Guard the menu actions behind the `Sidebar`/`Menu` contexts only
  (`composer_send.rs:214`'s test shows the existing guard pattern).
- The whole issue stays blocked until the reporter confirms the intent.

---

## 6. Cross-cutting notes

- **i18n.** Every new user-visible string goes through `tr!` and `locales/en.yml`
  (and the other locale files, or the tests will flag the gap).
- **No new dependencies.** Nothing in this plan needs one.
- **Release notes.** Each landing should add a `CHANGELOG.md` line; 81 is a bug
  fix under "Runtime", 80/78/79/77 are features.
- **What this plan deliberately does not do:** change the auto-restart retry
  budget (#81), add a queue-mutation command to Orbit's RPC types before pi has
  one (#79), or invent a session tab strip that does not exist (#78).
