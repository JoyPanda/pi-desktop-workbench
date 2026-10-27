//! Per-workspace sidebar marks: the curated HugeIcons and the semantic tints a
//! project header can wear. Orbit-owned, persisted inside
//! `~/.orbit-pi/workspaces.json` beside the project list, and resolved against
//! the active [`Theme`] at render time so a mark never fights a palette.

use gpui::{Hsla, SharedString};

use crate::theme::Theme;

/// One workspace's stored mark. Both halves are optional; the defaults (the
/// folder glyph, muted ink) apply when a field is unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceMark {
    /// Embedded HugeIcons file stem, e.g. `rocket-01`.
    pub icon: Option<String>,
    /// Stable tint key — see [`TINTS`].
    pub tint: Option<String>,
}

impl WorkspaceMark {
    /// Whether the mark is the unstyled default. Such an entry is dropped
    /// instead of persisted, so an untouched workspace writes no extra keys.
    pub fn is_default(&self) -> bool {
        self.icon.is_none() && self.tint.is_none()
    }
}

/// A curated subset of the embedded HugeIcons set that reads as a project
/// mark. Kept to a compact grid in the picker; the folder-open / sync /
/// archive glyphs double as the automatic status marks below.
pub(crate) const PROJECT_ICONS: &[&str] = &[
    "folder",
    "folder-open",
    "folder-sync",
    "folder-archive",
    "branch",
    "rocket-01",
    "star",
    "spark",
    "cloud",
    "terminal",
    "monitor",
    "server-stack",
    "git-fork",
    "github",
    "image",
    "lock",
    "clock",
    "tag-01",
    "task",
    "compose",
    "magic-wand",
    "start-up",
    "extensions",
    "chat",
    "archive",
    "contrast",
    "side-by-side",
];

/// One row of the picker's icon grid.
pub(crate) const ICON_COLUMNS: usize = 6;

/// A tint resolved against the active theme.
type TintFn = fn(&Theme) -> Hsla;

/// The semantic tints, in picker order. The key is stable on disk; the color
/// follows the active palette. `syn_*` tokens are reused as a dependable
/// spread of hues that every palette tunes to stay legible.
pub(crate) const TINTS: &[(&str, TintFn)] = &[
    ("default", |theme| theme.text_muted),
    ("accent", |theme| theme.accent),
    ("green", |theme| theme.success),
    ("red", |theme| theme.error),
    ("yellow", |theme| theme.warning),
    ("blue", |theme| theme.syn_function),
    ("purple", |theme| theme.syn_type),
    ("pink", |theme| theme.syn_string),
];

/// The ink for a persisted tint key, falling back to the muted default.
pub(crate) fn tint_color(theme: &Theme, key: Option<&str>) -> Hsla {
    key.and_then(|key| {
        TINTS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, color)| color(theme))
    })
    .unwrap_or(theme.text_muted)
}

/// The ink for a project group header's name. A project the user tinted wears
/// that hue on its name, not only on its icon; an untinted one leans toward the
/// palette's accent, so the group reads as Orbit's own container rather than as
/// another neutral row.
///
/// Either way a header never shares its ink with a session title, which sits a
/// step below at secondary — the two read as parent and child at a glance
/// instead of as the same kind of row (issue #78).
///
/// An unknown key (a hand-edited store) is not a tint, and neither is a key the
/// palette collapsed onto body ink (`OperationalLight` maps `syn_type` to
/// `text_secondary` and `syn_function` to `text_primary`): those take the
/// default accent-leaning ink rather than a color indistinguishable from the
/// name's own rows.
pub(crate) fn label_ink(theme: &Theme, key: Option<&str>) -> Hsla {
    let tint = key.and_then(|key| {
        TINTS
            .iter()
            .find(|(name, _)| *name == key && *name != "default")
            .map(|(_, color)| color(theme))
    });
    match tint {
        Some(color) if color != theme.text_secondary && color != theme.text_primary => {
            name_ink(theme, color, TINT_NAME_SATURATION)
        }
        _ => default_label_ink(theme),
    }
}

/// The untinted project ink: the palette's accent hue, held back to a hint.
pub(crate) fn default_label_ink(theme: &Theme) -> Hsla {
    name_ink(theme, theme.accent_text, DEFAULT_NAME_SATURATION)
}

/// Build a name ink from a hue at the primary ink's lightness.
///
/// Lightness is what the AA floor rides, and the primary ink already clears it
/// on the sidebar, so a project name keeps exactly that brightness and only the
/// hue and saturation vary. That matters for the user tints: [`TINTS`] are
/// chosen for *fills and glyphs*, and several (the `accent` fill, a saturated
/// blue) sit too dark to be read as text on a near-black sidebar. Borrowing the
/// name's lightness from the body ink instead of from the tint makes the hue
/// safe in every palette without a contrast search at paint time.
fn name_ink(theme: &Theme, hue: Hsla, saturation: f32) -> Hsla {
    Hsla {
        h: hue.h,
        s: hue.s.min(saturation),
        l: theme.text_primary.l,
        // Opaque: this is text, not a wash. The accessor above never returns a
        // translucent ink, and an alpha here would silently undercut the AA
        // floors the palettes are tested against.
        a: 1.0,
    }
}

/// How much of the accent hue an untinted project name keeps. Deliberately a
/// hint, not a statement: the name should read as warm/colored on inspection,
/// while the selected row's wash and the running session's accent stay the
/// loudest things in the sidebar.
///
/// The knob is saturation, not blend weight, because a blend is invisible here.
/// Against Orbit dark's body ink (`#E2E2E2`) the accent's hue moves the channels
/// by roughly:
///
/// | cap | name | ΔRGB |
/// |-----|------|------|
/// | 0.10 | `#E5E0DF` | ±3 — the literal "10%" ask, measurably tinted, visually identical |
/// | 0.22 | `#E8DEDC` | ±6 |
/// | **0.35** | `#ECDCD8` | ±10 — reads as colored without becoming a second accent |
/// | 0.50 | `#F0DAD3` | ±15 |
///
/// Light palettes lean the same way from their near-black body ink. Lower this
/// toward 0.10 for an almost-neutral name; raise it and the sidebar starts
/// competing with its own selection.
const DEFAULT_NAME_SATURATION: f32 = 0.35;

/// A user-picked project tint is a deliberate choice, so it may read as a real
/// color — capped only so a maximally saturated fill hue cannot turn a name into
/// a highlighter.
const TINT_NAME_SATURATION: f32 = 0.5;

/// The asset path for a persisted icon stem, falling back to the folder
/// glyph. Stems are validated against [`PROJECT_ICONS`] so a hand-edited store
/// can never point the renderer at an arbitrary asset.
pub(crate) fn icon_path(icon: Option<&str>) -> SharedString {
    match icon.filter(|stem| PROJECT_ICONS.contains(stem)) {
        Some(stem) => format!("icons/{stem}.svg").into(),
        None => "icons/folder.svg".into(),
    }
}

/// A workspace's live state, resolved to a folder glyph when the user has not
/// picked a custom mark icon. Ordered by priority: the first match wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceStatus {
    /// The active session, or the workspace it is working in.
    Current,
    /// A running session, or a workspace with at least one mid-run.
    Running,
    /// An archived session, or a workspace whose sessions are all archived.
    Archived,
    /// Nothing special — the plain folder.
    Idle,
}

impl WorkspaceStatus {
    fn icon_stem(self) -> &'static str {
        match self {
            Self::Current => "folder-open",
            Self::Running => "folder-sync",
            Self::Archived => "folder-archive",
            Self::Idle => "folder",
        }
    }
}

/// The mark's own icon when the user picked one; otherwise the glyph for the
/// workspace's status. A custom mark always wins over the status default, so
/// status never overrides a deliberate choice.
pub(crate) fn resolved_icon_path(mark: &WorkspaceMark, status: WorkspaceStatus) -> SharedString {
    match mark.icon.as_deref() {
        Some(stem) if PROJECT_ICONS.contains(&stem) => format!("icons/{stem}.svg").into(),
        _ => format!("icons/{}.svg", status.icon_stem()).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_stems_and_keys_fall_back_to_the_defaults() {
        assert_eq!(icon_path(None), SharedString::from("icons/folder.svg"));
        assert_eq!(
            icon_path(Some("../../etc/passwd")),
            SharedString::from("icons/folder.svg")
        );
        assert_eq!(
            icon_path(Some("rocket-01")),
            SharedString::from("icons/rocket-01.svg")
        );

        let theme = crate::theme::Theme::default();
        assert_eq!(tint_color(&theme, Some("nonsense")), theme.text_muted);
        assert_eq!(tint_color(&theme, None), theme.text_muted);
        assert_eq!(tint_color(&theme, Some("accent")), theme.accent);
    }

    #[test]
    fn a_project_name_never_wears_the_ink_of_a_session_title() {
        // Issue #78: a group header and the session rows under it must not
        // look like the same kind of row. The header owns the accent-hued,
        // body-bright ink (or its own hue); session titles sit a step below at
        // secondary.
        for id in crate::theme::ThemeId::ALL {
            let theme = crate::theme::Theme::for_id(id);
            let default_ink = default_label_ink(&theme);
            assert_ne!(
                default_ink, theme.text_secondary,
                "{id:?}: the default must not match a session title's ink"
            );
            // The default is the accent's *hue*, not a neutral: that is what
            // the title asked for, at a saturation that stays a hint.
            assert!(
                default_ink.s > 0.0 && default_ink.s <= DEFAULT_NAME_SATURATION,
                "{id:?}: the default is a barely-saturated accent, got s={}",
                default_ink.s
            );
            for key in [None, Some("default"), Some("nonsense")] {
                assert_eq!(
                    label_ink(&theme, key),
                    default_ink,
                    "{id:?} {key:?}: an untinted (or unknown) mark takes the default ink"
                );
            }
            for (name, color) in TINTS {
                let ink = label_ink(&theme, Some(name));
                // The invariant this whole change exists for: a project name
                // never wears the ink of the session titles beneath it.
                assert_ne!(
                    ink, theme.text_secondary,
                    "{id:?} {name}: a project name must not match a session title's ink"
                );
                let usable = *name != "default"
                    && color(&theme) != theme.text_secondary
                    && color(&theme) != theme.text_primary;
                if usable {
                    // The user's hue wins over the default — and names keep one
                    // brightness, so only the hue carries the identity.
                    assert_eq!(ink.l, theme.text_primary.l, "{id:?} {name}: lightness");
                    let tint_hue = color(&theme).h;
                    let dh = (ink.h - tint_hue).abs();
                    assert!(
                        dh.min(1.0 - dh) < 0.01,
                        "{id:?} {name}: hue {} should come from the tint {tint_hue}",
                        ink.h
                    );
                    assert_ne!(ink, default_ink, "{id:?} {name}: the tint overrides");
                } else {
                    // "default", and any tint the palette collapsed onto body
                    // ink, take the default instead.
                    assert_eq!(ink, default_ink, "{id:?} {name}");
                }
            }
        }
    }

    #[test]
    fn every_curated_icon_exists_in_the_embedded_set() {
        for stem in PROJECT_ICONS {
            let path = format!("icons/{stem}.svg");
            let bytes =
                <crate::assets::Assets as gpui::AssetSource>::load(&crate::assets::Assets, &path)
                    .expect("asset source reads");
            assert!(bytes.is_some(), "missing icon asset: {path}");
        }
    }

    #[test]
    fn status_icons_apply_only_without_a_custom_mark() {
        let idle = WorkspaceMark::default();
        assert_eq!(
            resolved_icon_path(&idle, WorkspaceStatus::Current),
            SharedString::from("icons/folder-open.svg")
        );
        assert_eq!(
            resolved_icon_path(&idle, WorkspaceStatus::Running),
            SharedString::from("icons/folder-sync.svg")
        );
        assert_eq!(
            resolved_icon_path(&idle, WorkspaceStatus::Archived),
            SharedString::from("icons/folder-archive.svg")
        );
        assert_eq!(
            resolved_icon_path(&idle, WorkspaceStatus::Idle),
            SharedString::from("icons/folder.svg")
        );

        // A user-picked icon wins over the status glyph.
        let custom = WorkspaceMark {
            icon: Some("rocket-01".into()),
            tint: None,
        };
        assert_eq!(
            resolved_icon_path(&custom, WorkspaceStatus::Current),
            SharedString::from("icons/rocket-01.svg")
        );
    }

    #[test]
    fn default_mark_is_recognised() {
        assert!(WorkspaceMark::default().is_default());
        assert!(!WorkspaceMark {
            icon: Some("star".into()),
            tint: None,
        }
        .is_default());
        assert!(!WorkspaceMark {
            icon: None,
            tint: Some("accent".into()),
        }
        .is_default());
    }
}
