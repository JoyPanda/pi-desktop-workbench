//! Header X restores queued text locally; `clear_queue` must not resurrect it.
use super::fake_pi::{self, FakePi};
use super::*;
use crate::theme::{Theme, ThemeKey};
use gpui::{point, px, Modifiers, MouseButton, MouseUpEvent};

fn mouse_up() -> MouseUpEvent {
    MouseUpEvent {
        button: MouseButton::Left,
        position: point(px(0.), px(0.)),
        modifiers: Modifiers::none(),
        click_count: 1,
    }
}

fn empty_queue() -> serde_json::Value {
    serde_json::json!({ "steering": [], "followUp": [] })
}

fn follow_ups(items: &[&str]) -> serde_json::Value {
    serde_json::json!({ "steering": [], "followUp": items })
}

fn app_with_fake_pi(cx: &mut gpui::VisualTestContext) -> (Entity<OrbitApp>, FakePi) {
    let fake = FakePi::start("queue-restore");
    let app = cx.update(|_, cx| {
        cx.set_global(Theme::for_id(ThemeKey::ORBIT));
        cx.new(OrbitApp::new)
    });
    cx.update(|_, cx| {
        app.update(cx, |app, _| fake_pi::adopt_alive(app, &fake));
    });
    (app, fake)
}

fn reply(
    app: &mut OrbitApp,
    command: &str,
    success: bool,
    data: Option<&serde_json::Value>,
    error: Option<&str>,
    cx: &mut gpui::Context<OrbitApp>,
) {
    reply_id(app, command, success, data, error, "test", cx);
}

fn reply_id(
    app: &mut OrbitApp,
    command: &str,
    success: bool,
    data: Option<&serde_json::Value>,
    error: Option<&str>,
    id: &str,
    cx: &mut gpui::Context<OrbitApp>,
) {
    let mut refresh = false;
    app.on_response(command, success, data, error, id, &mut refresh, cx);
}

fn last_enqueue_id(app: &OrbitApp) -> String {
    app.queue_wire
        .in_flight
        .last()
        .map(|entry| entry.id.clone())
        .expect("an enqueue is in flight")
}

/// Clicking X puts the queued follow-up back in the composer and leaves the
/// run going — it is not an abort.
#[gpui::test]
fn clear_queue_restores_follow_up_without_aborting(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("summarize when done".into());

            app.on_clear_queue(&click, window, cx);

            assert_eq!(app.input.read(cx).text(), "summarize when done");
            assert!(app.queue.is_empty());
            assert_eq!(app.queue_wire.clear_in_flight, 1);
            assert!(app.is_running(), "clearing the queue must not abort");
        });
    });
}

/// A draft already in the composer is kept, queued text first.
#[gpui::test]
fn clear_queue_keeps_a_typed_draft(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.steering.push("s1".into());
            app.queue.follow_up.push("f1".into());
            app.input
                .update(cx, |input, cx| input.set_text("draft", cx));

            app.on_clear_queue(&click, window, cx);

            assert_eq!(app.input.read(cx).text(), "s1\n\nf1\n\ndraft");
            assert!(app.queue.is_empty());
            assert!(app.is_running());
        });
    });
}

/// pi's real `clear_queue` reply echoes the dropped items after an empty
/// `queue_update`. That echo must not resurrect the bar or duplicate text.
#[gpui::test]
fn clear_queue_echo_does_not_resurrect_the_bar(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("keep me".into());
            app.on_clear_queue(&click, window, cx);
            assert!(app.queue.is_empty());
            assert_eq!(app.input.read(cx).text(), "keep me");

            app.apply_queue_update(&empty_queue());
            let dropped = follow_ups(&["keep me"]);
            reply(app, "clear_queue", true, Some(&dropped), None, cx);

            assert!(
                app.queue.is_empty(),
                "the dropped echo must not refill the bar"
            );
            assert_eq!(
                app.input.read(cx).text(),
                "keep me",
                "the echo must not rewrite or duplicate the composer"
            );
            assert_eq!(app.queue_wire.clear_in_flight, 0);
        });
    });
}

/// A stale non-empty `queue_update` arriving while `clear_queue` is out must
/// not bring the dismissed row back.
#[gpui::test]
fn stale_queue_update_is_ignored_while_clear_is_in_flight(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 1);

            app.apply_queue_update(&follow_ups(&["old"]));
            assert!(
                app.queue.is_empty(),
                "a pre-clear queue_update must not resurrect the row"
            );
            assert_eq!(app.input.read(cx).text(), "old");
        });
    });
}

/// Stop then a stalled force-stop restores queued text once, not twice.
#[gpui::test]
fn abort_then_force_stop_restores_queued_text_once(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("later".into());
            app.request_abort(cx);
            assert_eq!(app.input.read(cx).text(), "later");
            assert!(app.queue.is_empty());

            app.force_stop_run(cx);
            assert_eq!(
                app.input.read(cx).text(),
                "later",
                "escalating the stop must not prepend a second copy"
            );
        });
    });
}

/// X then Enter before `clear_queue` returns: the empty `queue_update` from
/// the clear must not wipe the newly queued follow-up.
#[gpui::test]
fn enter_after_x_keeps_the_new_follow_up(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 1);

            app.submit_as("new".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue.follow_up, vec!["new".to_string()]);
            assert_eq!(
                app.queue_wire.deferred.len(),
                1,
                "the new follow-up waits until clear_queue returns"
            );
            assert!(app.queue_wire.in_flight.is_empty());

            app.apply_queue_update(&empty_queue());
            assert_eq!(
                app.queue.follow_up,
                vec!["new".to_string()],
                "clear_queue's empty update must not wipe the new follow-up"
            );

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert_eq!(app.queue_wire.deferred.len(), 0);
            assert_eq!(app.queue_wire.in_flight.len(), 1);
            app.apply_queue_update(&follow_ups(&["new"]));
            let flushed_id = last_enqueue_id(app);
            reply_id(app, "follow_up", true, None, None, &flushed_id, cx);
            assert!(app.queue_wire.in_flight.is_empty());
            assert_eq!(app.queue_wire.clear_in_flight, 0);
        });
    });
}

/// X → Enter → X → Enter while both clears are in flight: the first clear's
/// reply must not drop `clear_in_flight` to 0 while the second is still out.
#[gpui::test]
fn double_unqueue_keeps_both_clears_in_flight(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("A".into());
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 1);

            app.on_clear_queue(&click, window, cx);
            assert_eq!(
                app.queue_wire.clear_in_flight, 2,
                "the second X must not clobber the first clear's in-flight count"
            );
            app.submit_as("C".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue.follow_up, vec!["C".to_string()]);
            assert_eq!(app.queue_wire.deferred.len(), 1);
            assert!(app.queue_wire.in_flight.is_empty());

            app.apply_queue_update(&empty_queue());
            assert_eq!(app.queue.follow_up, vec!["C".to_string()]);

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 1);
            app.apply_queue_update(&follow_ups(&["B"]));
            assert_eq!(
                app.queue.follow_up,
                vec!["C".to_string()],
                "a stale B must not resurrect while clear #2 is out"
            );

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert_eq!(app.queue_wire.in_flight.len(), 1);
            app.apply_queue_update(&follow_ups(&["C"]));
            assert_eq!(app.queue.follow_up, vec!["C".to_string()]);
            let flushed_id = last_enqueue_id(app);
            reply_id(app, "follow_up", true, None, None, &flushed_id, cx);
            assert!(app.queue_wire.in_flight.is_empty());
        });
    });
}

/// Enter then X while pi can reorder `clear_queue` ahead of `follow_up`:
/// Orbit must send a follow-up clear so pi cannot retain the sneaked item.
#[gpui::test]
fn enter_then_x_reclears_when_follow_up_resolves_after_clear(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("sneaked".into(), SendMode::FollowUp, cx);
            let sneaked_id = last_enqueue_id(app);
            assert_eq!(app.queue_wire.in_flight.len(), 1);

            app.on_clear_queue(&click, window, cx);
            assert!(app.queue_wire.has_cancelled_in_flight());
            assert_eq!(app.queue_wire.clear_in_flight, 1);
            assert_eq!(app.input.read(cx).text(), "sneaked");

            app.apply_queue_update(&empty_queue());
            app.apply_queue_update(&follow_ups(&["sneaked"]));
            assert!(
                app.queue.is_empty(),
                "the sneaked update must stay ignored while clear is out"
            );

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            app.apply_queue_update(&follow_ups(&["sneaked"]));
            assert!(
                app.queue.is_empty(),
                "a reclear is still armed, so the sneaked row must not return"
            );

            reply_id(app, "follow_up", true, None, None, &sneaked_id, cx);
            assert_eq!(
                app.queue_wire.clear_in_flight, 1,
                "resolving the raced follow_up must send another clear"
            );
            assert!(!app.queue_wire.reclear_needed);
            assert!(app.queue.is_empty());
        });
    });
}

/// A parked session mid-clear must keep the follow-up queued after X→Enter
/// and ignore the empty `queue_update` that `clear_queue` emits while away.
#[gpui::test]
fn parked_session_mid_clear_keeps_the_new_follow_up(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    let path = PathBuf::from("/tmp/orbit-queue-restore/parked.jsonl");
    let session = SessionInfo {
        path: path.clone(),
        id: "parked".into(),
        cwd: PathBuf::from("/tmp/orbit-queue-restore"),
        title: "Parked".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
        draft: None,
    };
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.current_session_path = Some(path.clone());
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);
            assert_eq!(app.queue_wire.clear_in_flight, 1);
            assert_eq!(app.queue_wire.deferred.len(), 1);

            app.park_active_session();
            {
                let parked = app.lives.get_mut(&path).expect("parked under its path");
                assert_eq!(parked.queue.follow_up, vec!["B".to_string()]);
                assert_eq!(parked.queue_wire.clear_in_flight, 1);
                parked.apply_queue_update(&empty_queue());
                assert_eq!(
                    parked.queue.follow_up,
                    vec!["B".to_string()],
                    "empty queue_update while parked must not wipe B"
                );
            }

            app.switch_to_session(session, true, cx);
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);
            assert_eq!(app.queue_wire.clear_in_flight, 1);
            app.apply_queue_update(&empty_queue());
            assert_eq!(
                app.queue.follow_up,
                vec!["B".to_string()],
                "resume must restore the in-flight clear so the empty update is ignored"
            );
        });
    });
}

/// A failed `clear_queue` still drops `clear_in_flight` and surfaces the
/// banner; a later success clears it.
#[gpui::test]
fn failed_clear_queue_decrements_and_shows_the_banner(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("keep me".into());
            app.on_clear_queue(&click, window, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 1);

            reply(app, "clear_queue", false, None, Some("nope"), cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert_eq!(app.error_source.as_deref(), Some("clear_queue"));
            assert!(app.error.is_some(), "the failure banner must surface");

            app.apply_queue_update(&follow_ups(&["other"]));
            assert_eq!(
                app.queue.follow_up,
                vec!["other".to_string()],
                "updates apply again after the failed clear decrements"
            );

            app.queue_wire.note_clear_sent();
            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert!(
                app.error_source.is_none(),
                "a later success must clear the banner"
            );
        });
    });
}

/// Double-clicking X when the bar is already empty must not send another RPC.
#[gpui::test]
fn empty_queue_clear_sends_no_rpc(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.on_clear_queue(&click, window, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert!(app.error.is_none());
            assert!(app.error_source.is_none());
        });
    });
}

/// Enter A (in-flight) → X → Enter B: `clear_queue` can overtake A's
/// `follow_up`. B stays deferred across both clears; A must not come back.
#[gpui::test]
fn enter_a_then_x_then_enter_b_reclears_before_flushing_b(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("A".into(), SendMode::FollowUp, cx);
            let a_id = last_enqueue_id(app);
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);
            assert_eq!(app.queue_wire.deferred.len(), 1);
            assert!(app.queue_wire.has_cancelled_in_flight());

            app.apply_queue_update(&follow_ups(&["A"]));
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert_eq!(
                app.queue_wire.deferred.len(),
                1,
                "B waits until the cancelled A is gone and recleared"
            );
            app.apply_queue_update(&follow_ups(&["A"]));
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);

            reply_id(app, "follow_up", true, None, None, &a_id, cx);
            assert_eq!(
                app.queue_wire.clear_in_flight, 1,
                "A landing after the first clear must send a reclear"
            );
            assert_eq!(app.queue_wire.deferred.len(), 1);
            app.apply_queue_update(&follow_ups(&["A", "B"]));
            assert_eq!(
                app.queue.follow_up,
                vec!["B".to_string()],
                "A must not resurrect beside B while reclear is out"
            );

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert_eq!(app.queue_wire.deferred.len(), 0);
            assert_eq!(app.queue_wire.in_flight.len(), 1);
            app.apply_queue_update(&follow_ups(&["B"]));
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);
        });
    });
}

/// Enter A → X → Enter B, A's `follow_up` succeeds while the first
/// `clear_queue` is still out: do not arm a second clear; flush B when the
/// first clear returns.
#[gpui::test]
fn follow_up_success_while_clear_in_flight_does_not_reclear(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("A".into(), SendMode::FollowUp, cx);
            let a_id = last_enqueue_id(app);
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 1);

            reply_id(app, "follow_up", true, None, None, &a_id, cx);
            assert_eq!(
                app.queue_wire.clear_in_flight, 1,
                "a clear is already out, so A's success must not send another"
            );
            assert!(!app.queue_wire.reclear_needed);
            assert_eq!(app.queue_wire.deferred.len(), 1);

            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert_eq!(app.queue_wire.deferred.len(), 0);
            assert_eq!(app.queue_wire.in_flight.len(), 1);
        });
    });
}

/// A cancelled pre-X `follow_up` that fails must not steal the post-X
/// follow-up out of the bar or into the composer.
#[gpui::test]
fn failed_cancelled_follow_up_does_not_steal_the_new_row(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("A".into(), SendMode::FollowUp, cx);
            let a_id = last_enqueue_id(app);
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert!(app.input.read(cx).text().is_empty());
            assert_eq!(app.queue.follow_up, vec!["B".to_string()]);

            reply_id(app, "follow_up", false, None, Some("nope"), &a_id, cx);
            assert_eq!(
                app.queue.follow_up,
                vec!["B".to_string()],
                "B must stay in the bar"
            );
            assert!(
                app.input.read(cx).text().is_empty(),
                "failed cancelled A must not restore into the empty composer"
            );
        });
    });
}

/// A failed non-cancelled steer is removed from the bar and restored to an
/// empty composer.
#[gpui::test]
fn failed_steer_restores_to_the_composer(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("steer me".into(), SendMode::Steer, cx);
            let id = last_enqueue_id(app);
            assert_eq!(app.queue.steering, vec!["steer me".to_string()]);

            reply_id(app, "steer", false, None, Some("nope"), &id, cx);
            assert!(app.queue.steering.is_empty());
            assert_eq!(app.input.read(cx).text(), "steer me");
        });
    });
}

/// A failed non-cancelled follow-up is removed from the bar and restored to
/// an empty composer.
#[gpui::test]
fn failed_follow_up_restores_to_the_composer(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("later".into(), SendMode::FollowUp, cx);
            let id = last_enqueue_id(app);

            reply_id(app, "follow_up", false, None, Some("nope"), &id, cx);
            assert!(app.queue.follow_up.is_empty());
            assert_eq!(app.input.read(cx).text(), "later");
        });
    });
}

/// AgentSettled while a post-X follow-up is deferred restores it to the
/// composer and does not flush it after `clear_queue` returns.
#[gpui::test]
fn agent_settled_restores_deferred_follow_up(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue_wire.deferred.len(), 1);

            app.settle_local_queue(cx);
            assert_eq!(app.input.read(cx).text(), "B");
            assert!(app.queue.is_empty());
            assert!(app.queue_wire.deferred.is_empty());
            assert_eq!(app.queue_wire.clear_in_flight, 1);

            app.busy = false;
            reply(app, "clear_queue", true, Some(&empty_queue()), None, cx);
            assert!(
                app.queue_wire.in_flight.is_empty(),
                "settled deferred B must not flush as follow_up onto an idle pi"
            );
            assert_eq!(app.input.read(cx).text(), "B");
        });
    });
}

/// A handled prompt (no `agent_settled`) must restore deferred B the same way.
#[gpui::test]
fn handled_prompt_restores_deferred_follow_up(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.prompt_in_flight = Some("p1".into());
            app.execution.prompt_id = Some("p1".into());
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);

            let handled = serde_json::json!({ "disposition": "handled" });
            reply_id(app, "prompt", true, Some(&handled), None, "p1", cx);
            assert_eq!(app.input.read(cx).text(), "B");
            assert!(app.queue_wire.deferred.is_empty());
            assert!(!app.busy);
        });
    });
}

/// A rejected prompt restores deferred B instead of flushing it idle.
#[gpui::test]
fn rejected_prompt_restores_deferred_follow_up(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);

            app.on_command_failure("prompt", Some("model not available"), cx);
            assert_eq!(app.input.read(cx).text(), "B");
            assert!(app.queue_wire.deferred.is_empty());
            assert!(!app.busy);
        });
    });
}

/// In-place session replace restores unsent B and drops in-flight enqueues so
/// they cannot reclear into the new session.
#[gpui::test]
fn session_replace_restores_deferred_and_drops_in_flight(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("A".into(), SendMode::FollowUp, cx);
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert_eq!(app.queue_wire.deferred.len(), 1);
            assert!(!app.queue_wire.in_flight.is_empty());

            app.discard_inflight_queue_on_replace(cx);
            assert_eq!(app.input.read(cx).text(), "B");
            assert!(app.queue.is_empty());
            assert!(app.queue_wire.deferred.is_empty());
            assert!(app.queue_wire.in_flight.is_empty());
            assert!(!app.queue_wire.reclear_needed);
        });
    });
}

fn parked_session_info(path: PathBuf) -> SessionInfo {
    SessionInfo {
        path,
        id: "parked".into(),
        cwd: PathBuf::from("/tmp/orbit-queue-restore"),
        title: "Parked".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
        draft: None,
    }
}

/// Parked AgentSettled with deferred B clears the ghost bar and restores B
/// into the session draft so resume shows it in the composer.
#[gpui::test]
fn parked_agent_settled_restores_deferred_to_the_draft(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    let path = PathBuf::from("/tmp/orbit-queue-restore/parked-settle.jsonl");
    let session = parked_session_info(path.clone());
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.current_session_path = Some(path.clone());
            app.queue.follow_up.push("old".into());
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);

            app.park_active_session();
            let restore = {
                let parked = app.lives.get_mut(&path).expect("parked under its path");
                parked.busy = false;
                let text = parked.on_agent_settled_queue();
                assert!(
                    parked.queue.is_empty(),
                    "settling must not leave a ghost queued row"
                );
                assert!(parked.queue_wire.deferred.is_empty());
                text
            };
            assert_eq!(restore.as_deref(), Some("B"));
            app.stash_parked_queue_restore(Some(&path), None, restore.unwrap());

            app.switch_to_session(session, true, cx);
            assert!(
                app.queue.is_empty(),
                "resume must not show a queued bar on an idle session"
            );
            assert_eq!(app.input.read(cx).text(), "B");
        });
    });
}

/// A cancelled `follow_up` that succeeds while parked must reclear on that
/// parked client.
#[gpui::test]
fn parked_cancelled_follow_up_reclears(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    let path = PathBuf::from("/tmp/orbit-queue-restore/parked-reclear.jsonl");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.current_session_path = Some(path.clone());
            app.submit_as("A".into(), SendMode::FollowUp, cx);
            let a_id = last_enqueue_id(app);
            app.on_clear_queue(&click, window, cx);

            app.park_active_session();
            let parked = app.lives.get_mut(&path).expect("parked under its path");
            let _ = parked.on_queue_response("clear-1", "clear_queue", true);
            assert_eq!(parked.queue_wire.clear_in_flight, 0);
            assert!(parked.queue_wire.has_cancelled_in_flight());
            let _ = parked.on_queue_response(&a_id, "follow_up", true);
            assert_eq!(
                parked.queue_wire.clear_in_flight, 1,
                "resolving the raced follow_up while parked must reclear"
            );
            assert!(!parked.queue_wire.reclear_needed);
        });
    });
}

/// Whitespace-only queued text must not clobber a typed draft, but still
/// sends `clear_queue`.
#[gpui::test]
fn whitespace_queue_does_not_clobber_the_draft(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("   ".into());
            app.input
                .update(cx, |input, cx| input.set_text("draft", cx));
            app.on_clear_queue(&click, window, cx);
            assert_eq!(app.input.read(cx).text(), "draft");
            assert!(app.queue.is_empty());
            assert_eq!(app.queue_wire.clear_in_flight, 1);
        });
    });
}

/// A second abort inside the retry window still restores a newly queued
/// follow-up and does not escalate to force-stop.
#[gpui::test]
fn second_abort_inside_grace_restores_a_new_follow_up(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, _fake) = app_with_fake_pi(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.queue.follow_up.push("A".into());
            app.request_abort(cx);
            assert_eq!(app.input.read(cx).text(), "A");
            assert!(app.abort_sent_at.is_some());
            let client_was = app.client.is_some();

            // A follow-up that landed in the bar after the first abort still
            // has to come back; sending it through `submit_as` would clear
            // the restored draft.
            app.queue.follow_up.push("B".into());
            app.request_abort(cx);
            assert_eq!(app.input.read(cx).text(), "B\n\nA");
            assert!(
                app.abort_sent_at.is_some(),
                "inside the retry window the first abort stands"
            );
            assert_eq!(app.client.is_some(), client_was);
        });
    });
}

/// Process death and a replacement client must not inherit in-flight counts.
#[gpui::test]
fn process_exit_and_adopt_zero_the_queue_wire(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let (app, fake) = app_with_fake_pi(cx);
    let click = mouse_up();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.busy = true;
            app.submit_as("A".into(), SendMode::FollowUp, cx);
            app.on_clear_queue(&click, window, cx);
            app.submit_as("B".into(), SendMode::FollowUp, cx);
            assert!(app.queue_wire.clear_in_flight > 0);
            assert!(!app.queue_wire.deferred.is_empty());

            app.reset_queue();
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert!(app.queue_wire.in_flight.is_empty());
            assert!(app.queue_wire.deferred.is_empty());
            assert!(!app.queue_wire.reclear_needed);
            assert!(app.queue.is_empty());

            app.queue_wire.note_clear_sent();
            app.queue_wire
                .note_enqueue_sent("1".into(), SendMode::FollowUp, "x".into());
            fake_pi::adopt_alive(app, &fake);
            assert_eq!(app.queue_wire.clear_in_flight, 0);
            assert!(app.queue_wire.in_flight.is_empty());
        });
    });
}
