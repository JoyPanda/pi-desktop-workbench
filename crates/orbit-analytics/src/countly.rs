//! Countly HTTP API client and request construction.
//!
//! Implements the documented `/i` endpoint: `app_key`, `device_id`, an
//! optional `begin_session` / `end_session` + `session_duration`, a `metrics`
//! JSON object, and an `events` JSON array. The request is built as pure data
//! ([`CountlyRequest`]) so tests can assert its structure without a network,
//! and delivered through the [`Transport`] seam so Countly can be replaced.

use std::time::Duration;

use serde_json::{json, Map, Value};
use url::Url;

use crate::config::AnalyticsConfig;
use crate::event::QueuedEvent;
use crate::privacy;

/// Identifies Orbit's first-party client in Countly's `sdk_name` field. There
/// is no third-party Countly SDK in this path, so `sdk_version` carries the
/// product version rather than this crate's own version.
const SDK_NAME: &str = "orbit-rust";

/// One `/i` request: the URL plus form fields, ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountlyRequest {
    pub url: Url,
    pub fields: Vec<(String, String)>,
}

impl CountlyRequest {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Transport seam. `CountlyTransport` is the production implementation; tests
/// use an in-memory recorder or a local mock server.
pub trait Transport: Send + Sync {
    fn send(&self, request: &CountlyRequest) -> Result<(), TransportError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Timeout,
    Network(String),
    Http { status: u16 },
    Other(String),
}

impl TransportError {
    /// Whether retrying the same batch is worthwhile. Client errors (4xx,
    /// except 408/429) are permanent and must not be retried.
    pub fn retryable(&self) -> bool {
        match self {
            Self::Timeout | Self::Network(_) => true,
            Self::Http { status } => *status == 408 || *status == 429 || *status >= 500,
            Self::Other(_) => false,
        }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timeout"),
            Self::Network(message) => write!(f, "network: {message}"),
            Self::Http { status } => write!(f, "http {status}"),
            Self::Other(message) => write!(f, "error: {message}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// The production transport. HTTPS-only, bounded timeout, one blocking client
/// reused across requests on the analytics worker thread.
pub struct CountlyTransport {
    client: reqwest::blocking::Client,
}

impl CountlyTransport {
    pub fn new(config: &AnalyticsConfig) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(config.request_timeout)
            .connect_timeout(Duration::from_secs(5))
            .user_agent(format!("Orbit/{}", config.metadata.app_version))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self { client }
    }
}

impl Transport for CountlyTransport {
    fn send(&self, request: &CountlyRequest) -> Result<(), TransportError> {
        // Encode form fields explicitly; the analytics payload is never put in
        // the URL, so it cannot leak into proxy logs.
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in &request.fields {
            form.append_pair(key, value);
        }
        let body = form.finish();

        let response = self
            .client
            .post(request.url.clone())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .map_err(|error| classify_reqwest(&error))?;

        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(TransportError::Http { status })
        }
    }
}

fn classify_reqwest(error: &reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else if let Some(status) = error.status() {
        TransportError::Http {
            status: status.as_u16(),
        }
    } else {
        TransportError::Network(deepest_cause(error))
    }
}

fn deepest_cause(error: &dyn std::error::Error) -> String {
    let mut cause = error.to_string();
    let mut current = error.source();
    while let Some(next) = current {
        cause = next.to_string();
        current = next.source();
    }
    cause
}

/// Build the `/i` request for a batch of events plus optional session control.
///
/// `begin_session` starts a Countly session; `end_session` ends it with the
/// elapsed duration in seconds. `session_id` is our per-launch UUID, carried
/// alongside Countly's server-side session for correlation.
pub fn build_request(
    config: &AnalyticsConfig,
    installation_id: &str,
    session_id: &str,
    events: &[QueuedEvent],
    begin_session: bool,
    end_session: Option<u64>,
    now_ms: i64,
) -> CountlyRequest {
    let mut fields: Vec<(String, String)> = vec![
        ("app_key".into(), config.app_key.clone()),
        // Countly's device_id is the pseudonymous installation identity.
        ("device_id".into(), installation_id.to_string()),
        ("session_id".into(), session_id.to_string()),
        ("timestamp".into(), now_ms.to_string()),
        ("sdk_name".into(), SDK_NAME.into()),
        // The product version, not orbit-analytics' own crate version: the app
        // overrides `metadata.app_version` with its own `CARGO_PKG_VERSION`.
        ("sdk_version".into(), config.metadata.app_version.clone()),
    ];

    if begin_session {
        fields.push(("begin_session".into(), "1".into()));
    }
    if let Some(duration_secs) = end_session {
        fields.push(("end_session".into(), "1".into()));
        fields.push(("session_duration".into(), duration_secs.to_string()));
    }

    let mut metrics = Map::new();
    metrics.insert("_os".into(), json!(config.metadata.os_metric()));
    metrics.insert("_app_version".into(), json!(config.metadata.app_version));
    metrics.insert("_device_type".into(), json!("desktop"));
    if let Some(locale) = &config.metadata.locale {
        metrics.insert("_locale".into(), json!(locale));
    }
    fields.push(("metrics".into(), Value::Object(metrics).to_string()));

    let events_json: Vec<Value> = events
        .iter()
        .map(|queued| {
            // Defense in depth: events come from a closed enum, but run the same
            // filter the tests use so a future event that carries a path, email,
            // or secret-shaped value is stripped before it can leave the process.
            // Only the event's own properties are filtered; the metadata
            // segmentation below is trusted and built internally.
            let mut event_properties = queued.event.properties();
            let removed = privacy::sanitize(&mut event_properties);
            if removed > 0 {
                eprintln!(
                    "orbit-analytics: stripped {removed} forbidden property(ies) from {}",
                    queued.event.name()
                );
            }
            let mut segmentation = metadata_segmentation(config);
            for (key, value) in event_properties {
                segmentation.insert(key, value);
            }
            json!({
                "key": queued.event.name(),
                "count": 1,
                "timestamp": queued.timestamp_ms,
                "segmentation": Value::Object(segmentation),
            })
        })
        .collect();
    fields.push(("events".into(), Value::Array(events_json).to_string()));

    CountlyRequest {
        url: endpoint_url(&config.endpoint),
        fields,
    }
}

fn metadata_segmentation(config: &AnalyticsConfig) -> Map<String, Value> {
    let mut segmentation = Map::new();
    segmentation.insert("app_version".into(), json!(config.metadata.app_version));
    segmentation.insert("os".into(), json!(config.metadata.os));
    segmentation.insert("architecture".into(), json!(config.metadata.architecture));
    segmentation.insert(
        "environment".into(),
        json!(config.metadata.environment.as_str()),
    );
    if let Some(locale) = &config.metadata.locale {
        segmentation.insert("locale".into(), json!(locale));
    }
    segmentation
}

/// Append `/i` to the configured base URL.
fn endpoint_url(endpoint: &Url) -> Url {
    let mut url = endpoint.clone();
    // `join` treats the last path segment as a file; a base ending in `/`
    // appends cleanly, and a bare host is the normal case.
    let base = if url.path().ends_with('/') {
        url.as_str().to_string()
    } else {
        format!("{}/", url.as_str())
    };
    Url::parse(&format!("{base}i")).unwrap_or_else(|_| {
        url.set_path("/i");
        url
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AgentFailure, AnalyticsEvent};
    use crate::identity::generate_installation_id;

    fn config() -> AnalyticsConfig {
        AnalyticsConfig::production("test-app-key")
    }

    #[test]
    fn request_endpoint_is_i() {
        let request = build_request(
            &config(),
            "install",
            "session",
            &[],
            true,
            None,
            1_700_000_000_000,
        );
        assert_eq!(request.url.as_str(), "https://telemetry.rajeshwar.tech/i");
    }

    #[test]
    fn request_carries_required_countly_fields() {
        let request = build_request(
            &config(),
            "install-123",
            "session-456",
            &[QueuedEvent::new(
                AnalyticsEvent::AppStarted,
                1_700_000_000_000,
            )],
            true,
            None,
            1_700_000_000_000,
        );
        assert_eq!(request.field("app_key"), Some("test-app-key"));
        assert_eq!(request.field("device_id"), Some("install-123"));
        assert_eq!(request.field("session_id"), Some("session-456"));
        assert_eq!(request.field("begin_session"), Some("1"));
        assert_eq!(request.field("end_session"), None);

        let metrics: Value = serde_json::from_str(request.field("metrics").unwrap()).unwrap();
        assert_eq!(metrics["_app_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(metrics["_device_type"], "desktop");

        let events: Value = serde_json::from_str(request.field("events").unwrap()).unwrap();
        assert_eq!(events[0]["key"], "app_started");
        assert_eq!(events[0]["count"], 1);
        assert_eq!(
            events[0]["segmentation"]["architecture"],
            std::env::consts::ARCH
        );
        assert_eq!(events[0]["segmentation"]["os"], std::env::consts::OS);
        // The trusted metadata segmentation must survive the runtime privacy
        // filter; `environment` is a legitimate dimension even though it also
        // appears in the forbidden-key list.
        assert_eq!(events[0]["segmentation"]["environment"], "production");
        // `sdk_version` tracks the product version in `metadata`, not this
        // crate's own version.
        assert_eq!(
            request.field("sdk_version"),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn end_session_carries_duration() {
        let request = build_request(
            &config(),
            "install",
            "session",
            &[],
            false,
            Some(42),
            1_700_000_000_000,
        );
        assert_eq!(request.field("begin_session"), None);
        assert_eq!(request.field("end_session"), Some("1"));
        assert_eq!(request.field("session_duration"), Some("42"));
    }

    #[test]
    fn agent_failure_reason_is_a_closed_set() {
        let request = build_request(
            &config(),
            "install",
            "session",
            &[QueuedEvent::new(
                AnalyticsEvent::AgentFailed {
                    reason: AgentFailure::Provider,
                },
                0,
            )],
            false,
            None,
            0,
        );
        let events: Value = serde_json::from_str(request.field("events").unwrap()).unwrap();
        assert_eq!(events[0]["segmentation"]["reason"], "provider");
    }

    #[test]
    fn generated_installation_ids_are_uuids() {
        assert!(crate::identity::is_valid_installation_id(
            &generate_installation_id()
        ));
    }
}
