//! Analytics wiring for the workbench.
//!
//! This module is the only place the app touches telemetry. It owns the
//! process-wide [`AnalyticsClient`] (a GPUI global) and exposes a single
//! [`track`] helper so no call site needs to know about Countly. Events are
//! queued in memory and sent by a background worker; the UI thread is never
//! blocked and failures are non-fatal.
//!
//! Privacy: an opt-out choice lives in `~/.orbit-pi/analytics.json` beside the
//! pseudonymous installation id. Disabling clears pending events immediately.

use std::sync::Arc;

use gpui::{App, Global};
use orbit_analytics::{
    config_from_env, Analytics, AnalyticsClient, AnalyticsConfig, AnalyticsEvent, FileStore,
    StateStore,
};

/// Process-wide analytics handle. Absent in tests that never call [`init`], so
/// every [`track`] call is a no-op there.
pub struct AnalyticsState(pub Arc<AnalyticsClient>);

impl Global for AnalyticsState {}

fn state_path() -> std::path::PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("analytics.json")
}

/// Build the configuration from the environment, wiring in the app's own
/// version and locale so telemetry identifies the product rather than the
/// `orbit-analytics` crate's version.
pub fn config() -> AnalyticsConfig {
    let mut config = config_from_env();
    let locale = rust_i18n::locale().to_string();
    config.metadata = config
        .metadata
        .with_app_version(env!("CARGO_PKG_VERSION"))
        .with_locale(locale);
    config
}

/// Start analytics: load/create the installation id, begin a session, and
/// record the launch. Never blocks and never fails the app.
pub fn init(cx: &mut App) {
    let store: Arc<dyn StateStore> = Arc::new(FileStore::new(state_path()));
    let client = Arc::new(AnalyticsClient::new(config(), store));
    client.start_session();
    client.track(AnalyticsEvent::SessionStarted);
    client.track(AnalyticsEvent::AppStarted);
    cx.set_global(AnalyticsState(client));
}

/// Record an event through the global client. A no-op before [`init`].
pub fn track(cx: &App, event: AnalyticsEvent) {
    if let Some(state) = cx.try_global::<AnalyticsState>() {
        state.0.track(event);
    }
}

/// Whether anonymous analytics is currently on.
pub fn is_enabled(cx: &App) -> bool {
    cx.try_global::<AnalyticsState>()
        .is_some_and(|state| state.0.is_enabled())
}

/// Enable or disable analytics. Disabling clears every queued event.
pub fn set_enabled(cx: &App, enabled: bool) {
    if let Some(state) = cx.try_global::<AnalyticsState>() {
        state.0.set_enabled(enabled);
    }
}

/// Best-effort shutdown: record the close, end the session, flush briefly, and
/// stop the worker. Bounded, so quitting is never held up by the network.
pub fn shutdown(cx: &App) {
    if let Some(state) = cx.try_global::<AnalyticsState>() {
        let client = &state.0;
        client.track(AnalyticsEvent::AppClosed);
        client.end_session();
        client.track(AnalyticsEvent::SessionEnded);
        client.shutdown();
    }
}
