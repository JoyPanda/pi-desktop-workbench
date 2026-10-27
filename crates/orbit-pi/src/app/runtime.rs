use super::helpers::*;
use super::*;
use gpui::Timer;

impl OrbitApp {
    /// Record a status message; the status bar surfaces it briefly (see
    /// [`STATUS_MESSAGE_TTL`]). Internal RPC chatter never calls this —
    /// only user-meaningful facts and failures.
    pub(super) fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_at = Some(Instant::now());
    }

    /// Record a command / protocol / extension error. Unlike the transient
    /// status line, errors persist (in a red banner) until dismissed or
    /// superseded, so a failure is never silently lost.
    pub(super) fn set_error(&mut self, message: impl Into<String>) {
        self.raise_error(message, None);
    }

    /// Record an error raised by a named source — a wire command, or a
    /// non-command event such as a failed assistant message. A later success
    /// on the same source retires the banner by comparing it, never by
    /// matching the banner's translated text.
    pub(super) fn set_error_from(&mut self, source: &str, message: impl Into<String>) {
        self.raise_error(message, Some(source.to_string()));
    }

    fn raise_error(&mut self, message: impl Into<String>, source: Option<String>) {
        let message = message.into();
        // The banner renders on the chat and settings surfaces. The Git and
        // Usage pages own the main area, so an error raised while one of them
        // is open also surfaces as a toast — the failure is never off-screen.
        if self.git_open || self.usage_open {
            self.toast_error(message.clone());
        }
        self.error = Some(message);
        self.error_source = source;
    }

    /// Dismiss the current error banner.
    pub(super) fn dismiss_error(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.error_source = None;
        cx.notify();
    }

    /// A successful command clears the banner it previously raised (e.g. a
    /// retried `set_model`), so a fixed error doesn't linger. A banner raised
    /// by something else — another command, or an event that names no command —
    /// is left alone.
    pub(super) fn clear_error_from(&mut self, source: &str) {
        if self.error_source.as_deref() == Some(source) {
            self.error = None;
            self.error_source = None;
        }
    }

    /// Surface a failed command's `error` — the docs' `success: false`
    /// contract. A `parse` command is pi's response to unparseable input.
    pub(super) fn on_command_failure(
        &mut self,
        command: &str,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if command == "parse" {
            self.set_error_from(
                "parse",
                tr!(
                    "runtime.protocol_error",
                    error = error.unwrap_or(&tr!("runtime.parse_failed"))
                ),
            );
            return;
        }
        // A rejected follow-up/steer never entered pi's queue: drop the
        // optimistic chip and hand the text back so nothing is lost.
        if matches!(command, "follow_up" | "steer") {
            if let Some(text) = self.pending_follow_up.take() {
                if let Some(pos) = self.queue.follow_up.iter().position(|t| *t == text) {
                    self.queue.follow_up.remove(pos);
                }
                if self.input.read(cx).text().trim().is_empty() {
                    self.input.update(cx, |input, cx| input.set_text(text, cx));
                }
            }
        }
        // A rejected prompt never opened a turn. The run state was set
        // optimistically when the prompt went out, so unwind it here instead of
        // leaving the composer on Stop and the transcript waiting for an answer
        // that is not coming (see `submit_as`).
        if command == "prompt" {
            self.execution.fail();
            self.busy = false;
            self.transcript.abandon_reply();
            // pi is the authority on whether something else is still running;
            // `get_state` re-syncs `busy` from it.
            self.send(CommandBody::GetState, "get_state");
        }
        let fallback = tr!("runtime.unspecified_error");
        let detail = error.unwrap_or(&fallback);
        self.set_error_from(
            command,
            tr!(
                "helpers.command_failed_detail",
                label = humanize_command(command),
                detail = detail
            ),
        );
    }

    /// Send a command to pi. Returns `false` when the write failed (or
    /// there is no process) so callers can keep the user's input intact.
    pub(super) fn send(&mut self, body: CommandBody, label: &str) -> bool {
        self.send_tracked(body, label).is_some()
    }

    /// [`Self::send`], returning pi's id for the command it queued. A caller
    /// that must recognise that command's own reply (a `prompt` and its
    /// `disposition`) keeps the id; everyone else drops it.
    pub(super) fn send_tracked(&mut self, body: CommandBody, label: &str) -> Option<String> {
        let Some(client) = self.client.as_ref() else {
            self.toast_warning(tr!("runtime.pi_not_running"));
            return None;
        };
        match client.send(body) {
            Ok((id, _)) => Some(id),
            Err(err) => {
                // Tagged with `label`: a later success on the same command
                // clears this banner, so a transient write failure cannot
                // strand a red banner the user has no way to dismiss.
                self.set_error_from(
                    label,
                    tr!("runtime.send_failed", label = label, error = err),
                );
                None
            }
        }
    }

    /// Re-fetch the model catalog and thinking levels for the current session.
    pub(super) fn refresh_catalogs(&mut self) {
        self.send(CommandBody::GetAvailableModels, "get_available_models");
        self.send(
            CommandBody::GetAvailableThinkingLevels,
            "get_available_thinking_levels",
        );
        self.send(CommandBody::GetCommands, "get_commands");
    }

    /// Re-fetch the current context-window estimate. Cheap; call after
    /// settle, session switch, compaction, and model changes — never per tick.
    pub(super) fn refresh_context_stats(&mut self) {
        self.send(CommandBody::GetSessionStats, "get_session_stats");
    }

    /// Make `client` the active pi process, resetting uptime / exit state.
    ///
    /// Callers send their primary command (`switch_session` / `new_session` /
    /// `get_state`) *before* [`probe_auth`](Self::probe_auth), so the slow
    /// provider-auth/quota probes never queue in front of the session load.
    pub(super) fn adopt_client(&mut self, client: PiClient) {
        self.runtime = RuntimeStatus {
            started_at: Some(Instant::now()),
            alive: true,
            exited: false,
            error: None,
        };
        self.client = Some(client);
        // A new process starts a new session stream: entry ids from the old
        // process mean nothing here.
        self.reset_quota_entries();
    }

    /// Tear down the active pi process (dropping `PiClient` kills the child).
    pub(super) fn drop_client(&mut self) {
        if self.execution.status().is_in_progress() {
            self.execution.stop();
        }
        if let Some(path) = &self.current_session_path {
            self.remember_session_outcome(path.clone(), self.execution.status());
        }
        // A process that exited on its own left its exit banner up; dropping
        // it supersedes that failure — a fresh process follows in the task /
        // restart paths, and an explicit stop acknowledges it.
        if self.runtime.exited {
            self.error = None;
            self.error_source = None;
        }
        self.client = None;
        self.runtime = RuntimeStatus::default();
        self.abort_requested = false;
        // A prompt the departing process never answered cannot be recognised
        // later, and the next process starts its own id sequence.
        self.prompt_in_flight = None;
        // Extension widgets belonged to the departing process.
        self.extension_widgets.clear();
        self.subagents = crate::subagent_status::SubagentBoard::default();
        self.subagent_scrolls.clear();
        self.subagent_list_scroll = ScrollHandle::new();
        self.subagent_scroll_placed = 0;
        self.subagent_list_rows = 0;
        self.clear_subagent_folds();
        // So did every custom UI surface: its extension is gone with the
        // process, so nothing can answer it (issue #76).
        self.custom_ui.clear();
        self.custom_ui_focus_pending = false;
        self.is_compacting = false;
        self.title_generating = false;
        // A modal dialog or approval belonged to that process too: it can never
        // be answered, and its keys would land in the next one.
        self.dialog = None;
        self.dialog_focus_pending = false;
        self.approval = None;
        self.approval_focus_pending = false;
        // Queued messages were queued for a run that no longer exists, and the
        // retry counter was counting that run's attempts.
        self.reset_queue();
        self.pending_follow_up = None;
        self.retrying = false;
        self.retry_detail = None;
        // A login in flight dies with its process; remember the provider so
        // the next `auth.list` can reconcile a completion that happened while
        // Orbit was reconnecting.
        self.auth.on_disconnect();
        self.quota.on_disconnect();
        self.reset_quota_entries();
        // A `get_entries` still in flight dies with its process; without this the
        // poll would stay blocked forever on a response that can never arrive.
        self.quota_entries_inflight = false;
    }

    /// Forget the bridge cursor and schedule an immediate poll. Called when
    /// the active session (and therefore the entry-id space) changes. The
    /// workflow mode is per session, so it resolves here too: a pending New
    /// Task choice is committed to the new id, otherwise the stored mode is
    /// loaded.
    pub(super) fn reset_quota_entries(&mut self) {
        self.quota_entries_cursor = None;
        self.quota_entries_inflight = false;
        self.quota_entries_bootstrap = QUOTA_ENTRY_BOOTSTRAP_POLLS;
        self.quota_entries_next_poll = Instant::now();
        if let Some(id) = self.session_id.clone() {
            match self.workflow_pending.take() {
                Some(mode) => {
                    crate::workflow::persist_for(&id, mode);
                    self.workflow_mode = mode;
                }
                None => self.workflow_mode = crate::workflow::load_for(&id),
            }
            // A different session is a different pushed-default record.
            self.default_model_pushed = DefaultModelPushed::default();
        }
    }

    /// The catalog entry for the configured default model, if the catalog has
    /// it. A removed or renamed model must never poison a session (fail soft);
    /// a default without a provider matches by id alone, mirroring pi's own
    /// fuzzy match.
    pub(super) fn default_model_entry(&self) -> Option<&ModelEntry> {
        let id = self.session_default.model_id.as_deref()?;
        self.available_models.iter().find(|model| {
            model.id == id
                && self
                    .session_default
                    .provider
                    .as_deref()
                    .is_none_or(|provider| model.provider == provider)
        })
    }

    /// The resolved default as `(provider, id)`.
    fn resolve_default_model(&self) -> Option<(String, String)> {
        self.default_model_entry()
            .map(|model| (model.provider.clone(), model.id.clone()))
    }

    /// Push the default model to the live session. One-shot: armed by a
    /// `new_session` birth and disarmed once there is nothing left that can be
    /// applied. An unset, unknown, or already-active model is skipped silently.
    ///
    /// May be called again when the model catalog arrives after the session
    /// (it is requested right after `get_state`); the `default_model_pushed`
    /// record keeps that from resending `set_model`.
    pub(super) fn apply_default_model(&mut self, cx: &mut Context<Self>) {
        if !self.default_model_armed {
            return;
        }
        let Some(session) = self.session_id.clone() else {
            return;
        };
        if self.client.is_none() {
            return;
        }
        if self.session_default.is_unset() {
            // Nothing to push; nothing to retry later.
            self.default_model_armed = false;
            return;
        }
        if self.default_model_pushed.session.as_deref() != Some(session.as_str()) {
            self.default_model_pushed = DefaultModelPushed {
                session: Some(session),
                ..DefaultModelPushed::default()
            };
        }
        let model = self.resolve_default_model();
        if model.is_none() && self.available_models.is_empty() {
            // The catalog that would let us honour the default hasn't loaded
            // yet; stay armed for the `get_available_models` response.
            return;
        }
        let model_live = model
            .as_ref()
            .is_some_and(|(provider, id)| self.model_id == *id && self.model_provider == *provider);
        if let Some((provider, id)) = &model {
            if !model_live {
                let sent = self.default_model_pushed.model.as_ref()
                    == Some(&(provider.clone(), id.clone()));
                if !sent {
                    // Sent directly, not through `set_model`: the public setter
                    // disarms the session-birth default, which the push itself
                    // must not do.
                    self.send(
                        CommandBody::SetModel {
                            model_id: id.clone(),
                            provider: provider.clone(),
                        },
                        "set_model",
                    );
                    self.default_model_pushed.model = Some((provider.clone(), id.clone()));
                    cx.notify();
                }
                // pi applies the switch asynchronously and resets the level to
                // the model's own default when it lands, so a thinking level
                // sent now would be lost. Stay armed: the `get_state` that
                // follows the `set_model` response calls us again with the new
                // model live.
                return;
            }
        } else {
            // A missing model disarms: a removed/renamed id must not make
            // every later catalog refresh retry (fail soft).
            self.default_model_armed = false;
            return;
        }
        // Only once the default model is live: the supported levels come from
        // its catalog entry, so a stale or unsupported level is skipped the
        // same way a missing model is.
        let levels = self
            .default_model_entry()
            .map(|model| model.thinking_levels.clone())
            .unwrap_or_default();
        let thinking = self
            .session_default
            .thinking
            .clone()
            .filter(|level| levels.iter().any(|candidate| candidate == level));
        if let Some(level) = thinking {
            if self.thinking_label != level
                && self.default_model_pushed.thinking.as_deref() != Some(level.as_str())
            {
                self.send(
                    CommandBody::SetThinkingLevel {
                        level: level.clone(),
                    },
                    "set_thinking_level",
                );
                self.default_model_pushed.thinking = Some(level);
                cx.notify();
            }
        }
        self.default_model_armed = false;
    }

    /// Write the default model and persist it. It takes effect on the next
    /// session; the composer chip can still change the live one.
    pub(super) fn set_default_model(&mut self, slot: crate::session_defaults::SessionDefault) {
        self.session_default = slot;
        self.session_default.persist();
    }

    /// Poll the active process for new quota-bridge session entries, at most
    /// once per [`QUOTA_ENTRY_POLL_INTERVAL`]. The bridge appends a snapshot
    /// only when the numbers change, so a quiet account returns no entries.
    pub(super) fn poll_quota_entries(&mut self) {
        if self.extensions.quota().is_none() && self.extensions.workflow().is_none() {
            return;
        }
        if self.quota_entries_inflight || self.quota_entries_next_poll > Instant::now() {
            return;
        }
        if !self.client.as_mut().is_some_and(|client| client.is_alive()) {
            return;
        }
        self.quota_entries_inflight = true;
        let interval = if self.quota_entries_bootstrap > 0 {
            self.quota_entries_bootstrap -= 1;
            QUOTA_ENTRY_BOOTSTRAP_INTERVAL
        } else {
            QUOTA_ENTRY_POLL_INTERVAL
        };
        self.quota_entries_next_poll = Instant::now() + interval;
        let sent = self.send(
            CommandBody::GetEntries {
                since: self.quota_entries_cursor.clone(),
            },
            "get_entries",
        );
        if !sent {
            // No response is coming; unblock the next attempt.
            self.quota_entries_inflight = false;
        }
    }

    /// Ask pi for provider auth capabilities. Harmless on a pi that doesn't
    /// implement `auth.*`: the failed `auth.list` response marks auth
    /// unsupported and the Providers page keeps the file/Terminal fallback.
    pub(super) fn probe_auth(&mut self) {
        let wanted = self.auth.on_reconnect();
        self.send(CommandBody::AuthList, "auth.list");
        for provider in wanted {
            self.send(CommandBody::AuthStatus { provider }, "auth.status");
        }
        self.refresh_quota();
    }

    /// Ask pi for account quota/balance/spend for every connected provider.
    /// Harmless on a pi build without the quota RPC: the failed response marks
    /// it unsupported and the cards simply omit quota meters.
    pub(super) fn refresh_quota(&mut self) {
        if self.quota.support() == QuotaSupport::Unsupported {
            return;
        }
        self.send(CommandBody::QuotaList { provider: None }, "quota.list");
    }

    /// Refresh capabilities and per-provider status against the *current*
    /// process without clearing what the card already knows. Used when the
    /// Providers page opens and on Refresh.
    pub(super) fn refresh_auth(&mut self) {
        if self.auth.support() == AuthSupport::Unsupported {
            return;
        }
        self.send(CommandBody::AuthList, "auth.list");
        self.refresh_quota();
        if self.auth.is_busy() {
            // A login is mid-flight; don't disturb it with status refreshes.
            return;
        }
        let providers: Vec<String> = self
            .auth
            .providers()
            .iter()
            .map(|provider| provider.id.clone())
            .collect();
        for provider in providers {
            self.send(CommandBody::AuthStatus { provider }, "auth.status");
        }
    }

    /// Perform the side effects the [`AuthManager`] emitted. The manager does
    /// no I/O; opening the browser and sending cancels happen here.
    pub(super) fn handle_auth_effects(&mut self, effects: Vec<AuthEffect>, cx: &mut Context<Self>) {
        if effects.is_empty() {
            return;
        }
        for effect in effects {
            match effect {
                AuthEffect::OpenUrl(url) => {
                    if let Err(err) = platform::open_url(&url) {
                        self.toast_warning(tr!("runtime.browser_failed", error = err));
                    }
                }
                AuthEffect::CancelLogin(session_id) => {
                    self.send(CommandBody::AuthCancel { session_id }, "auth.cancel");
                }
                AuthEffect::RefreshProviders => {
                    // The login/logout already took effect in pi; no restart
                    // banner is needed on the RPC path.
                    self.provider_auth_dirty = false;
                    if let Some(session) = self.auth.login() {
                        if session.phase == LoginPhase::Succeeded {
                            self.toast_success(tr!(
                                "runtime.signed_in_to",
                                provider = session.provider
                            ));
                        }
                    }
                    self.send(CommandBody::AuthList, "auth.list");
                    self.refresh_quota();
                    self.refresh_catalogs();
                    self.reload_custom_providers(cx);
                }
            }
        }
        cx.notify();
    }

    /// Start a provider login through the RPC namespace. `method` comes from
    /// the provider's discovered capability, never a hardcoded flow.
    pub(super) fn auth_start_login(
        &mut self,
        provider: String,
        name: String,
        method: String,
        cx: &mut Context<Self>,
    ) {
        if self.auth.support() == AuthSupport::Unsupported {
            // pi has no auth RPC: hand the login to the Terminal the way
            // the page has always done.
            self.provider_oauth_login(provider, name, cx);
            return;
        }
        let session_id = self.auth.start_login(&provider, &method);
        self.send(
            CommandBody::AuthLogin {
                provider,
                method,
                session_id: Some(session_id),
            },
            "auth.login",
        );
        cx.notify();
    }

    /// Cancel the active login and tell pi to tear its flow down.
    pub(super) fn auth_cancel_login(&mut self, cx: &mut Context<Self>) {
        if let Some(session_id) = self.auth.cancel_login() {
            self.send(CommandBody::AuthCancel { session_id }, "auth.cancel");
        }
        cx.notify();
    }

    pub(super) fn runtime_state(&self) -> RuntimeState {
        if self.client.is_none() {
            if self.runtime.error.is_some() {
                RuntimeState::Failed
            } else {
                RuntimeState::Stopped
            }
        } else if self.runtime.exited || !self.runtime.alive {
            RuntimeState::Exited
        } else {
            RuntimeState::Running
        }
    }

    /// Whether the active pi process is alive and has not exited — the only
    /// state in which commands sent over its stdin can still be answered. A
    /// never-started, stopped, or dead process all read as not live.
    pub(super) fn runtime_is_live(&self) -> bool {
        matches!(self.runtime_state(), RuntimeState::Running)
    }

    /// A queued retry may have been superseded by an explicit start or stop.
    pub(super) fn sidebar_runtime_retrying(&self) -> bool {
        self.auto_restart_scheduled && self.should_auto_restart_now()
    }

    /// Spawn the pi process (Start button). No-op while one is running.
    pub(super) fn runtime_start(&mut self, cx: &mut Context<Self>) {
        if self.client.is_some() {
            return;
        }
        let cwd = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        match self.extensions.spawn(&cwd, true, &self.mcp.secret_env()) {
            Ok(client) => {
                self.adopt_client(client);
                self.mcp_stamp = self.mcp.fingerprint_for(Some(&cwd));
                self.send(CommandBody::GetState, "get_state");
                self.refresh_catalogs();
                // Capability probes queue after the state request.
                self.probe_auth();
                self.toast_info(tr!("runtime.pi_process_started"));
            }
            Err(err) => {
                let message = tr!("runtime.pi_spawn_failed", error = err);
                self.client = None;
                self.runtime.error = Some(message.clone());
                self.toast_error(message);
            }
        }
        cx.notify();
    }

    /// Stop the pi process (Stop button).
    pub(super) fn runtime_stop(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        self.cancel_fork_followup();
        self.drop_client();
        self.busy = false;
        // The prompt this process was running has no answer coming: close the
        // transcript's live row too, or it would spin on a dead pi.
        self.transcript.abandon_reply();
        self.toast_info(tr!("runtime.pi_process_stopped"));
        cx.notify();
    }

    /// Stop then start the pi process (Restart button, and the automatic
    /// respawn after an exit).
    pub(super) fn runtime_restart(&mut self, cx: &mut Context<Self>) {
        self.cancel_fork_followup();
        self.drop_client();
        self.runtime_start(cx);
        self.reattach_current_session();
    }

    /// Point a just-spawned process back at the session the UI is showing. A
    /// fresh launch opens a *new* session, so without this pi answers on an
    /// empty one while the transcript still shows the old conversation and the
    /// next prompt reaches a model with none of it — the same reattach the MCP
    /// restart performs.
    pub(super) fn reattach_current_session(&mut self) {
        let Some(session_path) = self.current_session_path.clone() else {
            return;
        };
        if self.client.is_none() {
            // The spawn failed; its own banner says why.
            return;
        }
        self.send(
            CommandBody::SwitchSession {
                session_path: session_path.to_string_lossy().into_owned(),
            },
            "switch_session",
        );
    }

    /// Decide whether an exit is worth respawning on its own, consuming one
    /// attempt when it is. A process that stayed up for
    /// [`AUTO_RESTART_HEALTHY_UPTIME`] resets the count first: that is a new
    /// failure, not a crash loop, so the budget refills.
    pub(super) fn take_auto_restart(&mut self) -> bool {
        if self
            .runtime
            .started_at
            .is_some_and(|at| at.elapsed() >= AUTO_RESTART_HEALTHY_UPTIME)
        {
            self.auto_restarts = 0;
        }
        if self.auto_restarts >= AUTO_RESTART_LIMIT {
            return false;
        }
        self.auto_restarts += 1;
        true
    }

    /// Whether the queued respawn should still happen. A Start or Stop while
    /// it waits clears `exited` or fills `client`, so the user wins.
    pub(super) fn should_auto_restart_now(&self) -> bool {
        self.client.is_none() && self.runtime.exited
    }

    /// Handle the active process exiting: respawn it on its own, or — once the
    /// retry budget is spent — raise the banner that points at the Reconnect
    /// button (issue #76).
    ///
    /// One death can arrive as two events (the writer reports a dead stdin, the
    /// reader its EOF), so an exit that finds a respawn already queued is
    /// ignored: it must neither spend a second attempt nor raise a banner over
    /// a recovery that is already on its way.
    pub(super) fn on_process_exited(&mut self, cx: &mut Context<Self>) {
        if self.auto_restart_scheduled {
            return;
        }
        if !self.take_auto_restart() {
            self.set_error(self.process_exited_banner());
            return;
        }
        self.auto_restart_scheduled = true;
        cx.spawn(async move |this, cx| {
            Timer::after(AUTO_RESTART_DELAY).await;
            let _ = this.update(cx, |app, cx| {
                app.auto_restart_scheduled = false;
                if app.should_auto_restart_now() {
                    app.runtime_restart(cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The banner for a dead process: pi's own last stderr line when it left
    /// one, so "process exited" is not a dead end. A bundled `.app` is the
    /// case that needs it most — a launch whose `PATH` cannot reach `node`
    /// dies on the shim's `env: node: No such file or directory`, which used
    /// to reach the user as nothing at all (issue #81).
    ///
    /// The line is redacted (MCP secret values never reach a banner) and
    /// clipped, so a stack trace cannot take over the bar.
    fn process_exited_banner(&self) -> String {
        let detail = self
            .client
            .as_ref()
            .and_then(|client| {
                client
                    .recent_stderr(8)
                    .into_iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
            })
            .map(|line| clip_stderr_line(&self.mcp.redact_text(line.trim())));
        match detail {
            Some(detail) => tr!("events.process_exited_detail", error = detail),
            None => tr!("events.process_exited"),
        }
    }

    /// Register the active client under the session file pi reports. The
    /// startup process has no known path until pi's first state/stats
    /// response; a `new_session` re-keys it the same way (the handler clears
    /// `current_session_path` and the next response adopts the new file).
    /// An explicit switch never re-keys — its path is already claimed.
    pub(super) fn adopt_session_file(&mut self, file: PathBuf) {
        if self.current_session_path.is_none() {
            self.current_session_path = Some(file);
            self.stamp_held_fork_title();
        }
    }

    /// Park a background session, evicting the least-recently parked idle one
    /// if over the cap. Running sessions are never evicted.
    pub(super) fn park(&mut self, path: PathBuf, mut parked: ParkedSession) {
        parked.parked_at = Instant::now();
        if self.lives.len() >= MAX_LIVE_SESSIONS {
            let victim = self
                .lives
                .iter()
                .filter(|(_, p)| !p.busy)
                .min_by_key(|(_, p)| p.parked_at)
                .map(|(k, _)| k.clone());
            if let Some(victim) = victim {
                self.lives.remove(&victim);
            }
        }
        self.lives.insert(path, parked);
    }

    /// Reap idle parked processes past [`PARKED_IDLE_TTL`]. Running sessions
    /// stay regardless of age. Called from the heartbeat; a no-op when nothing
    /// is parked.
    pub(super) fn reap_idle_parked(&mut self) {
        if self.lives.is_empty() && self.pending_parks.is_empty() {
            return;
        }
        let now = Instant::now();
        let alive = |parked: &ParkedSession| {
            parked.busy || now.duration_since(parked.parked_at) < PARKED_IDLE_TTL
        };
        self.lives.retain(|_, parked| alive(parked));
        self.pending_parks.retain(alive);
    }
}

/// One stderr line, capped so a multi-kilobyte stack trace cannot take over
/// the error banner.
fn clip_stderr_line(line: &str) -> String {
    const MAX_CHARS: usize = 200;
    if line.chars().count() <= MAX_CHARS {
        return line.to_string();
    }
    let mut out: String = line.chars().take(MAX_CHARS).collect();
    out.push('…');
    out
}
