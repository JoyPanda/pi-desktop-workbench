//! Regression test: a wheel over a docked right panel stays in that panel.
//!
//! The dock is a column beside the conversation, not a scroll area nested
//! inside it. Nothing about a wheel over Review, Files, or a subagent
//! transcript belongs to the message list behind it — but the subagent pane
//! used to hand its leftover delta to `MessageScrollerState`, a habit from the
//! days when it replaced the transcript in the same slot. A notch at the end
//! of the dock's own scroll yanked the conversation the reader was reading.

use super::*;
use crate::theme::{Theme, ThemeKey};
use gpui::{point, px, ScrollDelta, ScrollWheelEvent};
use serde_json::json;

/// One wheel event at `at`: positive `dy` travels toward the top of the
/// content, matching a real wheel rolled away from the reader.
fn wheel(
    cx: &mut gpui::VisualTestContext,
    app: &Entity<OrbitApp>,
    at: gpui::Point<gpui::Pixels>,
    dy: f32,
) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        ..Default::default()
    });
    let viewport = cx.update(|window, _| window.viewport_size());
    let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
}

/// An app with a conversation long enough to scroll and one subagent leaf whose
/// inspect output overflows the dock.
fn dock_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            for index in 0..40 {
                app.transcript
                    .append_user_message(&format!("line {index}"), Vec::new());
            }
            app.subagents.apply_snapshot_value(&json!({
                "runs": [{ "id": "r1", "label": "Helper", "state": "running" }]
            }));
            app.open_subagent_tab("r1", cx);
            let request = app.subagents.alloc_request_id();
            app.subagents.note_inflight("r1", &request, 40);
            let messages: Vec<serde_json::Value> = (0..40)
                .map(|index| json!({ "kind": "text", "text": format!("step {index}") }))
                .collect();
            app.subagents
                .apply_inspect_value(&json!({ "requestId": request, "messages": messages }));
            // Park the reader away from the live edge, where a leaked delta
            // would visibly move the conversation.
            app.transcript.scroll_to_message(2);
        });
    });
    app
}

#[gpui::test]
fn a_wheel_over_the_subagent_tab_never_moves_the_conversation(cx: &mut gpui::TestAppContext) {
    let app = dock_app(cx);
    let mut cx = cx.add_empty_window();
    let viewport = cx.update(|window, _| window.viewport_size());
    for _ in 0..2 {
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    }

    let body = cx
        .debug_bounds("subagent-body")
        .expect("the dock body laid out");
    let at = body.center();
    let handle = cx.update(|_, cx| {
        app.read(cx)
            .subagent_scrolls
            .get("r1")
            .cloned()
            .expect("the pane tracked its scroll offset")
    });
    let conversation = cx.update(|_, cx| app.read(cx).transcript.first_visible_index());
    assert!(
        handle.max_offset().height > px(0.),
        "the pane must be scrollable for this test to mean anything"
    );

    // The very first notch arrives when the pane already sits at its top, so
    // it is the `split` case that used to be handed to the transcript: no room
    // to travel, and no earlier event to make it look like a continuing
    // stream. (A pane whose content fits is the same case with `max == 0`.)
    // Pinning the offset instead of wheeling there keeps the gesture fresh.
    handle.set_offset(point(px(0.), px(0.)));
    let viewport = cx.update(|window, _| window.viewport_size());
    let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    wheel(&mut cx, &app, at, 400.);
    assert_eq!(
        handle.offset().y,
        px(0.),
        "the pane stops at its own top instead of travelling past it"
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).transcript.first_visible_index()),
        conversation,
        "the leftover notch at the edge must be dropped, not handed on"
    );

    // Notches with room left scroll the pane and nothing else.
    wheel(&mut cx, &app, at, -400.);
    assert!(
        handle.offset().y < px(0.),
        "the dock's own wheel moves the dock: {}",
        handle.offset().y
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).transcript.first_visible_index()),
        conversation,
        "a wheel over the dock must not follow the conversation"
    );

    // And at the far edge the overshoot is dropped too, however many notches
    // arrive behind it.
    for _ in 0..4 {
        wheel(&mut cx, &app, at, -400.);
    }
    assert_eq!(
        handle.offset().y,
        -handle.max_offset().height,
        "the pane stops at its own bottom"
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).transcript.first_visible_index()),
        conversation,
        "notches past the bottom belong to nobody"
    );
}
