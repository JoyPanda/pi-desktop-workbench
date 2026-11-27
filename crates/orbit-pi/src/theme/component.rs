//! The gpui-component theme bridge (plan D8).
//!
//! gpui-component paints from its own `Theme` global, read through its
//! `ActiveTheme` trait; this app paints from [`super::Theme`]. Both must serve
//! the same values, or a component adopted in Phase 2 would drift from the
//! bespoke surfaces around it. So this writes the app's palette into
//! gpui-component's `Theme` field for field — it never adopts one of the
//! bundled looks, and the app's `Theme` stays the single source of truth.
//!
//! Alongside the colors it carries the parts of the theme that are geometry and
//! type: the radius scale, the UI/mono families, and the rem base. Nothing here
//! is read by the app's own paint sites yet; the point is that when a component
//! *is* adopted it already sees our design tokens.

use gpui::{App, px};
use gpui_component::theme::{Theme as ComponentTheme, ThemeColor, ThemeMode as ComponentMode};

use super::tokens::{Radius, BASE_REM_PX};
use super::{code_font_family, get, ui_font_family, Theme, ThemeMode};

/// Bring gpui-component up and point its theme at the app's palette.
///
/// Registers the globals, actions and key bindings its `Root` needs, writes the
/// active palette into its `Theme`, and keeps that copy in step whenever the app
/// theme changes (palette switch, system appearance, font size).
pub fn install(cx: &mut App) {
    gpui_component::init(cx);
    // Debug builds bind gpui's element inspector to a global shortcut. The app
    // never had one, and a stray ⌥⌘I opening a dev overlay is exactly the kind
    // of behavior change this branch must not ship, so the shortcut is unbound
    // again — a `NoAction` binding added later outranks the one `init` added.
    #[cfg(debug_assertions)]
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        gpui::KeyBinding::new("cmd-alt-i", gpui::NoAction, None),
        #[cfg(not(target_os = "macos"))]
        gpui::KeyBinding::new("ctrl-shift-i", gpui::NoAction, None),
    ]);
    apply(cx);
    cx.observe_global::<Theme>(apply).detach();
}

/// Write the app's active theme into gpui-component's `Theme` global.
fn apply(cx: &mut App) {
    let theme = *get(cx);
    // Start from the installed theme so the registry handles, the highlight
    // theme and the per-component settings `init` parked there survive.
    let mut component = ComponentTheme::global(cx).clone();
    component.mode = match theme.mode {
        ThemeMode::Dark => ComponentMode::Dark,
        ThemeMode::Light => ComponentMode::Light,
    };
    component.colors = colors(&theme);
    component.font_family = ui_font_family();
    // gpui's window rem base. `Root` pushes this into the window
    // (`set_rem_size`), and the app authors every size in px against its own
    // type scale, so keeping gpui's own 16px base is what leaves rem-sized
    // text — anything without an explicit size — where it is today.
    component.font_size = px(BASE_REM_PX);
    component.mono_font_family = code_font_family();
    component.mono_font_size = px(theme.ui.editor_font_size);
    // Zed's radius scale: `Medium` is the general control radius (inputs,
    // list rows), `Large` the elevated-surface one (dialogs, notifications).
    component.radius = Radius::Medium.px(&theme);
    component.radius_lg = Radius::Large.px(&theme);
    cx.set_global(component);
    // Project onto gpui-base's copy of the tokens (it paints its scrollbars,
    // resize handles and text runs without going through gpui-component).
    ComponentTheme::sync_base(cx);
}

/// The app palette as gpui-component's color roles.
///
/// Each field is the closest role the app already names; where the palette has
/// no separate step for a role (the status hovers, for one) the field repeats
/// its base color rather than falling back to a bundled default.
fn colors(theme: &Theme) -> ThemeColor {
    ThemeColor {
        // ── Canvas, ink, hairlines ───────────────────────────────────────────
        background: theme.background,
        foreground: theme.text_primary,
        border: theme.border,
        window_border: theme.border_strong,
        overlay: theme.overlay,
        // ── Surfaces and their text ──────────────────────────────────────────
        popover: theme.surface_elevated,
        popover_foreground: theme.text_primary,
        secondary: theme.surface_secondary,
        secondary_foreground: theme.text_secondary,
        muted: theme.surface_secondary,
        muted_foreground: theme.text_muted,
        accent: theme.surface_hover,
        accent_foreground: theme.text_primary,
        // ── Identity and actions ─────────────────────────────────────────────
        primary: theme.action,
        primary_hover: theme.action_hover,
        primary_active: theme.action_pressed,
        primary_foreground: theme.action_fg,
        // ── Status ───────────────────────────────────────────────────────────
        // One on-fill ink: the palette documents `accent_fg` as the foreground on
        // an accent or error fill, and status fills are the same solid-chip case.
        danger: theme.error,
        danger_hover: theme.error_hover,
        danger_active: theme.error_hover,
        danger_foreground: theme.error_fg,
        success: theme.success,
        success_hover: theme.success,
        success_active: theme.success,
        success_foreground: theme.accent_fg,
        warning: theme.warning,
        warning_hover: theme.warning,
        warning_active: theme.warning,
        warning_foreground: theme.accent_fg,
        info: theme.info,
        info_hover: theme.info,
        info_active: theme.info,
        info_foreground: theme.accent_fg,
        // ── Focus, selection, links ──────────────────────────────────────────
        ring: theme.focus_ring,
        drag_border: theme.focus_ring,
        selection: theme.selection,
        caret: theme.text_primary,
        link: theme.output_link,
        link_hover: theme.accent_text,
        link_active: theme.accent_pressed,
        // ── Buttons ──────────────────────────────────────────────────────────
        button: theme.surface_elevated,
        button_hover: theme.surface_hover,
        button_active: theme.surface_active,
        button_foreground: theme.text_primary,
        button_primary: theme.action,
        button_primary_hover: theme.action_hover,
        button_primary_active: theme.action_pressed,
        button_primary_foreground: theme.action_fg,
        button_secondary: theme.surface_secondary,
        button_secondary_hover: theme.surface_hover,
        button_secondary_active: theme.surface_active,
        button_secondary_foreground: theme.text_secondary,
        button_danger: theme.error,
        button_danger_hover: theme.error_hover,
        button_danger_active: theme.error_hover,
        button_danger_foreground: theme.error_fg,
        button_success: theme.success,
        button_success_hover: theme.success,
        button_success_active: theme.success,
        button_success_foreground: theme.accent_fg,
        button_warning: theme.warning,
        button_warning_hover: theme.warning,
        button_warning_active: theme.warning,
        button_warning_foreground: theme.accent_fg,
        button_info: theme.info,
        button_info_hover: theme.info,
        button_info_active: theme.info,
        button_info_foreground: theme.accent_fg,
        // ── Window chrome ────────────────────────────────────────────────────
        title_bar: theme.surface,
        title_bar_border: theme.border_subtle,
        status_bar: theme.surface,
        status_bar_border: theme.border_subtle,
        sidebar: theme.surface,
        sidebar_foreground: theme.text_secondary,
        sidebar_border: theme.border_subtle,
        sidebar_accent: theme.surface_selected,
        sidebar_accent_foreground: theme.selection_fg,
        sidebar_primary: theme.action,
        sidebar_primary_foreground: theme.action_fg,
        tab_bar: theme.surface,
        tab_bar_segmented: theme.surface_secondary,
        tab: theme.surface,
        tab_active: theme.surface_elevated,
        tab_foreground: theme.text_muted,
        tab_active_foreground: theme.text_primary,
        // ── Lists and tables ─────────────────────────────────────────────────
        list: theme.surface,
        list_even: theme.surface_secondary,
        list_hover: theme.surface_hover,
        list_active: theme.surface_selected,
        list_active_border: theme.border,
        list_head: theme.surface_secondary,
        table: theme.surface,
        table_even: theme.surface_secondary,
        table_hover: theme.surface_hover,
        table_active: theme.surface_selected,
        table_active_border: theme.border,
        table_row_border: theme.border_subtle,
        table_head: theme.surface_secondary,
        table_head_foreground: theme.text_secondary,
        table_foot: theme.surface_secondary,
        table_foot_foreground: theme.text_muted,
        group_box: theme.surface,
        group_box_foreground: theme.text_primary,
        accordion: theme.surface,
        description_list_label: theme.surface_secondary,
        description_list_label_foreground: theme.text_secondary,
        // ── Controls ─────────────────────────────────────────────────────────
        input: theme.surface_secondary,
        switch: theme.border,
        switch_thumb: theme.toggle_knob,
        slider_bar: theme.border,
        slider_thumb: theme.accent,
        progress_bar: theme.accent,
        skeleton: theme.surface_hover,
        // The app paints no scrollbar track (only the transcript draws a thumb),
        // so the track stays the canvas color and the thumb is its muted ink.
        scrollbar: theme.background,
        scrollbar_thumb: theme.text_muted,
        scrollbar_thumb_hover: theme.text_secondary,
        ..ThemeColor::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeKey;
    use gpui::TestAppContext;

    /// Install the bridge over a fresh app the way `main` does: the app theme
    /// first, then gpui-component and the bridge.
    fn install_for(cx: &mut App, key: ThemeKey) -> gpui_component::Theme {
        cx.set_global(Theme::for_id(key));
        install(cx);
        gpui_component::Theme::global(cx).clone()
    }

    /// D8: `ActiveTheme` must serve the app's own token values, not one of
    /// gpui-component's bundled looks.
    #[gpui::test]
    fn bridged_theme_serves_the_app_palette(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let component = install_for(cx, ThemeKey::ORBIT);
            let app = *get(cx);
            assert_eq!(component.mode, ComponentMode::Dark);
            let c = component.colors;
            // Canvas and ink.
            assert_eq!(c.background, app.background);
            assert_eq!(c.foreground, app.text_primary);
            assert_eq!(c.border, app.border);
            assert_eq!(c.window_border, app.border_strong);
            assert_eq!(c.overlay, app.overlay);
            // Surfaces.
            assert_eq!(c.popover, app.surface_elevated);
            assert_eq!(c.popover_foreground, app.text_primary);
            assert_eq!(c.secondary, app.surface_secondary);
            assert_eq!(c.secondary_foreground, app.text_secondary);
            assert_eq!(c.muted, app.surface_secondary);
            assert_eq!(c.muted_foreground, app.text_muted);
            assert_eq!(c.accent, app.surface_hover);
            assert_eq!(c.accent_foreground, app.text_primary);
            // Identity, actions, status.
            assert_eq!(c.primary, app.action);
            assert_eq!(c.primary_hover, app.action_hover);
            assert_eq!(c.primary_active, app.action_pressed);
            assert_eq!(c.primary_foreground, app.action_fg);
            assert_eq!(c.danger, app.error);
            assert_eq!(c.danger_foreground, app.error_fg);
            assert_eq!(c.success, app.success);
            assert_eq!(c.warning, app.warning);
            assert_eq!(c.info, app.info);
            // Focus, selection, links.
            assert_eq!(c.ring, app.focus_ring);
            assert_eq!(c.selection, app.selection);
            assert_eq!(c.caret, app.text_primary);
            assert_eq!(c.link, app.output_link);
            // Icons from the app's palette must not be overridden by anything
            // gpui-component carries of its own.
            assert_ne!(c.background, gpui_component::Theme::default().colors.background);
            // Geometry and type ride the same source.
            assert_eq!(component.font_family, crate::theme::ui_font_family());
            assert_eq!(component.mono_font_family, crate::theme::code_font_family());
            assert_eq!(component.font_size, px(BASE_REM_PX));
            assert_eq!(component.mono_font_size, px(app.ui.editor_font_size));
            assert_eq!(component.radius, Radius::Medium.px(&app));
            assert_eq!(component.radius_lg, Radius::Large.px(&app));
        });
    }

    /// A palette switch after startup must reach gpui-component too, or an
    /// adopted component would keep painting the previous theme.
    ///
    /// Global observers are activated and notified through the effect queue, so
    /// each step is its own `update` — that is also how the running app sees it:
    /// the palette change and the bridge land in the same event-loop turn.
    #[gpui::test]
    fn bridged_theme_follows_the_app_theme(cx: &mut TestAppContext) {
        cx.update(|cx| {
            install_for(cx, ThemeKey::ORBIT);
        });
        let switched = cx.update(|cx| {
            cx.set_global(Theme::for_id(ThemeKey::ORBIT_LIGHT));
            let app = *get(cx);
            (app.background, app.mode, app.selection)
        });
        assert_eq!(switched.1, ThemeMode::Light);
        cx.update(|cx| {
            let component = gpui_component::Theme::global(cx);
            assert_eq!(component.mode, ComponentMode::Light);
            assert_eq!(component.colors.background, switched.0);
            assert_eq!(component.colors.selection, switched.2);
        });
    }
}
