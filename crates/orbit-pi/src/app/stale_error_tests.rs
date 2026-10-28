//! Regression tests for issue #76: a failure that could never be cleared.
//!
//! The banner was retired by comparing its *text* against a translated
//! `"<Command> failed"` prefix. Two families of failures never carry that
//! shape — a write that failed before reaching pi, and any failure whose text
//! was rendered in a different language than the retry is read under — so the
//! banner outlived the problem it described. Clearing now keys off the command
//! that raised it, so the text is irrelevant.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeKey};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeKey::ORBIT));
        cx.new(OrbitApp::new)
    })
}

/// A command that failed to reach pi carries `runtime.send_failed` text, which
/// does not begin with its own humanized label. The success that follows it
/// must still retire the banner.
#[gpui::test]
fn a_send_failure_is_cleared_by_a_later_success_on_the_same_command(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.set_error_from(
                "get_state",
                tr!("runtime.send_failed", label = "get_state", error = "closed"),
            );
            let banner = app.error.clone().expect("the failed send raised one");
            assert!(
                !banner.starts_with(&helpers::humanize_command("get_state")),
                "precondition: this is exactly the text shape the old prefix \
                 match could never recognize, got {banner:?}"
            );

            app.clear_error_from("get_state");
            assert!(
                app.error.is_none(),
                "the retried command's success must retire the banner it raised"
            );
        });
    });
}

/// Clearing is decided by the raising command, never by the wording: a banner
/// rendered in another language still clears. The string here is a stand-in
/// for any translated text the prefix match would have failed on.
#[gpui::test]
fn a_banner_clears_regardless_of_its_wording(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.set_error_from(
                "set_model",
                "\u{6a21}\u{578b}\u{8bbe}\u{7f6e}\u{5931}\u{8d25}: quota",
            );
            app.clear_error_from("set_model");
            assert!(
                app.error.is_none(),
                "a banner raised under one language must clear under another"
            );
        });
    });
}

/// The clear stays narrow: an unrelated success leaves a banner alone, so a
/// fixed `set_model` cannot make an outstanding `abort` failure disappear.
#[gpui::test]
fn a_success_on_another_command_keeps_the_banner(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.set_error_from("abort", "Abort failed: no run");
            app.clear_error_from("set_model");
            assert_eq!(
                app.error.as_deref(),
                Some("Abort failed: no run"),
                "only the raising command clears its own banner"
            );
        });
    });
}

/// A banner raised by an event rather than a command belongs to no command, so
/// no command's success may silently clear it — it is dismissed or superseded.
#[gpui::test]
fn an_event_raised_banner_is_not_cleared_by_a_command_success(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.set_error(tr!("events.process_exited"));
            assert!(
                app.error_source.is_none(),
                "an event-raised banner names no raising command"
            );
            app.clear_error_from("get_state");
            assert!(
                app.error.is_some(),
                "the exit banner describes the process, not `get_state`: a \
                 successful poll must not make an unreachable runtime look OK"
            );
        });
    });
}

/// Raising a new failure re-arms the source record: a later success on the
/// *previous* command must not clear the banner that replaced it.
#[gpui::test]
fn a_new_failure_replaces_the_previous_source(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.set_error_from("get_state", "Get state failed: closed");
            app.set_error(tr!("events.process_exited"));
            app.clear_error_from("get_state");
            assert!(
                app.error.is_some(),
                "the outstanding banner was raised by an event, not by the earlier `get_state` failure"
            );

            // And an untagged failure clears the source rather than keeping it
            // armed against a banner that no longer describes it.
            app.set_error_from("abort", "Abort failed");
            app.set_error("unrelated");
            assert!(app.error_source.is_none());
        });
    });
}
