//! The last context and usage numbers pi reported for each session.
//!
//! A session without a process (INTENT D18) has nobody to ask for
//! `get_session_stats`, and a session's context does not move while no run is
//! going, so the numbers from its last live moment are still true. They are
//! kept per session file (`~/.orbit-pi/session-snapshots.json`), shown dimmed
//! while the session is cold, and replaced by the live reply on the next send.
//!
//! The raw fields of the reply are stored and replayed through the same
//! parsers a live reply uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::{json, Value};

/// The part of a `get_session_stats` reply a snapshot needs.
fn subset(data: &Value) -> Value {
    json!({
        "contextUsage": data.get("contextUsage").cloned().unwrap_or(Value::Null),
        "tokens": data.get("tokens").cloned().unwrap_or(Value::Null),
        "cost": data.get("cost").cloned().unwrap_or(Value::Null),
    })
}

#[derive(Default)]
struct Store {
    entries: BTreeMap<PathBuf, Value>,
}

fn persist_path() -> PathBuf {
    // Tests must never touch the real home directory.
    #[cfg(test)]
    {
        return std::env::temp_dir()
            .join(format!("orbit-snapshot-test-{}", std::process::id()))
            .join("session-snapshots.json");
    }
    #[cfg(not(test))]
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("session-snapshots.json")
}

impl Store {
    fn load() -> Self {
        let entries = std::fs::read_to_string(persist_path())
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|value| value.get("sessions").and_then(Value::as_object).cloned())
            .map(|map| {
                map.into_iter()
                    .map(|(path, data)| (PathBuf::from(path), data))
                    .collect()
            })
            .unwrap_or_default();
        Self { entries }
    }

    fn persist(&self) {
        let path = persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let sessions: serde_json::Map<String, Value> = self
            .entries
            .iter()
            .map(|(path, data)| (path.to_string_lossy().into_owned(), data.clone()))
            .collect();
        let _ = std::fs::write(path, json!({ "sessions": sessions }).to_string());
    }
}

static STORE: RwLock<Option<Store>> = RwLock::new(None);

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut guard = STORE.write().unwrap();
    f(guard.get_or_insert_with(Store::load))
}

/// The stored reply fields for a session, shaped like `get_session_stats`.
pub(crate) fn get(path: &Path) -> Option<Value> {
    with_store(|store| store.entries.get(path).cloned())
}

/// Remember a live `get_session_stats` reply. Written only when it changed:
/// pi is asked again after every turn and most replies repeat.
pub(crate) fn put(path: &Path, data: &Value) {
    let data = subset(data);
    with_store(|store| {
        if store.entries.get(path) != Some(&data) {
            store.entries.insert(path.to_path_buf(), data);
            store.persist();
        }
    });
}

/// Forget a session's snapshot (its file was deleted).
pub(crate) fn clear(path: &Path) {
    with_store(|store| {
        if store.entries.remove(path).is_some() {
            store.persist();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_keeps_only_the_stats_fields() {
        let reply = json!({
            "sessionFile": "/x.jsonl",
            "contextUsage": { "contextWindow": 200000, "tokens": 1000 },
            "tokens": { "input": 5 },
            "cost": 0.5,
            "noise": "dropped"
        });
        let kept = subset(&reply);
        assert!(kept.get("noise").is_none() && kept.get("sessionFile").is_none());
        assert_eq!(kept["cost"], json!(0.5));
        assert!(orbit_rpc::ContextUsage::from_stats(&kept).is_some());
        assert!(orbit_rpc::SessionUsage::from_stats(&kept).is_some());
    }
}