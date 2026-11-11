//! Regression tests: a stop pi never answers used to be a dead button.
//!
//! `abort` is only a request. When pi's event loop is stuck — the shape a
//! multi-GB bash result leaves behind — it never reads the line, and pi's own
//! `abort()` waits for the run to go idle, so no `agent_settled` ever arrives.
//! Stop and Esc both went silent while the turn stayed "Working for…" forever.
//!
//! A stop now carries its timestamp and, once the grace has run out (or on a
//! second press after the retry window), Orbit stops waiting for pi: it closes
//! the turn locally, drops the process — the client kills the whole tree, so the
//! wedged tool child goes with it — and starts a fresh pi on the same session.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeKey};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

/// A pi that reads stdin and answers nothing: the stand-in for a process whose
/// event loop is blocked behind a tool child.
struct SilentPi {
    dir: PathBuf,
    bin: PathBuf,
}

impl SilentPi {
    fn start() -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-stalled-stop-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create silent-pi dir");
        let bin = dir.join("silent-pi");
        std::fs::write(&bin, "#!/bin/sh\nexec cat >/dev/null\n").expect("write silent pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod silent pi");
        Self { dir, bin }
    }
}

impl Drop for SilentPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn app_with_silent_pi(cx: &mut gpui::TestAppContext) -> (Entity<OrbitApp>, SilentPi) {
    let fake = SilentPi::start();
    let app = cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeKey::ORBIT));
        cx.new(OrbitApp::new)
    });
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let workspace = std::env::temp_dir();
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
                    .expect("spawn silent pi");
            app.adopt_client(client);
        })
    });
    (app, fake)
}

/// The pid of the active process — the handle that tells one pi from its
/// replacement.
fn active_pid(app: &mut OrbitApp) -> Option<u32> {
    app.client.as_mut().map(PiClient::child_pid)
}

/// A stop with a run behind it is timestamped; a stop on an idle composer is
/// not, so Esc on nothing can never escalate against a live process.
#[gpui::test]
fn only_a_stop_with_a_run_behind_it_is_timestamped(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_silent_pi(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.request_abort(cx);
            assert!(
                !app.is_stopping(),
                "nothing is running, so there is nothing to give up on"
            );

            app.submit_as("run the tests".to_owned(), SendMode::FollowUp, cx);
            assert!(app.is_running(), "the send marks the run in flight");
            app.request_abort(cx);
            assert!(
                app.is_stopping(),
                "a stop with a run behind it is timestamped for the heartbeat"
            );
        });
    });
}

/// Inside the grace nothing happens: pi still gets to answer the abort itself,
/// and the composer reports that it is waiting on it.
#[gpui::test]
fn a_stop_still_inside_the_grace_leaves_pi_alone(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_silent_pi(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.submit_as("run the tests".to_owned(), SendMode::FollowUp, cx);
            app.request_abort(cx);
            let before = active_pid(app);

            app.check_stalled_stop(cx);

            assert_eq!(
                active_pid(app),
                before,
                "pi is still inside its grace and must not be taken down"
            );
            assert!(app.is_stopping(), "the stop is still outstanding");
        });
    });
}

/// The grace runs out with the run still open: Orbit ends it by taking the
/// process down, and the stop is spent rather than repeating every heartbeat.
#[gpui::test]
fn an_unanswered_stop_takes_the_process_down(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_silent_pi(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.submit_as("run the tests".to_owned(), SendMode::FollowUp, cx);
            app.request_abort(cx);
            let before = active_pid(app);
            assert!(before.is_some(), "a process was serving the run");

            // What the heartbeat sees once the grace has run out.
            app.abort_sent_at = Some(Instant::now() - ABORT_GRACE);
            app.check_stalled_stop(cx);

            assert!(
                !app.is_stopping(),
                "a stalled stop is spent once it has been acted on"
            );
            assert_ne!(
                active_pid(app),
                before,
                "the process that never answered the stop is gone"
            );
            assert!(
                !app.execution.status().is_in_progress(),
                "the run ended with the process that owned it"
            );
            assert!(
                !app.busy,
                "the composer leaves Stop here, without waiting for an event"
            );
            assert!(
                !app.transcript.is_awaiting_reply(),
                "the pending row comes down with the run"
            );
        });
    });
}

/// Pressing Stop again once the first request has been out long enough is the
/// user saying "now": no second grace period, the process goes at once.
#[gpui::test]
fn a_second_stop_after_the_retry_window_does_not_wait_out_the_grace(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_silent_pi(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.submit_as("run the tests".to_owned(), SendMode::FollowUp, cx);
            app.request_abort(cx);
            let before = active_pid(app);

            // The same request, pressed again a moment later.
            app.abort_sent_at = Some(Instant::now() - ABORT_RETRY_GRACE);
            app.request_abort(cx);

            assert_ne!(
                active_pid(app),
                before,
                "a second stop takes the process down instead of waiting out the grace"
            );
            assert!(!app.is_stopping(), "nothing is left waiting");
        });
    });
}

/// A double-tapped Esc is not a second stop: inside the retry window the first
/// request stands, so a healthy pi keeps the chance to write the aborted turn
/// into the session before anything is killed.
#[gpui::test]
fn a_second_stop_inside_the_retry_window_keeps_waiting(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_silent_pi(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.submit_as("run the tests".to_owned(), SendMode::FollowUp, cx);
            app.request_abort(cx);
            let before = active_pid(app);

            app.request_abort(cx);

            assert_eq!(
                active_pid(app),
                before,
                "the second press is the same gesture and must not kill pi"
            );
            assert!(
                app.is_stopping(),
                "the original request is still outstanding"
            );
        });
    });
}
