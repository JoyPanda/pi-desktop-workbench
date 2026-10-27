//! Appearance preferences are separate from the palette currently being painted.

use std::{io, path::Path};

use gpui::{App, Global, Window, WindowAppearance};
use serde_json::{json, Value};

use super::{get, keys_for, persist_path, Theme, ThemeKey, ThemeMode};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceMode {
    Light,
    Dark,
    #[default]
    System,
}

impl AppearanceMode {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::System];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AccentColor {
    #[default]
    Default,
    Blue,
    Coral,
    Orange,
    Lime,
    Shu,
    Kaki,
    Sakura,
    Matcha,
    Asagi,
    Ruri,
    Fuji,
    Kogane,
    Ume,
    Gin,
}

impl AccentColor {
    pub const ALL: [Self; 15] = [
        Self::Default,
        Self::Blue,
        Self::Coral,
        Self::Orange,
        Self::Lime,
        Self::Shu,
        Self::Kaki,
        Self::Sakura,
        Self::Matcha,
        Self::Asagi,
        Self::Ruri,
        Self::Fuji,
        Self::Kogane,
        Self::Ume,
        Self::Gin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Blue => "Blue",
            Self::Coral => "Coral",
            Self::Orange => "Orange",
            Self::Lime => "Lime",
            Self::Shu => "Shu",
            Self::Kaki => "Kaki",
            Self::Sakura => "Sakura",
            Self::Matcha => "Matcha",
            Self::Asagi => "Asagi",
            Self::Ruri => "Ruri",
            Self::Fuji => "Fuji",
            Self::Kogane => "Kogane",
            Self::Ume => "Ume",
            Self::Gin => "Gin",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Blue => "blue",
            Self::Coral => "coral",
            Self::Orange => "orange",
            Self::Lime => "lime",
            Self::Shu => "shu",
            Self::Kaki => "kaki",
            Self::Sakura => "sakura",
            Self::Matcha => "matcha",
            Self::Asagi => "asagi",
            Self::Ruri => "ruri",
            Self::Fuji => "fuji",
            Self::Kogane => "kogane",
            Self::Ume => "ume",
            Self::Gin => "gin",
        }
    }

    pub fn apply(self, mut theme: Theme) -> Theme {
        let value = match self {
            Self::Shu => Some(0xE8603C),
            Self::Kaki => Some(0xE58A4E),
            Self::Sakura => Some(0xE89BB0),
            Self::Matcha => Some(0x9DB86B),
            Self::Asagi => Some(0x4FB3B0),
            Self::Ruri => Some(0x5B8DEF),
            Self::Fuji => Some(0x9D8CE0),
            Self::Kogane => Some(0xE3B341),
            Self::Ume => Some(0xC2577E),
            Self::Gin => Some(0xC9C5BD),
            _ => None,
        };
        if let Some(value) = value {
            let base: gpui::Hsla = gpui::rgb(value).into();
            let mut ink = base;
            // The requested base remains unchanged for fills. Adjust only the
            // text role to clear AA on the active palette's canvas and cards.
            let surfaces = [
                theme.background,
                theme.surface,
                theme.surface_secondary,
                theme.surface_elevated,
                theme.surface_hover,
                theme.surface_active,
            ];
            while surfaces.iter().any(|surface| contrast(ink, *surface) < 4.5) {
                ink.l = match theme.mode {
                    ThemeMode::Dark => (ink.l + 0.005).min(1.0),
                    ThemeMode::Light => (ink.l - 0.005).max(0.0),
                };
                if ink.l == 0.0 || ink.l == 1.0 {
                    break;
                }
            }
            let mut hover = base;
            hover.l = (hover.l + 0.05).min(1.0);
            let mut pressed = base;
            pressed.l = (pressed.l + 0.09).min(1.0);
            let foreground = gpui::rgb(0x000000).into();
            theme.accent = base;
            theme.accent_hover = hover;
            theme.accent_pressed = pressed;
            theme.accent_fg = foreground;
            theme.accent_text = ink;
            theme.focus_ring = ink;
            theme.action = base;
            theme.action_hover = hover;
            theme.action_pressed = pressed;
            theme.action_fg = foreground;
            theme.surface_selected = base.opacity(0.14);
            theme.selection = base.opacity(0.14);
            theme.inline_code_bg = base.opacity(0.14);
            return theme;
        }
        let id = match (self, theme.mode) {
            (Self::Default, _) => return theme,
            (Self::Blue, ThemeMode::Dark) => ThemeKey::ORBIT_AZURE,
            (Self::Blue, ThemeMode::Light) => ThemeKey::ORBIT_AZURE_LIGHT,
            (Self::Coral, ThemeMode::Dark) => ThemeKey::ORBIT,
            (Self::Coral, ThemeMode::Light) => ThemeKey::ORBIT_LIGHT,
            (Self::Orange, ThemeMode::Dark) => ThemeKey::FLEXOKI,
            (Self::Orange, ThemeMode::Light) => ThemeKey::FLEXOKI_LIGHT,
            (Self::Lime, ThemeMode::Dark) => ThemeKey::OPERATIONAL,
            (Self::Lime, ThemeMode::Light) => ThemeKey::OPERATIONAL_LIGHT,
            _ => unreachable!(),
        };
        let colors = Theme::for_id(id);
        theme.accent = colors.accent;
        theme.accent_hover = colors.accent_hover;
        theme.accent_pressed = colors.accent_pressed;
        theme.accent_fg = colors.accent_fg;
        theme.accent_text = colors.accent_text;
        theme.focus_ring = colors.focus_ring;
        theme.action = colors.action;
        theme.action_hover = colors.action_hover;
        theme.action_pressed = colors.action_pressed;
        theme.action_fg = colors.action_fg;
        theme.inline_code_bg = colors.inline_code_bg;
        theme
    }
}

fn contrast(a: gpui::Hsla, b: gpui::Hsla) -> f32 {
    let luminance = |color: gpui::Hsla| {
        let color = gpui::Rgba::from(color);
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearancePrefs {
    pub mode: AppearanceMode,
    pub accent_color: AccentColor,
    /// The palette for light appearance, including install-directory themes.
    light_theme: ThemeKey,
    /// The palette for dark appearance.
    dark_theme: ThemeKey,
}

impl Global for AppearancePrefs {}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            mode: AppearanceMode::System,
            accent_color: AccentColor::Default,
            light_theme: ThemeKey::ORBIT_AZURE_LIGHT,
            dark_theme: ThemeKey::ORBIT_AZURE,
        }
    }
}

impl AppearancePrefs {
    /// Read the previous plain theme ID as an explicit appearance choice.
    /// JSON fields are validated independently so one bad value doesn't discard
    /// the user's other selections.
    fn parse(raw: &str) -> Self {
        let mut prefs = Self::default();
        if let Some(id) = ThemeKey::parse(raw) {
            prefs.mode = match id.appearance() {
                ThemeMode::Light => AppearanceMode::Light,
                ThemeMode::Dark => AppearanceMode::Dark,
            };
            prefs.store_theme(id.appearance(), id);
            return prefs;
        }
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return prefs;
        };
        prefs.mode = match value["mode"].as_str() {
            Some("light") => AppearanceMode::Light,
            Some("dark") => AppearanceMode::Dark,
            _ => AppearanceMode::System,
        };
        prefs.accent_color = AccentColor::ALL
            .into_iter()
            .find(|color| Some(color.as_str()) == value["accent_color"].as_str())
            .unwrap_or_default();
        // Old files kept the classic choice in the theme slots and selected
        // Operational through `design_system`, which was never written into
        // those slots. Honor that selection. The settings row is gone, so the
        // slots themselves become the operational pair.
        let operational = value["design_system"].as_str().is_some_and(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "operational" | "restrained" | "operational-minimalism"
            )
        });
        if operational {
            prefs.light_theme = ThemeKey::OPERATIONAL_LIGHT;
            prefs.dark_theme = ThemeKey::OPERATIONAL;
            return prefs;
        }
        for (key, mode) in [
            ("light_theme", ThemeMode::Light),
            ("dark_theme", ThemeMode::Dark),
        ] {
            if let Some(id) = value[key].as_str().and_then(ThemeKey::parse) {
                prefs.store_theme(mode, id);
            }
        }
        prefs
    }

    pub fn theme(self, mode: ThemeMode) -> ThemeKey {
        match mode {
            ThemeMode::Light => self.light_theme,
            ThemeMode::Dark => self.dark_theme,
        }
    }

    fn store_theme(&mut self, mode: ThemeMode, id: ThemeKey) {
        if id.appearance() != mode {
            return;
        }
        match mode {
            ThemeMode::Light => self.light_theme = id,
            ThemeMode::Dark => self.dark_theme = id,
        }
    }

    /// Select the palette for `mode`. The theme's own chrome travels with it.
    pub fn set_theme(&mut self, mode: ThemeMode, id: ThemeKey) {
        self.store_theme(mode, id);
    }

    pub fn resolve(self, system: WindowAppearance) -> ThemeKey {
        let mode = match self.mode {
            AppearanceMode::Light => ThemeMode::Light,
            AppearanceMode::Dark => ThemeMode::Dark,
            AppearanceMode::System => match system {
                WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeMode::Light,
                WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            },
        };
        self.theme(mode)
    }

    pub fn apply(self, theme: Theme) -> Theme {
        self.accent_color.apply(theme)
    }

    pub(super) fn load() -> Self {
        match std::fs::read_to_string(persist_path()) {
            Ok(raw) => Self::parse(&raw),
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("Failed to load appearance preferences: {error}");
                }
                Self::default()
            }
        }
    }

    fn persist_to(self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            json!({
                "mode": self.mode.as_str(),
                "accent_color": self.accent_color.as_str(),
                "light_theme": self.light_theme.as_str(),
                "dark_theme": self.dark_theme.as_str(),
            })
            .to_string(),
        )
    }
}

/// Every theme of `mode`: built-ins first, then install-directory themes.
pub fn themes_for(mode: ThemeMode) -> impl Iterator<Item = ThemeKey> {
    keys_for(mode).into_iter()
}

pub fn appearance_prefs(cx: &App) -> AppearancePrefs {
    *cx.global::<AppearancePrefs>()
}

/// Only user edits are persisted; OS changes must never overwrite the preference.
pub fn set_appearance_prefs(cx: &mut App, prefs: AppearancePrefs) -> io::Result<()> {
    if appearance_prefs(cx) == prefs {
        return Ok(());
    }
    prefs.persist_to(&persist_path())?;
    cx.set_global(prefs);
    // Notify even when editing the inactive palette or choosing a mode that
    // resolves to the same palette, so the settings controls stay in sync.
    let ui = get(cx).ui;
    cx.set_global(prefs.apply(Theme::for_id(prefs.resolve(cx.window_appearance())).with_ui(ui)));
    Ok(())
}

fn apply_system_appearance(cx: &mut App, system: WindowAppearance) {
    let id = appearance_prefs(cx).resolve(system);
    if get(cx).theme_id != id {
        let ui = get(cx).ui;
        cx.set_global(appearance_prefs(cx).apply(Theme::for_id(id).with_ui(ui)));
    }
}

pub fn watch_system_appearance(window: &Window, cx: &mut App) {
    apply_system_appearance(cx, window.appearance());
    // The subscription belongs to this window and ends when the window closes.
    window
        .observe_window_appearance(|window, cx| {
            apply_system_appearance(cx, window.appearance());
        })
        .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_preferences_follow_system() {
        let prefs = AppearancePrefs::default();
        assert_eq!(prefs.mode, AppearanceMode::System);
        assert_eq!(prefs.theme(ThemeMode::Dark), ThemeKey::ORBIT_AZURE);
        assert_eq!(prefs.theme(ThemeMode::Light), ThemeKey::ORBIT_AZURE_LIGHT);
    }

    #[test]
    fn operational_design_system_still_selects_operational() {
        let json = r#"{"mode":"dark","design_system":"operational","light_theme":"orbit-paper","dark_theme":"flexoki"}"#;
        let prefs = AppearancePrefs::parse(json);
        assert_eq!(prefs.resolve(WindowAppearance::Dark), ThemeKey::OPERATIONAL);
        assert_eq!(prefs.theme(ThemeMode::Light), ThemeKey::OPERATIONAL_LIGHT);
        assert_eq!(prefs.theme(ThemeMode::Dark), ThemeKey::OPERATIONAL);
        assert_ne!(prefs.theme(ThemeMode::Dark), ThemeKey::ORBIT_AZURE);
        assert_ne!(prefs.theme(ThemeMode::Dark), ThemeKey::FLEXOKI);
    }

    #[test]
    fn legacy_operational_theme_key_selects_the_system() {
        let prefs = AppearancePrefs::parse("operational-light");
        assert_eq!(prefs.mode, AppearanceMode::Light);
        assert_eq!(
            prefs.resolve(WindowAppearance::Light),
            ThemeKey::OPERATIONAL_LIGHT
        );
    }

    #[test]
    fn legacy_dark_theme_preserves_explicit_choice() {
        assert_eq!(
            AppearancePrefs::parse("flexoki\n"),
            AppearancePrefs {
                mode: AppearanceMode::Dark,
                dark_theme: ThemeKey::FLEXOKI,
                ..AppearancePrefs::default()
            }
        );
    }

    #[test]
    fn legacy_light_alias_preserves_explicit_choice() {
        assert_eq!(
            AppearancePrefs::parse("one-light"),
            AppearancePrefs {
                mode: AppearanceMode::Light,
                light_theme: ThemeKey::ORBIT_LIGHT,
                ..AppearancePrefs::default()
            }
        );
    }

    #[test]
    fn system_resolves_normal_and_vibrant_appearances() {
        let prefs = AppearancePrefs {
            dark_theme: ThemeKey::FLEXOKI,
            ..AppearancePrefs::default()
        };
        for (system, expected) in [
            (WindowAppearance::Light, ThemeKey::ORBIT_AZURE_LIGHT),
            (WindowAppearance::VibrantLight, ThemeKey::ORBIT_AZURE_LIGHT),
            (WindowAppearance::Dark, ThemeKey::FLEXOKI),
            (WindowAppearance::VibrantDark, ThemeKey::FLEXOKI),
        ] {
            assert_eq!(prefs.resolve(system), expected);
        }
    }

    #[test]
    fn explicit_modes_ignore_system() {
        for (mode, expected) in [
            (AppearanceMode::Light, ThemeKey::ORBIT_AZURE_LIGHT),
            (AppearanceMode::Dark, ThemeKey::FLEXOKI),
        ] {
            let prefs = AppearancePrefs {
                mode,
                dark_theme: ThemeKey::FLEXOKI,
                ..AppearancePrefs::default()
            };
            for system in [WindowAppearance::Light, WindowAppearance::Dark] {
                assert_eq!(prefs.resolve(system), expected);
            }
        }
    }

    #[test]
    fn inactive_theme_edit_does_not_change_resolved_palette() {
        let mut prefs = AppearancePrefs {
            mode: AppearanceMode::Light,
            ..AppearancePrefs::default()
        };
        prefs.set_theme(ThemeMode::Dark, ThemeKey::ORBIT_DARKER);
        assert_eq!(
            prefs.resolve(WindowAppearance::Dark),
            ThemeKey::ORBIT_AZURE_LIGHT
        );
        prefs.mode = AppearanceMode::Dark;
        assert_eq!(
            prefs.resolve(WindowAppearance::Light),
            ThemeKey::ORBIT_DARKER
        );
    }

    #[test]
    fn invalid_fields_fall_back_independently() {
        let prefs = AppearancePrefs::parse(
            r#"{"mode":"unknown","light_theme":"nord","dark_theme":"flexoki"}"#,
        );
        assert_eq!(
            prefs,
            AppearancePrefs {
                dark_theme: ThemeKey::FLEXOKI,
                ..AppearancePrefs::default()
            }
        );
        assert_eq!(
            AppearancePrefs::parse("broken JSON"),
            AppearancePrefs::default()
        );
        assert_eq!(
            AppearancePrefs::parse(r#"{"dark_theme":"orbit-light"}"#),
            AppearancePrefs::default()
        );
    }

    #[test]
    fn preferences_round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("orbit-appearance-{}", std::process::id()));
        let path = dir.join("prefs/theme");
        for mode in AppearanceMode::ALL {
            let prefs = AppearancePrefs {
                mode,
                accent_color: AccentColor::Coral,
                dark_theme: ThemeKey::ORBIT_COBALT,
                ..AppearancePrefs::default()
            };
            prefs.persist_to(&path).unwrap();
            assert_eq!(
                AppearancePrefs::parse(&std::fs::read_to_string(&path).unwrap()),
                prefs
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accent_choices_round_trip_and_apply_in_both_modes() {
        for color in AccentColor::ALL {
            let prefs = AppearancePrefs {
                accent_color: color,
                ..AppearancePrefs::default()
            };
            let raw = json!({"accent_color": color.as_str()}).to_string();
            assert_eq!(AppearancePrefs::parse(&raw).accent_color, color);
            for id in [ThemeKey::ORBIT_AZURE, ThemeKey::ORBIT_AZURE_LIGHT] {
                let base = Theme::for_id(id);
                let applied = prefs.apply(base);
                assert_eq!(applied.theme_id, id);
                assert_eq!(applied.background, base.background);
                if color == AccentColor::Default {
                    assert_eq!(applied.accent, base.accent);
                }
            }
        }
        assert_eq!(
            AppearancePrefs::parse(r#"{"accent_color":"invalid"}"#).accent_color,
            AccentColor::Default
        );
    }

    #[test]
    fn named_accents_keep_requested_bases_and_contrast_on_all_palettes() {
        for (color, value) in [
            (AccentColor::Shu, 0xE8603C),
            (AccentColor::Kaki, 0xE58A4E),
            (AccentColor::Sakura, 0xE89BB0),
            (AccentColor::Matcha, 0x9DB86B),
            (AccentColor::Asagi, 0x4FB3B0),
            (AccentColor::Ruri, 0x5B8DEF),
            (AccentColor::Fuji, 0x9D8CE0),
            (AccentColor::Kogane, 0xE3B341),
            (AccentColor::Ume, 0xC2577E),
            (AccentColor::Gin, 0xC9C5BD),
        ] {
            for id in crate::theme::bundled_keys() {
                let base = Theme::for_id(id);
                let theme = color.apply(base);
                assert_eq!(theme.accent, gpui::Hsla::from(gpui::rgb(value)));
                for surface in [
                    theme.background,
                    theme.surface,
                    theme.surface_secondary,
                    theme.surface_elevated,
                    theme.surface_hover,
                    theme.surface_active,
                ] {
                    assert!(
                        contrast(theme.accent_text, surface) >= 4.5,
                        "{color:?} {id:?}: {}",
                        contrast(theme.accent_text, surface)
                    );
                }
                for fill in [theme.action, theme.action_hover, theme.action_pressed] {
                    assert!(
                        contrast(theme.action_fg, fill) >= 4.5,
                        "{color:?} {id:?} action: {}",
                        contrast(theme.action_fg, fill)
                    );
                }
                assert_eq!(theme.success, base.success);
                assert_eq!(theme.error, base.error);
                assert_eq!(theme.info, base.info);
            }
        }
    }

    #[test]
    fn persistence_errors_are_returned() {
        let dir =
            std::env::temp_dir().join(format!("orbit-appearance-error-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(AppearancePrefs::default().persist_to(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[gpui::test]
    fn explicit_mode_ignores_live_system_changes(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let prefs = AppearancePrefs {
                mode: AppearanceMode::Dark,
                dark_theme: ThemeKey::FLEXOKI,
                ..AppearancePrefs::default()
            };
            cx.set_global(prefs);
            cx.set_global(Theme::for_id(ThemeKey::FLEXOKI));
            apply_system_appearance(cx, WindowAppearance::Light);
            assert_eq!(get(cx).theme_id, ThemeKey::FLEXOKI);
            assert_eq!(appearance_prefs(cx), prefs);
        });
    }

    #[test]
    fn dropdown_choices_partition_the_catalog() {
        let light: Vec<_> = themes_for(ThemeMode::Light).collect();
        let dark: Vec<_> = themes_for(ThemeMode::Dark).collect();
        assert!(light.contains(&ThemeKey::ORBIT_LIGHT));
        assert!(dark.contains(&ThemeKey::ORBIT));
        assert!(light.iter().all(|id| id.appearance() == ThemeMode::Light));
        assert!(dark.iter().all(|id| id.appearance() == ThemeMode::Dark));
        assert_eq!(light.len() + dark.len(), crate::theme::bundled_keys().len());
        assert!(dark.iter().all(|id| id.appearance() == ThemeMode::Dark));
    }

    #[test]
    fn every_theme_round_trips_through_its_key() {
        for id in crate::theme::bundled_keys() {
            assert_eq!(ThemeKey::parse(id.as_str()), Some(id));
        }
    }

    #[test]
    fn retired_theme_keys_still_load() {
        assert_eq!(ThemeKey::parse("dracula"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("nord"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("one-light"), Some(ThemeKey::ORBIT_LIGHT));
        assert_eq!(
            ThemeKey::parse("gruvbox-light"),
            Some(ThemeKey::ORBIT_LIGHT)
        );
    }

    #[gpui::test]
    fn system_changes_apply_live_and_preserve_ui_preferences(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let prefs = AppearancePrefs {
                accent_color: AccentColor::Coral,
                dark_theme: ThemeKey::FLEXOKI,
                ..AppearancePrefs::default()
            };
            cx.set_global(prefs);
            let ui = super::super::UiPrefs {
                reduce_motion: true,
                ..super::super::UiPrefs::default()
            };
            cx.set_global(Theme::for_id(ThemeKey::FLEXOKI).with_ui(ui));
            apply_system_appearance(cx, WindowAppearance::Light);
            assert_eq!(get(cx).theme_id, ThemeKey::ORBIT_AZURE_LIGHT);
            assert_eq!(
                get(cx).accent,
                Theme::for_id(ThemeKey::ORBIT_LIGHT).accent
            );
            assert_eq!(get(cx).ui, ui);
            apply_system_appearance(cx, WindowAppearance::Dark);
            assert_eq!(get(cx).theme_id, ThemeKey::FLEXOKI);
            assert_eq!(get(cx).accent, Theme::for_id(ThemeKey::ORBIT).accent);
            assert_eq!(appearance_prefs(cx), prefs);
        });
    }
}
