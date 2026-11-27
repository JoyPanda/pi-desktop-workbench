use super::helpers::*;
use super::*;
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
        // A rejected follow-up/steer is unwound from the local queue in
        // `on_response` (matched by RPC id) before this banner is raised.
        // A rejected prompt never opened a turn. The run state was set
        // optimistically when the prompt went out, so unwind it here instead of
        // leaving the composer on Stop and the transcript waiting for an answer
        // that is not coming (see `submit_as`).
        if command == "prompt" {
            self.execution.fail();
            self.busy = false;
            self.transcript.abandon_reply();
            // A post-X follow-up held for this turn must not flush onto an
            // idle pi after the prompt is rejected.
            self.settle_local_queue(cx);
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
            Ok((id, _)) => {
                #[cfg(test)]
                self.sent_commands.push(label.to_string());
                // A model or session change means later `get_state` answers
                // describe this session, not the model an empty argv boots.
                if matches!(
                    label,
                    "set_model"
                        | "set_thinking_level"
                        | "cycle_model"
                        | "cycle_thinking_level"
                        | "switch_session"
                        | "fork"
                        | "clone"
                ) {
                    self.startup_model_reportable = false;
                }
                // A manual level change supersedes the level the argv carried.
                // Without this, a correction accepted afterwards would re-ask
                // the *argv* level over the reader's explicit choice. The
                // correction's own re-ask is a different command id, so it
                // does not disarm itself.
                if matches!(label, "set_thinking_level" | "cycle_thinking_level")
                    && self.startup_model_fix.as_deref() != Some(id.as_str())
                {
                    if let Some(slot) = self.startup_model_expected.as_mut() {
                        slot.thinking = None;
                    }
                }
                Some(id)
            }
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
    ///
    /// A session with no process has nothing to ask: its ring shows the last
    /// live snapshot, and the "pi is not running" toast a probe would raise is
    /// noise for something the reader never asked for (opening the context
    /// card on a cold session, a Compact that is still starting pi).
    pub(super) fn refresh_context_stats(&mut self) {
        if self.client.is_none() {
            return;
        }
        self.send(CommandBody::GetSessionStats, "get_session_stats");
    }

    /// Make `client` the active pi process, resetting uptime / exit state.
    ///
    /// Callers send their primary command (`switch_session` / `new_session` /
    /// `get_state`) *before* [`probe_auth`](Self::probe_auth), so the slow
    /// provider-auth/quota probes never queue in front of the session load.
    pub(super) fn adopt_client(&mut self, client: PiClient) {
        // Whatever spawn was still on its way is moot: this process owns the
        // view now, so a late result must not replace it.
        self.invalidate_pending_spawn();
        self.runtime = RuntimeStatus {
            started_at: Some(Instant::now()),
            alive: true,
            exited: false,
            error: None,
        };
        self.client = Some(client);
        // Queued events belonged to the process just replaced. Warm resume
        // installs the incoming backlog after this returns.
        self.foreground_events.clear();
        // The birth push belongs to the process that armed it. This one has
        // not accepted `new_session` yet; a resumed process must not inherit it.
        self.default_model_armed = false;
        // A resumed or replaced process is not pi's untouched startup model
        // until the caller says this spawn carried no `--model`.
        self.startup_model_reportable = false;
        // A new process starts a new session stream: entry ids from the old
        // process mean nothing here.
        self.reset_quota_entries();
        // Nothing is in flight in a process that just started, so a stop left
        // pending on the previous one must not escalate against this one.
        self.abort_sent_at = None;
        // A pin belonged to the process being replaced. The caller sets a new
        // one only after this process has accepted `new_session`.
        self.startup_model_pinned = false;
        // So did the argv model check: a replaced process answers for itself.
        self.startup_model_expected = None;
        self.startup_model_fix = None;
        // A new process has no in-flight queue RPCs. Resume paths restore
        // `queue_wire` from the parked session after this.
        self.queue_wire = QueueWire::default();
    }

    /// Give up the spawn still on its way, if any. Any process the app already
    /// owns — or just gave up — supersedes it: the late result finds no pending
    /// ticket to match in [`Self::admit_spawned_client`] and drops the child it
    /// started instead of adopting it over the current view.
    pub(super) fn invalidate_pending_spawn(&mut self) {
        self.pending_spawn = None;
        // A cancelled spawn never reaches `new_session`. `adopt_client` also
        // comes through here, before the NewSession arm raises the pin for
        // the process it just took.
        self.startup_model_pinned = false;
    }

    /// End a replacement: name reports may paint again, and a click that
    /// arrived while pi was mid-replacement runs now instead of being dropped.
    pub(super) fn end_replacement(&mut self, cx: &mut Context<Self>) {
        self.replacement_in_flight = None;
        if let Some(session) = self.pending_switch.take() {
            self.switch_to_session(session, true, cx);
        }
    }

    /// Whether a background spawn is still on its way.
    pub(super) fn spawn_pending(&self) -> bool {
        self.pending_spawn.is_some()
    }

    /// Start a pi process for a brand-new session without blocking the frame
    /// that asked for it, and without knowing yet why it is wanted.
    ///
    /// Two callers share this seam because both start a brand-new session on a
    /// new process, not a resume:
    ///
    /// - [`SpawnThen::Prompt`] — the first message of a New Task. The click
    ///   listed the row and showed the page without starting anything; the
    ///   message went on screen and is delivered here once pi is up.
    /// - [`SpawnThen::NewSession`] — the Settings restarts that reload
    ///   credentials or a catalog onto an empty draft
    ///   (`resume_after_process_drop`).
    ///
    /// [`Self::runtime_start`] and the session-switch paths still spawn in
    /// place — they send a command the moment the client exists, and their
    /// ordering (re-attach a parked session, keep a run alive) needs its own
    /// pass.
    pub(super) fn spawn_pi_client_later(
        &mut self,
        cwd: PathBuf,
        then: SpawnThen,
        cx: &mut Context<Self>,
    ) {
        self.spawn_pi_client_inner(cwd, then, None, cx);
    }

    /// Start a process for an existing session that has none ([`SpawnThen::
    /// ResumePrompt`]). The child gets no `--model`: a resumed session keeps
    /// the model recorded in its file, never Orbit's default.
    pub(super) fn spawn_pi_client_to_resume(
        &mut self,
        cwd: PathBuf,
        session_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.spawn_pi_client_inner(cwd, SpawnThen::ResumePrompt, Some(session_path), cx);
    }

    fn spawn_pi_client_inner(
        &mut self,
        cwd: PathBuf,
        then: SpawnThen,
        session_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.invalidate_pending_spawn();
        self.spawn_generation = self.spawn_generation.wrapping_add(1);
        // Everything the spawn needs is a plain value: the extension entry
        // points, the MCP secret values (so pi can expand `${NAME}` in
        // `mcp.json` without the config holding a credential), and the folder.
        let extensions = self.extensions.clone();
        let env = self.mcp.secret_env();
        // The open draft's model, when the reader picked one; otherwise the
        // session default. Both become the child's `--provider` / `--model`
        // / `--thinking`, which is what the first turn actually runs on.
        // The pin is not raised here: a spawn that is cancelled before it
        // lands must not suppress the next session's default.
        let resuming = then == SpawnThen::ResumePrompt;
        let chosen = if resuming {
            None
        } else {
            self.current_draft_model()
        };
        // Recorded now: by the time the process lands the row may already be
        // a sent session, and `current_draft_model` would no longer mean
        // "this child's argv was a composer choice".
        let pinned_model = chosen.is_some();
        // The slot the child boots on: the draft's own choice, else the
        // configured default. Kept as a value because it is needed twice —
        // once as argv, and once as the model pi is *asked* for, which is not
        // necessarily the one its fuzzy `--model` pattern resolves to.
        let startup_slot = chosen.unwrap_or_else(|| self.session_default.clone());
        let startup_args = if resuming {
            // Boot straight into the file. `--session` with a path opens it
            // before RPC starts, so the prompt never lands on an empty session.
            match session_path.as_ref() {
                Some(path) => vec![
                    "--session".to_string(),
                    path.to_string_lossy().into_owned(),
                ],
                None => Vec::new(),
            }
        } else {
            startup_slot.cli_args()
        };
        let requested_model = (!resuming).then_some(startup_slot).filter(|slot| !slot.is_unset());
        // The model files as they are now: the child reads them at boot, so
        // the catalog it reports cannot be newer than this.
        let sources_stamp = crate::catalog_cache::sources_stamp();
        #[cfg(test)]
        let test_bin = self.spawn_bin.clone();
        #[cfg(not(test))]
        let test_bin: Option<PathBuf> = None;
        // The MCP stamp travels with the ticket, not read back when the result
        // lands: these are the secrets this child was actually spawned with,
        // and a config edited in between must not be recorded as its own.
        let mcp_stamp = self.mcp.fingerprint_for(Some(&cwd));
        let ticket = SpawnTicket {
            generation: self.spawn_generation,
            cwd,
            mcp_stamp,
            then,
            startup_args,
            pinned_model,
            requested_model,
            sources_stamp,
            session_path,
        };
        self.pending_spawn = Some(ticket.clone());
        let ticket_for_spawn = ticket.clone();
        cx.spawn(async move |this, cx| {
            let spawned = cx
                .background_executor()
                .spawn(async move {
                    let started = Instant::now();
                    let result = match test_bin {
                        Some(bin) => PiClient::spawn_with_bin(
                            bin.to_string_lossy().as_ref(),
                            &ticket_for_spawn.cwd,
                            None,
                        ),
                        None => extensions.spawn_with_startup_args(
                            &ticket_for_spawn.cwd,
                            &ticket_for_spawn.startup_args,
                            &env,
                        ),
                    };
                    if crate::bundled_extensions::spawn_timing_enabled() {
                        eprintln!(
                            "orbit: pi spawn took {:?} for {}",
                            started.elapsed(),
                            ticket_for_spawn.cwd.display()
                        );
                    }
                    result
                })
                .await;
            let _ = this.update(cx, |app, cx| app.admit_spawned_client(ticket, spawned, cx));
        })
        .detach();
        self.set_status(tr!("runtime.starting_pi"));
        cx.notify();
    }

    /// `get_messages`, staged choices, then state and catalogs. The prompt
    /// waits for the snapshot. Returns whether `get_messages` was queued.
    pub(super) fn begin_resumed_session(&mut self, cx: &mut Context<Self>) -> bool {
        let sent = self.send(CommandBody::GetMessages, "get_messages");
        self.apply_staged_choices();
        self.send(CommandBody::GetState, "get_state");
        self.refresh_catalogs();
        self.probe_auth();
        let _ = cx;
        sent
    }

    /// A send whose process landed after the user had already left. Run the
    /// same handshake on that process, and adopt it only when the page has
    /// come back to this session and is still empty of a process.
    fn admit_detached_spawn(
        &mut self,
        shell: DetachedSpawn,
        spawned: anyhow::Result<PiClient>,
        cx: &mut Context<Self>,
    ) {
        let client = match spawned {
            Ok(client) => client,
            Err(_) => {
                if let Some(prompt) = shell.pending_prompt {
                    self.stash_failed_send(
                        shell.session_path.as_deref(),
                        shell.draft,
                        prompt.composer,
                    );
                }
                return;
            }
        };
        let back = self.client.is_none()
            && match shell.ticket.then {
                SpawnThen::ResumePrompt => {
                    shell.session_path.is_some()
                        && self.current_session_path == shell.session_path
                }
                SpawnThen::Prompt => {
                    shell.draft.is_some() && self.current_draft_id == shell.draft
                }
                SpawnThen::NewSession => false,
            };
        if back {
            self.transcript = shell.transcript;
            self.pending_prompt = shell.pending_prompt;
            self.execution = shell.execution;
            self.busy = shell.busy;
            self.startup_model_pinned = shell.startup_model_pinned;
            self.turns = shell.turns;
            self.added = shell.added;
            self.removed = shell.removed;
            if let Some(id) = shell.draft {
                self.current_draft_id = Some(id);
            }
            if let Some(path) = shell.session_path.clone() {
                self.current_session_path = Some(path);
            }
            self.adopt_client(client);
            self.mcp_stamp = shell.ticket.mcp_stamp;
            self.startup_model_reportable = !shell
                .ticket
                .startup_args()
                .iter()
                .any(|arg| arg == "--model");
            // A detached send is checked against its own argv like any other.
            self.startup_model_expected = shell.ticket.requested_model.clone();
            self.startup_model_fix = None;
            self.process_sources_stamp = shell.ticket.sources_stamp;
            match shell.ticket.then {
                SpawnThen::Prompt => {
                    if let Some(pending) = self.pending_prompt.take() {
                        match self.send_tracked(pending.body.clone(), &pending.label) {
                            Some(id) => {
                                self.prompt_in_flight = Some(id.clone());
                                self.execution.prompt_id = Some(id);
                            }
                            // The write failed: put the prompt back so the
                            // unwind below has something to hand to the composer.
                            None => {
                                self.pending_prompt = Some(pending);
                                self.unwind_pending_prompt(cx);
                            }
                        }
                    }
                    self.send(CommandBody::GetState, "get_state");
                }
                SpawnThen::ResumePrompt => {
                    if !self.begin_resumed_session(cx) {
                        self.pending_compact = None;
                        self.unwind_pending_prompt(cx);
                    }
                }
                SpawnThen::NewSession => {}
            }
            cx.notify();
            return;
        }
        self.park_detached_client(client, shell);
        cx.notify();
    }

    /// The main-thread half of [`Self::spawn_pi_client_later`]: adopt the
    /// process if the view is still waiting for it, then do what the ticket
    /// was taken for.
    ///
    /// A ticket that is no longer the pending one is dropped here — dropping
    /// the [`PiClient`] kills the child — so a spawn that finished after the
    /// user opened another session never steals it.
    pub(super) fn admit_spawned_client(
        &mut self,
        ticket: SpawnTicket,
        spawned: anyhow::Result<PiClient>,
        cx: &mut Context<Self>,
    ) {
        // The view still has to be waiting for *this* spawn: a newer one, or a
        // view that gave up (switch, stop), leaves nothing to match.
        if !self
            .pending_spawn
            .as_ref()
            .is_some_and(|pending| pending.generation == ticket.generation)
        {
            if let Some(pos) = self
                .detached_spawns
                .iter()
                .position(|shell| shell.ticket.generation == ticket.generation)
            {
                let shell = self.detached_spawns.remove(pos);
                self.admit_detached_spawn(shell, spawned, cx);
            }
            return;
        }
        self.pending_spawn = None;
        match spawned {
            Ok(client) => {
                self.adopt_client(client);
                self.mcp_stamp = ticket.mcp_stamp;
                // Empty argv is pi's own startup model. A `--model` flag, or
                // a later switch, is this session only.
                self.startup_model_reportable =
                    !ticket.startup_args.iter().any(|arg| arg == "--model");
                // The argv model is a request, not a fact: pi resolves
                // `--model` as a fuzzy pattern, so what the process actually
                // came up on is checked against it (see
                // [`Self::reconcile_startup_model`]).
                self.startup_model_expected = ticket.requested_model.clone();
                self.startup_model_fix = None;
                self.process_sources_stamp = ticket.sources_stamp;
                match ticket.then {
                    SpawnThen::Prompt => {
                        // Argv already selected the model, and this path does
                        // not send `new_session`. `adopt_client` cleared the
                        // pin. The message that started this process is already
                        // on screen; hand it over now, and let its id drive the
                        // reply bookkeeping the way a live send does.
                        if let Some(pending) = self.pending_prompt.take() {
                            match self.send_tracked(pending.body.clone(), &pending.label) {
                                Some(id) => {
                                    // Delivered, so it joins the ↑/↓ recall log
                                    // — which is not when it was typed.
                                    self.prompt_history
                                        .borrow_mut()
                                        .record(&pending.composer.text);
                                    self.prompt_in_flight = Some(id.clone());
                                    self.execution.prompt_id = Some(id);
                                }
                                // The write failed: the message never left, so
                                // it goes back like any other failed send. Put
                                // it back first — the unwind reads pending_prompt.
                                None => {
                                    self.pending_prompt = Some(pending);
                                    self.unwind_pending_prompt(cx);
                                }
                            }
                        }
                        // The process is up either way. Catalog refresh does
                        // not fill the model chips or the session id, so ask
                        // pi what it started on before those probes queue.
                        self.send(CommandBody::GetState, "get_state");
                    }
                    SpawnThen::NewSession => {
                        // The session command goes out first: the catalog,
                        // auth, and quota probes queue behind it (see
                        // [`Self::adopt_client`]). Pin only while that command
                        // is actually queued, and only when the argv was a
                        // composer choice — a failed send must not suppress
                        // the next birth's default.
                        self.replacement_in_flight =
                            self.send_tracked(CommandBody::NewSession, "new_session");
                        self.startup_model_pinned =
                            self.replacement_in_flight.is_some() && ticket.pinned_model;
                        self.set_status(tr!("session.new_session"));
                    }
                    SpawnThen::ResumePrompt => {
                        // `--session` already opened the file. Load pi's view,
                        // then deliver the prompt once that snapshot lands.
                        if !self.begin_resumed_session(cx) {
                            // No snapshot will land, so nothing waiting on one
                            // can be delivered: drop the owed compaction too.
                            self.pending_compact = None;
                            self.unwind_pending_prompt(cx);
                        }
                    }
                }
                if ticket.then != SpawnThen::ResumePrompt {
                    self.refresh_catalogs();
                    self.probe_auth();
                }
            }
            Err(err) => {
                self.startup_model_pinned = false;
                // Nothing landed, so there is no process to check a request
                // against — and no reply that could ever release the hold.
                self.startup_model_expected = None;
                self.startup_model_fix = None;
                // A compaction this start owed goes with it.
                self.pending_compact = None;
                // Nothing is adopted, so only what this ticket owed is undone:
                // a waiting message goes back into the composer, and the banner
                // explains why nothing sent. Tagged with the command the spawn
                // owed, so a later success on that command retires it instead
                // of leaving a red banner the user has to dismiss.
                let source = match ticket.then {
                    SpawnThen::Prompt => "prompt",
                    SpawnThen::NewSession => "new_session",
                    SpawnThen::ResumePrompt => "prompt",
                };
                let message = tr!("runtime.pi_spawn_failed", error = err);
                self.client = None;
                self.runtime.error = Some(message.clone());
                if matches!(ticket.then, SpawnThen::Prompt | SpawnThen::ResumePrompt) {
                    self.unwind_pending_prompt(cx);
                }
                self.set_error_from(source, message.clone());
                self.set_status(message);
            }
        }
        cx.notify();
    }

    /// Tear down the active pi process (dropping `PiClient` kills the child).
    pub(super) fn drop_client(&mut self) {
        // Giving up the view gives up its pending spawn too: the caller is
        // about to open something else, and a client that arrives afterwards
        // would land on the wrong page.
        self.invalidate_pending_spawn();
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
        // What the meter shows stays, but it is no longer a live reply.
        self.context_is_snapshot = self.context.is_some() || self.session_usage.is_some();
        self.client = None;
        self.foreground_events.clear();
        self.runtime = RuntimeStatus::default();
        self.abort_requested = false;
        // The stop this clock belonged to has no process left to answer it.
        self.abort_sent_at = None;
        // A prompt the departing process never answered cannot be recognised
        // later, and the next process starts its own id sequence.
        self.prompt_in_flight = None;
        // The commands went with the process they were sent to.
        self.replacement_in_flight = None;
        self.pending_switch = None;
        self.startup_model_pinned = false;
        self.default_model_armed = false;
        self.startup_model_reportable = false;
        // A correction still in flight died with the process; the chips must
        // not stay held on a model nothing is running.
        self.startup_model_expected = None;
        self.startup_model_fix = None;
        // So did a compaction waiting on it.
        self.pending_compact = None;
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
        // A draft already chose its model: the process was started with that
        // argv, and pushing the configured default here would replace it.
        if self.startup_model_pinned || !self.default_model_armed {
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

    /// Whether the composer chips must stay on the model the argv asked for:
    /// pi booted a different one and the exact correction has not answered
    /// yet. Painting pi's answer in that window is what read as "the model
    /// changed by itself" — the correction's own `get_state` repaints.
    pub(super) fn startup_model_held(&self, data: &serde_json::Value) -> bool {
        let Some(expected) = self.startup_model_expected.as_ref().and_then(|slot| slot.as_pair())
        else {
            return false;
        };
        // Without a provider there is nothing exact to ask for (`set_model`
        // matches provider *and* id), so pi's resolution stands and the chips
        // follow it rather than holding a pick nothing can move pi to.
        if expected.0.is_empty() {
            return false;
        }
        // A reply that names no model has nothing to contradict the pick.
        let Some(reported) = reported_model_pair(data) else {
            return false;
        };
        !same_model(&reported, &expected)
    }

    /// Ask pi for the model its argv named when the process came up on
    /// another one.
    ///
    /// pi resolves `--model` as a *pattern* — an exact id first, then a fuzzy
    /// match — so a catalog entry its registry no longer lists exactly boots
    /// a neighbour instead (`--model gateway/glm-5` can come up on
    /// `glm-5-flash`). The RPC `set_model` resolves exactly, which makes it
    /// both the correction and the proof that the pick is gone. One attempt
    /// per process: a refusal is reported, never retried.
    pub(super) fn reconcile_startup_model(
        &mut self,
        data: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.startup_model_expected.clone() else {
            return;
        };
        let Some(expected) = slot.as_pair() else {
            // Nothing was actually requested; there is nothing to check.
            self.startup_model_expected = None;
            return;
        };
        let Some(reported) = reported_model_pair(data) else {
            return;
        };
        if same_model(&reported, &expected) {
            // A correction still out owns the ending: pi lands the switch
            // before its reply is written, so a matching report can arrive
            // first — and consuming the request here would make the success
            // fail its id guard and skip the level the argv carried.
            if self.startup_model_fix.is_none() {
                self.startup_model_expected = None;
            }
            return;
        }
        // One attempt per process, and its *reply* is what ends the hold: a
        // second report of the substitute changes nothing, so a report can
        // never be the reason the chips stay held.
        if self.startup_model_fix.is_some() {
            return;
        }
        // Without a provider the argv was a bare pattern and `set_model`
        // (which matches provider *and* id) has nothing exact to ask for.
        if expected.0.is_empty() {
            self.startup_model_expected = None;
            return;
        }
        match self.send_tracked(
            CommandBody::SetModel {
                model_id: expected.1,
                provider: expected.0,
            },
            "set_model",
        ) {
            // Tracked by id: a refusal of some *other* model choice (the
            // reader can pick while this is in flight) is not this one's.
            Some(id) => self.startup_model_fix = Some(id),
            // Nothing was written, so nothing will answer: let the chips
            // follow pi instead of holding a pick nobody is fetching.
            None => self.startup_model_expected = None,
        }
        cx.notify();
    }

    /// pi refused the one exact correction. The hold is released whatever the
    /// reason — the `get_state` behind a refused `set_model` then paints what
    /// pi is really on, so the chips can never stay pinned to a model nothing
    /// is running. Only a *missing* model also means the catalog that offered
    /// it is wrong: that re-reads pi's list so the picker drops the dead entry
    /// and says which pick died. Any other refusal (no API key, a signed-out
    /// provider) keeps the command's own banner, because naming the model as
    /// gone would be a lie and dropping a valid entry would cost the reader.
    pub(super) fn startup_model_refused(
        &mut self,
        id: &str,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if self.startup_model_fix.as_deref() != Some(id) {
            return;
        }
        self.startup_model_fix = None;
        let slot = self.startup_model_expected.take();
        let missing = error.is_some_and(|error| error.to_ascii_lowercase().contains("not found"));
        if let (true, Some(slot)) = (missing, slot) {
            self.refresh_catalogs();
            if let Some(model) = slot.model_id.filter(|model| !model.is_empty()) {
                self.toast_warning(tr!("session.startup_model_missing", model = model));
            }
        }
        cx.notify();
    }

    /// pi accepted the one exact correction, so the request is satisfied and
    /// the chips follow pi from here on.
    pub(super) fn startup_model_corrected(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.startup_model_fix.as_deref() != Some(id) {
            return;
        }
        self.startup_model_fix = None;
        let slot = self.startup_model_expected.take();
        self.reask_startup_thinking(slot, cx);
    }

    /// A switch lands with the thinking level reset to the new model's own
    /// default, which is why the level the argv carried is asked for again —
    /// validated against the catalog first, since a level this model does not
    /// offer would only buy a red banner for something the reader never
    /// clicked. Also runs for a correction whose success was drained while
    /// parked ([`ParkedSession::startup_model_corrected`]): the switch pi
    /// landed reset the level just the same.
    pub(super) fn reask_startup_thinking(
        &mut self,
        slot: Option<crate::session_defaults::SessionDefault>,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = slot else {
            return;
        };
        let Some(level) = slot.thinking.clone().filter(|level| !level.is_empty()) else {
            return;
        };
        if !self.thinking_level_fits(&slot, &level) {
            return;
        }
        if self.send(CommandBody::SetThinkingLevel { level }, "set_thinking_level") {
            cx.notify();
        }
    }

    /// Write the default model and persist it. It takes effect on the next
    /// session; the composer chip can still change the live one.
    pub(super) fn set_default_model(&mut self, slot: crate::session_defaults::SessionDefault) {
        self.session_default = slot;
        self.session_default.persist();
        // Only a page with no session of its own is showing this default.
        // A stopped conversation, and a process that is still starting, keep
        // the model they already have.
        if self.transcript.is_empty() && self.current_session_path.is_none() {
            self.paint_unsent_model();
        }
        if let Some((PickerKind::Thinking, _)) = &self.model_selector {
            if !self.can_choose_thinking() {
                self.model_selector = None;
            }
        }
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

    /// Replace the model catalog from a get_available_models payload — a live
    /// process's reply or the cached copy a cold launch shows.
    pub(super) fn apply_models_payload(&mut self, data: &Value) {
        self.available_models = data
            .get("models")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(serde_json::Value::as_str)?;
                        let name = m.get("name").and_then(serde_json::Value::as_str)?;
                        let provider =
                            m.get("provider").and_then(serde_json::Value::as_str)?;
                        Some(ModelEntry {
                            id: id.into(),
                            name: name.into(),
                            provider: provider.into(),
                            context_window: m
                                .get("contextWindow")
                                .and_then(serde_json::Value::as_u64),
                            thinking_levels: catalog_thinking_levels(m),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    /// Replace the slash-command list from a get_commands payload.
    pub(super) fn apply_commands_payload(&mut self, data: &Value) {
        let home = crate::platform::home_dir_opt();
        let workspace = self.current_workspace.clone();
        self.slash_commands = data
            .get("commands")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        let name = c.get("name").and_then(Value::as_str)?;
                        let description =
                            c.get("description").and_then(Value::as_str).unwrap_or("");
                        // `source` + `sourceInfo` carry the command's
                        // provenance; Orbit folds it into a scope badge.
                        let source = c.get("source").and_then(Value::as_str).unwrap_or("");
                        let info = c.get("sourceInfo");
                        let scope =
                            info.and_then(|i| i.get("scope")).and_then(Value::as_str);
                        let origin =
                            info.and_then(|i| i.get("origin")).and_then(Value::as_str);
                        let path = info.and_then(|i| i.get("path")).and_then(Value::as_str);
                        Some(SlashCommand {
                            name: name.to_string(),
                            description: description.to_string(),
                            scope: mentions::classify_scope(
                                source,
                                scope,
                                origin,
                                path,
                                workspace.as_deref(),
                                home.as_deref(),
                            ),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    /// Show the last catalog pi reported, so a cold launch has a model picker
    /// and a / menu before any process exists (INTENT D18). A live process's
    /// reply replaces it.
    pub(super) fn load_catalog_cache(&mut self, cx: &mut Context<Self>) {
        let cache = crate::catalog_cache::load();
        if let Some(models) = cache.models.as_ref() {
            self.apply_models_payload(models);
        }
        if let Some(commands) = self
            .current_workspace
            .as_deref()
            .and_then(|cwd| cache.commands_for(cwd))
        {
            self.apply_commands_payload(commands);
        }
        if let Some(state) = cache.state.as_ref() {
            self.apply_reported_model(state);
        }
        self.paint_unsent_model();
        self.sync_model_selector(cx);
    }

    /// Refresh that cache off-thread with a short-lived auxiliary pi: ask for
    /// the catalog and commands, store them, and let the child go. Not a
    /// session process, so it is outside the send-only rule (D18). Skipped
    /// when the cache is fresh for this workspace or a real process exists.
    pub(super) fn start_catalog_probe(&mut self, cx: &mut Context<Self>) {
        if self.client.is_some() || self.spawn_pending() || self.catalog_probing {
            return;
        }
        let cwd = self.task_cwd();
        if crate::catalog_cache::is_fresh_for(&cwd) {
            return;
        }
        // A probe that just failed is not retried on every chip click: the
        // reason it failed (no pi, no node, a timeout) outlives the click.
        if let Some(failed) = self.catalog_probe_failed_at {
            if failed.elapsed() < CATALOG_PROBE_RETRY {
                return;
            }
        }
        self.catalog_probing = true;
        let extensions = self.extensions.clone();
        let env = self.mcp.secret_env();
        #[cfg(test)]
        let test_bin = self.spawn_bin.clone();
        #[cfg(not(test))]
        let test_bin: Option<PathBuf> = None;
        let probe_cwd = cwd.clone();
        // Read before the child boots: the catalog it answers with is built
        // from these files as they are now, and that is what makes the cached
        // copy trustworthy until one of them moves again.
        let sources_stamp = crate::catalog_cache::sources_stamp();
        cx.spawn(async move |this, cx| {
            let probed = cx
                .background_executor()
                .spawn(async move { crate::catalog_cache::probe(&extensions, &probe_cwd, &env, test_bin) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.catalog_probing = false;
                // A process that came up meanwhile is the authority and has
                // already reported its own catalog.
                let Ok((models, commands, state)) = probed else {
                    app.catalog_probe_failed_at = Some(Instant::now());
                    return;
                };
                app.catalog_probe_failed_at = None;
                if app.client.is_none() {
                    crate::catalog_cache::store_models_at(&models, sources_stamp);
                    crate::catalog_cache::store_commands(&cwd, &commands);
                    app.apply_models_payload(&models);
                    if app.current_workspace.as_deref() == Some(cwd.as_path()) {
                        app.apply_commands_payload(&commands);
                    }
                    if let Some(state) = state.as_ref() {
                        crate::catalog_cache::store_state(state);
                        if !app.spawn_pending() {
                            app.apply_reported_model(state);
                        }
                    }
                    app.paint_unsent_model();
                    app.sync_model_selector(cx);
                    app.sync_autocomplete(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// Whether the active pi process is alive and has not exited — the only
    /// state in which commands sent over its stdin can still be answered. A
    /// never-started, stopped, or dead process all read as not live.
    pub(super) fn runtime_is_live(&self) -> bool {
        matches!(self.runtime_state(), RuntimeState::Running)
    }

    /// Spawn the pi process (Start button). No-op while one is running, and
    /// while a background spawn (New Task) is still on its way — that spawn
    /// already owns the launch, and a second process would only be dropped.
    pub(super) fn runtime_start(&mut self, cx: &mut Context<Self>) {
        if self.client.is_some() || self.spawn_pending() {
            return;
        }
        let cwd = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        // An unsent draft may already name its model. A promoted row is a
        // real session: its file, not the pre-send choice, is what a restart
        // must come back to.
        let chosen = self
            .draft_awaits_first_process()
            .then(|| self.current_draft_model())
            .flatten();
        if chosen.is_some() {
            self.default_model_armed = false;
            self.startup_model_pinned = false;
        }
        let secret_env = self.mcp.secret_env();
        // Read before the spawn: the child's registry comes from these files
        // as they are now, and a later edit is not something it can report.
        let sources_stamp = crate::catalog_cache::sources_stamp();
        let resume_path = self
            .current_session_path
            .clone()
            .filter(|path| !super::session::is_draft_placeholder(path));
        let spawned = if let Some(path) = &resume_path {
            let args = vec![
                "--session".to_string(),
                path.to_string_lossy().into_owned(),
            ];
            self.extensions
                .spawn_with_startup_args(&cwd, &args, &secret_env)
        } else {
            match &chosen {
                Some(choice) => {
                    let args = choice.cli_args();
                    self.extensions
                        .spawn_with_startup_args(&cwd, &args, &secret_env)
                }
                None => self.extensions.spawn(&cwd, true, &secret_env),
            }
        };
        match spawned {
            Ok(client) => {
                self.adopt_client(client);
                self.startup_model_reportable =
                    resume_path.is_none() && chosen.is_none() && self.session_default.is_unset();
                // A resume boots into its file's model; anything else boots
                // into the argv model, which pi resolves as a fuzzy pattern
                // and the first `get_state` therefore has to check.
                self.startup_model_expected = resume_path
                    .is_none()
                    .then(|| chosen.clone().unwrap_or_else(|| self.session_default.clone()))
                    .filter(|slot| !slot.is_unset());
                self.startup_model_fix = None;
                self.process_sources_stamp = sources_stamp;
                self.mcp_stamp = self.mcp.fingerprint_for(Some(&cwd));
                if resume_path.is_some() {
                    let _ = self.begin_resumed_session(cx);
                } else {
                    self.send(CommandBody::GetState, "get_state");
                    self.refresh_catalogs();
                    // Capability probes queue after the state request.
                    self.probe_auth();
                }
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
        // A process still starting is stopped the same way: cancel the spawn so
        // its client is dropped when it lands instead of adopted a moment after
        // the user asked for it to stop, and hand a waiting message back.
        let was_starting = self.spawn_pending();
        self.invalidate_pending_spawn();
        // A compaction the cancelled start was going to run is not owed any
        // more; the button offers it again once a process is up.
        self.pending_compact = None;
        if let Some(prompt) = self.take_detached_send_here() {
            self.pending_prompt = Some(prompt);
        }
        if self.client.is_none() {
            if was_starting || self.pending_prompt.is_some() {
                self.unwind_pending_prompt(cx);
                self.set_status(tr!("runtime.pi_process_stopped"));
                cx.notify();
            }
            return;
        }
        self.cancel_fork_followup();
        self.drop_client();
        self.busy = false;
        // A resumed session's message that was waiting on this process never
        // reached pi: it goes back to the composer.
        if self.pending_prompt.is_some() {
            self.unwind_pending_prompt(cx);
        }
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
        // `runtime_start` opens the current file with `--session` when there
        // is one, and loads it. A second `switch_session` would aim at a
        // process that is already in that file.
        self.runtime_start(cx);
    }

    /// Start a fresh process on the session the UI is showing, quietly, after
    /// the previous one was dropped.
    ///
    /// This is the shape for "that process cannot serve this session any more":
    /// it spawns without the startup toast (the caller has its own line to say
    /// what happened) and re-attaches by path, so the conversation survives the
    /// process instead of landing in an empty new session. The caller drops the
    /// old client first — the forced stop does, the MCP apply does.
    ///
    /// Returns whether a process came back: a session pi never claimed has
    /// nothing to re-attach, and a failed spawn leaves its banner to explain.
    pub(super) fn respawn_into_session(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(session_path) = self.current_session_path.clone() else {
            return false;
        };
        let workspace = self.workspace_dir();
        let args = vec![
            "--session".to_string(),
            session_path.to_string_lossy().into_owned(),
        ];
        // Read before the spawn: the child's catalog is the one on disk now.
        let sources_stamp = crate::catalog_cache::sources_stamp();
        match self
            .extensions
            .spawn_with_startup_args(&workspace, &args, &self.mcp.secret_env())
        {
            Ok(client) => {
                self.adopt_client(client);
                self.mcp_stamp = self.mcp.fingerprint_for(Some(&workspace));
                self.process_sources_stamp = sources_stamp;
                let _ = self.begin_resumed_session(cx);
                cx.notify();
                true
            }
            Err(err) => {
                let message = tr!("runtime.pi_spawn_failed", error = err);
                self.client = None;
                self.runtime.error = Some(message.clone());
                self.toast_error(message);
                cx.notify();
                false
            }
        }
    }

    /// Handle the active process exiting. `interrupted` is whether it died
    /// with work in flight (a run, a compaction), sampled before the exit
    /// handler reset that state.
    ///
    /// Orbit never respawns on its own (INTENT D18): a process only starts when
    /// the user sends. An idle exit therefore settles quietly into the cold
    /// state — the session is intact on disk and the next send resumes it. An
    /// exit mid-run lost something, so it raises the banner that points at the
    /// Reconnect button (issue #76).
    pub(super) fn on_process_exited(&mut self, interrupted: bool, cx: &mut Context<Self>) {
        if interrupted {
            self.set_error(self.process_exited_banner());
            return;
        }
        self.drop_client();
        cx.notify();
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
            // Bind only the draft the user is looking at. Startup, clone, and
            // fork also adopt a file; they have no current draft (or a fork
            // is in flight) and must not steal this path onto a blank row.
            if let Some(id) = self.current_draft_id {
                if !self.fork_in_flight() && !self.fork_hold_blocks() {
                    self.bind_draft_path(id, &file);
                }
            }
            self.current_session_path = Some(file);
            self.stamp_held_fork_title();
            self.release_promoted_drafts_on_disk();
        }
    }

    /// Park a background session. The pool has no cap and no idle TTL: a
    /// parked process leaves `lives` only when its session is deleted, the
    /// user stops it, or the process itself exits (see the parked drain in
    /// `events.rs`). An MCP edit does not drop it.
    pub(super) fn park(&mut self, path: PathBuf, parked: ParkedSession) {
        self.lives.insert(path, parked);
    }
}

/// The `(provider, id)` a `get_state` body names, when it names a model
/// completely. A reply missing either half is treated as *no report* rather
/// than as a mismatch: it cannot be compared with the request, and it gives
/// `set_model` (which matches provider *and* id) nothing exact to ask for.
fn reported_model_pair(data: &serde_json::Value) -> Option<(String, String)> {
    let model = data.get("model").filter(|model| model.is_object())?;
    let text = |key: &str| {
        model
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some((text("provider")?, text("id")?))
}

/// Whether pi's reported model is the one the argv asked for. Compared
/// case-insensitively because pi's own CLI resolution lowercases ids: a
/// correction that differed only in case would be refused by the exact RPC
/// anyway, so accepting pi's answer beats fighting it.
fn same_model(reported: &(String, String), expected: &(String, String)) -> bool {
    reported.0.eq_ignore_ascii_case(&expected.0)
        && reported.1.eq_ignore_ascii_case(&expected.1)
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

/// One background spawn, carried from the request to the frame its process
/// lands on (`OrbitApp::spawn_pi_client_later` → `admit_spawned_client`).
///
/// `generation` is the claim on the view: it must still be the app's current
/// one, or the result is dropped. `mcp_stamp` is the config the child was
/// actually handed, read at spawn time rather than when it lands. `then` is
/// what the caller wants done with an empty process.
#[derive(Clone)]
pub(super) struct SpawnTicket {
    pub(super) generation: u64,
    pub(super) cwd: PathBuf,
    pub(super) mcp_stamp: u64,
    pub(super) then: SpawnThen,
    /// `--provider` / `--model` / `--thinking` the child is started with.
    /// A resume carries `--session <file>` instead, so the process boots
    /// already in that file.
    startup_args: Vec<String>,
    /// `startup_args` came from the open draft, not the session default.
    pub(super) pinned_model: bool,
    /// The model `startup_args` asks pi to boot on. pi resolves `--model` as
    /// a fuzzy pattern, so the process can come up on a different model;
    /// [`OrbitApp::reconcile_startup_model`] checks the first `get_state`
    /// against this and asks once for the exact one.
    pub(super) requested_model: Option<crate::session_defaults::SessionDefault>,
    /// `catalog_cache::sources_stamp()` when the spawn was requested: the
    /// catalog this child reports is the one it read at boot.
    pub(super) sources_stamp: u64,
    /// The session file a [`SpawnThen::ResumePrompt`] process attaches to.
    pub(super) session_path: Option<PathBuf>,
}

/// A send that left the screen before its process existed. The ticket still
/// matches the child; the shell is the transcript and prompt that were on
/// screen when the user switched away.
pub(super) struct DetachedSpawn {
    pub(super) ticket: SpawnTicket,
    pub(super) transcript: crate::transcript::Transcript,
    pub(super) pending_prompt: Option<super::PendingPrompt>,
    pub(super) execution: super::SessionExecution,
    pub(super) busy: bool,
    pub(super) draft: Option<u64>,
    pub(super) discard: bool,
    pub(super) startup_model_pinned: bool,
    pub(super) session_path: Option<PathBuf>,
    /// The open turn clock and edit totals ride with the send, like a normal
    /// park, so a reattach does not restart the turn or lose the checkpoint.
    pub(super) turns: crate::turn_review::TurnClock,
    pub(super) added: u64,
    pub(super) removed: u64,
}

/// How long a failed auxiliary catalog probe suppresses the next one. Long
/// enough that opening the model picker cannot spawn a pi per click while the
/// reason stands, short enough that a fixed install is picked up without a
/// relaunch.
const CATALOG_PROBE_RETRY: Duration = Duration::from_secs(60);

impl SpawnTicket {
    /// The startup flags this child was given.
    pub(super) fn startup_args(&self) -> &[String] {
        &self.startup_args
    }
}

/// What a spawned process is for, once it is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpawnThen {
    /// Deliver the message that started it (a New Task's first send).
    Prompt,
    /// Ask pi for a new session on it (the Settings credential/catalog restart).
    NewSession,
    /// Boot it with `--session <file>`, load that file with `get_messages`,
    /// then deliver the message that started it. This is how a session with no
    /// process (opened from the sidebar, or left cold) comes back: only a send
    /// spawns, and the process resumes the file instead of starting a new one.
    ResumePrompt,
}
