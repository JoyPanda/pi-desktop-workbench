//! The right dock: one column on the window's right edge that hosts the side
//! panels as tabs — Changes (Review), Files (the project tree), and subagent
//! output — in the style of an editor's tab strip. Several tabs can be open at
//! once; one is shown.
//!
//! [`RightDock`] is only the tab list (order, which tab is shown, whether the
//! dock is collapsed). The panels keep their own state: a tab for Changes or
//! Files exists exactly while that panel reports itself open, and subagent tabs
//! follow the session's [`SubagentBoard`](crate::subagent_status::SubagentBoard).
//! [`RightDock::reconcile`] folds that outside truth back into the list every
//! frame, so every path that opens or closes a panel (palette, shortcut,
//! transcript card, Git page) keeps the strip honest without knowing about it.

use gpui::{anchored, deferred, point, AnchoredPositionMode, ClickEvent, Corner};

use super::helpers::*;
use super::view::header_menu_row;
use super::*;

use crate::commands::{self, CommandId};
use crate::subagent_status::ListRow;
use crate::theme::tokens::{
    button, picker, popover, ButtonSize, DynamicSpacing, IconSize, TextSize,
};

/// Tab strip height; the same row rhythm as the bottom terminal's header.
const TAB_BAR_H: f32 = 40.;
/// Widest a single tab grows before its title truncates.
const TAB_MAX_W: f32 = 168.;

/// One tab in the dock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DockTab {
    /// The Review pane.
    Changes,
    /// The project file tree.
    Files,
    /// A background subagent run, by leaf id.
    Subagent(String),
}

/// The dock's tab list. See the module docs for who owns the truth.
#[derive(Debug, Default)]
pub(super) struct RightDock {
    tabs: Vec<DockTab>,
    active: Option<DockTab>,
    /// The reader hid the dock without closing its tabs.
    collapsed: bool,
}

impl RightDock {
    pub fn tabs(&self) -> &[DockTab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<&DockTab> {
        self.active.as_ref()
    }

    pub fn is_active(&self, tab: &DockTab) -> bool {
        self.active.as_ref() == Some(tab)
    }

    pub fn contains(&self, tab: &DockTab) -> bool {
        self.tabs.contains(tab)
    }

    #[cfg(test)]
    pub fn collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    /// Whether the dock paints: it has a tab to show and was not collapsed.
    pub fn visible(&self) -> bool {
        !self.collapsed && self.active.is_some()
    }

    /// Add `tab` when missing, show it, and expand the dock.
    pub fn open(&mut self, tab: DockTab) {
        if !self.tabs.contains(&tab) {
            self.tabs.push(tab.clone());
        }
        self.active = Some(tab);
        self.collapsed = false;
    }

    /// Show an existing tab and expand the dock. A missing tab is ignored.
    pub fn activate(&mut self, tab: &DockTab) {
        if self.tabs.contains(tab) {
            self.active = Some(tab.clone());
            self.collapsed = false;
        }
    }

    /// Remove `tab`. When it was shown, the neighbour to its right takes over
    /// (the left one for the last tab), like an editor's tab strip.
    pub fn close(&mut self, tab: &DockTab) {
        let Some(index) = self.tabs.iter().position(|open| open == tab) else {
            return;
        };
        self.tabs.remove(index);
        if self.active.as_ref() == Some(tab) {
            self.active = self
                .tabs
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|left| self.tabs.get(left)))
                .cloned();
        }
        if self.tabs.is_empty() {
            self.collapsed = false;
        }
    }

    /// Fold the panels' own open state into the list: drop tabs whose panel
    /// closed, append tabs whose panel opened (the newest is shown and the
    /// dock expands), and repair `active` if it pointed at a dropped tab.
    pub fn reconcile(&mut self, alive: &[DockTab]) {
        self.tabs.retain(|tab| alive.contains(tab));
        let mut opened = None;
        for tab in alive {
            if !self.tabs.contains(tab) {
                self.tabs.push(tab.clone());
                opened = Some(tab.clone());
            }
        }
        if let Some(tab) = opened {
            self.active = Some(tab);
            self.collapsed = false;
        }
        if self
            .active
            .as_ref()
            .is_none_or(|active| !self.tabs.contains(active))
        {
            self.active = self.tabs.last().cloned();
        }
        if self.tabs.is_empty() {
            self.collapsed = false;
        }
    }
}

impl OrbitApp {
    /// Whether the Review pane is the tab on screen. Takes `&App` so the
    /// renderer, the terminal gate, and the chat-column rule share one answer.
    pub(super) fn changes_docked(&self, cx: &App) -> bool {
        self.dock_visible()
            && self.right_dock.is_active(&DockTab::Changes)
            && self.sidepane.read(cx).is_open()
    }

    /// Whether the dock is showing at full width: it fills the window beside
    /// the sessions sidebar and the chat column steps aside.
    pub(super) fn dock_full_width(&self, cx: &App) -> bool {
        self.dock_visible() && self.sidepane.read(cx).is_full_width()
    }

    /// Whether the project tree is the tab on screen.
    pub(super) fn files_docked(&self, cx: &App) -> bool {
        self.dock_visible()
            && self.right_dock.is_active(&DockTab::Files)
            && self.project_panel.read(cx).is_open()
    }

    /// Whether the dock paints at all. Settings, onboarding, and the Usage page
    /// own the window, so the dock steps aside for them.
    pub(super) fn dock_visible(&self) -> bool {
        self.right_dock.visible()
            && !self.settings_open
            && !self.usage_open
            && self.dependencies_ready()
    }

    /// Bring the tab list in line with the panels (see [`RightDock::reconcile`])
    /// and point the shared panels at the shown tab. Runs before layout each
    /// frame and after the Review pane notifies.
    pub(super) fn sync_dock(&mut self, cx: &mut Context<Self>) {
        // Tabs keep their strip position; a tab survives while what backs it
        // is still open.
        let mut ordered = Vec::new();
        for tab in self.right_dock.tabs() {
            let alive = match tab {
                DockTab::Changes => self.sidepane.read(cx).is_open(),
                DockTab::Files => self.project_panel.read(cx).is_open(),
                DockTab::Subagent(id) => self.subagents.knows(id),
            };
            if alive {
                ordered.push(tab.clone());
            }
        }
        if self.sidepane.read(cx).is_open() && !ordered.contains(&DockTab::Changes) {
            ordered.push(DockTab::Changes);
        }
        if self.project_panel.read(cx).is_open() && !ordered.contains(&DockTab::Files) {
            ordered.push(DockTab::Files);
        }
        if let Some(tab) = self.board_dock_tab().filter(|tab| !ordered.contains(tab)) {
            ordered.push(tab);
        }
        self.right_dock.reconcile(&ordered);

        // A caller asked for Review (a transcript card, the Git page, the
        // palette) even though it may already have been open behind another
        // tab: bring it forward.
        let epoch = self.sidepane.read(cx).reveal_epoch();
        if epoch != self.dock_seen_reveal {
            self.dock_seen_reveal = epoch;
            if self.sidepane.read(cx).is_open() {
                self.right_dock.open(DockTab::Changes);
            }
        }

        // The shared subagent view follows the shown tab.
        match self.right_dock.active().cloned() {
            Some(DockTab::Subagent(id)) => {
                self.subagents.focus_leaf(&id);
            }
            _ => {}
        }
        // An empty dock has nothing to be wide, and must not reopen as a full
        // sheet the next time a tab appears.
        if self.right_dock.tabs().is_empty() {
            self.sidepane
                .update(cx, |pane, cx| pane.leave_full_width(cx));
        }
    }

    /// The tab for the subagent view the board is showing, if any.
    pub(super) fn board_dock_tab(&self) -> Option<DockTab> {
        if !self.subagents.is_open() {
            return None;
        }
        self.subagents
            .pane_key()
            .map(|id| DockTab::Subagent(id.to_string()))
    }

    /// Close one tab: shut the panel behind it and let the dock pick the next.
    pub(super) fn close_dock_tab(&mut self, tab: &DockTab, cx: &mut Context<Self>) {
        match tab {
            DockTab::Changes => self.sidepane.update(cx, |pane, cx| pane.close(cx)),
            DockTab::Files => self.project_panel.update(cx, |panel, cx| panel.close(cx)),
            DockTab::Subagent(id) => {
                if self.subagents.pane_key() == Some(id.as_str()) {
                    self.subagents.close_explicit();
                }
            }
        }
        self.right_dock.close(tab);
        self.dock_add_menu_open = false;
        self.sync_dock(cx);
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// Show a tab the dock already holds.
    pub(super) fn activate_dock_tab(&mut self, tab: &DockTab, cx: &mut Context<Self>) {
        self.right_dock.activate(tab);
        self.dock_add_menu_open = false;
        self.sync_dock(cx);
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// Open (or show) the Changes tab. With `uncommitted` (the top-bar chip)
    /// a dirty tree switches Review to its uncommitted diff; every other entry
    /// keeps whatever source the pane already shows.
    pub(super) fn open_changes_tab(&mut self, uncommitted: bool, cx: &mut Context<Self>) {
        // Review replaces the Git page, which shares the window.
        self.close_git_for_review(cx);
        let dirty = uncommitted && (self.added > 0 || self.removed > 0);
        self.sidepane.update(cx, |pane, cx| {
            if dirty {
                pane.show_uncommitted(cx);
            } else {
                pane.show_review(cx);
            }
        });
        self.right_dock.open(DockTab::Changes);
        self.dock_add_menu_open = false;
        self.sync_dock(cx);
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// The palette's Show/Hide Side Panel: hide or show the whole dock, or open
    /// Changes when the dock holds no tab yet.
    pub(super) fn toggle_side_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_dock.tabs().is_empty() {
            self.open_changes_tab(false, cx);
        } else if self.right_dock.visible() && !self.dock_visible() {
            // Hidden by a page that owns the window (Usage), not by the
            // reader: "Show" brings the dock back by leaving that page.
            if self.usage_open {
                self.close_usage(cx);
            }
            cx.notify();
        } else {
            self.toggle_dock_collapsed(cx);
        }
    }

    /// Open (or show) the Files tab.
    pub(super) fn open_files_tab(&mut self, cx: &mut Context<Self>) {
        if !self.project_panel.read(cx).is_open() {
            self.project_panel.update(cx, |panel, cx| panel.toggle(cx));
        }
        self.right_dock.open(DockTab::Files);
        self.dock_add_menu_open = false;
        self.sync_dock(cx);
        cx.notify();
    }

    /// Hide or show the whole dock (the top-bar chip), keeping its tabs.
    pub(super) fn toggle_dock_collapsed(&mut self, cx: &mut Context<Self>) {
        let collapsed = self.right_dock.visible();
        self.right_dock.set_collapsed(collapsed);
        if collapsed {
            // Hiding the dock gives the window back; it returns docked.
            self.sidepane
                .update(cx, |pane, cx| pane.leave_full_width(cx));
        }
        self.dock_add_menu_open = false;
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// Fill the window beside the sessions sidebar, or return to the docked
    /// width the reader dialed in. Applies to whichever tab is shown.
    pub(super) fn toggle_dock_full_width(&mut self, cx: &mut Context<Self>) {
        self.sidepane
            .update(cx, |pane, cx| pane.toggle_full_width(cx));
        self.sync_terminal_active(cx);
        cx.notify();
    }

    pub(super) fn open_subagent_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.subagents.focus_leaf(id) {
            self.right_dock.open(DockTab::Subagent(id.to_string()));
        }
        self.dock_add_menu_open = false;
        self.dispatch_subagent_inspects();
        self.sync_dock(cx);
        cx.notify();
    }

    fn dock_tab_title(&self, tab: &DockTab) -> String {
        match tab {
            DockTab::Changes => tr!("git_panel.tab_changes"),
            DockTab::Files => tr!("explorer.files"),
            DockTab::Subagent(id) => self.subagents.display_label(id),
        }
    }

    fn dock_tab_icon(tab: &DockTab) -> &'static str {
        match tab {
            DockTab::Changes => "icons/file-diff.svg",
            DockTab::Files => "icons/folder.svg",
            DockTab::Subagent(_) => "icons/spark.svg",
        }
    }

    fn dock_tab_chip(
        &self,
        index: usize,
        tab: &DockTab,
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let active = self.right_dock.is_active(tab);
        let title = self.dock_tab_title(tab);
        let shown = tab.clone();
        let closing = tab.clone();
        let middle = tab.clone();
        let text_color = if active {
            theme.text_primary
        } else {
            theme.text_muted
        };
        press(
            div()
                .id(ElementId::Name(format!("dock-tab-{index}").into()))
                .h(ButtonSize::Default.height(&theme))
                .max_w(px(TAB_MAX_W))
                .flex_none()
                .px(DynamicSpacing::Base06.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .rounded(button::RADIUS.px(&theme))
                .cursor_pointer()
                .text_size(TextSize::Small.px(&theme))
                .when(active, |el| el.bg(theme.surface_active))
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_hover).text_color(theme.text_secondary))
                }),
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.activate_dock_tab(&shown, cx);
        }))
        .on_mouse_up(
            MouseButton::Middle,
            cx.listener(move |this, _: &MouseUpEvent, _, cx| {
                this.close_dock_tab(&middle, cx);
            }),
        )
        .child(icon(
            Self::dock_tab_icon(tab),
            IconSize::XSmall.px(&theme),
            text_color,
        ))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(if active {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(text_color)
                .child(title),
        )
        .child(
            icon_button_frame(
                div().id(ElementId::Name(format!("dock-tab-close-{index}").into())),
                &theme,
                ButtonSize::None,
            )
            .group(BUTTON_GROUP)
            .tip(tr!("common.close"))
            .cursor_pointer()
            .hover(|el| el.bg(theme.surface_hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.close_dock_tab(&closing, cx);
                // Without this the chip's own handler would select a tab by
                // the index the strip just shrank past.
                cx.stop_propagation();
            }))
            .child(icon(
                "icons/x.svg",
                ButtonSize::None.icon_size().px(&theme),
                text_color,
            )),
        )
        .into_any_element()
    }

    fn dock_tab_bar(&self, theme: Theme, cx: &Context<Self>) -> AnyElement {
        let full_width = self.sidepane.read(cx).is_full_width();
        let mut strip = div()
            .id("right-dock-tabs")
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .overflow_x_scroll();
        for (index, tab) in self.right_dock.tabs().iter().enumerate() {
            strip = strip.child(self.dock_tab_chip(index, tab, theme, cx));
        }
        div()
            .id("right-dock-tab-bar")
            .flex_none()
            .h(px(TAB_BAR_H))
            .w_full()
            .px_2()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(strip)
            .child(
                div()
                    .relative()
                    .flex_none()
                    .child(
                        icon_button_frame(div().id("right-dock-add"), &theme, ButtonSize::Default)
                            .group(BUTTON_GROUP)
                            .tip(tr!("right_dock.add_tab"))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme.surface_hover))
                            .active(|style| style.bg(theme.surface_active))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                // The press that dismissed the menu (mouse-down
                                // outside) must not reopen it on mouse-up.
                                const GESTURE: Duration = Duration::from_millis(200);
                                if let Some(dismissed) = this.dock_menu_dismissed_at.take() {
                                    if dismissed.elapsed() < GESTURE {
                                        return;
                                    }
                                }
                                this.dock_add_menu_open = !this.dock_add_menu_open;
                                cx.notify();
                            }))
                            .child(icon(
                                "icons/plus.svg",
                                ButtonSize::Default.icon_size().px(&theme),
                                theme.text_secondary,
                            )),
                    )
                    .children(self.dock_add_menu(theme, cx)),
            )
            // The panel-window controls, at the far end like an editor's
            // group controls: full width, then hide the whole dock.
            .child(div().flex_1().min_w_0())
            .child(
                icon_button_frame(
                    div().id("right-dock-full-width"),
                    &theme,
                    ButtonSize::Default,
                )
                .group(BUTTON_GROUP)
                .tip(if full_width {
                    tr!("sidepane.restore_width")
                } else {
                    tr!("sidepane.full_width")
                })
                .cursor_pointer()
                .hover(|style| style.bg(theme.surface_hover))
                .active(|style| style.bg(theme.surface_active))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle_dock_full_width(cx);
                }))
                .child(icon(
                    if full_width {
                        "icons/arrow-shrink.svg"
                    } else {
                        "icons/arrow-expand.svg"
                    },
                    ButtonSize::Default.icon_size().px(&theme),
                    if full_width {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    },
                )),
            )
            .child(
                icon_button_frame(div().id("right-dock-hide"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .tip(commands::tooltip(CommandId::ToggleSidePanel, true))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.surface_hover))
                    .active(|style| style.bg(theme.surface_active))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_dock_collapsed(cx);
                    }))
                    .child(icon(
                        "icons/panel-right.svg",
                        ButtonSize::Default.icon_size().px(&theme),
                        theme.text_secondary,
                    )),
            )
            .into_any_element()
    }

    /// The "+" menu: the panels that are not open yet, then the subagent runs
    /// the board is listing.
    fn dock_add_menu(&self, theme: Theme, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.dock_add_menu_open {
            return None;
        }
        let mut menu = picker_surface(div().id("right-dock-add-menu"), &theme)
            .w(px(220.))
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dock_add_menu_open = false;
                this.dock_menu_dismissed_at = Some(Instant::now());
                cx.notify();
            }))
            .child(header_menu_row(
                "right-dock-add-changes",
                "icons/file-diff.svg",
                tr!("git_panel.tab_changes"),
                commands::spec(CommandId::ReviewChanges).shortcut(),
                self.right_dock.contains(&DockTab::Changes),
                theme,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.open_changes_tab(false, cx)),
            ))
            .child(header_menu_row(
                "right-dock-add-files",
                "icons/folder.svg",
                commands::title(CommandId::ToggleProjectPanel, false),
                commands::spec(CommandId::ToggleProjectPanel).shortcut(),
                self.right_dock.contains(&DockTab::Files),
                theme,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.open_files_tab(cx)),
            ));
        for row in self.subagents.rows() {
            let ListRow::Agent(id) = row else { continue };
            let open = self.right_dock.contains(&DockTab::Subagent(id.clone()));
            let target = id.clone();
            menu = menu.child(header_menu_row(
                ElementId::Name(format!("right-dock-add-subagent-{id}").into()),
                "icons/spark.svg",
                self.subagents.display_label(&id),
                None,
                open,
                theme,
                cx.listener(move |this, _: &MouseUpEvent, _, cx| {
                    this.open_subagent_tab(&target, cx)
                }),
            ));
        }
        Some(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .size(px(0.))
                .child(
                    anchored()
                        .position_mode(AnchoredPositionMode::Local)
                        .anchor(Corner::TopRight)
                        .offset(point(px(0.), popover::MENU_OFFSET))
                        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
                        .child(deferred(menu)),
                )
                .into_any_element(),
        )
    }

    /// The dock column: tab strip over the shown tab's panel, a left hairline
    /// to the main column, and the drag handle that resizes all tabs together.
    pub(super) fn render_right_dock(&self, theme: Theme, cx: &Context<Self>) -> AnyElement {
        let width = self.sidepane.read(cx).width();
        let body = match self.right_dock.active() {
            Some(DockTab::Changes) => self.sidepane.clone().into_any_element(),
            Some(DockTab::Files) => self.project_panel.clone().into_any_element(),
            Some(DockTab::Subagent(_)) => self.subagent_pane(cx),
            None => div().into_any_element(),
        };
        div()
            .id("right-dock")
            .debug_selector(|| "right-dock".to_string())
            .relative()
            .flex_none()
            .w(width)
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme.background)
            .border_l_1()
            .border_color(theme.border)
            .child(self.dock_tab_bar(theme, cx))
            .child(body)
            .child(
                div()
                    .id("right-dock-resize-handle")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    // Flush with the hairline, so the edge users see is the
                    // edge they can grab.
                    .left_0()
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|style| style.bg(theme.accent.opacity(0.4)))
                    .on_drag(crate::sidepane::SidePaneResize, |_, _, _, cx| {
                        cx.new(|_| crate::sidepane::DragGhost)
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(id: &str) -> DockTab {
        DockTab::Subagent(id.to_string())
    }

    #[test]
    fn opening_adds_shows_and_expands() {
        let mut dock = RightDock::default();
        assert!(!dock.visible());
        dock.open(DockTab::Files);
        dock.open(DockTab::Changes);
        dock.set_collapsed(true);
        dock.open(DockTab::Files);
        assert_eq!(dock.tabs(), &[DockTab::Files, DockTab::Changes]);
        assert!(dock.is_active(&DockTab::Files));
        assert!(dock.visible());
    }

    #[test]
    fn closing_the_shown_tab_picks_the_right_neighbour_then_the_left() {
        let mut dock = RightDock::default();
        dock.open(DockTab::Changes);
        dock.open(DockTab::Files);
        dock.open(sub("a"));
        dock.activate(&DockTab::Files);
        dock.close(&DockTab::Files);
        assert!(dock.is_active(&sub("a")));
        dock.close(&sub("a"));
        assert!(dock.is_active(&DockTab::Changes));
        dock.close(&DockTab::Changes);
        assert!(dock.active().is_none());
        assert!(!dock.visible());
    }

    #[test]
    fn closing_a_background_tab_keeps_the_shown_one() {
        let mut dock = RightDock::default();
        dock.open(DockTab::Changes);
        dock.open(DockTab::Files);
        dock.close(&DockTab::Changes);
        assert!(dock.is_active(&DockTab::Files));
    }

    #[test]
    fn reconcile_drops_closed_panels_and_surfaces_new_ones() {
        let mut dock = RightDock::default();
        dock.open(DockTab::Changes);
        dock.open(DockTab::Files);
        dock.reconcile(&[DockTab::Changes, sub("a")]);
        assert_eq!(dock.tabs(), &[DockTab::Changes, sub("a")]);
        assert!(dock.is_active(&sub("a")), "the new tab is shown");
        dock.reconcile(&[DockTab::Changes]);
        assert!(dock.is_active(&DockTab::Changes), "active repaired");
    }

    #[test]
    fn reconcile_without_news_keeps_the_reader_where_they_were() {
        let mut dock = RightDock::default();
        dock.open(DockTab::Changes);
        dock.open(DockTab::Files);
        dock.activate(&DockTab::Changes);
        dock.set_collapsed(true);
        dock.reconcile(&[DockTab::Changes, DockTab::Files]);
        assert!(dock.is_active(&DockTab::Changes));
        assert!(dock.collapsed());
    }

    #[test]
    fn an_empty_dock_is_never_stuck_collapsed() {
        let mut dock = RightDock::default();
        dock.open(DockTab::Files);
        dock.set_collapsed(true);
        dock.reconcile(&[]);
        assert!(!dock.collapsed());
        dock.open(DockTab::Files);
        assert!(dock.visible());
    }
}
