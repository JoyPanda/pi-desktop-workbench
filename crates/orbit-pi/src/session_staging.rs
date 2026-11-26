//! Choices made on a session that has no pi process (a "cold" session).
//!
//! Orbit starts pi only when the user sends (INTENT D18), so a session opened
//! from the sidebar has nothing to send `set_model` or `set_session_name` to.
//! The choice is not lost and not refused: it is staged here, shown at once,
//! and applied to the process that the first send starts.
//!
//! Keyed by the session file path, like pins and archives, and persisted to
//! `~/.orbit-pi/session-staging.json` so a quit does not drop it. Orbit-owned:
//! nothing here is written into pi's session files until it is applied.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::{json, Value};

use crate::session_defaults::SessionDefault;

/// What the user chose for one cold session and pi has not heard yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionStaging {
    /// Model and thinking level. `None` keeps whatever the session file says.
    pub model: Option<SessionDefault>,
    /// A new session name.
    pub rename: Option<String>,
}

impl SessionStaging {
    fn is_empty(&self) -> bool {
        self.model.as_ref().is_none_or(SessionDefault::is_unset) && self.rename.is_none()
    }

    fn to_json(&self) -> Value {
        let mut out = serde_json::Map::new();
        if let Some(model) = self.model.as_ref().filter(|model| !model.is_unset()) {
            out.insert(
                "model".into(),
                json!({
                    "provider": model.provider,
                    "modelId": model.model_id,
                    "thinking": model.thinking,
                }),
            );
        }
        if let Some(name) = &self.rename {
            out.insert("rename".into(), json!(name));
        }
        Value::Object(out)
    }

    fn from_json(value: &Value) -> Self {
        let text = |node: &Value, key: &str| {
            node.get(key)
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        let model = value
            .get("model")
            .map(|node| SessionDefault {
                provider: text(node, "provider"),
                model_id: text(node, "modelId"),
                thinking: text(node, "thinking"),
            })
            .filter(|model| !model.is_unset());
        Self {
            model,
            rename: text(value, "rename"),
        }
    }
}

#[derive(Debug, Default)]
struct Store {
    entries: BTreeMap<PathBuf, SessionStaging>,
}

impl Store {
    fn load_from(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let entries = value
            .get("sessions")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .map(|(key, node)| (PathBuf::from(key), SessionStaging::from_json(node)))
                    .filter(|(_, staging)| !staging.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self { entries }
    }

    fn persist_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let sessions: serde_json::Map<String, Value> = self
            .entries
            .iter()
            .map(|(key, staging)| (key.to_string_lossy().into_owned(), staging.to_json()))
            .collect();
        let _ = std::fs::write(path, json!({ "sessions": sessions }).to_string());
    }

    fn edit(&mut self, path: &Path, f: impl FnOnce(&mut SessionStaging)) {
        let entry = self.entries.entry(path.to_path_buf()).or_default();
        f(entry);
        if entry.is_empty() {
            self.entries.remove(path);
        }
    }
}

fn persist_path() -> PathBuf {
    // Tests must never touch the real home directory.
    #[cfg(test)]
    {
        return std::env::temp_dir()
            .join(format!("orbit-staging-test-{}", std::process::id()))
            .join("session-staging.json");
    }
    #[cfg(not(test))]
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("session-staging.json")
}

static STORE: RwLock<Option<Store>> = RwLock::new(None);

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut guard = STORE.write().unwrap();
    f(guard.get_or_insert_with(|| Store::load_from(&persist_path())))
}

/// What is staged for a session, if anything.
pub(crate) fn get(path: &Path) -> Option<SessionStaging> {
    with_store(|store| store.entries.get(path).cloned())
}

/// The staged name, read by the session list on every reload.
pub(crate) fn rename_for(path: &Path) -> Option<String> {
    with_store(|store| store.entries.get(path).and_then(|s| s.rename.clone()))
}

/// Stage a model (and thinking level) for a session.
pub(crate) fn set_model(path: &Path, model: SessionDefault) {
    with_store(|store| {
        store.edit(path, |staging| staging.model = Some(model));
        store.persist_to(&persist_path());
    });
}

/// Stage a new name for a session.
pub(crate) fn set_rename(path: &Path, name: String) {
    with_store(|store| {
        store.edit(path, |staging| staging.rename = Some(name));
        store.persist_to(&persist_path());
    });
}

/// Drop everything staged for a session: it was applied, or it was deleted.
pub(crate) fn clear(path: &Path) {
    with_store(|store| {
        if store.entries.remove(path).is_some() {
            store.persist_to(&persist_path());
        }
    });
}

/// The model part was applied to pi.
pub(crate) fn clear_model(path: &Path) {
    with_store(|store| {
        if store.entries.contains_key(path) {
            store.edit(path, |staging| staging.model = None);
            store.persist_to(&persist_path());
        }
    });
}

/// The rename was applied to pi.
pub(crate) fn clear_rename(path: &Path) {
    with_store(|store| {
        if store.entries.contains_key(path) {
            store.edit(path, |staging| staging.rename = None);
            store.persist_to(&persist_path());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(id: &str, thinking: Option<&str>) -> SessionDefault {
        SessionDefault {
            provider: Some("anthropic".into()),
            model_id: Some(id.into()),
            thinking: thinking.map(str::to_string),
        }
    }

    #[test]
    fn staging_round_trips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("orbit-staging-{}", std::process::id()));
        let file = dir.join("session-staging.json");
        let mut store = Store::default();
        let path = Path::new("/store/a.jsonl");
        store.edit(path, |s| s.model = Some(choice("sonnet", Some("high"))));
        store.edit(path, |s| s.rename = Some("renamed".into()));
        store.persist_to(&file);

        let loaded = Store::load_from(&file);
        let staged = loaded.entries.get(path).expect("staged entry survives");
        assert_eq!(staged.model, Some(choice("sonnet", Some("high"))));
        assert_eq!(staged.rename.as_deref(), Some("renamed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_emptied_entry_is_removed() {
        let mut store = Store::default();
        let path = Path::new("/store/a.jsonl");
        store.edit(path, |s| s.rename = Some("x".into()));
        store.edit(path, |s| s.rename = None);
        assert!(store.entries.is_empty());
    }

    #[test]
    fn a_missing_or_garbled_file_loads_empty() {
        assert!(Store::load_from(Path::new("/no/such/file.json"))
            .entries
            .is_empty());
    }
}
