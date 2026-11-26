//! Conversation scroller — an original, plain-GPUI implementation built on
//! [`gpui::ListState`] (no component library).
//!
//! Tail following matches the agent transcript. While the reader is within
//! [`TAIL_SLACK`] of the live edge, new output keeps that edge against the
//! viewport. A user scroll — wheel, scrollbar, or the residual from a nested
//! card — that leaves the band detaches immediately. Further output does not
//! move the reader and does not turn following back on, including when the
//! new text leaves them inside the band. Following resumes when they scroll
//! back into the band, jump to the latest row, or the transcript resets. Layout
//! also resumes it when a shrink no longer fills the viewport and the list
//! drops its anchor; a gap that merely falls inside the band does not.
//!
//! GPUI 0.2.2 has no `FollowMode::Tail`; [`ListAlignment::Bottom`] plus a
//! past-the-end `scroll_to` is the equivalent: the last row's bottom stays
//! against the viewport while it grows.

use std::{cell::Cell, ops::Range, rc::Rc};

use gpui::{
    canvas, div, fill, point, prelude::*, px, size, svg, App, Background, Bounds, DispatchPhase,
    ElementId, Hsla, ListAlignment, ListOffset, ListScrollEvent, ListState, MouseButton,
    MouseMoveEvent, MouseUpEvent, Pixels, Window,
};

use crate::app::{button_frame, icon_button_frame, TipExt, BUTTON_GROUP};
use crate::theme::tokens::{scrollbar, ButtonSize, StyledExt};
use crate::theme::Theme;

/// Painted thumb width. The hit strip is wider so a drag can start on its edge.
const SCROLLBAR_THUMB_WIDTH: f32 = 4.;
const SCROLLBAR_HIT_WIDTH: f32 = 10.;
/// Positions within this many pixels of an edge count as already there.
const SCROLLBAR_EDGE: f32 = 0.5;
/// Extra pixels above and below the thumb that still start a drag.
const SCROLLBAR_THUMB_SLOP: f32 = 4.;
/// The reader is on the live edge when the tail is inside this band. A user
/// scroll that stays inside keeps following; one that leaves detaches. Coming
/// back inside locks on again. Streaming growth never crosses this line by
/// itself. Same width Cursor uses for the composer (`itg`, 50px).
const TAIL_SLACK: f32 = 50.;
/// Leftmost ink of `icons/arrow-down.svg` in its 24px viewBox. The arrowhead
/// starts at x=6, and the round cap adds half of the 1.5px stroke.
const ARROW_INK_LEFT: f32 = 5.25;
const ARROW_VIEWBOX: f32 = 24.;

const LIST_OVERDRAW: f32 = 400.0;

/// Entity-owned scrolling state for the transcript list.
#[derive(Clone)]
pub struct MessageScrollerState {
    list: ListState,
    following_tail: Rc<Cell<bool>>,
    /// First row currently visible in the viewport — the "reader is here"
    /// hint that drives the navigation rail's active tick.
    visible_start: Rc<Cell<usize>>,
    /// New content arrived while the reader was away from the live edge —
    /// the jump-to-latest control says so until they return.
    unread: Rc<Cell<bool>>,
    /// The transcript has overflowed. A streaming remeasure keeps the previous
    /// height, but a layout that has not run yet can still report no overflow;
    /// the jump control stays up through that blip instead of flickering off.
    overflowing: Rc<Cell<bool>>,
    /// Live thumb drag. Window-level move and release listeners read it, so a
    /// drag that leaves the strip — or a frame where the strip is gone — still
    /// ends and unfreezes the list's scrollbar height.
    thumb_drag: Rc<Cell<Option<ScrollbarDrag>>>,
}

/// Pointer and scroll offset captured when a thumb drag starts.
#[derive(Clone, Copy)]
struct ScrollbarDrag {
    start_y: f32,
    /// Pixels down from the top, in `0..=max`.
    start_scrolled: f32,
}

impl MessageScrollerState {
    /// Create state for `item_count` rows and enable tail following.
    pub fn new(item_count: usize) -> Self {
        let list = ListState::new(item_count, ListAlignment::Bottom, px(LIST_OVERDRAW));
        let following_tail = Rc::new(Cell::new(true));
        let visible_start = Rc::new(Cell::new(0));
        let unread = Rc::new(Cell::new(false));
        let overflowing = Rc::new(Cell::new(false));
        {
            let following_tail = following_tail.clone();
            let visible_start = visible_start.clone();
            let unread = unread.clone();
            let overflowing = overflowing.clone();
            list.set_scroll_handler(move |event: &ListScrollEvent, _, cx| {
                visible_start.set(event.visible_range.start);
                // The list is already borrowed for this gesture, so the gap
                // comes from the event instead of a second read. Scrollbar
                // drags and chained deltas update the pin themselves. Content
                // growth does not come through here.
                note_tail_distance(
                    f32::from(event.offset_from_bottom),
                    &following_tail,
                    &unread,
                    &overflowing,
                );
                cx.refresh_windows();
            });
        }
        Self {
            list,
            following_tail,
            visible_start,
            unread,
            overflowing,
            thumb_drag: Rc::new(Cell::new(None)),
        }
    }

    /// Record that content changed. When the reader is away from the live
    /// edge this flags the jump-to-latest control; while following, it's a
    /// no-op (the content is already in view).
    pub fn note_activity(&self) {
        if !self.is_following_tail() {
            self.unread.set(true);
        }
    }

    /// True when content changed while the reader was scrolled up.
    pub fn has_unread(&self) -> bool {
        self.unread.get()
    }

    /// First row currently in view (viewport-top hint for the rail).
    ///
    /// Kept current by the wheel handler and by every programmatic scroll
    /// ([`Self::scroll_to_item`], [`Self::scroll_by`], [`Self::scroll_to_end`],
    /// [`Self::reset`]); gpui only fires the scroll handler for wheel and
    /// scrollbar gestures, so those paths must refresh it by hand or the rail
    /// keeps highlighting the turn the reader left behind.
    pub fn first_visible_index(&self) -> usize {
        self.visible_start.get()
    }

    /// Point the reader hint at `ix` without waiting for the list's scroll
    /// handler — programmatic scrolls never invoke it.
    fn note_visible_start(&self, ix: usize) {
        self.visible_start
            .set(ix.min(self.item_count().saturating_sub(1)));
    }

    pub fn list_state(&self) -> ListState {
        self.list.clone()
    }

    pub fn item_count(&self) -> usize {
        self.list.item_count()
    }

    /// True when the reader has left the live edge and there is room to scroll.
    pub fn is_scrolled_up(&self) -> bool {
        if self.is_following_tail() {
            return false;
        }
        let overflow = self.list.max_offset_for_scrollbar().height > px(0.);
        if overflow {
            self.overflowing.set(true);
            return true;
        }
        // Keep the control up when the list reports no overflow yet. A layout
        // that has not measured the live row, or a one-frame gap after a
        // splice, would otherwise flicker the jump control off.
        self.overflowing.get()
    }

    /// Resume following when layout has pinned the reader to the live edge.
    ///
    /// Bottom alignment clears the anchor only when the content below it no
    /// longer fills the viewport. A shorter extent, or a gap that merely fell
    /// inside the slack, leaves the anchor on a real row. Turning following
    /// back on there would scroll the next chunk to the end.
    fn sync_tail_follow(&self) {
        if self.following_tail.get() {
            return;
        }
        // `None` (and a past-the-end `scroll_to`) both read as `item_ix ==
        // count`. A real row means the reader was not clamped.
        if self.list.logical_scroll_top().item_ix < self.item_count() {
            return;
        }
        self.following_tail.set(true);
        self.unread.set(false);
        self.overflowing.set(false);
        self.note_visible_start(self.item_count());
    }

    /// True when the transcript holds more content than the viewport shows
    /// (drives the navigation-rail visibility).
    pub fn is_scrollable(&self) -> bool {
        self.list.max_offset_for_scrollbar().height > px(0.)
    }

    pub fn is_following_tail(&self) -> bool {
        self.following_tail.get()
    }

    /// Reset to `item_count` rows and resume tail following.
    pub fn reset(&self, item_count: usize) {
        // A drag freezes the height the scrollbar reports. Drop it before the
        // list is rebuilt, or the next session keeps the previous one's max.
        self.end_scrollbar_drag();
        self.unread.set(false);
        self.list.reset(item_count);
        self.scroll_to_end();
    }

    /// Replace `old_range` with `count` new rows.
    ///
    /// Returns `false` when the range is outside the current list.
    pub fn splice(&self, old_range: Range<usize>, count: usize) -> bool {
        if !self.valid_range(&old_range) {
            return false;
        }
        self.list.splice(old_range, count);
        self.stick_if_following();
        true
    }

    /// Append `count` rows to the end of the list.
    pub fn append(&self, count: usize) -> bool {
        let item_count = self.item_count();
        self.splice(item_count..item_count, count)
    }

    /// Prepend `count` rows while preserving the current scroll anchor.
    #[allow(dead_code)]
    pub fn prepend(&self, count: usize) -> bool {
        self.splice(0..0, count)
    }

    /// Mark rows in `range` for remeasurement (streaming growth).
    ///
    /// The last measured height stays in the scroll extent until layout, so a
    /// wheel event in that window still has somewhere to go. The reader's
    /// offset inside the row is left alone. While following, the tail is
    /// pinned after the invalidation.
    pub fn remeasure_items(&self, range: Range<usize>) -> bool {
        if !self.valid_range(&range) || range.start == range.end {
            return false;
        }
        self.list.invalidate(range);
        self.stick_if_following();
        true
    }

    /// Remeasure one row whose height changed from an expand/collapse, keeping
    /// the transcript where the reader left it.
    ///
    /// While tail-following, a plain remeasure re-pins the list to the bottom
    /// and yanks the growing row out of view. Leave tail-following first. The
    /// past-the-end anchor has no row to freeze, so step back one viewport;
    /// a reader already inside the slack already has a real row, and moving
    /// them would jump a full screen.
    ///
    /// The row is invalidated in place: its last height and the reader's
    /// offset stay until layout measures the new one, so opening a card in
    /// the message on screen does not snap that row to the viewport top.
    pub fn remeasure_toggle(&self, ix: usize) -> bool {
        if ix >= self.item_count() {
            return false;
        }
        // Only freeze an anchor when there is content to scroll. A transcript
        // shorter than the viewport is bottom-pinned; materializing an anchor
        // there would snap the whole run to the top.
        if self.is_following_tail() && self.is_scrollable() {
            // The past-the-end anchor has no row to freeze. Step back one
            // viewport so the row at the top of the screen becomes it. A
            // reader already inside the slack has a real row; moving them
            // would jump a full screen.
            if self.list.logical_scroll_top().item_ix >= self.item_count() {
                let viewport = self.list.viewport_bounds().size.height;
                self.list.scroll_by(-viewport);
            }
            let anchor = self.list.logical_scroll_top();
            if anchor.item_ix < self.item_count() {
                self.following_tail.set(false);
                // Programmatic scrolls skip the scroll handler; keep the
                // rail's "reader is here" hint on the frozen top row.
                self.visible_start.set(anchor.item_ix);
            }
        }
        self.list.invalidate(ix..ix + 1);
        // A run that still fits, or a freeze that did not land on a row,
        // stays pinned. Detached readers are left on the anchor they have.
        self.stick_if_following();
        true
    }

    /// Scroll to the row at `index`, if it exists. Leaves tail following.
    pub fn scroll_to_item(&self, index: usize) -> bool {
        if index >= self.item_count() {
            return false;
        }
        self.following_tail.set(false);
        self.list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        // A programmatic scroll skips the list's handler; without this the
        // rail (and repeated ⌘↑/⌘↓ jumps) would read a stale viewport top.
        self.note_visible_start(index);
        true
    }

    /// Resume tail following and scroll to the latest row.
    pub fn scroll_to_end(&self) {
        self.unread.set(false);
        self.overflowing.set(false);
        self.following_tail.set(true);
        self.list.scroll_to(ListOffset {
            item_ix: self.item_count(),
            offset_in_item: px(0.),
        });
        // The live edge is the newest content; a follow-up jump uses the
        // reader hint, so keep it pointing at the tail rather than a stale row.
        self.note_visible_start(self.item_count());
    }

    /// Scroll the transcript by `distance` pixels — positive moves toward the
    /// live edge, negative back through history. Used to chain a wheel event
    /// from a nested scroll area (the "Thought" card) once it bottoms out.
    pub fn scroll_by(&self, distance: Pixels) {
        if distance == px(0.) {
            return;
        }
        // At the tail GPUI's logical offset is past-the-end, a full viewport
        // below the actual scroll top. Clamp that sentinel to the real pixel
        // range before applying a chained wheel delta; otherwise small upward
        // gestures over a Thought card get clamped straight back to the tail.
        let max = self.list.max_offset_for_scrollbar().height;
        // Nothing to scroll. An upward flick must not snap a reader who just
        // left the tail back onto it — `target == max` below means "at the
        // live edge", and a zero extent is that edge for both directions.
        if distance < px(0.) && max <= px(SCROLLBAR_EDGE) {
            return;
        }
        let current = (-self.list.scroll_px_offset_for_scrollbar().y).clamp(px(0.), max);
        let target = (current + distance).clamp(px(0.), max);
        self.apply_scrollbar_offset(target);
    }

    /// Move the list to `target` pixels down from the top. Within
    /// [`TAIL_SLACK`] of the live edge this keeps following; further up it
    /// releases. Programmatic offsets do not fire the list's scroll handler,
    /// so the pin and the rail hint are written here.
    fn apply_scrollbar_offset(&self, target: Pixels) {
        let max = self.list.max_offset_for_scrollbar().height;
        let target = target.clamp(px(0.), max);
        if max <= px(SCROLLBAR_EDGE) || target >= max - px(SCROLLBAR_EDGE) {
            self.scroll_to_end();
        } else {
            self.list.set_offset_from_scrollbar(point(px(0.), -target));
            self.note_visible_start(self.list.logical_scroll_top().item_ix);
            // The gesture's own gap, not a second read of the list. A remeasure
            // that has not been laid out yet must not be what decides the pin.
            note_tail_distance(
                f32::from(max - target),
                &self.following_tail,
                &self.unread,
                &self.overflowing,
            );
        }
    }

    /// Pixels currently scrolled down from the top, clamped into the range the
    /// scrollbar reports. At the tail the logical offset is a past-the-end
    /// sentinel; without the clamp the thumb would paint below the track.
    fn scrolled_px(&self) -> (f32, f32) {
        let max = f32::from(self.list.max_offset_for_scrollbar().height);
        let scrolled = (-f32::from(self.list.scroll_px_offset_for_scrollbar().y)).clamp(0., max);
        (scrolled, max)
    }

    fn pointer_on_thumb(&self, x: f32, y: f32) -> bool {
        let viewport = self.list.viewport_bounds();
        let right = f32::from(viewport.right());
        if x < right - SCROLLBAR_HIT_WIDTH || x > right + 1. {
            return false;
        }
        let (scrolled, max) = self.scrolled_px();
        let Some((top, height)) = scrollbar_thumb(f32::from(viewport.size.height), max, scrolled)
        else {
            return false;
        };
        let local = y - f32::from(viewport.origin.y);
        local >= top - SCROLLBAR_THUMB_SLOP && local <= top + height + SCROLLBAR_THUMB_SLOP
    }

    fn begin_scrollbar_drag(&self, x: f32, y: f32) -> bool {
        if self.thumb_drag.get().is_some() || !self.pointer_on_thumb(x, y) {
            return false;
        }
        // Freeze the measured height for the rest of the gesture. Newly
        // measured rows would otherwise change the thumb's travel under the
        // pointer.
        self.list.scrollbar_drag_started();
        let (scrolled, _) = self.scrolled_px();
        self.thumb_drag.set(Some(ScrollbarDrag {
            start_y: y,
            start_scrolled: scrolled,
        }));
        true
    }

    fn update_scrollbar_drag(&self, y: f32) {
        let Some(drag) = self.thumb_drag.get() else {
            return;
        };
        let viewport = self.list.viewport_bounds();
        let max = f32::from(self.list.max_offset_for_scrollbar().height);
        let Some(next) = scrollbar_dragged(
            drag.start_scrolled,
            y - drag.start_y,
            f32::from(viewport.size.height),
            max,
        ) else {
            self.end_scrollbar_drag();
            return;
        };
        self.apply_scrollbar_offset(px(next));
    }

    fn end_scrollbar_drag(&self) {
        if self.thumb_drag.take().is_some() {
            self.list.scrollbar_drag_ended();
        }
    }

    fn scrollbar_thumb_bounds(&self) -> Option<Bounds<Pixels>> {
        let viewport = self.list.viewport_bounds();
        let (scrolled, max) = self.scrolled_px();
        let (top, height) = scrollbar_thumb(f32::from(viewport.size.height), max, scrolled)?;
        let width = px(SCROLLBAR_THUMB_WIDTH);
        Some(Bounds::new(
            point(
                viewport.right() - width - px(2.),
                viewport.origin.y + px(top),
            ),
            size(width, px(height)),
        ))
    }

    fn stick_if_following(&self) {
        if self.is_following_tail() {
            self.scroll_to_end();
        }
    }

    fn valid_range(&self, range: &Range<usize>) -> bool {
        range.start <= range.end && range.end <= self.item_count()
    }
}

fn within_tail_slack(distance: f32) -> bool {
    distance <= TAIL_SLACK
}

/// Lock following when `distance` is inside the slack, and release it past that.
/// Returning to the slack clears the unread mark: the reader has caught up.
fn note_tail_distance(
    distance: f32,
    following_tail: &Cell<bool>,
    unread: &Cell<bool>,
    overflowing: &Cell<bool>,
) {
    if within_tail_slack(distance) {
        following_tail.set(true);
        unread.set(false);
        overflowing.set(false);
    } else {
        following_tail.set(false);
    }
}

/// Right inset of the "New" label. The arrow's ink sits `ARROW_INK_LEFT`
/// into the icon box, so the gap from the pill's left edge to the stroke is
/// the frame padding plus that inset. The label has no matching inset; this
/// adds it on the right.
fn new_label_padding_right(theme: &Theme) -> Pixels {
    let pad = f32::from(ButtonSize::Large.padding_x(theme));
    let icon = f32::from(ButtonSize::Large.icon_size().px(theme));
    px(pad + icon * (ARROW_INK_LEFT / ARROW_VIEWBOX))
}

/// Viewport + jump-to-latest chrome around a virtualized `list()`.
pub fn render_scroller(
    state: MessageScrollerState,
    theme: Theme,
    list: impl IntoElement,
) -> impl IntoElement {
    // Checked after the list lays out. A splice that swaps a measured row
    // for an unmeasured one drops the extent until that layout; reading it
    // here would treat the gap as a clamp and pin the reader again.
    let after_layout = state.clone();
    let scrolled_up = state.is_scrolled_up();
    div()
        .id(ElementId::Name("message-scroller".into()))
        .relative()
        .w_full()
        .min_w_0()
        .h_full()
        .min_h_0()
        .overflow_hidden()
        .child(
            div()
                .id(ElementId::Name("message-scroller-viewport".into()))
                .w_full()
                .min_w_0()
                .h_full()
                .min_h_0()
                .child(list),
        )
        .child(
            // Runs after the list's prepaint, so the height just measured is
            // visible. Resuming follow has to wait for the next frame: this
            // canvas runs while the window is already drawing, and refresh
            // is ignored until that finishes. The jump control was built for
            // this frame and leaves on the one just scheduled. No hitbox.
            canvas(
                move |_, window, _| {
                    let was_following = after_layout.is_following_tail();
                    after_layout.sync_tail_follow();
                    if !was_following && after_layout.is_following_tail() {
                        window.on_next_frame(|window, _| window.refresh());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size(px(0.)),
        )
        .when(scrolled_up, |shell| {
            shell.child(render_jump_button(state, theme))
        })
}

fn render_jump_button(state: MessageScrollerState, theme: Theme) -> impl IntoElement {
    // Content arrived while the reader was scrolled up: the round jump
    // affordance grows a "New" label so the stream's continued progress is
    // visible from anywhere in the transcript.
    let unread = state.has_unread();
    let button = div()
        .id(ElementId::Name("message-scroller-jump".into()))
        .group(BUTTON_GROUP);
    let button = if unread {
        button_frame(button, &theme, ButtonSize::Large).pr(new_label_padding_right(&theme))
    } else {
        icon_button_frame(button, &theme, ButtonSize::Large)
    };
    div()
        .id(ElementId::Name("message-scroller-jump-layer".into()))
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(8.))
        .w_full()
        .flex()
        .justify_center()
        .child(
            // Floating round scroll-to-bottom affordance: the elevated
            // surface, kept round and on the composer's fill.
            button
                .elevation_2(&theme)
                .rounded_full()
                .tip(tr!("message_scroller.jump_to_latest"))
                .debug_selector(|| "message-scroller-jump".to_string())
                .bg(theme.surface_secondary)
                .cursor_pointer()
                .hover(|style| style.bg(theme.surface_elevated))
                .on_click(move |_, _, cx| {
                    state.scroll_to_end();
                    cx.refresh_windows();
                })
                .child(
                    svg()
                        .path("icons/arrow-down.svg")
                        .flex_none()
                        .size(ButtonSize::Large.icon_size().px(&theme))
                        .text_color(theme.text_primary),
                )
                .when(unread, |button| {
                    button.child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(tr!("message_scroller.new_activity")),
                    )
                }),
        )
}

/// Overlay scrollbar for the transcript. The canvas is always present so a
/// drag's move and release are still delivered after the hit strip goes away.
/// The strip itself sits above the text-selection layer and blocks presses
/// without swallowing the list's wheel events.
pub fn render_scrollbar(state: MessageScrollerState, theme: Theme) -> impl IntoElement {
    let scrollable = state.is_scrollable();
    let color = theme.text_muted.opacity(0.45);
    let hit = state.clone();
    div()
        .absolute()
        .inset_0()
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| paint_scrollbar(&state, color, window),
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
        .when(scrollable, |layer| {
            layer.child(
                div()
                    .id("transcript-scrollbar")
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(SCROLLBAR_HIT_WIDTH))
                    .block_mouse_except_scroll()
                    .on_mouse_down(MouseButton::Left, move |event, window, _| {
                        if hit.begin_scrollbar_drag(
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                        ) {
                            window.refresh();
                        }
                    }),
            )
        })
}

fn paint_scrollbar(state: &MessageScrollerState, color: Hsla, window: &mut Window) {
    register_scrollbar_drag(state.clone(), window);
    let Some(thumb) = state.scrollbar_thumb_bounds() else {
        return;
    };
    window.paint_quad(fill(thumb, Background::from(color)));
}

/// Move and release are window-level. The press that starts a drag is on the
/// hit strip, which is the only place a press must not begin a text selection.
fn register_scrollbar_drag(state: MessageScrollerState, window: &mut Window) {
    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseMoveEvent, phase, window, _: &mut App| {
            if phase != DispatchPhase::Capture || state.thumb_drag.get().is_none() {
                return;
            }
            if event.pressed_button != Some(MouseButton::Left) {
                state.end_scrollbar_drag();
                window.refresh();
                return;
            }
            state.update_scrollbar_drag(f32::from(event.position.y));
            window.refresh();
        }
    });
    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseUpEvent, phase, window, _: &mut App| {
            if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                return;
            }
            if state.thumb_drag.get().is_some() {
                state.end_scrollbar_drag();
                window.refresh();
            }
        }
    });
}

/// Thumb `(top, height)` inside a viewport of `viewport` px. `scrolled` is
/// pixels down from the top. `None` when the content fits.
fn scrollbar_thumb(viewport: f32, max_scroll: f32, scrolled: f32) -> Option<(f32, f32)> {
    if max_scroll <= SCROLLBAR_EDGE || viewport <= 1. {
        return None;
    }
    let min_thumb = f32::from(scrollbar::MIN_THUMB_LENGTH).min(viewport);
    let content = viewport + max_scroll;
    let height = ((viewport * viewport) / content).clamp(min_thumb, viewport);
    let travel = (viewport - height).max(0.);
    let scrolled = scrolled.clamp(0., max_scroll);
    let top = scrolled / max_scroll * travel;
    Some((top, height))
}

/// Scroll offset after dragging the thumb by `dy` pixels. `None` when there
/// is no thumb. A thumb that already fills the track does not move.
fn scrollbar_dragged(start_scrolled: f32, dy: f32, viewport: f32, max_scroll: f32) -> Option<f32> {
    let (_, height) = scrollbar_thumb(viewport, max_scroll, start_scrolled)?;
    let travel = viewport - height;
    if travel <= SCROLLBAR_EDGE {
        return Some(start_scrolled.clamp(0., max_scroll));
    }
    Some((start_scrolled + dy / travel * max_scroll).clamp(0., max_scroll))
}

#[cfg(test)]
mod tests {
    use super::*;

    type RowHeights = Rc<std::cell::RefCell<std::collections::HashMap<usize, f32>>>;

    /// A bottom-aligned transcript standing in for real message rows: each row
    /// is 40 px unless `heights` gives it a taller one, which is how an opened
    /// thinking/tool card changes a row's size.
    struct TranscriptProbe {
        state: MessageScrollerState,
        heights: RowHeights,
    }

    impl gpui::Render for TranscriptProbe {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let heights = self.heights.clone();
            let list = gpui::list(self.state.list_state(), move |ix, _, _| {
                let height = heights.borrow().get(&ix).copied().unwrap_or(40.);
                gpui::div()
                    .id(gpui::ElementId::NamedInteger("probe-row".into(), ix as u64))
                    .debug_selector(move || format!("probe-row-{ix}"))
                    .w_full()
                    .h(px(height))
                    .child(format!("row {ix}"))
                    .into_any_element()
            })
            .size_full();
            // The production shell is what re-reads the tail after layout.
            // Drawing the list alone never hit that path.
            super::render_scroller(self.state.clone(), crate::theme::Theme::dark(), list)
        }
    }

    /// Open a probe at a 600×400 window and return its first painted frame.
    fn draw_probe(
        cx: &mut gpui::VisualTestContext,
        probe: &gpui::Entity<TranscriptProbe>,
    ) -> gpui::Size<gpui::Pixels> {
        let space = gpui::size(px(600.), px(400.));
        let origin = gpui::point(px(0.), px(0.));
        let _ = cx.draw(origin, space, |_, _| {
            gpui::div().size_full().child(probe.clone())
        });
        space
    }

    /// Expanding a card while tail-following must not re-pin the list to the
    /// bottom: the row under the reader stays put and the card grows below it.
    #[gpui::test]
    fn remeasure_toggle_keeps_the_reader_anchored(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(40);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        let before = cx.debug_bounds("probe-row-30").expect("row 30 visible");
        assert!(state.is_following_tail(), "starts at the live edge");

        heights.borrow_mut().insert(38, 500.);
        state.remeasure_toggle(38);
        draw_probe(cx, &probe);

        assert!(
            !state.is_following_tail(),
            "toggling leaves tail-following so the view holds still"
        );
        let after = cx
            .debug_bounds("probe-row-30")
            .expect("row 30 still visible");
        assert_eq!(
            after.top(),
            before.top(),
            "the row under the reader did not move"
        );
    }

    /// The failing shape: the changed row is itself the row pinned to the top
    /// of the viewport. gpui's splice zeroes that row's scroll offset, so the
    /// anchor has to be re-applied or the row's top snaps to the viewport top.
    #[gpui::test]
    fn remeasure_toggle_holds_a_pinned_row_that_is_itself_toggled(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(40);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        // A tall row 29 sits under the viewport top (offset 420), with rows
        // 30..39 (30 px each) beneath it so the anchor holds while it grows.
        heights.borrow_mut().insert(29, 520.);
        for ix in 30..40 {
            heights.borrow_mut().insert(ix, 30.);
        }
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        let before = cx
            .debug_bounds("probe-row-29")
            .expect("row 29 is the pinned row");
        assert!(before.top() < px(0.), "row 29 is scrolled under the top");

        heights.borrow_mut().insert(29, 700.);
        state.remeasure_toggle(29);
        draw_probe(cx, &probe);

        let after = cx.debug_bounds("probe-row-29").expect("row 29 visible");
        assert_eq!(
            after.top(),
            before.top(),
            "toggling the pinned row keeps its offset instead of snapping to the top"
        );
    }

    /// A run shorter than the viewport stays bottom-pinned through a toggle —
    /// freezing an anchor would snap it to the top of the panel.
    #[gpui::test]
    fn remeasure_toggle_keeps_a_short_run_bottom_pinned(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(5);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        let before = cx.debug_bounds("probe-row-4").expect("row 4 visible");

        heights.borrow_mut().insert(4, 200.);
        state.remeasure_toggle(4);
        draw_probe(cx, &probe);

        assert!(
            state.is_following_tail(),
            "a run that still fits stays at the live edge"
        );
        let after = cx.debug_bounds("probe-row-4").expect("row 4 visible");
        assert_eq!(
            after.bottom(),
            before.bottom(),
            "the run stays pinned to the bottom of the panel"
        );
    }

    #[gpui::test]
    fn streaming_preserves_the_readers_offset_inside_the_live_row(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(3);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(2, 1200.);
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(300.), px(200.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(150.))),
            ..Default::default()
        });
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        let before = state.list_state().logical_scroll_top();
        assert_eq!(before.item_ix, 2);
        assert!(before.offset_in_item > px(0.));
        let top = cx.debug_bounds("probe-row-2").unwrap().top();

        for height in [1300., 1450., 1600.] {
            heights.borrow_mut().insert(2, height);
            assert!(state.remeasure_items(2..3));
            state.note_activity();
            draw_probe(cx, &probe);
            assert_eq!(
                state.list_state().logical_scroll_top().offset_in_item,
                before.offset_in_item
            );
            assert_eq!(cx.debug_bounds("probe-row-2").unwrap().top(), top);
            assert!(!state.is_following_tail());
            assert!(state.has_unread());
        }

        state.scroll_to_end();
        heights.borrow_mut().insert(2, 1800.);
        state.remeasure_items(2..3);
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());
        assert!(!state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-2").unwrap().bottom(), px(400.));
    }

    #[gpui::test]
    fn collapsing_a_row_while_scrolled_up_keeps_the_anchor(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(3);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(2, 1600.);
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        // Far enough from the tail that a 200px shrink still leaves a full
        // viewport of this row below the anchor. A bottom-aligned list pulls
        // back to the tail only when the content under the anchor no longer
        // fills the viewport; this case must not.
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(300.), px(200.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(600.))),
            ..Default::default()
        });
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        let before = state.list_state().logical_scroll_top();
        assert!(before.offset_in_item > px(0.));
        let top = cx.debug_bounds("probe-row-2").unwrap().top();

        heights.borrow_mut().insert(2, 1400.);
        assert!(state.remeasure_items(2..3));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        assert_eq!(
            state.list_state().logical_scroll_top().offset_in_item,
            before.offset_in_item
        );
        assert_eq!(cx.debug_bounds("probe-row-2").unwrap().top(), top);
        assert!(cx.debug_bounds("probe-row-2").unwrap().bottom() > px(400.));
    }

    #[gpui::test]
    fn streaming_and_appending_below_history_keep_the_visible_row_fixed(
        cx: &mut gpui::TestAppContext,
    ) {
        let state = MessageScrollerState::new(40);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        state.scroll_to_item(20);
        draw_probe(cx, &probe);
        let before = cx.debug_bounds("probe-row-20").unwrap();

        heights.borrow_mut().insert(39, 1200.);
        state.remeasure_items(39..40);
        state.append(1);
        state.note_activity();
        draw_probe(cx, &probe);

        assert_eq!(cx.debug_bounds("probe-row-20").unwrap().top(), before.top());
        assert!(!state.is_following_tail());
        assert!(state.has_unread());
    }

    #[gpui::test]
    fn chained_scroll_leaves_the_tail_and_resumes_following_at_the_bottom(
        cx: &mut gpui::TestAppContext,
    ) {
        let state = MessageScrollerState::new(3);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(2, 1200.);
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        let bottom = cx.debug_bounds("probe-row-2").unwrap();

        // A small residual from a nested Thought card must move immediately,
        // not require a single gesture larger than the viewport. Twenty pixels
        // is still inside the tail slack, so the pin stays and new output is
        // not marked unread.
        state.scroll_by(px(-20.));
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());
        assert_eq!(
            cx.debug_bounds("probe-row-2").unwrap().top(),
            bottom.top() + px(20.)
        );
        assert_eq!(state.first_visible_index(), 2);
        state.note_activity();
        assert!(!state.has_unread());

        // Past the slack the reader leaves, and output that arrives there is
        // unread until they come back inside the band.
        state.scroll_by(px(-40.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        assert_eq!(
            cx.debug_bounds("probe-row-2").unwrap().top(),
            bottom.top() + px(60.)
        );
        state.note_activity();
        assert!(state.has_unread());

        state.scroll_by(px(20.));
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());
        assert!(!state.has_unread());
        assert_eq!(
            cx.debug_bounds("probe-row-2").unwrap().top(),
            bottom.top() + px(40.)
        );
    }

    /// Streaming while the reader is only a few pixels off the tail keeps the
    /// pin and pulls the viewport back to the live edge, instead of flashing
    /// the jump control.
    #[gpui::test]
    fn streaming_inside_the_tail_slack_stays_pinned(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(3);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(2, 1200.);
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);

        state.scroll_by(px(-20.));
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());

        heights.borrow_mut().insert(2, 1400.);
        assert!(state.remeasure_items(2..3));
        state.note_activity();
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());
        assert!(!state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-2").unwrap().bottom(), px(400.));
    }

    /// History taller than the viewport, with every row measured before the
    /// caller scrolls.
    ///
    /// A single bottom-aligned paint only walks the viewport plus
    /// [`LIST_OVERDRAW`]. A live row taller than that leaves the rows above
    /// it at height 0, so `scroll_by` clamps and assertions about rows above
    /// the fold read a list that was never laid out.
    fn tall_probe(
        cx: &mut gpui::VisualTestContext,
        rows: usize,
        live: usize,
        live_height: f32,
    ) -> (MessageScrollerState, RowHeights, gpui::Entity<TranscriptProbe>) {
        let state = MessageScrollerState::new(rows);
        // First layout measures every row, then later frames stay on the
        // visible walk. Dropping the returned handle keeps the flag on the
        // shared list.
        drop(state.list_state().measure_all());
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(live, live_height);
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        let content = (rows - 1) as f32 * 40. + live_height;
        let max = f32::from(state.list_state().max_offset_for_scrollbar().height);
        let expected = (content - 400.).max(0.);
        assert!(
            (max - expected).abs() < 1.,
            "rows above the fold must be measured; max {max}, expected {expected}"
        );
        (state, heights, probe)
    }

    #[gpui::test]
    fn streaming_growth_keeps_a_reader_inside_the_live_row(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        // Twelve measured rows of 40px already overflow a 400px viewport.
        // The live row is the one the reader is inside.
        let (state, heights, probe) = tall_probe(cx, 13, 12, 900.);
        assert!(state.is_following_tail());

        state.scroll_by(px(-80.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        let top = cx.debug_bounds("probe-row-12").unwrap().top();

        for height in [1100., 1400., 1800.] {
            let max_before = state.list_state().max_offset_for_scrollbar().height;
            heights.borrow_mut().insert(12, height);
            assert!(state.remeasure_items(12..13));
            assert_eq!(
                state.list_state().max_offset_for_scrollbar().height,
                max_before,
                "invalidate keeps the last measured height until layout"
            );
            state.note_activity();
            draw_probe(cx, &probe);
            assert!(!state.is_following_tail());
            assert!(state.has_unread());
            assert_eq!(cx.debug_bounds("probe-row-12").unwrap().top(), top);
        }

        state.scroll_to_end();
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());
        assert!(!state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-12").unwrap().bottom(), px(400.));
    }

    #[gpui::test]
    fn streaming_growth_just_below_the_fold_does_not_relock(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        // Twenty rows of 40px, then a 500px live row. Scrolling up by the
        // live row plus 30px parks its top 30px under the viewport bottom —
        // the spot where a height of 0 used to look like "within 50px".
        let (state, heights, probe) = tall_probe(cx, 21, 20, 500.);
        state.scroll_by(px(-(500. + 30.)));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        // The live row's top sits under the fold, so a painted history row is
        // the one whose position must survive the next chunk.
        assert!(cx.debug_bounds("probe-row-20").is_none());
        let top = cx.debug_bounds("probe-row-9").unwrap().top();

        let max_before = state.list_state().max_offset_for_scrollbar().height;
        heights.borrow_mut().insert(20, 1200.);
        assert!(state.remeasure_items(20..21));
        assert_eq!(
            state.list_state().max_offset_for_scrollbar().height,
            max_before
        );
        state.note_activity();
        draw_probe(cx, &probe);

        assert!(!state.is_following_tail());
        assert!(state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-9").unwrap().top(), top);
    }

    #[gpui::test]
    fn a_shrink_that_clamps_the_reader_into_the_band_follows_again(
        cx: &mut gpui::TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let (state, heights, probe) = tall_probe(cx, 13, 12, 900.);
        // Far enough that the viewport top is a history row. The shrink then
        // leaves less than a viewport below that anchor, which is the clamp.
        state.scroll_by(px(-600.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        assert!(state.first_visible_index() < 12);

        heights.borrow_mut().insert(12, 100.);
        assert!(state.remeasure_items(12..13));
        state.note_activity();
        assert!(state.has_unread());
        draw_probe(cx, &probe);

        assert!(state.is_following_tail());
        assert!(!state.has_unread());
        assert_eq!(state.first_visible_index(), 12);
        // This frame already built the jump control. Layout asked for another
        // frame to take it down; drawing again by hand would hide it even if
        // that request had been dropped.
        assert!(cx.debug_bounds("message-scroller-jump").is_some());
        let redraw_scheduled = cx.update(|window, cx| {
            let scheduled = window.flush_next_frame(cx);
            scheduled && window.is_dirty()
        });
        assert!(
            redraw_scheduled,
            "a clamp must schedule a redraw for the next frame"
        );
        draw_probe(cx, &probe);
        assert!(cx.debug_bounds("message-scroller-jump").is_none());
    }

    /// The live row is the only one measured, so a shrink that pulls the
    /// anchor upward also measures the history above it and the extent grows.
    /// Layout still drops the anchor. That is a clamp, and following resumes.
    #[gpui::test]
    fn a_shrink_clamps_even_when_newly_measured_history_grows_the_extent(
        cx: &mut gpui::TestAppContext,
    ) {
        let state = MessageScrollerState::new(13);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(12, 900.);
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);
        let max = f32::from(state.list_state().max_offset_for_scrollbar().height);
        assert!(
            (max - 500.).abs() < 1.,
            "rows above the live row stay unmeasured; max {max}"
        );

        state.scroll_by(px(-60.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        let max = f32::from(state.list_state().max_offset_for_scrollbar().height);
        assert!((max - 500.).abs() < 1., "the scroll must not measure history; max {max}");

        heights.borrow_mut().insert(12, 100.);
        assert!(state.remeasure_items(12..13));
        state.note_activity();
        assert!(state.has_unread());
        draw_probe(cx, &probe);

        assert!(state.is_following_tail());
        assert!(!state.has_unread());
    }

    #[gpui::test]
    fn a_shrink_that_leaves_the_reader_outside_the_band_stays_detached(
        cx: &mut gpui::TestAppContext,
    ) {
        let cx = cx.add_empty_window();
        let (state, heights, probe) = tall_probe(cx, 13, 12, 900.);
        state.scroll_by(px(-300.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        let top = cx.debug_bounds("probe-row-12").unwrap().top();

        heights.borrow_mut().insert(12, 820.);
        assert!(state.remeasure_items(12..13));
        state.note_activity();
        draw_probe(cx, &probe);

        assert!(!state.is_following_tail());
        assert!(state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-12").unwrap().top(), top);
    }

    /// Sixty-five pixels up, then the live row loses twenty. The reader is
    /// inside the slack afterwards and still has a real anchor, so the next
    /// chunk must not pin them.
    #[gpui::test]
    fn a_small_shrink_inside_the_band_stays_detached(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let (state, heights, probe) = tall_probe(cx, 13, 12, 900.);
        state.scroll_by(px(-65.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        let top = cx.debug_bounds("probe-row-12").unwrap().top();

        heights.borrow_mut().insert(12, 880.);
        assert!(state.remeasure_items(12..13));
        state.note_activity();
        draw_probe(cx, &probe);

        assert!(!state.is_following_tail());
        assert!(state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-12").unwrap().top(), top);
    }

    /// Replacing the tail row drops its height to 0 until the next layout.
    /// A reader seventy pixels up must stay detached through that frame.
    #[gpui::test]
    fn replacing_the_live_row_while_detached_does_not_relock(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let (state, _, probe) = tall_probe(cx, 13, 12, 48.);
        state.scroll_by(px(-70.));
        draw_probe(cx, &probe);
        assert!(!state.is_following_tail());
        state.note_activity();
        let top = cx.debug_bounds("probe-row-2").unwrap().top();

        assert!(state.splice(12..13, 1));
        state.note_activity();
        draw_probe(cx, &probe);

        assert!(!state.is_following_tail());
        assert!(state.has_unread());
        assert_eq!(cx.debug_bounds("probe-row-2").unwrap().top(), top);
    }

    /// Following is still on a few pixels off the tail, and the anchor is
    /// already a real row. Opening a card must not step back a viewport.
    #[gpui::test]
    fn remeasure_toggle_inside_the_slack_stays_put(cx: &mut gpui::TestAppContext) {
        let state = MessageScrollerState::new(3);
        let heights: RowHeights =
            Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
        heights.borrow_mut().insert(2, 1200.);
        let cx = cx.add_empty_window();
        let probe = cx.update(|_, cx| {
            cx.new(|_| TranscriptProbe {
                state: state.clone(),
                heights: heights.clone(),
            })
        });
        draw_probe(cx, &probe);

        state.scroll_by(px(-20.));
        draw_probe(cx, &probe);
        assert!(state.is_following_tail());
        let top = cx.debug_bounds("probe-row-2").unwrap().top();

        heights.borrow_mut().insert(2, 1400.);
        assert!(state.remeasure_toggle(2));
        draw_probe(cx, &probe);

        assert!(!state.is_following_tail());
        assert_eq!(cx.debug_bounds("probe-row-2").unwrap().top(), top);
    }

    #[test]
    fn programmatic_scroll_updates_the_viewport_hint() {
        let state = MessageScrollerState::new(40);
        assert_eq!(state.first_visible_index(), 0);
        // A rail tick / ⌘↑ jump moves the reader hint without a wheel event.
        assert!(state.scroll_to_item(12));
        assert!(!state.is_following_tail());
        assert_eq!(state.first_visible_index(), 12);
        // A second jump from the same state must advance the hint, so
        // repeated keyboard jumps keep walking instead of re-reading row 12.
        assert!(state.scroll_to_item(30));
        assert_eq!(state.first_visible_index(), 30);
        // Resuming the tail repoints the hint at the newest content.
        state.scroll_to_end();
        assert!(state.is_following_tail());
        assert_eq!(state.first_visible_index(), 39);
        // A refused scroll must not move the hint.
        assert!(!state.scroll_to_item(40));
        assert_eq!(state.first_visible_index(), 39);
    }

    #[test]
    fn tail_slack_keeps_a_few_pixels_and_releases_past_fifty() {
        assert!(super::within_tail_slack(0.));
        assert!(super::within_tail_slack(3.));
        assert!(super::within_tail_slack(super::TAIL_SLACK));
        assert!(!super::within_tail_slack(super::TAIL_SLACK + 1.));
    }

    #[test]
    fn new_label_padding_matches_the_arrow_ink() {
        let theme = crate::theme::Theme::dark();
        let pad = f32::from(ButtonSize::Large.padding_x(&theme));
        let icon = f32::from(ButtonSize::Large.icon_size().px(&theme));
        let right = f32::from(super::new_label_padding_right(&theme));
        let ink_gap = pad + icon * (super::ARROW_INK_LEFT / super::ARROW_VIEWBOX);
        assert!((right - ink_gap).abs() < 0.001);
        assert!((right - 15.5).abs() < 0.001);
    }

    #[test]
    fn state_starts_following_tail() {
        let state = MessageScrollerState::new(3);
        assert_eq!(state.item_count(), 3);
        assert!(state.is_following_tail());
        assert!(!state.is_scrolled_up());
    }

    #[test]
    fn append_prepend_splice_and_reset() {
        let state = MessageScrollerState::new(3);
        assert!(state.append(2));
        assert_eq!(state.item_count(), 5);
        assert!(state.prepend(1));
        assert_eq!(state.item_count(), 6);
        assert!(!state.splice(5..7, 0));
        assert!(state.remeasure_items(0..6));
        assert!(!state.remeasure_items(6..7));
        assert!(state.scroll_to_item(2));
        assert!(!state.is_following_tail());
        assert!(!state.scroll_to_item(6));
        state.scroll_to_end();
        assert!(state.is_following_tail());
        state.reset(2);
        assert_eq!(state.item_count(), 2);
        assert!(state.is_following_tail());
    }

    #[test]
    fn the_thumb_rests_at_the_top_and_ends_flush_with_the_bottom() {
        assert!(super::scrollbar_thumb(200., 0., 0.).is_none());
        let (top, height) = super::scrollbar_thumb(200., 100., 0.).unwrap();
        assert_eq!(top, 0.);
        assert!((height - (200. * 200. / 300.)).abs() < 0.01);
        let (top, height) = super::scrollbar_thumb(200., 100., 100.).unwrap();
        assert!((top + height - 200.).abs() < 0.01);
        // A long list still keeps a thumb the reader can grab.
        let (_, height) = super::scrollbar_thumb(200., 10_000., 0.).unwrap();
        assert!(
            (height - f32::from(crate::theme::tokens::scrollbar::MIN_THUMB_LENGTH)).abs() < 0.01
        );
    }

    #[test]
    fn dragging_the_thumb_the_length_of_its_travel_reaches_the_bottom() {
        let viewport = 200.;
        let max_scroll = 100.;
        let (_, height) = super::scrollbar_thumb(viewport, max_scroll, 0.).unwrap();
        let travel = viewport - height;
        let end = super::scrollbar_dragged(0., travel, viewport, max_scroll).unwrap();
        assert!((end - max_scroll).abs() < 0.01);
        let back = super::scrollbar_dragged(end, -travel, viewport, max_scroll).unwrap();
        assert!(back.abs() < 0.01);
    }
}
