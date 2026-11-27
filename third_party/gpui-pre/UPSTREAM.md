# Vendored gpui-pre

Source: crates.io `gpui-pre` 0.3.3
(sha256 `f8b0f6f593153b6eb84c336ea7ccf12a1fca186f0db99af628e72f85048c1d7b`).
It is Zed's `gpui` crate re-published from Zed commit
`5b055fa789a8b8d38ac951a6e0cde272f66b4495` (the source declares
`zed-crate = "gpui"`, `zed-version = "0.2.2"`, `zed-rev = "5b055fa"`).

This tree keeps `src/`, `resources/`, `build.rs`, `Cargo.toml`,
`Cargo.toml.orig`, `LICENSE-APACHE`, and `README.md`. Examples and integration
tests from the published crate are not included, and their `[[example]]` /
`[[test]]` targets are dropped from `Cargo.toml` so the tree builds.

## Version lock — read before touching any version

**Do not float these pins.** The whole gpui-pre family is pinned to `=0.3.3`
exactly (workspace `[patch.crates-io]`, `crates/orbit-pi/Cargo.toml`, and the
`=0.3.3` pins inside this tree).

**0.3.14 has deleted the ClearType/subpixel rasterizer** that 0.3.3 still
carries (`scene.rs` `SubpixelSprite`, `gpui-pre-windows/src/color_text_raster.hlsl`,
`DWRITE_TEXTURE_CLEARTYPE_3x1` in `direct_write.rs`). Semver says 0.3.3 → 0.3.x
is compatible; for this app it is not — a bump silently returns Windows text to
grayscale and re-opens the Cleartype work. A version bump is its own migration
project with the behavior checklist behind it, never a routine `cargo update`.

## Local hunks

Five app-relevant defects in 0.3.3 are patched here; everything else is
byte-identical to the published crate.

1. **`src/elements/list.rs`, `StateInner::scroll` — wheel tail fixes** (ported
   from this repo's gpui 0.2.2 fixes, commit `07c97e4`; upstream still has all
   three).
   - Clamp the current offset to `scroll_max` *before* applying the wheel. A
     bottom-aligned tail has no anchor, so `scroll_top()` reports the content
     end — one viewport past the real maximum — and a normal notch would be
     clamped straight back to the tail.
   - Report the row actually at the top of the viewport in `ListScrollEvent`.
     The incoming offset is the past-the-end sentinel while pinned, so upstream
     marks one row past the last item as visible.
   - In `List::paint`, apply only the current event's delta to the live
     `logical_scroll_top` and the live `viewport_bounds()` height. Upstream
     keeps one listener per frame that folds every delta onto the scroll
     position captured at first paint, which replays a whole gesture.
2. **`src/window.rs` — `Window::is_dirty`** under `cfg(any(test,
   feature = "test-support"))`. The `message_scroller.rs` tests assert that a
   redraw scheduled from prepaint actually marked the window dirty. Draining the
   queue uses upstream's `Window::simulate_next_frame`.
3. **`build.rs` — the Windows manifest embed is a no-op** (decision D9). Upstream
   embeds `resources/windows/gpui.manifest.xml` whenever the `windows-manifest`
   feature is on, and `gpui-pre-platform` turns that feature on unconditionally.
   `crates/orbit-pi/build.rs` embeds the app's own
   `resources/windows.manifest`, so two `RT_MANIFEST` resources make CVTRES fail
   the link with `CVT1100: duplicate resource`. The app's manifest is the
   superset (per-monitor-v2 DPI, `longPathAware`, `asInvoker`, supportedOS
   list) and is the one that ships. The feature flag is kept declared because a
   dependency enables it.

4. **`src/window.rs`, `Window::dispatch_event` — the modality refresh runs after
   the event is dispatched.** Upstream marks the window dirty the moment the
   input modality changes (keyboard ⇄ mouse, for focus-visible styling and
   hover suppression) and *before* dispatching, and
   `dispatch_key_event`/`dispatch_mouse_event` begin by drawing a dirty window.
   That draw installs the window root's frame before the dispatch reads it — the
   correct thing for the app, whose root view is the surface, but it discards the
   frame of an element a test painted with `VisualTestContext::draw` (the probe
   is not part of the root), so the focused node is gone and the key is dropped.
   Modality only affects paint, so the dirty mark now happens after the dispatch:
   the same frame is painted either way, just not in the middle of a dispatch.
5. **`src/app/test_context.rs`, `VisualTestContext::draw` — no arena clear.** The
   arena moved from a thread-local (`gpui` 0.2.2, cleared by the next
   `Window::draw`) into the `App`, and upstream now clears it at the end of the
   probe draw. A probe's deferred elements (`deferred(...)` overlays) are still
   queued in `next_frame.deferred_draws` for the next real draw to prepaint, so
   that clear frees them out from under it and the next draw panics with
   "attempted to dereference an ArenaRef after its Arena was cleared". The clear
   is left to the `Window::draw` that consumes them, as in 0.2.2.

Nothing else is forked: `ListItem::Unmeasured` / `remeasure_items` (the
streaming remeasure), `FollowMode`, and window-level selection are upstream and
are used as published.
