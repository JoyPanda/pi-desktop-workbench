//! Regression tests for issue #46: switching away from a session must park its
//! process, never drop it. A session that has not claimed its file yet (pi is
//! still booting, or the `new_session` reply is in flight) parks into the
//! pending pool and re-keys into `lives` once the boot handshake names the
//! file — otherwise the next `adopt_client` overwrites (and kills) a live run.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeKey};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;

/// A fake pi that swallows stdin, so the transport sees a live child without
/// spawning the real CLI. Deleted when the test ends.
struct FakePi {
    dir: PathBuf,
    bin: PathBuf,
}

impl FakePi {
    fn start(tag: &str) -> Self {
        Self::install(tag, "#!/bin/sh\nexec cat >/dev/null\n".to_string())
    }

    /// A pi that is slow to answer `new_session` (the boot window where the
    /// session file is unknown), then names its file on `get_state`.
    fn slow_boot(tag: &str, session_path: &str) -> Self {
        let script = format!(
            r#"#!/bin/sh
emit() {{ printf '%s\n' "$1" | sed "s/^{{/{{\"id\":\"$id\",/"; }}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"type":"new_session"'*)
      sleep 1
      emit '{{"type":"response","command":"new_session","success":true,"data":{{"cancelled":false}}}}'
      ;;
    *'"type":"get_state"'*)
      emit '{{"type":"response","command":"get_state","success":true,"data":{{"sessionFile":"{session}","sessionId":"sid"}}}}'
      ;;
    *) emit '{{"type":"response","success":true}}' ;;
  esac
done
"#,
            session = session_path
        );
        Self::install(tag, script)
    }

    fn install(tag: &str, script: String) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "orbit-park-{tag}-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create fake-pi dir");
        let bin = dir.join("fake-pi");
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

fn spawn_fake(app: &mut OrbitApp, fake: &FakePi) -> PiClient {
    let workspace = app
        .current_workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
        .expect("spawn fake pi")
}

fn child_alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Drive the app's heartbeat until `done` holds or the timeout lapses.
fn pump_until(
    cx: &mut gpui::TestAppContext,
    app: &Entity<OrbitApp>,
    timeout: Duration,
    done: impl Fn(&OrbitApp) -> bool,
) {
    let deadline = Instant::now() + timeout;
    loop {
        cx.update(|cx| app.update(cx, |app, cx| app.tick(cx)));
        if cx.update(|cx| done(app.read(cx))) || Instant::now() > deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A claimed session parks straight into the warm pool, as before.
#[gpui::test]
fn claimed_running_session_parks_into_the_warm_pool(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let session = PathBuf::from("/tmp/orbit-park-tests/claimed.jsonl");
    let fake = FakePi::start("claimed");
    let mut pid = 0;
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &fake);
            pid = client.child_pid();
            app.adopt_client(client);
            app.current_session_path = Some(session.clone());
            app.busy = true;

            app.park_active_session();

            assert!(
                app.pending_parks.is_empty(),
                "no pending entry for a claimed file"
            );
            let parked = app.lives.get(&session).expect("parked under its path");
            assert!(parked.busy, "a mid-run session parks busy");
            assert!(child_alive(pid), "the parked process must stay alive");
        });
    });
}

/// An unclaimed running session parks into the pending pool; the next
/// `adopt_client` cannot kill it, and the boot handshake re-keys it into
/// `lives` under its file.
#[gpui::test]
fn unclaimed_running_session_parks_pending_and_rekeys(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let session = "/tmp/orbit-park-tests/pending.jsonl";
    let running = FakePi::slow_boot("pending", session);
    let replacement = FakePi::start("pending-replacement");
    let mut running_pid = 0;
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &running);
            running_pid = client.child_pid();
            app.adopt_client(client);
            app.current_session_path = None;
            app.busy = true;
            app.send(CommandBody::NewSession, "new_session");

            app.park_active_session();

            assert_eq!(
                app.pending_parks.len(),
                1,
                "an unclaimed session must park pending, not stay active"
            );
            assert!(app.pending_parks[0].busy, "a mid-run session parks busy");
            assert!(app.lives.is_empty(), "no file path to key it by yet");

            // The kill scenario from #46: adopting the incoming session used
            // to overwrite (and drop) the outgoing client.
            let incoming = spawn_fake(app, &replacement);
            app.adopt_client(incoming);
            assert_eq!(app.pending_parks.len(), 1);
            assert!(
                child_alive(running_pid),
                "the pending run's process must survive the adopt"
            );
        });
    });

    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.pending_parks.is_empty() && app.lives.contains_key(Path::new(session))
    });

    cx.update(|cx| {
        app.update(cx, |app, _| {
            assert!(
                app.pending_parks.is_empty(),
                "the boot handshake must re-key it"
            );
            let parked = app
                .lives
                .get(Path::new(session))
                .expect("re-keyed into lives under its session file");
            assert!(parked.busy, "the run stays busy across the re-key");
            assert!(
                child_alive(running_pid),
                "the run's process must stay alive"
            );
        });
    });
}

/// The session-navigation shortcuts (⌘1…⌘9 / Ctrl+Tab) route through
/// `switch_to_session` exactly like a sidebar click. Switching to a warm
/// target must park — never drop — the outgoing running process.
#[gpui::test]
fn shortcut_switch_to_a_warm_session_keeps_the_running_process(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let running_path = PathBuf::from("/tmp/orbit-park-tests/shortcut-running.jsonl");
    let target_path = PathBuf::from("/tmp/orbit-park-tests/shortcut-target.jsonl");
    let running = FakePi::start("shortcut-running");
    let target = FakePi::start("shortcut-target");
    let mut running_pid = 0;
    let target_session = SessionInfo {
        path: target_path.clone(),
        id: "target".into(),
        cwd: PathBuf::from("/tmp/orbit-park-tests"),
        title: "Target".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            // The live, mid-run session the shortcut switches away from.
            let client = spawn_fake(app, &running);
            running_pid = client.child_pid();
            app.adopt_client(client);
            app.current_session_path = Some(running_path.clone());
            app.busy = true;

            // The target is already warm in `lives`, so the switch takes the
            // resume path and never spawns a real pi.
            let parked_target = spawn_fake(app, &target);
            let target_pid = parked_target.child_pid();
            let stamp = app.mcp.fingerprint_for(Some(&target_session.cwd));
            app.park(
                target_path.clone(),
                ParkedSession {
                    client: parked_target,
                    transcript: Transcript::new(),
                    busy: false,
                    added: 0,
                    removed: 0,
                    mcp_stamp: stamp,
                    turns: crate::turn_review::TurnClock::default(),
                    widgets: Vec::new(),
                    subagents: crate::subagent_status::SubagentBoard::default(),
                    subagent_scrolls: HashMap::new(),
                    subagent_list_scroll: ScrollHandle::new(),
                    queue: PendingQueue::default(),
                    parked_at: Instant::now(),
                    execution: SessionExecution::default(),
                    interaction: ParkedInteraction::default(),
                    retry_detail: None,
                },
            );

            app.switch_to_session(target_session.clone(), true, cx);

            assert_eq!(
                app.current_session_path.as_ref(),
                Some(&target_path),
                "the switch lands on the target"
            );
            assert!(
                app.lives.contains_key(&running_path),
                "the outgoing running session parks under its file"
            );
            assert!(
                child_alive(running_pid),
                "the outgoing run's process must survive the switch"
            );
            assert!(child_alive(target_pid), "the target process is now active");
        });
    });
}

/// The outgoing run must keep *streaming* after a switch, not just keep its
/// process alive: the parked transcript keeps draining deltas until the turn
/// settles.
#[gpui::test]
fn parked_running_session_keeps_streaming_after_a_shortcut_switch(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let running_path = PathBuf::from("/tmp/orbit-park-tests/stream-running.jsonl");
    let target_path = PathBuf::from("/tmp/orbit-park-tests/stream-target.jsonl");
    // Streams "Hello" on boot, then — after the switch — " world" and settle.
    let script = r#"#!/bin/sh
emit() { printf '%s\n' "$1"; }
emit '{"type":"agent_start"}'
emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Hello"}}'
sleep 1
emit '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":" world"}}'
emit '{"type":"agent_end","willRetry":false}'
emit '{"type":"agent_settled"}'
exec cat >/dev/null
"#;
    let running = FakePi::install("stream-running", script.to_string());
    let target = FakePi::start("stream-target");
    let target_session = SessionInfo {
        path: target_path.clone(),
        id: "target".into(),
        cwd: PathBuf::from("/tmp/orbit-park-tests"),
        title: "Target".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };

    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &running);
            app.adopt_client(client);
            app.current_session_path = Some(running_path.clone());
        });
    });

    // Wait for the first delta so the session is genuinely mid-stream.
    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.transcript.is_streaming()
    });

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let parked_target = spawn_fake(app, &target);
            let stamp = app.mcp.fingerprint_for(Some(&target_session.cwd));
            app.park(
                target_path.clone(),
                ParkedSession {
                    client: parked_target,
                    transcript: Transcript::new(),
                    busy: false,
                    added: 0,
                    removed: 0,
                    mcp_stamp: stamp,
                    turns: crate::turn_review::TurnClock::default(),
                    widgets: Vec::new(),
                    subagents: crate::subagent_status::SubagentBoard::default(),
                    subagent_scrolls: HashMap::new(),
                    subagent_list_scroll: ScrollHandle::new(),
                    queue: PendingQueue::default(),
                    parked_at: Instant::now(),
                    execution: SessionExecution::default(),
                    interaction: ParkedInteraction::default(),
                    retry_detail: None,
                },
            );
            app.switch_to_session(target_session.clone(), true, cx);
            assert!(
                app.lives.contains_key(&running_path),
                "the streaming session parks under its file"
            );
        });
    });

    // The remaining deltas must reach the *parked* transcript and settle it.
    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.lives
            .get(&running_path)
            .is_some_and(|parked| !parked.busy && !parked.transcript.is_streaming())
    });

    cx.update(|cx| {
        app.update(cx, |app, _| {
            let parked = app.lives.get(&running_path).expect("still parked");
            let (_, text) = parked
                .transcript
                .last_response_text()
                .expect("the parked run produced a response");
            assert!(
                text.contains("Hello world"),
                "the parked run kept streaming: {text:?}"
            );
        });
    });
}

/// Unlike native approvals and inputs, a custom terminal surface cannot be
/// parked. Its cancellation must still use pi's top-level `cancelled` flag.
#[gpui::test]
fn parked_session_cancels_custom_ui_with_a_top_level_flag(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let out = std::env::temp_dir().join(format!(
        "orbit-park-dialog-{}-{:?}.txt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&out);
    // Capture the response for an unsupported background custom surface.
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' '{{"type":"extension_ui_request","id":"req-1","method":"custom"}}'
IFS= read -r line
printf '%s\n' "$line" > "{out}"
exec cat >/dev/null
"#,
        out = out.display()
    );
    let fake = FakePi::install("dialog-cancel", script);

    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &fake);
            app.adopt_client(client);
            // Unclaimed, so it parks pending — the same pool a mid-boot
            // session lands in — and the request drains there.
            app.current_session_path = None;
            app.busy = true;
            app.park_active_session();
            assert_eq!(app.pending_parks.len(), 1);
        });
    });

    pump_until(cx, &app, Duration::from_secs(5), |_| out.exists());

    let line = std::fs::read_to_string(&out).expect("fake pi captured the cancel");
    let _ = std::fs::remove_file(&out);
    let wire: serde_json::Value =
        serde_json::from_str(line.trim()).expect("the cancel is valid JSON");
    assert_eq!(wire["type"], "extension_ui_response");
    assert_eq!(wire["id"], "req-1");
    assert_eq!(wire["cancelled"], true, "cancel must be top-level: {line}");
    assert!(
        wire.get("value").is_none(),
        "cancel must not be nested under `value`: {line}"
    );
}

/// A running session must survive a round-trip through a session in another
/// workspace. The MCP stamp is workspace-specific, so comparing a parked
/// session's stamp against the *current* workspace's fingerprint made every
/// cross-workspace switch look like a config change and drop — killing — the
/// parked run (issue #46 follow-up).
#[gpui::test]
fn cross_workspace_switch_back_resumes_the_running_session(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let ws_a = PathBuf::from("/tmp/orbit-park-tests/ws-a");
    let ws_b = PathBuf::from("/tmp/orbit-park-tests/ws-b");
    std::fs::create_dir_all(&ws_a).expect("ws-a");
    std::fs::create_dir_all(&ws_b).expect("ws-b");
    let running_path = ws_a.join("running.jsonl");
    let target_path = ws_b.join("target.jsonl");
    let running = FakePi::start("xws-running");
    let target = FakePi::start("xws-target");
    let mut running_pid = 0;
    let running_session = SessionInfo {
        path: running_path.clone(),
        id: "running".into(),
        cwd: ws_a.clone(),
        title: "Running".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };
    let target_session = SessionInfo {
        path: target_path.clone(),
        id: "target".into(),
        cwd: ws_b.clone(),
        title: "Target".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            // A live run in workspace A, stamped like a process from yet
            // another workspace — what the lagging current-workspace
            // fingerprint used to produce. Only "never drop a busy run"
            // can save it when switching back.
            let client = spawn_fake(app, &running);
            running_pid = client.child_pid();
            app.adopt_client(client);
            app.current_session_path = Some(running_path.clone());
            app.current_workspace = Some(ws_a.clone());
            app.mcp_stamp = app
                .mcp
                .fingerprint_for(Some(Path::new("/tmp/orbit-park-tests/ws-stale")));
            app.busy = true;

            // The target is warm in workspace B, keyed to B's own fingerprint.
            let parked_target = spawn_fake(app, &target);
            let stamp_b = app.mcp.fingerprint_for(Some(&ws_b));
            app.park(
                target_path.clone(),
                ParkedSession {
                    client: parked_target,
                    transcript: Transcript::new(),
                    busy: false,
                    added: 0,
                    removed: 0,
                    mcp_stamp: stamp_b,
                    turns: crate::turn_review::TurnClock::default(),
                    widgets: Vec::new(),
                    subagents: crate::subagent_status::SubagentBoard::default(),
                    subagent_scrolls: HashMap::new(),
                    subagent_list_scroll: ScrollHandle::new(),
                    queue: PendingQueue::default(),
                    parked_at: Instant::now(),
                    execution: SessionExecution::default(),
                    interaction: ParkedInteraction::default(),
                    retry_detail: None,
                },
            );

            // A -> B (A parks) then B -> A (must resume A, not kill it).
            app.switch_to_session(target_session.clone(), true, cx);
            assert_eq!(app.current_session_path.as_ref(), Some(&target_path));
            assert!(child_alive(running_pid), "A keeps running while B is open");

            app.switch_to_session(running_session.clone(), true, cx);
            assert_eq!(
                app.current_session_path.as_ref(),
                Some(&running_path),
                "the switch back lands on A"
            );
            assert_eq!(
                app.client.as_ref().map(PiClient::child_pid),
                Some(running_pid),
                "A resumes its own warm process"
            );
            assert!(child_alive(running_pid), "A's run was not killed");
        });
    });
}

/// Issue #57: the pending queue must survive a switch away and back. pi only
/// emits `queue_update` when the queue changes, so a warm session that
/// reopens with an unchanged queue would show an empty bar unless Orbit keeps
/// the mirror on the parked session.
#[gpui::test]
fn queued_messages_survive_a_switch_away_and_back(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let running_path = PathBuf::from("/tmp/orbit-park-tests/queue-running.jsonl");
    let target_path = PathBuf::from("/tmp/orbit-park-tests/queue-target.jsonl");
    let running = FakePi::start("queue-running");
    let target = FakePi::start("queue-target");
    let running_session = SessionInfo {
        path: running_path.clone(),
        id: "running".into(),
        cwd: PathBuf::from("/tmp/orbit-park-tests"),
        title: "Running".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };
    let target_session = SessionInfo {
        path: target_path.clone(),
        id: "target".into(),
        cwd: PathBuf::from("/tmp/orbit-park-tests"),
        title: "Target".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            // A live run with a follow-up queued behind it — the bar above
            // the composer is showing it.
            let client = spawn_fake(app, &running);
            app.adopt_client(client);
            app.current_session_path = Some(running_path.clone());
            app.busy = true;
            app.queue.follow_up.push("summarize when done".into());

            let parked_target = spawn_fake(app, &target);
            let stamp = app.mcp.fingerprint_for(Some(&target_session.cwd));
            app.park(
                target_path.clone(),
                ParkedSession {
                    client: parked_target,
                    transcript: Transcript::new(),
                    busy: false,
                    added: 0,
                    removed: 0,
                    mcp_stamp: stamp,
                    turns: crate::turn_review::TurnClock::default(),
                    widgets: Vec::new(),
                    subagents: crate::subagent_status::SubagentBoard::default(),
                    subagent_scrolls: HashMap::new(),
                    subagent_list_scroll: ScrollHandle::new(),
                    queue: PendingQueue::default(),
                    parked_at: Instant::now(),
                    execution: SessionExecution::default(),
                    interaction: ParkedInteraction::default(),
                    retry_detail: None,
                },
            );

            app.switch_to_session(target_session.clone(), true, cx);
            assert!(
                app.queue.is_empty(),
                "the target shows its own (empty) queue"
            );

            app.switch_to_session(running_session.clone(), true, cx);
            assert_eq!(
                app.queue.follow_up,
                vec!["summarize when done".to_string()],
                "the queued message returns with the session"
            );
        });
    });
}

/// The parked mirror must not go stale: a `queue_update` drained while the
/// session is in the background has to replace the value captured at park
/// time, so a follow-up pi already delivered is not shown again on resume.
#[gpui::test]
fn parked_queue_tracks_updates_while_away(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let running_path = PathBuf::from("/tmp/orbit-park-tests/queue-drain.jsonl");
    let target_path = PathBuf::from("/tmp/orbit-park-tests/queue-drain-target.jsonl");
    // Emits an empty `queue_update` a beat after boot (pi drained the
    // follow-up), then stays alive in the background.
    let script = r#"#!/bin/sh
sleep 1
printf '%s\n' '{"type":"queue_update","steering":[],"followUp":[]}'
exec cat >/dev/null
"#;
    let running = FakePi::install("queue-drain", script.to_string());
    let target = FakePi::start("queue-drain-target");
    let running_session = SessionInfo {
        path: running_path.clone(),
        id: "running".into(),
        cwd: PathBuf::from("/tmp/orbit-park-tests"),
        title: "Running".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };
    let target_session = SessionInfo {
        path: target_path.clone(),
        id: "target".into(),
        cwd: PathBuf::from("/tmp/orbit-park-tests"),
        title: "Target".into(),
        first_message: "hi".into(),
        modified: SystemTime::UNIX_EPOCH,
    };

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let client = spawn_fake(app, &running);
            app.adopt_client(client);
            app.current_session_path = Some(running_path.clone());
            app.busy = true;
            app.queue.follow_up.push("delivered while away".into());

            let parked_target = spawn_fake(app, &target);
            let stamp = app.mcp.fingerprint_for(Some(&target_session.cwd));
            app.park(
                target_path.clone(),
                ParkedSession {
                    client: parked_target,
                    transcript: Transcript::new(),
                    busy: false,
                    added: 0,
                    removed: 0,
                    mcp_stamp: stamp,
                    turns: crate::turn_review::TurnClock::default(),
                    widgets: Vec::new(),
                    subagents: crate::subagent_status::SubagentBoard::default(),
                    subagent_scrolls: HashMap::new(),
                    subagent_list_scroll: ScrollHandle::new(),
                    queue: PendingQueue::default(),
                    parked_at: Instant::now(),
                    execution: SessionExecution::default(),
                    interaction: ParkedInteraction::default(),
                    retry_detail: None,
                },
            );
            app.switch_to_session(target_session.clone(), true, cx);
        });
    });

    // The parked process drains the empty queue.
    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.lives
            .get(&running_path)
            .is_some_and(|parked| parked.queue.is_empty())
    });

    cx.update(|cx| {
        app.update(cx, |app, cx| {
            assert!(
                app.lives
                    .get(&running_path)
                    .is_some_and(|parked| parked.queue.is_empty()),
                "the parked mirror tracked the drained queue"
            );
            app.switch_to_session(running_session.clone(), true, cx);
            assert!(
                app.queue.is_empty(),
                "the delivered follow-up is not shown again on resume"
            );
        });
    });
}

#[gpui::test]
fn reading_a_result_does_not_erase_its_execution_outcome(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let path = PathBuf::from("/tmp/orbit-park-tests/completed.jsonl");
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.current_session_path = Some(path.clone());
            app.execution.start();
            app.execution
                .apply_event(&Event::AgentSettled { aborted: false });
            app.awaiting_review.insert(path.clone(), ReviewMark::Done);
            super::sidebar::clear_review_mark(&mut app.awaiting_review, Some(&path), true);
            assert_eq!(
                app.sidebar_session_statuses().get(&path),
                Some(&SessionStatus::Completed)
            );
            assert!(!app.awaiting_review.contains_key(&path));
        });
    });
}

#[gpui::test]
fn a_background_approval_stays_pending_and_opens_on_resume(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = FakePi::install("pending-approval", r#"#!/bin/sh
printf '%s\n' '{"type":"agent_start"}' '{"type":"extension_ui_request","id":"allow-edit","method":"select","title":"[orbit-guard] edit\tfile.rs","options":["Allow once","Deny"]}'
while IFS= read -r line; do :; done
"#.to_string());
    let path = fake.dir.join("session.jsonl");
    let session = SessionInfo {
        path: path.clone(),
        id: "pending-approval".into(),
        cwd: fake.dir.clone(),
        title: "Pending approval".into(),
        first_message: "edit a file".into(),
        modified: SystemTime::now(),
    };
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &fake);
            app.adopt_client(client);
            app.current_session_path = Some(path.clone());
            app.busy = true;
            app.execution.start();
            app.park_active_session();
        });
    });
    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.lives
            .get(&path)
            .is_some_and(|parked| parked.execution.status() == SessionStatus::WaitingApproval)
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let parked = app.lives.get(&path).expect("session remains parked");
            assert_eq!(parked.execution.status(), SessionStatus::WaitingApproval);
            assert!(parked.busy, "waiting work must not be reaped as idle");
            app.switch_to_session(session.clone(), true, cx);
            assert_eq!(
                app.approval.as_ref().map(|request| request.id.as_str()),
                Some("allow-edit")
            );
            assert_eq!(app.execution.status(), SessionStatus::WaitingApproval);
            // Switching away again must park this exact approval, not deny it.
            app.park_active_session();
            assert!(app.approval.is_none());
            app.switch_to_session(session.clone(), true, cx);
            assert_eq!(
                app.approval.as_ref().map(|request| request.id.as_str()),
                Some("allow-edit")
            );
            app.approval = None;
            app.respond_to_dialog("allow-edit", &DialogResponse::Value("Allow once".into()));
            assert_eq!(app.execution.status(), SessionStatus::Running);
        });
    });
}

#[gpui::test]
fn background_completion_survives_process_reaping_and_reading(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = FakePi::install("completed-status", r#"#!/bin/sh
printf '%s\n' '{"type":"agent_start"}' '{"type":"message_start","message":{"role":"assistant","content":[]}}' '{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"Done"}],"stopReason":"stop"}}' '{"type":"agent_end","willRetry":false}' '{"type":"agent_settled","aborted":false}'
exec cat >/dev/null
"#.to_string());
    let path = fake.dir.join("session.jsonl");
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &fake);
            app.adopt_client(client);
            app.current_session_path = Some(path.clone());
            app.busy = true;
            app.park_active_session();
        });
    });
    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.session_outcomes.get(&path) == Some(&SessionStatus::Completed)
    });
    cx.update(|cx| {
        app.update(cx, |app, _| {
            assert_eq!(
                app.sidebar_session_statuses().get(&path),
                Some(&SessionStatus::Completed)
            );
            app.lives.remove(&path);
            app.awaiting_review.remove(&path);
            assert_eq!(
                app.sidebar_session_statuses().get(&path),
                Some(&SessionStatus::Completed)
            );
        });
    });
}
