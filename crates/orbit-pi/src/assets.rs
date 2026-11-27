//! App assets — embedded HugeIcons SVGs (free set, MIT) plus the app mark
//! and the Zed-bundled typefaces (IBM Plex Sans for UI, Lilex for code).
//!
//! `Assets` implements gpui's `AssetSource` over a compile-time-embedded
//! directory so packaged builds don't depend on filesystem layout. It also
//! carries gpui-component's Lucide icon set (`gpui_kit_assets::Assets`), the
//! one `IconName` draws from — an app has a single `AssetSource`, so the two
//! trees are multiplexed here rather than chosen at the application.
//!
//! The app's tree wins: 28 file names exist in both (`copy.svg`, `check.svg`,
//! `chevron-*.svg`, …) and they are different artwork, so a path this app
//! ships always resolves to the app's file. Nothing renders a
//! gpui-component icon yet; if one does, this precedence is what to revisit.
//!
//! A second, debug-only directory (`dev-assets/`) overlays the main one on
//! macOS debug builds for the Alpha app mark. Keeping it outside `assets/`
//! means `include_dir!` never bakes dev artwork into release binaries.

use std::borrow::Cow;

use include_dir::{include_dir, Dir};

static ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets");

/// macOS debug-only overlay: the Alpha app mark (regenerate with
/// `scripts/make-alpha-icon.sh`). [`Assets::load`] falls back to it after the
/// production directory, so `app_icon::ASSET` resolves to the same PNG the
/// Dock uses.
#[cfg(all(debug_assertions, target_os = "macos"))]
static DEV_ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/dev-assets");

pub struct Assets;

impl Assets {
    /// The embedded directories, production first, the macOS debug overlay
    /// second. The length is compile-time selected so release builds carry
    /// exactly one directory — a debug-only asset is never embedded in them.
    #[cfg(all(debug_assertions, target_os = "macos"))]
    fn dirs() -> [&'static Dir<'static>; 2] {
        [&ASSETS, &DEV_ASSETS]
    }

    #[cfg(not(all(debug_assertions, target_os = "macos")))]
    fn dirs() -> [&'static Dir<'static>; 1] {
        [&ASSETS]
    }
}

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = Self::dirs()
            .into_iter()
            .find_map(|dir| dir.get_file(path))
            .map(|file| Cow::Owned(file.contents().to_vec()))
        {
            return Ok(Some(bytes));
        }
        // Not ours: gpui-component's `IconName` icons live in the same
        // `gpui_kit_assets::Assets` tree. Its `load` reports a miss as an
        // error where gpui callers expect `Ok(None)`, so a miss is normalized.
        Ok(gpui_kit_assets::Assets.load(path).ok().flatten())
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        let mut paths: Vec<gpui::SharedString> = Self::dirs()
            .into_iter()
            .filter_map(|dir| dir.get_dir(path))
            .flat_map(|dir| dir.files())
            .map(|file| gpui::SharedString::from(file.path().to_string_lossy().to_string()))
            .collect();
        for path in gpui_kit_assets::Assets.list(path)? {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        Ok(paths)
    }
}

/// The bundled typefaces that back Zed's `IBM Plex Sans` / `Lilex` aliases.
/// gpui maps `.ZedSans` → `IBM Plex Sans` and `.ZedMono` → `Lilex`, so these
/// are loaded once at startup and the UI can reference them by those names.
const ZED_FONTS: [&str; 8] = [
    "fonts/zed/IBMPlexSans-Regular.ttf",
    "fonts/zed/IBMPlexSans-Italic.ttf",
    "fonts/zed/IBMPlexSans-SemiBold.ttf",
    "fonts/zed/IBMPlexSans-SemiBoldItalic.ttf",
    "fonts/zed/Lilex-Regular.ttf",
    "fonts/zed/Lilex-Bold.ttf",
    "fonts/zed/Lilex-Italic.ttf",
    "fonts/zed/Lilex-BoldItalic.ttf",
];

/// Load the bundled Zed fonts into gpui's text system so the `.ZedSans` /
/// `.ZedMono` family names resolve to real faces. Call once at startup.
pub fn register_zed_fonts(cx: &mut gpui::App) -> anyhow::Result<()> {
    let mut fonts = Vec::new();
    for path in ZED_FONTS {
        if let Some(bytes) = cx.asset_source().load(path)? {
            fonts.push(bytes);
        }
    }
    cx.text_system().add_fonts(fonts)
}

/// The Nerd Fonts symbols face backing the devicons file glyphs (see
/// `app::helpers::file_glyph`). It carries only private-use glyphs, so it is
/// loaded with the core faces rather than through the curated picker catalog.
const SYMBOL_FONTS: [&str; 1] = ["fonts/SymbolsNerdFont-Regular.ttf"];

/// Load the bundled Nerd Fonts symbols face so file glyphs resolve even when
/// the machine has no Nerd Font installed. Call once at startup.
pub fn register_symbol_fonts(cx: &mut gpui::App) -> anyhow::Result<()> {
    let mut fonts = Vec::new();
    for path in SYMBOL_FONTS {
        if let Some(bytes) = cx.asset_source().load(path)? {
            fonts.push(bytes);
        }
    }
    cx.text_system().add_fonts(fonts)
}

/// Recursively collect every `.ttf` / `.otf` under an embedded directory.
fn collect_fonts(dir: &Dir<'_>, out: &mut Vec<String>) {
    for entry in dir.entries() {
        if let Some(sub) = entry.as_dir() {
            collect_fonts(sub, out);
        } else if let Some(file) = entry.as_file() {
            let path = file.path();
            let is_font = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
                .unwrap_or(false);
            if is_font {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
}

/// Load Orbit's curated font catalog (`fonts/bundled/**`) so the Interface
/// and Code font pickers can resolve every named family without an OS
/// dependency. Call once at startup, after the Zed faces.
pub fn register_bundled_fonts(cx: &mut gpui::App) -> anyhow::Result<()> {
    let Some(dir) = ASSETS.get_dir("fonts/bundled") else {
        return Ok(());
    };
    let mut paths = Vec::new();
    collect_fonts(dir, &mut paths);
    paths.sort();
    let mut fonts = Vec::new();
    for path in paths {
        if let Some(bytes) = cx.asset_source().load(&path)? {
            fonts.push(bytes);
        }
    }
    cx.text_system().add_fonts(fonts)
}

#[cfg(test)]
mod tests {
    /// The macOS debug overlay must serve the Alpha mark through the normal
    /// asset path, so Settings → About's `img(app_icon::ASSET)` resolves.
    #[cfg(all(debug_assertions, target_os = "macos"))]
    #[test]
    fn dev_overlay_serves_the_alpha_mark() {
        use gpui::AssetSource as _;

        use super::Assets;

        let bytes = Assets
            .load("alpha-app-icon.png")
            .expect("embedded assets cannot fail to load")
            .expect("dev-assets/alpha-app-icon.png is embedded by the debug overlay");
        assert_eq!(&bytes[..4], b"\x89PNG");
    }

    #[test]
    fn onest_is_available_in_the_interface_font_catalog() {
        assert!(crate::theme::UI_FONTS
            .iter()
            .any(|choice| choice.label == "Onest" && choice.family == "Onest"));
    }

    #[test]
    fn onest_weights_are_embedded_and_collected_for_registration() {
        use gpui::AssetSource as _;

        let mut paths = Vec::new();
        super::collect_fonts(super::ASSETS.get_dir("fonts/bundled").unwrap(), &mut paths);
        for style in ["Regular", "Medium", "SemiBold", "Bold"] {
            let path = format!("fonts/bundled/onest/Onest-{style}.ttf");
            assert!(
                paths.contains(&path),
                "font registration must include {path}"
            );
            let bytes = super::Assets.load(&path).unwrap().unwrap();
            assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00], "{path}");
        }
        assert!(super::Assets
            .load("fonts/bundled/onest/OFL.txt")
            .unwrap()
            .is_some());
    }

    /// The devicons face is the only source of file glyphs on machines
    /// without a system Nerd Font; if the asset moves or is dropped, every
    /// file icon silently falls back to a text badge.
    #[test]
    fn symbols_font_is_embedded() {
        use gpui::AssetSource as _;

        use super::Assets;

        let bytes = Assets
            .load("fonts/SymbolsNerdFont-Regular.ttf")
            .expect("embedded assets cannot fail to load")
            .expect("SymbolsNerdFont-Regular.ttf is embedded under assets/fonts/");
        // TrueType sfnt version — a cheap sanity check that the file is a
        // font, not an empty or placeholder asset.
        assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    /// The multiplexed source: the app's own files must keep resolving to the
    /// app's artwork (a shared name would otherwise swap in Lucide's), while
    /// an icon only `gpui-kit-assets` carries still resolves — that is what
    /// gpui-component's `IconName` renders through.
    #[test]
    fn asset_source_multiplexes_the_app_tree_and_gpui_kit_assets() {
        use gpui::AssetSource as _;

        use super::Assets;

        // A name both trees carry: the app's file wins.
        let ours = super::ASSETS.get_file("icons/copy.svg").unwrap();
        let loaded = Assets.load("icons/copy.svg").unwrap().unwrap();
        assert_eq!(loaded.as_ref(), ours.contents());

        // A name only gpui-kit-assets carries (Lucide): served from there.
        let theirs = gpui_kit_assets::Assets
            .load("icons/battery-full.svg")
            .expect("gpui-kit-assets carries battery-full");
        assert_eq!(Assets.load("icons/battery-full.svg").unwrap(), theirs);
        assert!(super::ASSETS.get_file("icons/battery-full.svg").is_none());

        // A miss is still a miss, not an error.
        assert!(Assets.load("icons/no-such-icon.svg").unwrap().is_none());

        // `list` merges both trees without repeating a shared name.
        let listed = Assets.list("icons").unwrap();
        assert!(listed.iter().any(|path| path == "icons/copy.svg"));
        assert!(listed.iter().any(|path| path == "icons/battery-full.svg"));
        for path in ["icons/copy.svg", "icons/check.svg"] {
            assert_eq!(
                listed.iter().filter(|listed| *listed == path).count(),
                1,
                "{path} is listed once"
            );
        }
    }
}
