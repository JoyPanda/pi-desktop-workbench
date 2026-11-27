//! Compact on a session that has no process.
//!
//! pi is spawned only by a send (INTENT D18), so a cold session has nothing to
//! send `compact` to — but it does have a file to resume. The click starts pi
//! on that file exactly the way a send does, and the compaction stays owed
//! until `get_messages` says the transcript is loaded: compacting before that
//! would compact an empty session and quietly throw the history away.

use super::fake_pi::{adopt_alive, prepare, spawn_client, FakePi};
use super::*;

/// The app on a cold session: a file on disk, no process, nothing pending.
fn cold_app(cx: &mut gpui::VisualTestContext, fake: &FakePi, name: &str) -> Entity<OrbitApp> {
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.drop_client();
            app.spawn_bin = Some(fake.bin());
            app.current_session_path = Some(cwd.join(name));
        });
    });
    app
}

fn session_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

/// The click starts pi on the file and the compaction lands with the snapshot.
#[gpui::test]
fn compact_on_a_cold_session_starts_pi_and_runs_when_it_loads(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(app.client.is_none());
            assert!(
                app.cold_session_path().is_some(),
                "a file with no process is what a cold session is"
            );

            app.compact_now(cx);

            assert_eq!(
                app.pending_compact.as_deref(),
                Some(session_file("cold-compact.jsonl").as_path()),
                "the compaction is owed to *this* session, not dropped"
            );
            assert!(
                !app.is_compacting,
                "pi is not compacting yet — it is not even up"
            );
            assert!(
                !app.sent_commands.iter().any(|label| label == "compact"),
                "nothing was asked of a process that does not exist"
            );
            let ticket = app.pending_spawn.clone().expect("the click started pi");
            assert_eq!(
                ticket.startup_args().first().map(String::as_str),
                Some("--session"),
                "it resumes the file instead of starting a new session"
            );
            assert!(
                ticket.requested_model.is_none(),
                "a resume keeps the model in its file"
            );
        });
    });

    // pi comes up; the resume handshake asks for the transcript first.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let ticket = app.pending_spawn.clone().expect("still booting");
            let client = spawn_client(&fake, &std::env::temp_dir());
            app.admit_spawned_client(ticket, Ok(client), cx);
            assert!(app.client.is_some());
            assert!(
                app.pending_compact.is_some(),
                "still owed until pi has loaded the session"
            );
            assert!(!app.is_compacting);
            assert!(
                !app.sent_commands.iter().any(|label| label == "compact"),
                "the boot handshake asks for messages, not for a compaction"
            );
        });
    });

    // The snapshot lands: now there is a transcript to compact.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let mut refresh_sessions = false;
            app.on_response(
                "get_messages",
                true,
                Some(&serde_json::json!({ "messages": [] })),
                None,
                "messages-1",
                &mut refresh_sessions,
                cx,
            );
            assert!(app.pending_compact.is_none(), "the snapshot delivered it");
            assert!(app.is_compacting, "and pi was asked to compact");
            assert!(app.sent_commands.iter().any(|label| label == "compact"));
            app.drop_client();
        });
    });
}

/// A draft has no file and no history: there is nothing to compact and nothing
/// to start, so the click is a no-op rather than a surprise process.
#[gpui::test]
fn compact_without_a_session_file_starts_nothing(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-draft");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-draft.jsonl");

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            assert!(app.cold_session_path().is_none());

            app.compact_now(cx);

            assert!(app.pending_compact.is_none());
            assert!(!app.is_compacting);
            assert!(!app.spawn_pending(), "no process was started for nothing");
        });
    });
}

/// A start that never lands owes nothing: the button offers Compact again once
/// a process is up, instead of leaving the page stuck on "Compacting…".
#[gpui::test]
fn a_failed_start_drops_the_owed_compaction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-failed");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-failed.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.compact_now(cx);
            assert!(app.pending_compact.is_some());
            let ticket = app.pending_spawn.clone().expect("a ticket");

            app.admit_spawned_client(ticket, Err(anyhow::anyhow!("node not found")), cx);

            assert!(app.pending_compact.is_none(), "the start that owed it is gone");
            assert!(!app.is_compacting);
            assert!(app.client.is_none());
        });
    });
}

/// A load that fails can never deliver what waits on it. Without this the card
/// sits on "Compacting…" with no click handler and `compact_now` refuses every
/// later click, so Compact is dead on that page for the rest of the session.
#[gpui::test]
fn a_failed_load_drops_the_owed_compaction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-load-failed");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-load-failed.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.compact_now(cx);
            let ticket = app.pending_spawn.clone().expect("a ticket");
            let client = spawn_client(&fake, &std::env::temp_dir());
            app.admit_spawned_client(ticket, Ok(client), cx);
            assert!(app.pending_compact.is_some());

            let mut refresh_sessions = false;
            app.on_response(
                "get_messages",
                false,
                None,
                Some("no such session file"),
                "messages-1",
                &mut refresh_sessions,
                cx,
            );

            assert!(
                app.pending_compact.is_none(),
                "a snapshot that never lands delivers nothing"
            );
            assert!(!app.is_compacting, "the card is not stuck on Compacting");
            app.drop_client();
        });
    });
}

/// The debt belongs to one file. A fork, clone, or switch that lands during
/// the boot asks for its own snapshot, and delivering on that one would
/// compact whichever conversation pi happens to be on.
#[gpui::test]
fn a_snapshot_for_another_session_does_not_deliver_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-other");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-owed.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.compact_now(cx);
            assert!(app.pending_compact.is_some());
            let ticket = app.pending_spawn.clone().expect("a ticket");
            let client = spawn_client(&fake, &std::env::temp_dir());
            app.admit_spawned_client(ticket, Ok(client), cx);
            // pi moved onto a copy while the boot was in flight.
            app.current_session_path = Some(session_file("cold-compact-copy.jsonl"));

            let mut refresh_sessions = false;
            app.on_response(
                "get_messages",
                true,
                Some(&serde_json::json!({ "messages": [] })),
                None,
                "messages-1",
                &mut refresh_sessions,
                cx,
            );

            assert!(
                app.pending_compact.is_some(),
                "the copy's snapshot is not the session that was clicked"
            );
            assert!(
                !app.sent_commands.iter().any(|label| label == "compact"),
                "the wrong conversation must not be compacted"
            );
            app.drop_client();
        });
    });
}

/// The owed compaction belongs to the page, not to the process pool: leaving
/// drops it *and* gives up the start, so an abandoned click cannot park a
/// process that nothing will ever reap (D17).
#[gpui::test]
fn leaving_the_page_drops_an_owed_compaction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-left");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-left.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.compact_now(cx);
            assert!(app.pending_compact.is_some());

            app.park_active_session();

            assert!(
                app.pending_compact.is_none(),
                "a compaction must not follow the reader to another session"
            );
            assert!(!app.spawn_pending());
            assert!(
                app.detached_spawns.is_empty(),
                "a start that owes no message is given up, not parked for the run"
            );
        });
    });
}

/// A message still booting keeps going when the reader leaves — the leave must
/// not give up a send just because the rule now cancels an unowed start.
#[gpui::test]
fn leaving_keeps_a_send_that_is_still_booting(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-keeps-send");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-keeps-send.jsonl");

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.sessions.push(SessionInfo {
                path: session_file("cold-compact-keeps-send.jsonl"),
                id: "kept".into(),
                cwd: std::env::temp_dir(),
                title: "a cold session".into(),
                first_message: "hello".into(),
                modified: SystemTime::UNIX_EPOCH,
                draft: None,
            });
            app.input
                .update(cx, |input, cx| input.set_text("keep this", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.spawn_pending(), "the send started pi");
            assert!(app.pending_prompt.is_some());

            app.park_active_session();

            assert!(
                !app.detached_spawns.is_empty(),
                "an owed message keeps its booting process"
            );
        });
    });
}

/// The Compact button is the only thing on a cold page that can start a
/// process for a reason other than a message, so its gate and `compact_now`'s
/// own precondition have to agree — a button that no-ops is a lie.
#[gpui::test]
fn the_compact_gate_matches_what_the_click_can_do(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-gate");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-gate.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // Cold session: offered, and the click does start something.
            assert!(app.compact_offered());
            app.compact_now(cx);
            assert!(app.pending_compact.is_some());
            assert!(app.spawn_pending());
            // The start makes `cold_session_path` None; the queued debt must
            // keep the control (and its Compacting… state) on screen anyway.
            assert!(app.cold_session_path().is_none());
            assert!(
                app.compact_offered(),
                "the button must not vanish while the start it asked for is in flight"
            );
            app.park_active_session();

            // A dead process is the Reconnect banner's business, not this one.
            app.runtime.exited = true;
            assert!(
                !app.compact_offered(),
                "an exited process offers Reconnect instead"
            );
            app.compact_now(cx);
            assert!(app.pending_compact.is_none());
            app.runtime.exited = false;

            // A live process takes the ordinary path: no debt, no spawn.
            adopt_alive(app, &fake);
            app.compact_now(cx);
            assert!(app.pending_compact.is_none());
            assert!(app.is_compacting);
            assert!(app.sent_commands.iter().any(|label| label == "compact"));
            app.drop_client();
        });
    });
}

/// pi's "nothing to do" answers are a notice, not a failure. Only pi knows the
/// keep-recent window for the model a session runs on, so a Compact on a
/// session without a process cannot pre-check it — and a red banner saying
/// "the session is too small" reads as a broken click.
#[gpui::test]
fn a_too_small_session_is_a_notice_not_a_failure(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let mut refresh_sessions = false;
            app.on_response(
                "compact",
                false,
                None,
                Some("Nothing to compact (session too small)"),
                "c1",
                &mut refresh_sessions,
                cx,
            );
            assert!(
                app.error.is_none(),
                "pi's nothing-to-do is not a command failure"
            );
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title == tr!("events.compaction_too_small")),
                "the reader is told plainly instead"
            );
            assert!(!app.is_compacting);

            app.on_response(
                "compact",
                false,
                None,
                Some("Already compacted"),
                "c2",
                &mut refresh_sessions,
                cx,
            );
            assert!(app.toasts.items().iter().any(|toast| toast.title
                == tr!("events.compaction_already")));

            // A refusal that is not pi's nothing-to-do stays a real failure.
            app.on_response(
                "compact",
                false,
                None,
                Some("No API key for HE_Gateway/anthropic/claude-haiku-5-5"),
                "c3",
                &mut refresh_sessions,
                cx,
            );
            assert!(app.error.is_some(), "a real failure still raises the banner");

            // A longer diagnostic that merely *mentions* the phrase is real.
            app.error = None;
            app.on_response(
                "compact",
                false,
                None,
                Some("Nothing to compact (session too small) — and the summary model refused"),
                "c4",
                &mut refresh_sessions,
                cx,
            );
            assert!(app.error.is_some(), "exact messages only, never substrings");

            // Another command's failure quoting the phrase is that command's.
            app.error = None;
            app.on_response(
                "set_model",
                false,
                None,
                Some("Model not found: anthropic/Already compacted"),
                "m1",
                &mut refresh_sessions,
                cx,
            );
            assert!(app.error.is_some(), "only compact's own refusals are notices");
        });
    });
}

/// pi emits `compaction_end` with the error *and* rethrows for the failed
/// response. The event must not raise the red banner for the two nothing-to-do
/// answers — the response's single notice owns them, and a banner raised here
/// first would never be retired.
#[gpui::test]
fn the_benign_event_does_not_raise_a_banner_the_response_forgets(
    cx: &mut gpui::TestAppContext,
) {
    let fake = FakePi::start("benign-compaction-event");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_alive(app, &fake);
            app.is_compacting = true;
            app.foreground_events.push_back(Event::CompactionEnd {
                value: serde_json::json!({
                    "reason": "manual",
                    "aborted": false,
                    "errorMessage": "Compaction failed: Nothing to compact (session too small)",
                }),
            });
            app.tick(cx);
            assert!(app.error.is_none(), "the event owns no banner for a notice");
            assert!(
                app.foreground_events.is_empty(),
                "the tick applied the event"
            );
            assert!(!app.is_compacting);

            let mut refresh_sessions = false;
            app.on_response(
                "compact",
                false,
                None,
                Some("Nothing to compact (session too small)"),
                "c1",
                &mut refresh_sessions,
                cx,
            );
            assert!(app.error.is_none(), "one notice, no banner, end to end");
            assert_eq!(
                app.toasts
                    .items()
                    .iter()
                    .filter(|toast| toast.title == tr!("events.compaction_too_small"))
                    .count(),
                1
            );
            assert!(!app.is_compacting);

            // A real failure on the event path still raises the banner.
            app.foreground_events.push_back(Event::CompactionEnd {
                value: serde_json::json!({
                    "reason": "manual",
                    "aborted": false,
                    "errorMessage": "Compaction failed: No API key for anthropic/claude",
                }),
            });
            app.tick(cx);
            assert!(app.error.is_some());
            app.drop_client();
        });
    });
}

/// pi switches onto the copy the moment it handles `clone` — before Orbit
/// drains the replies. An owed Compact still waiting on the *source's*
/// snapshot must be cancelled when the clone is queued, not when its reply
/// is consumed, or the source snapshot delivers it onto the copy.
#[gpui::test]
fn a_queued_clone_cancels_the_owed_compaction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-clone");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-clone.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.compact_now(cx);
            let ticket = app.pending_spawn.clone().expect("a ticket");
            let client = spawn_client(&fake, &std::env::temp_dir());
            app.admit_spawned_client(ticket, Ok(client), cx);
            assert!(app.pending_compact.is_some());

            app.clone_session(cx);
            assert!(
                app.pending_compact.is_none(),
                "the debt died with the queueing, not with the reply"
            );

            // The source's snapshot arrives after the clone was queued.
            let mut refresh_sessions = false;
            app.on_response(
                "get_messages",
                true,
                Some(&serde_json::json!({ "messages": [] })),
                None,
                "messages-1",
                &mut refresh_sessions,
                cx,
            );
            assert!(
                !app.sent_commands.iter().any(|label| label == "compact"),
                "the copy must not be compacted for the source's click"
            );
            app.drop_client();
        });
    });
}

/// Leaving after the start was admitted but before the snapshot arrived owes
/// nothing either: that process exists only for the abandoned Compact, so it
/// is dropped, not parked for the rest of the run (D17 never reaps).
#[gpui::test]
fn leaving_after_admission_drops_the_compact_only_process(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-compact-left-admitted");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cold_app(cx, &fake, "cold-compact-left-admitted.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.compact_now(cx);
            let ticket = app.pending_spawn.clone().expect("a ticket");
            let client = spawn_client(&fake, &std::env::temp_dir());
            let pid = client.child_pid();
            app.admit_spawned_client(ticket, Ok(client), cx);
            assert!(app.client.is_some());
            assert!(app.pending_compact.is_some());

            app.park_active_session();

            assert!(app.pending_compact.is_none());
            assert!(app.client.is_none());
            assert!(
                app.lives.is_empty() && app.pending_parks.is_empty(),
                "a process nothing owes must not join the never-reaped pool"
            );
            // The child itself is gone, not orphaned.
            let alive = || {
                std::process::Command::new("/bin/kill")
                    .arg("-0")
                    .arg(pid.to_string())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            };
            for _ in 0..50 {
                if !alive() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(!alive(), "the abandoned start's child is terminated");
        });
    });
}
