//! Build-time analytics configuration.
//!
//! The Countly application key is embedded into the binary as
//! `ORBIT_ANALYTICS_APP_KEY` so a packaged release (a macOS `.app` launched
//! from Finder inherits no shell environment) can still send telemetry. The
//! value is not a credential in the signing sense — it travels in every request
//! — but keeping it out of source control avoids scattering it through the tree.
//!
//! Precedence at runtime is: a process-environment `ORBIT_ANALYTICS_APP_KEY`
//! wins, so local development can override the baked value; the embedded value
//! is the fallback. An empty key leaves analytics inert, exactly as before.

fn main() {
    println!("cargo:rerun-if-env-changed=ORBIT_ANALYTICS_APP_KEY");
    // Always emit the variable (possibly empty) so `option_env!` at the call
    // site is deterministic rather than depending on whether the build
    // environment happened to define it.
    let key = std::env::var("ORBIT_ANALYTICS_APP_KEY").unwrap_or_default();
    println!("cargo:rustc-env=ORBIT_ANALYTICS_APP_KEY={}", key.trim());
}
