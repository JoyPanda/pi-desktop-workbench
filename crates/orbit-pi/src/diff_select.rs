//! Read-only selection over Review diff rows.
//!
//! Code rows register their layout while painting. The pane's canvas owns the
//! pointer gestures. Copy reads the snapshot, so a range that scrolls off
//! screen still copies. Nothing here is an editor: there is no caret and no
//! text input.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    point, px, Bounds, ContentMask, DispatchPhase, Hitbox, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, TextLayout, Window,
};

use crate::review::{Line, LineKind};
use crate::transcript_view::{byte_boundary, word_range_at};

/// Which half of a row the selection lives on. Unified diffs use [`Self::Unified`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffSide {
    Unified,
    Old,
    New,
}

/// One end of a selection: a snapshot line and a byte offset into its content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffCaret {
    pub line: usize,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Granularity {
    Character,
    Word,
}

struct Selection {
    side: DiffSide,
    anchor: DiffCaret,
    focus: DiffCaret,
    granularity: Granularity,
    /// Word under the first press, so a word drag keeps that word whole.
    origin_word: Range<usize>,
}

struct LastClick {
    at: Instant,
    line: usize,
    side: DiffSide,
    word: Range<usize>,
}

/// A code row painted this frame.
struct Block {
    line: usize,
    side: DiffSide,
    text: String,
    layout: TextLayout,
    bounds: Bounds<Pixels>,
}

/// Shared with every code row and the diff canvas.
pub struct DiffTextSelection {
    blocks: Vec<Block>,
    selection: Option<Selection>,
    dragging: bool,
    last_click: Option<LastClick>,
    menu: Option<Point<Pixels>>,
}

pub type DiffTextState = Rc<RefCell<DiffTextSelection>>;

impl DiffTextSelection {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            selection: None,
            dragging: false,
            last_click: None,
            menu: None,
        }
    }

    pub fn state() -> DiffTextState {
        Rc::new(RefCell::new(Self::new()))
    }

    /// Drop the previous frame's hit targets. Called from the canvas prepaint,
    /// which runs before any row paints, so the paint that follows refills
    /// `blocks` with only the rows on screen.
    pub fn clear_blocks(&mut self) {
        self.blocks.clear();
    }

    pub fn register(
        &mut self,
        line: usize,
        side: DiffSide,
        text: String,
        layout: TextLayout,
        bounds: Bounds<Pixels>,
    ) {
        if let Some(existing) = self
            .blocks
            .iter_mut()
            .find(|block| block.line == line && block.side == side)
        {
            *existing = Block {
                line,
                side,
                text,
                layout,
                bounds,
            };
            return;
        }
        self.blocks.push(Block {
            line,
            side,
            text,
            layout,
            bounds,
        });
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.dragging = false;
        self.last_click = None;
        self.menu = None;
    }

    pub fn menu_position(&self) -> Option<Point<Pixels>> {
        self.menu
    }

    pub fn dismiss_menu(&mut self) {
        self.menu = None;
    }

    /// Byte range to wash on `line` / `side`, if this row is inside the selection.
    pub fn highlight(&self, line: usize, side: DiffSide, text: &str) -> Option<Range<usize>> {
        let selection = self.selection.as_ref()?;
        if selection.side != side {
            return None;
        }
        highlight_range(text, line, selection.anchor, selection.focus)
    }

    pub fn selected_text(&self, lines: &[Line], collapsed: &HashSet<usize>) -> Option<String> {
        let selection = self.selection.as_ref()?;
        diff_selection_text(
            lines,
            collapsed,
            selection.side,
            selection.anchor,
            selection.focus,
        )
    }

    fn block_at(&self, position: Point<Pixels>) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| block.bounds.contains(&position))
    }

    fn nearest_on_side(&self, position: Point<Pixels>, side: DiffSide) -> Option<usize> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.side == side)
            .min_by(|(_, a), (_, b)| {
                let da = block_distance(a.bounds, position);
                let db = block_distance(b.bounds, position);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
    }

    fn hit(&self, index: usize, position: Point<Pixels>) -> (usize, bool) {
        let block = &self.blocks[index];
        let local = point(
            position.x.clamp(block.bounds.left(), block.bounds.right()),
            position.y.clamp(block.bounds.top(), block.bounds.bottom()),
        );
        match block.layout.index_for_position(local) {
            Ok(offset) => (offset.min(block.text.len()), false),
            Err(offset) => {
                let offset = offset.min(block.text.len());
                let past_end = match block.layout.position_for_index(offset) {
                    Some(caret) => local.x >= caret.x,
                    None => true,
                };
                (offset, past_end)
            }
        }
    }

    /// Left press. Returns whether the pane should redraw.
    pub fn begin_press(&mut self, position: Point<Pixels>, now: Instant) -> bool {
        self.menu = None;
        let Some(index) = self.block_at(position) else {
            let had = self.selection.take().is_some();
            self.dragging = false;
            self.last_click = None;
            return had;
        };
        let (offset, past_end) = self.hit(index, position);
        let block_line = self.blocks[index].line;
        let block_side = self.blocks[index].side;
        let text = self.blocks[index].text.clone();
        let probe = unit_probe(&text, offset, past_end);
        let word = word_range_at(&text, probe);
        let repeat = self.last_click.as_ref().is_some_and(|previous| {
            now.saturating_duration_since(previous.at) <= DOUBLE_CLICK
                && previous.line == block_line
                && previous.side == block_side
                && previous.word == word
        });
        self.last_click = Some(LastClick {
            at: now,
            line: block_line,
            side: block_side,
            word: word.clone(),
        });
        if repeat && !word.is_empty() {
            self.selection = Some(Selection {
                side: block_side,
                anchor: DiffCaret {
                    line: block_line,
                    offset: word.start,
                },
                focus: DiffCaret {
                    line: block_line,
                    offset: word.end,
                },
                granularity: Granularity::Word,
                origin_word: word,
            });
        } else {
            let caret = DiffCaret {
                line: block_line,
                offset,
            };
            self.selection = Some(Selection {
                side: block_side,
                anchor: caret,
                focus: caret,
                granularity: Granularity::Character,
                origin_word: offset..offset,
            });
        }
        self.dragging = true;
        true
    }

    pub fn extend(&mut self, position: Point<Pixels>) -> bool {
        if !self.dragging {
            return false;
        }
        let Some(side) = self.selection.as_ref().map(|selection| selection.side) else {
            return false;
        };
        let Some(index) = self
            .block_at(position)
            .filter(|index| self.blocks[*index].side == side)
            .or_else(|| self.nearest_on_side(position, side))
        else {
            return false;
        };
        let (offset, past_end) = self.hit(index, position);
        let line = self.blocks[index].line;
        let text = self.blocks[index].text.clone();
        let Some(selection) = self.selection.as_mut() else {
            return false;
        };
        match selection.granularity {
            Granularity::Character => {
                selection.focus = DiffCaret { line, offset };
            }
            Granularity::Word => {
                let probe = unit_probe(&text, offset, past_end);
                let word = word_range_at(&text, probe);
                let before = line < selection.anchor.line
                    || (line == selection.anchor.line && word.start < selection.origin_word.start);
                if before {
                    selection.anchor.offset = selection.origin_word.end;
                    selection.focus = DiffCaret {
                        line,
                        offset: word.start,
                    };
                } else {
                    selection.anchor.offset = selection.origin_word.start;
                    selection.focus = DiffCaret {
                        line,
                        offset: word.end,
                    };
                }
            }
        }
        true
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    pub fn open_menu(&mut self, position: Point<Pixels>) -> bool {
        if self.block_at(position).is_none() && self.selection.is_none() {
            return false;
        }
        self.menu = Some(position);
        true
    }
}

impl Default for DiffTextSelection {
    fn default() -> Self {
        Self::new()
    }
}

const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// Window listeners for the diff canvas. A press outside the diff clears the
/// selection. A press on the diff that is not on code clears it too, without
/// swallowing the click, so file headers and gap buttons still run.
pub fn register_diff_pointer(
    state: DiffTextState,
    bounds: Bounds<Pixels>,
    hitbox: Hitbox,
    window: &mut Window,
) {
    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseDownEvent, phase, window, _| {
            if phase != DispatchPhase::Capture {
                return;
            }
            if state.borrow().menu.is_some() && event.button == MouseButton::Left {
                return;
            }
            if !bounds.contains(&event.position) {
                if event.button == MouseButton::Left {
                    let mut selection = state.borrow_mut();
                    let had = selection.selection.is_some();
                    selection.clear_selection();
                    if had {
                        window.refresh();
                    }
                }
                return;
            }
            if !hitbox.is_hovered(window) {
                return;
            }
            let redraw = match event.button {
                MouseButton::Left => state
                    .borrow_mut()
                    .begin_press(event.position, Instant::now()),
                MouseButton::Right => state.borrow_mut().open_menu(event.position),
                _ => false,
            };
            if redraw {
                window.refresh();
            }
        }
    });
    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseMoveEvent, phase, window, _| {
            if phase != DispatchPhase::Capture || !state.borrow().dragging {
                return;
            }
            if state.borrow_mut().extend(event.position) {
                window.refresh();
            }
        }
    });
    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseUpEvent, phase, _, _| {
            if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                return;
            }
            state.borrow_mut().end_drag();
        }
    });
}

/// Clip `bounds` to the paint mask so a wrapped-off or overflowing run cannot
/// be hit in the neighbouring split column.
pub fn clip_to_mask(bounds: Bounds<Pixels>, mask: &ContentMask<Pixels>) -> Bounds<Pixels> {
    let mask = mask.bounds;
    let left = bounds.left().max(mask.left());
    let top = bounds.top().max(mask.top());
    let right = bounds.right().min(mask.right());
    let bottom = bounds.bottom().min(mask.bottom());
    Bounds {
        origin: point(left, top),
        size: gpui::size((right - left).max(px(0.)), (bottom - top).max(px(0.))),
    }
}

pub fn diff_selection_text(
    lines: &[Line],
    collapsed: &HashSet<usize>,
    side: DiffSide,
    anchor: DiffCaret,
    focus: DiffCaret,
) -> Option<String> {
    let (start, end) = normalize(anchor, focus);
    if start == end {
        return None;
    }
    let last = lines.len().saturating_sub(1);
    let mut parts = Vec::new();
    for index in start.line..=end.line.min(last) {
        let Some(line) = lines.get(index) else {
            continue;
        };
        if collapsed.contains(&line.file_index) {
            continue;
        }
        if !side_takes(side, &line.kind) {
            continue;
        }
        let from = if index == start.line {
            byte_boundary(&line.content, start.offset)
        } else {
            0
        };
        let to = if index == end.line {
            byte_boundary(&line.content, end.offset)
        } else {
            line.content.len()
        };
        if from < to {
            parts.push(line.content[from..to].to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

pub fn highlight_range(
    text: &str,
    line: usize,
    anchor: DiffCaret,
    focus: DiffCaret,
) -> Option<Range<usize>> {
    let (start, end) = normalize(anchor, focus);
    if line < start.line || line > end.line {
        return None;
    }
    let from = if line == start.line {
        byte_boundary(text, start.offset)
    } else {
        0
    };
    let to = if line == end.line {
        byte_boundary(text, end.offset)
    } else {
        text.len()
    };
    (from < to).then_some(from..to)
}

fn normalize(anchor: DiffCaret, focus: DiffCaret) -> (DiffCaret, DiffCaret) {
    if (focus.line, focus.offset) < (anchor.line, anchor.offset) {
        (focus, anchor)
    } else {
        (anchor, focus)
    }
}

fn side_takes(side: DiffSide, kind: &LineKind) -> bool {
    match side {
        DiffSide::Unified => {
            matches!(
                kind,
                LineKind::Context | LineKind::Addition | LineKind::Deletion
            )
        }
        DiffSide::Old => matches!(kind, LineKind::Context | LineKind::Deletion),
        DiffSide::New => matches!(kind, LineKind::Context | LineKind::Addition),
    }
}

fn block_distance(bounds: Bounds<Pixels>, position: Point<Pixels>) -> f32 {
    let dx: f32 = (position.x - position.x.clamp(bounds.left(), bounds.right()))
        .abs()
        .into();
    let dy: f32 = (position.y - position.y.clamp(bounds.top(), bounds.bottom()))
        .abs()
        .into();
    dx + dy
}

fn unit_probe(text: &str, offset: usize, past_end: bool) -> usize {
    let offset = offset.min(text.len());
    if !past_end || offset == 0 {
        return offset;
    }
    let offset = byte_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::LineKind;

    fn code(kind: LineKind, content: &str) -> Line {
        Line {
            file_index: 0,
            old_line: None,
            new_line: None,
            kind,
            content: content.into(),
            tokens: Vec::new(),
        }
    }

    fn header() -> Line {
        code(LineKind::FileHeader, "")
    }

    #[test]
    fn selection_joins_partial_lines_and_skips_headers() {
        let lines = vec![
            header(),
            code(LineKind::Deletion, "alpha"),
            code(LineKind::HunkHeader, "@@"),
            code(LineKind::Addition, "beta-line"),
        ];
        let text = diff_selection_text(
            &lines,
            &HashSet::new(),
            DiffSide::Unified,
            DiffCaret { line: 1, offset: 2 },
            DiffCaret { line: 3, offset: 4 },
        );
        assert_eq!(text.as_deref(), Some("pha\nbeta"));
    }

    #[test]
    fn an_empty_caret_copies_nothing() {
        let lines = vec![code(LineKind::Context, "same")];
        let caret = DiffCaret { line: 0, offset: 1 };
        assert_eq!(
            diff_selection_text(&lines, &HashSet::new(), DiffSide::Unified, caret, caret),
            None
        );
    }

    #[test]
    fn split_sides_do_not_cross() {
        let lines = vec![
            code(LineKind::Deletion, "old"),
            code(LineKind::Addition, "new"),
            code(LineKind::Context, "keep"),
        ];
        let text = diff_selection_text(
            &lines,
            &HashSet::new(),
            DiffSide::Old,
            DiffCaret { line: 0, offset: 0 },
            DiffCaret { line: 2, offset: 4 },
        );
        assert_eq!(text.as_deref(), Some("old\nkeep"));
        let text = diff_selection_text(
            &lines,
            &HashSet::new(),
            DiffSide::New,
            DiffCaret { line: 0, offset: 0 },
            DiffCaret { line: 2, offset: 4 },
        );
        assert_eq!(text.as_deref(), Some("new\nkeep"));
    }

    #[test]
    fn collapsed_files_are_left_out() {
        let mut hidden = code(LineKind::Addition, "secret");
        hidden.file_index = 1;
        let lines = vec![code(LineKind::Addition, "shown"), hidden];
        let text = diff_selection_text(
            &lines,
            &HashSet::from([1]),
            DiffSide::Unified,
            DiffCaret { line: 0, offset: 0 },
            DiffCaret { line: 1, offset: 6 },
        );
        assert_eq!(text.as_deref(), Some("shown"));
    }

    #[test]
    fn highlight_stays_on_char_boundaries() {
        let range = highlight_range(
            "héllo",
            0,
            DiffCaret { line: 0, offset: 1 },
            DiffCaret { line: 0, offset: 4 },
        );
        let range = range.expect("range");
        assert!("héllo".is_char_boundary(range.start));
        assert!("héllo".is_char_boundary(range.end));
        assert_eq!(&"héllo"[range], "él");
    }
}
