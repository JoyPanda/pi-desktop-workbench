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
    commands: BTreeMap<PathBuf, (u64, Value)>,
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
        Self {
            models,
            state,
            models_at,
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
            "commands": commands,
        })
    }

    /// The cached `/` commands for a workspace.
    pub fn commands_for(&self, cwd: &Path) -> Option<&Value> {
        self.commands.get(cwd).map(|(_, data)| data)
    }

    /// Whether both halves are cached for `cwd` and recent enough to skip a
    /// probe.
    pub fn is_fresh_for(&self, cwd: &Path) -> bool {
        let recent = |at: u64| now_secs().saturating_sub(at) < TTL.as_secs();
        self.models.is_some()
            && self.state.is_some()
            && recent(self.models_at)
            && self.commands.get(cwd).is_some_and(|(at, _)| recent(*at))
    }

    /// Record a models payload. Returns whether anything changed (an equal
    /// payload only refreshes the timestamp in memory).
    fn set_models(&mut self, data: &Value) -> bool {
        let changed = self.models.as_ref() != Some(data);
        self.models = Some(data.clone());
        self.models_at = now_secs();
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

/// Keep a `get_available_models` reply. Written to disk only when it differs:
/// a live process re-reports it on every session switch.
pub(crate) fn store_models(data: &Value) {
    with_store(|cache| {
        if cache.set_models(data) {
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

    #[test]
    fn a_cache_round_trips_and_goes_stale() {
        let mut cache = CatalogCache::default();
        let cwd = Path::new("/work/a");
        assert!(cache.set_models(&json!({ "models": [{ "id": "m" }] })));
        assert!(cache.set_commands(cwd, &json!({ "commands": [] })));
        cache.state = Some(json!({ "model": { "id": "m" } }));
        assert!(cache.is_fresh_for(cwd));
        assert!(!cache.is_fresh_for(Path::new("/work/other")), "commands are per workspace");

        let back = CatalogCache::from_json(&cache.to_json());
        assert_eq!(back, cache);

        let mut old = back;
        old.models_at = now_secs() - TTL.as_secs() - 1;
        assert!(!old.is_fresh_for(cwd), "an old catalog is probed again");
    }

    #[test]
    fn an_equal_payload_is_not_a_change() {
        let mut cache = CatalogCache::default();
        let data = json!({ "models": [] });
        assert!(cache.set_models(&data));
        assert!(!cache.set_models(&data));
    }
}