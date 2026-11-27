//! A scripted `pi` for tests that need a live client but no agent: it stays
//! alive and swallows stdin. One module so a test that needs a process does
//! not grow its own copy.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// A fake `pi` launcher, removed when the test drops it.
pub(super) struct FakePi {
    dir: PathBuf,
}

impl FakePi {
    pub(super) fn start(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-fake-pi-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("fake pi dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bin = dir.join("fake-pi");
            // Drain stdin and never write to stdout, while keeping the stdout
            // pipe open for as long as the client holds its end: the reader
            // thread reports EOF on stdout as `ProcessExited`, so a `cat
            // >/dev/null` (which closes the pipe the moment it starts) announced
            // the death of a child that was still running, and a test that
            // pumped the event stream saw its client dropped underneath it.
            std::fs::write(&bin, "#!/bin/sh\nwhile IFS= read -r line; do :; done\n")
                .expect("fake pi");
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        #[cfg(windows)]
        {
            std::fs::write(
                dir.join("fake-pi.cmd"),
                "@echo off\r\nping -n 60 127.0.0.1 > nul\r\n",
            )
            .expect("fake pi");
        }
        Self { dir }
    }

    pub(super) fn bin(&self) -> PathBuf {
        #[cfg(unix)]
        {
            self.dir.join("fake-pi")
        }
        #[cfg(windows)]
        {
            self.dir.join("fake-pi.cmd")
        }
    }
}

impl Drop for FakePi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Install the globals the app's render-read paths expect.
pub(super) fn prepare(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeKey::ORBIT)));
}

/// A live client running `fake` in `cwd` — the child a spawn would have
/// produced, with no agent behind it.
pub(super) fn spawn_client(fake: &FakePi, cwd: &Path) -> PiClient {
    PiClient::spawn_with_bin(fake.bin().to_string_lossy().as_ref(), cwd, None)
        .expect("spawn keep-alive")
}

/// Adopt a live fake client for the open workspace, the way a spawn would, so
/// the test can drive the paths that need a process without starting one.
pub(super) fn adopt_alive(app: &mut OrbitApp, fake: &FakePi) {
    let workspace = app
        .current_workspace
        .clone()
        .unwrap_or_else(|| std::env::temp_dir());
    let client = spawn_client(fake, &workspace);
    app.adopt_client(client);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The promise the tests above lean on: a client adopted from the fake is
    /// still there when the test pumps the heartbeat. The reader thread reads
    /// EOF on the child's stdout as `ProcessExited` and the app then drops the
    /// client, so the keep-alive script has to hold that pipe open — a fake
    /// that closes it drops the client underneath any test that ticks.
    #[gpui::test]
    fn a_keep_alive_client_outlives_a_tick_taken_after_the_child_started(
        cx: &mut gpui::TestAppContext,
    ) {
        let fake = FakePi::start("keep-alive");
        prepare(cx);
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));

        cx.update(|_, cx| {
            app.update(cx, |app, _cx| adopt_alive(app, &fake));
        });
        // Long enough for the reader thread to have published an exit the
        // child did not have.
        std::thread::sleep(std::time::Duration::from_millis(300));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.tick(cx);
                assert!(
                    app.client.is_some(),
                    "the keep-alive child exited on its own while the test still held it"
                );
                app.drop_client();
            });
        });
    }
}
