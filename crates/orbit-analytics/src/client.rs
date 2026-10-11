//! The application-facing client: a bounded queue, a background worker, and a
//! provider-independent [`Analytics`] trait.
//!
//! `track` / `start_session` / `end_session` only touch an in-memory queue and
//! notify the worker; they never perform network I/O, so the GPUI UI thread is
//! never blocked. All failures are non-fatal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::AnalyticsConfig;
use crate::countly::{build_request, CountlyRequest, CountlyTransport, Transport, TransportError};
use crate::event::{AnalyticsEvent, QueuedEvent};
use crate::identity::{
    generate_session_id, load_or_create_installation_id, StateStore, StoredState,
};
use crate::queue::EventQueue;

/// Bounded retry policy: at most this many consecutive failures before a batch
/// is dropped.
const MAX_RETRY_ATTEMPTS: u32 = 5;

/// The provider-independent analytics interface application code depends on.
pub trait Analytics: Send + Sync {
    fn track(&self, event: AnalyticsEvent);
    fn start_session(&self);
    fn end_session(&self);
}

/// An inert implementation used when a caller wants a no-op without a worker.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAnalytics;

impl Analytics for NoopAnalytics {
    fn track(&self, _event: AnalyticsEvent) {}
    fn start_session(&self) {}
    fn end_session(&self) {}
}

/// Deterministic queue/session engine. The worker owns one behind a mutex; tests
/// exercise it directly with an in-memory transport.
struct Engine {
    config: AnalyticsConfig,
    store: Arc<dyn StateStore>,
    installation_id: String,
    session_id: String,
    session_started_at: Option<Instant>,
    queue: EventQueue,
    /// Batch currently being sent (or awaiting retry).
    pending: Vec<QueuedEvent>,
    pending_begin: bool,
    pending_end: Option<u64>,
    enabled: bool,
    retry_attempt: u32,
    retry_after: Option<Instant>,
    last_flush: Instant,
    shutdown_requested: bool,
    stopping: bool,
}

impl Engine {
    fn new(config: AnalyticsConfig, store: Arc<dyn StateStore>) -> Self {
        let stored = store.load();
        let installation_id = load_or_create_installation_id(store.as_ref(), stored.clone());
        // The user's persisted choice wins over the build default.
        let requested = stored.enabled.unwrap_or(config.enabled);
        let enabled = requested && config.is_configured();
        Self {
            queue: EventQueue::new(config.queue_limit),
            config,
            store,
            installation_id,
            session_id: generate_session_id(),
            session_started_at: None,
            pending: Vec::new(),
            pending_begin: false,
            pending_end: None,
            enabled,
            retry_attempt: 0,
            retry_after: None,
            last_flush: Instant::now(),
            shutdown_requested: false,
            stopping: false,
        }
    }

    fn persist(&self) {
        self.store.save(&StoredState {
            installation_id: Some(self.installation_id.clone()),
            enabled: Some(self.enabled),
        });
    }

    fn track(&mut self, event: AnalyticsEvent) {
        if !self.enabled {
            return;
        }
        self.queue.push(QueuedEvent::new(event, now_millis()));
    }

    fn start_session(&mut self) {
        if !self.enabled {
            return;
        }
        self.session_id = generate_session_id();
        self.session_started_at = Some(Instant::now());
        self.pending_begin = true;
    }

    fn end_session(&mut self) {
        if !self.enabled {
            return;
        }
        if let Some(started) = self.session_started_at.take() {
            self.pending_end = Some(started.elapsed().as_secs());
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        let enabled = enabled && self.config.is_configured();
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.persist();
        if !enabled {
            // Disabling immediately stops collection and discards everything
            // queued, so nothing can be sent later.
            self.queue.clear();
            self.pending.clear();
            self.pending_begin = false;
            self.pending_end = None;
            self.session_started_at = None;
            self.retry_attempt = 0;
            self.retry_after = None;
        }
    }

    fn reset_identity(&mut self) {
        self.installation_id = crate::identity::generate_installation_id();
        self.session_id = generate_session_id();
        self.queue.clear();
        self.pending.clear();
        self.pending_begin = false;
        self.pending_end = None;
        self.session_started_at = None;
        self.persist();
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn needs_attention(&self) -> bool {
        self.queue.len() >= self.config.batch_size
    }

    fn has_pending_send(&self) -> bool {
        !self.pending.is_empty() || self.pending_begin || self.pending_end.is_some()
    }

    /// Move queued events into the pending batch when nothing is in flight.
    fn prepare(&mut self) {
        if self.pending.is_empty() {
            self.pending = self.queue.drain_batch(self.config.batch_size);
        }
    }

    fn build_pending_request(&self) -> Option<CountlyRequest> {
        if !self.has_pending_send() {
            return None;
        }
        Some(build_request(
            &self.config,
            &self.installation_id,
            &self.session_id,
            &self.pending,
            self.pending_begin,
            self.pending_end,
            now_millis(),
        ))
    }

    /// Whether the worker should attempt a send now.
    fn should_flush(&self, now: Instant) -> bool {
        if self.retry_after.is_some_and(|after| now < after) {
            return false;
        }
        if self.has_pending_send() {
            return true;
        }
        if self.queue.len() >= self.config.batch_size {
            return true;
        }
        !self.queue.is_empty() && now.duration_since(self.last_flush) >= self.config.flush_interval
    }

    /// How long the worker may sleep before it must reconsider.
    fn next_wait(&self, now: Instant) -> Duration {
        // Backoff takes precedence over a pending batch. After a retryable
        // failure `pending` is deliberately kept, so checking it first would
        // return ZERO and spin the worker for the whole backoff window.
        if let Some(after) = self.retry_after {
            if after > now {
                return (after - now).min(self.config.flush_interval);
            }
        }
        if self.has_pending_send() || self.queue.len() >= self.config.batch_size {
            return Duration::ZERO;
        }
        if self.queue.is_empty() {
            // Nothing to do until notified; still wake occasionally so a
            // session heartbeat is noticed.
            return self.config.flush_interval;
        }
        let elapsed = now.duration_since(self.last_flush);
        self.config.flush_interval.saturating_sub(elapsed)
    }

    fn complete(&mut self, result: Result<(), TransportError>, now: Instant) {
        match result {
            Ok(()) => self.clear_pending(now),
            Err(error) if error.retryable() && self.retry_attempt + 1 < MAX_RETRY_ATTEMPTS => {
                self.retry_attempt += 1;
                let delay = backoff(self.config.retry_base_backoff, self.retry_attempt);
                self.retry_after = Some(now + delay);
                // Log only the failure class and retry number — never payloads.
                eprintln!(
                    "orbit-analytics: request failed ({error}); retry {} in {:?}",
                    self.retry_attempt, delay
                );
            }
            Err(error) => {
                // Permanent failure or exhausted retries: drop the batch so the
                // queue cannot wedge, and keep the app running.
                eprintln!("orbit-analytics: dropping batch after failure ({error})");
                self.clear_pending(now);
            }
        }
    }

    fn clear_pending(&mut self, now: Instant) {
        self.pending.clear();
        self.pending_begin = false;
        self.pending_end = None;
        self.retry_attempt = 0;
        self.retry_after = None;
        self.last_flush = now;
    }
}

fn backoff(base: Duration, attempt: u32) -> Duration {
    let seconds = base.as_secs().max(1).saturating_mul(1u64 << attempt.min(6));
    Duration::from_secs(seconds).min(Duration::from_secs(60))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// The public client. Cheap to clone via `Arc`; all clones share one worker.
pub struct AnalyticsClient {
    engine: Arc<Mutex<Engine>>,
    wake: Arc<Condvar>,
    done: Mutex<Option<mpsc::Receiver<()>>>,
    shutdown_timeout: Duration,
    running: Arc<AtomicBool>,
}

impl AnalyticsClient {
    /// Build the production client: a Countly transport plus the supplied state
    /// store. The worker thread starts immediately.
    pub fn new(config: AnalyticsConfig, store: Arc<dyn StateStore>) -> Self {
        let transport = Arc::new(CountlyTransport::new(&config));
        Self::with_transport(config, store, transport)
    }

    /// Build a client with a custom transport. Used by tests and by anyone
    /// replacing Countly without touching application code.
    pub fn with_transport(
        config: AnalyticsConfig,
        store: Arc<dyn StateStore>,
        transport: Arc<dyn Transport>,
    ) -> Self {
        let engine = Arc::new(Mutex::new(Engine::new(config, store)));
        let wake = Arc::new(Condvar::new());
        let running = Arc::new(AtomicBool::new(true));
        let (done_tx, done_rx) = mpsc::channel();

        let worker_engine = engine.clone();
        let worker_wake = wake.clone();
        let worker_running = running.clone();
        let _ = std::thread::Builder::new()
            .name("orbit-analytics".into())
            .spawn(move || {
                worker_loop(worker_engine, worker_wake, transport, worker_running);
                drop(done_tx);
            });

        let shutdown_timeout = engine
            .lock()
            .map(|engine| engine.config.shutdown_timeout)
            .unwrap_or_else(|_| Duration::from_secs(2));

        Self {
            engine,
            wake,
            done: Mutex::new(Some(done_rx)),
            shutdown_timeout,
            running,
        }
    }

    /// The current installation id. Exposed for diagnostics and tests; the
    /// analytics layer itself never logs it.
    pub fn installation_id(&self) -> String {
        self.engine
            .lock()
            .map(|engine| engine.installation_id.clone())
            .unwrap_or_default()
    }

    pub fn is_enabled(&self) -> bool {
        self.engine
            .lock()
            .map(|engine| engine.is_enabled())
            .unwrap_or(false)
    }

    /// Enable or disable collection. Disabling immediately clears the queue and
    /// stops the worker from sending.
    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_enabled(enabled);
        }
        self.wake.notify_all();
    }

    /// Generate a new installation id and session id, and clear pending events.
    pub fn reset_identity(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.reset_identity();
        }
        self.wake.notify_all();
    }

    /// Best-effort final flush, then stop the worker. Bounded by the configured
    /// shutdown timeout so quitting is never held up by the network.
    pub fn shutdown(&self) {
        if !self.running.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.shutdown_requested = true;
        }
        self.wake.notify_all();
        let receiver = self.done.lock().ok().and_then(|mut done| done.take());
        if let Some(receiver) = receiver {
            let _ = receiver.recv_timeout(self.shutdown_timeout);
        }
    }
}

impl Analytics for AnalyticsClient {
    fn track(&self, event: AnalyticsEvent) {
        let attention = {
            let Ok(mut engine) = self.engine.lock() else {
                return;
            };
            engine.track(event);
            engine.needs_attention()
        };
        if attention {
            self.wake.notify_all();
        }
    }

    fn start_session(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.start_session();
        }
        self.wake.notify_all();
    }

    fn end_session(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.end_session();
        }
        self.wake.notify_all();
    }
}

fn worker_loop(
    engine: Arc<Mutex<Engine>>,
    wake: Arc<Condvar>,
    transport: Arc<dyn Transport>,
    running: Arc<AtomicBool>,
) {
    loop {
        let guard = match engine.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        if guard.shutdown_requested {
            let mut guard = guard;
            // One bounded, best-effort flush of whatever is pending.
            guard.prepare();
            if let Some(request) = guard.build_pending_request() {
                drop(guard);
                let result = transport.send(&request);
                guard = match engine.lock() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                guard.complete(result, Instant::now());
            }
            guard.stopping = true;
            drop(guard);
            wake.notify_all();
            return;
        }

        let now = Instant::now();
        let wait = guard.next_wait(now);
        let (mut guard, _timeout) = match wake.wait_timeout(guard, wait) {
            Ok(result) => result,
            Err(_) => return,
        };

        if guard.shutdown_requested {
            continue;
        }
        if !running.load(Ordering::Acquire) {
            guard.shutdown_requested = true;
            continue;
        }
        if guard.should_flush(Instant::now()) {
            guard.prepare();
            if let Some(request) = guard.build_pending_request() {
                drop(guard);
                let result = transport.send(&request);
                guard = match engine.lock() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                guard.complete(result, Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::InMemoryStore;

    /// Records attempts and answers with a scripted result.
    #[derive(Default)]
    struct Recorder {
        attempts: Mutex<u32>,
        requests: Mutex<Vec<CountlyRequest>>,
        fail_with: Mutex<Option<TransportError>>,
    }

    impl Recorder {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        fn attempts(&self) -> u32 {
            *self.attempts.lock().unwrap()
        }

        fn requests(&self) -> Vec<CountlyRequest> {
            self.requests.lock().unwrap().clone()
        }

        fn fail_next(&self, error: TransportError) {
            *self.fail_with.lock().unwrap() = Some(error);
        }
    }

    impl Transport for Recorder {
        fn send(&self, request: &CountlyRequest) -> Result<(), TransportError> {
            *self.attempts.lock().unwrap() += 1;
            if let Some(error) = self.fail_with.lock().unwrap().take() {
                return Err(error);
            }
            self.requests.lock().unwrap().push(request.clone());
            Ok(())
        }
    }

    fn test_config() -> AnalyticsConfig {
        let mut config = AnalyticsConfig::production("test-key");
        config.metadata = config.metadata.with_app_version("9.9.9");
        config.retry_base_backoff = Duration::from_millis(10);
        config
    }

    fn new_engine(store: Arc<dyn StateStore>) -> Engine {
        Engine::new(test_config(), store)
    }

    #[test]
    fn installation_id_is_stable_across_restarts() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let first = new_engine(store.clone()).installation_id;
        let second = new_engine(store.clone()).installation_id;
        assert_eq!(first, second);
        assert!(crate::identity::is_valid_installation_id(&first));
    }

    #[test]
    fn reset_generates_a_new_installation_id() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let mut engine = new_engine(store.clone());
        let before = engine.installation_id.clone();
        engine.reset_identity();
        assert_ne!(before, engine.installation_id);
        let reloaded = new_engine(store);
        assert_eq!(reloaded.installation_id, engine.installation_id);
    }

    #[test]
    fn session_id_is_fresh_per_launch() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let first = new_engine(store.clone()).session_id;
        let second = new_engine(store).session_id;
        assert_ne!(first, second);
        assert!(crate::identity::is_valid_installation_id(&first));
    }

    #[test]
    fn start_session_changes_the_session_id_and_requests_begin() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let mut engine = new_engine(store);
        let before = engine.session_id.clone();
        engine.start_session();
        assert_ne!(before, engine.session_id);
        assert!(engine.pending_begin);
    }

    #[test]
    fn disabled_engine_queues_nothing() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let mut engine = new_engine(store);
        engine.set_enabled(false);
        engine.track(AnalyticsEvent::AppStarted);
        engine.start_session();
        assert!(engine.queue.is_empty());
        assert!(!engine.pending_begin);
        assert!(engine.build_pending_request().is_none());
    }

    #[test]
    fn disabling_clears_pending_events() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let mut engine = new_engine(store);
        engine.track(AnalyticsEvent::AppStarted);
        engine.start_session();
        assert!(!engine.queue.is_empty());
        engine.set_enabled(false);
        assert!(engine.queue.is_empty());
        assert!(!engine.has_pending_send());
    }

    #[test]
    fn opt_out_persists_across_restarts() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let mut engine = new_engine(store.clone());
        engine.set_enabled(false);
        let reloaded = new_engine(store);
        assert!(!reloaded.is_enabled());
    }

    #[test]
    fn unconfigured_key_leaves_analytics_inert() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let config = AnalyticsConfig::production("");
        let mut engine = Engine::new(config, store);
        engine.track(AnalyticsEvent::AppStarted);
        assert!(!engine.is_enabled());
        assert!(engine.queue.is_empty());
    }

    #[test]
    fn client_batches_and_sends() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let recorder = Recorder::new();
        let mut config = test_config();
        config.batch_size = 2;
        let client = AnalyticsClient::with_transport(config, store, recorder.clone());
        client.track(AnalyticsEvent::AppStarted);
        client.track(AnalyticsEvent::ProjectOpened);
        wait_until(|| !recorder.requests().is_empty());
        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        let events: serde_json::Value =
            serde_json::from_str(requests[0].field("events").unwrap()).unwrap();
        assert_eq!(events.as_array().unwrap().len(), 2);
        assert_eq!(events[0]["key"], "app_started");
        assert_eq!(events[1]["key"], "project_opened");
        client.shutdown();
    }

    #[test]
    fn retryable_failure_is_retried_then_succeeds() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let recorder = Recorder::new();
        recorder.fail_next(TransportError::Http { status: 500 });
        let mut config = test_config();
        config.batch_size = 1;
        let client = AnalyticsClient::with_transport(config, store, recorder.clone());
        client.track(AnalyticsEvent::AppStarted);
        wait_until(|| !recorder.requests().is_empty());
        assert_eq!(recorder.requests().len(), 1);
        assert!(
            recorder.attempts() >= 2,
            "the batch should have been retried"
        );
        client.shutdown();
    }

    #[test]
    fn permanent_failure_is_not_retried() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let recorder = Recorder::new();
        recorder.fail_next(TransportError::Http { status: 401 });
        let mut config = test_config();
        config.batch_size = 1;
        let client = AnalyticsClient::with_transport(config, store, recorder.clone());
        client.track(AnalyticsEvent::AppStarted);
        wait_until(|| recorder.attempts() >= 1);
        // A 401 is permanent; the batch is dropped and not retried.
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(recorder.attempts(), 1);
        assert!(recorder.requests().is_empty());
        client.shutdown();
    }

    #[test]
    fn timeout_is_retried() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let recorder = Recorder::new();
        recorder.fail_next(TransportError::Timeout);
        let mut config = test_config();
        config.batch_size = 1;
        let client = AnalyticsClient::with_transport(config, store, recorder.clone());
        client.track(AnalyticsEvent::AppStarted);
        wait_until(|| recorder.attempts() >= 2);
        client.shutdown();
    }

    #[test]
    fn backoff_sleeps_instead_of_busy_waiting() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let mut engine = new_engine(store);
        engine.track(AnalyticsEvent::AppStarted);
        engine.prepare();
        assert!(engine.has_pending_send());
        let now = Instant::now();
        engine.retry_after = Some(now + Duration::from_secs(3));
        let wait = engine.next_wait(now);
        assert!(
            wait >= Duration::from_secs(1),
            "worker must sleep through the backoff window, got {wait:?}"
        );
    }

    #[test]
    fn disabling_stops_sends() {
        let store: Arc<dyn StateStore> = Arc::new(InMemoryStore::new());
        let recorder = Recorder::new();
        let client = AnalyticsClient::with_transport(test_config(), store, recorder.clone());
        client.track(AnalyticsEvent::AppStarted);
        client.set_enabled(false);
        std::thread::sleep(Duration::from_millis(50));
        assert!(recorder.requests().is_empty());
        client.shutdown();
    }

    fn wait_until(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("condition not met before timeout");
    }
}
