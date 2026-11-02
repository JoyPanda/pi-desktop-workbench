# Plan — `@` file mentions from a shared workspace index (drop `fd`)

**Status:** Proposed, revised after review (`docs/plans/mention-index.review.md`), not implemented.
**Owner:** TBD
**Scope:** Orbit desktop app (`crates/orbit-pi`) — composer `@` autocomplete, Explorer project panel, `mentions`.
**Related:** `crates/orbit-pi/src/mentions.rs`, `crates/orbit-pi/src/app/composer_ops.rs`,
`crates/orbit-pi/src/explorer/walk.rs`, `crates/orbit-pi/src/explorer/panel.rs`,
`crates/orbit-pi/src/explorer/tree.rs`, `crates/orbit-pi/src/app.rs`,
`crates/orbit-pi/src/app/events.rs`, `crates/orbit-pi/src/app/session.rs`,
`crates/orbit-pi/src/watch.rs`.

---

## 1. Problem

`@` in the composer spawns `fd` **synchronously on the UI thread, once per keystroke**
(`composer_ops.rs:51-84` → `mentions::search_fd`, `mentions.rs:508-571`). The frame
blocks for the whole walk. Measured on a real Unity workspace (`F:\WorkSite\_main_orig`,
90k candidates, F: drive, fd 10.4.2, min/max of 3 runs):

| query | today (fd, app's exact flags) | after this plan (in-process) |
|---|---|---|
| `@` (empty) | 92 – 140 ms | ~0.01 ms |
| `@a` (dense) | 82 – 122 ms | ~1.6 ms |
| `@Player` | 316 – 338 ms | ~6.2 ms |
| `@Playwr` (sparse) | 607 – 646 ms | ~6.5 ms |
| `@zzzzqqqx` (zero match) | 591 – 774 ms | ~5.5 ms |

A zero-match pattern is the worst case because `--max-results 100` can only short-circuit
when enough hits exist. Typing `@Player` costs 7 spawns ≈ 1.4 s of frozen frames.

Root cause is the per-keystroke subprocess, not the fallback walk. `search_fd` is only
skipped when `locate("fd")` fails (`FdSearch::Missing`, `mentions.rs:509-511`); an fd
failure or an empty result returns `FdSearch::Hits(vec![])` and returns early
(`mentions.rs:550-552`), so the bounded walk (`list_workspace_files`, 4000-entry cap,
`mentions.rs:373-422`) is unreachable whenever fd exists — and it is cached per workspace
anyway.

---

## 2. What the reference implementations do

None of them spawn a search binary per keystroke; all keep an in-process candidate list
and re-rank it.

| Product | Mechanism | Source |
|---|---|---|
| Codex CLI | long-lived session per root; parallel `ignore::WalkBuilder` streams paths into a **nucleo** index; keystroke = `nucleo.pattern.reparse` (`append` fast path); notifications coalesced on a 10 ms tick; empty `@` shows a hint, not a list | `codex-rs/file-search/src/lib.rs`, `codex-rs/tui/src/file_search.rs`, `codex-rs/tui/src/bottom_pane/file_search_popup.rs` |
| Zed | candidates from the editor's own worktree snapshot (`worktree.snapshot()`); fuzzy match on the background executor with an `Arc<AtomicBool>` cancellation flag, limit 100; empty query = recent navigation history + all entries | `crates/agent_ui/src/completion_provider.rs:1828+`, `crates/worktree/src/worktree.rs` |
| Claude Code | `FileIndex` (nucleo) seeded from `git ls-files`, ripgrep fallback, cached with generation counters + `.git/index` mtime checks so git is not spawned per keystroke | `src/hooks/fileSuggestions.ts` (mirror) |
| VS Code / Cursor | ripgrep `--files --hidden --case-sensitive --no-require-git`, ignore files gated by `search.useIgnoreFiles`; VCS metadata dropped via `files.exclude` defaults; results cached between queries | `src/vs/workbench/services/search/node/ripgrepFileSearch.ts:32`, `src/vs/workbench/contrib/files/browser/files.contribution.ts:158` |

Ignore semantics we adopt from them:

- `.gitignore` must apply **whether or not** the workspace is a git repo → `require_git(false)`
  (VS Code passes `--no-require-git` unconditionally; `explorer/walk.rs:56` already sets it).
- Ancestor-directory ignore files must **not** apply → `parents(false)` (Codex uses
  `require_git(true)` partly to avoid the "`~/.gitignore` with `*` blanks the tree" trap;
  VS Code passes `--no-ignore-parent`).
- Hidden files are **included** in the candidate set (VS Code `--hidden`; Zed indexes
  dotfiles and only marks them via `hidden_files: ["**/.*"]`). Once hidden is on, VCS
  metadata must be excluded explicitly (Zed `file_scan_exclusions`, VS Code `files.exclude`).
- No `.env*` special case: Zed re-includes `.env*` even when gitignored
  (`file_scan_inclusions`) — we deliberately do **not**, because a single rule
  (".gitignore decides") is easier to explain.

---

## 3. Decisions (settled with the operator)

| # | Decision |
|---|---|
| 1 | `@` candidates come from an in-memory index. No subprocess on the keystroke path. `search_fd` and friends are deleted. |
| 2 | One index, one walk: `explorer::walk::snapshot` is promoted to app level and shared by the Explorer and `@`. |
| 3 | Walk flags: `require_git(false)`, `git_global(true)`, `git_exclude(true)`, `ignore(true)`, **`parents(false)`**, `follow_links(false)`, hidden **included**, `max_depth(24)`. |
| 4 | Exclusion list is generic VCS/OS metadata only: `.git`, `.svn`, `.hg`, `.jj`, `.DS_Store`, `Thumbs.db` — matched **case-insensitively** (Windows/macOS filesystems are). No Orbit-specific entries (`.pi` stays searchable), no `.env*` override. |
| 5 | `Snapshot` entries carry `is_hidden` (any path segment starting with `.`). Hidden entries are indexed and `@`-searchable; the Explorer filters them in the view, so `show_hidden` becomes a pure render toggle. |
| 6 | `MAX_ENTRIES` 50,000 → 200,000; `build()` → `build_parallel()` with `WalkState::Quit` (measured 2.19 s → 0.43 s). Overshoot before `Quit` lands is accepted. |
| 7 | Scorer: allocation-free `char_indices` version of the existing rules (basename hit weighted 2×, else path hit). Lowercased path, basename offset and hidden bit are precomputed at index build. Hidden entries lose **ties** only. |
| 8 | Empty `@`: in-process recents ring (32 entries, opened-in-Explorer + sent `@` files, **not persisted**) first, then shallow-first (depth, then path). |
| 9 | Invalidation: every trigger marks the index stale. Rebuild is lazy, throttled (`Option<Instant>`, 2 s, mirroring §30 in `app/worktrees.rs:168-178`) **and has a trailing edge** — a change during the window schedules the rebuild for `due` instead of being dropped. `stale` is cleared when the rebuild *starts*, not when it lands. File-system operations bypass the throttle. |
| 10 | Directory entries stay in the results (accepting one still inserts `@dir `), matching today's fd `--type d`. |
| 11 | If the index is `truncated`, the `@` menu shows one non-selectable dim row at the bottom. It must not participate in `AutocompleteState::count`/`highlighted`. |
| 12 | Parallel-walk overshoot at the cap is accepted; no post-pass truncation. |
| 13 | The `@` query result is memoised per `(workspace, index_generation, query)`, because `sync_autocomplete` runs every frame (`composer_ops.rs:155-157`) and the render path calls `autocomplete_entries` again. |
| 14 | Index root = the Explorer's root (`current_workspace.or_else(current_dir)`, `app.rs:2457-2461`). `at_paths` keeps its own `current_workspace` gate (`composer_ops.rs:52`) so `@` still does nothing without a workspace. |

---

## 4. Design

### 4.1 `explorer/walk.rs` — the one walk

```rust
pub const MAX_ENTRIES: usize = 200_000;
pub const MAX_DEPTH: usize = 24;

pub struct Entry { pub path: String, pub is_dir: bool, pub is_hidden: bool }

pub struct Snapshot {
    pub entries: Vec<Entry>,   // replaces `dirs`/`files`
    pub truncated: bool,
}

pub fn snapshot(root: &Path) -> Snapshot;   // `show_hidden` parameter goes away
```

- `hidden(false)` — "do not skip hidden" (the crate default is `true`).
- VCS/OS metadata excluded through `WalkBuilder::filter_entry` (available in the locked
  `ignore 0.4.33`, `walk.rs:1046`; the predicate also prunes descent). Two rules:
  **`entry.depth() == 0` always returns true** (the predicate applies to the root too), and
  the file-name check is `eq_ignore_ascii_case` against `.git`/`.svn`/`.hg`/`.jj`/`.DS_Store`/`Thumbs.db`.
- `is_hidden = path.split('/').any(|s| s.starts_with('.'))`.
- `build_parallel()`; a `Mutex<Vec<Entry>>` + `AtomicUsize` counter; the closure returns
  `WalkState::Quit` once the counter passes `MAX_ENTRIES`, and `snapshot` sorts the result
  afterwards (so the non-deterministic completion order does not matter).
- Symlinks still never followed (existing behavior and test).

### 4.2 `mentions.rs` — `MentionIndex`

```rust
pub struct MentionIndex {
    paths:  Vec<String>,   // display form, sorted by (depth, path)
    lower:  Vec<String>,   // precomputed lowercase path
    name:   Vec<u32>,      // byte offset of the basename inside `lower[i]`
    hidden: Vec<bool>,
}

impl MentionIndex {
    pub fn build(entries: &[walk::Entry]) -> Self;          // ~20 ms per 90k entries
    pub fn query(&self, query: &str, limit: usize) -> Vec<&str>;
    #[cfg(test)] pub fn from_paths(paths: Vec<String>) -> Self;  // test/seed seam
}
```

`query` walks `lower` once per candidate: basename subsequence (score × 2) else full-path
subsequence — exactly today's `filter_entries` file ranking — then sorts by
`score desc → !hidden → depth → path`. Empty query short-circuits with `limit` rows from the
head of `paths`.

`fuzzy_match` keeps its signature and scoring rules but iterates `char_indices()` instead
of collecting `Vec<char>`, so it is allocation-free and still correct on non-ASCII paths.

`filter_entries` **stays** — it still serves slash commands (`composer_ops.rs:31,168`);
only its file branch is replaced by `MentionIndex::query`.

### 4.3 `app/composer_ops.rs` — the query path

`at_paths(query)` becomes:

1. `self.ensure_mention_index(cx)` — lazily (re)builds, throttled, off-thread.
2. Memo hit? Serve it. Key: `(workspace, index_generation, query)`. Miss → compute and store.
3. `let index = self.mention_index.clone()` (Arc).
4. Empty query → `recents` (filtered to entries still in the index, cap 32) then
   `index.query("", limit)`.
5. Otherwise `index.query(query, AUTOCOMPLETE_LIMIT)`.

The memo is invalidated by `index_generation`, so a rebuild never serves stale paths.
No process spawn, no `cx.notify()` churn beyond what exists.

### 4.4 Index ownership and the Explorer

The app owns the index; the panel renders it.

```rust
// app.rs
workspace_index: Option<Arc<Snapshot>>,
mention_index: Option<Arc<MentionIndex>>,
mention_index_workspace: Option<PathBuf>,
mention_index_stale: bool,
mention_index_generation: u64,
mention_index_due: Option<Instant>,      // §30-style throttle, 2 s
mention_index_loading: bool,
mention_query_memo: Option<(PathBuf, u64, String, Vec<String>)>,
mention_recents: Vec<String>,            // 32, in-process only
```

`ensure_mention_index(cx)`:

- workspace changed → `stale = true`, `mention_index = None`, memo clear, recents clear.
- `stale && (now >= due || force)` → **`stale = false` at once**, `due = now + 2 s`,
  `generation += 1`, `loading = true`, background task builds
  `(Snapshot, MentionIndex, git badges)` (badges move here from `panel.rs:268-281`), then
  `this.update(...)` applies it **only if `generation` still matches**; if `stale` was set
  again in the meantime, schedule the next rebuild instead of pretending it is clean.
- If `stale && now < due`, arm a one-shot timer for `due` (trailing edge) — otherwise the
  last change of a save burst is dropped.
- While a rebuild is in flight the previous index keeps serving `@`.

`ProjectPanel` loses its own `stale`/`loading`/`generation`/walk, and gains:

```rust
pub fn set_index(&mut self, snapshot: Arc<Snapshot>, badges: HashMap<String, StatusBadge>, loading: bool, cx: &mut Context<Self>);
pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>);
```

`loading` keeps driving the refresh spinner (`panel.rs:784`) and the empty-body
placeholder (`panel.rs:862`). `TreeIndex::build` becomes `build(&Snapshot, badges)` (it
takes the snapshot by value today, `tree.rs:89`), stores `is_hidden` per node, and
`visible_rows(index, expanded, filter, show_hidden)` filters hidden rows. Toggling
`show_hidden` no longer re-walks.

**Alternative considered:** let the panel keep owning the walk and have `@` read
`panel.read(cx).snapshot()`. Rejected: it makes a composer feature depend on a panel entity,
and the panel already walks while closed, so hoisting costs nothing.

### 4.5 Recents

`mention_recents: Vec<String>` (workspace-relative, MRU, dedup, cap 32), pushed **from the
app**, not the panel (the panel cannot reach app fields):

- the app's `file_viewer` observer (`app.rs:1780-1784`, which already drives
  `panel.set_active`),
- the send path in `app/session.rs` (`fold_attached_files`), for each `@path` sent.

Cleared on workspace switch. Not persisted; no config surface. Only used for empty-query
ordering.

### 4.6 Invalidation triggers (must all reach the app)

| Site | Trigger | Immediate? |
|---|---|---|
| `app/events.rs:113-124` | workspace watcher dirty | no (throttled) |
| `app/session.rs:3042` | file created (immediately followed by `open_file_in_viewer` → `set_active` → `reveal_path`) | **yes** |
| `app/session.rs:3067, 3108, 3139` | file renamed / moved / deleted | **yes** |
| `app/session.rs:3275` | manual refresh action | **yes** |
| `explorer/panel.rs:779` | panel refresh button | **yes** |
| `explorer/panel.rs:817-818` | hidden-files toggle | no longer a walk at all (view filter) |
| `app.rs:2488` (`set_workspace`) | workspace switch | **yes** |

New API: `OrbitApp::invalidate_index(&mut self, immediate: bool, cx: &mut Context<Self>)`.
Immediate callers also rebuild when `now < due` (they clear `due` first). The panel's
refresh button goes through the existing `FileOpHandler`-style callback the panel already
has (`project_panel` is constructed with one, `panel.rs:1630`), so no new entity coupling.

---

## 5. File-by-file

| File | Change | Est. |
|---|---|---|
| `explorer/walk.rs` | `Entry`/`Snapshot` reshape, `build_parallel` + cap, VCS excludes via `filter_entry` (depth-0 safe, case-insensitive), hidden always on, `parents(false)` | +90 / −40 |
| `explorer/tree.rs` | `TreeIndex::build(&Snapshot, badges)`, `Node.is_hidden`, `visible_rows(..., show_hidden)`, test helper `snapshot(dirs, files)` (`tree.rs:320`) reshaped | +40 / −20 |
| `explorer/panel.rs` | drop `stale`/`loading`/`generation`/walk/git-badges; add `set_index`/`set_loading`; `show_hidden` = view filter; refresh button routes to `invalidate_index(true)`; panel test at `panel.rs:1633-1639` rewritten | +50 / −100 |
| `mentions.rs` | add `MentionIndex`; delete `list_workspace_files`, `search_fd`, `FdSearch`, `fd_pattern`, `escape_regex`, `score_path`, `path_depth`; `fuzzy_match` allocation-free; file branch of `filter_entries` → `MentionIndex::query` | +150 / −170 |
| `app/composer_ops.rs` | `at_paths` rewrite + memo; `ensure_mention_index`; drop `ensure_mention_files` and the fd cache | +100 / −80 |
| `app.rs` | 5 mention fields → 9 index fields; init; `invalidate_index`; memo field; `AUTOCOMPLETE_LIMIT` untouched | +45 / −25 |
| `app/events.rs` | watcher-dirty branch calls `invalidate_index(false, cx)` | +3 |
| `app/session.rs` | 5 call sites route to `invalidate_index(true, cx)`; `@path` sends feed recents | +15 / −5 |
| `app/send_control_tests.rs` | seed `mention_index` via `MentionIndex::from_paths` instead of `mention_files` + `mention_fd_missing` | +10 / −10 |
| `locales/*.yml` (**10 files**) | one key for the truncated row | +10 |

---

## 6. Deletions

- `mentions::list_workspace_files` (the 4000-entry bounded walk)
- `mentions::{search_fd, FdSearch, fd_pattern, escape_regex, score_path, path_depth}`
- `OrbitApp::{mention_files, mention_files_workspace, mention_fd_key, mention_fd_hits,
  mention_fd_missing}` and `ensure_mention_files`
- `ProjectPanel::{stale, loading, generation}` and its in-panel walk + badge scan
- tests `fd_pattern_matches_pi_file_and_path_queries` (including the `score_path` asserts at
  `mentions.rs:964-969`), `workspace_walk_skips_ignorable_dirs`
- the `fd` dependency mentions in comments/docs (verify: `locate("fd")` has exactly one
  caller today, `mentions.rs:509`, so `onboarding` itself is untouched)

---

## 7. Tests

1. **Scorer equivalence** — random paths (ASCII + CJK + spaces + dots) × random queries:
   the new `char_indices` scorer must return the same hit set and the same ranking keys as
   the old `Vec<char>` implementation (keep the old one under `#[cfg(test)]` for the
   comparison only).
2. **Index ordering** — basename hit outranks path hit; dirs tie-broken by depth then path;
   hidden loses ties but wins on score; empty query is depth-then-path; `limit` respected.
3. **Memo** — the same `(workspace, generation, query)` twice returns identical output and
   does not re-run `query` (assert on a call counter); a generation bump invalidates it.
4. **Stale race** — set `stale` while a rebuild is in flight; the applied snapshot must not
   clear it, and a second rebuild must be scheduled.
5. **Immediate invalidation** — `invalidate_index(true)` during the 2 s window rebuilds
   immediately (the reveal-after-create case at `session.rs:3042`).
6. **Recents** — empty query puts recents first, drops paths no longer in the index, dedups,
   caps at 32, clears on workspace switch.
7. **Walk** — existing three tests rewritten for `entries`: dotfiles are now **present** and
   flagged `is_hidden` (the old `snapshot_hides_dotfiles_until_toggled` assertion inverts);
   VCS metadata excluded even with hidden on; `.gitignore` honored in a plain folder;
   ancestor `.gitignore` ignored (`parents(false)`); `Thumbs.db`/`thumbs.db` both excluded;
   `truncated` set when the cap is hit; root is never pruned by `filter_entry`.
8. **Panel view** — `show_hidden` toggles rows without bumping `generation`; `set_index`
   clears the loading spinner; the panel test at `panel.rs:1633-1639` adapted.
9. **Truncated row** — present in the rendered entries, absent from `AutocompleteState::count`,
   and unreachable by ↑/↓/Enter/Tab.
10. Keep the existing `filter_*` tests (slash branch) plus new `MentionIndex::query` ranking
    tests.

No test may depend on `fd` being installed or on the real workspace (the `open_menu_on`
pattern in `send_control_tests.rs:20-35` stays).

---

## 8. Docs

`CHANGELOG.md` — one entry with five behavior changes:

- `@` file search no longer requires `fd`; it reads the same index as the Explorer.
- `.gitignore` now applies even when the workspace is not a git repository; ancestor
  `.gitignore` files are no longer consulted (this also changes the Explorer: opening a
  subfolder of a repo no longer inherits the repo root's ignore rules).
- Hidden files (`**/.*`, `.env*`, `.claude/**`, …) are now `@`-searchable; VCS metadata
  (`.git`, `.svn`, `.hg`, `.jj`, `.DS_Store`, `Thumbs.db`) is not.
- `@` is instant: no subprocess on the keystroke path.
- The Explorer's hidden-files toggle no longer triggers a re-scan.

`AGENT.md` — update the module table rows for `mentions.rs` / `explorer/walk.rs` (who owns
the workspace file list changed).

---

## 9. Risks

| Risk | Mitigation |
|---|---|
| A 200k-entry index (~4 `Vec`s, ~30 MB) is held for the session | measured 20 ms build / 90k; one `Arc` shared with the panel; `truncated` discloses the cap |
| Parallel walk + `Quit` overshoots the cap | accepted (decision 12) |
| Rebuild on every watcher burst burns CPU (0.43 s × saves) | lazy + 2 s throttle with a trailing edge; the previous index keeps serving meanwhile |
| A rebuild that lands during another change silently loses it | `stale` clears at rebuild *start*, re-arms on landing (P1-2) |
| Panel refactor regresses loading/empty states | `loading` stays a panel field fed by `set_loading`; `error`/`notice` untouched |
| `parents(false)` reveals previously ignored trees when a workspace is a subfolder of a repo | documented in CHANGELOG; `require_git(false)` still keeps in-workspace `.gitignore` authoritative |
| Immediate (non-throttled) rebuilds from file ops could stampede on bulk operations | immediate path only clears `due` when the caller is a single file op; bulk watcher bursts stay throttled |
| Non-ASCII scoring drift after the allocation-free rewrite | test 7.1 pins identical hit sets and ranking keys |

---

## 10. Verification

```
cargo fmt --all -- --check
cargo clippy -p orbit-pi --all-targets -- -D warnings
cargo test -p orbit-pi
```

Manual, on `F:\WorkSite\_main_orig`:

- `@Player` and `@Playwr` — no perceptible stall, same or better results than today.
- `@Library` returns nothing (`.gitignore` has `/Library/*` and the workspace root holds it).
- `@.claude` returns hits; `@.git` returns nothing.
- Toggling the Explorer's hidden-files eye does not spin the refresh glyph.
- Create a file, watch it appear in the tree it was revealed in (the immediate path).
- Save twice in a row with the tree open: the second change appears within ~2 s.

The §1 numbers come from `/tmp/bench_fuzzy2.rs` against
`fd --no-require-git --type f --type d --hidden --exclude .git` (90,395 paths) and are
re-runnable if needed.

---

## 11. Non-goals

- No nucleo, no semantic/embedding search, no persistent on-disk index.
- No per-path incremental index updates (whole-snapshot rebuild, throttled).
- No change to `AUTOCOMPLETE_LIMIT` (8), the trigger parser, or the send/fold path.
- No new user-facing settings.
- `fd` is not re-added behind a flag; if a future need appears, it is a new decision.

---

## 12. Review status

Reviewed (`docs/plans/mention-index.review.md`, verdict **OK with notes**). All eight
claims verified; P1-1…P1-5 and P2-1…P2-8 are folded in above:

- P1-1 memo (§3.13, §4.3), P1-2 stale race (§3.9, §4.4), P1-3 trailing edge (§3.9, §4.4),
  P1-4 invalidation routing (§4.6), P1-5 index root (§3.14, §4.4).
- P2-1 `filter_entries` stays for commands (§4.2), P2-2 test gaps (§7), P2-3
  `TreeIndex::build(&Snapshot)` (§5), P2-4 recents pushed app-side (§4.5), P2-5
  case-insensitive metadata match (§3.4, §4.1), P2-6 `parents(false)` documented (§8, §9),
  P2-7 truncated row not selectable (§3.11, §7.9), P2-8 loading contract (§4.4).
- `ignore 0.4.33` carries `filter_entry` (`~/.cargo/registry/.../ignore-0.4.33/src/walk.rs:1046`)
  and applies it to depth-0 entries — hence the explicit root rule in §4.1.
- Locale file count is **10**, not 11 (§5).

## 13. Implementation notes (as built)

- The eager build is triggered from `set_current_workspace` (the funnel for
  workspace pick / session restore / worktree switch), **not** from
  `sync_review_context`: the latter runs on every session switch and turn
  checkpoint, and a walk per call cost ~100 s across the test suite. The panel
  also asks for an index the first time it is opened without one, and `@`
  triggers it lazily — so nothing can ask for a menu that has no index behind it.
- Stale-clearing, the trailing-edge timer, the memo and the generation guard are
  all in `composer_ops.rs` (`ensure_mention_index` / `schedule_mention_rebuild` /
  `invalidate_index` / `at_paths`).
- The truncated notice is rendered by `autocomplete_popup` as a plain dim row
  that is not an `AcEntry`, so `AutocompleteState::count` and the ↑/↓ index can
  never reach it by construction.
- Test coverage landed: walk (gitignore-without-git, ancestor ignore, metadata
  exclusion incl. case, hidden flagging, ordering, symlinks), tree (hidden view
  filter), mentions (scorer equivalence against the old `Vec<char>` oracle over
  400 randomised non-ASCII cases, ranking, empty query, hidden tie-break,
  recents), and app (empty menu leads with the most recent file, dead recents
  dropped). Not unit-tested (app-level timing): the stale-race and
  immediate-invalidation paths — they need a live `Context` and are covered by
  the manual check in §10.
