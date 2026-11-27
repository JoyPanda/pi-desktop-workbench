# Plan — Migrate the UI foundation to gpui-pre 0.3.3, then adopt gpui-ai card components

**Status:** Approved plan (grill session 2026-11-27). Not started. Implementation happens
on a dedicated branch, in a separate session.
**Owner:** TBD
**Scope:** `crates/orbit-pi` (the only crate that depends on gpui), `third_party/`,
workspace `Cargo.toml`. `crates/orbit-rpc` is untouched.
**Related:** `transcript_view.rs` (bespoke transcript, 12.7k lines), `message_scroller.rs`
(tail-follow), `diff_select.rs`, `markdown.rs`, `highlight.rs`, `main.rs` (bootstrap),
`build.rs` + `resources/windows.manifest` (Windows resources),
`third_party/gpui/UPSTREAM.md`, `zed-design-tokens.md`
**Upstream under evaluation:** <https://github.com/labcoder/gpui-ai> (gpui-ai 0.8.0, MIT;
depends on gpui-pre 0.3.3 / gpui-component 0.6.0 / gpui-base 0.6.0, all Apache-2.0)

---

## 1. Goal and motivation

The original ask was "replace our StyledText-based rendering wholesale with gpui-ai".
The driving motives are (a) maintainability of the hand-rolled transcript
(`transcript_view.rs`, 12,703 lines) and (b) visual polish.

The evaluation (two scout passes over this repo, the gpui-ai clone, and the unpacked
crates.io sources of gpui-pre 0.3.3 / gpui-component 0.6.0 / gpui-base 0.6.0) found that
**a wholesale replacement is not on offer**:

- gpui-ai's components are thin `RenderOnce` shells; their substance is
  gpui-component's `TextView` (markdown + highlight + selection) and gpui-pre's
  window-level selection, `FollowMode`, and accessibility tree. None of these exist in
  our vendored gpui 0.2.2, and two GPUI copies cannot coexist in one binary.
- `TextView` requires content to flow through a markdown/HTML state model
  (`gpui_base::TextViewState`). Our transcript is a heterogeneous element tree (tool
  cards, diffs, mermaid, images, changed-files). Prose and code text therefore stay
  bespoke; `StreamingText`/`CodeBlock` do not fit.
- gpui-ai lacks seven behaviors we deliberately built: mermaid rendering, inline edit
  diffs from tool args (`oldText`/`newText`), tool-output truncation caps, the
  changed-files card, "Worked for" turn folding, the wheel-gesture tail hold, and the
  thinking card's independent follow policy.

**The redefined prize** is the foundation itself: upstream list rewrite, free ClearType,
platform modernization, a net reduction of hand-rolled scroll infrastructure — plus
gpui-ai's *card chrome* (ToolCall, Thinking, ApprovalCard, StatusBadge, TodoList,
CodeDiff) as a Phase 2 polish layer.

## 2. Decisions

- **D1 — Scope: transcript only.** Explorer viewer, docked review pane, and the
  composer (input, queued prompts, model picker) stay bespoke. gpui-ai entity components
  (Chat, PromptBar, CommandSearch, …) are never adopted: application state stays owned
  by the app (gpui-ai's own README prescribes this).
- **D2 — Two phases, one branch each.** Phase 1 swaps the foundation with **zero
  behavior change** and is independently mergeable. Phase 2 adopts components one at a
  time, each behind the behavior checklist (D5). Phase 2's final scope is decided
  *after* Phase 1 lands, from measured results — not from this document's guesses.
- **D3 — Dependency form.** Pin the whole gpui-pre family to **`=0.3.3` exactly**.
  Vendor only the two crates that need patches into `third_party/` and redirect with
  `[patch.crates-io]`; everything else (gpui-pre-platform, -macos, -linux, gpui-base,
  gpui-component, gpui-kit-assets) comes from crates.io. `third_party/gpui` retires.
- **D4 — Version lock.** A gpui-pre version bump is its own project, never a routine
  `cargo update`: **0.3.14 has deleted the ClearType/subpixel code** that 0.3.3 carries.
  Semver says 0.3.3 → 0.3.x is safe; it is not. Record this in the new
  `third_party/*/UPSTREAM.md` and in Cargo.toml comments.
- **D5 — 1:1 behavior is a hard requirement; the checklist is the merge gate.**
  Native mechanisms are preferred, but a hand-rolled behavior is only deleted when its
  checklist item passes on the new foundation. Items that fail stay as app code
  (ported). The checklist grows on the existing test suite (~95 test markers in
  `transcript_view.rs`, ~23 in `message_scroller.rs`; 363 `TestAppContext` usages).
- **D6 — Selection: hybrid, native-first where it passes.** gpui-base's selection is
  stronger at cross-row coverage and drag auto-scroll, but fails four checklist items
  out of the box (§5). Those stay hand-rolled. Do **not** fork gpui-base to close the
  gaps (rejected option: largest fork surface, contradicts the maintainability motive).
- **D7 — Wrap-first for gpui-ai, no deep forks.** Where a gpui-ai component cannot host
  one of our behaviors, that block type stays bespoke and is interleaved with gpui-ai
  components. Never fork gpui-ai to force a fit.
- **D8 — Theme bridge, not a new look.** Write our existing design tokens
  (`zed-design-tokens.md` / current `Theme`) as a gpui-component theme JSON so
  `ActiveTheme` serves the same values. Bespoke surfaces and gpui-ai components must
  share one token source; no visible color drift.
- **D9 — Windows manifest: keep ours.** The fork makes gpui-pre's `windows-manifest`
  embedding a no-op; `crates/orbit-pi/resources/windows.manifest` stays as-is
  (`longPathAware` and the legacy-OS GUIDs are real requirements for a workbench that
  opens arbitrary repos; gpui's own manifest lacks both).
- **D10 — Merge gate platforms: macOS + Windows hard, Linux best-effort.** All three
  headline risks (manifest collision, ClearType, white-frame flash) are on Windows.
  If no Windows environment is available at implementation time, downgrade this
  explicitly in the PR description as a known risk — never claim untested parity.
- **D11 — Stop-loss.** At the end of Phase 1, hand-rolled infrastructure code must be
  **net smaller** (at least one of {selection stack, scroller patches} partially
  replaced by native equivalents). If we end up carrying the same hand-written code
  *plus* a fork on a new foundation, abandon the migration and fall back to
  borrowing gpui-ai's visual design only.
- **D12 — Rollback is git-only.** Two GPUI type systems cannot coexist in one binary;
  there is no runtime fallback. Phase 1 runs on a branch and must pass the full
  checklist before merge, because merging removes the cheap escape hatch.

## 3. Patch disposition (the four local gpui commits)

| local commit | what it does | disposition on gpui-pre 0.3.3 |
|---|---|---|
| `3a6fb06` ClearType | ports Zed PR 45423 (`color_text_raster.hlsl`, subpixel variants, `SPI_GETFONTSMOOTHINGTYPE`) | **Drop.** Byte-identical upstream: `gpui-pre-windows-0.3.3/src/color_text_raster.hlsl`; `direct_write.rs:847` `DWRITE_TEXTURE_CLEARTYPE_3x1`; core `SubpixelSprite` in gpui-pre `scene.rs:730`. Gone again in 0.3.14 → D4. |
| `3a6fb06` (undocumented hunk) | vendor removed upstream's opaque-window white clear (`UPSTREAM.md:13`) | **Re-patch in fork.** `gpui-pre-windows/directx_renderer.rs:354` clears `Opaque => [1.0;4]`; migrating unpatched reintroduces the white frame flash. |
| `3e6368f` `ListScrollEvent::offset_from_bottom` | +4 lines in `list.rs`, read at `message_scroller.rs:98` for the 50px `TAIL_SLACK` | **Recompute in app** from `viewport_bounds()` / `is_scrolled_to_end()` (`list.rs:484`) and the new `ListScrollEvent::is_following_tail` (`:172-184`). No patch. |
| `2ea39b8` `ListItem::Stale` + `invalidate(range)` | keep last measured height while streaming | **Mostly upstream** as `ListItem::Unmeasured { size_hint }` + `remeasure_items(range)` (`list.rs:412`), which also preserves the reader's intra-row offset via `PendingScroll::Absolute`. Call sites `message_scroller.rs:247,288` map 1:1. Replay-test needed: upstream also moves `logical_scroll_top`; our `invalidate` did not. |
| `07c97e4` three wheel fixes | clamp `scroll_top` before wheel; stop folding accumulated deltas onto paint-time offset; report top-of-viewport row | **Re-port in fork.** Upstream `StateInner::scroll` (`list.rs:915-918`) still subtracts from the past-the-end offset for Bottom alignment; `FollowMode`/`stop_following` mitigate but do not fix. |
| `2ea39b8` window.rs test helpers | `is_dirty` / `flush_next_frame` under `cfg(test)`, used by `message_scroller.rs:1204-1205` | Keep as a tiny fork hunk or restructure the two tests. |

Selection is **not** patched into gpui today (it is all app code:
`transcript_view.rs:295-1300`, `diff_select.rs`); nothing to port in `window.rs`.

## 4. Breaking API surface (Phase 1 compile work)

Concrete, from diffing unpacked 0.3.3 sources against app call sites:

1. **Bootstrap.** `Application::new()` is gone (`main.rs:649`). Add
   `gpui-pre-platform` (rename `gpui_platform`) and call `gpui_platform::application()`;
   tests use `gpui_platform::headless()`. Platform backends moved to
   `gpui-pre-{macos,windows,linux,web}`.
2. **Feature flags move.** `font-kit, runtime_shaders, wayland, x11` (and dev-dep
   `test-support`) move from the `gpui` dependency to `gpui-pre-platform`
   (`crates/orbit-pi/Cargo.toml:30-35` and workspace `:189`).
3. **Windows manifest collision (hard blocker).** `gpui-pre-platform` enables
   `windows-manifest` unconditionally; gpui-pre `build.rs:9-22` embeds
   `resources/windows/gpui.manifest.xml` → CVT1100 duplicate resource against our
   `build.rs:185-189` embedding. Resolved by D9 (fork no-ops the embed).
4. **`HttpClient::type_name` removed.** `http.rs:31-34` won't compile; drop the method.
   `AsyncBody` unchanged.
5. **Stale pins.** `cocoa = "=0.26.0"` was pinned to match gpui 0.2.2; gpui-pre-macos is
   objc2-based. Re-verify `raw-window-handle` / `windows-sys` against gpui-pre-windows.
6. **Low risk / unchanged:** `list`/`uniform_list` constructors, `ListState` basics,
   `StyledText`/`TextRun`, `InteractiveText`, `canvas`, `Window::paint_svg`,
   `TitlebarOptions`, `AssetSource`, `TestAppContext`/`VisualTestContext`,
   `WindowOptions` (superset).
7. **rust-i18n registry audit.** gpui-component depends on `rust-i18n 4.2` and loads
   `locales/ui.yml` into the same process-global `t!` registry the app uses. Audit for
   key/locale collisions before merge.
8. **Assets multiplexing.** One `AssetSource` per app: our `assets::Assets`
   (`assets.rs:41`) must multiplex its own tree with `gpui_kit_assets::Assets` (101
   Lucide icons; keep it a regular dependency — its `build.rs` `links =` key feeds
   `IconName`'s proc macro). **NOTICE:** add Lucide/ISC attribution (gpui-kit-assets
   declares Apache-2.0 but ships ISC-licensed Lucide files, no LICENSE in the tarball).

## 5. Selection checklist vs gpui-base 0.6 (input to D5/D6)

Engine: `gpui-base/src/text_selection.rs` (3,554 lines), surfaced via
gpui-component `Root`'s `TextSelectionLayer` (`root.rs:595`).

| item (our `transcript_view.rs:295-1300`) | native | verdict |
|---|---|---|
| char/word/line granularity | yes (`points_for_multi_click:420-436`) | pass |
| 1→2→3→1 click cycling | no — 4th+ click stays line (`:1894-1913`); our `LastClick` wraps (`:396-407`) | **fail → keep app logic** |
| line = wrapped visual row | no — logical line (upstream test `:2468`) | **fail → keep app logic** |
| CJK-aware word segmentation | no — ASCII/Latin/Cyrillic/Vietnamese only (`text/selection.rs:17-30`); ours uses `unicode_segmentation` + joiners (`:825-903`) | **fail → keep app logic** |
| right-click menu (Copy Selection / Copy Message) | absent (no `MouseButton::Right` handling) | **fail → keep `TextMenu`** |
| selection across rows / non-text gaps | yes, better: `document_order` + `TextSelectionCoverage` (`:131-146,204-280`); arbitrary elements can register via `TextSelectionRun::new` (`:296`) | pass (candidate) |
| copy assembly | yes (`resolve_copy_items:497-517`, sorted, skips blanks) | pass (candidate) |
| drag auto-scroll / past-panel drag | built in (`AutoScroll:488-496`, capture-phase layer) — **collides** with our canvas capture listener (`register_text_selection:1298`); suppression hatch exists (`GlobalState::reset_text_selection_suppression:1889`) | needs design care |
| virtualized rows | participants register per frame; scrolled-out rows lose registration; our snapshot-at-drag-start (`SelectedBlock:310-315`) is immune | verify before adopting |

## 6. Phase 1 — foundation swap (zero behavior change)

Branch: `feat/gpui-pre-foundation` (suggested). Steps, roughly in order:

1. Vendor `gpui-pre` 0.3.3 and `gpui-pre-windows` 0.3.3 sources into `third_party/`,
   each with an `UPSTREAM.md` (origin: zed@5b055fa re-publish; patch list; D4 warning).
   Apply the fork hunks from §3 (wheel fixes ×3, white-clear removal, manifest no-op,
   test helpers). Wire `[patch.crates-io]`.
2. Workspace + crate Cargo.toml: `gpui = { package = "gpui-pre", version = "=0.3.3" }`,
   add `gpui_platform = { package = "gpui-pre-platform", version = "=0.3.3", features =
   [...] }`, add gpui-component `=0.6.0` (needed for Phase 2 theming/Root; may land in
   step 5 instead). Remove `third_party/gpui` and its workspace exclude.
3. Fix the compile surface (§4): bootstrap, features, `HttpClient`, pins.
4. Port `message_scroller.rs`: `invalidate` → `remeasure_items` (replay-test the
   `logical_scroll_top` difference), `offset_from_bottom` → recomputation. Port
   `SelectableText`/`TextSelection` unchanged (they sit on stable APIs).
5. Theme bridge (D8): tokens → gpui-component theme JSON; wrap the top-level view in
   `gpui_component::Root`; `gpui_ai::init(cx)` only when Phase 2 starts.
6. Asset multiplexer + NOTICE + rust-i18n audit.
7. Port and green the full test suite; run the behavior checklist (§7) on macOS +
   Windows (D10); measure the stop-loss (D11): diff of hand-rolled infra LOC before /
   after. Record the numbers in the PR.

**Merge gate:** tests green + checklist pass on both hard platforms + D11 satisfied.

## 7. Behavior checklist skeleton (grows into tests)

Selection: word/line/char grains · CJK word join · 1-2-3-click wrap · visual-line
selection · drag across rows and past panel edge · drag auto-scroll · right-click menu
(Copy Selection / Copy Message) · copy assembly order and blank-row skipping ·
selection survives streaming splices.
Scrolling: tail-follow while streaming · scrolled-up reader undisturbed by growth
(both prior regressions `07c97e4`, `2ea39b8`) · trackpad flick still streams (wheel
tail hold) · thinking card independent follow · jump-to-latest + unread dot · fold
earlier work only after the next prompt (`ffb8657`) · queued prompts return to input
(`ffb8657` neighborhood, composer regression).
Rendering: prose spans/links/autolink arrow marks · tables (`fit_columns`) · alerts ·
task lists · images · mermaid fences → diagrams · syntax colors measure-stable on
streaming code · tool cards (truncation caps, collapsed previews, inline edit diff,
error strip) · changed-files card.
Platform: Windows ClearType subpixel rendering · no white frame flash on opaque
windows · per-monitor-v2 DPI crispness · `longPathAware` honored.

## 8. Phase 2 — gpui-ai card chrome (scope re-decided after Phase 1)

Candidate set (all stateless `RenderOnce`): **ToolCall/ToolGroup, Thinking,
ApprovalCard, StatusBadge, TodoList, CodeDiff** (feed it a unified patch generated
from `oldText`/`newText`). Default-excluded: StreamingText, CodeBlock (TextView model
mismatch, §1), all entity components (D1).

Rules: one component per commit-sized step; each step gated by the checklist items it
touches; wrap-first, no deep forks (D7); the seven reverse-gap behaviors survive as
bespoke blocks interleaved with adopted components. If a component cannot pass its
items, it is not adopted — the checklist outranks the component set.

## 9. Risks and stop-loss

- **Stop-loss (D11):** Phase 1 must end with net less hand-rolled infrastructure.
  Expected wins: `invalidate`/`Stale` machinery and `offset_from_bottom` patch die;
  ClearType fork dies. Expected carry-overs: wheel fixes (fork), selection granularity
  (app). If the measured diff says otherwise → abandon, fall back to design-borrowing.
- **Version drift (D4):** `=0.3.3` pins; a bump re-runs this evaluation.
- **Windows regressions:** manifest collision, white clear, ClearType — all covered by
  fork hunks + D10 gate; no Windows env ⇒ explicit documented downgrade, not silence.
- **rust-i18n key collisions:** audit in Phase 1 step 6; failure mode is wrong UI
  strings, caught by review.
- **Selection capture-phase collision:** if native `TextSelectionLayer` and our canvas
  listener ever coexist, use the suppression hatch or keep our listener exclusively;
  never both uncoordinated.
- **No runtime fallback (D12):** branch discipline + full pre-merge regression.

## 10. Out of scope

Composer/PromptBar, explorer viewer, review pane rendering, gpui-base selection forks,
gpui-pre version upgrades, gpui-ai entity components, new themes/visual redesign.