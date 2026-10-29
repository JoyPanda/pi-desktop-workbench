//! Text metrics for diagrams.
//!
//! Layout needs a line's width before any element exists. Measuring goes
//! through [`crate::text_measure`], which the shell installs once per frame.

use gpui::Window;

/// Install the window's text system for this frame's diagram layout.
pub fn install(window: &Window) {
    crate::text_measure::install(window);
}

/// Width of one line of diagram label text at `size`.
pub fn line_width(text: &str, size: f32) -> f32 {
    crate::text_measure::line_width(text, size)
}
