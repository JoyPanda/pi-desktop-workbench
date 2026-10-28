//! Regression tests for issue #76, cancel half: running a prompt and then
//! cancelling it still left the app showing "Working".
//!
//! The wire sequences below are what pi 1.4.0 emits, captured from a live
//! `--mode rpc` process. An aborted turn lands as a `message_end` carrying
//! `stopReason: "aborted"` + `errorMessage: "Request aborted"`, then `turn_end`,
//! `agent_end`, `agent_settled`. A cancel must end as a *clean* stop — no
//! failure banner — and the live "Working for…" row must close with it.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeKey};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

/// How pi can end a turn, for the stand-in below.
#[derive(Clone, Copy)]
enum Ending {
    /// The user cancelled: pi reports `aborted`, and must not read as a failure.
    Aborted,
    /// The user cancelled during an automatic retry: pi reports
    /// `auto_retry_end { success: false, finalError: "Retry cancelled" }`
    /// before settling (pi-core `_finishCancelledRetry`).
    CancelledRetry,
    /// The provider failed: pi reports `error`, and must read as a failure.
    Failed,
}

impl Ending {
    fn message_end(self) -> &'static str {
        match self {
            Ending::CancelledRetry | Ending::Aborted => {
                r#"{"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"aborted","errorMessage":"Request aborted"}}"#
            }
            Ending::Failed => {
                r#"{"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"error","errorMessage":"provider 500"}}"#
            }
        }
    }

    fn turn_end(self) -> String {
        self.message_end()
            .replacen("\"type\":\"message_end\"", "\"type\":\"turn_end\"", 1)
    }

    /// The lines pi writes after the abort arrives, in order.
    fn abort_lines(self) -> Vec<String> {
        let mut lines = vec![self.message_end().to_string(), self.turn_end()];
        if matches!(self, Ending::CancelledRetry) {
            lines.push(
                r#"{"type":"auto_retry_end","success":false,"attempt":1,"finalError":"Retry cancelled"}"#
                    .to_string(),
            );
        }
        lines.push(r#"{"type":"agent_end","willRetry":false}"#.to_string());
        lines.push(r#"{"type":"agent_settled"}"#.to_string());
        lines
    }
}

/// A pi stand-in that speaks the captured turn sequence and nothing else.
struct FakePi {
    dir: PathBuf,
    bin: PathBuf,
}

impl FakePi {
    fn start(ending: Ending) -> Self {
        // A per-test counter, not just the clock: two tests building a stand-in
        // in the same microsecond would otherwise share one script file, and
        // whichever wrote last would speak for both.
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-cancel-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create fake-pi dir");
        let bin = dir.join("fake-pi");
        // One event per line, exactly as pi's RPC mode writes them: the prompt
        // opens a turn, the abort closes it the way pi closed the live one.
        let abort = ending
            .abort_lines()
            .iter()
            .map(|line| format!("      emit '{line}'\n"))
            .collect::<String>();
        let script = format!(
            r#"#!/bin/sh
emit() {{ printf '%s\n' "$1"; }}
while IFS= read -r line; do
  case "$line" in
    *'"type":"prompt"'*)
      emit '{{"type":"agent_start"}}'
      emit '{{"type":"turn_start"}}'
      emit '{{"type":"message_start","message":{{"role":"assistant","content":[]}}}}'
      ;;
    *'"type":"abort"'*)
{abort}      ;;
  esac
done
"#
        );
        std::fs::write(&bin, script).expect("write fake pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake pi");
        Self { dir, bin }
    }
}

impl Drop for FakePi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeKey::ORBIT));
        cx.new(OrbitApp::new)
    })
}

/// Install `fake` as the active pi process, as a live spawn would.
fn adopt_fake_pi(app: &mut OrbitApp, fake: &FakePi) {
    let workspace = app
        .current_workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let client = PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
        .expect("spawn fake pi");
    app.adopt_client(client);
}

/// Tick until `done` holds. Events arrive on the reader thread, so the first
/// tick cannot be assumed to see them.
fn pump_until(
    app: &Entity<OrbitApp>,
    cx: &mut gpui::TestAppContext,
    done: impl Fn(&OrbitApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reached = app.update(cx, |app, cx| {
            app.tick(cx);
            done(app)
        });
        if reached {
            return;
        }
        assert!(Instant::now() < deadline, "the app never reached the state");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Send a prompt the way `submit` does, then cancel it the way Esc does.
fn prompt_then_cancel(app: &Entity<OrbitApp>, cx: &mut gpui::TestAppContext) {
    app.update(cx, |app, _| {
        // What `submit` leaves behind when the prompt goes out: the turn's clock
        // starts at the send, not at the first token.
        app.transcript.expect_reply();
        app.busy = true;
        app.send(
            CommandBody::Prompt {
                message: "count to 200".into(),
                images: None,
                streaming_behavior: None,
            },
            "prompt",
        );
    });
    pump_until(app, cx, |app| app.busy && app.transcript.is_streaming());
    app.update(cx, |app, _| {
        // Exactly what Esc does.
        app.request_abort();
    });
}

/// A cancelled run stops showing as work in progress, and the cancel itself is
/// not reported as a failure: pi sends `errorMessage: "Request aborted"` on the
/// aborted message, and treating that as an error would put a red banner up for
/// something the user asked for.
#[gpui::test]
fn cancelling_a_run_clears_the_working_state(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = FakePi::start(Ending::Aborted);
    cx.update(|cx| {
        app.update(cx, |app, _| adopt_fake_pi(app, &fake));
    });

    prompt_then_cancel(&app, cx);
    pump_until(&app, cx, |app| !app.is_running());

    app.update(cx, |app, _| {
        assert!(
            !app.is_running(),
            "an aborted run must stop reading as work in progress"
        );
        assert!(
            !app.transcript.is_awaiting_reply() && !app.transcript.is_streaming(),
            "the live \"Working for…\" row must close with the run"
        );
        assert!(
            app.error.is_none(),
            "a cancel is not a failure, got {:?}",
            app.error
        );
    });
}

/// The other direction, so the test above cannot pass vacuously: the same
/// wire shape with `stopReason: "error"` does raise the banner and does close
/// the working row.
#[gpui::test]
fn a_failed_turn_still_reports_the_failure(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = FakePi::start(Ending::Failed);
    cx.update(|cx| {
        app.update(cx, |app, _| adopt_fake_pi(app, &fake));
    });

    prompt_then_cancel(&app, cx);
    pump_until(&app, cx, |app| !app.is_running());

    app.update(cx, |app, _| {
        assert_eq!(
            app.error.as_deref(),
            Some("Agent error: provider 500"),
            "a failed turn is never swallowed"
        );
        assert!(
            !app.is_running(),
            "the failed turn still closes its working row"
        );
    });
}

/// Cancelling during an automatic retry is also a cancel, not a failure: pi
/// answers the abort with `auto_retry_end { success: false, finalError:
/// "Retry cancelled" }`, and reading that `success: false` as a failure left a
/// banner saying the retries failed for something the user asked to stop.
#[gpui::test]
fn cancelling_a_retry_does_not_report_a_failure(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = FakePi::start(Ending::CancelledRetry);
    cx.update(|cx| {
        app.update(cx, |app, _| adopt_fake_pi(app, &fake));
    });

    prompt_then_cancel(&app, cx);
    pump_until(&app, cx, |app| !app.is_running());

    app.update(cx, |app, _| {
        assert!(
            !app.retrying,
            "the cancelled retry must not leave the retry strip up"
        );
        assert!(
            app.error.is_none(),
            "cancelling a retry is not a failed retry, got {:?}",
            app.error
        );
    });
}

/// A pi that answers a prompt with the disposition pi's RPC mode reports for
/// it, and otherwise stays quiet — the shape of `/review`, which pi hands to an
/// extension command, and of a prompt that really does start a turn.
struct QuietPi {
    dir: PathBuf,
    bin: PathBuf,
}

impl QuietPi {
    fn start(disposition: &str) -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-quiet-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create quiet-pi dir");
        let bin = dir.join("fake-pi");
        // The prompt answer is what pi writes through `preflightResult`; a
        // handled command starts no run, so nothing else follows it. The answer
        // carries the id of the command it answers, which is how the app tells
        // the composer's own prompt from an extension command's.
        let started = if disposition == "started" {
            r#"      emit '{"id":"'"$id"'","type":"response","command":"prompt","success":true,"data":{"disposition":"started"}}'
      emit '{"type":"agent_start"}'
      emit '{"type":"turn_start"}'
      # A second prompt, from an extension command (the title regenerator),
      # answered while the run above is still streaming.
      emit '{"id":"title-1","type":"response","command":"prompt","success":true,"data":{"disposition":"handled"}}'
"#
        } else {
            r#"      emit '{"id":"'"$id"'","type":"response","command":"prompt","success":true,"data":{"disposition":"handled"}}'
      emit '{"type":"extension_ui_request","id":"n1","method":"notify","message":"Review cancelled","notifyType":"info"}'
"#
        };
        let script = format!(
            r#"#!/bin/sh
emit() {{ printf '%s\n' "$1"; }}
while IFS= read -r line; do
  case "$line" in
    *'"type":"prompt"'*)
      id=$(printf '%s\n' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
{started}      ;;
  esac
done
"#
        );
        std::fs::write(&bin, script).expect("write quiet pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir, bin }
    }
}

impl Drop for QuietPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The `/review` case from issue #76: pi answers `/review` as an *extension
/// command* (`disposition: "handled"`) and starts no run — its toast arrives,
/// so the command clearly worked, but the composer's optimistic "Working for…"
/// row had nothing left to clear it and stayed up forever. Cancelling the
/// custom UI could not help, because there was never a run to settle.
#[gpui::test]
fn a_command_pi_handled_stops_the_working_row(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = QuietPi::start("handled");
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let workspace = app
                .current_workspace
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8"), &workspace, None)
                    .expect("spawn quiet pi");
            app.adopt_client(client);
            // What `submit` leaves behind: the clock starts at the send, and the
            // prompt's id is the one the app recognises the answer by.
            app.transcript.expect_reply();
            app.busy = true;
            app.prompt_in_flight = app.send_tracked(
                CommandBody::Prompt {
                    message: "/review".into(),
                    images: None,
                    streaming_behavior: None,
                },
                "prompt",
            );
        });
    });

    pump_until(&app, cx, |app| !app.is_running());

    app.update(cx, |app, _| {
        assert!(
            !app.is_running(),
            "a handled command starts no turn, so nothing may stay \"working\""
        );
        assert!(
            !app.transcript.is_awaiting_reply() && !app.transcript.is_streaming(),
            "the live row must close when pi reports the prompt as handled"
        );
        assert!(
            app.error.is_none(),
            "handling a command is not a failure, got {:?}",
            app.error
        );
    });
}

/// The control: the same answer with `disposition: "started"` is a real turn,
/// and the working row must stay up until that turn settles. The stand-in also
/// answers a second, extension-command `prompt` (the title regenerator's) with
/// `handled` mid-run; that reply belongs to no composer turn and must leave the
/// live one alone.
#[gpui::test]
fn a_started_turn_keeps_the_working_row(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = QuietPi::start("started");
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let workspace = app
                .current_workspace
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8"), &workspace, None)
                    .expect("spawn quiet pi");
            app.adopt_client(client);
            app.transcript.expect_reply();
            app.busy = true;
            app.prompt_in_flight = app.send_tracked(
                CommandBody::Prompt {
                    message: "hello".into(),
                    images: None,
                    streaming_behavior: None,
                },
                "prompt",
            );
        });
    });

    pump_until(&app, cx, |app| app.busy);
    // Give both responses time to land: the started turn must not be un-armed by
    // its own reply, nor by the title command's `handled` one behind it.
    for _ in 0..20 {
        app.update(cx, |app, cx| {
            app.tick(cx);
        });
        std::thread::sleep(Duration::from_millis(5));
    }
    app.update(cx, |app, _| {
        assert!(
            app.is_running(),
            "a started turn is still working; its row must not close at the response"
        );
        assert!(
            app.transcript.is_awaiting_reply(),
            "the live row belongs to the run, not to the title command's reply"
        );
    });
}

/// A pi that runs a turn, starts an automatic retry after it fails, and reports
/// the retry as cancelled when the Retry strip's Cancel button aborts it — the
/// sequence pi-core emits from `_finishCancelledRetry`.
struct RetryingPi {
    dir: PathBuf,
    bin: PathBuf,
}

impl RetryingPi {
    fn start() -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-retry-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create retrying-pi dir");
        let bin = dir.join("fake-pi");
        let script = r#"#!/bin/sh
emit() { printf '%s\n' "$1"; }
while IFS= read -r line; do
  case "$line" in
    *'"type":"prompt"'*)
      emit '{"type":"agent_start"}'
      emit '{"type":"turn_start"}'
      emit '{"type":"auto_retry_start","attempt":1,"maxAttempts":3,"errorMessage":"provider 500","delayMs":1000}'
      ;;
    *'"type":"abort_retry"'*)
      emit '{"type":"auto_retry_end","success":false,"attempt":1,"finalError":"Retry cancelled"}'
      emit '{"type":"agent_end","willRetry":false}'
      emit '{"type":"agent_settled"}'
      ;;
  esac
done
"#
        .to_string();
        std::fs::write(&bin, script).expect("write retrying pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir, bin }
    }
}

impl Drop for RetryingPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The Retry strip's Cancel button is a second way to stop a retry, and it hits
/// the same pi report Esc does: `auto_retry_end { success: false, finalError:
/// "Retry cancelled" }`. Unless the request is recorded, that reads as retries
/// that failed and puts a banner up for a cancel the user asked for.
#[gpui::test]
fn the_cancel_retry_button_does_not_report_a_failure(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = RetryingPi::start();
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let workspace = app
                .current_workspace
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8"), &workspace, None)
                    .expect("spawn retrying pi");
            app.adopt_client(client);
            app.send(
                CommandBody::Prompt {
                    message: "hello".into(),
                    images: None,
                    streaming_behavior: None,
                },
                "prompt",
            );
        });
    });

    // The retry strip is up while pi waits to try again.
    pump_until(&app, cx, |app| app.retrying);
    app.update(cx, |app, cx| app.abort_retry(cx));
    pump_until(&app, cx, |app| !app.is_running());

    app.update(cx, |app, _| {
        assert!(
            !app.retrying,
            "the cancelled retry must not leave the retry strip up"
        );
        assert!(
            app.error.is_none(),
            "cancelling a retry is not a failed retry, got {:?}",
            app.error
        );
    });
}
