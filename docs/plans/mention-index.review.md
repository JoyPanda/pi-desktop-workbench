I couldn't write the review file because no write tool is available in this run, so the full report is below. The runtime will save it to the configured path.

```markdown
# Review: docs/plans/mention-index.md

## Summary
The diagnosis is right. With `fd` installed, `@` runs it on the UI thread (`composer_ops.rs:51-84`
→ `search_fd`, `mentions.rs:508-571`), and when the run fails it returns `Hits(vec![])` rather than
falling back to the walk (`mentions.rs:550-552`). That makes the bounded walk unreachable whenever
`fd` exists. Moving to one shared in-memory index is the right direction. But the plan misses several
consumers and has three behaviour bugs in its design (memo, stale race, trailing rebuild). It needs one
more revision before implementation.

## Claims checked (8/8 hold; line-number fixes noted)
1. `mentions.rs`: `fuzzy_match` :296 (collects `Vec<char>`), `filter_entries` :326, `list_workspace_files` :373 (4000 cap),
   `FdSearch` :426, `fd_pattern` :436, `escape_regex` :458, `score_path` :473, `path_depth` :497, `search_fd` :508.
   `locate("fd")` has one caller, :509. Confirmed.
2. `composer_ops.rs`: `at_paths` :51-84 (fd first, cached by `mention_fd_key`, walk fallback); `ensure_mention_files` :140-150. Confirmed.
3. `walk.rs`: `MAX_ENTRIES` :18, `Snapshot{dirs,files,truncated}` :23-28, `.hidden(!show_hidden)` :49, `.parents(true)` :54,
   `.require_git(false)` :56, `builder.build()` :60. Confirmed.
4. `panel.rs`: `set_workspace` :213, `mark_stale` :232, `set_active` **:240** (the plan's "panel.rs:232" points at `mark_stale`),
   `ensure_loaded` :255, `git::status_rows` :275 (the plan's 268-281 is right).
5. `tree.rs`: `TreeIndex::build(snapshot: Snapshot, …)` :89 takes the snapshot **by value** and reads `dirs`/`files` at :94/:98;
   `visible_rows(index, expanded, filter)` :229.
6. `app.rs`: the five `mention_*` fields at :794-801 are confirmed; `AUTOCOMPLETE_LIMIT = 8` is at :109.
7. `events.rs:113-124`: the watcher-dirty branch calls `project_panel.mark_stale`. Confirmed.
8. `send_control_tests.rs:23-35`: `open_menu_on` seeds `mention_fd_missing`, `mention_fd_key`, `mention_fd_hits`, `mention_files_workspace`, `mention_files`. Confirmed.

## Findings

**P1-1: §4.3 drops the per-query cache, but the `@` query runs every frame.**
- `sync_autocomplete` is documented as "runs every frame" (`composer_ops.rs:155-157`). It calls `at_entries` (:177-180).
- `autocomplete_entries` (:29-38) calls it again when the menu renders.
- Today `mention_fd_key`/`mention_fd_hits` (`composer_ops.rs:55-63`) stop repeat searches on redraws. The plan deletes that cache with nothing in its place.
- At the plan's own figure of ~6 ms per query on 90k entries, that is ≥12 ms per frame while the menu is open, not "instant".
- Fix: memo `(workspace, index_generation, query) -> Vec<String>` and serve repeat frames from it.

**P1-2: Stale race. A change during the rebuild is lost.**
- §4.4 says: "applies it if `generation` still matches, sets … `stale = false`". The watcher sets `stale = true` without bumping `generation`.
- So a save that lands during the ~0.43 s walk gets cleared when the older snapshot is applied.
- Fix: clear `stale` when the rebuild **starts** (as `panel.rs:263` does today). On apply, if `stale` is set again, schedule another rebuild.

**P1-3: The throttle has no trailing edge, and nothing drives the "lazy" rebuild when `@` isn't used.**
- The "mirror §30" pattern (`worktrees.rs:168-178`) is leading-edge only: events inside the 2 s window are just dropped.
- `ensure_mention_index` is called only from `at_paths`. So with the Explorer open and `@` never typed, the last change in a save burst never shows up.
- Fix: when `stale && now < due`, spawn a timer for `due`, or call `ensure_mention_index` from the watcher branch, `set_workspace` and panel open as well.

**P1-4: The plan's file table (§5) misses explicit panel refreshes, and the 2 s throttle would make them slow.**
- `panel.mark_stale` callers not in §5:
  - `session.rs:3042, 3067, 3108, 3139`: file create/rename/delete. 3042 then immediately calls `open_file_in_viewer` → `set_active` → `reveal_path`.
  - `session.rs:3275`: the manual refresh action.
  - `panel.rs:779`: the panel's refresh button.
- `panel.rs:817-818`: the hidden toggle, which the plan does mention.
- Under the new design these must reach the app-owned index. The panel needs a callback (like `on_file_op`) because it cannot write app fields.
- They also must **skip** the 2 s throttle. Otherwise a newly created file is missing from the tree it was just revealed in.
- Fix: add an `invalidate_index(immediate: bool)` app method, route all six sites to it, and list them in §5.

**P1-5: The Explorer and `@` use different roots today.**
- The Explorer root is `current_workspace.or_else(current_dir)` (`app.rs:2458-2461` → `panel.set_workspace` :2489).
- `at_paths` returns nothing without `current_workspace` (`composer_ops.rs:52`).
- §4.4 keys the shared index on `mention_index_workspace` and doesn't say which root wins.
- Fix: state that the index uses the panel's root (with the `current_dir` fallback) and that `at_paths` keeps its `current_workspace` gate. Otherwise the Explorer silently loses the fallback.

**P2-1: `filter_entries` also serves slash commands.**
- §5 says "`filter_entries` → `MentionIndex::query`", but `filter_entries` is also the slash path (`composer_ops.rs:31, 168`).
- Fix: say it stays for commands and only the file branch moves.

**P2-2: Tests that §7 does not list.**
- `panel.rs:1633-1639` (`walk::snapshot(&root, false)` + `panel.stale`).
- The `tree.rs:320` `snapshot(dirs, files)` helper.
- Walk tests that assert today's hidden-toggle behaviour (`walk.rs` `snapshot_hides_dotfiles_until_toggled`) must be rewritten, not "adapted".
- The `score_path` asserts at `mentions.rs:964-969` go with the deleted fd test.

**P2-3: `TreeIndex::build` takes `Snapshot` by value (`tree.rs:89`).**
- An `Arc<Snapshot>` shared with the index means changing it to `&Snapshot`. List this in §5.

**P2-4: Recents write site.**
- `ProjectPanel::set_active` is driven by the app's `file_viewer` observer (`app.rs:1780-1784`).
- Push recents there (app side), not inside the panel, which cannot reach `mention_recents`. Fix the cited line (:240, not :232).

**P2-5: `Thumbs.db` casing is inconsistent.**
- §3/§4.1 exclude `Thumbs.db`; §7.4 tests `thumbs.db`. A plain name check is case-sensitive, and Windows/macOS filesystems are not.
- Fix: compare case-insensitively (`eq_ignore_ascii_case`).

**P2-6: `parents(false)` is also a behaviour change for the Explorer, not just `@`.**
- Today `parents(true)` (`walk.rs:54`) applies the repo-root `.gitignore` when the workspace is a subfolder of a repo.
- With `false`, opening `repo/sub` starts indexing whatever the root `.gitignore` excluded (`node_modules`, Unity `Library`).
- The §10 check "`@Library` returns nothing" only holds if the ignore file is inside the workspace.
- Fix: record this in §9 Risks / CHANGELOG, or limit `parents` to the git root (`require_git` semantics) instead of turning it off.

**P2-7: The truncated row in the `@` menu.**
- `state.count` / `highlighted` (`composer_ops.rs:181-183`) must exclude the non-selectable row, or Enter/Tab can land on it.
- Add a test. Also confirm the locale file count (11) before writing "+11".

**P2-8: Panel loading state.**
- The `loading` flag drives the refresh spinner (`panel.rs:784`) and the empty-body placeholder (`panel.rs:862`).
- §9 mentions "a loading flag driven by the app", but §5 lists no `set_loading`/`set_index` contract for it.
- Add it to the `set_index` signature or a sibling setter.

## B. Index ownership (§4.4)
Moving the index up to the app is sound, and the rejected alternative really is worse:
- Today the panel walks even when closed (`set_workspace`/`mark_stale` → `ensure_loaded` with no `open` check, `panel.rs:226-236`), so one app-level walk costs no more.
- It removes the `@` → panel-entity dependency.
- Git badges belong with the rebuild.

The seam is only sound if the app takes over every trigger the panel owns today: file ops, the refresh button, the hidden toggle, the workspace root fallback, and the loading state (P1-4, P1-5, P2-8). The panel then becomes a pure view with `set_index` plus a refresh callback. Keeping the walk in the panel only saves diff size; it doesn't avoid any of that work.

## C. `ignore` usage (§4.1)
- `hidden(false)` means "do not skip hidden" (the default is `true`), so the plan's reading is correct.
- `require_git(false)`, `parents(false)`, `git_global`, `git_exclude`, `max_depth`, `follow_links` all exist on `WalkBuilder` in 0.4.x.
- `WalkBuilder::filter_entry` exists in recent 0.4.x releases and applies to both `build()` and `build_parallel()`. Returning `false` for a directory prunes the walk below it, so `.git` (directory or worktree file) is skipped cheaply.
- `build_parallel().run(|| Box::new(|r| …WalkState::Quit))` is the correct API. Collecting needs a `Mutex`/channel plus an `AtomicUsize`, and output order is nondeterministic. That is fine here because both the index and the tree sort.
- Unverified: I did not open the locked `ignore` version (out of scope). Confirm it is ≥ the release that added `filter_entry`, and that `filter_entry` is not applied to the depth-0 root.

## D. Missing steps
- Throttle trailing edge and a memo (P1-1, P1-3).
- File-op/refresh rerouting (P1-4).
- Root fallback (P1-5).
- The panel test and the tree test helper (P2-2).
- `TreeIndex::build` signature (P2-3).
- Locales: one key is enough, but count the files.
- Docs: `AGENT.md` rows plus CHANGELOG (§8) are fine. Add the `parents(false)` Explorer change (P2-6).
- Moving git badges into the app rebuild is covered (§4.4).

Verdict: OK with notes. Fix P1-1 through P1-5 in the plan before implementation; none of them is a reason to reject the approach.
```