//! How agent call details open in the transcript (issue #80).
//!
//! By default an activity group is open while its turn streams and shuts as
//! soon as the run settles — a group the reader never touched follows the run
//! state, so a finished conversation reads as a wall of collapsed rows that
//! visibly snapped shut. This module owns the persisted preference that decides
//! what "untouched" means: follow the run, always open, always closed.
//!
//! An explicit per-group or per-call toggle always wins over the preference, so
//! the setting only moves the default. Persisted to
//! `~/.orbit-pi/agent-calls.json`, parsed leniently: a malformed file can only
//! fall back to [`AgentCallDisplay::FollowRun`], which is the behavior every
//! build before this one had.

use std::path::PathBuf;

use serde_json::{json, Value};

/// What an agent call (activity group / tool detail card) the reader has not
/// touched should do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentCallDisplay {
    /// Open while the turn streams, closed once it settles. The pre-#80 default.
    #[default]
    FollowRun,
    /// Always open, live or settled.
    Expanded,
    /// Always closed until the reader opens one.
    Collapsed,
}

impl AgentCallDisplay {
    /// The stable on-disk key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FollowRun => "follow_run",
            Self::Expanded => "expanded",
            Self::Collapsed => "collapsed",
        }
    }

    /// Parse a persisted key. Anything unknown is the default, so a file
    /// written by a newer build cannot strand the reader with no calls.
    pub fn parse(value: &str) -> Self {
        match value {
            "expanded" => Self::Expanded,
            "collapsed" => Self::Collapsed,
            _ => Self::FollowRun,
        }
    }

    /// Whether a call the reader has not toggled is open. `follow_run` is what
    /// the caller computes for the current frame (a group is `live`), so the
    /// preference only has to override it.
    pub fn open_by_default(self, follow_run: bool) -> bool {
        match self {
            Self::FollowRun => follow_run,
            Self::Expanded => true,
            Self::Collapsed => false,
        }
    }

    /// The Settings → Agent dropdown label.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::FollowRun => "settings.agent_calls_follow_run",
            Self::Expanded => "settings.agent_calls_expanded",
            Self::Collapsed => "settings.agent_calls_collapsed",
        }
    }

    /// The dropdown's option order: follow the run, expanded, collapsed. An
    /// index outside the list is the default rather than a panic.
    pub const OPTIONS: [Self; 3] = [Self::FollowRun, Self::Expanded, Self::Collapsed];

    /// The option at `ix`, or the default when `ix` is out of range.
    pub fn from_index(ix: usize) -> Self {
        Self::OPTIONS.get(ix).copied().unwrap_or_default()
    }

    /// This mode's index in [`OPTIONS`](Self::OPTIONS).
    pub fn index(self) -> usize {
        Self::OPTIONS
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or(0)
    }

    /// Read `~/.orbit-pi/agent-calls.json`, defaulting on any failure.
    pub fn load() -> Self {
        std::fs::read_to_string(store_path())
            .map(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|value| {
                value
                    .get("display")
                    .and_then(Value::as_str)
                    .map(Self::parse)
            })
            .unwrap_or_default()
    }

    pub fn to_json(self) -> String {
        json!({ "display": self.as_str() }).to_string()
    }

    /// Persist to `~/.orbit-pi/agent-calls.json`. Best-effort: a failed write
    /// leaves the previous preference in place.
    pub fn persist(self) {
        let path = store_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.to_json());
    }
}

/// `~/.orbit-pi/agent-calls.json`.
pub(crate) fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("agent-calls.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_follows_the_run() {
        assert_eq!(AgentCallDisplay::default(), AgentCallDisplay::FollowRun);
        assert!(AgentCallDisplay::FollowRun.open_by_default(true));
        assert!(!AgentCallDisplay::FollowRun.open_by_default(false));
    }

    #[test]
    fn fixed_modes_ignore_the_run_state() {
        assert!(AgentCallDisplay::Expanded.open_by_default(false));
        assert!(!AgentCallDisplay::Collapsed.open_by_default(true));
    }

    #[test]
    fn json_round_trips_every_mode() {
        for mode in [
            AgentCallDisplay::FollowRun,
            AgentCallDisplay::Expanded,
            AgentCallDisplay::Collapsed,
        ] {
            assert_eq!(AgentCallDisplay::from_json(&mode.to_json()), mode);
        }
    }

    #[test]
    fn malformed_or_unknown_json_falls_back_to_follow_run() {
        for raw in ["not json at all", "{}", r#"{"display":"sideways"}"#, "[]"] {
            assert_eq!(
                AgentCallDisplay::from_json(raw),
                AgentCallDisplay::FollowRun,
                "{raw}"
            );
        }
    }

    #[test]
    fn dropdown_index_round_trips() {
        for (ix, mode) in AgentCallDisplay::OPTIONS.into_iter().enumerate() {
            assert_eq!(AgentCallDisplay::from_index(ix), mode);
            assert_eq!(mode.index(), ix);
        }
        assert_eq!(
            AgentCallDisplay::from_index(99),
            AgentCallDisplay::FollowRun
        );
    }

    #[test]
    fn store_path_is_orbit_owned() {
        assert!(store_path().ends_with(".orbit-pi/agent-calls.json"));
    }
}
