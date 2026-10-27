//! Text metrics for diagrams.
//!
//! Layout needs a line's width before any element exists, and gpui only
//! shapes text through the window's [`WindowTextSystem`]. The markdown
//! renderer is a plain function with no `window`, so the shell's render pass
//! installs the system into a thread-local and layout reads it back from
//! there. Without an install (unit tests, and markdown built outside the
//! window) widths fall back to a character-count estimate: good enough for a
//! valid layout, never used for a diagram the app paints.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{px, Font, FontStyle, FontWeight, SharedString, TextRun, Window};

use crate::theme;

type Measurer = Rc<dyn Fn(&str, f32) -> f32>;

thread_local! {
    static MEASURE: RefCell<Option<Measurer>> = const { RefCell::new(None) };
}

/// Install the window's text system for this frame's diagram layout.
pub fn install(window: &Window) {
    let text_system = window.text_system().clone();
    let font = Font {
        family: theme::ui_font_family(),
        features: Default::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };
    let measurer: Measurer = Rc::new(move |text: &str, size: f32| {
        if text.is_empty() {
            return 0.0;
        }
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: gpui::Hsla::transparent_black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            text_system.shape_line(SharedString::from(text.to_string()), px(size), &[run], None);
        f32::from(line.width)
    });
    MEASURE.with(|slot| *slot.borrow_mut() = Some(measurer));
}

/// Width of one line of diagram label text at `size`.
pub fn line_width(text: &str, size: f32) -> f32 {
    MEASURE.with(|slot| match slot.borrow().as_ref() {
        Some(measure) => measure(text, size),
        None => estimate_width(text, size),
    })
}

/// Rough advance for the interface face when no text system is installed:
/// Latin glyphs average a little over half an em, full-width scripts a whole
/// one, so an estimate never comes in far under the shaped width.
fn estimate_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|ch| if ch.is_ascii() { 0.55 } else { 1.0 })
        .sum::<f32>()
        * size
}
