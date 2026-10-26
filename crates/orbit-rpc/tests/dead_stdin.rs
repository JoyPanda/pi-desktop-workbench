//! Regression coverage for issue #76: a single failed write to pi's stdin
//! killed the writer thread permanently while the child kept running. The
//! process still reported alive, so every later command was written into a
//! stdin nobody would ever drain again — the app showed a failure it could
//! never clear and stopped answering shortcuts.
//!
//! The fake server here closes its own stdin and then lives, which reproduces
//! exactly that shape: process alive, pipe dead. Unix-only because the fake
//! server is a shell script.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use orbit_rpc::{CommandBody, Event, PiClient};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

/// A pi stand-in that closes stdin and then blocks. Every later write fails;
/// the process itself never exits.
struct DeadPi {
    dir: PathBuf,
    ready: PathBuf,
}

impl DeadPi {
    fn start(tag: &str) -> Self {
        let unique = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "orbit-{tag}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let bin = dir.join("fake-pi");
        // `ready` is written only after stdin is closed: without that handshake
        // a send racing the shell would land in the still-open pipe buffer and
        // the writer would never see an error.
        let ready = dir.join("ready");
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nexec 0<&- 2>/dev/null || true\n: > '{}'\nexec sleep 120\n",
                ready.display()
            ),
        )
        .expect("write fake pi");
        exec(&bin);
        Self { dir, ready }
    }

    fn bin(&self) -> PathBuf {
        self.dir.join("fake-pi")
    }

    /// Block until the child has actually closed its stdin.
    fn wait_until_stdin_closed(&self) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !self.ready.exists() {
            assert!(
                Instant::now() < deadline,
                "the fake pi never got as far as closing stdin"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for DeadPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Make a scratch file an executable script.
fn exec(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod fake pi");
}

/// Poll until `probe` returns true, or the deadline lapses. The writer
/// discovers the dead pipe asynchronously, so the first read cannot be assumed
/// to already report it.
fn wait_for(mut probe: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if probe() {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The dead stdin is published, not swallowed: `is_alive` turns false even
/// though the child is still running, and an exit event reaches the app so it
/// can offer a reconnect instead of silently holding a zombie.
#[test]
fn a_dead_stdin_is_reported_as_exited_not_as_alive() {
    let fake = DeadPi::start("dead-stdin");
    let mut client =
        PiClient::spawn_with_bin(fake.bin().to_str().unwrap(), &fake.dir, None).expect("spawn");
    fake.wait_until_stdin_closed();

    // The pipe is already dead when this is queued, so the writer thread is the
    // one that discovers it.
    let _ = client.send(CommandBody::GetState);

    assert!(
        wait_for(|| !client.writer_alive()),
        "the writer thread must publish its own death"
    );
    assert!(
        !client.is_alive(),
        "a process that cannot be commanded is not alive"
    );

    assert!(
        wait_for(|| {
            client
                .drain_events()
                .into_iter()
                .any(|event| matches!(event, Event::ProcessExited))
        }),
        "the app must learn the transport died without waiting for an EOF that \
         a live child never sends"
    );
}

/// After the transport is gone, sends fail fast and stop registering response
/// waiters: the old shape grew the pending map by one leaked sender for every
/// command the user kept trying.
#[test]
fn a_send_after_the_pipe_died_registers_no_waiter() {
    let fake = DeadPi::start("dead-pending");
    let client =
        PiClient::spawn_with_bin(fake.bin().to_str().unwrap(), &fake.dir, None).expect("spawn");
    fake.wait_until_stdin_closed();

    let _ = client.send(CommandBody::GetState);
    assert!(
        wait_for(|| !client.writer_alive()),
        "the writer should notice the closed stdin"
    );

    let before = client.pending_responses();
    for _ in 0..10 {
        assert!(
            client.send(CommandBody::GetState).is_err(),
            "sending into a dead transport must not look accepted"
        );
    }
    assert_eq!(
        client.pending_responses(),
        before,
        "ten refused sends must not leave ten waiters behind"
    );
}

/// A healthy server is unaffected: writes land, the writer stays alive, and
/// `is_alive` keeps tracking the child rather than the pipe.
#[test]
fn a_live_stdin_stays_alive() {
    let unique = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "orbit-live-stdin-{}-{nanos}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    let bin = dir.join("fake-pi");
    let log = dir.join("stdin.log");
    std::fs::write(
        &bin,
        format!("#!/bin/sh\nexec cat >> \"{}\"\n", log.display()),
    )
    .expect("write");
    exec(&bin);

    let mut client = PiClient::spawn_with_bin(bin.to_str().unwrap(), &dir, None).expect("spawn");
    assert!(
        client.send(CommandBody::GetState).is_ok(),
        "a live transport accepts commands"
    );
    assert!(client.is_alive(), "an idle live child is alive");

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("\"get_state\"")
        {
            break;
        }
        assert!(Instant::now() < deadline, "the command never landed");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(client.writer_alive(), "a healthy writer stays healthy");

    drop(client);
    let _ = std::fs::remove_dir_all(&dir);
}
