//! The last model catalog and slash-command list pi reported.
//!
//! Orbit starts a session's pi only when the user sends (INTENT D18), so a
//! cold launch has nobody to ask for the model picker or the `/` menu. The
//! raw replies are kept here (`~/.orbit-pi/catalog-cache.json`), shown until a
//! live process reports its own, and refreshed by a short-lived auxiliary pi
//! ([`probe`]) when they are older than [`TTL`] or missing for the workspace.
//!
//! The payloads are stored as pi sent them and replayed through the same
//! parsers a live reply uses, so a cached catalog can never parse differently.
//!
//! A cache is only as good as the file pi read it from, so the models half
//! carries a stamp of that file ([`sources_stamp`]) and an edit makes the cache
//! stale at once instead of at the end of [`TTL`]. A catalog from a *running* pi
//! does not move that stamp forward: pi reads its model config once at process
//! start and `get_available_models` answers from that boot snapshot, so a
//! process older than the edit cannot vouch for it (and never stamps the cache
//! backwards over a newer one either).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use orbit_rpc::{CommandBody, Event, PiClient};
use serde_json::{json, Value};

/// How long a cached catalog counts as current. Models and commands change
/// when the user installs a plugin or refreshes the catalog, not by the hour.
const TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// How long the auxiliary pi waits for each answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CatalogCache {
    pub models: Option<Value>,
    /// pi's own get_state from a process started with no --model: the
    /// default model and thinking level a new task begins on.
    pub state: Option<Value>,
    models_at: u64,
    /// [`sources_stamp`] as of whichever read `models` last: the probe that
    /// read it off disk, or the boot of the process that reported it.
    sources_at: u64,
    commands: BTreeMap<PathBuf, (u64, Value)>,
}

/// `~/.pi/agent/models.json` (honouring `PI_CODING_AGENT_DIR`), pi's
/// user-facing model and provider config — and the file Orbit's own Providers
/// page writes.
///
/// Deliberately the *only* source stamped. `models-store.json` (pi's dynamic
/// provider catalogs) and `settings.json` (pi's preferences) are rewritten by
/// pi itself: the first when it refreshes a dynamic provider, the second by
/// the `set_follow_up_mode` / `set_steering_mode` / `set_auto_compaction` /
/// `set_auto_retry` commands Orbit sends and by a global skill toggle. Stamping
/// those would invalidate the cache on writes that cannot change the model
/// list, so the auxiliary probe would run on every launch. Model sources
/// outside this file — the bundle's built-ins, `pi update self`, installed
/// packages and extensions — stay bounded by [`TTL`], as they were before.
fn models_json() -> PathBuf {
    // Tests must never read the developer's own pi config.
    #[cfg(test)]
    {
        std::env::temp_dir()
            .join(format!("orbit-catalog-sources-test-{}", std::process::id()))
            .join("models.json")
    }
    #[cfg(not(test))]
    crate::pi_install::agent_dir().join("models.json")
}

/// The stamp of pi's model config: its mtime in unix milliseconds, or 0 when
/// there is no such file. An edit moves it, which is what makes a cached
/// catalog stale; milliseconds so an edit inside the second a process booted
/// in still counts. Stamps are identities, not an ordering — deleting the
/// file moves the stamp to 0, and restoring an older copy moves it backwards.
pub(crate) fn sources_stamp() -> u64 {
    std::fs::metadata(models_json())
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|age| age.as_millis() as u64)
        .unwrap_or(0)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl CatalogCache {
    fn from_json(value: &Value) -> Self {
        let models = value.get("models").filter(|m| m.is_object()).cloned();
        let models_at = value.get("modelsAt").and_then(Value::as_u64).unwrap_or(0);
        let commands = value
            .get("commands")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter_map(|(cwd, node)| {
                        let at = node.get("at").and_then(Value::as_u64)?;
                        let data = node.get("data").filter(|d| d.is_object())?.clone();
                        Some((PathBuf::from(cwd), (at, data)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let state = value.get("state").filter(|s| s.is_object()).cloned();
        let sources_at = value.get("sourcesAt").and_then(Value::as_u64).unwrap_or(0);
        Self {
            models,
            state,
            models_at,
            sources_at,
            commands,
        }
    }

    fn to_json(&self) -> Value {
        let commands: serde_json::Map<String, Value> = self
            .commands
            .iter()
            .map(|(cwd, (at, data))| {
                (
                    cwd.to_string_lossy().into_owned(),
                    json!({ "at": at, "data": data }),
                )
            })
            .collect();
        json!({
            "models": self.models,
            "state": self.state,
            "modelsAt": self.models_at,
            "sourcesAt": self.sources_at,
            "commands": commands,
        })
    }

    /// The cached `/` commands for a workspace.
    pub fn commands_for(&self, cwd: &Path) -> Option<&Value> {
        self.commands.get(cwd).map(|(_, data)| data)
    }

    /// [`is_fresh_for`] against a stamp the caller already read.
    ///
    /// The cached `state` (pi's own default model) is *not* required: the
    /// probe asks for it best-effort, so a pi that answers the catalog but not
    /// `get_state` would otherwise make the cache permanently un-fresh and
    /// re-probe on every launch and every picker open.
    fn is_fresh(&self, cwd: &Path, sources: u64) -> bool {
        let recent = |at: u64| now_secs().saturating_sub(at) < TTL.as_secs();
        self.models.is_some()
            && recent(self.models_at)
            // The catalog on disk is not the one that was cached: an edit to
            // pi's model files is news the TTL knows nothing about.
            && self.sources_at == sources
            && self.commands.get(cwd).is_some_and(|(at, _)| recent(*at))
    }

    /// Record a models payload. Returns whether anything changed (an equal
    /// payload only refreshes the timestamp in memory). `sources` is the
    /// stamp the payload was read against: [`sources_stamp`] for a probe,
    /// the boot stamp of the process that answered for a live reply.
    ///
    /// A payload that does not describe the files *as they are now* is not
    /// stored over one that does: a warm process resumed from the pool
    /// reports the catalog it booted with, and filing that over a probe's
    /// current read would replace the newer list with the older one *and*
    /// mark it stale for the next launch. The caller still applies it in
    /// memory — that process is the authority on what its own `set_model`
    /// will accept.
    ///
    /// Equality with the current on-disk stamp (`current`, read once by the
    /// caller) wins over numeric order: deleting `models.json` (stamp 0) or
    /// restoring an older copy moves the stamp *backwards*, and a probe's
    /// fresh read of that state must be able to replace the cache — otherwise
    /// it stays stale forever, re-probing on every picker open.
    fn set_models(&mut self, data: &Value, sources: u64, current: u64) -> bool {
        if sources != current && self.sources_at == current {
            return false;
        }
        let changed = self.models.as_ref() != Some(data) || self.sources_at != sources;
        self.models = Some(data.clone());
        self.models_at = now_secs();
        self.sources_at = sources;
        changed
    }

    fn set_commands(&mut self, cwd: &Path, data: &Value) -> bool {
        let changed = self.commands.get(cwd).map(|(_, old)| old) != Some(data);
        self.commands
            .insert(cwd.to_path_buf(), (now_secs(), data.clone()));
        changed
    }
}

fn persist_path() -> PathBuf {
    // Tests must never touch the real home directory.
    #[cfg(test)]
    {
        return std::env::temp_dir()
            .join(format!("orbit-catalog-test-{}", std::process::id()))
            .join("catalog-cache.json");
    }
    #[cfg(not(test))]
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("catalog-cache.json")
}

static STORE: RwLock<Option<CatalogCache>> = RwLock::new(None);

fn load_from_disk() -> CatalogCache {
    std::fs::read_to_string(persist_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .map(|value| CatalogCache::from_json(&value))
        .unwrap_or_default()
}

fn persist(cache: &CatalogCache) {
    let path = persist_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, cache.to_json().to_string());
}

fn with_store<R>(f: impl FnOnce(&mut CatalogCache) -> R) -> R {
    let mut guard = STORE.write().unwrap();
    f(guard.get_or_insert_with(load_from_disk))
}

/// A snapshot of the cache.
pub(crate) fn load() -> CatalogCache {
    with_store(|cache| cache.clone())
}

/// Whether the cached catalog is current for `cwd`. Checked under the lock so
/// a caller that only needs the answer (opening the model picker) does not
/// clone every cached payload onto the UI thread to ask.
pub(crate) fn is_fresh_for(cwd: &Path) -> bool {
    let sources = sources_stamp();
    with_store(|cache| cache.is_fresh(cwd, sources))
}

/// Keep a `get_available_models` reply. Written to disk only when it differs:
/// a live process re-reports it on every session switch. `sources` is the
/// stamp the answering process booted with — its catalog cannot describe
/// files edited after that, so a newer edit keeps the cache stale. A catalog
/// an auxiliary probe just read off disk passes [`sources_stamp`] instead.
pub(crate) fn store_models_at(data: &Value, sources: u64) {
    let current = sources_stamp();
    with_store(|cache| {
        if cache.set_models(data, sources, current) {
            persist(cache);
        }
    });
}

/// Keep the default model pi reported at startup.
pub(crate) fn store_state(data: &Value) {
    // Only the model and level: the rest of `get_state` (session file, id)
    // changes every launch and has no business on disk.
    let data = &json!({
        "model": data.get("model").cloned().unwrap_or(Value::Null),
        "thinkingLevel": data.get("thinkingLevel").cloned().unwrap_or(Value::Null),
    });
    with_store(|cache| {
        if cache.state.as_ref() != Some(data) {
            cache.state = Some(data.clone());
            persist(cache);
        }
    });
}

/// Test seam: drop the in-memory store. It is process-global, and the
/// no-backwards-stamp rule would make an older test's payload a silent no-op
/// after a newer one, so a test that writes the cache resets it first.
#[cfg(test)]
pub(crate) fn reset_for_test() {
    let mut guard = STORE.write().unwrap();
    *guard = Some(CatalogCache::default());
    // The disk copy too: tests share one PID-scoped file, and a reload of a
    // previous test's persisted stamp would leak into the next one.
    let _ = std::fs::remove_file(persist_path());
}

/// Test seam: the stamp the cached models are filed under.
#[cfg(test)]
pub(crate) fn sources_at_for_test() -> u64 {
    with_store(|cache| cache.sources_at)
}

/// Test seam: seed the cached stamp directly, so a test can describe a
/// process that booted under a stamp the shared test file does not have.
#[cfg(test)]
pub(crate) fn set_sources_at_for_test(sources: u64) {
    with_store(|cache| cache.sources_at = sources);
}

/// Keep a `get_commands` reply for the workspace it was asked in.
pub(crate) fn store_commands(cwd: &Path, data: &Value) {
    with_store(|cache| {
        if cache.set_commands(cwd, data) {
            persist(cache);
        }
    });
}

/// Ask a short-lived pi for the catalog and the workspace's commands, then
/// let it go. Blocking: run it off the main thread.
pub(crate) fn probe(
    extensions: &crate::bundled_extensions::BundledExtensions,
    cwd: &Path,
    env: &[(String, String)],
    test_bin: Option<PathBuf>,
) -> anyhow::Result<(Value, Value, Option<Value>)> {
    let client = match test_bin {
        Some(bin) => PiClient::spawn_with_bin(bin.to_string_lossy().as_ref(), cwd, None)?,
        None => extensions.spawn_with_startup_args(cwd, &[], env)?,
    };
    let ask = |body: CommandBody| -> anyhow::Result<Value> {
        match client.send_blocking(body, PROBE_TIMEOUT)? {
            Event::Response {
                success: true,
                data: Some(data),
                ..
            } => Ok(data),
            Event::Response { error, .. } => Err(anyhow::anyhow!(
                "pi refused the catalog probe: {}",
                error.unwrap_or_default()
            )),
            _ => Err(anyhow::anyhow!("pi answered the catalog probe with a non-response")),
        }
    };
    let models = ask(CommandBody::GetAvailableModels)?;
    let commands = ask(CommandBody::GetCommands)?;
    // Best effort: without it the chips just wait for the first process.
    let state = ask(CommandBody::GetState).ok();
    // Dropping the client kills the child.
    Ok((models, commands, state))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests create and delete the one PID-scoped `models.json`, so
    /// they must not overlap each other (the app-level tests never touch it).
    static FILE_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_cache_round_trips_and_goes_stale() {
        let mut cache = CatalogCache::default();
        let cwd = Path::new("/work/a");
        let sources = sources_stamp();
        assert!(cache.set_models(&json!({ "models": [{ "id": "m" }] }), sources, sources));
        assert!(cache.set_commands(cwd, &json!({ "commands": [] })));
        cache.state = Some(json!({ "model": { "id": "m" } }));
        assert!(cache.is_fresh(cwd, sources));
        assert!(
            !cache.is_fresh(Path::new("/work/other"), sources),
            "commands are per workspace"
        );

        let back = CatalogCache::from_json(&cache.to_json());
        assert_eq!(back, cache);

        let mut old = back.clone();
        old.models_at = now_secs() - TTL.as_secs() - 1;
        assert!(!old.is_fresh(cwd, sources), "an old catalog is probed again");

        // An edit to pi's model files is newer than the cached catalog, so a
        // launch inside the TTL probes again anyway.
        assert!(
            !back.is_fresh(cwd, sources + 1),
            "a moved model source stamps the cache stale"
        );
    }

    #[test]
    fn a_process_older_than_an_edit_keeps_the_cache_stale() {
        let mut cache = CatalogCache::default();
        let cwd = Path::new("/work/a");
        cache.set_models(&json!({ "models": [] }), 10, 10);
        cache.set_commands(cwd, &json!({ "commands": [] }));
        cache.state = Some(json!({ "model": { "id": "m" } }));
        assert!(cache.is_fresh(cwd, 10));
        // A live pi answers from the registry it booted with, so its catalog
        // is stored under its own boot stamp and cannot vouch for the edit.
        cache.set_models(&json!({ "models": [{ "id": "m" }] }), 10, 20);
        assert!(!cache.is_fresh(cwd, 20));
    }

    #[test]
    fn a_cache_without_a_reported_default_is_still_fresh() {
        let mut cache = CatalogCache::default();
        let cwd = Path::new("/work/a");
        let sources = 10;
        cache.set_models(&json!({ "models": [] }), sources, sources);
        cache.set_commands(cwd, &json!({ "commands": [] }));
        assert!(cache.state.is_none());
        assert!(
            cache.is_fresh(cwd, sources),
            "state is best-effort: a pi that does not report it must not re-probe forever"
        );
    }

    #[test]
    fn an_older_payload_never_stamps_the_cache_backwards() {
        let _guard = FILE_TESTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // The file exists, so its mtime — not a synthetic number — is the
        // current stamp the guard compares against.
        let path = models_json();
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, b"{\"providers\":{}}").expect("write");
        let current = sources_stamp();
        assert_ne!(current, 0);

        let mut cache = CatalogCache::default();
        let cwd = Path::new("/work/a");
        cache.set_commands(cwd, &json!({ "commands": [] }));
        let probed = json!({ "models": [{ "id": "new" }] });
        assert!(cache.set_models(&probed, current, current));
        // A warm process booted before the edit reports its older list.
        assert!(!cache.set_models(&json!({ "models": [{ "id": "old" }] }), current - 1, current));
        assert_eq!(cache.models.as_ref(), Some(&probed));
        assert!(cache.is_fresh(cwd, current));

        // Deleting the file moves the stamp to 0 — *backwards*. A probe's
        // fresh read of that state must replace the cache, or it stays stale
        // forever and every picker open re-probes for nothing.
        let _ = std::fs::remove_file(&path);
        assert_eq!(sources_stamp(), 0);
        let without_file = json!({ "models": [] });
        assert!(cache.set_models(&without_file, 0, 0));
        assert!(cache.is_fresh(cwd, 0));
        // And the late reply from the process that booted with the file must
        // not stamp it forwards again.
        assert!(!cache.set_models(&probed, current, 0));
        assert_eq!(cache.models.as_ref(), Some(&without_file));
    }

    #[test]
    fn the_stamp_follows_the_model_file_pi_reads() {
        let _guard = FILE_TESTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = models_json();
        let _ = std::fs::remove_file(&path);
        assert_eq!(sources_stamp(), 0, "no file, no stamp");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, b"{\"providers\":{}}").expect("write");
        assert_ne!(
            sources_stamp(),
            0,
            "the watched file is the one pi builds its registry from"
        );
        let _ = std::fs::remove_file(&path);
        assert_eq!(sources_stamp(), 0);
    }

    #[test]
    fn an_equal_payload_is_not_a_change() {
        let mut cache = CatalogCache::default();
        let data = json!({ "models": [] });
        assert!(cache.set_models(&data, 0, 0));
        assert!(!cache.set_models(&data, 0, 0));
    }
}