//! Shaped text widths for layout that runs before an element exists.
//!
//! GPUI only shapes text through the window's text system. The shell's render
//! pass installs that system into a thread-local; callers that build elements
//! without a `Window` (markdown, diagrams) read it back here. With nothing
//! installed — unit tests, and any build outside the window — widths fall
//! back to a character estimate.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{px, FontStyle, FontWeight, SharedString, TextRun, Window};

use crate::theme;

type RunMeasurer = Rc<dyn Fn(&str, f32, &[TextRun]) -> f32>;

thread_local! {
    static MEASURE: RefCell<Option<RunMeasurer>> = const { RefCell::new(None) };
}

/// Install the window's text system for this frame. A later install replaces
/// it. [`uninstall`] clears it so a test thread does not leak shaping into a
/// later test that expects the estimate.
pub fn install(window: &Window) {
    let text_system = window.text_system().clone();
    let measurer: RunMeasurer = Rc::new(move |text, size, runs| {
        if text.is_empty() {
            return 0.0;
        }
        let line =
            text_system.shape_line(SharedString::from(text.to_string()), px(size), runs, None);
        f32::from(line.width)
    });
    MEASURE.with(|slot| *slot.borrow_mut() = Some(measurer));
}

/// Drop a previously installed text system on this thread.
///
/// Tests call this so a shaped width does not leak into a later test that
/// expects the estimate. The windowed app replaces the slot every frame.
#[cfg_attr(not(test), allow(dead_code))]
pub fn uninstall() {
    MEASURE.with(|slot| *slot.borrow_mut() = None);
}

/// Width of one already-single-line string. `runs` cover `text` exactly.
/// The caller splits on newlines: GPUI's shaper rejects them.
pub fn line_width_runs(text: &str, size: f32, runs: &[TextRun]) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    MEASURE.with(|slot| match slot.borrow().as_ref() {
        Some(measure) => measure(text, size, runs),
        None => estimate_runs(text, size, runs),
    })
}

/// Width of one line of UI-font body text. Diagram labels use this.
pub fn line_width(text: &str, size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let run = TextRun {
        len: text.len(),
        font: gpui::Font {
            family: theme::ui_font_family(),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        },
        color: gpui::Hsla::transparent_black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    line_width_runs(text, size, &[run])
}

/// Latin glyphs average a little over half an em on the UI face, a bit more
/// on the code face, and a full em for everything else.
fn estimate_runs(text: &str, size: f32, runs: &[TextRun]) -> f32 {
    if runs.is_empty() {
        return estimate_slice(text, size, false);
    }
    let code = theme::code_font_family();
    let mut offset = 0usize;
    let mut width = 0.0f32;
    for run in runs {
        let end = (offset + run.len).min(text.len());
        let Some(slice) = text.get(offset..end) else {
            break;
        };
        let monospace = run.font.family == code;
        width += estimate_slice(slice, size, monospace);
        offset = end;
    }
    width
}

fn estimate_slice(text: &str, size: f32, monospace: bool) -> f32 {
    let ascii_em = if monospace { 0.6 } else { 0.55 };
    text.chars()
        .map(|ch| if ch.is_ascii() { ascii_em } else { 1.0 })
        .sum::<f32>()
        * size
}
