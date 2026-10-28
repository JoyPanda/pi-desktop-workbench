//! The composer's send control is one state machine, not three buttons: what
//! it shows must match what a send will actually do. These tests pin the two
//! rules that are easy to break — the payload check (which decides whether the
//! control is live) and the running state (which decides queue vs steer).

use super::*;
use crate::app::composer_ops::CompletionIntent;
use crate::composer_send::SendMode;
use crate::theme::{Theme, ThemeKey};

/// Put the composer on `text` with the completion menu open on a single known
/// file. The catalog is seeded directly and stamped as belonging to the
/// current workspace, so `ensure_mention_files` leaves it alone instead of
/// re-scanning the real tree.
fn open_menu_on(app: &mut OrbitApp, text: &str, cx: &mut Context<OrbitApp>) {
    app.mention_files_workspace = app.current_workspace.clone();
    app.mention_files = vec!["src/main.rs".to_string()];
    app.input.update(cx, |input, cx| input.set_text(text, cx));
    let mut state = app.autocomplete.borrow_mut();
    state.open = true;
    state.count = 1;
    state.highlighted = 0;
}

fn app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeKey::ORBIT));
        cx.new(OrbitApp::new)
    })
}

/// A collapsed paste is a sendable message on its own. The control used to
/// check only text and attachments, so a pill-only composer rendered as
/// disabled while Enter still sent it — the button lied about what it would do.
#[gpui::test]
fn a_pasted_pill_alone_makes_the_send_control_live(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            assert!(!app.send_has_payload(cx), "nothing typed yet");

            app.input.update(cx, |input, cx| input.set_text("", cx));
            app.pasted_texts
                .push(PastedText::new("fn main() {}".into()));

            assert!(
                app.send_has_payload(cx),
                "a pasted pill is a payload even with an empty box"
            );
        });
    });
}

/// An attachment alone is likewise sendable (the old rule had this one right).
#[gpui::test]
fn an_attachment_alone_makes_the_send_control_live(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            assert!(!app.send_has_payload(cx));
            app.attachments.push(Attachment {
                name: "shot.png".into(),
                mime: "image/png".into(),
                data: String::new(),
                preview: None,
                original_len: 0,
                stored_len: 0,
                compressed: false,
            });
            assert!(app.send_has_payload(cx));

            // A file chip is a payload too: it folds into the prompt as an
            // `@path` mention, exactly like an image rides as bytes.
            app.attachments.clear();
            app.files.push(AttachedFile {
                path: PathBuf::from("/tmp/notes.md"),
                name: "notes.md".into(),
                size: 12,
            });
            assert!(app.send_has_payload(cx));
        });
    });
}

/// Whitespace is not a message: the control stays disabled so a stray space
/// cannot arm a button that would no-op.
#[gpui::test]
fn whitespace_is_not_a_payload(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.input
                .update(cx, |input, cx| input.set_text("   \n\t ", cx));
            assert!(!app.send_has_payload(cx));
        });
    });
}

/// The control's "running" state must agree with the delivery policy: sending
/// mid-run queues or steers, and `is_running` is what decides. A control keyed
/// to `busy` alone would show Send while `submit_as` queued a follow-up.
#[gpui::test]
fn the_send_state_follows_the_delivery_policy(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _cx| {
            assert!(!app.is_running(), "idle: plain send");

            // A pending reply is a run in flight even before tokens arrive —
            // the same state `submit_as` reads to choose follow_up over prompt.
            app.transcript.expect_reply();
            assert!(app.is_running(), "pending reply counts as running");
            assert!(!app.busy, "…even though `busy` has not been set yet");

            // Idle sends always start a prompt, whatever the mode says.
            let (_, label) = SendMode::Steer.command(false, "hello".into(), None);
            assert_eq!(label, "prompt");
            // Mid-run the mode picks the command.
            let (_, label) = SendMode::FollowUp.command(true, "hello".into(), None);
            assert_eq!(label, "follow_up");
            let (_, label) = SendMode::Steer.command(true, "hello".into(), None);
            assert_eq!(label, "steer");
        });
    });
}

/// Finishing a mention and reaching for Send must send. `@src/main.rs` with
/// the menu still open used to "accept" the completion the user had already
/// typed — replacing it with itself plus a space — so the first click on Send
/// looked dead and the message never left.
#[gpui::test]
fn an_already_complete_mention_does_not_swallow_the_send(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            open_menu_on(app, "@src/main.rs", cx);

            assert!(
                !app.commit_autocomplete_if_open(CompletionIntent::Send, cx),
                "an exact match must fall through so the send happens"
            );
            assert_eq!(
                app.input.read(cx).text(),
                "@src/main.rs",
                "nothing was rewritten"
            );
        });
    });
}

/// …but Tab still owns the key. Tab is focus traversal when the gate declines,
/// so an exact match must report handled rather than moving focus behind the
/// user's back.
#[gpui::test]
fn an_already_complete_mention_is_still_handled_for_tab(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            open_menu_on(app, "@src/main.rs", cx);
            assert!(
                app.commit_autocomplete_if_open(CompletionIntent::Accept, cx),
                "Tab must not fall through to focus traversal"
            );
        });
    });
}

/// A real completion still wins: typing `@src/ma` with `@src/main.rs`
/// highlighted must complete the token, not send a half-typed mention.
#[gpui::test]
fn a_partial_mention_is_still_completed(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            open_menu_on(app, "@src/ma", cx);
            app.autocomplete.borrow_mut().highlighted = 0;

            assert!(
                app.commit_autocomplete_if_open(CompletionIntent::Send, cx),
                "a partial token is completed, not sent"
            );
            assert_eq!(app.input.read(cx).text(), "@src/main.rs ");
        });
    });
}

/// A click outside the menu closes it on the spot. Waiting for the next
/// heartbeat made the same Send click commit sometimes and send other times,
/// depending on how long the button was held.
#[gpui::test]
fn clicking_outside_closes_the_menu_immediately(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.set_global(Theme::for_id(ThemeKey::ORBIT));
        cx.new(OrbitApp::new)
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            open_menu_on(app, "@src/main.rs", cx);
            assert!(app.autocomplete.borrow().open);

            app.on_autocomplete_outside_down(
                &gpui::MouseDownEvent {
                    button: gpui::MouseButton::Left,
                    position: gpui::point(gpui::px(0.), gpui::px(0.)),
                    modifiers: gpui::Modifiers::none(),
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            );

            assert!(
                !app.autocomplete.borrow().open,
                "the menu is closed before the click reaches the button"
            );
        });
    });
}
