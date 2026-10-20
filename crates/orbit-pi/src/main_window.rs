//! Opening — and re-opening — the app's one main window.
//!
//! macOS does not quit an app when its last window closes. The Dock icon
//! stays (with the running dot), and clicking it asks the app to show a
//! window again: `applicationShouldHandleReopen:hasVisibleWindows:`, which
//! GPUI surfaces as [`Application::on_reopen`]. An app that never installs a
//! handler answers nothing, so the Dock click is a no-op and only a full quit
//! brings a window back. Orbit installs one here.
//!
//! Reopening restores the *same* shell rather than building a second one —
//! see [`Shell`].

use std::{cell::RefCell, time::Duration};

use gpui::{
    prelude::*, px, size, App, Application, AsyncWindowContext, Bounds, Entity, Focusable, Global,
    Timer, WindowBounds, WindowOptions,
};

use crate::{app::OrbitApp, platform, theme};

/// The shell model, parked while no window is open.
///
/// A window only *views* this entity — the pi child process behind it, the
/// streaming transcript, the open session, every in-flight request live in
/// [`OrbitApp`]. Dropping the entity when the window closes would kill the
/// run the user closed the window over, so it is parked in a global instead
/// and the next window re-renders the state that is already there.
#[derive(Default)]
pub struct Shell(pub RefCell<Option<Entity<OrbitApp>>>);

impl Global for Shell {}

/// Answer the Dock's reopen request with a window.
///
/// Registered on the [`Application`] before it runs, because that is where
/// the platform callback lives; macOS only fires it when it reports no
/// visible window, which is exactly the windowless state this handles. The
/// emptiness check is belt-and-braces: a hidden-but-alive window already
/// responds to a Dock click on its own, and a second window is never what
/// the user asked for.
pub fn install_reopen_handler(application: &mut Application) {
    application.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open(cx);
        }
    });
}

/// Open the main window, reusing the parked shell when there is one.
pub fn open(cx: &mut App) {
    // Clone out and drop the borrow before the arms touch the global again.
    let parked = cx.global::<Shell>().0.borrow().clone();
    let app = match parked {
        // Already running: the same sessions, the same pi process, the same
        // stream. The window that watched the old window's activation is
        // gone with it, so the watcher is re-attached below.
        Some(app) => app,
        None => {
            let app = cx.new(OrbitApp::new);
            // Detect installed editors/terminals for the header "open in"
            // control (off-thread; icons load once).
            app.update(cx, |app, cx| app.detect_open_in_apps(cx));
            cx.global::<Shell>().0.borrow_mut().replace(app.clone());
            app
        }
    };

    // Open maximized: full width of the screen, filling the visible
    // frame. The computed bounds are the
    // restore size macOS returns to when the window is un-zoomed,
    // sized relative to the display so it always fits even on
    // small/scaled screens.
    let (w, h) = match cx.primary_display().map(|d| d.bounds().size) {
        Some(s) => (
            (f32::from(s.width) * 0.85).min(1440.),
            (f32::from(s.height) * 0.9).min(920.),
        ),
        None => (1240., 840.),
    };
    let restore_bounds = Bounds::centered(None, size(px(w), px(h)), cx);
    let _window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Maximized(restore_bounds)),
                // Keep the window usable when shrunk: sidebar (min
                // 200px) + a readable transcript + the composer.
                window_min_size: Some(size(px(960.), px(640.))),
                titlebar: Some(platform::titlebar_options()),
                app_id: Some("dev.orbit.pi".into()),
                focus: true,
                ..Default::default()
            },
            |window, cx| {
                theme::watch_system_appearance(window, cx);

                // Focus the composer so typing works immediately; track
                // window focus so background notifications know whether
                // the user is already looking at the transcript.
                app.update(cx, |app, cx| {
                    app.watch_window_activation(window, cx);
                    let handle = app.input.read(cx).focus_handle(cx);
                    window.focus(&handle);
                });

                // Heartbeat: drain pi events into the UI. Idle at ~90 ms;
                // while a run streams, poll at ~60 Hz so streamed tokens
                // land smoothly instead of in visible 90 ms batches.
                let heartbeat = app.clone();
                window
                    .spawn(cx, async move |cx: &mut AsyncWindowContext| {
                        let mut delay = Duration::from_millis(90);
                        loop {
                            Timer::after(delay).await;
                            let active = heartbeat
                                .update(cx, |app: &mut OrbitApp, cx| app.tick(cx))
                                .unwrap_or(false);
                            delay = if active {
                                Duration::from_millis(16)
                            } else {
                                Duration::from_millis(90)
                            };
                        }
                    })
                    .detach();

                app
            },
        )
        .unwrap();
}
