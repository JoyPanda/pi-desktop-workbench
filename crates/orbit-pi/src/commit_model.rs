//! The model that writes commit messages from the Git page.
//!
//! Commit messages are a short, cheap task, so they do not have to run on the
//! model the chat session uses: a small local model is enough, and a model with
//! a different context window changes how much of a large change set fits the
//! prompt. The commit bar's model chip picks one; the choice is persisted to
//! `~/.orbit-pi/commit-model.json`:
//!
//! ```jsonc
//! { "model": { "provider": "ollama", "id": "qwen3-coder" } }
//! ```
//!
//! No `model` key means "follow the session model", which is also the default.
//! Parsing is lenient, so a hand-edited or half-written file can only fall back
//! to that default.

use std::path::PathBuf;

use serde_json::{json, Value};

/// A model to ask: the pi provider id and model id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitModel {
    pub provider: String,
    pub id: String,
}

/// Persisted commit-message model preference. `None` follows the session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommitModelConfig {
    pub model: Option<CommitModel>,
}

impl CommitModelConfig {
    /// Read `~/.orbit-pi/commit-model.json`, defaulting on any failure.
    pub fn load() -> Self {
        std::fs::read_to_string(store_path())
            .map(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    /// Parse the persisted shape. Unknown/missing fields fall back to the
    /// default; a half-written model is treated as "follow the session".
    pub fn from_json(raw: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Self::default();
        };
        let model = value.get("model").and_then(|model| {
            let provider = model.get("provider").and_then(Value::as_str)?;
            let id = model.get("id").and_then(Value::as_str)?;
            (!provider.is_empty() && !id.is_empty()).then(|| CommitModel {
                provider: provider.to_string(),
                id: id.to_string(),
            })
        });
        Self { model }
    }

    /// The wire shape written to disk.
    pub fn to_json(&self) -> String {
        let model = self
            .model
            .as_ref()
            .map(|model| json!({ "provider": model.provider, "id": model.id }));
        json!({ "model": model }).to_string()
    }

    /// Persist to `~/.orbit-pi/commit-model.json`. Best-effort: a failed write
    /// only means the next launch returns to the session model.
    pub fn persist(&self) {
        let path = store_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.to_json());
    }

    /// The model to ask for a commit message: the explicit pick, otherwise the
    /// session's own pair when it names one. `None` leaves pi's default alone.
    pub fn resolve(&self, active_provider: &str, active_model: &str) -> Option<CommitModel> {
        self.model.clone().or_else(|| {
            (!active_model.is_empty()).then(|| CommitModel {
                provider: active_provider.to_string(),
                id: active_model.to_string(),
            })
        })
    }
}

/// `~/.orbit-pi/commit-model.json` — Orbit-owned, never pi's global settings.
pub(crate) fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("commit-model.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_session_model() {
        let config = CommitModelConfig::default();
        assert_eq!(config.model, None);
        assert_eq!(
            config.resolve("anthropic", "claude-sonnet-4-5"),
            Some(CommitModel {
                provider: "anthropic".into(),
                id: "claude-sonnet-4-5".into(),
            })
        );
        // With no session model either, pi keeps its own default.
        assert_eq!(config.resolve("anthropic", ""), None);
    }

    #[test]
    fn an_explicit_pick_wins_over_the_session_model() {
        let config = CommitModelConfig {
            model: Some(CommitModel {
                provider: "ollama".into(),
                id: "qwen3-coder".into(),
            }),
        };
        assert_eq!(
            config.resolve("anthropic", "claude-sonnet-4-5"),
            Some(CommitModel {
                provider: "ollama".into(),
                id: "qwen3-coder".into(),
            })
        );
    }

    #[test]
    fn json_round_trips_and_survives_malformed_files() {
        let config = CommitModelConfig {
            model: Some(CommitModel {
                provider: "ollama".into(),
                id: "qwen3-coder".into(),
            }),
        };
        assert_eq!(CommitModelConfig::from_json(&config.to_json()), config);
        assert_eq!(
            CommitModelConfig::from_json(r#"{"model": null}"#),
            CommitModelConfig::default()
        );
        // A half-written model, an empty value, and junk all mean "follow the
        // session model" rather than a broken provider id on the next call.
        for raw in [
            r#"{"model": {"provider": "ollama"}}"#,
            r#"{"model": {"provider": "", "id": "qwen3-coder"}}"#,
            r#"{"model": {"provider": "ollama", "id": ""}}"#,
            "not json",
        ] {
            assert_eq!(
                CommitModelConfig::from_json(raw),
                CommitModelConfig::default()
            );
        }
    }
}
