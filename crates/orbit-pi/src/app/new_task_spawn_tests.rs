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

/// A click that lands while a replacement is in flight switches immediately.
/// The in-flight command stays with the process being left.
#[gpui::test]
fn a_click_during_a_replacement_switches_immediately(cx: &mut gpui::TestAppContext) {
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

            assert!(app.pending_switch.is_none(), "the click is not queued");
            assert_eq!(app.current_session_path.as_ref(), Some(&target.path));
            assert!(
                !app.pending_parks.is_empty() || !app.lives.is_empty(),
                "the process that was mid-replacement stays up"
            );
            app.pending_parks.clear();
            app.lives.clear();
            app.drop_client();
        });
    });
}

fn cwd_of(app: &OrbitApp) -> PathBuf {
    app.current_workspace
        .clone()
        .unwrap_or_else(|| std::env::temp_dir())
}

/// Leaving the page while its first message still waits for a process keeps
/// the send. The spawn finishes in the background and delivers the prompt.
#[gpui::test]
fn leaving_during_the_start_keeps_the_spawn(cx: &mut gpui::TestAppContext) {
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

            assert!(app.pending_prompt.is_none(), "the message left with the spawn");
            assert!(!app.spawn_pending(), "the view is no longer waiting");
            assert_eq!(app.detached_spawns.len(), 1, "the spawn keeps booting");
            assert_eq!(app.listed_drafts.len(), 1, "the row stays");
            assert!(app.listed_drafts[0].promoted, "the send already happened");
            assert!(app.listed_drafts[0].composer.is_none());
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

fn catalog_model(id: &str, name: &str, provider: &str) -> ModelEntry {
    ModelEntry {
        id: id.into(),
        name: name.into(),
        provider: provider.into(),
        context_window: None,
        thinking_levels: vec!["off".into(), "low".into(), "high".into()],
    }
}

fn no_pi_not_running_toast(app: &OrbitApp) -> bool {
    app.toasts
        .items()
        .iter()
        .all(|toast| toast.title != tr!("runtime.pi_not_running"))
}

/// Picking a model on a new task does not need a process. The choice stays on
/// the row when the reader leaves and comes back, and the first message
/// starts pi on that model rather than the configured default.
#[gpui::test]
fn a_model_chosen_before_pi_starts_is_kept_and_used(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("draft-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some("openai".into()),
                model_id: Some("gpt-default".into()),
                thinking: Some("low".into()),
            };
            app.available_models = vec![
                catalog_model("gpt-default", "GPT Default", "openai"),
                catalog_model("claude-chosen", "Chosen", "anthropic"),
            ];
            app.model_id = "stale-from-the-last-session".into();
            app.model_label = "Stale".into();
            app.model_provider = "other".into();

            app.on_new_session(&crate::NewSession, window, cx);

            assert_eq!(app.model_id, "gpt-default");
            assert_eq!(app.model_label, "GPT Default");
            assert_eq!(app.thinking_label, "low");
            assert!(app.client.is_none());

            app.open_picker(crate::model_selector::PickerKind::Model, window, cx);
            assert!(
                no_pi_not_running_toast(app),
                "opening the picker must not ask a process that does not exist"
            );
            app.close_model_selector(window, cx);

            app.set_model("claude-chosen".into(), "anthropic".into(), cx);
            app.set_thinking_level("high".into(), cx);
            assert_eq!(app.model_id, "claude-chosen");
            assert_eq!(app.model_provider, "anthropic");
            assert_eq!(app.model_label, "Chosen");
            assert_eq!(app.thinking_label, "high");
            assert!(no_pi_not_running_toast(app));
            let draft_id = app.current_draft_id.expect("the new task row");
            assert_eq!(
                app.listed_drafts[0]
                    .model
                    .as_ref()
                    .and_then(|model| model.model_id.as_deref()),
                Some("claude-chosen")
            );

            // Leave for another workspace's task and come back. The row is
            // kept even with an empty composer, and the chips show the choice.
            let other = std::env::temp_dir().join(format!(
                "orbit-draft-model-other-{}",
                std::process::id()
            ));
            app.on_new_session_in_workspace(other, window, cx);
            assert_eq!(app.listed_drafts.len(), 2, "the choice keeps the row");
            assert_eq!(app.model_id, "gpt-default", "the other task shows the default");
            let pos = app
                .listed_drafts
                .iter()
                .position(|draft| draft.id == draft_id)
                .expect("the first row is still listed");
            let info = app.session_info_from_draft(&app.listed_drafts[pos]);
            app.switch_to_session(info, false, cx);
            assert_eq!(app.current_draft_id, Some(draft_id));
            assert_eq!(app.model_id, "claude-chosen");
            assert_eq!(app.model_label, "Chosen");
            assert_eq!(app.thinking_label, "high");
            assert!(app.client.is_none(), "coming back still starts nothing");

            app.input
                .update(cx, |input, cx| input.set_text("use the chosen model", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);

            let ticket = app.pending_spawn.clone().expect("the send started pi");
            let expected = crate::session_defaults::SessionDefault {
                provider: Some("anthropic".into()),
                model_id: Some("claude-chosen".into()),
                thinking: Some("high".into()),
            };
            assert_eq!(ticket.startup_args(), expected.cli_args());
            assert!(
                !app.startup_model_pinned,
                "a spawn that has not landed must not pin the next session"
            );
        });
    });

    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            assert!(!app.startup_model_pinned);
            assert!(!app.default_model_armed);
            app.drop_client();
        });
    });
}

/// With no composer choice, the first message still starts on the default.
#[gpui::test]
fn the_first_send_uses_the_default_model_when_none_was_chosen(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("default-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some("openai".into()),
                model_id: Some("gpt-default".into()),
                thinking: Some("low".into()),
            };
            app.on_new_session(&crate::NewSession, window, cx);
            app.input
                .update(cx, |input, cx| input.set_text("plain", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);

            let ticket = app.pending_spawn.clone().expect("a ticket");
            assert_eq!(ticket.startup_args(), app.session_default.cli_args());
            assert!(!app.startup_model_pinned);
        });
    });

    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            app.drop_client();
        });
    });
}

/// Stopping a process that is still starting drops any pin, so the next
/// session is allowed to apply its default.
#[gpui::test]
fn stopping_a_starting_process_clears_a_model_pin(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("stop-pin");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some("openai".into()),
                model_id: Some("gpt-default".into()),
                thinking: Some("low".into()),
            };
            app.available_models = vec![catalog_model("gpt-default", "GPT Default", "openai")];
            app.on_new_session(&crate::NewSession, window, cx);
            app.set_model("gpt-default".into(), "openai".into(), cx);
            app.input
                .update(cx, |input, cx| input.set_text("hold", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.spawn_pending());
            app.startup_model_pinned = true;
            app.runtime_stop(cx);
            assert!(!app.spawn_pending());
            assert!(
                !app.startup_model_pinned,
                "cancelling the spawn must not leave the next session pinned"
            );
        });
    });
}

/// A thinking level with no model is not a draft. New in the same folder
/// reuses the row.
#[gpui::test]
fn thinking_without_a_model_does_not_keep_the_draft(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("thinking-only");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault::default();
            // The shared catalog cache may name a default; this test is about
            // having no model at all.
            app.reported_model = Default::default();
            app.on_new_session(&crate::NewSession, window, cx);
            app.set_thinking_level("high".into(), cx);
            let id = app.current_draft_id.expect("the blank row");
            assert!(
                app.listed_drafts
                    .iter()
                    .find(|draft| draft.id == id)
                    .and_then(|draft| draft.model.as_ref())
                    .is_none_or(|model| model.is_unset()),
                "thinking alone must not be stored"
            );
            app.on_new_session(&crate::NewSession, window, cx);
            assert_eq!(app.listed_drafts.len(), 1, "the blank row is reused");
            assert_eq!(app.current_draft_id, Some(id));
        });
    });
}

/// A model that does not offer the current thinking level drops it, and a
/// level the catalog rejects is not written onto the row.
#[gpui::test]
fn a_model_switch_drops_thinking_the_model_does_not_offer(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("thinking-fit");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some("openai".into()),
                model_id: Some("gpt-default".into()),
                thinking: Some("high".into()),
            };
            let mut limited = catalog_model("limited", "Limited", "anthropic");
            limited.thinking_levels = vec!["off".into(), "low".into()];
            app.available_models = vec![
                catalog_model("gpt-default", "GPT Default", "openai"),
                limited,
            ];
            app.on_new_session(&crate::NewSession, window, cx);
            assert_eq!(app.thinking_label, "high");

            app.set_model("limited".into(), "anthropic".into(), cx);
            assert_eq!(app.model_id, "limited");
            assert_eq!(app.thinking_label, "…");
            let stored = app.current_draft_model().expect("the chosen model");
            assert!(stored.thinking.is_none());

            app.set_thinking_level("high".into(), cx);
            assert!(app.current_draft_model().unwrap().thinking.is_none());
            app.set_thinking_level("low".into(), cx);
            assert_eq!(
                app.current_draft_model().unwrap().thinking.as_deref(),
                Some("low")
            );
        });
    });
}

/// New in the same folder keeps a row that already has a model and opens a
/// fresh draft beside it.
#[gpui::test]
fn a_model_choice_is_not_reused_as_a_blank_draft(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("reuse-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some("openai".into()),
                model_id: Some("gpt-default".into()),
                thinking: Some("low".into()),
            };
            app.available_models = vec![
                catalog_model("gpt-default", "GPT Default", "openai"),
                catalog_model("claude-chosen", "Chosen", "anthropic"),
            ];
            app.on_new_session(&crate::NewSession, window, cx);
            app.set_model("claude-chosen".into(), "anthropic".into(), cx);
            let first = app.current_draft_id.expect("the chosen row");
            app.on_new_session(&crate::NewSession, window, cx);
            assert_eq!(app.listed_drafts.len(), 2);
            assert_ne!(app.current_draft_id, Some(first));
            assert_eq!(app.model_id, "gpt-default");
            assert_eq!(
                app.listed_drafts
                    .iter()
                    .find(|draft| draft.id == first)
                    .and_then(|draft| draft.model.as_ref())
                    .and_then(|model| model.model_id.as_deref()),
                Some("claude-chosen")
            );
        });
    });
}

/// With no session default, a new task keeps the model name pi last reported
/// instead of replacing the chip with a placeholder.
#[gpui::test]
fn a_new_task_keeps_the_model_pi_last_reported(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("reported-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_default = crate::session_defaults::SessionDefault::default();
            app.available_models = vec![catalog_model("claude-chosen", "Chosen", "anthropic")];
            app.model_id = "stale".into();
            app.model_label = "Stale".into();
            app.remember_reported_model(&serde_json::json!({
                "model": {
                    "id": "claude-chosen",
                    "name": "Chosen",
                    "provider": "anthropic",
                    "reasoning": true
                },
                "thinkingLevel": "low"
            }));
            assert_eq!(app.model_label, "Stale", "a switched session is not the startup model");

            app.startup_model_reportable = true;
            app.remember_reported_model(&serde_json::json!({
                "model": {
                    "id": "claude-chosen",
                    "name": "Chosen",
                    "provider": "anthropic",
                    "reasoning": true
                },
                "thinkingLevel": "low"
            }));

            app.on_new_session(&crate::NewSession, window, cx);

            assert_eq!(app.model_id, "claude-chosen");
            assert_eq!(app.model_label, "Chosen");
            assert_eq!(app.thinking_label, "low");
            assert!(app.current_draft_model().is_none());
        });
    });
}


/// Opening a session that has no process starts nothing: the disk preview
/// stands in until the first send (INTENT D18).
#[gpui::test]
fn opening_a_cold_session_starts_nothing(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-open");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let target = session_at("cold-target.jsonl");
            app.switch_to_session(target.clone(), true, cx);

            assert!(app.client.is_none(), "a click spawns nothing");
            assert!(!app.spawn_pending(), "and queues no spawn");
            assert_eq!(app.current_session_path.as_ref(), Some(&target.path));
            assert!(app.is_cold_session());
        });
    });
}

/// A process that dies idle leaves no banner and no respawn: the session is
/// simply cold, and the next send resumes it. A death mid-run is a loss, so
/// it raises the banner.
#[gpui::test]
fn an_idle_exit_goes_cold_and_a_mid_run_exit_warns(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("idle-exit");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.current_session_path = Some(cwd.join("idle-exit.jsonl"));
            app.adopt_client(spawn_client(&fake, &cwd));
            app.on_process_exited(false, cx);
            assert!(app.client.is_none(), "the dead process is dropped");
            assert!(!app.spawn_pending(), "nothing is respawned");
            assert!(app.error.is_none(), "an idle exit is not an error");
            assert!(app.is_cold_session());

            app.adopt_client(spawn_client(&fake, &cwd));
            app.on_process_exited(true, cx);
            assert!(!app.spawn_pending(), "a mid-run exit is not respawned either");
            assert!(app.error.is_some(), "it raises the Reconnect banner");
            app.drop_client();
        });
    });
}


/// A model picked on a cold session is staged, shown, and spawns nothing; a
/// rename is staged and the session list already carries it.
#[gpui::test]
fn choices_on_a_cold_session_are_staged_not_sent(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cold-stage");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let target = session_at("cold-stage.jsonl");
            crate::session_staging::clear(&target.path);
            app.switch_to_session(target.clone(), true, cx);
            assert!(app.cold_session_path().is_some());

            app.set_model("sonnet".into(), "anthropic".into(), cx);
            assert!(!app.spawn_pending(), "picking a model starts nothing");
            let staged = crate::session_staging::get(&target.path)
                .and_then(|s| s.model)
                .expect("model staged");
            assert_eq!(staged.model_id.as_deref(), Some("sonnet"));
            assert_eq!(app.model_id, "sonnet", "the chip shows it at once");
            assert_eq!(
                app.staged_model_for_current().and_then(|m| m.model_id),
                Some("sonnet".to_string())
            );

            app.begin_session_rename(window, cx);
            app.session_name_input
                .update(cx, |input, cx| input.set_text("staged name", cx));
            app.rename_session(cx);
            assert!(!app.spawn_pending());
            assert_eq!(
                crate::session_staging::rename_for(&target.path).as_deref(),
                Some("staged name")
            );
            crate::session_staging::clear(&target.path);
        });
    });
}


/// Staged choices go to the resumed process and leave the store only when pi
/// answers; a refusal keeps them, and a model the catalog lost is dropped.
#[gpui::test]
fn staged_choices_merge_into_the_resumed_process(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("merge");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            let cwd = std::env::temp_dir();
            let target = session_at("merge.jsonl");
            let choice = crate::session_defaults::SessionDefault {
                provider: Some("anthropic".into()),
                model_id: Some("sonnet".into()),
                thinking: Some("high".into()),
            };
            crate::session_staging::clear(&target.path);
            crate::session_staging::set_model(&target.path, choice);
            crate::session_staging::set_rename(&target.path, "merged name".into());
            app.current_session_path = Some(target.path.clone());
            app.adopt_client(spawn_client(&fake, &cwd));
            // The shared catalog cache may have filled this; an empty catalog
            // counts as unknown, so the model is sent.
            app.available_models.clear();

            app.apply_staged_choices();
            assert_eq!(app.staged_acks.len(), 3, "model, level and name are sent");
            let ids: Vec<String> = app.staged_acks.iter().map(|a| a.id.clone()).collect();

            // The model is accepted but the level is refused: the choice is
            // dropped and a late ack cannot complete it.
            assert!(!app.ack_staged(&ids[0]));
            assert!(crate::session_staging::get(&target.path)
                .and_then(|s| s.model)
                .is_some());
            app.fail_staged(&ids[1]);
            assert!(!app.ack_staged(&ids[1]));
            assert!(
                crate::session_staging::get(&target.path)
                    .and_then(|s| s.model)
                    .is_none(),
                "a refused choice is dropped, not retried on every resume"
            );

            // The rename is answered: it is applied for good.
            assert!(app.ack_staged(&ids[2]), "a rename ack refreshes the list");
            assert!(crate::session_staging::rename_for(&target.path).is_none());
            crate::session_staging::clear(&target.path);

            // A model the catalog no longer lists is dropped, not sent.
            let gone = session_at("merge-gone.jsonl");
            crate::session_staging::set_model(
                &gone.path,
                crate::session_defaults::SessionDefault {
                    provider: Some("anthropic".into()),
                    model_id: Some("retired".into()),
                    thinking: None,
                },
            );
            app.staged_acks.clear();
            app.current_session_path = Some(gone.path.clone());
            app.available_models = vec![ModelEntry {
                id: "other".into(),
                provider: "anthropic".into(),
                ..Default::default()
            }];
            app.apply_staged_choices();
            assert!(app.staged_acks.is_empty(), "nothing was sent");
            assert!(crate::session_staging::get(&gone.path).is_none());
            app.drop_client();
        });
    });
}


/// A cold launch shows the cached catalog and commands with no process.
#[gpui::test]
fn a_cold_launch_shows_the_cached_catalog(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("catalog-cache");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = app.task_cwd();
            crate::catalog_cache::store_models(&serde_json::json!({
                "models": [{ "id": "cached-model", "name": "Cached", "provider": "anthropic" }]
            }));
            crate::catalog_cache::store_commands(
                &cwd,
                &serde_json::json!({ "commands": [{ "name": "cached-cmd", "description": "d" }] }),
            );
            app.available_models.clear();
            app.slash_commands.clear();

            app.load_catalog_cache(cx);

            assert!(app.client.is_none() && !app.spawn_pending());
            assert!(app.available_models.iter().any(|m| m.id == "cached-model"));
            assert!(app.slash_commands.iter().any(|c| c.name == "cached-cmd"));
        });
    });
}


/// A cold session shows the numbers from its last live moment, flagged as a
/// snapshot, and a live stats reply replaces them.
#[gpui::test]
fn a_cold_session_shows_its_last_context_numbers(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("snapshot");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let target = session_at("snapshot.jsonl");
            crate::session_snapshot::put(
                &target.path,
                &serde_json::json!({
                    "contextUsage": { "contextWindow": 200000, "tokens": 50000 },
                    "tokens": { "input": 10, "output": 20 },
                    "cost": 0.25
                }),
            );

            app.switch_to_session(target.clone(), true, cx);

            assert!(app.client.is_none());
            assert!(app.context_is_snapshot, "flagged as stored, not live");
            assert_eq!(app.context.as_ref().map(|c| c.context_window), Some(200000));
            assert_eq!(app.session_usage.as_ref().map(|u| u.total), Some(30));

            // A session with no snapshot shows an empty meter, not stale data.
            app.switch_to_session(session_at("no-snapshot.jsonl"), true, cx);
            assert!(app.context.is_none() && !app.context_is_snapshot);
            crate::session_snapshot::clear(&target.path);
        });
    });
}

/// The blank launch page has no row and no process: picking a model there is
/// remembered on a fresh draft row and never reports "pi is not running".
#[gpui::test]
fn a_model_pick_on_the_blank_launch_page_is_kept(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("blank-launch-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            if !app.is_blank_launch_task() {
                return;
            }
            app.set_model("picked".into(), "anthropic".into(), cx);
            assert!(no_pi_not_running_toast(app));
            let stored = app.current_draft_model().expect("the chosen model");
            assert_eq!(stored.model_id.as_deref(), Some("picked"));
            assert!(app.client.is_none() && !app.spawn_pending());
        });
    });
}
/// With nothing chosen, a cold launch's chips show pi's own default (from the
/// cached startup state) instead of "…".
#[gpui::test]
fn a_cold_launch_shows_pis_default_model(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("cached-default");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            crate::catalog_cache::store_models(&serde_json::json!({
                "models": [{ "id": "dflt", "name": "Default One", "provider": "anthropic",
                             "reasoning": true }]
            }));
            crate::catalog_cache::store_state(&serde_json::json!({
                "model": { "id": "dflt", "name": "Default One", "provider": "anthropic" },
                "thinkingLevel": "medium"
            }));
            app.session_default = Default::default();
            app.reported_model = Default::default();
            app.current_draft_id = None;
            app.current_session_path = None;

            app.load_catalog_cache(cx);

            assert_eq!(app.model_id, "dflt");
            assert_ne!(app.model_label, "…");
            assert!(app.client.is_none() && !app.spawn_pending());
        });
    });
}
/// A cold session shows the model and level it last ran on (from its file),
/// not the default a new task would get; a staged pick still wins.
#[gpui::test]
fn a_cold_session_shows_the_model_it_last_ran_on(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("own-model");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let target = session_at("own-model.jsonl");
            std::fs::write(
                &target.path,
                "{\"type\":\"session\",\"id\":\"x\"}\n\
                 {\"type\":\"model_change\",\"provider\":\"anthropic\",\"modelId\":\"old-one\"}\n\
                 {\"type\":\"thinking_level_change\",\"thinkingLevel\":\"high\"}\n\
                 {\"type\":\"model_change\",\"provider\":\"openai\",\"modelId\":\"last-one\"}\n",
            )
            .unwrap();
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some("anthropic".into()),
                model_id: Some("the-default".into()),
                thinking: None,
            };

            app.switch_to_session(target.clone(), true, cx);
        });
    });
    // The file is read off the UI thread; let it land.
    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            let target = session_at("own-model.jsonl");
            assert_eq!(app.model_id, "last-one");
            let own = crate::sessions::last_model_choice(&target.path).unwrap();
            assert_eq!(own.thinking.as_deref(), Some("high"));
            let _ = std::fs::remove_file(&target.path);
        });
    });
}

/// Returning to a session whose send is still booting re-attaches that send
/// instead of showing a cold page. The session is not cold while its send
/// waits, so a second send cannot start another process and overwrite it.
#[gpui::test]
fn returning_to_a_booting_send_reattaches_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("reattach");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let a = session_at("reattach-a.jsonl");
            // Open A cold and send: the resume spawn starts booting.
            app.switch_to_session(a.clone(), true, cx);
            app.input
                .update(cx, |input, cx| input.set_text("first message", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.spawn_pending(), "the resume send asked for a process");
            assert!(app.pending_prompt.is_some());

            // Leave before it lands: the send detaches but keeps booting.
            app.switch_to_session(session_at("reattach-b.jsonl"), false, cx);
            assert_eq!(app.detached_spawns.len(), 1);
            assert!(app.pending_prompt.is_none());

            // Return to A before the spawn lands: the send comes back onto the
            // page, its ticket is the foreground's again, and A is not cold.
            app.switch_to_session(a.clone(), true, cx);
            assert!(app.detached_spawns.is_empty(), "the send is back on the page");
            assert!(app.spawn_pending(), "its spawn is the foreground's again");
            assert_eq!(
                app.pending_prompt.as_ref().map(|p| p.composer.text.clone()),
                Some("first message".to_string()),
                "the original message is intact"
            );
            assert!(
                app.cold_session_path().is_none(),
                "A is not cold while its send boots"
            );
        });
    });
}

/// A parked resume whose process dies before `get_messages` answers hands the
/// unsent prompt back to the session's draft instead of dropping it with the park.
#[gpui::test]
fn a_parked_resume_whose_process_dies_hands_the_prompt_back(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("parked-exit");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let a = session_at("parked-exit-a.jsonl");
            app.switch_to_session(a.clone(), true, cx);
            app.input
                .update(cx, |input, cx| input.set_text("stranded work", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            // Leave before landing: the resume spawn detaches.
            app.switch_to_session(session_at("parked-exit-b.jsonl"), false, cx);
            assert_eq!(app.detached_spawns.len(), 1);
        });
    });
    // The spawn lands while the reader is away: it parks with the prompt still
    // waiting on get_messages (the fake pi never answers).
    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let a = session_at("parked-exit-a.jsonl");
            assert!(
                app.lives.get(&a.path).is_some_and(|p| p.resume_prompt.is_some()),
                "the parked resume still holds its prompt"
            );
            // The parked process dies before the snapshot arrives.
            app.lives
                .get_mut(&a.path)
                .expect("parked")
                .pending
                .push_back(orbit_rpc::Event::ProcessExited);
            app.tick(cx);
            assert!(app.lives.get(&a.path).is_none(), "the dead park is dropped");
            assert_eq!(
                app.drafts.get(&a.path).map(|d| d.text.clone()),
                Some("stranded work".to_string()),
                "the unsent prompt goes back to the session's draft"
            );
        });
    });
}

/// Returning to a New Task draft whose send is still booting re-attaches that
/// send by draft id (the file pi has not named yet has no session path). A
/// second send cannot start and overwrite it.
#[gpui::test]
fn returning_to_a_booting_new_task_reattaches_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("reattach-draft");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            let id = app.current_draft_id.expect("draft row");
            app.input
                .update(cx, |input, cx| input.set_text("first task message", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.spawn_pending());

            // Leave before landing: the send detaches under the draft id.
            app.switch_to_session(session_at("draft-elsewhere.jsonl"), false, cx);
            assert_eq!(app.detached_spawns.len(), 1);
            assert!(app.pending_prompt.is_none());

            // Reopen the draft row before the spawn lands: the send comes back.
            let row = app.session_info_from_draft(&app.listed_drafts[0]);
            app.switch_to_session(row, true, cx);
            assert_eq!(app.current_draft_id, Some(id));
            assert!(app.detached_spawns.is_empty(), "the send is back on the page");
            assert!(app.spawn_pending(), "its spawn is the foreground's again");
            assert_eq!(
                app.pending_prompt.as_ref().map(|p| p.composer.text.clone()),
                Some("first task message".to_string()),
                "the original message is intact"
            );
        });
    });
}

/// The blank launch page's first send gets a draft row, so the send has an
/// identity to detach / re-attach / stash under instead of being lost.
#[gpui::test]
fn a_blank_launch_send_gets_a_draft_row(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("blank-send-row");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(app.is_blank_launch_task(), "no row, no process yet");
            app.input
                .update(cx, |input, cx| input.set_text("launch message", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.current_draft_id.is_some(), "the send now has a row");
            assert_eq!(app.listed_drafts.len(), 1);
            assert!(app.spawn_pending());
            assert_eq!(
                app.pending_prompt.as_ref().map(|p| p.composer.text.clone()),
                Some("launch message".to_string())
            );
        });
    });
}

/// Stopping a booting send hands the message back without clobbering text typed
/// since, and restores image attachments (images were not folded into the text).
#[gpui::test]
fn stopping_a_booting_send_restores_images_and_keeps_newer_text(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("stop-images");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_new_session(&crate::NewSession, window, cx);
            app.attachments = vec![Attachment {
                name: "img.png".into(),
                mime: "image/png".into(),
                data: "aGVsbG8=".into(),
                preview: None,
                original_len: 5,
                stored_len: 5,
                compressed: false,
            }];
            app.input
                .update(cx, |input, cx| input.set_text("first message", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.pending_prompt.is_some());
            assert!(app.attachments.is_empty(), "the image rode with the send");

            // Type more while the process boots, then stop.
            app.input
                .update(cx, |input, cx| input.set_text("typed since", cx));
            app.request_abort(cx);

            let text = app.input.read(cx).text();
            assert!(text.contains("first message"), "unsent message back: {text}");
            assert!(text.contains("typed since"), "newer text preserved: {text}");
            assert_eq!(app.attachments.len(), 1, "the image is back in the tray");
            assert!(app.pending_prompt.is_none());
        });
    });
}

/// The resume replay (get_messages landed) re-puts the sent message without
/// clearing text typed since the send.
#[gpui::test]
fn resume_replay_keeps_text_typed_since_the_send(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("resume-replay");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let a = session_at("resume-replay.jsonl");
            app.switch_to_session(a.clone(), true, cx);
            app.input
                .update(cx, |input, cx| input.set_text("first message", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.pending_prompt.is_some());
        });
    });
    // The process lands and asks get_messages; the fake pi never answers, so
    // the prompt is still waiting.
    cx.run_until_parked();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(app.client.is_some(), "the resume process landed");
            assert!(app.pending_prompt.is_some(), "still waiting on get_messages");
            // Type more while the snapshot is on its way.
            app.input
                .update(cx, |input, cx| input.set_text("typed since", cx));
            // The snapshot lands and the prompt is delivered: the replay must
            // not clear what was typed since the send.
            app.deliver_resume_prompt(cx);
            let text = app.input.read(cx).text();
            assert!(text.contains("typed since"), "replay kept newer text: {text}");
            assert!(app.pending_prompt.is_none(), "the prompt was delivered");
        });
    });
}

/// A stashed failed send keeps the folded text but not the original file/paste
/// trays, so restoring and re-sending does not fold them a second time.
#[gpui::test]
fn a_stashed_failed_send_does_not_refold_files_and_pastes(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("stash-refold");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let path = session_at("stash-refold.jsonl").path;
            // The send already folded the file and paste into the text; the
            // composer still carries the original trays.
            let composer = ComposerDraft {
                text: "message\n\n@file.txt\n\npasted".into(),
                attachments: vec![],
                files: vec![AttachedFile {
                    path: PathBuf::from("/file.txt"),
                    name: "file.txt".into(),
                    size: 1,
                }],
                pasted_texts: vec![PastedText {
                    title: "pasted".into(),
                    lines: 1,
                    content: "pasted".into(),
                }],
            };
            app.stash_failed_send(Some(&path), None, composer);
            app.restore_composer_draft(Some(&path), cx);
            assert!(app.files.is_empty(), "the already-folded file is not reinstated");
            assert!(
                app.pasted_texts.is_empty(),
                "the already-folded paste is not reinstated"
            );
            assert_eq!(
                app.input.read(cx).text(),
                "message\n\n@file.txt\n\npasted"
            );
        });
    });
}

/// A stashed failed send merges into newer work already saved to the session's
/// draft instead of replacing it.
#[gpui::test]
fn a_stashed_failed_send_keeps_newer_draft_work(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("stash-merge");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            let path = session_at("stash-merge.jsonl").path;
            // Newer work already saved to the session's draft.
            app.drafts.insert(
                path.clone(),
                ComposerDraft {
                    text: "newer work".into(),
                    ..Default::default()
                },
            );
            // The background send fails and stashes its unsent message + image.
            let failed = ComposerDraft {
                text: "failed send".into(),
                attachments: vec![Attachment {
                    name: "i.png".into(),
                    mime: "image/png".into(),
                    data: "aGk=".into(),
                    preview: None,
                    original_len: 2,
                    stored_len: 2,
                    compressed: false,
                }],
                ..Default::default()
            };
            app.stash_failed_send(Some(&path), None, failed);
            let draft = app.drafts.get(&path).expect("draft");
            assert!(draft.text.contains("failed send"), "failed text back: {}", draft.text);
            assert!(draft.text.contains("newer work"), "newer work kept: {}", draft.text);
            assert_eq!(draft.attachments.len(), 1, "the failed image is kept");
        });
    });
}

/// A cold resume refused because the workspace is gone hands the images back
/// with the text (images were not folded into the text).
#[gpui::test]
fn a_refused_cold_resume_hands_the_images_back(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start("refusal-images");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = app_without_a_process(cx, &fake);

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let mut a = session_at("refusal-images.jsonl");
            a.cwd = PathBuf::from("/nonexistent-orbit-workspace");
            app.sessions.push(a.clone());
            app.switch_to_session(a.clone(), true, cx);
            app.attachments = vec![Attachment {
                name: "i.png".into(),
                mime: "image/png".into(),
                data: "aGk=".into(),
                preview: None,
                original_len: 2,
                stored_len: 2,
                compressed: false,
            }];
            app.input
                .update(cx, |input, cx| input.set_text("refused message", cx));
            let handle = app.input.read(cx).focus_handle(cx);
            window.focus(&handle);
            app.on_submit(&crate::Submit, window, cx);
            assert!(app.worktree_dialog.is_some(), "the send was refused");
            assert!(app.pending_prompt.is_none(), "nothing was queued");
            assert_eq!(app.attachments.len(), 1, "the image is back in the tray");
            assert!(app.input.read(cx).text().contains("refused message"));
        });
    });
}