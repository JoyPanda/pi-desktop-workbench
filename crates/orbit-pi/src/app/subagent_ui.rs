//! Subagent list above the composer, and the transcript dock that shares the
//! right column with Review and Project. One of those three is open at a time.
//!
//! The list is a fixed-height strip: each leaf keeps its first-seen place and
//! a one-line preview. The dock shows one leaf. Inspect replies land in that
//! leaf's cache; the dock scrolls only the leaf it is showing.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::helpers::{icon, icon_button_frame, TipExt, BUTTON_GROUP, PRESS_DIM};
use super::OrbitApp;
use crate::subagent_status::{ListRow, MessageKind};
use crate::theme::tokens::{panel, ButtonSize, DynamicSpacing, TextSize};
use crate::theme::{self, Theme};
use crate::transcript_view::{self, CopiedSections, ExpandedBlocks};
use gpui::{
    canvas, div, fill, point, prelude::*, px, size, AnyElement, Background, Bounds, ClickEvent,
    ClipboardItem, Context, CursorStyle, ElementId, FontWeight, IntoElement, Pixels, Render,
    ScrollHandle, ScrollWheelEvent, Window,
};
use orbit_rpc::CommandBody;

const LIST_MAX_H: f32 = 160.;
const ROW_H: f32 = 28.;
const GROUP_H: f32 = 22.;
/// Estimated message stride used to turn a scroll offset back into "how many
/// messages sit below the anchor". Inspect replaces the window, so the anchor
/// is a count, not a pixel locked to text that is about to be swapped out.
const MESSAGE_ROW: f32 = 22.;

pub(super) const SUBAGENT_MIN_W: f32 = 300.;
pub(super) const SUBAGENT_MAX_W: f32 = 900.;

/// Drag marker for the subagent dock's left edge.
pub(super) struct SubagentResize;

/// Painted fold keys for one leaf. Content hashes live on the board.
pub(super) struct SubagentFoldUi {
    expanded: ExpandedBlocks,
    copied: CopiedSections,
    layout: Vec<(usize, usize, u64)>,
    /// Keys written on the last paint. A click is the difference from this.
    seeded: HashSet<(usize, u64, usize)>,
}

impl SubagentFoldUi {
    fn new() -> Self {
        Self {
            expanded: Rc::new(RefCell::new(HashSet::new())),
            copied: Rc::new(RefCell::new(HashMap::new())),
            layout: Vec::new(),
            seeded: HashSet::new(),
        }
    }
}

struct SubagentDragGhost;

impl Render for SubagentDragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl OrbitApp {
    pub(super) fn subagent_pane_width(&self) -> Pixels {
        self.subagent_width
            .clamp(px(SUBAGENT_MIN_W), px(SUBAGENT_MAX_W))
    }

    /// Place the open transcript from its anchor when a new inspect arrived
    /// or the reader switched leaves. A wheel that only moves within the
    /// current window does not come through here, so it is not pulled back.
    pub(super) fn place_subagent_scroll(&mut self) {
        self.sync_subagent_folds();
        let epoch = self.subagents.scroll_epoch();
        if epoch != self.subagent_scroll_placed {
            self.subagent_scroll_placed = epoch;
            if self.subagents.is_open() {
                if let Some(id) = self.subagents.selected_id().map(str::to_string) {
                    let anchor = self.subagents.anchor(&id);
                    let count = self.subagents.messages(&id).len();
                    let handle = self
                        .subagent_scrolls
                        .entry(id)
                        .or_insert_with(ScrollHandle::new)
                        .clone();
                    if anchor.following || count == 0 {
                        handle.scroll_to_bottom();
                    } else {
                        let index = count.saturating_sub(anchor.from_bottom.min(count));
                        handle.set_offset(point(px(0.), px(-(index as f32 * MESSAGE_ROW))));
                    }
                }
            }
        }
        if self.subagents.list_shown() {
            let rows = self.subagents.rows().len();
            if rows != self.subagent_list_rows {
                self.subagent_list_rows = rows;
                if self.subagents.list_following() {
                    self.subagent_list_scroll.scroll_to_bottom();
                }
            }
        }
    }

    pub(super) fn clear_subagent_folds(&mut self) {
        self.subagent_folds.clear();
        self.subagent_fold_owner = None;
    }

    /// Turn the last paint's fold clicks into content hashes, then open the
    /// same blocks in the window about to be painted. A block that left the
    /// window keeps its hash.
    fn sync_subagent_folds(&mut self) {
        let pending = self.subagent_fold_owner.as_ref().and_then(|owner| {
            self.subagent_folds.get(owner).map(|ui| {
                (
                    owner.clone(),
                    ui.expanded.borrow().clone(),
                    ui.layout.clone(),
                    ui.seeded.clone(),
                )
            })
        });
        if let Some((owner, open, layout, seeded)) = pending {
            if !layout.is_empty() {
                self.subagents
                    .reconcile_code_folds(&owner, &layout, &open, &seeded);
            }
        }
        let Some(id) = self.subagents.selected_id().map(str::to_string) else {
            return;
        };
        if !self.subagents.is_open() {
            return;
        }
        let sites = subagent_code_sites(&self.subagents, &id);
        let hashes: Vec<u64> = sites
            .iter()
            .filter(|(_, _, hash)| self.subagents.code_expanded(&id, *hash))
            .map(|(_, _, hash)| *hash)
            .collect();
        let ui = self
            .subagent_folds
            .entry(id.clone())
            .or_insert_with(SubagentFoldUi::new);
        let mut open = HashSet::new();
        for &(message_ix, block_ix, hash) in &sites {
            if hashes.contains(&hash) {
                open.insert((message_ix, crate::subagent_status::FOLD_SALT, block_ix));
            }
        }
        *ui.expanded.borrow_mut() = open.clone();
        ui.seeded = open;
        ui.layout = sites;
        self.subagent_fold_owner = Some(id);
    }

    pub(super) fn after_subagent_board(&mut self, cx: &mut Context<Self>) {
        let free = !self.sidepane.read(cx).is_open() && !self.project_panel.read(cx).is_open();
        self.subagents.consider_auto_open(free);
        self.dispatch_subagent_inspects();
        cx.notify();
    }

    fn inspect_available(&self) -> bool {
        self.slash_commands
            .iter()
            .any(|command| command.name == "subagents-inspect-rpc")
    }

    /// Ask pi for each leaf whose snapshot moved. This is an extension
    /// command: it is not the composer's prompt, so it does not arm
    /// "Working for…". One leaf has at most one request in flight.
    pub(super) fn dispatch_subagent_inspects(&mut self) {
        if !self.inspect_available() {
            return;
        }
        let due = self.subagents.due();
        for item in due {
            let request_id = self.subagents.alloc_request_id();
            let message = item.command(&request_id);
            if self.send(
                CommandBody::Prompt {
                    message,
                    images: None,
                    streaming_behavior: None,
                },
                "subagent_inspect",
            ) {
                self.subagents
                    .note_inflight(&item.leaf_id, &request_id, item.lines);
            }
        }
    }

    fn open_subagent(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.subagents.click(id) {
            self.sidepane.update(cx, |pane, cx| pane.close(cx));
            self.project_panel.update(cx, |panel, cx| panel.close(cx));
        }
        self.dispatch_subagent_inspects();
        cx.notify();
    }

    fn close_subagent_panel(&mut self, cx: &mut Context<Self>) {
        self.subagents.close_explicit();
        cx.notify();
    }

    fn copy_subagent(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.subagents.selected_id() else {
            return;
        };
        let text = self.subagents.copy_text(id);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(super) fn subagent_list(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.subagents.list_shown() {
            return None;
        }
        let theme = *theme::get(cx);
        let rows = self.subagents.rows();
        let mut column = div()
            .id("subagent-list")
            .w_full()
            .max_h(px(LIST_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.subagent_list_scroll)
            .on_scroll_wheel(cx.listener(Self::on_subagent_list_wheel))
            .flex()
            .flex_col()
            .py(DynamicSpacing::Base04.px(&theme));
        for row in rows {
            column = column.child(self.subagent_row(row, theme, cx));
        }
        Some(
            div()
                .relative()
                .w_full()
                .flex_none()
                .mb(DynamicSpacing::Base08.px(&theme))
                .rounded(panel::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .overflow_hidden()
                .child(column)
                .child(overlay_thumb(
                    &self.subagent_list_scroll,
                    theme.text_muted.opacity(0.45),
                ))
                .into_any_element(),
        )
    }

    fn subagent_row(&self, row: ListRow, theme: Theme, cx: &Context<Self>) -> AnyElement {
        match row {
            ListRow::Group(label) => div()
                .w_full()
                .h(px(GROUP_H))
                .flex_none()
                .px(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_end()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(label),
                )
                .into_any_element(),
            ListRow::Omitted(count) => div()
                .w_full()
                .h(px(ROW_H))
                .flex_none()
                .px(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_center()
                .child(
                    div()
                        .truncate()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("subagent.omitted", count = count)),
                )
                .into_any_element(),
            ListRow::Agent(id) => {
                let open = self.subagents.is_open() && self.subagents.selected_id() == Some(&id);
                let label = self.subagents.display_label(&id);
                let preview = self.subagents.preview(&id);
                let mark = self.subagents.state(&id).and_then(end_mark);
                let row_id = id.clone();
                div()
                    .id(ElementId::Name(format!("subagent-row-{id}").into()))
                    .w_full()
                    .h(px(ROW_H))
                    .flex_none()
                    .px(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .overflow_hidden()
                    .rounded(panel::RADIUS.px(&theme))
                    .cursor_pointer()
                    .bg(if open {
                        theme.surface_hover
                    } else {
                        theme.surface
                    })
                    .hover(|style| style.bg(theme.surface_hover))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_subagent(&row_id, cx);
                    }))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(180.))
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(preview),
                    )
                    .children(mark.map(|key| {
                        div()
                            .flex_none()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(if key == "subagent.failed" {
                                theme.error
                            } else {
                                theme.text_muted
                            })
                            .child(tr!(key))
                    }))
                    .into_any_element()
            }
        }
    }

    pub(super) fn subagent_pane(&self, cx: &Context<Self>) -> AnyElement {
        let theme = *theme::get(cx);
        let width = self.subagent_pane_width();
        let leaf = self.subagents.viewed();
        let id = leaf.map(|leaf| leaf.id.clone()).unwrap_or_default();
        let label = self.subagents.display_label(&id);
        let state = leaf.map(|leaf| leaf.state.clone()).unwrap_or_default();
        let mark = end_mark(&state);
        let stale = self.subagents.stale(&id);
        let error_code = self.subagents.error_code(&id).map(str::to_string);
        let handle = self
            .subagent_scrolls
            .get(&id)
            .cloned()
            .unwrap_or_else(ScrollHandle::new);
        div()
            .id("subagent-pane")
            .relative()
            .flex_none()
            .w(width)
            .h_full()
            .flex()
            .flex_col()
            .min_h_0()
            .pl(px(6.))
            .child(
                div()
                    .id("subagent-resize-handle")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|style| style.bg(theme.accent.opacity(0.4)))
                    .on_drag(SubagentResize, |_, _, _, cx| cx.new(|_| SubagentDragGhost)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(panel::RADIUS.px(&theme))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(self.subagent_header(
                        &label,
                        mark,
                        stale,
                        error_code.as_deref(),
                        theme,
                        cx,
                    ))
                    .child(self.subagent_body(&id, &handle, theme, cx)),
            )
            .into_any_element()
    }

    fn subagent_header(
        &self,
        label: &str,
        mark: Option<&'static str>,
        stale: bool,
        error_code: Option<&str>,
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .h(px(40.))
            .flex_none()
            .px(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(label.to_string()),
            )
            .children(mark.map(|key| {
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(if key == "subagent.failed" {
                        theme.error
                    } else {
                        theme.text_muted
                    })
                    .child(tr!(key))
            }))
            .children(stale.then(|| {
                div()
                    .flex_none()
                    .max_w(px(120.))
                    .truncate()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(
                        error_code
                            .map(str::to_string)
                            .unwrap_or_else(|| tr!("subagent.stale")),
                    )
            }))
            .child(
                icon_button_frame(div().id("subagent-copy"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .tip(tr!("subagent.copy"))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.surface_hover))
                    .active(|style| style.opacity(PRESS_DIM))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_subagent(cx)))
                    .child(icon(
                        "icons/copy.svg",
                        ButtonSize::Default.icon_size().px(&theme),
                        theme.text_muted,
                    )),
            )
            .child(
                icon_button_frame(div().id("subagent-close"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .tip(tr!("subagent.close"))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.surface_hover))
                    .active(|style| style.opacity(PRESS_DIM))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.close_subagent_panel(cx);
                    }))
                    .child(icon(
                        "icons/x.svg",
                        ButtonSize::Default.icon_size().px(&theme),
                        theme.text_muted,
                    )),
            )
            .into_any_element()
    }

    fn subagent_body(
        &self,
        id: &str,
        handle: &ScrollHandle,
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (expanded, copied) = self
            .subagent_folds
            .get(id)
            .map(|ui| (ui.expanded.clone(), ui.copied.clone()))
            .unwrap_or_else(|| {
                (
                    Rc::new(RefCell::new(HashSet::new())),
                    Rc::new(RefCell::new(HashMap::new())),
                )
            });
        let messages = self.subagents.messages(id);
        let final_output = self.subagents.final_output(id);
        if messages.is_empty() && final_output.is_none() && !self.inspect_available() {
            return div()
                .flex_1()
                .min_h_0()
                .w_full()
                .p(DynamicSpacing::Base16.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .text_center()
                        .child(tr!("subagent.unavailable")),
                )
                .into_any_element();
        }
        let truncated = self.subagents.truncated(id);
        let mut body = div()
            .id(ElementId::Name(format!("subagent-body-{id}").into()))
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(handle)
            .occlude()
            .on_scroll_wheel(cx.listener(Self::on_subagent_wheel))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .p(DynamicSpacing::Base12.px(&theme));
        if truncated {
            body = body.child(
                div()
                    .w_full()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("subagent.truncated")),
            );
        }
        for (index, message) in messages.iter().enumerate() {
            body = body.child(subagent_message(
                messages,
                index,
                message,
                theme,
                expanded.clone(),
                copied.clone(),
            ));
        }
        if let Some(final_output) = final_output {
            body = body.child(
                div()
                    .w_full()
                    .mt(DynamicSpacing::Base08.px(&theme))
                    .pt(DynamicSpacing::Base08.px(&theme))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(transcript_view::render_subagent_markdown(
                        final_output,
                        messages.len(),
                        theme,
                        expanded,
                        copied,
                    )),
            );
        }
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .child(body)
            .child(overlay_thumb(&handle, theme.text_muted.opacity(0.45)))
            .into_any_element()
    }

    fn on_subagent_list_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(following) = follow_after(&self.subagent_list_scroll, event, window.line_height())
        else {
            return;
        };
        self.subagents.set_list_following(following);
        cx.notify();
    }

    fn on_subagent_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.subagents.selected_id().map(str::to_string) else {
            return;
        };
        let Some(handle) = self.subagent_scrolls.get(&id).cloned() else {
            return;
        };
        let line = window.line_height();
        let raw = event.delta.pixel_delta(line);
        let delta_y = if raw.y != px(0.) { raw.y } else { raw.x };
        if delta_y == px(0.) {
            return;
        }
        let offset = handle.offset();
        let max = handle.max_offset().height;
        let previous = offset.y - delta_y;
        let unclamped = previous + delta_y;
        let clamped = unclamped.clamp(-max, px(0.));
        let following = max <= px(0.) || clamped <= -max;
        let was_following = self.subagents.anchor(&id).following;
        if following {
            self.subagents.note_follow(&id, true, 0);
            self.subagents.set_at_top(&id, false);
        } else if delta_y > px(0.) || !was_following {
            let count = self.subagents.messages(&id).len();
            let from_top = ((-f32::from(clamped)) / MESSAGE_ROW).round().max(0.) as usize;
            let from_bottom = count.saturating_sub(from_top.min(count));
            self.subagents.note_follow(&id, false, from_bottom);
            let at_top = clamped >= px(-1.);
            self.subagents.set_at_top(&id, at_top);
            if at_top {
                self.dispatch_subagent_inspects();
            }
        }
        if clamped != offset.y {
            handle.set_offset(point(offset.x, clamped));
        }
        let residual = unclamped - clamped;
        if residual != px(0.) {
            self.transcript.scroll_transcript_by(-residual);
        }
        cx.notify();
    }
}

fn end_mark(state: &str) -> Option<&'static str> {
    match state {
        "complete" => Some("subagent.done"),
        "failed" | "rejected" => Some("subagent.failed"),
        "stopped" => Some("subagent.stopped"),
        _ => None,
    }
}

fn subagent_code_sites(
    board: &crate::subagent_status::SubagentBoard,
    id: &str,
) -> Vec<(usize, usize, u64)> {
    let messages = board.messages(id);
    let mut sites = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.kind == MessageKind::Text {
            sites.extend(transcript_view::markdown_code_sites(&message.text, index));
        }
    }
    if let Some(final_output) = board.final_output(id) {
        sites.extend(transcript_view::markdown_code_sites(
            final_output,
            messages.len(),
        ));
    }
    sites
}

fn subagent_message(
    messages: &[crate::subagent_status::TranscriptMessage],
    index: usize,
    message: &crate::subagent_status::TranscriptMessage,
    theme: Theme,
    expanded: ExpandedBlocks,
    copied: CopiedSections,
) -> AnyElement {
    match message.kind {
        MessageKind::Text => {
            transcript_view::render_subagent_markdown(&message.text, index, theme, expanded, copied)
                .into_any_element()
        }
        MessageKind::ToolCall => {
            let pending = tool_call_pending(messages, index);
            let name = message
                .name
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or_default();
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base02.px(&theme))
                .child(
                    div()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if pending {
                            theme.accent_text
                        } else {
                            theme.text_secondary
                        })
                        .child(name),
                )
                .children((!message.text.trim().is_empty()).then(|| {
                    div()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(message.text.clone())
                }))
                .into_any_element()
        }
        MessageKind::ToolResult => div()
            .w_full()
            .pl(DynamicSpacing::Base12.px(&theme))
            .text_size(TextSize::Small.px(&theme))
            .text_color(if message.is_error {
                theme.error
            } else {
                theme.text_secondary
            })
            .whitespace_normal()
            .child(message.text.clone())
            .into_any_element(),
    }
}

fn tool_call_pending(messages: &[crate::subagent_status::TranscriptMessage], index: usize) -> bool {
    let name = messages[index].name.as_deref();
    !messages[index + 1..]
        .iter()
        .any(|message| message.kind == MessageKind::ToolResult && message.name.as_deref() == name)
}

/// `Some(true)` once the wheel reaches the bottom, `Some(false)` when the
/// reader wheels upward, `None` when a downward wheel has not arrived yet.
fn follow_after(handle: &ScrollHandle, event: &ScrollWheelEvent, line: Pixels) -> Option<bool> {
    let raw = event.delta.pixel_delta(line);
    let delta_y = if raw.y != px(0.) { raw.y } else { raw.x };
    if delta_y == px(0.) {
        return None;
    }
    let offset = handle.offset();
    let max = handle.max_offset().height;
    let previous = offset.y - delta_y;
    let clamped = (previous + delta_y).clamp(-max, px(0.));
    if max <= px(0.) || clamped <= -max {
        Some(true)
    } else if delta_y > px(0.) {
        Some(false)
    } else {
        None
    }
}

/// Where a 3px overlay thumb sits in a viewport. `offset_y` is GPUI's scroll
/// offset: 0 at the top, negative toward the bottom. `None` when the content
/// fits or the viewport is shorter than the minimum thumb.
fn thumb_rect(viewport: f32, max_scroll: f32, offset_y: f32) -> Option<(f32, f32)> {
    if max_scroll <= 1. || viewport <= 12. {
        return None;
    }
    let content = viewport + max_scroll;
    let height = ((viewport * viewport) / content).max(12.).min(viewport);
    let travel = (viewport - height).max(0.);
    let scrolled = (-offset_y).clamp(0., max_scroll);
    let top = scrolled / max_scroll * travel;
    Some((top, height))
}

/// Paint-only thumb, the same mark the terminal uses. It is not a hit target:
/// the wheel still scrolls the view underneath.
fn overlay_thumb(handle: &ScrollHandle, color: gpui::Hsla) -> AnyElement {
    let handle = handle.clone();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let Some((top, height)) = thumb_rect(
                f32::from(bounds.size.height),
                f32::from(handle.max_offset().height),
                f32::from(handle.offset().y),
            ) else {
                return;
            };
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        bounds.origin.x + bounds.size.width - px(4.),
                        bounds.origin.y + px(top),
                    ),
                    size(px(3.), px(height)),
                ),
                Background::from(color),
            ));
        },
    )
    .absolute()
    .inset_0()
    .size_full()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::thumb_rect;

    #[test]
    fn a_thumb_appears_only_when_the_content_overflows() {
        assert!(thumb_rect(160., 0., 0.).is_none());
        assert!(thumb_rect(10., 40., 0.).is_none());
        assert!(thumb_rect(160., 80., 0.).is_some());
    }

    #[test]
    fn the_thumb_rests_at_the_top_and_ends_flush_with_the_bottom() {
        let (top, height) = thumb_rect(200., 100., 0.).unwrap();
        assert_eq!(top, 0.);
        assert!((height - (200. * 200. / 300.)).abs() < 0.01);
        let (top, height) = thumb_rect(200., 100., -100.).unwrap();
        assert!((top + height - 200.).abs() < 0.01);
    }
}
