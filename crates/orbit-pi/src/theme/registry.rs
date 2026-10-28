//! Theme files: palettes compiled into the binary, plus editable copies
//! beside the executable.
//!
//! `cargo run` reads `target/debug/themes` or `target/release/themes`
//! (`current_exe().parent().join("themes")`). Packaged builds copy
//! `crates/orbit-pi/themes` next to the exe (Windows zip and Inno layout,
//! macOS `.app` `Contents/MacOS/themes`). A missing directory is ignored.
//! Invalid JSON skips that file. One bad theme inside a valid file is
//! skipped and its siblings stay. Duplicate ids keep the first theme,
//! except a file beside the executable may replace a built-in id. Duplicate
//! display names, ignoring ASCII case, are rejected. The path is recorded
//! either way, and the themes compiled in from `themes/*.json` stay loaded.
//! Adding a palette is a new JSON file in that directory: the loader picks
//! up every `*.json` there, so a new theme does not need a Rust constant or
//! an `include_str!`.
//! Optional family `"order"` (an integer) sets dropdown order; files without
//! it sort after numbered ones, then by filename. This loader does
//! not read `~/.orbit-pi/themes`. The selected id is still `~/.orbit-pi/theme`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock, RwLock};

use include_dir::{include_dir, Dir};

use super::{Theme, ThemeMode, UiPrefs};

/// Every `themes/*.json` next to this crate. A new file is a new theme.
static BUNDLED_THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/themes");

/// Flat or raised chrome. `flat` is the geometry Operational used to select
/// on its own; `classic` keeps the existing radii, button shadow, and elevation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chrome {
    #[default]
    Classic,
    Flat,
}

impl Chrome {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "classic" => Some(Self::Classic),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }
}

/// Interned theme id. Copy, compared by the id string.
#[derive(Clone, Copy, Debug)]
pub struct ThemeKey(&'static str);

impl PartialEq for ThemeKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for ThemeKey {}

impl std::hash::Hash for ThemeKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl ThemeKey {
    pub const ORBIT: Self = Self("orbit");
    pub const ORBIT_LIGHT: Self = Self("orbit-light");
    pub const ORBIT_PAPER: Self = Self("orbit-paper");
    pub const ORBIT_CONTRAST: Self = Self("orbit-contrast");
    pub const ORBIT_DARKER: Self = Self("orbit-darker");
    pub const ORBIT_COBALT: Self = Self("orbit-cobalt");
    pub const ORBIT_AZURE: Self = Self("orbit-azure");
    pub const ORBIT_AZURE_LIGHT: Self = Self("orbit-azure-light");
    pub const FLEXOKI: Self = Self("flexoki");
    pub const FLEXOKI_LIGHT: Self = Self("flexoki-light");
    pub const OPERATIONAL: Self = Self("operational");
    pub const OPERATIONAL_LIGHT: Self = Self("operational-light");
    pub const VSCODE_DARK: Self = Self("vscode-dark");
    pub const VSCODE_LIGHT: Self = Self("vscode-light");

    pub fn as_str(self) -> &'static str {
        self.0
    }

    /// Intern `id` for the process lifetime. Known literals compare equal to
    /// the constants above because equality is the string, not the pointer.
    pub fn intern(id: &str) -> Self {
        Self(leak(id))
    }

    /// Saved ids, including the legacy aliases that still load.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        let raw = raw.to_ascii_lowercase();
        if let Some(key) = alias(&raw) {
            return Some(key);
        }
        let key = Self::intern(&raw);
        if contains(key) {
            Some(key)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        record(self).map(|record| record.name).unwrap_or(self.0)
    }

    pub fn appearance(self) -> ThemeMode {
        record(self)
            .map(|record| record.appearance)
            .unwrap_or(ThemeMode::Dark)
    }

    pub fn chrome(self) -> Chrome {
        record(self)
            .map(|record| record.chrome)
            .unwrap_or(Chrome::Classic)
    }
}

/// Authored conversation sizes, before the UI or editor font-size setting
/// scales them. Body, headings, and aside ride the UI size. Code rides the
/// editor size. Terminal size is not part of this scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeScale {
    pub body: f32,
    pub body_line: f32,
    pub h1: f32,
    pub h1_line: f32,
    pub h2: f32,
    pub h2_line: f32,
    pub h3: f32,
    pub h3_line: f32,
    pub h4: f32,
    pub h4_line: f32,
    pub code: f32,
    pub code_line: f32,
    pub aside: f32,
    pub aside_line: f32,
}

impl Default for TypeScale {
    fn default() -> Self {
        Self {
            body: 14.,
            body_line: 24.,
            h1: 18.,
            h1_line: 26.,
            h2: 16.,
            h2_line: 24.,
            h3: 15.,
            h3_line: 22.,
            h4: 14.,
            h4_line: 22.,
            code: 13.,
            code_line: 20.,
            aside: 12.,
            aside_line: 18.,
        }
    }
}

impl TypeScale {
    fn apply(&mut self, key: &str, value: f32) -> bool {
        let slot = match key {
            "body" => &mut self.body,
            "bodyLine" => &mut self.body_line,
            "h1" => &mut self.h1,
            "h1Line" => &mut self.h1_line,
            "h2" => &mut self.h2,
            "h2Line" => &mut self.h2_line,
            "h3" => &mut self.h3,
            "h3Line" => &mut self.h3_line,
            "h4" => &mut self.h4,
            "h4Line" => &mut self.h4_line,
            "code" => &mut self.code,
            "codeLine" => &mut self.code_line,
            "aside" => &mut self.aside,
            "asideLine" => &mut self.aside_line,
            _ => return false,
        };
        *slot = value;
        true
    }
}

macro_rules! palette_fields {
    ($($field:ident => $json:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Palette {
            $(pub $field: u32,)*
        }

        #[derive(Clone, Copy, Debug, Default)]
        struct PartialColors {
            $($field: Option<u32>,)*
        }

        impl PartialColors {
            fn set(&mut self, key: &str, value: u32) -> bool {
                match key {
                    $($json => {
                        self.$field = Some(value);
                        true
                    })*
                    _ => false,
                }
            }

            fn fill(self, base: Palette) -> Palette {
                Palette {
                    $($field: self.$field.unwrap_or(base.$field),)*
                }
            }
        }
    };
}

palette_fields! {
    background => "background",
    surface => "surface",
    surface_secondary => "surfaceSecondary",
    elevated => "elevated",
    hover => "hover",
    active => "active",
    selection => "selection",
    selected_surface => "selectedSurface",
    selection_fg => "selectionFg",
    border => "border",
    border_subtle => "borderSubtle",
    text => "text",
    text_secondary => "textSecondary",
    text_muted => "textMuted",
    text_disabled => "textDisabled",
    success => "success",
    error => "error",
    error_hover => "errorHover",
    error_fg => "errorFg",
    accent => "accent",
    accent_hover => "accentHover",
    accent_pressed => "accentPressed",
    accent_fg => "accentFg",
    accent_text => "accentText",
    focus_ring => "focusRing",
    action => "action",
    action_hover => "actionHover",
    action_pressed => "actionPressed",
    action_fg => "actionFg",
    warning => "warning",
    info => "info",
    assistant_text => "assistantText",
    code_bg => "codeBg",
    code_text => "codeText",
    syn_string => "synString",
    syn_number => "synNumber",
    syn_function => "synFunction",
    syn_type => "synType",
    syn_comment => "synComment",
    syn_literal => "synLiteral",
    syn_meta => "synMeta",
    syn_operator => "synOperator",
}

/// Optional conversation colors. Missing roles fall back when the theme is built.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OutputSpec {
    pub prose: Option<u32>,
    pub heading: Option<u32>,
    pub link: Option<u32>,
    pub inline_code_bg: Option<u32>,
    pub inline_code_fg: Option<u32>,
    pub code_bg: Option<u32>,
    pub code_fg: Option<u32>,
    pub code_header_bg: Option<u32>,
    pub code_header_fg: Option<u32>,
    pub user_bubble_bg: Option<u32>,
    pub user_bubble_fg: Option<u32>,
    pub quote_bg: Option<u32>,
    pub quote_border: Option<u32>,
    pub quote_fg: Option<u32>,
    pub keyword: Option<u32>,
    pub syn_string: Option<u32>,
    pub syn_number: Option<u32>,
    pub syn_function: Option<u32>,
    pub syn_type: Option<u32>,
    pub syn_comment: Option<u32>,
    pub syn_literal: Option<u32>,
    pub syn_meta: Option<u32>,
    pub syn_operator: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ThemeRecord {
    pub key: ThemeKey,
    pub name: &'static str,
    pub appearance: ThemeMode,
    pub chrome: Chrome,
    pub builtin: bool,
    pub colors: Palette,
    pub output: OutputSpec,
    pub scale: TypeScale,
}

#[derive(Clone, Copy)]
struct ParsedTheme {
    key: ThemeKey,
    name: &'static str,
    appearance: ThemeMode,
    chrome: Chrome,
    colors: PartialColors,
    output: OutputSpec,
    scale: TypeScale,
}

pub(crate) struct ThemeRegistry {
    order: Vec<ThemeKey>,
    records: HashMap<ThemeKey, ThemeRecord>,
    bases: HashMap<ThemeMode, Palette>,
    errors: Vec<String>,
}

impl ThemeRegistry {
    fn bundled() -> Self {
        let mut registry = Self {
            order: Vec::new(),
            records: HashMap::new(),
            bases: HashMap::new(),
            errors: Vec::new(),
        };
        let mut families = Vec::new();
        for file in BUNDLED_THEMES.files() {
            let Some(name) = file.path().file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let Some(text) = file.contents_utf8() else {
                registry
                    .errors
                    .push(format!("bundled theme {name} is not utf-8"));
                continue;
            };
            match parse_family(text) {
                Ok(family) => families.push((family.order, name.to_string(), family)),
                Err(error) => registry
                    .errors
                    .push(format!("bundled theme {name}: {error}")),
            }
        }
        families.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut themes = Vec::new();
        for (_, name, family) in &families {
            for problem in &family.problems {
                registry
                    .errors
                    .push(format!("bundled theme {name}: {problem}"));
            }
            themes.extend(family.themes.iter().cloned());
        }
        registry.capture_bases(&themes);
        for (_, name, family) in families {
            for theme in family.themes {
                if let Err(error) = registry.insert(theme, true, &name) {
                    registry.errors.push(error);
                }
            }
        }
        registry
    }

    fn capture_bases(&mut self, themes: &[ParsedTheme]) {
        let gray = gray_palette();
        for appearance in [ThemeMode::Dark, ThemeMode::Light] {
            let base_id = base_id(appearance);
            let colors = themes
                .iter()
                .find(|theme| theme.key == base_id)
                .map(|theme| theme.colors.fill(gray))
                .unwrap_or(gray);
            self.bases.insert(appearance, colors);
        }
    }

    fn insert(&mut self, theme: ParsedTheme, builtin: bool, origin: &str) -> Result<(), String> {
        let base = self
            .bases
            .get(&theme.appearance)
            .copied()
            .unwrap_or_else(gray_palette);
        let mut colors = theme.colors.fill(base);
        if let Some(value) = theme.output.syn_string {
            colors.syn_string = value;
        }
        if let Some(value) = theme.output.syn_number {
            colors.syn_number = value;
        }
        if let Some(value) = theme.output.syn_function {
            colors.syn_function = value;
        }
        if let Some(value) = theme.output.syn_type {
            colors.syn_type = value;
        }
        if let Some(value) = theme.output.syn_comment {
            colors.syn_comment = value;
        }
        if let Some(value) = theme.output.syn_literal {
            colors.syn_literal = value;
        }
        if let Some(value) = theme.output.syn_meta {
            colors.syn_meta = value;
        }
        if let Some(value) = theme.output.syn_operator {
            colors.syn_operator = value;
        }
        let record = ThemeRecord {
            key: theme.key,
            name: theme.name,
            appearance: theme.appearance,
            chrome: theme.chrome,
            builtin,
            colors,
            output: theme.output,
            scale: theme.scale,
        };
        if let Some(existing) = self.records.get(&theme.key) {
            // An install file replaces a built-in id. A second copy of the
            // same id, in another bundled file or another install file, keeps
            // the first theme.
            if builtin || !existing.builtin {
                return Err(format!(
                    "{origin}: duplicate id '{}'; already loaded as {}",
                    theme.key.as_str(),
                    existing.name
                ));
            }
            if let Some(owner) = self.name_owner(theme.name, Some(theme.key)) {
                return Err(format!(
                    "{origin}: name '{}' on id '{}' is already used by '{}'",
                    theme.name,
                    theme.key.as_str(),
                    owner.as_str()
                ));
            }
            if let Some(existing) = self.records.get_mut(&theme.key) {
                *existing = record;
            }
            return Ok(());
        }
        if let Some(owner) = self.name_owner(theme.name, None) {
            return Err(format!(
                "{origin}: name '{}' on id '{}' is already used by '{}'",
                theme.name,
                theme.key.as_str(),
                owner.as_str()
            ));
        }
        self.order.push(theme.key);
        self.records.insert(theme.key, record);
        Ok(())
    }

    /// Display names are unique, ignoring ASCII case, so the dropdown never
    /// shows two indistinguishable rows. `except` is the id being replaced.
    fn name_owner(&self, name: &str, except: Option<ThemeKey>) -> Option<ThemeKey> {
        self.records.iter().find_map(|(key, record)| {
            if except.is_some_and(|id| id == *key) {
                return None;
            }
            record.name.eq_ignore_ascii_case(name).then_some(*key)
        })
    }

    /// Read `dir/*.json`. Same id replaces a built-in theme, unless the new
    /// display name already belongs to a different id. A bad file or a bad
    /// theme is recorded and skipped. A missing directory leaves the registry
    /// unchanged.
    pub(crate) fn apply_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                self.errors.push(format!("{}: {error}", dir.display()));
                return;
            }
        };
        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                paths.push(path);
            }
        }
        paths.sort();
        for path in paths {
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    self.errors.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            let origin = path.display().to_string();
            match parse_family(&text) {
                Ok(family) => {
                    for problem in family.problems {
                        self.errors.push(format!("{origin}: {problem}"));
                    }
                    for theme in family.themes {
                        if let Err(error) = self.insert(theme, false, &origin) {
                            self.errors.push(error);
                        }
                    }
                }
                Err(error) => {
                    self.errors.push(format!("{origin}: {error}"));
                }
            }
        }
    }

    pub(crate) fn build(&self, key: ThemeKey, ui: UiPrefs) -> Option<Theme> {
        self.records
            .get(&key)
            .copied()
            .map(|record| Theme::from_resolved(record, ui))
    }
}

fn gray_palette() -> Palette {
    PartialColors::default().fill(Palette {
        background: 0x808080,
        surface: 0x808080,
        surface_secondary: 0x808080,
        elevated: 0x808080,
        hover: 0x808080,
        active: 0x808080,
        selection: 0x808080,
        selected_surface: 0x808080,
        selection_fg: 0x808080,
        border: 0x808080,
        border_subtle: 0x808080,
        text: 0x808080,
        text_secondary: 0x808080,
        text_muted: 0x808080,
        text_disabled: 0x808080,
        success: 0x808080,
        error: 0x808080,
        error_hover: 0x808080,
        error_fg: 0x808080,
        accent: 0x808080,
        accent_hover: 0x808080,
        accent_pressed: 0x808080,
        accent_fg: 0x808080,
        accent_text: 0x808080,
        focus_ring: 0x808080,
        action: 0x808080,
        action_hover: 0x808080,
        action_pressed: 0x808080,
        action_fg: 0x808080,
        warning: 0x808080,
        info: 0x808080,
        assistant_text: 0x808080,
        code_bg: 0x808080,
        code_text: 0x808080,
        syn_string: 0x808080,
        syn_number: 0x808080,
        syn_function: 0x808080,
        syn_type: 0x808080,
        syn_comment: 0x808080,
        syn_literal: 0x808080,
        syn_meta: 0x808080,
        syn_operator: 0x808080,
    })
}

fn base_id(appearance: ThemeMode) -> ThemeKey {
    match appearance {
        ThemeMode::Dark => ThemeKey::ORBIT_AZURE,
        ThemeMode::Light => ThemeKey::ORBIT_AZURE_LIGHT,
    }
}

fn global() -> &'static RwLock<ThemeRegistry> {
    static REGISTRY: OnceLock<RwLock<ThemeRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(ThemeRegistry::bundled()))
}

fn read_registry<T>(read: impl FnOnce(&ThemeRegistry) -> T) -> T {
    let registry = global()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read(&registry)
}

fn contains(key: ThemeKey) -> bool {
    read_registry(|registry| registry.records.contains_key(&key))
}

pub(crate) fn record(key: ThemeKey) -> Option<ThemeRecord> {
    read_registry(|registry| registry.records.get(&key).copied())
}

pub(crate) fn raw_palette(key: ThemeKey) -> Option<Palette> {
    record(key).map(|record| record.colors)
}

/// Built-in ids in load order, then ids added from the install directory.
pub fn bundled_keys() -> Vec<ThemeKey> {
    read_registry(|registry| {
        registry
            .order
            .iter()
            .copied()
            .filter(|key| {
                registry
                    .records
                    .get(key)
                    .is_some_and(|record| record.builtin)
            })
            .collect()
    })
}

pub fn keys_for(mode: ThemeMode) -> Vec<ThemeKey> {
    read_registry(|registry| {
        registry
            .order
            .iter()
            .copied()
            .filter(|key| {
                registry
                    .records
                    .get(key)
                    .is_some_and(|record| record.appearance == mode)
            })
            .collect()
    })
}

pub(crate) fn theme(key: ThemeKey, ui: UiPrefs) -> Option<Theme> {
    read_registry(|registry| registry.build(key, ui))
}

/// Directory of `themes/*.json` next to the running executable.
pub fn install_themes_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("themes"))
}

/// Load install-directory themes into the process registry once.
pub fn load_install_themes() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(dir) = install_themes_dir() else {
            return;
        };
        let mut registry = global()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry.apply_dir(&dir);
        for error in &registry.errors {
            eprintln!("Failed to load theme file: {error}");
        }
    });
}

fn leak(value: &str) -> &'static str {
    static CACHE: OnceLock<std::sync::Mutex<Vec<&'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(found) = cache.iter().copied().find(|item| *item == value) {
        return found;
    }
    let leaked: &'static str = Box::leak(value.to_string().into_boxed_str());
    cache.push(leaked);
    leaked
}

fn parse_hex(raw: &str) -> Option<u32> {
    let raw = raw.trim().strip_prefix('#')?;
    if raw.len() != 6 || !raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(raw, 16).ok()
}

struct ParsedFamily {
    order: i64,
    themes: Vec<ParsedTheme>,
    problems: Vec<String>,
}

fn parse_family(text: &str) -> Result<ParsedFamily, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("invalid JSON ({error})"))?;
    let themes = value
        .get("themes")
        .and_then(|themes| themes.as_array())
        .ok_or_else(|| "missing themes array".to_string())?;
    let mut parsed = Vec::with_capacity(themes.len());
    let mut problems = Vec::new();
    for theme in themes {
        if !theme.is_object() {
            problems.push("theme entry is not an object".to_string());
            continue;
        }
        match parse_theme(theme) {
            Ok(theme) => parsed.push(theme),
            Err(error) => problems.push(error),
        }
    }
    if parsed.is_empty() && problems.is_empty() {
        problems.push("themes array is empty".to_string());
    }
    // Numbered families keep their place. A file with no order sorts after
    // them, so dropping in a new JSON does not require a code change.
    let order = match value.get("order") {
        None => i64::MAX,
        Some(order) => match order.as_i64() {
            Some(order) => order,
            None => {
                problems.push("order must be an integer".to_string());
                i64::MAX
            }
        },
    };
    Ok(ParsedFamily {
        order,
        themes: parsed,
        problems,
    })
}

fn parse_theme(value: &serde_json::Value) -> Result<ParsedTheme, String> {
    let id = value
        .get("id")
        .and_then(|id| id.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "theme is missing an id".to_string())?
        .to_ascii_lowercase();
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(format!(
            "{id}: id may only contain letters, digits, hyphens, and underscores"
        ));
    }
    let appearance = match value.get("appearance").and_then(|mode| mode.as_str()) {
        Some("dark") => ThemeMode::Dark,
        Some("light") => ThemeMode::Light,
        _ => return Err(format!("{id}: appearance must be dark or light")),
    };
    let chrome = match value.get("chrome").and_then(|chrome| chrome.as_str()) {
        Some(raw) => {
            Chrome::parse(raw).ok_or_else(|| format!("{id}: chrome must be classic or flat"))?
        }
        None => Chrome::Classic,
    };
    let name = value
        .get("name")
        .and_then(|name| name.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(id.as_str());
    let mut colors = PartialColors::default();
    if let Some(colors_value) = value.get("colors") {
        let Some(table) = colors_value.as_object() else {
            return Err(format!("{id}: colors must be an object"));
        };
        for (key, entry) in table {
            let Some(raw) = entry.as_str() else {
                return Err(format!("{id}: color {key} is not a string"));
            };
            if !colors.set(key, 0) && !is_known_color(key) {
                continue;
            }
            let hex = parse_hex(raw).ok_or_else(|| format!("{id}: invalid color {key}"))?;
            if !colors.set(key, hex) {
                continue;
            }
        }
    }
    let mut output = OutputSpec::default();
    let mut scale = TypeScale::default();
    if let Some(table) = value.get("output") {
        parse_output(&id, table, &mut output, &mut scale)?;
    }
    Ok(ParsedTheme {
        key: ThemeKey::intern(&id),
        name: leak(name),
        appearance,
        chrome,
        colors,
        output,
        scale,
    })
}

fn is_known_color(key: &str) -> bool {
    let mut probe = PartialColors::default();
    probe.set(key, 0)
}

fn parse_output(
    id: &str,
    value: &serde_json::Value,
    output: &mut OutputSpec,
    scale: &mut TypeScale,
) -> Result<(), String> {
    let Some(table) = value.as_object() else {
        return Err(format!("{id}: output must be an object"));
    };
    for (key, entry) in table {
        if key == "syntax" {
            parse_syntax(id, entry, output)?;
            continue;
        }
        if key == "type" {
            parse_scale(id, entry, scale)?;
            continue;
        }
        if output_slot(output, key).is_none() {
            continue;
        }
        let raw = entry
            .as_str()
            .ok_or_else(|| format!("{id}: output {key} is not a string"))?;
        let hex = parse_hex(raw).ok_or_else(|| format!("{id}: invalid color {key}"))?;
        if let Some(slot) = output_slot(output, key) {
            *slot = Some(hex);
        }
    }
    Ok(())
}

fn output_slot<'a>(output: &'a mut OutputSpec, key: &str) -> Option<&'a mut Option<u32>> {
    Some(match key {
        "prose" => &mut output.prose,
        "heading" => &mut output.heading,
        "link" => &mut output.link,
        "inlineCodeBg" => &mut output.inline_code_bg,
        "inlineCodeFg" => &mut output.inline_code_fg,
        "codeBg" => &mut output.code_bg,
        "codeFg" => &mut output.code_fg,
        "codeHeaderBg" => &mut output.code_header_bg,
        "codeHeaderFg" => &mut output.code_header_fg,
        "userBubbleBg" => &mut output.user_bubble_bg,
        "userBubbleFg" => &mut output.user_bubble_fg,
        "quoteBg" => &mut output.quote_bg,
        "quoteBorder" => &mut output.quote_border,
        "quoteFg" => &mut output.quote_fg,
        _ => return None,
    })
}

fn parse_syntax(
    id: &str,
    value: &serde_json::Value,
    output: &mut OutputSpec,
) -> Result<(), String> {
    let Some(table) = value.as_object() else {
        return Err(format!("{id}: output.syntax must be an object"));
    };
    for (key, entry) in table {
        let slot = match key.as_str() {
            "keyword" => &mut output.keyword,
            "string" => &mut output.syn_string,
            "number" => &mut output.syn_number,
            "function" => &mut output.syn_function,
            "type" => &mut output.syn_type,
            "comment" => &mut output.syn_comment,
            "literal" => &mut output.syn_literal,
            "meta" => &mut output.syn_meta,
            "operator" => &mut output.syn_operator,
            _ => continue,
        };
        let raw = entry
            .as_str()
            .ok_or_else(|| format!("{id}: syntax {key} is not a string"))?;
        *slot = Some(parse_hex(raw).ok_or_else(|| format!("{id}: invalid color {key}"))?);
    }
    Ok(())
}

fn parse_scale(id: &str, value: &serde_json::Value, scale: &mut TypeScale) -> Result<(), String> {
    let Some(table) = value.as_object() else {
        return Err(format!("{id}: output.type must be an object"));
    };
    for (key, entry) in table {
        if !matches!(
            key.as_str(),
            "body"
                | "bodyLine"
                | "h1"
                | "h1Line"
                | "h2"
                | "h2Line"
                | "h3"
                | "h3Line"
                | "h4"
                | "h4Line"
                | "code"
                | "codeLine"
                | "aside"
                | "asideLine"
        ) {
            continue;
        }
        let number = entry
            .as_f64()
            .filter(|number| number.is_finite() && *number > 0.0)
            .ok_or_else(|| format!("{id}: type {key} must be a positive number"))?;
        scale.apply(key, number as f32);
    }
    Ok(())
}

fn alias(raw: &str) -> Option<ThemeKey> {
    Some(match raw {
        "orbit"
        | "dark"
        | "one-dark"
        | "zedokai"
        | "zedokai-darker"
        | "zedokai-filter-octagon"
        | "zedokai-darker-filter-octagon"
        | "zedokai-filter-spectrum"
        | "zedokai-darker-filter-spectrum" => ThemeKey::ORBIT,
        "orbit-light" | "light" => ThemeKey::ORBIT_LIGHT,
        "orbit-paper" => ThemeKey::ORBIT_PAPER,
        "orbit-contrast" | "orbit-high-contrast" => ThemeKey::ORBIT_CONTRAST,
        "orbit-darker" => ThemeKey::ORBIT_DARKER,
        "orbit-cobalt" | "orbit-minimal" | "codex" | "codex-dark" => ThemeKey::ORBIT_COBALT,
        "orbit-azure" | "azure" => ThemeKey::ORBIT_AZURE,
        "orbit-azure-light" | "azure-light" => ThemeKey::ORBIT_AZURE_LIGHT,
        "vague"
        | "batsignal-dark"
        | "batsignal"
        | "ashwood"
        | "obsidian-dark"
        | "obsidian"
        | "matte-black"
        | "matte-black-theme"
        | "adwaita-pastel-dark"
        | "adwaita-pastel"
        | "ashen"
        | "discord-dark"
        | "dark-discord"
        | "discord"
        | "catppuccin"
        | "catppuccin-mocha"
        | "catppuccin-dark"
        | "dracula"
        | "dracula-dark"
        | "gruvbox"
        | "gruvbox-dark"
        | "nord"
        | "nord-dark"
        | "tokyo-night"
        | "tokyonight"
        | "tokyo-night-storm"
        | "one-dark-pro"
        | "onedarkpro"
        | "one-dark-pro-max"
        | "onedarkpromax"
        | "odpm"
        | "monokai"
        | "solarized-dark"
        | "solarized"
        | "ayu-dark"
        | "ayu"
        | "kanagawa"
        | "kanagawa-wave"
        | "aura"
        | "aura-dark"
        | "carbonfox"
        | "carbonfox-dark"
        | "fields-of-the-shire"
        | "fields-of-the-shire-dark"
        | "jetbrains"
        | "jetbrains-dark"
        | "mono"
        | "mono-plus"
        | "night-owl"
        | "nightowl"
        | "night-owl-dark"
        | "open-chamber"
        | "openchamber"
        | "vesper"
        | "vitesse" => ThemeKey::ORBIT,
        "catppuccin-latte" | "catppuccin-light" | "latte" | "solarized-light" | "one-light"
        | "one-light-pro" | "nord-light" | "nord-snow-storm" | "gruvbox-light"
        | "tokyo-night-light" | "tokyonight-light" | "tokyo-night-day" | "ayu-light" => {
            ThemeKey::ORBIT_LIGHT
        }
        "flexoki" | "flexoki-dark" => ThemeKey::FLEXOKI,
        "flexoki-light" => ThemeKey::FLEXOKI_LIGHT,
        "operational" | "operational-dark" | "operational-minimalism" => ThemeKey::OPERATIONAL,
        "operational-light" | "operational-minimalism-light" => ThemeKey::OPERATIONAL_LIGHT,
        "vscode-dark" => ThemeKey::VSCODE_DARK,
        "vscode-light" => ThemeKey::VSCODE_LIGHT,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::TokenClass;
    use gpui::px;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orbit-themes-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn registry_from(dir: &Path) -> ThemeRegistry {
        let mut registry = ThemeRegistry::bundled();
        registry.apply_dir(dir);
        registry
    }

    #[test]
    fn install_dir_override_replaces_builtin_and_fills_from_base() {
        let dir = temp_dir("override");
        std::fs::write(
            dir.join("orbit.json"),
            r##"{
                "name": "Local",
                "themes": [{
                    "id": "orbit",
                    "name": "Orbit Night",
                    "appearance": "dark",
                    "chrome": "classic",
                    "colors": { "accent": "#112233" }
                }]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        let theme = registry.build(ThemeKey::ORBIT, UiPrefs::default()).unwrap();
        assert_eq!(
            registry.records.get(&ThemeKey::ORBIT).unwrap().name,
            "Orbit Night"
        );
        assert_eq!(theme.accent, super::super::hex(0x112233));
        let azure = registry
            .build(ThemeKey::ORBIT_AZURE, UiPrefs::default())
            .unwrap();
        assert_eq!(theme.text_primary, azure.text_primary);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_user_theme_fills_missing_colors_from_the_appearance_base() {
        let dir = temp_dir("partial");
        std::fs::write(
            dir.join("custom.json"),
            r##"{
                "name": "Custom",
                "themes": [{
                    "id": "custom-dark",
                    "name": "Custom Dark",
                    "appearance": "dark",
                    "colors": { "background": "#010101" }
                }]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        let theme = registry
            .build(ThemeKey::intern("custom-dark"), UiPrefs::default())
            .unwrap();
        let azure = registry
            .build(ThemeKey::ORBIT_AZURE, UiPrefs::default())
            .unwrap();
        assert_eq!(theme.background, super::super::hex(0x010101));
        assert_eq!(theme.text_primary, azure.text_primary);
        assert_eq!(theme.accent, azure.accent);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_hex_is_rejected_and_builtins_remain() {
        let dir = temp_dir("invalid");
        let path = dir.join("broken.json");
        std::fs::write(
            &path,
            r##"{
                "name": "Broken",
                "themes": [{
                    "id": "orbit",
                    "name": "Nope",
                    "appearance": "dark",
                    "colors": { "accent": "not-a-color" }
                }]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        assert!(
            registry
                .errors
                .iter()
                .any(|error| error.contains(&path.display().to_string())),
            "errors: {:?}",
            registry.errors
        );
        let orbit = registry.build(ThemeKey::ORBIT, UiPrefs::default()).unwrap();
        assert_eq!(orbit.theme_id.label(), "Orbit");
        assert_eq!(orbit.accent, super::super::hex(0xE2795B));
        assert!(registry.records.contains_key(&ThemeKey::VSCODE_DARK));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bundled_themes_have_unique_names_and_no_load_errors() {
        let registry = ThemeRegistry::bundled();
        assert!(
            registry.errors.is_empty(),
            "bundled themes: {:?}",
            registry.errors
        );
        let mut names: Vec<&str> = registry
            .records
            .values()
            .map(|record| record.name)
            .collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        assert_eq!(names.len(), before);
    }

    #[test]
    fn a_bad_theme_does_not_drop_its_siblings() {
        let dir = temp_dir("sibling");
        let path = dir.join("mixed.json");
        std::fs::write(
            &path,
            r##"{
                "themes": [
                    {
                        "id": "good-dark",
                        "name": "Good Dark",
                        "appearance": "dark",
                        "colors": { "background": "#010101" }
                    },
                    "not-an-object",
                    {
                        "id": "bad-dark",
                        "name": "Bad Dark",
                        "appearance": "sideways",
                        "colors": { "accent": "nope" }
                    }
                ]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        let theme = registry
            .build(ThemeKey::intern("good-dark"), UiPrefs::default())
            .unwrap();
        assert_eq!(theme.background, super::super::hex(0x010101));
        assert!(!registry.records.contains_key(&ThemeKey::intern("bad-dark")));
        let text = registry.errors.join("\n");
        assert!(text.contains(&path.display().to_string()), "{text}");
        assert!(text.contains("not an object"), "{text}");
        assert!(text.contains("appearance must be dark or light"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn duplicate_id_keeps_the_first_theme() {
        let dir = temp_dir("dup-id");
        std::fs::write(
            dir.join("a.json"),
            r##"{
                "themes": [{
                    "id": "dup-dark",
                    "name": "First Dup",
                    "appearance": "dark",
                    "colors": { "background": "#010101" }
                }]
            }"##,
        )
        .unwrap();
        std::fs::write(
            dir.join("b.json"),
            r##"{
                "themes": [
                    {
                        "id": "DUP-dark",
                        "name": "Second Dup",
                        "appearance": "dark",
                        "colors": { "background": "#020202" }
                    },
                    {
                        "id": "other-dark",
                        "name": "First Dup",
                        "appearance": "dark"
                    }
                ]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        let theme = registry
            .build(ThemeKey::intern("dup-dark"), UiPrefs::default())
            .unwrap();
        assert_eq!(theme.background, super::super::hex(0x010101));
        assert_eq!(
            registry
                .records
                .get(&ThemeKey::intern("dup-dark"))
                .unwrap()
                .name,
            "First Dup"
        );
        assert!(!registry
            .records
            .contains_key(&ThemeKey::intern("other-dark")));
        let text = registry.errors.join("\n");
        assert!(text.contains("duplicate id 'dup-dark'"), "{text}");
        assert!(text.contains("already used by 'dup-dark'"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn override_that_steals_another_name_is_rejected() {
        let dir = temp_dir("steal");
        std::fs::write(
            dir.join("steal.json"),
            r##"{
                "order": "first",
                "themes": [{
                    "id": "orbit",
                    "name": "vs code dark",
                    "appearance": "dark",
                    "colors": { "accent": "#112233" }
                }]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        assert_eq!(
            registry.records.get(&ThemeKey::ORBIT).unwrap().name,
            "Orbit"
        );
        assert_eq!(
            registry
                .build(ThemeKey::ORBIT, UiPrefs::default())
                .unwrap()
                .accent,
            super::super::hex(0xE2795B)
        );
        let text = registry.errors.join("\n");
        assert!(text.contains("already used by 'vscode-dark'"), "{text}");
        assert!(text.contains("order must be an integer"), "{text}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_skips_the_file_and_keeps_the_rest() {
        let dir = temp_dir("json");
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();
        std::fs::write(
            dir.join("ok.json"),
            r##"{
                "themes": [{
                    "id": "ok-dark",
                    "name": "Ok Dark",
                    "appearance": "dark"
                }]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        assert!(registry
            .build(ThemeKey::intern("ok-dark"), UiPrefs::default())
            .is_some());
        assert!(registry
            .build(ThemeKey::ORBIT, UiPrefs::default())
            .is_some());
        assert!(registry
            .errors
            .iter()
            .any(|error| error.contains("broken.json") && error.contains("invalid JSON")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_install_dir_keeps_bundled_themes() {
        let mut registry = ThemeRegistry::bundled();
        registry.apply_dir(Path::new("this-themes-dir-does-not-exist"));
        assert!(registry.errors.is_empty());
        assert!(registry.records.contains_key(&ThemeKey::ORBIT_AZURE));
    }

    #[test]
    fn omitted_output_uses_semantic_colors_and_the_default_scale() {
        let theme = Theme::for_id(ThemeKey::ORBIT);
        assert_eq!(theme.output_prose, theme.assistant_text);
        assert_eq!(theme.type_scale.body, 14.);
        assert_eq!(theme.type_scale.body_line, 24.);
        assert_eq!(theme.type_scale.code, 13.);
        assert_eq!(theme.type_scale.aside, 12.);
        assert_eq!(theme.body_px(), px(14.));
        assert_eq!(theme.code_block_px(), px(13.));
        assert_eq!(theme.aside_px(), px(12.));
        assert_eq!(
            theme.token_color(TokenClass::Keyword),
            theme.accent_text,
            "an omitted keyword falls back to accent text"
        );
    }

    #[test]
    fn keyword_color_is_independent_of_accent_when_set() {
        let dir = temp_dir("keyword");
        std::fs::write(
            dir.join("keyword.json"),
            r##"{
                "name": "Keyword",
                "themes": [{
                    "id": "keyword-dark",
                    "name": "Keyword Dark",
                    "appearance": "dark",
                    "colors": { "accent": "#0078D4" },
                    "output": { "syntax": { "keyword": "#FF00FF" } }
                }]
            }"##,
        )
        .unwrap();
        let registry = registry_from(&dir);
        let theme = registry
            .build(ThemeKey::intern("keyword-dark"), UiPrefs::default())
            .unwrap();
        assert_ne!(theme.token_color(TokenClass::Keyword), theme.accent);
        assert_eq!(
            theme.token_color(TokenClass::Command),
            theme.token_color(TokenClass::Keyword)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn editor_size_does_not_grow_aside_text() {
        let theme = Theme::for_id(ThemeKey::ORBIT_AZURE).with_ui(UiPrefs {
            ui_font_size: 14.,
            editor_font_size: 16.,
            ..UiPrefs::default()
        });
        assert_eq!(theme.aside_px(), px(12.));
        assert_eq!(theme.aside_line_px(), px(18.));
        assert_eq!(theme.code_block_px(), px(16.));
        assert_eq!(theme.body_px(), px(14.));
    }

    #[test]
    fn legacy_aliases_resolve_to_builtin_ids() {
        assert_eq!(ThemeKey::parse("dracula"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("nord"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("one-dark"), Some(ThemeKey::ORBIT));
        assert_eq!(ThemeKey::parse("one-light"), Some(ThemeKey::ORBIT_LIGHT));
        assert_eq!(ThemeKey::parse("system"), None);
        for key in bundled_keys() {
            assert_eq!(ThemeKey::parse(key.as_str()), Some(key));
        }
    }

    #[test]
    fn vscode_family_is_flat_and_leaves_the_type_scale_empty() {
        let dark = Theme::for_id(ThemeKey::VSCODE_DARK);
        assert_eq!(dark.chrome, Chrome::Flat);
        assert_eq!(dark.type_scale, TypeScale::default());
        assert_eq!(dark.output_link, super::super::hex(0x4DAAF5));
        assert_ne!(dark.token_color(TokenClass::Keyword), dark.accent);
        let light = Theme::for_id(ThemeKey::VSCODE_LIGHT);
        assert_eq!(light.chrome, Chrome::Flat);
        assert_eq!(light.background, super::super::hex(0xFFFFFF));
        assert_eq!(light.surface, super::super::hex(0xF3F3F3));
        assert_eq!(light.action, super::super::hex(0x005FB8));
    }
}
