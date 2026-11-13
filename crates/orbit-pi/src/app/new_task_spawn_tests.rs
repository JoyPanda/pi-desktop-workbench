//! New Task starts nothing; the first message starts pi.
//!
//! A click lists the row, names it `New task`, and shows a composer. A blank
//! row is dropped when the reader leaves it, so a process started on the read
//! was a cold spawn nobody asked for. The message is what asks for it: it goes
//! on screen at once and is delivered when pi is up
//! (`spawn_pi_client_later` → `admit_spawned_client`).

use super::fake_pi::{prepare, spawn_client, FakePi};
use super::*;

/// The app with no process at all, so the first send has to start one.
fn app_without_a_process(cx: &mut gpui::VisualTestContext, fake: &FakePi) -> Entity<OrbitApp> {
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd);
            app.drop_client();
            app.spawn_bin = Some(fake.bin());
        });
    });
    app
}

fn session_at(name: &str) -> SessionInfo {
    SessionInfo {
        path: std::env::temp_dir().join(name),
        id: name.into(),
        cwd: std::env::temp_dir(),
        title: "titled task".into(),
        first_message: "hello".into(),
        modified: SystemTime::UNIX_EPOCH,
        draft: None,
    }
}

/// The click's own frame carries the row and the title, and starts nothing.
#[gpui::test]
fn a_new_task_lists_its_row_without_starting_anything(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("lists");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.current_title = Some("the task being left".into());
            app.session_name = Some("left behind".into());

            app.on_new_session(&crate::NewSession, window, cx);

            assert!(app.client.is_none(), "no process behind a blank row");
            assert!(!app.spawn_pending(), "nothing was started");
            assert!(app.current_title.is_none(), "the old title is gone");
            assert!(app.session_name.is_none(), "the old name is gone");
            assert_eq!(app.listed_drafts.len(), 1);
            let row = app.session_info_from_draft(&app.listed_drafts[0]);
            assert_eq!(row.title, tr!("menu.new_task"));
            assert_eq!(row.draft, app.current_draft_id);
        });
    });
}

/// The first message goes on screen at once and starts pi under it.
#[gpui::test]
fn the_first_send_starts_the_process(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("first-send");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("build the thing", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);

            app.on_submit(&crate::Submit, window, cx);

            // The reader sees their message and a working turn immediately.
            assert_eq!(
                app.transcript.first_user_message().as_deref(),
                Some("build the thing")
            );
            assert!(app.busy);
            assert!(app.execution.status().is_in_progress());
            assert!(app.spawn_pending(), "the send asked for a process");
            assert!(app.client.is_none(), "it is not up yet");
            assert!(app.input.read(cx).text().is_empty(), "the box is cleared");
        });
    });

    // The process lands and takes the waiting message.
    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(app.client.is_some(), "the started process is adopted");
            assert!(!app.spawn_pending());
            assert!(app.pending_prompt.is_none(), "the message was delivered");
            assert!(app.prompt_in_flight.is_some(), "and its reply is tracked");
            assert!(app.execution.prompt_id.is_some());
            app.drop_client();
            let _ = cx;
        });
    });
}

/// A spawn that fails hands the message back: it never reached pi, so it
/// leaves the transcript and the text returns to the composer.
#[gpui::test]
fn a_failed_spawn_hands_the_message_back(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("failed");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("keep this", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            let ticket = app.pending_spawn.clone().expect("a ticket");

            app.admit_spawned_client(ticket, Err(anyhow::anyhow!("node not found")), cx);

            assert!(!app.spawn_pending(), "the pending window is closed");
            assert!(app.client.is_none());
            assert!(!app.busy, "no turn is in flight");
            assert!(app.transcript.first_user_message().is_none());
            assert_eq!(
                app.input.read(cx).text(),
                "keep this",
                "the text comes back"
            );
            assert_eq!(app.listed_drafts.len(), 1, "the row is the user's");
            assert!(!app.listed_drafts[0].promoted, "and it is unsent again");
            let error = app.error.clone().unwrap_or_default();
            assert!(
                error.contains("node not found"),
                "the banner names the failure: {error}"
            );
        });
    });
}

/// Stop in the starting window cancels the spawn and hands the message back.
#[gpui::test]
fn stopping_a_starting_process_hands_the_message_back(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("stop-starting");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("never mind", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            let ticket = app.pending_spawn.clone().expect("a ticket");
            assert!(app.spawn_pending());

            app.runtime_stop(cx);

            assert!(!app.spawn_pending(), "the stop cancelled the spawn");
            assert!(app.client.is_none());
            assert_eq!(app.input.read(cx).text(), "never mind");
            assert!(app.transcript.first_user_message().is_none());

            // The cancelled spawn's client is dropped when it lands.
            let late = spawn_client(&fake, &cwd_of(app));
            app.admit_spawned_client(ticket, Ok(late), cx);
            assert!(app.client.is_none(), "the cancelled spawn is discarded");
        });
    });
}

/// A spawn whose view moved on is dropped — dropping the client kills the
/// child — instead of being adopted over whatever the reader is looking at.
#[gpui::test]
fn a_spawn_that_lost_the_view_is_dropped(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("superseded");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input.update(cx, |input, cx| input.set_text("lost", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            let ticket = app.pending_spawn.clone().expect("a ticket");
            let cwd = cwd_of(app);

            // The reader leaves before pi is up: the view gives up its claim.
            app.drop_client();
            assert!(!app.spawn_pending());

            let late = spawn_client(&fake, &cwd);
            app.admit_spawned_client(ticket, Ok(late), cx);
            assert!(
                app.client.is_none(),
                "a superseded spawn must not steal the view"
            );
        });
    });
}

/// A name report that describes the session being replaced must not paint the
/// header: that is the flicker where the previous task's title showed up on a
/// New Task page before settling on `New task`. The replacement gate is the
/// same one that keeps a second command out of pi's teardown window.
#[gpui::test]
fn a_name_report_for_the_session_being_left_is_held(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("name-hold");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.drop_client();
            app.spawn_bin = Some(fake.bin());
            assert!(!app.name_report_held(), "nothing in flight yet");

            // The Settings credential restart is the path that still replaces a
            // session on a fresh process.
            app.spawn_pi_client_later(cwd.clone(), SpawnThen::NewSession, cx);
            assert!(app.name_report_held(), "the replacement is unanswered");
        });
    });

    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(app.replacement_in_flight.is_some(), "the command is out");
            assert!(app.name_report_held());
            app.drop_client();
            assert!(app.replacement_in_flight.is_none(), "the process took it");
            assert!(!app.name_report_held());
            let _ = cx;
        });
    });
}

/// Starting a task over a live session parks that process — it keeps running
/// in the background — and clears the chrome it filled in.
#[gpui::test]
fn starting_a_task_parks_the_live_process_and_clears_the_header(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("parks");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            let old = session_at("titled-session.jsonl");
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.sessions.push(old.clone());
            app.current_session_path = Some(old.path.clone());
            app.current_title = Some("old task".into());
            app.session_name = Some("old name".into());
            app.adopt_client(spawn_client(&fake, &cwd));
            assert!(app.runtime_is_live());

            app.on_new_session(&crate::NewSession, window, cx);

            assert!(app.current_title.is_none(), "the old title is gone");
            assert!(app.session_name.is_none(), "nor the explicit rename");
            assert!(app.client.is_none(), "the old process left the view");
            assert!(
                app.lives.contains_key(&old.path),
                "but it stays warm in the background"
            );
            assert_eq!(app.listed_drafts.len(), 1);
            let row = app.session_info_from_draft(&app.listed_drafts[0]);
            assert_eq!(row.title, tr!("menu.new_task"));
            app.drop_client();
        });
    });
}

/// A click that lands while a replacement is in flight waits for pi instead of
/// sending a second command into the same teardown window.
#[gpui::test]
fn a_click_during_a_replacement_waits_for_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("switch-gate");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.adopt_client(spawn_client(&fake, &cwd));

            // A replacement is already out (the id is opaque here; the gate
            // only cares that one is pending).
            app.replacement_in_flight = Some("switch-1".into());
            let target = session_at("elsewhere.jsonl");
            app.switch_to_session(target.clone(), true, cx);

            assert_eq!(
                app.pending_switch
                    .as_ref()
                    .map(|session| session.path.clone()),
                Some(target.path),
                "the click is held, not dropped"
            );
            assert!(
                app.current_session_path.is_none(),
                "and the view has not moved yet"
            );
            app.drop_client();
        });
    });
}

fn cwd_of(app: &OrbitApp) -> PathBuf {
    app.current_workspace
        .clone()
        .unwrap_or_else(|| std::env::temp_dir())
}

/// Leaving the page while its first message still waits for a process takes
/// the message back: it never reached pi, so the row goes back to being an
/// unsent draft that holds the text.
#[gpui::test]
fn leaving_during_the_start_keeps_the_message_as_a_draft(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("leave-starting");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("unsent work", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.pending_prompt.is_some());

            app.switch_to_session(session_at("elsewhere.jsonl"), false, cx);

            assert!(app.pending_prompt.is_none(), "the message was taken back");
            assert!(!app.spawn_pending(), "and its spawn was cancelled");
            assert!(!app.busy);
            assert_eq!(app.listed_drafts.len(), 1, "the row survives as a draft");
            let kept = app.listed_drafts[0].composer.as_ref().expect("kept text");
            assert_eq!(kept.text, "unsent work");
            assert!(!app.listed_drafts[0].promoted, "it was never sent");
        });
    });
}

/// Stop during the start cancels the spawn and hands the message back — the
/// same outcome as stopping a run, which is what the button promises.
#[gpui::test]
fn stop_during_the_start_hands_the_message_back(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("stop-press");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("abandoned", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.pending_prompt.is_some());

            app.request_abort(cx);

            assert!(app.pending_prompt.is_none());
            assert!(!app.spawn_pending());
            assert!(!app.busy);
            assert!(app.transcript.first_user_message().is_none());
            assert_eq!(app.input.read(cx).text(), "abandoned");
        });
    });
}

/// A second message while the first one waits cannot be sent yet, and it must
/// not be swallowed: the composer keeps it.
#[gpui::test]
fn a_second_message_while_starting_stays_in_the_composer(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("second-send");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("first", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.pending_prompt.is_some());

            app.input
                .update(cx, |input, cx| input.set_text("second", cx));
            app.on_submit(&crate::Submit, window, cx);

            assert_eq!(
                app.input.read(cx).text(),
                "second",
                "the composer keeps what could not be sent"
            );
            assert_eq!(
                app.transcript.first_user_message().as_deref(),
                Some("first"),
                "the transcript still shows only the message that is on its way"
            );
        });
    });
}
