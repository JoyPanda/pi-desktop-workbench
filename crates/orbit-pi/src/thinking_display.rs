//! Whether a thinking step streams open (Settings → Behavior).
//!
//! On (the default) the card is open while that step is still reasoning.
//! Off, it stays a one-line header until the reader opens it. Either way a
//! finished step collapses unless the reader opened that card themselves.
//! Persisted to `~/.orbit-pi/thinking-display.json`. A missing or malformed
//! file can only fall back to on.

use std::path::PathBuf;

use serde_json::{json, Value};

/// The persisted "expand thinking while it streams" preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThinkingDisplay {
    pub expand_while_streaming: bool,
}

impl Default for ThinkingDisplay {
    fn default() -> Self {
        Self {
            expand_while_streaming: true,
        }
    }
}

impl ThinkingDisplay {
    /// Read `~/.orbit-pi/thinking-display.json`, defaulting to on.
    pub fn load() -> Self {
        std::fs::read_to_string(store_path())
            .map(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    /// Parse a store file. Anything that is not a boolean `expand_while_streaming`
    /// is on, so a newer or broken file cannot hide reasoning by accident.
    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|value| value.get("expand_while_streaming").and_then(Value::as_bool))
            .map(|expand_while_streaming| Self {
                expand_while_streaming,
            })
            .unwrap_or_default()
    }

    pub fn to_json(self) -> String {
        json!({ "expand_while_streaming": self.expand_while_streaming }).to_string()
    }

    /// Persist to `~/.orbit-pi/thinking-display.json`. Best-effort: a failed
    /// write leaves the previous preference in place.
    pub fn persist(self) {
        let path = store_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.to_json());
    }
}

/// `~/.orbit-pi/thinking-display.json`.
pub(crate) fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("thinking-display.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_expands_while_streaming() {
        assert!(ThinkingDisplay::default().expand_while_streaming);
    }

    #[test]
    fn json_round_trips_both_values() {
        for expand_while_streaming in [true, false] {
            let display = ThinkingDisplay {
                expand_while_streaming,
            };
            assert_eq!(ThinkingDisplay::from_json(&display.to_json()), display);
        }
    }

    #[test]
    fn malformed_or_missing_json_falls_back_to_on() {
        for raw in [
            "not json at all",
            "{}",
            r#"{"expand_while_streaming":"yes"}"#,
            "[]",
            "",
        ] {
            assert!(
                ThinkingDisplay::from_json(raw).expand_while_streaming,
                "{raw}"
            );
        }
    }

    #[test]
    fn store_path_is_orbit_owned() {
        assert!(store_path().ends_with(".orbit-pi/thinking-display.json"));
    }
}
