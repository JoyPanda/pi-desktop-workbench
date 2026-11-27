//! The model a process was started on is checked, not assumed.
//!
//! Orbit hands the composer's pick to pi as `--provider` / `--model`, and pi
//! resolves that flag as a *pattern*: an exact id first, then a fuzzy match.
//! A catalog entry pi's registry no longer lists exactly therefore boots a
//! neighbour instead, and a `get_state` painted straight to the chips read as
//! "the model changed by itself". The RPC `set_model` resolves by provider
//! *and* id, so it is both the correction and the proof that the pick is gone.
//!
//! The hold that keeps the chips on the pick is only ever ended by an answer:
//! a report that matches, the correction's own reply, a manual pick that
//! supersedes it, or the process going away. A *report* can never be the
//! reason the chips stay held, or one substitute would pin them for good.

use super::fake_pi::{adopt_alive, prepare, FakePi};
use super::*;

/// A `get_state` body naming one model, the way pi reports it.
fn state(provider: &str, id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "model": { "provider": provider, "id": id, "name": name },
        "thinkingLevel": "low",
    })
}

/// A `get_state` body on the pick at a given thinking level.
fn state_at(level: &str) -> serde_json::Value {
    serde_json::json!({
        "model": { "provider": "anthropic", "id": "chosen", "name": "Chosen" },
        "thinkingLevel": level,
    })
}

fn asked(app: &OrbitApp, label: &str) -> bool {
    app.sent_commands.iter().any(|sent| sent == label)
}

/// The app on a live process whose argv asked for `anthropic/chosen` at the
/// thinking level `high`.
fn app_booted_on_a_pick(cx: &mut gpui::VisualTestContext, fake: &FakePi) -> Entity<OrbitApp> {
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            app.current_workspace = Some(std::env::temp_dir());
            app.spawn_bin = Some(fake.bin());
            app.available_models = vec![ModelEntry {
                id: "chosen".into(),
                name: "Chosen".into(),
                provider: "anthropic".into(),
                context_window: None,
                thinking_levels: vec!["off".into(), "low".into(), "high".into()],
            }];
            adopt_alive(app, fake);
            // `adopt_client` clears both; a spawn landing sets them from the
            // ticket, which is the state under test here.
            app.startup_model_expected = Some(crate::session_defaults::SessionDefault {
                provider: Some("anthropic".into()),
                model_id: Some("chosen".into()),
                thinking: Some("high".into()),
            });
            app.startup_model_fix = None;
            // What the composer painted when the reader picked the model.
            app.model_id = "chosen".into();
            app.model_provider = "anthropic".into();
            app.model_label = "Chosen".into();
            app.thinking_label = "high".into();
        });
    });
    app
}

fn reply(
    app: &mut OrbitApp,
    command: &str,
    success: bool,
    data: Option<&serde_json::Value>,
    error: Option<&str>,
    id: &str,
    cx: &mut Context<OrbitApp>,
) {
    let mut refresh_sessions = false;
    app.on_response(command, success, data, error, id, &mut refresh_sessions, cx);
}

/// pi came up on a fuzzy neighbour of the pick: the chips stay on the pick
/// while one exact `set_model` is out, and follow pi once it agrees.
#[gpui::test]
fn a_loosely_resolved_startup_model_is_corrected_exactly(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("loose-startup-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);

            assert_eq!(
                app.model_id, "chosen",
                "the chip must not flip to the model pi resolved instead"
            );
            assert_eq!(app.model_label, "Chosen");
            assert_eq!(app.thinking_label, "high", "the level rides on the pick");
            assert!(asked(app, "set_model"), "one exact correction went out");
            assert!(
                app.startup_model_fix.is_some(),
                "and it is tracked by its own command id"
            );
            assert!(app.startup_model_expected.is_some());

            // A second report of the same substitute neither asks again nor
            // paints: only the correction's reply ends the hold.
            app.sent_commands.clear();
            reply(app, "get_state", true, Some(&substitute), None, "s2", cx);
            assert_eq!(app.model_id, "chosen", "still held");
            assert!(!asked(app, "set_model"), "one attempt per process");

            // pi is on the pick now, but the correction has not answered: a
            // report cannot consume the request, or the success would fail
            // its id guard and skip the level re-ask.
            let corrected = state("anthropic", "chosen", "Chosen");
            reply(app, "get_state", true, Some(&corrected), None, "s3", cx);
            assert!(app.startup_model_fix.is_some(), "the reply still owns the ending");
            assert_eq!(app.model_id, "chosen");
            assert_eq!(app.model_label, "Chosen");

            // The correction's own success ends the check; the chips follow.
            let fix = app.startup_model_fix.clone().expect("the correction");
            reply(app, "set_model", true, Some(&corrected), None, &fix, cx);
            assert!(app.startup_model_expected.is_none());
            assert!(app.startup_model_fix.is_none());
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// The correction's own success is what releases the hold — not a later report
/// that happens to agree. pi lands a switch with the thinking level reset to
/// the model's own default, so the level the argv carried is asked for again.
#[gpui::test]
fn an_accepted_correction_releases_the_hold_and_reasks_the_level(
    cx: &mut gpui::TestAppContext,
) {
    let fake = FakePi::start("accepted-correction");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");
            app.sent_commands.clear();
            // pi accepted, and reports the level it reset to on the switch.
            reply(
                app,
                "set_model",
                true,
                Some(&serde_json::json!({
                    "provider": "anthropic", "id": "chosen", "name": "Chosen"
                })),
                None,
                &fix,
                cx,
            );

            assert!(app.startup_model_expected.is_none(), "the request is satisfied");
            assert!(app.startup_model_fix.is_none());
            assert!(
                asked(app, "set_thinking_level"),
                "the level the argv carried is asked back"
            );

            // The state read behind the reply now paints freely.
            reply(app, "get_state", true, Some(&substitute), None, "s2", cx);
            assert_eq!(
                app.model_id, "chosen-flash",
                "with the check over, pi's report is the truth again"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// A level the corrected model does not offer is not asked for: pi would
/// refuse it and raise a banner for something the reader never clicked.
#[gpui::test]
fn a_correction_does_not_reask_a_level_the_model_lacks(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("correction-bad-level");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // The catalog entry offers no ladder worth choosing from.
            app.available_models = vec![ModelEntry {
                id: "chosen".into(),
                name: "Chosen".into(),
                provider: "anthropic".into(),
                context_window: None,
                thinking_levels: vec!["off".into()],
            }];
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");
            app.sent_commands.clear();

            reply(app, "set_model", true, Some(&substitute), None, &fix, cx);

            assert!(app.startup_model_expected.is_none());
            assert!(
                !asked(app, "set_thinking_level"),
                "a level this model does not offer is not sent"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// The reader can pick another model while the correction is in flight. That
/// pick supersedes the argv request: without this the chips would stay pinned
/// to the argv model over an explicit choice, for the life of the process.
#[gpui::test]
fn a_manual_pick_supersedes_the_argv_request(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("pick-during-correction");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            assert!(app.startup_model_fix.is_some(), "a correction is in flight");

            app.sent_commands.clear();
            app.set_model("chosen-flash".into(), "anthropic".into(), cx);

            assert!(
                app.startup_model_expected.is_none(),
                "an explicit pick ends the argv check"
            );
            assert!(app.startup_model_fix.is_none());
            assert!(
                asked(app, "set_model"),
                "the pick went to pi, which is what paints the chip"
            );

            // And the report of that pick now paints instead of being held.
            reply(app, "get_state", true, Some(&substitute), None, "s2", cx);
            assert_eq!(app.model_id, "chosen-flash");
            assert_eq!(app.model_label, "Chosen Flash");
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// pi has no such model at all: the hold is released, the reader is told, and
/// the catalog is re-read so the picker drops the entry pi refuses.
#[gpui::test]
fn a_refused_startup_correction_releases_the_chips(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("refused-startup-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");

            reply(
                app,
                "set_model",
                false,
                None,
                Some("Model not found: anthropic/chosen"),
                &fix,
                cx,
            );

            assert!(
                app.startup_model_expected.is_none(),
                "a refusal ends the check"
            );
            assert!(app.startup_model_fix.is_none());
            assert!(
                app.toasts.items().iter().any(|toast| toast.title
                    == tr!("session.startup_model_missing", model = "chosen")),
                "the reader is told the pick is gone"
            );
            assert!(
                asked(app, "get_available_models"),
                "the catalog that offered it is re-read"
            );

            // The refusal's own `get_state` now paints pi's truth.
            reply(app, "get_state", true, Some(&substitute), None, "s2", cx);
            assert_eq!(app.model_id, "chosen-flash");
            assert_eq!(app.model_label, "Chosen Flash");
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// A refusal for any other reason (no key, a signed-out provider) still
/// releases the hold, but must not claim the model is gone: the command's own
/// banner is the honest report, and the entry stays in the picker.
#[gpui::test]
fn a_refusal_that_is_not_a_missing_model_says_nothing_about_the_catalog(
    cx: &mut gpui::TestAppContext,
) {
    let fake = FakePi::start("refused-auth");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");
            app.sent_commands.clear();

            reply(
                app,
                "set_model",
                false,
                None,
                Some("No API key for anthropic/chosen"),
                &fix,
                cx,
            );

            assert!(app.startup_model_expected.is_none(), "the hold is released");
            assert!(
                app.toasts.items().iter().all(|toast| toast.title
                    != tr!("session.startup_model_missing", model = "chosen")),
                "an auth failure is not a missing model"
            );
            assert!(!asked(app, "get_available_models"), "the catalog is not thrown away");
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// A process that came up on the model it was asked for needs no correction,
/// and its report paints normally.
#[gpui::test]
fn a_startup_model_pi_already_runs_on_is_left_alone(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("exact-startup-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let exact = state("anthropic", "chosen", "Chosen");
            reply(app, "get_state", true, Some(&exact), None, "s1", cx);

            assert!(!asked(app, "set_model"), "an exact boot needs no correction");
            assert!(app.startup_model_expected.is_none());
            assert!(app.startup_model_fix.is_none());
            assert_eq!(app.model_label, "Chosen");
            assert_eq!(app.thinking_label, "low", "pi's own level paints");
            assert!(
                !asked(app, "set_thinking_level"),
                "the boot's --thinking stands, so nothing is re-asked"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// A provider-less request is a bare pattern with nothing exact to ask for,
/// so pi's resolution stands instead of holding a pick nothing can move pi to.
#[gpui::test]
fn a_startup_model_without_a_provider_is_not_corrected(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("providerless-startup-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.startup_model_expected = Some(crate::session_defaults::SessionDefault {
                provider: None,
                model_id: Some("chosen".into()),
                thinking: None,
            });
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);

            assert!(!asked(app, "set_model"));
            assert!(app.startup_model_expected.is_none());
            assert_eq!(
                app.model_id, "chosen-flash",
                "with nothing exact to ask for, pi's answer is the truth"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// A report that names no complete model cannot contradict the request, and
/// gives `set_model` nothing exact to ask for: it is ignored, not held on.
#[gpui::test]
fn an_incomplete_report_neither_holds_nor_corrects(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("incomplete-report");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let no_provider = serde_json::json!({
                "model": { "id": "chosen-flash", "name": "Chosen Flash" },
                "thinkingLevel": "low",
            });
            reply(app, "get_state", true, Some(&no_provider), None, "s1", cx);

            assert!(!asked(app, "set_model"));
            assert!(
                app.startup_model_expected.is_some(),
                "an unusable report leaves the request to the next one"
            );
            assert_eq!(
                app.model_id, "chosen-flash",
                "the chips are not held against an incomplete report"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// The reader can pick another model while the correction is in flight, and
/// that pick's own refusal is a different command: it must not be read as the
/// startup one's, or the hold would be released onto a model nobody asked for.
#[gpui::test]
fn an_unrelated_model_refusal_keeps_the_hold(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("unrelated-refusal");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");

            reply(
                app,
                "set_model",
                false,
                None,
                Some("Model not found: anthropic/something-else"),
                "other-1",
                cx,
            );

            assert_eq!(
                app.startup_model_fix.as_deref(),
                Some(fix.as_str()),
                "the startup correction is still the one outstanding"
            );
            assert!(app.startup_model_expected.is_some());
            assert!(
                app.toasts.items().iter().all(|toast| toast.title
                    != tr!("session.startup_model_missing", model = "chosen")),
                "another choice's refusal says nothing about the pick"
            );
            reply(app, "get_state", true, Some(&substitute), None, "s2", cx);
            assert_eq!(app.model_id, "chosen", "still held");
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// The check rides with the process, not with the page: a child that came up
/// on a neighbour while the reader was elsewhere is still put right on return.
#[gpui::test]
fn parking_a_process_keeps_its_argv_request(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("parked-request");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let path = std::env::temp_dir().join("parked-request.jsonl");
            app.current_session_path = Some(path.clone());
            app.startup_model_fix = Some("fix-1".into());
            // The catalog this process reports is the one it booted with, and
            // that stamp has to come back with it or a later reply would file
            // an old catalog as current.
            app.process_sources_stamp = 4242;

            app.park_active_session();

            assert_eq!(app.lives.get(&path).expect("parked").sources_stamp, 4242);

            assert!(
                app.startup_model_expected.is_none(),
                "the page no longer holds it"
            );
            let parked = app.lives.get(&path).expect("the process is parked");
            assert_eq!(
                parked.startup_model_expected.as_ref().and_then(|slot| slot.model_id.as_deref()),
                Some("chosen"),
                "the request travelled with the process"
            );

            app.switch_to_session(
                SessionInfo {
                    path: path.clone(),
                    id: "parked".into(),
                    cwd: std::env::temp_dir(),
                    title: "parked".into(),
                    first_message: "hello".into(),
                    modified: SystemTime::UNIX_EPOCH,
                    draft: None,
                },
                false,
                cx,
            );

            assert_eq!(
                app.startup_model_expected
                    .as_ref()
                    .and_then(|slot| slot.model_id.as_deref()),
                Some("chosen"),
                "and came back with it"
            );
            assert_eq!(
                app.startup_model_fix.as_deref(),
                Some("fix-1"),
                "an unanswered correction rides with the process: pi may still accept it"
            );
            assert_eq!(
                app.process_sources_stamp, 4242,
                "and so did the stamp its catalog is filed under"
            );
            app.drop_client();
        });
    });
}

/// Leaving the process releases the hold: a dead pi has no model to correct,
/// and the next page must not inherit a chip nobody can verify.
#[gpui::test]
fn dropping_the_process_drops_a_pending_correction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("dropped-startup-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            assert!(app.startup_model_fix.is_some());

            app.drop_client();

            assert!(app.startup_model_expected.is_none());
            assert!(app.startup_model_fix.is_none());
        });
    });
}

/// pi lands a model switch *before* its RPC reply is written, so a matching
/// `get_state` can arrive while the correction is still outstanding. That
/// report must not consume the request: the success still has to pass its id
/// guard, or the level the argv carried is silently never re-asked.
#[gpui::test]
fn a_matching_report_before_the_ack_keeps_the_correction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("report-before-ack");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");

            // pi already switched; the report beat the reply.
            let corrected = state("anthropic", "chosen", "Chosen");
            reply(app, "get_state", true, Some(&corrected), None, "s2", cx);
            assert_eq!(
                app.startup_model_fix.as_deref(),
                Some(fix.as_str()),
                "the correction's own reply is still what ends the hold"
            );
            assert!(app.startup_model_expected.is_some());

            app.sent_commands.clear();
            reply(app, "set_model", true, Some(&corrected), None, &fix, cx);
            assert!(app.startup_model_expected.is_none());
            assert_eq!(
                app.sent_commands
                    .iter()
                    .filter(|label| *label == "set_thinking_level")
                    .count(),
                1,
                "the argv level is asked back exactly once"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// A manual level pick while the correction is in flight supersedes the level
/// the argv carried — but not the model correction itself. Without this the
/// accepted correction would re-ask the *argv* level over the reader's choice.
#[gpui::test]
fn a_manual_level_pick_supersedes_the_argv_level(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("manual-level-during-fix");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");

            app.set_thinking_level("low".into(), cx);
            assert_eq!(
                app.startup_model_expected
                    .as_ref()
                    .and_then(|slot| slot.thinking.as_deref()),
                None,
                "the reader's level replaced the argv's"
            );
            assert!(app.startup_model_fix.is_some(), "the model check goes on");

            app.sent_commands.clear();
            reply(app, "set_model", true, Some(&substitute), None, &fix, cx);
            assert!(
                !asked(app, "set_thinking_level"),
                "the accepted correction must not resend the argv level"
            );
        });
    });

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| app.drop_client());
    });
}

/// D17 keeps a parked child alive, so pi can accept the correction while the
/// reader is elsewhere. The parked drain must consume that reply — resume
/// asking again would be a second `set_model` for one request — and the switch
/// pi landed reset the thinking level, so the argv level is re-asked once.
#[gpui::test]
fn a_correction_answered_while_parked_is_not_asked_again(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("parked-correction-ack");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_booted_on_a_pick(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let path = std::env::temp_dir().join("parked-correction-ack.jsonl");
            app.current_session_path = Some(path.clone());
            let substitute = state("anthropic", "chosen-flash", "Chosen Flash");
            reply(app, "get_state", true, Some(&substitute), None, "s1", cx);
            let fix = app.startup_model_fix.clone().expect("a correction");

            app.park_active_session();
            assert_eq!(
                app.lives.get(&path).expect("parked").startup_model_fix.as_deref(),
                Some(fix.as_str()),
                "the outstanding command parked with its process"
            );

            // pi answers while parked.
            app.sent_commands.clear();
            app.lives
                .get_mut(&path)
                .expect("parked")
                .pending
                .push_back(Event::Response {
                    id: fix.clone(),
                    command: "set_model".into(),
                    success: true,
                    data: Some(serde_json::json!({
                        "provider": "anthropic", "id": "chosen", "name": "Chosen"
                    })),
                    error: None,
                });
            app.tick(cx);
            let parked = app.lives.get(&path).expect("still parked");
            assert!(parked.pending.is_empty(), "the tick drained the reply");
            assert!(parked.startup_model_fix.is_none(), "the reply was consumed");
            assert!(parked.startup_model_expected.is_none());

            app.sent_commands.clear();
            app.switch_to_session(
                SessionInfo {
                    path: path.clone(),
                    id: "parked".into(),
                    cwd: std::env::temp_dir(),
                    title: "parked".into(),
                    first_message: "hello".into(),
                    modified: SystemTime::UNIX_EPOCH,
                    draft: None,
                },
                false,
                cx,
            );
            assert!(app.startup_model_expected.is_none(), "nothing left to check");
            assert!(app.startup_model_fix.is_none());
            assert!(
                !asked(app, "set_model"),
                "resume must not ask a second time for one request"
            );
            // The re-ask went to the parked child itself, before the resume:
            // pi lands it ahead of the resume's own state read, so that reply
            // already carries the level.
            reply(app, "get_state", true, Some(&state_at("high")), None, "s9", cx);
            assert_eq!(app.thinking_label, "high");
            app.drop_client();
        });
    });
}
