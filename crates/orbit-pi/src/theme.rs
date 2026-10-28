//! App theme: a GPUI [`Global`] with a selectable palette.
//!
//! Colors are semantic roles (backgrounds, text, borders, transcript
//! surfaces) rather than raw steps, so every paint site reads from one
//! place. A [`ThemeKey`] names a palette loaded by [`registry`]; [`Chrome`]
//! chooses flat or raised geometry; [`AppearancePrefs`] chooses which light
//! or dark palette to use, and optionally follows the system appearance.
//! The UI face is Zed's bundled IBM Plex Sans (`.ZedSans`); code surfaces
//! use Zed's Lilex (`.ZedMono`).

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use gpui::{hsla, point, px, rgb, App, BoxShadow, Global, Hsla, Pixels, SharedString};
use serde_json::Value;

use crate::highlight::TokenClass;

mod appearance;
pub use appearance::*;

mod registry;
pub use registry::{bundled_keys, keys_for, load_install_themes, Chrome, ThemeKey, TypeScale};

/// Zed's design tokens (spacing, type, icons, buttons, elevation, motion),
/// resolved through this theme.
pub mod tokens;

/// Dark or light appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Dark,
    Light,
}

/// Semantic palette. Copy so list closures and hover styles can capture it
/// without a lifetime.
///
/// The first group is the canonical surface: one semantic role per meaning,
/// resolved from the active [`ThemeKey`]. Paint sites should read these names
/// rather than raw color values. The remaining fields are specialized roles
/// (code washes, syntax, selection washes, conversation output) that do not
/// belong to the general surface ramp.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub theme_id: ThemeKey,
    pub mode: ThemeMode,
    /// Flat or raised geometry. Comes from the theme file, not a settings row.
    pub chrome: Chrome,
    /// UI customization (General settings): language + font sizes.
    pub ui: UiPrefs,
    /// Conversation type scale. Body, headings, and aside are authored here
    /// and scaled by the UI font size. Code is scaled by the editor font size.
    pub type_scale: TypeScale,

    // ── Surfaces ────────────────────────────────────────────────────────
    /// Root window and workspace canvas.
    pub background: Hsla,
    /// Primary panel surface: sidebars, panes, composer.
    pub surface: Hsla,
    /// Nested/secondary containers and subtle wells.
    pub surface_secondary: Hsla,
    /// Raised cards, chips, menus, popovers, modals.
    pub surface_elevated: Hsla,
    /// Pointer-hover feedback.
    pub surface_hover: Hsla,
    /// Pressed/open/highlighted feedback.
    pub surface_active: Hsla,
    /// Hairline between regions.
    pub border: Hsla,
    /// Quiet separator inside a surface.
    pub border_subtle: Hsla,

    // ── Text ─────────────────────────────────────────────────────────────
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub text_disabled: Hsla,

    // ── Accent ───────────────────────────────────────────────────────────
    /// The palette's identity hue: primary fills, selection washes, focus,
    /// tints, and accent-colored text derive from it.
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_pressed: Hsla,
    /// Foreground on an accent or error fill.
    pub accent_fg: Hsla,
    /// Accent-colored text/icons on canvas or panel surfaces.
    pub accent_text: Hsla,
    /// Selected navigation/list row wash. Classic palettes share this with
    /// [`Self::selection`]; Operational gives rows their own olive-tinted step.
    pub surface_selected: Hsla,
    /// Text selection and find-match wash (terminal, editor, transcript).
    pub selection: Hsla,
    /// Foreground on [`Self::surface_selected`] and [`Self::selection`].
    pub selection_fg: Hsla,
    /// Keyboard focus indicator.
    pub focus_ring: Hsla,

    // ── Primary action ───────────────────────────────────────────────────
    /// Fill of a primary action button (send, commit, Save, Update). A palette
    /// may key it to its accent or to a neutral step, so an existing identity
    /// keeps its primary buttons while the accent stays the selection/text hue.
    pub action: Hsla,
    pub action_hover: Hsla,
    pub action_pressed: Hsla,
    /// Foreground on [`Self::action`].
    pub action_fg: Hsla,

    // ── Status ───────────────────────────────────────────────────────────
    pub success: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
    pub error_hover: Hsla,
    /// Foreground on a solid error fill.
    pub error_fg: Hsla,
    pub info: Hsla,

    // ── Specialized surfaces and roles ───────────────────────────────────
    /// Knob of a toggle switch. Kept light in both modes so it reads on the
    /// accent (on) and the elevated track (off).
    pub toggle_knob: Hsla,
    /// Translucent hover wash over content rows.
    pub overlay: Hsla,
    /// Stronger translucent hover/selection wash.
    pub overlay_strong: Hsla,
    /// Ink-derived strong hairline for floating surfaces.
    pub border_strong: Hsla,
    pub assistant_text: Hsla,
    pub code_bg: Hsla,
    pub code_text: Hsla,
    /// Syntax token colors (transcript code blocks + the review diff). A
    /// palette sets these explicitly so a ported editor theme keeps its own
    /// syntax identity instead of borrowing the UI's semantic roles.
    pub syn_string: Hsla,
    pub syn_number: Hsla,
    pub syn_function: Hsla,
    pub syn_type: Hsla,
    pub syn_comment: Hsla,
    pub syn_literal: Hsla,
    pub syn_meta: Hsla,
    pub syn_operator: Hsla,
    /// Inline-code chip wash. The wash is the accent at low alpha unless the
    /// theme sets `output.inlineCodeBg`.
    pub inline_code_bg: Hsla,
    /// Set only when the theme file names an inline-code foreground. Paint
    /// falls back to the surrounding prose ink when this is `None`.
    pub output_inline_fg: Option<Hsla>,
    pub output_prose: Hsla,
    pub output_heading: Hsla,
    pub output_link: Hsla,
    pub output_code_bg: Hsla,
    pub output_code_fg: Hsla,
    pub output_code_header_bg: Hsla,
    pub output_code_header_fg: Hsla,
    pub output_user_bg: Hsla,
    pub output_user_fg: Hsla,
    pub output_quote_bg: Hsla,
    pub output_quote_border: Hsla,
    pub output_quote_fg: Hsla,
    /// Syntax keyword. Shell command tokens use the same color. Omitted
    /// keywords fall back to [`Self::accent_text`].
    pub syn_keyword: Hsla,
    pub shadow_contact: Hsla,
}

impl Global for Theme {}

/// Interface language for the workbench chrome. Re-exported from
/// [`crate::i18n`] so every persisted pref reads as a locale id while the
/// `Language` name stays stable for the settings surface.
pub use crate::i18n::AppLanguage as Language;

/// Orbit UI preferences: language, type sizes, density, and composer behavior.
/// Every size is px with the built-in defaults (UI 14, terminal / editor 13);
/// only spacing density remains a percentage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPrefs {
    pub language: Language,
    /// Interface text size, px ("UI font size").
    pub ui_font_size: f32,
    /// Terminal / tool-output size, px ("Terminal Font Size").
    pub terminal_font_size: f32,
    /// Editor / diff / code-block size, px ("Editor Font Size").
    pub editor_font_size: f32,
    /// Global spacing multiplier, percent ("Spacing Density").
    pub spacing_density: u32,
    /// Honor reduce-motion: perpetual/looping animations (spinners, the
    /// running-session shimmer, the drop-overlay fade) render static.
    pub reduce_motion: bool,
    /// Enter's delivery mode while the agent is working (Orbit-only).
    pub composer_send_mode: crate::composer_send::SendMode,
}

/// The interface text size authored against, px. Chrome sizes scale relative
/// to this, like a rem authored at its default UI font size.
pub const DEFAULT_UI_FONT_SIZE: f32 = 14.;
/// Selectable interface / terminal / editor font sizes, px.
pub const FONT_SIZES: [f32; 8] = [11., 12., 13., 14., 15., 16., 18., 20.];
/// Selectable spacing densities, percent.
pub const SPACING_DENSITIES: [u32; 9] = [80, 85, 90, 95, 100, 105, 110, 115, 120];

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            language: Language::System,
            ui_font_size: DEFAULT_UI_FONT_SIZE,
            terminal_font_size: 13.,
            editor_font_size: 13.,
            spacing_density: 100,
            reduce_motion: false,
            composer_send_mode: crate::composer_send::SendMode::default(),
        }
    }
}

impl UiPrefs {
    fn persist_path() -> PathBuf {
        crate::platform::home_dir()
            .join(".orbit-pi")
            .join("ui.json")
    }

    fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::persist_path()) else {
            return Self::default();
        };
        serde_json::from_str::<Value>(&raw)
            .map(|value| Self::from_value(&value))
            .unwrap_or_default()
    }

    /// Nearest selectable font size, for migrating legacy percentage scales.
    fn nearest_size(value: f32) -> f32 {
        // A NaN in a legacy file must not panic startup; fall back to the
        // default size (a NaN-safe `total_cmp` alone would pick
        // `FONT_SIZES[0]`, because NaN compares greater than everything).
        if value.is_nan() {
            return DEFAULT_UI_FONT_SIZE;
        }
        FONT_SIZES
            .into_iter()
            .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
            .unwrap_or(DEFAULT_UI_FONT_SIZE)
    }

    /// Parse prefs from JSON, falling back to defaults per field. A legacy
    /// `interface_scale` percentage migrates onto the equivalent px size.
    fn from_value(value: &Value) -> Self {
        let mut prefs = Self::default();
        if let Some(language) = value
            .get("language")
            .and_then(Value::as_str)
            .and_then(Language::parse)
        {
            prefs.language = language;
        }
        if let Some(size) = value.get("ui_font_size").and_then(Value::as_f64) {
            let size = size as f32;
            if FONT_SIZES.contains(&size) {
                prefs.ui_font_size = size;
            }
        } else if let Some(scale) = value.get("interface_scale").and_then(Value::as_u64) {
            let px = scale as f32 / 100. * DEFAULT_UI_FONT_SIZE;
            prefs.ui_font_size = Self::nearest_size(px);
        }
        if let Some(size) = value.get("terminal_font_size").and_then(Value::as_f64) {
            if FONT_SIZES.contains(&(size as f32)) {
                prefs.terminal_font_size = size as f32;
            }
        }
        if let Some(size) = value.get("editor_font_size").and_then(Value::as_f64) {
            if FONT_SIZES.contains(&(size as f32)) {
                prefs.editor_font_size = size as f32;
            }
        } else if let Some(size) = value.get("code_font_size").and_then(Value::as_f64) {
            let size = size as f32;
            if FONT_SIZES.contains(&size) {
                prefs.editor_font_size = size;
            }
        }
        if let Some(density) = value.get("spacing_density").and_then(Value::as_u64) {
            let density = density as u32;
            if SPACING_DENSITIES.contains(&density) {
                prefs.spacing_density = density;
            }
        }
        if let Some(reduce) = value.get("reduce_motion").and_then(Value::as_bool) {
            prefs.reduce_motion = reduce;
        }
        if let Some(mode) = value
            .get("composer_send_mode")
            .and_then(Value::as_str)
            .and_then(crate::composer_send::SendMode::parse)
        {
            prefs.composer_send_mode = mode;
        }
        prefs
    }

    fn persist(self) {
        let path = Self::persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.to_value().to_string());
    }

    fn to_value(self) -> Value {
        serde_json::json!({
            "language": self.language.as_str(),
            "ui_font_size": self.ui_font_size,
            "terminal_font_size": self.terminal_font_size,
            "editor_font_size": self.editor_font_size,
            "spacing_density": self.spacing_density,
            "reduce_motion": self.reduce_motion,
            "composer_send_mode": self.composer_send_mode.as_str(),
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::for_id(ThemeKey::ORBIT_AZURE)
    }
}

/// Current UI / code font families. Stored as statics (readable without an
/// app handle) so transcript helpers, which don't carry `cx`, can look them
/// up — keeping [`Theme`] itself `Copy`. Persisted to `~/.orbit-pi/fonts.json`.
static UI_FONT_FAMILY: RwLock<SharedString> = RwLock::new(SharedString::new_static(".ZedSans"));
static CODE_FONT_FAMILY: RwLock<SharedString> = RwLock::new(SharedString::new_static(".ZedMono"));

/// The persisted font-families payload.
#[derive(Debug, Clone, PartialEq)]
pub struct FontPrefs {
    /// UI face family (`.ZedSans` → bundled IBM Plex Sans by default).
    pub ui_font_family: SharedString,
    /// Code/monospace face family (`.ZedMono` → bundled Lilex by default).
    pub code_font_family: SharedString,
}

impl Default for FontPrefs {
    fn default() -> Self {
        Self {
            ui_font_family: ".ZedSans".into(),
            code_font_family: ".ZedMono".into(),
        }
    }
}

impl FontPrefs {
    fn persist_path() -> PathBuf {
        crate::platform::home_dir()
            .join(".orbit-pi")
            .join("fonts.json")
    }

    fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::persist_path()) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let mut prefs = Self::default();
        if let Some(family) = value.get("ui_font_family").and_then(Value::as_str) {
            if !family.is_empty() {
                prefs.ui_font_family = family.to_string().into();
            }
        }
        if let Some(family) = value.get("code_font_family").and_then(Value::as_str) {
            if !family.is_empty() {
                prefs.code_font_family = family.to_string().into();
            }
        }
        prefs
    }

    fn persist(&self) {
        let path = Self::persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            path,
            serde_json::json!({
                "ui_font_family": self.ui_font_family.to_string(),
                "code_font_family": self.code_font_family.to_string(),
            })
            .to_string(),
        );
    }
}

/// Current UI face family (readable without `cx`).
pub fn ui_font_family() -> SharedString {
    UI_FONT_FAMILY
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Current code/mono face family (readable without `cx`).
pub fn code_font_family() -> SharedString {
    CODE_FONT_FAMILY
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// A bundled face whose closed settings chip uses a friendlier label than
/// the family gpui resolves. The Appearance pickers list installed families;
/// this catalog only renames a stored family on that chip.
#[derive(Debug, Clone, Copy)]
pub struct FontChoice {
    pub label: &'static str,
    pub family: &'static str,
}

/// Orbit's curated interface faces (plus a System escape hatch).
pub const UI_FONTS: [FontChoice; 10] = [
    FontChoice {
        label: "Inter",
        family: "Inter",
    },
    FontChoice {
        label: "Fixel Text",
        family: "Fixel Text",
    },
    FontChoice {
        label: "Geist Sans",
        family: "Geist",
    },
    FontChoice {
        label: "Atkinson Hyperlegible",
        family: "Atkinson Hyperlegible",
    },
    FontChoice {
        label: "Source Sans 3",
        family: "Source Sans 3",
    },
    FontChoice {
        label: "Roboto",
        family: "Roboto",
    },
    FontChoice {
        label: "Noto Sans",
        family: "Noto Sans",
    },
    FontChoice {
        label: "DM Sans",
        family: "DM Sans",
    },
    FontChoice {
        label: "Manrope",
        family: "Manrope",
    },
    FontChoice {
        label: "IBM Plex Sans",
        family: ".ZedSans",
    },
];

/// Orbit's curated code faces (plus a System Mono escape hatch).
pub const CODE_FONTS: [FontChoice; 16] = [
    FontChoice {
        label: "JetBrains Mono",
        family: "JetBrains Mono",
    },
    FontChoice {
        label: "Fira Code",
        family: "Fira Code",
    },
    FontChoice {
        label: "Geist Mono",
        family: "Geist Mono",
    },
    FontChoice {
        label: "Commit Mono",
        family: "CommitMono",
    },
    FontChoice {
        label: "Source Code Pro",
        family: "Source Code Pro",
    },
    FontChoice {
        label: "Cascadia Code",
        family: "Cascadia Code",
    },
    FontChoice {
        label: "Roboto Mono",
        family: "Roboto Mono",
    },
    FontChoice {
        label: "Iosevka",
        family: "Iosevka",
    },
    FontChoice {
        label: "DM Mono",
        family: "DM Mono",
    },
    FontChoice {
        label: "IBM Plex Mono",
        family: "IBM Plex Mono",
    },
    FontChoice {
        label: "Inconsolata",
        family: "Inconsolata",
    },
    FontChoice {
        label: "Noto Sans Mono",
        family: "Noto Sans Mono",
    },
    FontChoice {
        label: "Space Mono",
        family: "Space Mono",
    },
    FontChoice {
        label: "Anonymous Pro",
        family: "Anonymous Pro",
    },
    FontChoice {
        label: "Martian Mono",
        family: "Martian Mono",
    },
    FontChoice {
        label: "Lilex",
        family: ".ZedMono",
    },
];

/// Display label for a stored family name: a curated label when one exists,
/// otherwise the raw family (so legacy/installed choices still read right).
pub fn font_choice_label(family: &str) -> String {
    if family == ".ZedSans" {
        return "IBM Plex Sans".into();
    }
    if family == ".ZedMono" {
        return "Lilex".into();
    }
    for choice in UI_FONTS.iter().chain(CODE_FONTS.iter()) {
        if choice.family == family {
            return choice.label.into();
        }
    }
    family.to_string()
}

/// Installed families for the Appearance font pickers, cached for the
/// process. Fonts do not change mid-run, and the settings page rebuilds on
/// every scroll tick, so the operating-system enumeration runs once.
static SYSTEM_FONT_FAMILIES: OnceLock<Vec<SharedString>> = OnceLock::new();

/// Families gpui can resolve on this machine: operating-system faces plus
/// the bundled faces registered at startup. Dot-prefixed internal names
/// (`.SystemUIFont`, `.ZedSans`, `.ZedMono`) are omitted; the real bundled
/// families (`IBM Plex Sans`, `Lilex`) remain.
pub fn system_font_families(cx: &App) -> &[SharedString] {
    SYSTEM_FONT_FAMILIES.get_or_init(|| {
        normalize_installed_fonts(cx.text_system().all_font_names())
            .into_iter()
            .map(SharedString::from)
            .collect()
    })
}

/// Drop empty and dot-prefixed names, then sort and dedupe without regard
/// to ASCII case. A case-insensitive tie keeps the spelling that sorts
/// first (`Arial` before `arial`), so a bundled face and the same system
/// face collapse to one row.
fn normalize_installed_fonts(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut keyed: Vec<(String, String)> = names
        .into_iter()
        .filter_map(|name| {
            let name = name.trim();
            if name.is_empty() || name.starts_with('.') {
                None
            } else {
                Some((name.to_lowercase(), name.to_string()))
            }
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    keyed.dedup_by(|next, kept| next.0 == kept.0);
    keyed.into_iter().map(|(_, name)| name).collect()
}

/// Whether a picker row is the stored family. A row matches the raw name,
/// a Zed alias (`IBM Plex Sans` for `.ZedSans`, `Lilex` for `.ZedMono`), or
/// a curated chip label (`Geist Sans` for `Geist`).
pub fn font_family_matches(stored: &str, listed: &str) -> bool {
    stored.eq_ignore_ascii_case(listed) || font_choice_label(stored).eq_ignore_ascii_case(listed)
}

/// Current font families as a struct (for the settings UI).
pub fn font_prefs() -> FontPrefs {
    FontPrefs {
        ui_font_family: ui_font_family(),
        code_font_family: code_font_family(),
    }
}

impl Theme {
    pub fn for_id(id: ThemeKey) -> Self {
        registry::theme(id, UiPrefs::default()).unwrap_or_else(|| {
            Self::from_resolved(
                registry::record(ThemeKey::ORBIT_AZURE).expect("bundled azure theme"),
                UiPrefs::default(),
            )
        })
    }

    #[cfg(test)]
    pub fn dark() -> Self {
        Self::for_id(ThemeKey::ORBIT_AZURE)
    }

    #[cfg(test)]
    pub fn light() -> Self {
        Self::for_id(ThemeKey::ORBIT_AZURE_LIGHT)
    }

    /// Keep the current UI customization across a palette switch.
    pub fn with_ui(mut self, ui: UiPrefs) -> Self {
        self.ui = ui;
        self
    }

    /// Scale an interface text size by the UI font size setting, the way
    /// rems resolve against the UI font size (`ui_font_size /
    /// DEFAULT_UI_FONT_SIZE`). Chrome sizes are authored at the 14px default,
    /// so at the default setting this resolves to the authored pixel value.
    pub fn ui_px(&self, value: f32) -> Pixels {
        px(value * (self.ui.ui_font_size / DEFAULT_UI_FONT_SIZE))
    }

    /// Scale an editor-surface size (code blocks, diffs) by the Editor font
    /// size setting (`editor_font_size / 13`, the default).
    pub fn code_px(&self, value: f32) -> Pixels {
        px(value * (self.ui.editor_font_size / 13.))
    }

    /// Scale a terminal/tool-output size by the Terminal font size setting
    /// (`terminal_font_size / 13`, the default). The terminal panel is the
    /// only surface that uses this; conversation asides use [`Self::aside_px`].
    pub fn term_px(&self, value: f32) -> Pixels {
        px(value * (self.ui.terminal_font_size / 13.))
    }

    /// Composer, user bubble, and assistant body. Authored on the type scale
    /// and multiplied by the UI font size.
    pub fn body_px(&self) -> Pixels {
        self.ui_px(self.type_scale.body)
    }

    pub fn body_line_px(&self) -> Pixels {
        self.ui_px(self.type_scale.body_line)
    }

    /// Heading size and line height for a markdown level. Levels below 4 use
    /// the h4 step.
    pub fn heading_px(&self, level: u8) -> (Pixels, Pixels) {
        let (size, line) = match level {
            1 => (self.type_scale.h1, self.type_scale.h1_line),
            2 => (self.type_scale.h2, self.type_scale.h2_line),
            3 => (self.type_scale.h3, self.type_scale.h3_line),
            _ => (self.type_scale.h4, self.type_scale.h4_line),
        };
        (self.ui_px(size), self.ui_px(line))
    }

    /// Code fences. The authored code size is scaled by Editor Size, so the
    /// default 13px step equals the editor font size.
    pub fn code_block_px(&self) -> Pixels {
        self.code_px(self.type_scale.code)
    }

    pub fn code_block_line_px(&self) -> Pixels {
        self.code_px(self.type_scale.code_line)
    }

    /// Thinking and tool output. Scaled by the UI font size, not Editor Size
    /// or Terminal Size.
    pub fn aside_px(&self) -> Pixels {
        self.ui_px(self.type_scale.aside)
    }

    pub fn aside_line_px(&self) -> Pixels {
        self.ui_px(self.type_scale.aside_line)
    }

    /// Paint color for one syntax token, shared by transcript code blocks
    /// and the review diff so both read as the same editor. Each palette
    /// carries its own `syn_*` colors (so a ported editor theme keeps its
    /// syntax identity); comments are explicitly legible on the code wash.
    pub fn token_color(self, class: TokenClass) -> Hsla {
        match class {
            TokenClass::Keyword => self.syn_keyword,
            TokenClass::Literal => self.syn_literal,
            TokenClass::String => self.syn_string,
            TokenClass::Comment => self.syn_comment,
            TokenClass::Number => self.syn_number,
            TokenClass::Type => self.syn_type,
            TokenClass::Function => self.syn_function,
            TokenClass::Meta => self.syn_meta,
            // Terminal vocabulary: command, flag, path, control operator.
            TokenClass::Command => self.syn_keyword,
            TokenClass::Flag => self.syn_number,
            TokenClass::Path => self.text_primary,
            TokenClass::Operator => self.syn_operator,
            TokenClass::Added => self.success,
            TokenClass::Removed => self.error,
        }
    }

    /// Paint color for a `/command` token in the composer. Commands are
    /// actions, so they take the readable accent foreground — the role
    /// terminal command words and syntax keywords already carry.
    pub fn mention_command(self) -> Hsla {
        self.syn_keyword
    }

    /// Paint color for an `@file` mention in the composer. Files take the
    /// accent's complement, so an `@reference` never reads as a `/command`
    /// while still riding the palette's tuned chroma and lightness.
    pub fn mention_file(self) -> Hsla {
        if self.accent_text.s < 0.15 {
            return self.accent_text.opacity(0.7);
        }
        let mut color = self.accent_text;
        color.h = (color.h + 0.5).fract();
        color.s = color.s.max(0.5);
        color
    }

    /// Build a full [`Theme`] from a resolved registry record.
    pub(crate) fn from_resolved(record: registry::ThemeRecord, ui: UiPrefs) -> Self {
        let p = record.colors;
        let mode = record.appearance;
        let text = hex(p.text);
        let accent = hex(p.accent);
        let assistant = hex(p.assistant_text);
        let (wash, wash_strong, border_strong, contact) = match mode {
            ThemeMode::Dark => (0.05, 0.09, 0.14, 0.32),
            ThemeMode::Light => (0.06, 0.10, 0.16, 0.10),
        };
        let color = |value: Option<u32>, fallback: Hsla| value.map(hex).unwrap_or(fallback);
        let inline_bg = color(record.output.inline_code_bg, accent.opacity(0.10));
        Self {
            theme_id: record.key,
            mode,
            chrome: record.chrome,
            ui,
            type_scale: record.scale,
            background: hex(p.background),
            surface: hex(p.surface),
            surface_secondary: hex(p.surface_secondary),
            surface_elevated: hex(p.elevated),
            surface_hover: hex(p.hover),
            surface_active: hex(p.active),
            border: hex(p.border),
            border_subtle: hex(p.border_subtle),
            text_primary: text,
            text_secondary: hex(p.text_secondary),
            text_muted: hex(p.text_muted),
            text_disabled: hex(p.text_disabled),
            accent,
            accent_hover: hex(p.accent_hover),
            accent_pressed: hex(p.accent_pressed),
            accent_fg: hex(p.accent_fg),
            accent_text: hex(p.accent_text),
            surface_selected: hex(p.selected_surface),
            selection: hex(p.selection),
            selection_fg: hex(p.selection_fg),
            focus_ring: hex(p.focus_ring),
            action: hex(p.action),
            action_hover: hex(p.action_hover),
            action_pressed: hex(p.action_pressed),
            action_fg: hex(p.action_fg),
            success: hex(p.success),
            warning: hex(p.warning),
            error: hex(p.error),
            error_hover: hex(p.error_hover),
            error_fg: hex(p.error_fg),
            info: hex(p.info),
            toggle_knob: match mode {
                ThemeMode::Light => hex(0xFFFFFF),
                ThemeMode::Dark => text,
            },
            overlay: text.opacity(wash),
            overlay_strong: text.opacity(wash_strong),
            border_strong: text.opacity(border_strong),
            assistant_text: hex(p.assistant_text),
            code_bg: hex(p.code_bg),
            code_text: hex(p.code_text),
            syn_string: hex(p.syn_string),
            syn_number: hex(p.syn_number),
            syn_function: hex(p.syn_function),
            syn_type: hex(p.syn_type),
            syn_comment: hex(p.syn_comment),
            syn_literal: hex(p.syn_literal),
            syn_meta: hex(p.syn_meta),
            syn_operator: hex(p.syn_operator),
            inline_code_bg: inline_bg,
            output_inline_fg: record.output.inline_code_fg.map(hex),
            output_prose: color(record.output.prose, assistant),
            output_heading: color(record.output.heading, text),
            output_link: color(record.output.link, hex(p.accent_text)),
            output_code_bg: color(record.output.code_bg, hex(p.code_bg)),
            output_code_fg: color(record.output.code_fg, hex(p.code_text)),
            output_code_header_bg: color(record.output.code_header_bg, text.opacity(wash)),
            output_code_header_fg: color(record.output.code_header_fg, hex(p.text_muted)),
            output_user_bg: color(record.output.user_bubble_bg, hex(p.elevated)),
            output_user_fg: color(record.output.user_bubble_fg, text),
            output_quote_bg: color(record.output.quote_bg, text.opacity(wash)),
            output_quote_border: color(record.output.quote_border, hex(p.text_muted)),
            output_quote_fg: color(record.output.quote_fg, hex(p.text_secondary)),
            syn_keyword: color(record.output.keyword, hex(p.accent_text)),
            shadow_contact: hsla(0., 0., 0., contact),
        }
    }

    /// Soft lift for the floating composer: one tight contact layer. The
    /// hairline border carries definition, so no wide ambient here (a
    /// border under a broad shadow reads as a ghost card).
    pub fn composer_shadow(self) -> Vec<BoxShadow> {
        vec![BoxShadow {
            color: self.shadow_contact,
            offset: point(px(0.), px(2.)),
            blur_radius: px(10.),
            spread_radius: px(-3.),
        }]
    }

    /// Backdrop dim under a floating search surface (command palette):
    /// present enough to separate the card from the page, light enough that
    /// the workspace still reads through.
    pub fn scrim(self) -> Hsla {
        match self.mode {
            ThemeMode::Dark => hsla(0., 0., 0., 0.26),
            ThemeMode::Light => hsla(0., 0., 0., 0.14),
        }
    }

    /// Backdrop dim under a blocking modal (extension dialogs, custom-UI
    /// surfaces, the update prompt): a step stronger than [`Self::scrim`]
    /// because a modal *must* read as the only live layer.
    pub fn scrim_modal(self) -> Hsla {
        match self.mode {
            ThemeMode::Dark => hsla(0., 0., 0., 0.32),
            ThemeMode::Light => hsla(0., 0., 0., 0.18),
        }
    }

    /// Backdrop dim under the image lightbox: near-opaque, so a viewed
    /// attachment is what the eye lands on, not the transcript behind it.
    pub fn scrim_media(self) -> Hsla {
        match self.mode {
            ThemeMode::Dark => hsla(0., 0., 0., 0.72),
            ThemeMode::Light => hsla(0., 0., 0., 0.6),
        }
    }
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir().join(".orbit-pi").join("theme")
}

/// Install the persisted appearance (System by default) and font preferences.
pub fn init(cx: &mut App) {
    load_install_themes();
    let prefs = AppearancePrefs::load();
    cx.set_global(prefs);
    cx.set_global(Theme::for_id(prefs.resolve(cx.window_appearance())).with_ui(UiPrefs::load()));
    let prefs = FontPrefs::load();
    *UI_FONT_FAMILY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = prefs.ui_font_family;
    *CODE_FONT_FAMILY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = prefs.code_font_family;
}

/// Current theme. Panics if [`init`] has not run.
pub fn get(cx: &App) -> &Theme {
    cx.global::<Theme>()
}

/// Whether the user asked to reduce motion. Looping animations should render
/// static when this is true.
pub fn reduce_motion(cx: &App) -> bool {
    get(cx).ui.reduce_motion
}

/// Apply new UI / code font families to the statics and persist them.
pub fn set_font_prefs(prefs: FontPrefs) {
    *UI_FONT_FAMILY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = prefs.ui_font_family.clone();
    *CODE_FONT_FAMILY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = prefs.code_font_family.clone();
    prefs.persist();
}

/// Apply new UI customization (language / font sizes), persist it, and
/// notify global observers — every paint site reads sizes through the
/// theme, so the change is live.
pub fn set_ui_prefs(cx: &mut App, ui: UiPrefs) {
    let previous = get(cx).ui;
    if previous == ui {
        return;
    }
    // Adopt the new locale before the theme global changes so the repaint
    // driven by that change already reads the right strings, then rebuild
    // the native menu bar (which only GPUI can refresh).
    crate::i18n::set_language(ui.language);
    crate::set_app_menus(cx);
    ui.persist();
    let id = get(cx).theme_id;
    cx.set_global(Theme::for_id(id).with_ui(ui));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_font_names_drop_hidden_families_and_case_duplicates() {
        let names = normalize_installed_fonts([
            ".SystemUIFont".into(),
            ".ZedSans".into(),
            ".ZedMono".into(),
            "IBM Plex Sans".into(),
            "arial".into(),
            "Arial".into(),
            "  ".into(),
            String::new(),
            "Lilex".into(),
            "  Cascadia Mono  ".into(),
        ]);
        assert_eq!(
            names,
            vec![
                "Arial".to_string(),
                "Cascadia Mono".to_string(),
                "IBM Plex Sans".to_string(),
                "Lilex".to_string(),
            ]
        );
    }

    #[test]
    fn font_family_match_accepts_zed_aliases_and_curated_labels() {
        assert!(font_family_matches(".ZedSans", "IBM Plex Sans"));
        assert!(font_family_matches(".ZedMono", "Lilex"));
        assert!(font_family_matches("Geist", "Geist"));
        assert!(font_family_matches("Geist", "Geist Sans"));
        assert!(font_family_matches("CommitMono", "Commit Mono"));
        assert!(!font_family_matches(".ZedSans", "Inter"));
        assert!(!font_family_matches("Arial", "Lilex"));
    }

    #[test]
    fn chrome_follows_the_theme() {
        assert_eq!(Theme::for_id(ThemeKey::OPERATIONAL).chrome, Chrome::Flat);
        assert_eq!(
            Theme::for_id(ThemeKey::OPERATIONAL_LIGHT).chrome,
            Chrome::Flat
        );
        assert_eq!(Theme::for_id(ThemeKey::ORBIT).chrome, Chrome::Classic);
        assert_eq!(Theme::for_id(ThemeKey::VSCODE_DARK).chrome, Chrome::Flat);
        assert_eq!(Theme::for_id(ThemeKey::VSCODE_LIGHT).chrome, Chrome::Flat);
    }

    #[test]
    fn parse_theme_id() {
        assert_eq!(ThemeKey::parse("orbit"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("orbit-light"), Some(ThemeKey::ORBIT_LIGHT));
        assert_eq!(ThemeKey::parse("system"), None);
        assert_eq!(ThemeKey::FLEXOKI.as_str(), "flexoki");
        assert_eq!(ThemeKey::FLEXOKI.appearance(), ThemeMode::Dark);
        // Legacy theme names map onto Orbit / Orbit Light.
        assert_eq!(ThemeKey::parse("dark"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("light"), Some(ThemeKey::ORBIT_LIGHT));
        assert_eq!(ThemeKey::parse("one-dark"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("zedokai"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("flexoki-dark"), Some(ThemeKey::FLEXOKI));
        assert_eq!(ThemeKey::parse("flexoki"), Some(ThemeKey::FLEXOKI));
        assert_eq!(
            ThemeKey::parse("flexoki-light"),
            Some(ThemeKey::FLEXOKI_LIGHT)
        );
        // Orbit-family keys.
        assert_eq!(ThemeKey::parse("orbit-paper"), Some(ThemeKey::ORBIT_PAPER));
        assert_eq!(
            ThemeKey::parse("orbit-contrast"),
            Some(ThemeKey::ORBIT_CONTRAST)
        );
        assert_eq!(
            ThemeKey::parse("orbit-darker"),
            Some(ThemeKey::ORBIT_DARKER)
        );
        assert_eq!(ThemeKey::ORBIT_DARKER.as_str(), "orbit-darker");
        assert_eq!(ThemeKey::ORBIT_DARKER.appearance(), ThemeMode::Dark);
        assert_eq!(
            ThemeKey::parse("orbit-cobalt"),
            Some(ThemeKey::ORBIT_COBALT)
        );
        // The pre-rename key and the palette's origin still load.
        assert_eq!(
            ThemeKey::parse("orbit-minimal"),
            Some(ThemeKey::ORBIT_COBALT)
        );
        assert_eq!(ThemeKey::parse("codex"), Some(ThemeKey::ORBIT_COBALT));
        assert_eq!(ThemeKey::ORBIT_COBALT.as_str(), "orbit-cobalt");
        assert_eq!(ThemeKey::parse("operational"), Some(ThemeKey::OPERATIONAL));
        assert_eq!(
            ThemeKey::parse("operational-dark"),
            Some(ThemeKey::OPERATIONAL)
        );
        assert_eq!(
            ThemeKey::parse("operational-light"),
            Some(ThemeKey::OPERATIONAL_LIGHT)
        );
        assert_eq!(ThemeKey::OPERATIONAL.as_str(), "operational");
        assert_eq!(ThemeKey::OPERATIONAL_LIGHT.as_str(), "operational-light");
        // Retired editor themes still load: dark onto Orbit, light onto Orbit
        // Light.
        assert_eq!(ThemeKey::parse("dracula"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("nord"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("catppuccin-mocha"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("one-light"), Some(ThemeKey::ORBIT_LIGHT));
        assert_eq!(
            ThemeKey::parse("nord-snow-storm"),
            Some(ThemeKey::ORBIT_LIGHT)
        );
        assert_eq!(
            ThemeKey::parse("tokyo-night-day"),
            Some(ThemeKey::ORBIT_LIGHT)
        );
        // Every selectable theme round-trips through its persisted key.
        for id in bundled_keys() {
            assert_eq!(ThemeKey::parse(id.as_str()), Some(id));
        }
    }

    /// Composer mention tokens stay legible and distinct from each other in
    /// every palette, including the monochrome ones (Ashwood, Mono) where the
    /// split is by ink rather than hue.
    #[test]
    fn mention_token_colors_are_distinct_across_palettes() {
        for id in bundled_keys() {
            let theme = Theme::for_id(id);
            let command = theme.mention_command();
            let file = theme.mention_file();
            assert!(
                command != file,
                "mention colors collide in {id:?}: {command:?}"
            );
            assert!(command.a > 0., "command token is transparent in {id:?}");
            assert!(file.a > 0., "file token is transparent in {id:?}");
        }
    }

    /// The toggle knob must read on both the accent (on) and the raised track
    /// (off) in every palette. Light mode used `text` — a near-black dot on
    /// the pale track — so the knob is its own light token there.
    #[test]
    fn toggle_knob_stays_light_on_light_tracks() {
        for id in bundled_keys() {
            let theme = Theme::for_id(id);
            match theme.mode {
                ThemeMode::Light => assert!(
                    theme.toggle_knob.l > theme.surface_elevated.l,
                    "toggle knob is not lighter than the off track in {id:?}"
                ),
                ThemeMode::Dark => assert_eq!(
                    theme.toggle_knob, theme.text_primary,
                    "dark toggle knob drifted in {id:?}"
                ),
            }
        }
    }

    /// Every shipped palette is selectable under a unique name. The list
    /// itself comes from `themes/*.json`, so a new file must not require
    /// this test to grow.
    #[test]
    fn theme_catalog_labels() {
        let keys = bundled_keys();
        assert!(!keys.is_empty());
        let mut labels: Vec<&str> = keys.iter().map(|id| id.label()).collect();
        assert!(labels.iter().all(|label| !label.is_empty()));
        let before = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), before, "theme names must be unique");
        assert!(labels.contains(&"Orbit"));
    }

    fn rel_lum(v: u32) -> f64 {
        let f = |c: u32| {
            let c = c as f64 / 255.;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f((v >> 16) & 0xff) + 0.7152 * f((v >> 8) & 0xff) + 0.0722 * f(v & 0xff)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (l1, l2) = (rel_lum(a), rel_lum(b));
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// A built theme's ink against a built theme's surface. The palette tests
    /// below use the raw hex `Palette`; a color *derived* at build time (the
    /// sidebar's accent-leaning project ink) can only be checked on the resolved
    /// `Theme`.
    fn theme_contrast(ink: Hsla, surface: Hsla) -> f64 {
        let lum = |color: Hsla| {
            let to_lin = |v: f32| -> f64 {
                let v = v.clamp(0.0, 1.0) as f64;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            let rgba = gpui::Rgba::from(color);
            0.2126 * to_lin(rgba.r) + 0.7152 * to_lin(rgba.g) + 0.0722 * to_lin(rgba.b)
        };
        let (l1, l2) = (lum(ink), lum(surface));
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The sidebar's project ink is derived, not authored: primary ink leaned
    /// toward the accent. Both inputs clear AA, and a shortest-arc blend of two
    /// inks lands between them, so the derived name ink must clear AA too — on
    /// the sidebar surface and on the hover pill it is painted over.
    #[test]
    fn the_project_name_ink_stays_readable_in_every_palette() {
        for id in bundled_keys() {
            let theme = Theme::for_id(id);
            let ink = crate::workspace_mark::default_label_ink(&theme);
            // Untinted is the common case; a tinted project is the user's own
            // choice and is checked against the same floor.
            for (label, ink) in [
                ("untinted", ink),
                (
                    "tinted",
                    crate::workspace_mark::label_ink(&theme, Some("accent")),
                ),
            ] {
                for (surface_label, surface) in
                    [("sidebar", theme.surface), ("hover", theme.surface_hover)]
                {
                    let ratio = theme_contrast(ink, surface);
                    assert!(
                        ratio >= 4.5,
                        "{id:?}: {label} project name on {surface_label} is {ratio:.2}:1"
                    );
                }
            }
        }
    }

    fn palette(id: ThemeKey) -> registry::Palette {
        registry::raw_palette(id).unwrap_or_else(|| panic!("{id:?} is not bundled"))
    }

    /// Every palette must keep its foreground readable against the surfaces
    /// it is painted on. The bars are the delivered floor: ink clears AA on
    /// the canvas, the accent stays legible as text (`accent_text`, the role
    /// links and inline code read) rather than only as a wash, and every state
    /// pair (selected row, primary button) holds at 4.5:1.
    #[test]
    fn every_theme_has_readable_foregrounds() {
        for id in bundled_keys() {
            let p = palette(id);
            let c = |a, b| contrast(a, b);
            assert!(
                c(p.text, p.background) >= 7.0,
                "{id:?}: body text on canvas"
            );
            assert!(c(p.text, p.elevated) >= 4.5, "{id:?}: body text on raised");
            assert!(
                c(p.text_secondary, p.background) >= 4.0,
                "{id:?}: secondary text"
            );
            assert!(
                c(p.text_muted, p.background) >= 3.0,
                "{id:?}: tertiary/label text"
            );
            assert!(
                c(p.accent_text, p.background) >= 4.5,
                "{id:?}: accent as text"
            );
            assert!(
                c(p.selection_fg, p.selection) >= 4.5,
                "{id:?}: selected row"
            );
            assert!(c(p.action_fg, p.action) >= 4.5, "{id:?}: primary button");
            assert!(c(p.accent_fg, p.accent) >= 4.5, "{id:?}: accent fill");
            assert!(
                c(p.border, p.background) >= 1.15,
                "{id:?}: hairline vs canvas"
            );
            // Syntax must stay legible on the code wash, comments included.
            for (label, token) in [
                ("string", p.syn_string),
                ("number", p.syn_number),
                ("function", p.syn_function),
                ("type", p.syn_type),
                ("literal", p.syn_literal),
                ("meta", p.syn_meta),
                ("operator", p.syn_operator),
            ] {
                assert!(
                    c(token, p.code_bg) >= 3.5,
                    "{id:?}: syntax {label} on code wash"
                );
            }
            assert!(
                c(p.syn_comment, p.code_bg) >= 3.0,
                "{id:?}: comment on code wash"
            );
            // Status and diff colors are painted on the dark work surfaces as
            // badge dots and +/− counts. AA-large is the floor there; labels
            // ride ink, so the hue is reinforcement, never the only signal.
            if id.appearance() == ThemeMode::Dark {
                for (label, token) in [
                    ("success", p.success),
                    ("danger", p.error),
                    ("caution", p.warning),
                ] {
                    assert!(
                        c(token, p.background) >= 3.0,
                        "{id:?}: status {label} on canvas"
                    );
                    assert!(
                        c(token, p.elevated) >= 2.5,
                        "{id:?}: status {label} on raised"
                    );
                }
            }
            // A run's failed/cancelled badge and a deleted diff line both use
            // red; the badge's dot + label is what separates them, never the
            // hue alone. Guard the one collision that would erase that:
            // `error` (failure) must not equal `text_muted` (cancel).
            assert_ne!(
                p.error, p.text_muted,
                "{id:?}: failure red collides with the muted cancel ink"
            );
        }
    }

    /// The two default appearances ship the agreed neutral/blue design
    /// tokens. `text_muted` and the primary hover step are the two values
    /// adjusted from the first draft: the draft muted gray failed AA on
    /// elevated cards, and the draft hover blue failed AA with a white label.
    #[test]
    fn default_palettes_ship_the_agreed_tokens() {
        let dark = Theme::dark();
        let light = Theme::light();
        let expected_dark = [
            ("background", dark.background, 0x181818),
            ("surface", dark.surface, 0x1F1F1F),
            ("surface_secondary", dark.surface_secondary, 0x242424),
            ("surface_elevated", dark.surface_elevated, 0x292929),
            ("surface_hover", dark.surface_hover, 0x303030),
            ("surface_active", dark.surface_active, 0x363636),
            ("border", dark.border, 0x343434),
            ("border_subtle", dark.border_subtle, 0x292929),
            ("text_primary", dark.text_primary, 0xE5E5E5),
            ("text_secondary", dark.text_secondary, 0xA3A3A3),
            ("text_muted", dark.text_muted, 0x909090),
            ("text_disabled", dark.text_disabled, 0x606060),
            ("accent", dark.accent, 0x286CF0),
            ("accent_hover", dark.accent_hover, 0x2D6FEF),
            ("accent_pressed", dark.accent_pressed, 0x1F5DD8),
            ("selection", dark.selection, 0x2F3949),
            ("focus_ring", dark.focus_ring, 0x6EA0FF),
            ("success", dark.success, 0x4CAF86),
            ("warning", dark.warning, 0xE5AD55),
            ("error", dark.error, 0xE47478),
            ("info", dark.info, 0x65A4E8),
        ];
        for (label, actual, expected) in expected_dark {
            assert_eq!(actual, hex(expected), "dark {label}");
        }
        let expected_light = [
            ("background", light.background, 0xF5F5F5),
            ("surface", light.surface, 0xFFFFFF),
            ("surface_secondary", light.surface_secondary, 0xF8F8F8),
            ("surface_elevated", light.surface_elevated, 0xF0F0F0),
            ("surface_hover", light.surface_hover, 0xEBEBEB),
            ("surface_active", light.surface_active, 0xE2E2E2),
            ("border", light.border, 0xE5E5E5),
            ("border_subtle", light.border_subtle, 0xEEEEEE),
            ("text_primary", light.text_primary, 0x171717),
            ("text_secondary", light.text_secondary, 0x525252),
            ("text_muted", light.text_muted, 0x696969),
            ("text_disabled", light.text_disabled, 0xA3A3A3),
            ("accent", light.accent, 0x286CF0),
            ("accent_hover", light.accent_hover, 0x1F5DD8),
            ("accent_pressed", light.accent_pressed, 0x194BB5),
            ("selection", light.selection, 0xE8EEFA),
            ("focus_ring", light.focus_ring, 0x286CF0),
            ("success", light.success, 0x247A56),
            ("warning", light.warning, 0x94600D),
            ("error", light.error, 0xC03940),
            ("info", light.info, 0x2864A8),
        ];
        for (label, actual, expected) in expected_light {
            assert_eq!(actual, hex(expected), "light {label}");
        }
        // The canvas is charcoal / warm-neutral, never pure black or white.
        assert_ne!(dark.background, hex(0x000000));
        assert_ne!(light.background, hex(0xFFFFFF));
        assert_ne!(dark.text_primary, light.text_primary);
    }

    /// Every palette resolves the complete semantic vocabulary. A missing or
    /// transparent role is a defect in the palette, not in the caller.
    #[test]
    fn every_palette_exposes_the_semantic_vocabulary() {
        for id in bundled_keys() {
            let t = Theme::for_id(id);
            let colors = [
                ("background", t.background),
                ("surface", t.surface),
                ("surface_secondary", t.surface_secondary),
                ("surface_elevated", t.surface_elevated),
                ("surface_hover", t.surface_hover),
                ("surface_active", t.surface_active),
                ("border", t.border),
                ("border_subtle", t.border_subtle),
                ("surface_selected", t.surface_selected),
                ("text_primary", t.text_primary),
                ("text_secondary", t.text_secondary),
                ("text_muted", t.text_muted),
                ("text_disabled", t.text_disabled),
                ("accent", t.accent),
                ("accent_hover", t.accent_hover),
                ("accent_pressed", t.accent_pressed),
                ("accent_fg", t.accent_fg),
                ("accent_text", t.accent_text),
                ("selection", t.selection),
                ("selection_fg", t.selection_fg),
                ("focus_ring", t.focus_ring),
                ("action", t.action),
                ("action_hover", t.action_hover),
                ("action_pressed", t.action_pressed),
                ("action_fg", t.action_fg),
                ("success", t.success),
                ("warning", t.warning),
                ("error", t.error),
                ("error_hover", t.error_hover),
                ("error_fg", t.error_fg),
                ("info", t.info),
            ];
            for (name, color) in colors {
                assert!(color.a > 0., "{id:?}: {name} is transparent");
            }
            assert_ne!(t.selection, t.surface_active, "{id:?}: selection role");
            assert_ne!(t.text_disabled, t.text_muted, "{id:?}: disabled role");
            assert_ne!(t.accent_text, t.text_primary, "{id:?}: accent text role");
        }
    }

    /// Muted metadata is painted on cards, not only on the canvas; it must
    /// still clear AA there in every palette. The accent foreground and all
    /// three primary-button states carry the same floor.
    #[test]
    fn interactive_and_muted_foregrounds_clear_aa() {
        for id in bundled_keys() {
            let p = palette(id);
            let c = |a, b| contrast(a, b);
            assert!(
                c(p.text_muted, p.elevated) >= 4.5,
                "{id:?}: muted text on elevated surface"
            );
            assert!(
                c(p.accent_text, p.background) >= 4.5,
                "{id:?}: accent text on canvas"
            );
            assert!(
                c(p.accent_text, p.elevated) >= 4.5,
                "{id:?}: accent text on elevated surface"
            );
            assert!(
                c(p.accent_fg, p.accent) >= 4.5,
                "{id:?}: accent fill foreground"
            );
            assert!(
                c(p.action_fg, p.action) >= 4.5,
                "{id:?}: primary action rest state"
            );
            assert!(
                c(p.action_fg, p.action_hover) >= 4.5,
                "{id:?}: primary action hover state"
            );
            assert!(
                c(p.action_fg, p.action_pressed) >= 4.5,
                "{id:?}: primary action pressed state"
            );
            assert!(
                c(p.error_fg, p.error) >= 4.5,
                "{id:?}: error fill foreground"
            );
            assert!(
                c(p.error_fg, p.error_hover) >= 4.5,
                "{id:?}: error fill hover foreground"
            );
            assert!(
                c(p.focus_ring, p.background) >= 3.0,
                "{id:?}: focus ring on canvas"
            );
            assert!(
                c(p.focus_ring, p.elevated) >= 3.0,
                "{id:?}: focus ring on elevated surface"
            );
            // The primary-action fill is its own role (a palette may key it to
            // a neutral step rather than its accent), so its ink needs its own
            // floor. Hover is transient and not held here.
            assert!(
                c(p.action_fg, p.action) >= 4.5,
                "{id:?}: primary action rest state"
            );
        }
    }

    /// A selected row paints its metadata with `text_secondary`, never
    /// `text_muted`: the selection wash sits between the canvas and a card,
    /// where muted ink drops below AA. The same holds for the accent text on
    /// the selection wash. Guard both so a palette edit cannot silently
    /// reintroduce low-contrast metadata on the active row.
    #[test]
    fn selection_rows_keep_their_metadata_readable() {
        for id in bundled_keys() {
            let p = palette(id);
            let c = |a, b| contrast(a, b);
            for (label, wash) in [
                ("selection", p.selection),
                ("selected row", p.selected_surface),
            ] {
                assert!(
                    c(p.text_secondary, wash) >= 4.5,
                    "{id:?}: secondary text on the {label} wash"
                );
                assert!(
                    c(p.selection_fg, wash) >= 7.0,
                    "{id:?}: primary ink on the {label} wash"
                );
                assert!(
                    c(p.accent_text, wash) >= 4.5,
                    "{id:?}: accent text on the {label} wash"
                );
            }
        }
    }

    /// One surface grammar for every palette: on dark, each step up the ramp is
    /// lighter than the one below (canvas → raised → hover → active); on light
    /// it is the inverse. Menus paint on `surface_elevated`, which `Theme::build`
    /// sets to the palette's raised step by construction, and the primary
    /// button always answers a hover with a visible step.
    #[test]
    fn every_theme_holds_its_surface_grammar() {
        for id in bundled_keys() {
            let p = palette(id);
            let l = |v: u32| rel_lum(v);
            match id.appearance() {
                ThemeMode::Dark => {
                    assert!(
                        l(p.elevated) > l(p.background),
                        "{id:?}: raised must lift off the canvas"
                    );
                    assert!(
                        l(p.elevated) >= l(p.surface),
                        "{id:?}: raised must float above the chrome plane"
                    );
                    assert!(
                        l(p.hover) >= l(p.elevated),
                        "{id:?}: hover must sit above raised"
                    );
                    assert!(
                        l(p.active) >= l(p.hover),
                        "{id:?}: active must sit above hover"
                    );
                }
                ThemeMode::Light if id == ThemeKey::OPERATIONAL_LIGHT => {
                    // Operational light is paper-canvas + white panels: the
                    // raised step is the brightest plane, so it lifts off the
                    // warm canvas instead of recessing into it. The interaction
                    // steps still recess from there.
                    assert!(
                        l(p.elevated) > l(p.background),
                        "{id:?}: raised must lift off the paper canvas"
                    );
                    assert!(
                        l(p.elevated) <= l(p.surface),
                        "{id:?}: the primary panel must stay the brightest plane"
                    );
                    assert!(
                        l(p.hover) <= l(p.elevated),
                        "{id:?}: hover must recess below raised"
                    );
                    assert!(
                        l(p.active) <= l(p.hover),
                        "{id:?}: active must recess below hover"
                    );
                }
                ThemeMode::Light => {
                    assert!(
                        l(p.elevated) < l(p.background),
                        "{id:?}: raised must recess in light mode"
                    );
                    assert!(
                        l(p.elevated) <= l(p.surface),
                        "{id:?}: raised must sit below the light chrome plane"
                    );
                    assert!(
                        l(p.hover) <= l(p.elevated),
                        "{id:?}: hover must recess below raised"
                    );
                    assert!(
                        l(p.active) <= l(p.hover),
                        "{id:?}: active must recess below hover"
                    );
                }
            }
            assert_ne!(
                p.accent, p.accent_hover,
                "{id:?}: primary button hover feedback"
            );
            // The accent is the lone interactive hue; it can never double as
            // a status color or a second accent would read as state.
            assert_ne!(p.accent, p.success, "{id:?}: accent collides with success");
            assert_ne!(p.accent, p.error, "{id:?}: accent collides with error");
            assert_ne!(p.accent, p.warning, "{id:?}: accent collides with warning");
        }
    }

    /// Every palette must carry a real accent hue (the composer derives
    /// `@file` by inverting it), so a grey accent is a defect.
    /// Classic palettes must stay pixel-identical after the row-selection
    /// role split: their selected-row wash is the old `selection` color.
    #[test]
    fn classic_row_selection_matches_the_existing_wash() {
        for id in bundled_keys() {
            if Theme::for_id(id).chrome == Chrome::Classic {
                let p = palette(id);
                assert_eq!(
                    p.selected_surface, p.selection,
                    "{id:?}: Classic row selection drifted"
                );
            }
        }
    }

    #[test]
    fn accent_carries_chroma() {
        for id in bundled_keys() {
            let accent = Theme::for_id(id).accent;
            assert!(
                accent.s > 0.05,
                "{id:?}: accent is nearly achromatic (s = {:.3})",
                accent.s
            );
        }
    }

    #[test]
    fn theme_appearance_matches_label() {
        assert_eq!(ThemeKey::ORBIT.appearance(), ThemeMode::Dark);
        assert_eq!(ThemeKey::ORBIT_LIGHT.appearance(), ThemeMode::Light);
        assert_eq!(ThemeKey::ORBIT_PAPER.appearance(), ThemeMode::Light);
        assert_eq!(ThemeKey::ORBIT_CONTRAST.appearance(), ThemeMode::Light);
        assert_eq!(ThemeKey::ORBIT_DARKER.appearance(), ThemeMode::Dark);
        assert_eq!(ThemeKey::ORBIT_COBALT.appearance(), ThemeMode::Dark);
        assert_eq!(ThemeKey::FLEXOKI_LIGHT.appearance(), ThemeMode::Light);
        assert_eq!(ThemeKey::ORBIT_AZURE.appearance(), ThemeMode::Dark);
        assert_eq!(ThemeKey::ORBIT_AZURE_LIGHT.appearance(), ThemeMode::Light);
        assert_eq!(ThemeKey::OPERATIONAL.appearance(), ThemeMode::Dark);
        assert_eq!(ThemeKey::OPERATIONAL_LIGHT.appearance(), ThemeMode::Light);
        assert_eq!(ThemeKey::ORBIT.label(), "Orbit");
        assert_eq!(ThemeKey::ORBIT_LIGHT.label(), "Orbit Light");
        assert_eq!(ThemeKey::ORBIT_PAPER.label(), "Orbit Paper");
        assert_eq!(ThemeKey::ORBIT_CONTRAST.label(), "Orbit Contrast");
        assert_eq!(ThemeKey::ORBIT_DARKER.label(), "Orbit Darker");
        assert_eq!(ThemeKey::ORBIT_COBALT.label(), "Orbit Cobalt");
        assert_eq!(ThemeKey::FLEXOKI.label(), "Flexoki");
        assert_eq!(ThemeKey::FLEXOKI_LIGHT.label(), "Flexoki Light");
        assert_eq!(ThemeKey::ORBIT_AZURE.label(), "Orbit Azure");
        assert_eq!(ThemeKey::ORBIT_AZURE_LIGHT.label(), "Orbit Azure Light");
        assert_eq!(ThemeKey::OPERATIONAL.label(), "Operational");
        assert_eq!(ThemeKey::OPERATIONAL_LIGHT.label(), "Operational Light");
        assert_eq!(ThemeKey::VSCODE_DARK.label(), "VS Code Dark");
        assert_eq!(ThemeKey::VSCODE_LIGHT.label(), "VS Code Light");
        assert_eq!(ThemeKey::OPERATIONAL.chrome(), Chrome::Flat);
        assert_eq!(ThemeKey::ORBIT.chrome(), Chrome::Classic);
        // The reverted default is the ember identity; Azure is the blue design.
        assert_eq!(
            Theme::for_id(ThemeKey::ORBIT).accent,
            hex(0xE2795B),
            "Orbit keeps its ember accent"
        );
        assert_eq!(
            Theme::for_id(ThemeKey::ORBIT_AZURE).accent,
            hex(0x286CF0),
            "Orbit Azure keeps the blue accent"
        );
    }

    /// Flexoki is ported, so it keeps its own syntax colors rather than
    /// borrowing Orbit's semantic green/amber reuse. If the two ever collapse
    /// onto the same tokens, the theme has lost what makes it Flexoki.
    #[test]
    fn flexoki_keeps_its_own_syntax_palette() {
        let flexoki = Theme::for_id(ThemeKey::FLEXOKI);
        let orbit = Theme::dark();
        assert_eq!(flexoki.mode, ThemeMode::Dark);
        assert_ne!(
            flexoki.token_color(TokenClass::String),
            orbit.token_color(TokenClass::String)
        );
        assert_eq!(flexoki.token_color(TokenClass::String), hex(0x3AA99F));
        assert_eq!(flexoki.token_color(TokenClass::Function), hex(0xDA702C));
    }

    #[test]
    fn parse_language() {
        assert_eq!(Language::parse("system"), Some(Language::System));
        assert_eq!(Language::parse("en"), Some(Language::English));
        assert_eq!(Language::parse("zh-CN"), Some(Language::SimplifiedChinese));
        assert_eq!(Language::parse("fr"), Some(Language::French));
        assert_eq!(Language::parse("xx"), None);
        assert_eq!(Language::English.label(), "English");
    }

    #[test]
    fn ui_prefs_parse_rejects_out_of_range_sizes() {
        use serde_json::json;
        let prefs = UiPrefs::from_value(&json!({
            "language": "en",
            "ui_font_size": 500,
            "terminal_font_size": 15,
            "editor_font_size": 12,
            "spacing_density": 100,
        }));
        // 500px isn't selectable — falls back to the default; the rest apply.
        assert_eq!(prefs.language, Language::English);
        assert_eq!(prefs.ui_font_size, 14.);
        assert_eq!(prefs.terminal_font_size, 15.);
        assert_eq!(prefs.editor_font_size, 12.);
        assert_eq!(prefs.spacing_density, 100);
        // Malformed payload → all defaults.
        assert_eq!(
            UiPrefs::from_value(&json!({"language": 3})),
            UiPrefs::default()
        );
        // Legacy percentage / px keys migrate onto the px scales.
        let legacy = UiPrefs::from_value(&json!({
            "interface_scale": 115,
            "code_font_size": 15,
        }));
        assert_eq!(legacy.ui_font_size, 16.);
        assert_eq!(legacy.editor_font_size, 15.);
    }

    #[test]
    fn composer_send_mode_round_trips_in_ui_prefs() {
        use crate::composer_send::SendMode;
        for mode in [SendMode::FollowUp, SendMode::Steer] {
            let prefs = UiPrefs {
                composer_send_mode: mode,
                language: Language::French,
                ui_font_size: 16.,
                ..UiPrefs::default()
            };
            let json = prefs.to_value().to_string();
            assert_eq!(
                UiPrefs::from_value(&serde_json::from_str(&json).unwrap()),
                prefs
            );
        }
    }

    #[test]
    fn composer_send_mode_missing_or_invalid_keeps_legacy_default() {
        use crate::composer_send::SendMode;
        for value in [
            serde_json::json!({}),
            serde_json::json!({"composer_send_mode": "unknown"}),
            serde_json::json!({"composer_send_mode": 42}),
            serde_json::json!({"composer_send_mode": null}),
        ] {
            assert_eq!(
                UiPrefs::from_value(&value).composer_send_mode,
                SendMode::FollowUp
            );
        }
    }

    #[test]
    fn font_scale_helpers() {
        fn close(a: Pixels, b: Pixels) -> bool {
            (f32::from(a) - f32::from(b)).abs() < 0.01
        }
        let mut theme = Theme::dark();
        // Defaults are unscaled.
        assert_eq!(theme.ui_px(14.), px(14.));
        assert_eq!(theme.code_px(13.), px(13.));
        assert_eq!(theme.term_px(13.), px(13.));
        theme.ui.ui_font_size = 18.;
        theme.ui.editor_font_size = 11.;
        theme.ui.terminal_font_size = 15.;
        theme.ui.spacing_density = 75;
        assert_eq!(theme.ui_px(14.), px(18.));
        assert!(close(theme.code_px(13.), px(11.)));
        assert!(close(theme.term_px(13.), px(15.)));
        // A palette switch carries UI customization over.
        let light = Theme::for_id(ThemeKey::ORBIT_LIGHT).with_ui(theme.ui);
        assert_eq!(light.theme_id, ThemeKey::ORBIT_LIGHT);
        assert_eq!(light.ui, theme.ui);
    }

    #[test]
    fn palettes_differ() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_ne!(dark.background, light.background);
        assert_ne!(dark.text_primary, light.text_primary);
        assert_eq!(dark.theme_id, ThemeKey::ORBIT_AZURE);
        assert_eq!(light.theme_id, ThemeKey::ORBIT_AZURE_LIGHT);
        // Every palette is distinct from the default.
        for id in bundled_keys() {
            assert_eq!(Theme::for_id(id).theme_id, id);
        }
        // Orbit is dark; Orbit Light is light.
        for id in bundled_keys() {
            assert_eq!(Theme::for_id(id).mode, id.appearance());
        }
    }
}
