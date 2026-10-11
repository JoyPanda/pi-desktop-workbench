//! Build-environment and endpoint configuration.

use std::time::Duration;

use url::Url;

/// The production Countly instance. Never hard-code this anywhere else.
pub const PRODUCTION_ENDPOINT: &str = "https://telemetry.rajeshwar.tech";

/// Environment the build targets. Development telemetry must never land in the
/// production Countly application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    /// Analytics is off by default outside production, so a `cargo run` never
    /// pollutes the release dashboard.
    pub fn enabled_by_default(self) -> bool {
        matches!(self, Self::Production)
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" | "debug" => Some(Self::Development),
            "staging" | "stage" => Some(Self::Staging),
            "production" | "prod" | "release" => Some(Self::Production),
            _ => None,
        }
    }
}

/// Metadata attached to every request. These are the only fields collected
/// automatically; none of them identify a person or machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMetadata {
    pub app_version: String,
    /// `macos` / `windows` / `linux`.
    pub os: String,
    /// `aarch64` / `x86_64` / …
    pub architecture: String,
    /// Optional BCP-47 locale supplied by the app's i18n layer.
    pub locale: Option<String>,
    pub environment: Environment,
}

impl AppMetadata {
    /// Detect build/platform facts from the compiled target. The app version
    /// defaults to this crate's own `CARGO_PKG_VERSION`; an application should
    /// override it via [`AppMetadata::with_app_version`] so telemetry reports
    /// the product version, not the analytics crate's version.
    pub fn detect() -> Self {
        let environment = if cfg!(debug_assertions) {
            Environment::Development
        } else {
            Environment::Production
        };
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            locale: None,
            environment,
        }
    }

    pub fn with_app_version(mut self, version: impl Into<String>) -> Self {
        self.app_version = version.into();
        self
    }

    pub fn with_environment(mut self, environment: Environment) -> Self {
        self.environment = environment;
        self
    }

    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        let locale = locale.into();
        if !locale.trim().is_empty() {
            self.locale = Some(locale);
        }
        self
    }

    /// Countly's `_os` metric prefers the display spelling; segmentation keeps
    /// the lower-case Rust constant.
    pub fn os_metric(&self) -> String {
        match self.os.as_str() {
            "macos" => "macOS".to_string(),
            "windows" => "Windows".to_string(),
            "linux" => "Linux".to_string(),
            other => other.to_string(),
        }
    }
}

/// Everything the client needs to talk to a backend.
#[derive(Debug, Clone)]
pub struct AnalyticsConfig {
    pub endpoint: Url,
    /// Countly application key. Not a secret in the credential sense, but kept
    /// as configuration rather than scattered through source.
    pub app_key: String,
    pub environment: Environment,
    /// Default on/off before the user's persisted choice is applied.
    pub enabled: bool,
    /// Flush when this long has passed since the last request.
    pub flush_interval: Duration,
    /// Flush immediately once this many events are queued.
    pub batch_size: usize,
    /// Hard cap on queued events. Oldest are dropped first.
    pub queue_limit: usize,
    /// Per-request network timeout.
    pub request_timeout: Duration,
    /// Base delay for exponential backoff between retries.
    pub retry_base_backoff: Duration,
    /// How long `shutdown` waits for the best-effort final flush.
    pub shutdown_timeout: Duration,
    pub metadata: AppMetadata,
}

impl AnalyticsConfig {
    /// The production configuration, with the endpoint and defaults wired up.
    /// `app_key` comes from the Countly application; an empty key leaves
    /// analytics inert.
    pub fn production(app_key: impl Into<String>) -> Self {
        Self {
            endpoint: Url::parse(PRODUCTION_ENDPOINT).expect("valid production endpoint"),
            app_key: app_key.into(),
            environment: Environment::Production,
            enabled: true,
            flush_interval: Duration::from_secs(20),
            batch_size: 10,
            queue_limit: 200,
            request_timeout: Duration::from_secs(5),
            retry_base_backoff: Duration::from_secs(2),
            shutdown_timeout: Duration::from_secs(2),
            metadata: AppMetadata::detect().with_environment(Environment::Production),
        }
    }

    /// A fully inert configuration for tests and development.
    pub fn disabled() -> Self {
        let mut config = Self::production(String::new());
        config.enabled = false;
        config.metadata = config.metadata.with_environment(Environment::Development);
        config
    }

    /// Build a config for a local endpoint (mock server / development).
    pub fn for_endpoint(endpoint: Url, app_key: impl Into<String>) -> Self {
        let mut config = Self::production(app_key);
        config.endpoint = endpoint;
        config
    }

    /// Whether the client can actually send: a key is present and the endpoint
    /// is HTTPS (or loopback for tests).
    pub fn is_configured(&self) -> bool {
        !self.app_key.trim().is_empty() && endpoint_is_allowed(&self.endpoint)
    }

    /// Reject plaintext endpoints on real hosts, and any endpoint with userinfo
    /// or credentials embedded.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.app_key.trim().is_empty() && !endpoint_is_allowed(&self.endpoint) {
            return Err(ConfigError::InsecureEndpoint);
        }
        if !self.endpoint.username().is_empty() || self.endpoint.password().is_some() {
            return Err(ConfigError::CredentialedEndpoint);
        }
        Ok(())
    }
}

fn endpoint_is_allowed(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        // Loopback HTTP exists only so integration tests can point at a mock
        // server. Remote plaintext is never accepted.
        "http" => matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("[::1]")
        ),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InsecureEndpoint,
    CredentialedEndpoint,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsecureEndpoint => write!(f, "analytics endpoint must use https"),
            Self::CredentialedEndpoint => {
                write!(f, "analytics endpoint must not embed credentials")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Resolve the Countly application key. A non-empty runtime value wins so
/// development can point at a local key; otherwise the value embedded at build
/// time by `build.rs` is used, so a packaged release can send telemetry with no
/// launch environment. An empty result leaves analytics inert.
fn resolve_app_key(runtime: Option<&str>, embedded: Option<&str>) -> String {
    runtime
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| embedded.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or_default()
        .to_string()
}

/// Configuration resolved from the process environment. Real deployments set
/// `ORBIT_ANALYTICS_APP_KEY` (and optionally `ORBIT_ANALYTICS_ENDPOINT`,
/// `ORBIT_ANALYTICS_ENVIRONMENT`, `ORBIT_ANALYTICS_DISABLED`).
///
/// The app key is deliberately read from the environment rather than committed
/// to source control. When the process environment does not define it, the
/// value baked in at build time by the crate's `build.rs` is used instead, so a
/// packaged release can send telemetry without any launch environment. With no
/// key at all, analytics stays inert and Orbit behaves normally.
pub fn config_from_env() -> AnalyticsConfig {
    let environment = std::env::var("ORBIT_ANALYTICS_ENVIRONMENT")
        .ok()
        .and_then(|value| Environment::parse(&value))
        .unwrap_or(if cfg!(debug_assertions) {
            Environment::Development
        } else {
            Environment::Production
        });

    let endpoint = std::env::var("ORBIT_ANALYTICS_ENDPOINT")
        .ok()
        .and_then(|value| Url::parse(value.trim()).ok())
        .unwrap_or_else(|| Url::parse(PRODUCTION_ENDPOINT).expect("valid production endpoint"));

    // Runtime environment wins so development can point at a local key; the
    // build-time value (embedded by build.rs, fed by the release CI secret) is
    // the fallback for packaged builds launched without a shell environment.
    // An empty runtime value falls through to the embedded key rather than
    // disabling telemetry by accident; use ORBIT_ANALYTICS_DISABLED for that.
    let app_key = resolve_app_key(
        std::env::var("ORBIT_ANALYTICS_APP_KEY").ok().as_deref(),
        option_env!("ORBIT_ANALYTICS_APP_KEY"),
    );
    let disabled = std::env::var("ORBIT_ANALYTICS_DISABLED")
        .map(|value| !matches!(value.trim(), "0" | "false" | "no" | ""))
        .unwrap_or(false);

    AnalyticsConfig {
        endpoint,
        app_key,
        environment,
        enabled: !disabled && environment.enabled_by_default(),
        flush_interval: Duration::from_secs(20),
        batch_size: 10,
        queue_limit: 200,
        request_timeout: Duration::from_secs(5),
        retry_base_backoff: Duration::from_secs(2),
        shutdown_timeout: Duration::from_secs(2),
        metadata: AppMetadata::detect().with_environment(environment),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_endpoint_is_https() {
        let config = AnalyticsConfig::production("key");
        assert_eq!(
            config.endpoint.as_str(),
            "https://telemetry.rajeshwar.tech/"
        );
        assert!(config.is_configured());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn empty_key_is_not_configured() {
        let config = AnalyticsConfig::production("");
        assert!(!config.is_configured());
    }

    #[test]
    fn remote_http_is_rejected() {
        let mut config = AnalyticsConfig::production("key");
        config.endpoint = Url::parse("http://telemetry.rajeshwar.tech").unwrap();
        assert!(!config.is_configured());
        assert_eq!(config.validate(), Err(ConfigError::InsecureEndpoint));
    }

    #[test]
    fn loopback_http_is_allowed_for_tests() {
        let config =
            AnalyticsConfig::for_endpoint(Url::parse("http://127.0.0.1:9999").unwrap(), "key");
        assert!(config.is_configured());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn credentialed_endpoint_is_rejected() {
        let mut config = AnalyticsConfig::production("key");
        config.endpoint = Url::parse("https://user:pass@example.com").unwrap();
        assert_eq!(config.validate(), Err(ConfigError::CredentialedEndpoint));
    }

    #[test]
    fn app_key_precedence() {
        // Runtime wins when non-empty; otherwise the build-time value is used.
        assert_eq!(
            resolve_app_key(Some("runtime"), Some("embedded")),
            "runtime"
        );
        assert_eq!(resolve_app_key(Some("  runtime "), None), "runtime");
        assert_eq!(resolve_app_key(Some(""), Some("embedded")), "embedded");
        assert_eq!(resolve_app_key(None, Some("embedded")), "embedded");
        // Whitespace counts as empty on both sides.
        assert_eq!(resolve_app_key(Some("  "), Some("  ")), "");
        assert_eq!(resolve_app_key(None, None), "");
    }

    #[test]
    fn environment_defaults() {
        assert!(Environment::Production.enabled_by_default());
        assert!(!Environment::Development.enabled_by_default());
        assert!(!Environment::Staging.enabled_by_default());
    }
}
