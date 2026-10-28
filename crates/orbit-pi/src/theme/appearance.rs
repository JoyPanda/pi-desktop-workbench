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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearancePrefs {
    pub mode: AppearanceMode,
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
    cx.set_global(Theme::for_id(prefs.resolve(cx.window_appearance())).with_ui(ui));
    Ok(())
}

fn apply_system_appearance(cx: &mut App, system: WindowAppearance) {
    let id = appearance_prefs(cx).resolve(system);
    if get(cx).theme_id != id {
        let ui = get(cx).ui;
        cx.set_global(Theme::for_id(id).with_ui(ui));
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
            assert_eq!(get(cx).ui, ui);
            apply_system_appearance(cx, WindowAppearance::Dark);
            assert_eq!(get(cx).theme_id, ThemeKey::FLEXOKI);
            assert_eq!(appearance_prefs(cx), prefs);
        });
    }
}
