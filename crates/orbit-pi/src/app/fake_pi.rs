//! A scripted `pi` for tests that need a live client but no agent: it stays
//! alive and swallows stdin. One module so a test that needs a process does
//! not grow its own copy.

use super::*;

/// A fake `pi` launcher, removed when the test drops it.
pub(super) struct FakePi {
    dir: PathBuf,
}

impl FakePi {
    pub(super) fn start(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "orbit-fake-pi-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("fake pi dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bin = dir.join("fake-pi");
            std::fs::write(&bin, "#!/bin/sh\nexec cat >/dev/null\n").expect("fake pi");
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
