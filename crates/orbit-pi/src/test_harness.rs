//! Helpers shared by the `VisualTestContext::draw` probe harnesses.

use gpui::VisualTestContext;

/// Dispatch space-separated `keystrokes` one key at a time, repainting the
/// probe before each press.
///
/// A key event resolves the focused node against the *rendered* frame, and a
/// probe painted by `VisualTestContext::draw` is not part of the window's root
/// view. When the window repaints — which it does after every dispatched key,
/// because that key's own `notify` dirties it — the probe's frame is replaced
/// by the root view's, so the next key in the same batch has no focused node
/// to reach. The app never sees that (its own root view *is* the surface), so
/// the harness puts its frame back before every press, the way the running
/// window paints one frame per key.
pub(crate) fn type_keys(
    cx: &mut VisualTestContext,
    keystrokes: &str,
    mut repaint: impl FnMut(&mut VisualTestContext),
) {
    for keystroke in keystrokes.split(' ') {
        repaint(cx);
        cx.simulate_keystrokes(keystroke);
    }
}
