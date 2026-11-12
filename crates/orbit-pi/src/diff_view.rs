//! The shared diff-row painters: file headers, hunk headers, meta lines, gap
//! rows, and syntax-coloured code rows.
//!
//! Extracted from the Review pane so the pane's diff and the Review page's
//! per-file preview render identically. The pane still owns its virtualized
//! list and gap expansion; this module owns the row vocabulary. Code rows
//! also register their layout with the pane's read-only selection.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, AnyElement, Bounds, CursorStyle, Div, Element, ElementId, Font,
    FontFeatures, FontStyle, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, StyledText, TextLayout,
    TextRun, Window,
};

use crate::app::{file_glyph, icon};
use crate::diff_select::{clip_to_mask, DiffSide, DiffTextState};
use crate::review::{self, LineKind};
use crate::theme::tokens::{DynamicSpacing, IconSize, TextSize};
use crate::theme::{self, Theme, ThemeMode};
use crate::transcript_view::highlight_runs;

/// Review diff row metrics, shared by every surface that paints rows.
pub const DIFF_TEXT_SIZE: f32 = 12.5;
/// File headers are one fixed row.
pub const REVIEW_FILE_HEADER_HEIGHT: f32 = 36.;
/// A grouped list's section divider is one fixed row — tall enough to seat a
/// 28px bulk action button beside its label and counts.
pub const REVIEW_SECTION_HEADER_HEIGHT: f32 = 40.;
/// Hunk and meta rows are one fixed row.
pub const REVIEW_HUNK_HEIGHT: f32 = 24.;

/// One file's header row: collapse chevron, glyph, path, and line counts.
/// Returns a plain `Div` so the owning surface can make it actionable (the
/// Review pane toggles the file's rows with a click).
pub fn render_file_header(
    file: &review::File,
    theme: Theme,
    nerd: Option<&SharedString>,
    dark: bool,
    collapsed: bool,
) -> Div {
    let fallback = icon(
        "icons/file.svg",
        IconSize::Small.px(&theme),
        theme.text_muted,
    )
    .into_any_element();
    let glyph = file_glyph(&file.path, dark, nerd, IconSize::Small.px(&theme), fallback);
    div()
        .w_full()
        .min_w_0()
        .h(px(REVIEW_FILE_HEADER_HEIGHT))
        .px(DynamicSpacing::Base12.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.surface_elevated)
        .child(icon(
            if collapsed {
                "icons/chevron-right.svg"
            } else {
                "icons/chevron-down.svg"
            },
            IconSize::Small.px(&theme),
            theme.text_muted,
        ))
        .child(glyph)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(theme::code_font_family())
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(file.path.clone()),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(if file.additions > 0 {
                    theme.success
                } else {
                    theme.text_muted
                })
                .child(format!("+{}", file.additions)),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(if file.deletions > 0 {
                    theme.error
                } else {
                    theme.text_muted
                })
                .child(format!("-{}", file.deletions)),
        )
}

/// A group divider in a grouped list ("Staged" / "Changes"): a leading
/// stage / unstage glyph, the label, its file count in a quiet pill, the
/// section's actions, and its line deltas at the far edge.
pub fn render_section_header(
    label: &str,
    icon_path: Option<&'static str>,
    count: usize,
    additions: u64,
    deletions: u64,
    actions: Option<AnyElement>,
    theme: Theme,
) -> Div {
    div()
        .w_full()
        .h(px(REVIEW_SECTION_HEADER_HEIGHT))
        .px(DynamicSpacing::Base16.px(&theme))
        .flex()
        .items_center()
        .gap(px(10.))
        .border_b_1()
        .border_color(theme.border)
        // The section head sits on the page's own black canvas — the raised
        // surface is reserved for the file rows below it.
        .bg(theme.background)
        .when_some(icon_path, |row, icon_path| {
            row.child(icon(
                icon_path,
                IconSize::Small.px(&theme),
                theme.text_secondary,
            ))
        })
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_primary)
                .child(label.to_string()),
        )
        .child(
            div()
                .min_w(px(22.))
                .h(px(18.))
                .px(px(7.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme.overlay)
                .text_size(TextSize::XSmall.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(count.to_string()),
        )
        .child(div().flex_1())
        .when_some(actions, |row, actions| row.child(actions))
        .child(div().w(px(1.)).h(px(14.)).flex_none().bg(theme.border))
        .child(
            div()
                .min_w(px(42.))
                .text_align(gpui::TextAlign::Right)
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if additions > 0 {
                    theme.success
                } else {
                    theme.text_muted
                })
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .min_w(px(36.))
                .text_align(gpui::TextAlign::Right)
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if deletions > 0 {
                    theme.error
                } else {
                    theme.text_muted
                })
                .child(format!("-{deletions}")),
        )
}

pub fn render_hunk_header(content: &str, theme: Theme, wrap: bool, gutter_w: Pixels) -> AnyElement {
    div()
        .min_h(px(REVIEW_HUNK_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .text_color(theme.text_muted)
        .child(
            div()
                .w(gutter_w)
                .min_h(px(REVIEW_HUNK_HEIGHT))
                .flex_none()
                .border_r_1()
                .border_color(theme.border)
                .bg(theme.overlay),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base04.px(&theme))
                .overflow_hidden()
                .when(wrap, |body| body.whitespace_normal())
                .when(!wrap, |body| body.whitespace_nowrap())
                .bg(theme.overlay)
                .child(content.to_string()),
        )
        .into_any_element()
}

pub fn render_meta(content: &str, theme: Theme, wrap: bool, gutter_w: Pixels) -> AnyElement {
    div()
        .min_h(px(REVIEW_HUNK_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .text_color(theme.text_muted)
        .child(div().w(gutter_w).min_h(px(REVIEW_HUNK_HEIGHT)).flex_none())
        .child(
            div()
                .min_w_0()
                .flex_1()
                .py(DynamicSpacing::Base04.px(&theme))
                .pr(px(10.))
                .overflow_hidden()
                .when(wrap, |body| body.whitespace_normal())
                .when(!wrap, |body| body.whitespace_nowrap())
                .child(content.to_string()),
        )
        .into_any_element()
}

/// The gutter of one diff row at the default editor size and three digits.
/// Wider numbers and a larger editor font grow past this; they never shrink it.
pub const DIFF_GUTTER_MIN_WIDTH: f32 = (DIFF_TEXT_SIZE * 3. + 14.).round();

/// Gutter width for `digits` at the measured monospace advance. The fixed
/// padding matches the historical `+ 14` and the 9px inset the row paints.
pub fn diff_gutter_width(cell: Pixels, digits: usize) -> Pixels {
    let digits = digits.max(1) as f32;
    (cell * digits + px(14.)).max(px(DIFF_GUTTER_MIN_WIDTH))
}

/// The advance of one monospace cell at the diff font, used to give
/// unwrapped rows a horizontal scroll range without shaping every line.
pub fn code_cell_width(window: &Window, theme: &Theme) -> Pixels {
    let font = mono_font();
    let size = theme.code_px(DIFF_TEXT_SIZE);
    let font_id = window.text_system().resolve_font(&font);
    window
        .text_system()
        .advance(font_id, size, 'M')
        .map(|advance| advance.width)
        .unwrap_or(px(DIFF_TEXT_SIZE * 0.6))
}

fn diff_row_height() -> f32 {
    (DIFF_TEXT_SIZE * 1.5).round()
}

/// One context/addition/deletion row: a single line-number gutter (the new
/// number, falling back to the old one) and syntax-coloured code. `wrap`
/// false lets the row run its natural width; the surface that owns the list
/// then pans horizontally.
pub fn render_code_row(
    line: &review::Line,
    line_index: usize,
    theme: Theme,
    wrap: bool,
    gutter_w: Pixels,
    text: &DiffTextState,
) -> AnyElement {
    let row_height = diff_row_height();
    let (body_bg, gutter_bg, edge, number_color) = match line.kind {
        LineKind::Addition => (
            Some(theme.success.opacity(body_wash(theme))),
            Some(theme.success.opacity(gutter_wash(theme))),
            Some(theme.success),
            theme.success,
        ),
        LineKind::Deletion => (
            Some(theme.error.opacity(body_wash(theme))),
            Some(theme.error.opacity(gutter_wash(theme))),
            Some(theme.error),
            theme.error,
        ),
        _ => (None, None, None, theme.text_muted),
    };
    let shown_line = line.new_line.or(line.old_line);
    let number = shown_line.map(|n| n.to_string()).unwrap_or_default();
    let content = selectable_line(line, line_index, DiffSide::Unified, theme, text);
    div()
        .w_full()
        .min_w_0()
        .min_h(px(row_height))
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .when_some(edge, |row, edge| row.border_l_2().border_color(edge))
        .child(
            div()
                .w(gutter_w)
                .min_h(px(row_height))
                .flex_none()
                .pr(px(9.))
                .flex()
                .justify_end()
                .border_r_1()
                .border_color(theme.border)
                .text_color(number_color)
                .when_some(gutter_bg, |gutter, bg| gutter.bg(bg))
                .child(number),
        )
        .child(
            div()
                .min_h(px(row_height))
                .min_w_0()
                .flex_1()
                .pl(DynamicSpacing::Base12.px(&theme))
                .pr(px(10.))
                .overflow_hidden()
                .when(wrap, |body| body.whitespace_normal())
                .when(!wrap, |body| body.whitespace_nowrap())
                .when_some(body_bg, |body, bg| body.bg(bg))
                .child(content),
        )
        .into_any_element()
}

/// How unified rows map onto a side-by-side view.
#[derive(Default)]
pub struct SplitPairing {
    /// The opposite-side line a deletion or addition is paired with. Git
    /// writes each change block as a run of `-` lines then a run of `+`
    /// lines, so the nth deletion pairs with the nth addition.
    pub partner: Vec<Option<usize>>,
    /// A paired addition the deletion's row already draws, so its own row
    /// collapses to zero height instead of repeating the line.
    pub secondary: Vec<bool>,
}

impl SplitPairing {
    pub fn for_lines(lines: &[review::Line]) -> Self {
        let mut partner = vec![None; lines.len()];
        let mut secondary = vec![false; lines.len()];
        let mut index = 0;
        while index < lines.len() {
            if !matches!(lines[index].kind, LineKind::Deletion) {
                index += 1;
                continue;
            }
            let deletions = index;
            while index < lines.len() && matches!(lines[index].kind, LineKind::Deletion) {
                index += 1;
            }
            let additions = index;
            while index < lines.len() && matches!(lines[index].kind, LineKind::Addition) {
                index += 1;
            }
            let pairs = (additions - deletions).min(index - additions);
            for offset in 0..pairs {
                partner[deletions + offset] = Some(additions + offset);
                secondary[additions + offset] = true;
            }
        }
        Self { partner, secondary }
    }
}

/// One side-by-side row: the old side's line in the left half, the new
/// side's in the right. `None` paints the filler the opposite side leaves.
/// A paired deletion/addition rides one row; context rides both halves.
///
/// `text_scroll` shifts unwrapped code inside each half. The halves
/// themselves stay put, so both columns remain on screen.
pub fn render_split_row(
    left: Option<(&review::Line, usize)>,
    right: Option<(&review::Line, usize)>,
    theme: Theme,
    wrap: bool,
    gutter_w: Pixels,
    text_scroll: Pixels,
    text: &DiffTextState,
) -> AnyElement {
    let row_height = diff_row_height();
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .min_h(px(row_height))
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .child(split_half(
            left,
            DiffSide::Old,
            theme,
            wrap,
            row_height,
            gutter_w,
            text_scroll,
            text,
        ))
        .child(div().w(px(1.)).flex_none().bg(theme.border))
        .child(split_half(
            right,
            DiffSide::New,
            theme,
            wrap,
            row_height,
            gutter_w,
            text_scroll,
            text,
        ))
        .into_any_element()
}

fn split_half(
    line: Option<(&review::Line, usize)>,
    side: DiffSide,
    theme: Theme,
    wrap: bool,
    row_height: f32,
    gutter_w: Pixels,
    text_scroll: Pixels,
    text: &DiffTextState,
) -> AnyElement {
    // Cap each half at half the row. A nowrap line is wider than the pane;
    // without the cap its min-content width pushes the other column off screen.
    let frame = div()
        .flex_1()
        .min_w_0()
        .max_w(gpui::relative(0.5))
        .overflow_hidden()
        .min_h(px(row_height))
        .debug_selector(move || match side {
            DiffSide::Old => "diff-split-old".to_string(),
            DiffSide::New => "diff-split-new".to_string(),
            DiffSide::Unified => "diff-split-unified".to_string(),
        });
    let Some((line, line_index)) = line else {
        return frame.bg(theme.overlay).into_any_element();
    };
    let (body_bg, gutter_bg, number_color) = match line.kind {
        LineKind::Addition => (
            Some(theme.success.opacity(body_wash(theme))),
            Some(theme.success.opacity(gutter_wash(theme))),
            theme.success,
        ),
        LineKind::Deletion => (
            Some(theme.error.opacity(body_wash(theme))),
            Some(theme.error.opacity(gutter_wash(theme))),
            theme.error,
        ),
        _ => (None, None, theme.text_muted),
    };
    let shown_line = if side == DiffSide::Old {
        line.old_line
    } else {
        line.new_line
    };
    let number = shown_line.map(|n| n.to_string()).unwrap_or_default();
    let code = selectable_line(line, line_index, side, theme, text);
    // The scroll shift sits on its own box so a nowrap line can slide under
    // the clip. Wrapped text stays a direct child: an extra box would size
    // to the unwrapped line and the words would never wrap.
    let body = div()
        .min_h(px(row_height))
        .min_w_0()
        .flex_1()
        .px(DynamicSpacing::Base12.px(&theme))
        .overflow_hidden()
        .when_some(body_bg, |body, bg| body.bg(bg));
    let body = if wrap {
        body.whitespace_normal().child(code)
    } else {
        body.whitespace_nowrap()
            .child(div().left(text_scroll).child(code))
    };
    frame
        .flex()
        .child(
            div()
                .w(gutter_w)
                .min_h(px(row_height))
                .flex_none()
                .pr(px(9.))
                .flex()
                .justify_end()
                .text_color(number_color)
                .when_some(gutter_bg, |gutter, bg| gutter.bg(bg))
                .child(number),
        )
        .child(body)
        .into_any_element()
}

fn body_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.20
    } else {
        0.12
    }
}

fn gutter_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.15
    } else {
        0.09
    }
}

fn selectable_line(
    line: &review::Line,
    line_index: usize,
    side: DiffSide,
    theme: Theme,
    text: &DiffTextState,
) -> SelectableDiffText {
    let highlight = text.borrow().highlight(line_index, side, &line.content);
    SelectableDiffText {
        line_index,
        side,
        plain: line.content.clone(),
        text: code_text(line, theme, highlight),
        state: Rc::clone(text),
    }
}

/// Build syntax-colored text for one diff line.
fn code_text(line: &review::Line, theme: Theme, highlight: Option<Range<usize>>) -> StyledText {
    let base = theme.text_secondary;
    let font = mono_font();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut offset = 0usize;
    for token in &line.tokens {
        let start = token.range.start.min(line.content.len());
        let end = token.range.end.min(line.content.len());
        if start > offset {
            runs.push(run(start - offset, base, &font));
        }
        if end > start {
            runs.push(run(end - start, theme.token_color(token.class), &font));
        }
        offset = offset.max(end);
    }
    if offset < line.content.len() {
        runs.push(run(line.content.len() - offset, base, &font));
    }
    if runs.is_empty() {
        runs.push(run(line.content.len(), base, &font));
    }
    let runs = highlight_runs(runs, highlight, selection_color(theme));
    StyledText::new(line.content.clone()).with_runs(runs)
}

fn selection_color(theme: Theme) -> Hsla {
    theme.accent.opacity(if theme.mode == ThemeMode::Dark {
        0.30
    } else {
        0.20
    })
}

/// Syntax-coloured diff text that registers itself for hit-testing. It is not
/// an input: the pane only reads the layout back to place a selection.
struct SelectableDiffText {
    line_index: usize,
    side: DiffSide,
    plain: String,
    text: StyledText,
    state: DiffTextState,
}

impl Element for SelectableDiffText {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, TextLayout);

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (LayoutId, ()) {
        self.text.request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (Hitbox, TextLayout) {
        self.text
            .prepaint(None, inspector_id, bounds, &mut (), window, cx);
        let layout = self.text.layout().clone();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        (hitbox, layout)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (hitbox, layout): &mut (Hitbox, TextLayout),
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        self.text
            .paint(None, inspector_id, bounds, &mut (), &mut (), window, cx);
        let visible = clip_to_mask(bounds, &window.content_mask());
        self.state.borrow_mut().register(
            self.line_index,
            self.side,
            self.plain.clone(),
            layout.clone(),
            visible,
        );
        if hitbox.is_hovered(window) {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }
    }
}

impl IntoElement for SelectableDiffText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn run(len: usize, color: Hsla, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn mono_font() -> Font {
    Font {
        family: theme::code_font_family(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_width_never_shrinks_and_grows_past_three_digits() {
        let cell = px(8.);
        assert_eq!(
            diff_gutter_width(cell, 3),
            px(DIFF_GUTTER_MIN_WIDTH),
            "three digits stay on the historical slot"
        );
        assert_eq!(diff_gutter_width(cell, 1), px(DIFF_GUTTER_MIN_WIDTH));
        let six = diff_gutter_width(cell, 6);
        assert!(six > px(DIFF_GUTTER_MIN_WIDTH));
        assert_eq!(six, px(8. * 6. + 14.));
    }

    fn line(kind: LineKind, content: &str) -> review::Line {
        review::Line {
            file_index: 0,
            old_line: None,
            new_line: None,
            kind,
            content: content.into(),
            tokens: Vec::new(),
        }
    }

    /// A replacement block pairs each deletion with the addition that
    /// follows it; the addition's own row collapses so the line is drawn
    /// once.
    #[test]
    fn split_pairing_pairs_replacements_and_skips_the_second_row() {
        let lines = vec![
            line(LineKind::Context, "ctx"),
            line(LineKind::Deletion, "old a"),
            line(LineKind::Deletion, "old b"),
            line(LineKind::Addition, "new a"),
            line(LineKind::Addition, "new b"),
            line(LineKind::Context, "after"),
            line(LineKind::Addition, "pure add"),
        ];
        let pairing = SplitPairing::for_lines(&lines);
        assert_eq!(
            pairing.partner,
            vec![None, Some(3), Some(4), None, None, None, None]
        );
        assert_eq!(
            pairing.secondary,
            vec![false, false, false, true, true, false, false]
        );
    }

    /// An uneven block leaves the extra deletions unpaired — they render on
    /// the old side alone.
    #[test]
    fn split_pairing_leaves_an_unbalanced_block_unpaired() {
        let lines = vec![
            line(LineKind::Deletion, "one"),
            line(LineKind::Deletion, "two"),
            line(LineKind::Addition, "new"),
        ];
        let pairing = SplitPairing::for_lines(&lines);
        assert_eq!(pairing.partner, vec![Some(2), None, None]);
        assert_eq!(pairing.secondary, vec![false, false, true]);
    }
}
