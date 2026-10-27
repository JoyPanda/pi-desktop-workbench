//! Execution state shared by the foreground and parked event pumps. Read/unread
//! state is deliberately separate: reading a result does not undo completion.

use orbit_rpc::{Event, SessionState};
use serde_json::Value;

use crate::dialog::{ApprovalRequest, DialogRequest};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SessionStatus {
    #[default]
    Idle,
    Running,
    WaitingApproval,
    WaitingInput,
    Retrying,
    Compacting,
    Completed,
    Failed,
    Stopped,
    Disconnected,
}

impl SessionStatus {
    pub(crate) fn is_in_progress(self) -> bool {
        matches!(
            self,
            Self::Running
                | Self::WaitingApproval
                | Self::WaitingInput
                | Self::Retrying
                | Self::Compacting
        )
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Idle => tr!("sidebar.status_idle"),
            Self::Running => tr!("sidebar.status_running"),
            Self::WaitingApproval => tr!("sidebar.status_approval"),
            Self::WaitingInput => tr!("sidebar.status_input"),
            Self::Retrying => tr!("sidebar.status_retrying"),
            Self::Compacting => tr!("sidebar.status_compacting"),
            Self::Completed => tr!("sidebar.status_completed"),
            Self::Failed => tr!("sidebar.status_failed"),
            Self::Stopped => tr!("sidebar.status_stopped"),
            Self::Disconnected => tr!("sidebar.status_disconnected"),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PendingSessionRequest {
    pub id: String,
    pub method: String,
    pub value: Value,
    approval: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SessionExecution {
    running: bool,
    pub retrying: bool,
    pub compacting: bool,
    outcome: SessionStatus,
    pub pending_request: Option<PendingSessionRequest>,
    pub prompt_id: Option<String>,
}

impl SessionExecution {
    pub fn from_status(outcome: SessionStatus) -> Self {
        Self {
            outcome,
            ..Self::default()
        }
    }

    pub fn status(&self) -> SessionStatus {
        if let Some(request) = &self.pending_request {
            if request.approval {
                SessionStatus::WaitingApproval
            } else {
                SessionStatus::WaitingInput
            }
        } else if self.retrying {
            SessionStatus::Retrying
        } else if self.compacting {
            SessionStatus::Compacting
        } else if self.running {
            SessionStatus::Running
        } else {
            self.outcome
        }
    }

    pub fn start(&mut self) {
        if !self.running {
            self.running = true;
            self.outcome = SessionStatus::Idle;
        }
    }

    pub fn respond(&mut self, id: &str) {
        if self
            .pending_request
            .as_ref()
            .is_some_and(|request| request.id == id)
        {
            self.pending_request = None;
        }
    }

    pub fn fail(&mut self) {
        *self = Self::from_status(SessionStatus::Failed);
    }

    pub fn stop(&mut self) {
        *self = Self::from_status(SessionStatus::Stopped);
    }

    pub fn reconcile(&mut self, state: &SessionState) {
        if state.is_streaming {
            self.start();
        }
        self.compacting = state.is_compacting;
        // isStreaming can be false between low-level runs or while a dialog
        // blocks a tool. Only agent_settled can complete known active work.
    }

    pub fn apply_event(&mut self, event: &Event) {
        match event {
            Event::AgentStart => self.start(),
            Event::Response {
                id,
                command,
                success,
                data,
                ..
            } if command == "prompt" && self.prompt_id.as_deref() == Some(id) => {
                self.prompt_id = None;
                if !success {
                    self.fail();
                } else if data
                    .as_ref()
                    .and_then(|value| value.get("disposition"))
                    .and_then(Value::as_str)
                    == Some("handled")
                {
                    *self = Self::default();
                }
            }
            Event::MessageEnd { value } => {
                let message = value.get("message").unwrap_or(value);
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    self.outcome = match message.get("stopReason").and_then(Value::as_str) {
                        Some("error") => SessionStatus::Failed,
                        Some("aborted") => SessionStatus::Stopped,
                        Some("stop" | "length" | "toolUse") => SessionStatus::Completed,
                        _ => self.outcome,
                    };
                }
            }
            Event::AgentSettled { aborted } => {
                let had_work = self.status().is_in_progress();
                let outcome = if *aborted || self.outcome == SessionStatus::Stopped {
                    SessionStatus::Stopped
                } else if self.outcome == SessionStatus::Failed {
                    SessionStatus::Failed
                } else if had_work {
                    SessionStatus::Completed
                } else {
                    self.outcome
                };
                *self = Self::from_status(outcome);
            }
            Event::AutoRetryStart { .. } => self.retrying = true,
            Event::AutoRetryEnd { value } => {
                self.retrying = false;
                if value.get("success").and_then(Value::as_bool) == Some(false) {
                    self.outcome = SessionStatus::Failed;
                }
            }
            Event::CompactionStart { .. } => self.compacting = true,
            Event::CompactionEnd { value } => {
                self.compacting = false;
                if value.get("errorMessage").and_then(Value::as_str).is_some() {
                    self.outcome = SessionStatus::Failed;
                }
            }
            Event::ExtensionUiRequest { id, method, value } => {
                if self.pending_request.is_none()
                    && DialogRequest::from_event(id.clone(), method, value).is_some()
                {
                    self.pending_request = Some(PendingSessionRequest {
                        id: id.clone(),
                        method: method.clone(),
                        value: value.clone(),
                        approval: ApprovalRequest::from_event(id.clone(), method, value).is_some(),
                    });
                }
            }
            // A timed-out question has resumed on pi's side. Its next
            // assistant turn retires the request even without a UI reply.
            Event::TurnStart => self.pending_request = None,
            Event::ProcessExited => {
                let status = self.status();
                *self = Self::from_status(
                    if status.is_in_progress() || status == SessionStatus::Idle {
                        SessionStatus::Disconnected
                    } else {
                        status
                    },
                );
            }
            _ => {}
        }
    }
}

impl super::OrbitApp {
    pub(super) fn remember_session_outcome(
        &mut self,
        path: std::path::PathBuf,
        status: SessionStatus,
    ) {
        if !status.is_in_progress() {
            self.session_outcomes.insert(path, status);
        }
    }

    pub(super) fn sidebar_session_statuses(
        &self,
    ) -> std::collections::HashMap<std::path::PathBuf, SessionStatus> {
        let mut statuses = self.session_outcomes.clone();
        for (path, parked) in &self.lives {
            statuses.insert(path.clone(), parked.execution.status());
        }
        if let Some(path) = &self.current_session_path {
            statuses.insert(path.clone(), self.execution.status());
        }
        statuses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_end_does_not_complete_queued_or_retrying_work() {
        let mut state = SessionExecution::default();
        state.apply_event(&Event::AgentStart);
        state.apply_event(&Event::AgentEnd { will_retry: false });
        assert_eq!(state.status(), SessionStatus::Running);
        state.apply_event(&Event::AgentSettled { aborted: false });
        assert_eq!(state.status(), SessionStatus::Completed);
        state.reconcile(&SessionState::default());
        assert_eq!(state.status(), SessionStatus::Completed);
    }

    #[test]
    fn settled_failure_and_abort_are_distinct_from_completion() {
        let mut state = SessionExecution::default();
        state.start();
        state.apply_event(&Event::MessageEnd {
            value: json!({"message": {
                "role": "assistant", "stopReason": "error", "errorMessage": "unavailable"
            }}),
        });
        state.apply_event(&Event::AgentSettled { aborted: false });
        assert_eq!(state.status(), SessionStatus::Failed);
        state.start();
        state.apply_event(&Event::AgentSettled { aborted: true });
        assert_eq!(state.status(), SessionStatus::Stopped);
    }

    #[test]
    fn successful_retry_replaces_the_failed_attempt() {
        let mut state = SessionExecution::default();
        state.start();
        state.apply_event(&Event::MessageEnd {
            value: json!({"message": {
                "role": "assistant", "stopReason": "error"
            }}),
        });
        state.apply_event(&Event::AutoRetryStart { value: json!({}) });
        assert_eq!(state.status(), SessionStatus::Retrying);
        state.apply_event(&Event::AgentStart);
        assert_eq!(state.status(), SessionStatus::Retrying);
        state.apply_event(&Event::MessageEnd {
            value: json!({"message": {
                "role": "assistant", "stopReason": "stop"
            }}),
        });
        state.apply_event(&Event::AutoRetryEnd {
            value: json!({"success": true}),
        });
        state.apply_event(&Event::AgentSettled { aborted: false });
        assert_eq!(state.status(), SessionStatus::Completed);
    }

    #[test]
    fn compaction_temporarily_overrides_running_and_completed() {
        for initial in [SessionStatus::Running, SessionStatus::Completed] {
            let mut state = SessionExecution::from_status(initial);
            if initial == SessionStatus::Running {
                state.start();
            }
            state.apply_event(&Event::CompactionStart { value: json!({}) });
            assert_eq!(state.status(), SessionStatus::Compacting);
            state.apply_event(&Event::CompactionEnd { value: json!({}) });
            assert_eq!(state.status(), initial);
        }
    }

    #[test]
    fn only_a_matching_response_clears_a_waiting_request() {
        let mut state = SessionExecution::default();
        state.start();
        state.apply_event(&Event::ExtensionUiRequest {
            id: "approval".into(),
            method: "select".into(),
            value: json!({"title": "[orbit-guard] bash\trm file", "options": ["Allow", "Deny"]}),
        });
        assert_eq!(state.status(), SessionStatus::WaitingApproval);
        state.reconcile(&SessionState::default());
        state.respond("other");
        assert_eq!(state.status(), SessionStatus::WaitingApproval);
        state.respond("approval");
        assert_eq!(state.status(), SessionStatus::Running);
    }

    #[test]
    fn input_waits_but_status_widgets_do_not() {
        let mut state = SessionExecution::default();
        state.start();
        state.apply_event(&Event::ExtensionUiRequest {
            id: "widget".into(),
            method: "setStatus".into(),
            value: json!({}),
        });
        assert_eq!(state.status(), SessionStatus::Running);
        state.apply_event(&Event::ExtensionUiRequest {
            id: "question".into(),
            method: "input".into(),
            value: json!({"title": "Which file?"}),
        });
        assert_eq!(state.status(), SessionStatus::WaitingInput);
        state.apply_event(&Event::TurnStart);
        assert_eq!(state.status(), SessionStatus::Running);
    }

    #[test]
    fn process_exit_cannot_turn_active_work_into_completed() {
        let mut state = SessionExecution::default();
        state.start();
        state.apply_event(&Event::ProcessExited);
        assert_eq!(state.status(), SessionStatus::Disconnected);
        state.reconcile(&SessionState::default());
        assert_eq!(state.status(), SessionStatus::Disconnected);
    }

    #[test]
    fn an_idle_snapshot_and_child_exit_do_not_erase_a_result() {
        for outcome in [
            SessionStatus::Completed,
            SessionStatus::Failed,
            SessionStatus::Stopped,
        ] {
            let mut state = SessionExecution::from_status(outcome);
            state.reconcile(&SessionState::default());
            state.apply_event(&Event::ProcessExited);
            assert_eq!(state.status(), outcome);
        }
    }

    #[test]
    fn an_extension_command_cannot_leave_an_optimistic_run_spinning() {
        let mut state = SessionExecution::default();
        state.start();
        state.prompt_id = Some("send-1".into());
        state.apply_event(&Event::Response {
            id: "other-command".into(),
            command: "prompt".into(),
            success: true,
            data: Some(json!({"disposition": "handled"})),
            error: None,
        });
        assert_eq!(state.status(), SessionStatus::Running);
        state.apply_event(&Event::Response {
            id: "send-1".into(),
            command: "prompt".into(),
            success: true,
            data: Some(json!({"disposition": "handled"})),
            error: None,
        });
        assert_eq!(state.status(), SessionStatus::Idle);
    }

    #[test]
    fn rejected_prompts_and_exhausted_retries_are_failed_not_completed() {
        let mut state = SessionExecution::default();
        state.start();
        state.prompt_id = Some("send-1".into());
        state.apply_event(&Event::Response {
            id: "send-1".into(),
            command: "prompt".into(),
            success: false,
            data: None,
            error: Some("invalid model".into()),
        });
        assert_eq!(state.status(), SessionStatus::Failed);
        state.start();
        state.apply_event(&Event::AutoRetryStart { value: json!({}) });
        state.apply_event(&Event::AutoRetryEnd {
            value: json!({"success": false}),
        });
        state.apply_event(&Event::AgentSettled { aborted: false });
        assert_eq!(state.status(), SessionStatus::Failed);
    }

    #[test]
    fn tool_failure_is_not_session_failure() {
        let mut state = SessionExecution::default();
        state.start();
        state.apply_event(&Event::ToolExecutionEnd {
            value: json!({"isError": true}),
        });
        assert_eq!(state.status(), SessionStatus::Running);
        state.apply_event(&Event::AgentSettled { aborted: false });
        assert_eq!(state.status(), SessionStatus::Completed);
    }
}
