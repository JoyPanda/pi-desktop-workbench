use super::*;
use crate::app::sidebar::{clear_review_mark, should_mark_review};
use crate::toast::ToastKind;

/// One heartbeat may spend about this long applying protocol events. The
/// clock is checked between events, so a single huge payload still runs, and
/// whatever is left stays queued.
const APPLY_BUDGET: Duration = Duration::from_millis(4);

/// Streaming rows and the Working clock commit on this cadence. Tool and
/// message boundaries paint immediately and are not waiting on it.
const STREAM_COMMIT: Duration = Duration::from_millis(120);

/// Text, thinking, tool-argument, and tool-output deltas. Start, end, and
/// message boundaries still paint on this frame.
fn is_stream_delta(event: &Event) -> bool {
    match event {
        Event::MessageUpdate { assistant, .. } => matches!(
            assistant,
            Some(
                orbit_rpc::AssistantMessageEvent::TextDelta { .. }
                    | orbit_rpc::AssistantMessageEvent::ThinkingDelta { .. }
                    | orbit_rpc::AssistantMessageEvent::ToolcallDelta { .. }
            )
        ),
        Event::ToolExecutionUpdate { .. } => true,
        _ => false,
    }
}

/// A boundary the cadence must not swallow. The last deferred row and the
/// changed-files card commit on the same frame the boundary is applied.
fn forces_visual_commit(event: &Event) -> bool {
    match event {
        Event::MessageEnd { .. }
        | Event::ToolExecutionStart { .. }
        | Event::ToolExecutionEnd { .. }
        | Event::AgentSettled { .. } => true,
        Event::MessageUpdate { assistant, .. } => matches!(
            assistant,
            Some(orbit_rpc::AssistantMessageEvent::ToolcallEnd { .. })
        ),
        _ => false,
    }
}

/// Newest `sessionFile` already sitting in an app-level queue. Does not
/// remove or reorder the events.
pub(super) fn newest_session_file(
    events: &std::collections::VecDeque<Event>,
) -> Option<PathBuf> {
    events.iter().rev().find_map(|event| {
        let Event::Response { success, data, .. } = event else {
            return None;
        };
        if !success {
            return None;
        }
        data.as_ref()
            .and_then(|data| data.get("sessionFile"))
            .and_then(Value::as_str)
            .map(PathBuf::from)
    })
}

/// pi reports `cancelled: true` when an extension refused the fork or clone.
fn response_cancelled(data: Option<&serde_json::Value>) -> bool {
    data.and_then(|data| data.get("cancelled"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// One tick's worth of results from draining a background session.
struct ParkedDrain {
    /// An event changed the parked state — repaint.
    changed: bool,
    /// The process named its session file; move it from the pending pool into
    /// `lives` under this path.
    rekey: Option<PathBuf>,
    /// Unsent post-X queue text taken when the parked run settled (or a
    /// prompt was handled without a run). Stashed onto the session draft.
    restore: Option<String>,
    /// The resume handshake failed. The composer goes back to this session,
    /// and the process is dropped.
    failed_send: Option<ComposerDraft>,
    /// Staged model / thinking / rename replies from this process.
    staged: Vec<(String, bool)>,
}

fn take_restore(slot: &mut Option<String>, next: Option<String>) {
    let Some(next) = next else {
        return;
    };
    *slot = Some(match slot.take() {
        Some(existing) => format!("{existing}\n\n{next}"),
        None => next,
    });
}

impl OrbitApp {
    /// Heartbeat (~90ms): drain protocol events into the UI. Returns whether a
    /// run is in flight so the caller can repaint faster while tokens stream.
    pub(crate) fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(generation) = self.pending_turn_records.take() {
            if generation == self.session_generation {
                self.push_turn_records(cx);
            }
        }
        // Let a lapsed status message disappear from the status bar.
        if self
            .status_at
            .is_some_and(|at| at.elapsed() >= STATUS_MESSAGE_TTL)
        {
            self.status_at = None;
            cx.notify();
        }
        // Retire toasts past their TTL (plus the fade-out, when animated).
        let toast_fade = if theme::reduce_motion(cx) {
            Duration::ZERO
        } else {
            crate::toast::FADE
        };
        if self.toasts.expire(Instant::now(), toast_fade) {
            cx.notify();
        }
        // The new-task page's dithered field is alive: advance its clock and
        // repaint. Only that page pays for it, and Reduce Motion freezes it.
        if self.advance_backdrop(theme::reduce_motion(cx)) {
            cx.notify();
        }
        // Session retention: once per launch, archive or delete sessions the
        // user's policy marks idle. A no-op until enabled in Settings.
        self.run_session_retention_once(cx);
        self.tick_background(cx);
        // MCP: external-change detection, the debounced `pi mcp list` probe,
        // and a deferred restart that applies new server configuration while
        // preserving the session.
        self.tick_mcp(cx);
        // Persist a settled panel layout (a drag writes once it stops).
        crate::layout::flush_if_settled();
        // A banner click routes back to the session it announced.
        if let Some(path) = notifications::take_clicked_session() {
            self.activate_session_from_notification(PathBuf::from(path), cx);
        }
        // Background update checks and the install handoff report here.
        self.drain_updater_events(cx);
        // Sessions can be written by the CLI or another Orbit window; the
        // watcher has already scanned off-thread, so this only swaps the list.
        if let Some(reloaded) = self
            .session_watcher
            .as_ref()
            .and_then(sessions::SessionWatcher::take_reload)
        {
            // A reload that changed nothing (e.g. a bookkeeping append the
            // scan already reflected) must not rebuild the sidebar or repaint.
            if reloaded != self.sessions {
                self.sessions = reloaded;
                self.release_promoted_drafts_on_disk();
                self.prune_workflow_store();
                self.sync_session_menu(cx);
                // The usage index is built from the same files; a write means
                // the analytics are stale (rate-limited inside the page).
                if self.usage_open {
                    self.usage.update(cx, |page, cx| page.mark_stale(cx));
                }
                cx.notify();
            }
        }
        // Usage: drain any finished scan and recompute if the filter or the
        // index moved. Cheap when nothing changed.
        self.usage.update(cx, |page, cx| page.sync(cx));
        // Workspace edits (pi, the user, or git) have no RPC event either;
        // refresh Review, the Git page, and the branch chip when the tree
        // changes.
        self.sync_workspace_watcher(cx);
        // An added or removed project needs its branch for the sidebar row's
        // metadata line; a compare of the project list, so it is free while
        // nothing moved.
        self.sync_sidebar_branches(cx);
        // A review mark is spent the moment its session is really in front of
        // the user — by click, by shortcut, by the window coming forward, or by
        // a full-page surface closing. One rule on the heartbeat instead of a
        // hook on each of those paths, so they cannot drift apart, and a
        // selection made while Orbit is behind another app deliberately does
        // not clear it (issue #83).
        let in_front = self.transcript_in_front(cx);
        if clear_review_mark(
            &mut self.awaiting_review,
            self.current_session_path.as_deref(),
            in_front,
        ) {
            cx.notify();
        }
        let workspace_dirty = self
            .workspace_watcher
            .as_ref()
            .is_some_and(watch::WorkspaceWatcher::take_dirty);
        if workspace_dirty {
            // Last Turn is the conversation's edit/write calls. A file save
            // must not rebuild it or throw away the reader's scroll position.
            self.sidepane
                .update(cx, |pane, cx| pane.mark_git_review_stale(cx));
            self.git_panel.update(cx, |panel, cx| panel.refresh(cx));
            // One index feeds the tree and `@`; the watcher throttle decides
            // when it actually re-walks.
            self.invalidate_index(false, cx);
            if let Some(path) = self.file_viewer.read(cx).active_path() {
                self.file_viewer
                    .update(cx, |viewer, cx| viewer.reload(&path, cx));
            }
            self.refresh_branch_status(cx);
            // External `git worktree add`/`remove` lands as a workspace
            // change; re-list the page so Git's own worktrees appear (§30).
            self.refresh_worktrees_soon(cx);
            cx.notify();
        }
        // Expire a stalled login and auto-cancel it with pi.
        let had_login = self.auth.login().is_some();
        let auth_effects = self.auth.poll(Instant::now());
        self.handle_auth_effects(auth_effects, cx);
        if had_login || self.auth.login().is_some() {
            cx.notify();
        }
        // Keep the Runtime panel's liveness fresh (try_wait is cheap).
        if let Some(client) = self.client.as_mut() {
            self.runtime.alive = client.is_alive();
        }
        // A stop pi has not answered stops being a request and becomes an
        // action. Runs before the event drain on purpose: while pi is stuck it
        // sends no events at all, which is exactly the tick that needs it.
        self.check_stalled_stop(cx);
        // Images pasted in the composer become message attachments.
        self.drain_pasted_images(cx);
        // Long pastes become collapsible pills above the composer.
        self.drain_pasted_texts(cx);
        // The placeholder names what Enter will do right now.
        self.sync_composer_placeholder(cx);
        // Refresh the in-transcript find hits when the query or messages moved.
        self.sync_transcript_search(cx);
        // A drag that left the window clears gpui's active drag without any
        // element event seeing it — the heartbeat drops a stale highlight.
        if self.file_drag_hovered && !cx.has_active_drag() {
            self.file_drag_hovered = false;
            cx.notify();
        }
        if self.transcript.flush_dirty_rows_limited(4) {
            cx.notify();
        }
        if self.client.is_none() {
            let deadline = Instant::now() + APPLY_BUDGET;
            let mut applied = 0;
            let mut repaint = self.apply_parked_budget(deadline, &mut applied, cx);
            repaint |= self.flush_visual_commit(false);
            if repaint {
                cx.notify();
            }
            return self.is_active() || self.background_is_active();
        }
        self.foreground_events
            .extend(self.client.as_ref().expect("client").drain_events());
        let deadline = Instant::now() + APPLY_BUDGET;
        let mut repaint = false;
        // The bridge writes snapshots into the session file, not the event
        // stream; poll for new entries on a slow clock independent of events.
        self.poll_quota_entries();
        let copy_pending = self.transcript.prune_copy_feedback();
        // The one-time rail hint dismisses itself once its TTL lapses.
        if self.transcript.rail_hint_timed_out() {
            cx.notify();
        }
        repaint |= copy_pending;
        let mut refresh_sessions = false;
        let mut applied = 0usize;
        let mut force_visual = false;
        while !self.foreground_events.is_empty() {
            if self.budget_spent(deadline, applied) {
                break;
            }
            let owned = self
                .foreground_events
                .pop_front()
                .expect("foreground queue");
            let event = &owned;
            let quiet = is_stream_delta(event);
            force_visual |= forces_visual_commit(event);
            self.execution.apply_event(event);
            if matches!(event, Event::TurnStart | Event::AgentSettled { .. }) {
                self.dialog = None;
                self.dialog_focus_pending = false;
                self.approval = None;
                self.approval_focus_pending = false;
            }
            match event {
                Event::AgentStart => {
                    self.busy = true;
                }
                // An extension dialog blocks the run until the client answers.
                // Render it natively (select / confirm / input / editor); the
                // user's reply on the modal unblocks pi.
                Event::ExtensionUiRequest { id, method, value } => {
                    self.handle_extension_ui_request(id.clone(), method, value, cx);
                    // A dialog blocks the run until answered — the one state a
                    // user who walked away cannot otherwise discover.
                    self.notify_input_needed(method, value);
                    repaint = true;
                }
                Event::SessionInfoChanged { name } => {
                    // pi renamed the session (its own auto-title, the popover's
                    // Generate title, or the rename field echoing back).
                    // `session_name` wins over `current_title` in the header,
                    // so both move together or a re-title would not show.
                    // A footer fork holds its `(N)` title until `set_session_name`
                    // answers. pi runs that command beside the `get_state`
                    // queued first, so the echo can arrive before a report
                    // that still carries the name copied from the source.
                    if self.name_report_held() {
                        self.title_generating = false;
                        refresh_sessions = true;
                    } else {
                        match name {
                            Some(name) => {
                                self.current_title = Some(name.clone());
                                // Seed the rename field from the new name, unless
                                // the user is mid-edit; a manual generation owns
                                // the field it asked to refresh.
                                let seed = self.session_name.is_none() || self.title_generating;
                                self.session_name = Some(name.clone());
                                if seed {
                                    self.session_name_input
                                        .update(cx, |input, cx| input.set_text(name, cx));
                                }
                            }
                            None => self.session_name = None,
                        }
                        self.title_generating = false;
                        refresh_sessions = true;
                    }
                }
                Event::Auth(event) => {
                    let effects = self.auth.on_event(event.clone());
                    self.handle_auth_effects(effects, cx);
                }
                Event::AutoRetryStart { value } => {
                    // A transient provider error is not a user-facing failure:
                    // it rides the quiet run-status strip (with the attempt
                    // counter) until the retry resolves — never the banner.
                    self.retrying = true;
                    self.retry_detail = Some(RetryDetail {
                        attempt: value.get("attempt").and_then(Value::as_u64).unwrap_or(1),
                        max: value.get("maxAttempts").and_then(Value::as_u64),
                        error: value
                            .get("errorMessage")
                            .and_then(Value::as_str)
                            .unwrap_or(&tr!("events.transient_error"))
                            .to_string(),
                    });
                }
                Event::AutoRetryEnd { value } => {
                    self.retrying = false;
                    self.retry_detail = None;
                    let cancelled = std::mem::replace(&mut self.abort_requested, false);
                    let success = value.get("success").and_then(|v| v.as_bool());
                    // A retry the user stopped is a cancel, not a failed retry:
                    // pi answers the abort with `success: false` and
                    // `finalError: "Retry cancelled"`, and raising a banner for
                    // it would report a failure the user asked for (issue #76).
                    if success == Some(false) && !cancelled {
                        let error = value
                            .get("finalError")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| tr!("events.retries_exhausted"));
                        self.set_error(tr!("events.auto_retry_failed", error = error));
                    }
                }
                Event::QueueUpdate { value } => {
                    self.apply_queue_update(value);
                }
                Event::ExtensionError { value } => {
                    let path = value
                        .get("extensionPath")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let name = Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .filter(|n| !n.is_empty())
                        .unwrap_or("extension");
                    let hook = value
                        .get("event")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| tr!("events.unknown"));
                    let error = value
                        .get("error")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| tr!("events.unknown_error"));
                    self.set_error(tr!(
                        "events.extension_failed",
                        name = name,
                        hook = hook,
                        error = error
                    ));
                    // A failed title command never sends its
                    // `session_info_changed`, so drop the in-flight flag here.
                    self.title_generating = false;
                }
                Event::AgentSettled { aborted } => {
                    self.abort_requested = false;
                    self.abort_sent_at = None;
                    self.busy = false;
                    self.retrying = false;
                    self.retry_detail = None;
                    // A settled run has no queued continuation left; clear the
                    // bar even if the final `queue_update` was missed. Keep
                    // wire counters: a `clear_queue` sent just before settle
                    // is still on this process. A post-X message that never
                    // left Orbit is restored, not dropped.
                    self.settle_local_queue(cx);
                    refresh_sessions = true;
                    self.refresh_context_stats();
                    #[cfg(debug_assertions)]
                    self.log_turn_diagnostics();
                    // The bridge appends its post-turn snapshot at settle;
                    // read it now instead of waiting for the slow poll.
                    self.quota_entries_next_poll = Instant::now();
                    self.poll_quota_entries();
                    // Close the turn on this frame. The Git checkpoint is
                    // best-effort and does not gate Last Turn's recorded edits.
                    self.finish_turn(cx);
                    // An MCP configuration change deferred during the run
                    // applies now, with the session preserved.
                    self.mcp_after_settle(cx);
                    // The run ended: announce it if the user is elsewhere.
                    let path = self.current_session_path.clone();
                    let title = self
                        .current_title
                        .clone()
                        .or_else(|| self.session_name.clone());
                    let summary = self.transcript.latest_turn_summary();
                    if !aborted {
                        self.notify_turn_finished(
                            path.as_deref(),
                            title.as_deref(),
                            summary.as_ref(),
                            cx,
                        );
                    }
                }
                Event::CompactionStart { .. } => {
                    // The run-status strip carries the in-progress state;
                    // no transient status line that would lapse mid-compact.
                    self.is_compacting = true;
                }
                Event::CompactionEnd { value } => {
                    self.is_compacting = false;
                    let aborted = value.get("aborted").and_then(Value::as_bool) == Some(true);
                    // A failed compaction carries `errorMessage` (docs). pi
                    // emits this event *and* rethrows for the failed `compact`
                    // response; its two "nothing to do" answers belong to that
                    // response's single notice, not to a red banner raised
                    // here first and never retired.
                    let benign = value
                        .get("errorMessage")
                        .and_then(Value::as_str)
                        .and_then(Self::benign_compaction_message)
                        .is_some();
                    if let Some(error) = value
                        .get("errorMessage")
                        .and_then(Value::as_str)
                        .filter(|_| !benign)
                    {
                        self.set_error(tr!("events.compaction_failed", error = error));
                    } else if benign {
                        // The response arm owns the notice.
                    } else if aborted {
                        self.set_status(tr!("events.compaction_aborted"));
                    } else {
                        // Success: pi replaced the summarized span with a
                        // compaction summary plus the kept entries. Re-read
                        // the projection so the boundary card lands in the
                        // transcript (and pre-compaction rows drop).
                        self.send(CommandBody::GetMessages, "get_messages");
                    }
                    // Post-compaction usage is unknown until the next turn;
                    // refresh so the meter can show an empty/unknown state.
                    self.refresh_context_stats();
                }
                Event::ProcessExited => {
                    // Sampled first: the resets below clear the very flags that
                    // say whether the process died with work in flight.
                    let interrupted = self.is_running() || self.is_compacting || self.retrying;
                    // The rename can no longer answer, and a held title would
                    // ignore the next session's name reports. The same goes for
                    // a replacement: the process that owed the answer is gone,
                    // so its gate and any click queued behind it are dropped
                    // (a click on a dead process starts a new one instead).
                    self.cancel_fork_followup();
                    self.replacement_in_flight = None;
                    self.pending_switch = None;
                    // `new_session` will not answer. A pin or an armed default
                    // for that birth must not follow the next process.
                    self.startup_model_pinned = false;
                    self.default_model_armed = false;
                    // A startup-model correction died with it: the chips must
                    // not stay held on a model no process is running.
                    self.startup_model_expected = None;
                    self.startup_model_fix = None;
                    // A compaction it was going to run died with it too.
                    self.pending_compact = None;
                    self.pending_session_rename = None;
                    self.session_rename_ready = false;
                    self.busy = false;
                    self.retrying = false;
                    self.retry_detail = None;
                    self.reset_queue();
                    // The run is gone; its dialog (if any) can never be
                    // answered, so drop it without a reply.
                    self.dialog = None;
                    self.dialog_focus_pending = false;
                    self.approval = None;
                    self.approval_focus_pending = false;
                    // Every custom surface belonged to that process too: the
                    // promise behind it can no longer resolve, so a form left on
                    // screen would swallow keystrokes forever. Compaction and the
                    // title command also die with the process, and `is_compacting`
                    // alone keeps `is_running()` true — the composer would stay on
                    // Stop over a dead pi.
                    self.custom_ui.clear();
                    self.custom_ui_focus_pending = false;
                    self.is_compacting = false;
                    self.title_generating = false;
                    self.runtime.alive = false;
                    self.runtime.exited = true;
                    self.abort_requested = false;
                    self.abort_sent_at = None;
                    self.auth.on_disconnect();
                    // An idle exit goes cold quietly; one mid-run raises the
                    // banner — see `on_process_exited`.
                    self.on_process_exited(interrupted, cx);
                }
                Event::MessageEnd { value } => {
                    // A failed LLM call ends the assistant message with
                    // `stopReason: "error"` + `errorMessage` (e.g. an
                    // unsupported model). Surface it in the composer banner
                    // only — a second red card in the transcript duplicates
                    // the same prompt.
                    if let Some(error) = transcript::message_error(value) {
                        self.set_error_from(
                            AGENT_ERROR_SOURCE,
                            tr!("events.agent_error", error = error),
                        );
                    } else {
                        // The next attempt produced a message — clear the
                        // stale agent-error banner. Matched by source, so a
                        // language change never strands it (issue #76).
                        self.clear_error_from(AGENT_ERROR_SOURCE);
                    }
                    // Real edit stats from finalized tool calls.
                    let (a, r) = transcript::diff_from_message(value);
                    self.added += a;
                    self.removed += r;
                }
                Event::Response {
                    command,
                    success,
                    data,
                    error,
                    id,
                } => {
                    self.on_response(
                        command,
                        *success,
                        data.as_ref(),
                        error.as_deref(),
                        id,
                        &mut refresh_sessions,
                        cx,
                    );
                }
                // A running `ask_user_question` tool is tracked so its
                // `select` / `input` primitives route to the inline panel.
                Event::ToolExecutionStart { value } => {
                    self.on_ask_tool_start(value, cx);
                }
                Event::ToolExecutionEnd { value } => {
                    self.on_ask_tool_end(value, cx);
                }
                _ => {}
            }
            self.transcript.apply_event(event);
            if !quiet {
                repaint = true;
            }
            applied += 1;
        }
        self.busy = self.execution.status().is_in_progress();
        if let Some(path) = &self.current_session_path {
            self.remember_session_outcome(path.clone(), self.execution.status());
        }
        if refresh_sessions {
            self.sessions = sessions::load_sessions();
            self.release_promoted_drafts_on_disk();
            self.prune_workflow_store();
            self.sync_session_menu(cx);
            repaint = true;
        }
        // The session on screen spent this tick's slice. Parked sessions
        // share a slice of their own, so a busy foreground cannot starve them
        // and a click does not inherit the whole backlog.
        let parked_deadline = Instant::now() + APPLY_BUDGET;
        repaint |= self.apply_parked_budget(parked_deadline, &mut applied, cx);
        repaint |= self.flush_visual_commit(force_visual);
        if repaint {
            cx.notify();
        }
        self.is_active() || self.background_is_active()
    }

    /// A parked session is mid-run or still booting. The heartbeat keeps its fast
    /// cadence for it too: at the idle cadence the 4 ms apply budget cannot keep
    /// up with a streaming run, and the backlog is what a later switch to that
    /// session would have to chew through on the click.
    fn background_is_active(&self) -> bool {
        self.lives.values().any(|parked| parked.busy) || !self.pending_parks.is_empty()
    }

    /// The time budget, or the test cap, is spent. Checked between events so
    /// a boundary stays behind the deltas already queued in front of it.
    fn queued_apply_limit(&self) -> Option<usize> {
        #[cfg(test)]
        {
            self.test_apply_limit
        }
        #[cfg(not(test))]
        {
            None
        }
    }

    fn budget_spent(&self, deadline: Instant, applied: usize) -> bool {
        if Instant::now() >= deadline {
            return true;
        }
        #[cfg(test)]
        if self.test_apply_limit.is_some_and(|limit| applied >= limit) {
            return true;
        }
        #[cfg(not(test))]
        let _ = applied;
        false
    }

    /// Move the Working clock and commit a deferred streaming row or review
    /// card. Quiet deltas wait for the cadence. A message end, a tool
    /// boundary, or a settle applied this tick commits on this frame, so the
    /// last height and the changed-files card are not left for the next beat.
    fn flush_visual_commit(&mut self, force: bool) -> bool {
        if !force && self.last_visual_commit.elapsed() < STREAM_COMMIT {
            return false;
        }
        let streamed = self.transcript.commit_pending_stream_row();
        let reviewed = self.transcript.flush_pending_review();
        let clock =
            self.busy || self.transcript.is_streaming() || self.retrying || self.is_compacting;
        if streamed || reviewed || clock {
            self.last_visual_commit = Instant::now();
            return true;
        }
        false
    }

    /// Step the new-task backdrop's field. Returns whether anything changed and
    /// the window should repaint.
    ///
    /// The phase is wall-clock seconds, not a tick counter: the heartbeat's
    /// cadence changes with the run state, and the field must not speed up
    /// while something else streams. The clock only runs while the page is
    /// actually on screen (`render` records that), so a backdrop nobody can see
    /// costs nothing. Reduce Motion holds the frame it was showing — a halftone
    /// is a perfectly good still.
    fn advance_backdrop(&mut self, reduce_motion: bool) -> bool {
        if !self.backdrop_live || reduce_motion {
            self.backdrop_clock = None;
            return false;
        }
        // Resume from the phase already on screen rather than snapping back to
        // zero: a page the reader returns to keeps the screen it left.
        if self.backdrop_clock.is_none() {
            let resumed = Duration::from_secs_f32(self.backdrop_phase);
            self.backdrop_clock = Some(Instant::now() - resumed);
        }
        let Some(clock) = self.backdrop_clock else {
            return false;
        };
        self.backdrop_phase = clock.elapsed().as_secs_f32();
        true
    }

    /// True while a run is in flight. The heartbeat polls fast (instead of the
    /// idle ~90 ms cadence) when this holds, so streamed tokens land smoothly
    /// rather than in visible batches, and drops back the moment it settles.
    fn is_active(&self) -> bool {
        self.busy || self.transcript.is_streaming() || self.retrying || self.is_compacting
    }

    /// Debug-build-only, local per-turn diagnostic (not telemetry: it never
    /// leaves the process, never touches the network, and is compiled out of
    /// release builds). Prints the context window, this turn's cache hit,
    /// uncached prompt, output, duration, and average rate so a "lower tok/s"
    /// can be attributed to prefill vs. decode without guessing.
    #[cfg(debug_assertions)]
    fn log_turn_diagnostics(&self) {
        let Some(usage) = self.transcript.last_message_usage() else {
            return;
        };
        let window = self.context.as_ref().map(|c| c.context_window).unwrap_or(0);
        let cache_pct = usage
            .cache_read_percent()
            .map(|p| format!("{p:.1}%"))
            .unwrap_or_else(|| "n/a".into());
        let duration = self.transcript.last_message_duration();
        let tps = duration
            .filter(|d| d.as_secs_f64() >= 1.0 && usage.output > 0)
            .map(|d| usage.output as f64 / d.as_secs_f64());
        eprintln!(
            "[orbit:perf] turn settled — window {window} tokens | cache {} ({cache_pct} hit) | prompt {} uncached | output {} | {}ms | {} tok/s avg",
            usage.cache_read,
            usage.input,
            usage.output,
            duration
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|| "?".into()),
            tps.map(|t| format!("{t:.0}")).unwrap_or_else(|| "?".into()),
        );
    }

    /// Local resident-memory line (not telemetry: it never leaves the
    /// process and never touches the network). Printed when the workspace
    /// index lands and when a session snapshot replaces the transcript, so
    /// a rebuild can be compared without a heap profiler. The counter name
    /// is whatever [`crate::platform::process_memory`] actually read
    /// (`private`, `rss`, or `peak`).
    pub(super) fn log_resident(&self, when: &str) {
        let memory = crate::platform::process_memory()
            .map(|sample| {
                format!(
                    "{} {:.1}MB",
                    sample.kind,
                    sample.bytes as f64 / (1024.0 * 1024.0)
                )
            })
            .unwrap_or_else(|| "memory n/a".into());
        let entries = self
            .mention_index
            .as_ref()
            .map(|index| index.len())
            .unwrap_or(0);
        let tools = self.transcript.retained_tool_bytes();
        let parked = self.lives.len() + self.pending_parks.len();
        eprintln!(
            "[orbit:mem] {when} — {memory} | entries {entries} | tools {tools}B | parked {parked}"
        );
    }

    /// Pull parked protocol events into each session's queue. Applying waits
    /// until the foreground queue has had this tick's budget.
    pub(super) fn tick_background(&mut self, _cx: &mut Context<Self>) {
        for parked in self.lives.values_mut() {
            parked.pending.extend(parked.client.drain_events());
        }
        for parked in &mut self.pending_parks {
            parked.pending.extend(parked.client.drain_events());
        }
    }

    /// Apply queued background events with whatever budget the foreground
    /// left. A parked stream cannot spend the budget before the session on
    /// screen has applied.
    fn apply_parked_budget(
        &mut self,
        deadline: Instant,
        applied: &mut usize,
        cx: &mut Context<Self>,
    ) -> bool {
        for parked in self.lives.values_mut() {
            parked.pending.extend(parked.client.drain_events());
        }
        for parked in &mut self.pending_parks {
            parked.pending.extend(parked.client.drain_events());
        }
        if self.lives.is_empty() && self.pending_parks.is_empty() {
            return false;
        }
        let limit = self.queued_apply_limit();
        let mut changed = false;
        let mut dead: Vec<PathBuf> = Vec::new();
        // Settles to announce once the `lives` borrow (and the map itself)
        // are no longer held.
        let mut finished: Vec<(PathBuf, Option<String>, Option<transcript::TurnSummary>)> =
            Vec::new();
        let mut restores: Vec<(Option<PathBuf>, Option<u64>, String)> = Vec::new();
        let mut failed_sends: Vec<(Option<PathBuf>, Option<u64>, ComposerDraft)> = Vec::new();
        let mut staged_replies: Vec<(String, bool)> = Vec::new();
        let mut live_paths: Vec<PathBuf> = self.lives.keys().cloned().collect();
        live_paths.sort();
        let live_count = live_paths.len();
        let live_start = if live_count == 0 {
            0
        } else {
            self.park_budget_cursor % live_count
        };
        for step in 0..live_count {
            let path = live_paths[(live_start + step) % live_count].clone();
            let Some(parked) = self.lives.get_mut(&path) else {
                continue;
            };
            let drained = Self::drain_parked_session(
                parked,
                Some(&path),
                &self.sessions,
                &mut finished,
                deadline,
                applied,
                limit,
            );
            changed |= drained.changed;
            if let Some(text) = drained.restore {
                restores.push((Some(path.clone()), parked.draft, text));
            }
            if let Some(composer) = drained.failed_send {
                failed_sends.push((Some(path.clone()), parked.draft, composer));
                dead.push(path.clone());
            }
            staged_replies.extend(drained.staged);
            let status = parked.execution.status();
            // Record the outcome, but only when it moved: this runs per parked
            // session on every heartbeat, and re-inserting the same value would
            // dirty the map for nothing.
            if !status.is_in_progress()
                && self.session_outcomes.insert(path.clone(), status) != Some(status)
            {
                changed = true;
            }
            // A dead process can still have settle or rekey events sitting
            // behind this tick's budget. Dropping the park now would lose them.
            if !parked.client.is_alive() && parked.pending.is_empty() {
                // The exit notification may still sit in the channel, undrained,
                // so the ProcessExited arm above has not run: recover a waiting
                // resume prompt here too, or this reap drops it with the park.
                if let Some(prompt) = parked.resume_prompt.take() {
                    failed_sends.push((Some(path.clone()), parked.draft, prompt.composer));
                }
                dead.push(path.clone());
            }
        }
        self.park_budget_cursor = self.park_budget_cursor.wrapping_add(1);
        if !dead.is_empty() {
            changed = true;
        }
        for path in dead {
            self.lives.remove(&path);
        }
        // Pending parks: live processes whose session file pi has not named
        // yet. They drain like any parked session (a run started during boot
        // keeps going) and join `lives` under their file the moment the boot
        // handshake reveals it. A process that never claims a file is dropped
        // only when it exits.
        let mut rekeyed: Vec<(PathBuf, ParkedSession)> = Vec::new();
        let mut ix = 0;
        while ix < self.pending_parks.len() {
            let draft_id = self.pending_parks[ix].draft;
            let drained = Self::drain_parked_session(
                &mut self.pending_parks[ix],
                None,
                &self.sessions,
                &mut finished,
                deadline,
                applied,
                limit,
            );
            changed |= drained.changed;
            if let Some(text) = drained.restore {
                restores.push((drained.rekey.clone(), draft_id, text));
            }
            if let Some(composer) = drained.failed_send {
                failed_sends.push((drained.rekey.clone(), draft_id, composer));
                staged_replies.extend(drained.staged);
                self.pending_parks.remove(ix);
                changed = true;
                continue;
            }
            staged_replies.extend(drained.staged);
            // A rekey learned this tick has to land even when the process
            // already exited or later events are still queued. Reaping first
            // would drop the file name and the leftover backlog with it.
            if let Some(path) = drained.rekey {
                let parked = self.pending_parks.remove(ix);
                rekeyed.push((path, parked));
                changed = true;
            } else if !self.pending_parks[ix].client.is_alive()
                && self.pending_parks[ix].pending.is_empty()
            {
                // Same undrained-exit case as the lives loop: hand back a waiting
                // prompt before the park goes.
                if let Some(prompt) = self.pending_parks[ix].resume_prompt.take() {
                    failed_sends.push((None, draft_id, prompt.composer));
                }
                self.pending_parks.remove(ix);
                changed = true;
            } else {
                ix += 1;
            }
        }
        for (path, parked) in rekeyed {
            self.admit_rekeyed_park(path, parked);
        }
        for (path, draft_id, text) in restores {
            self.stash_parked_queue_restore(path.as_deref(), draft_id, text);
        }
        for (path, draft_id, composer) in failed_sends {
            self.stash_failed_send(path.as_deref(), draft_id, composer);
        }
        for (id, success) in staged_replies {
            if success {
                let _ = self.ack_staged(&id);
            } else {
                self.fail_staged(&id);
            }
        }
        // A parked run that settled while the user was elsewhere still wants
        // saying. The settle also flips the sidebar's running loader, so the
        // final repaint is unconditional.
        for (path, title, summary) in finished {
            self.notify_turn_finished(Some(&path), title.as_deref(), summary.as_ref(), cx);
            changed = true;
        }
        // Busy staying on does not repaint. The sidebar loader changes when
        // the run starts, settles, or the process leaves.
        changed
    }

    /// Read a pending park once and return a session file it already named.
    pub(super) fn take_pending_draft_file(
        parked: &mut ParkedSession,
        _sessions: &[SessionInfo],
    ) -> Option<PathBuf> {
        // Same queue the heartbeat applies, in the same order. Scanning does
        // not apply or remove events; this park is about to be dropped.
        parked.pending.extend(parked.client.drain_events());
        newest_session_file(&parked.pending)
    }

    /// Drain one parked session's buffered events into its transcript and
    /// report what changed. `path` is `None` for a pending park: such a
    /// session also answers the boot handshake, because only the active view
    /// used to — a `new_session`/`clone` reply asks the process for
    /// `get_state`, and the reply naming `sessionFile` re-keys it into `lives`
    /// (issue #46).
    fn drain_parked_session(
        parked: &mut ParkedSession,
        path: Option<&Path>,
        sessions: &[SessionInfo],
        finished: &mut Vec<(PathBuf, Option<String>, Option<transcript::TurnSummary>)>,
        deadline: Instant,
        applied: &mut usize,
        limit: Option<usize>,
    ) -> ParkedDrain {
        let was_busy = parked.busy;
        let mut changed = false;
        let mut rekey = None;
        let mut restore = None;
        let mut failed_send = None;
        let mut staged = Vec::new();
        while !parked.pending.is_empty() {
            if Instant::now() >= deadline || limit.is_some_and(|limit| *applied >= limit) {
                break;
            }
            let event = parked.pending.pop_front().expect("parked queue");
            *applied += 1;
            let own_prompt = matches!(&event, Event::Response { id, command, .. }
                if command == "prompt" && parked.execution.prompt_id.as_deref() == Some(id));
            parked.execution.apply_event(&event);
            if own_prompt && !parked.execution.status().is_in_progress() {
                parked.transcript.abandon_reply();
                parked.busy = false;
                take_restore(&mut restore, parked.on_agent_settled_queue());
            }
            match &event {
                Event::AgentStart => {
                    parked.busy = true;
                    changed = true;
                }
                // `agent_settled` is the real settle (queued steering /
                // follow-up / retry can continue past `agent_end`).
                Event::AgentSettled { aborted } => {
                    changed = true;
                    parked.busy = false;
                    // The clock was parked open with this run. Close it here;
                    // the active view's `finish_turn` does not see this process.
                    parked.turns.finish();
                    parked.interaction = ParkedInteraction::default();
                    parked.retry_detail = None;
                    // A settled run has no queued continuation left; clear
                    // the mirror even if the final `queue_update` was missed.
                    // Keep `queue_wire`: this process may still owe replies.
                    // A deferred post-X message is restored onto the session
                    // draft so resume does not show a ghost bar.
                    take_restore(&mut restore, parked.on_agent_settled_queue());
                    // A pending session has no sidebar row yet, so there is
                    // no path to name in a notification; it is re-keyed
                    // moments later and notices from then on.
                    if let Some(path) = path.filter(|_| !aborted) {
                        finished.push((
                            path.to_path_buf(),
                            sessions
                                .iter()
                                .find(|session| session.path == path)
                                .map(|session| session.title.clone()),
                            parked.transcript.latest_turn_summary(),
                        ));
                    }
                }
                Event::ProcessExited => {
                    changed = true;
                    parked.startup_model_pinned = false;
                    parked.busy = false;
                    parked.interaction = ParkedInteraction::default();
                    parked.retry_detail = None;
                    parked.queue = PendingQueue::default();
                    parked.queue_wire = QueueWire::default();
                    // A resume whose process died before `get_messages` answered
                    // never delivered its prompt. Hand the unsent message back to
                    // the session's draft rather than dropping it with the park.
                    if let Some(prompt) = parked.resume_prompt.take() {
                        failed_send = Some(prompt.composer);
                    }
                }
                // pi emits `queue_update` only when the queue changes, so a
                // parked session must mirror it here to restore the pending
                // bar on resume — and to drop messages pi already delivered.
                Event::QueueUpdate { value } => {
                    parked.apply_queue_update(value);
                }
                // Native requests remain blocked until this session is opened.
                // The reducer saved the raw request; no answer is fabricated.
                Event::ExtensionUiRequest { id, method, value } => {
                    if matches!(method.as_str(), "select" | "input") {
                        if let Some(replay) = parked.interaction.ask_replay.as_mut() {
                            // These answers were already committed by the user;
                            // replaying them must not wait for a foreground UI.
                            let reply = replay.reply(id, method);
                            let finished = replay.is_finished()
                                || matches!(reply, crate::ask::AskReplayReply::Cancel);
                            let payload = match reply {
                                crate::ask::AskReplayReply::Value(value) => serde_json::json!({
                                    "type": "extension_ui_response", "id": id, "value": value,
                                }),
                                crate::ask::AskReplayReply::Cancel => serde_json::json!({
                                    "type": "extension_ui_response", "id": id, "cancelled": true,
                                }),
                            };
                            if parked.client.send(CommandBody::Raw(payload)).is_ok() {
                                parked.execution.respond(id);
                            } else {
                                parked.execution.apply_event(&Event::ProcessExited);
                            }
                            if finished {
                                parked.interaction.ask_replay = None;
                            }
                        }
                    } else if method == "custom" {
                        let _ = parked.client.cancel_dialog(id);
                    } else if method == "notify" {
                        // No toast for a background session, but the row waits
                        // in its transcript for when it is opened.
                        if let Some(message) = value.get("message").and_then(Value::as_str) {
                            parked
                                .transcript
                                .push_notice(&tr!("transcript.extension_notice"), message);
                        }
                    }
                    continue;
                }
                Event::AutoRetryEnd { .. } => parked.retry_detail = None,
                Event::TurnStart => parked.interaction = ParkedInteraction::default(),
                Event::ToolExecutionStart { value } => {
                    if value
                        .get("toolName")
                        .and_then(Value::as_str)
                        .is_some_and(crate::ask::is_ask_tool)
                    {
                        parked.interaction.ask_tool_id = value
                            .get("toolCallId")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        parked.interaction.ask_questions =
                            crate::ask::questions_from_args(value.get("args"));
                    }
                }
                Event::ToolExecutionEnd { value } => {
                    if parked.interaction.ask_tool_id.as_deref()
                        == value.get("toolCallId").and_then(Value::as_str)
                    {
                        parked.interaction = ParkedInteraction::default();
                    }
                }
                Event::MessageEnd { value } => {
                    // Real edit stats from finalized tool calls.
                    let (a, r) = transcript::diff_from_message(value);
                    parked.added += a;
                    parked.removed += r;
                }
                Event::Response {
                    id,
                    command,
                    success,
                    data,
                    ..
                } => {
                    if failed_send.is_none() {
                        failed_send =
                            parked.continue_resume(command, *success, data.as_ref());
                    }
                    if matches!(
                        command.as_str(),
                        "set_model" | "set_thinking_level" | "set_session_name"
                    ) {
                        staged.push((id.clone(), *success));
                    }
                    // pi answered this process's own startup correction while
                    // the reader was elsewhere: consume it here or resume
                    // would ask a second time.
                    if let Some(slot) =
                        parked.on_startup_model_response(id, command, *success)
                    {
                        if let Some(level) =
                            slot.thinking.filter(|level| !level.is_empty())
                        {
                            // Straight to the parked child: the foreground may
                            // have no process at all, or another session's.
                            let _ = parked.client.send(CommandBody::SetThinkingLevel {
                                level,
                            });
                        }
                        changed = true;
                    }
                    take_restore(
                        &mut restore,
                        parked.on_queue_response(id, command, *success),
                    );
                    if path.is_none() && *success {
                        let file = data
                            .as_ref()
                            .and_then(|data| data.get("sessionFile"))
                            .and_then(Value::as_str)
                            .map(PathBuf::from);
                        match command.as_str() {
                            "new_session" | "clone" | "fork" => {
                                // The reply landed while parked, so resume must
                                // not treat it as still in flight.
                                if command == "new_session" {
                                    parked.startup_model_pinned = false;
                                }
                                // A clone or fork inherits the source transcript in
                                // the active view, which clears it on the reply; the
                                // parked one never saw that, so clear it here.
                                if command == "clone" || command == "fork" {
                                    let mut transcript = Transcript::new();
                                    transcript.park_presentation();
                                    parked.transcript = transcript;
                                    parked.turns = crate::turn_review::TurnClock::default();
                                    changed = true;
                                }
                                match file {
                                    Some(file) => rekey = Some(file),
                                    // pi creates the file but does not echo it on
                                    // this reply; the follow-up `get_state` names
                                    // it (same order the active view uses).
                                    None => {
                                        let _ = parked.client.send(CommandBody::GetState);
                                    }
                                }
                            }
                            "get_state" | "get_session_stats" => rekey = file,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            parked.transcript.apply_event(&event);
        }
        // A settled parent is not idle while a subagent it spawned is still
        // running. The stamp check on reopen only keeps a parked process when
        // this flag is set.
        let busy =
            parked.execution.status().is_in_progress() || parked.subagents.work_in_flight();
        if busy != was_busy {
            changed = true;
        }
        parked.busy = busy;
        ParkedDrain {
            changed,
            rekey,
            restore,
            failed_send,
            staged,
        }
    }

    /// The newest `sessionFile` already sitting in this process's buffer.
    /// Used when a draft is deleted before the reply is applied, so a
    /// header-only file can still be removed. This bypasses the app queue on
    /// purpose: it only wants `sessionFile`, and every other event is dropped.
    pub(super) fn session_file_in_buffer(client: &mut PiClient) -> Option<PathBuf> {
        let mut found = None;
        for event in client.drain_events() {
            let Event::Response { success, data, .. } = &event else {
                continue;
            };
            if !success {
                continue;
            }
            if let Some(file) = data
                .as_ref()
                .and_then(|data| data.get("sessionFile"))
                .and_then(Value::as_str)
            {
                found = Some(PathBuf::from(file));
            }
        }
        found
    }

    /// Replace the transcript with a `get_messages`-shaped snapshot and
    /// rebuild the top-bar diff counts from it. Shared by pi's authoritative
    /// `get_messages` response and the disk preview that paints a cold session
    /// before pi answers.
    pub(super) fn apply_messages_snapshot(&mut self, data: &Value, cx: &mut Context<Self>) {
        self.transcript.load_from(data);
        self.added = 0;
        self.removed = 0;
        if let Some(messages) = data.get("messages").and_then(Value::as_array) {
            for message in messages {
                let (a, r) = transcript::diff_from_message(message);
                self.added += a;
                self.removed += r;
            }
        }
        // Message indices moved. Rebuild the turn clock without letting the
        // latest number step backwards, then refresh Last Turn.
        self.rebuild_turn_clock();
        self.push_turn_records(cx);
        self.log_resident("session");
    }

    /// Mirror a `queue_update` unless a `clear_queue` is still on the wire
    /// (or a reclear is armed because an enqueue raced the last clear).
    pub(super) fn apply_queue_update(&mut self, value: &serde_json::Value) {
        if self.queue_wire.ignores_updates() {
            return;
        }
        self.queue = PendingQueue::from_value(value);
    }

    /// pi's two "there is nothing to do" compaction answers, as a ready-to-show
    /// notice. There is no summarizable prefix outside the keep-recent window
    /// for the model the session runs on, or its newest entry already is a
    /// compaction: only pi knows either, so a Compact on a session without a
    /// process cannot check in advance — but a red banner for "nothing to do"
    /// reads as a broken click. Matched *exactly* against pi's two thrown
    /// messages (`core/compaction/compaction.js`), never as a substring: a
    /// real diagnostic that merely mentions one of them is a real failure.
    fn benign_compaction_message(error: &str) -> Option<String> {
        // The `compaction_end` event carries pi's decorated form.
        let error = error.strip_prefix("Compaction failed: ").unwrap_or(error);
        match error {
            "Nothing to compact (session too small)" => Some(tr!("events.compaction_too_small")),
            "Already compacted" => Some(tr!("events.compaction_already")),
            _ => None,
        }
    }

    /// [`Self::benign_compaction_message`] for a refused `compact` response.
    /// Scoped to that one command: a `prompt` or `set_model` failure whose
    /// text happens to quote the phrase is still that command's failure.
    fn benign_compaction_refusal(command: &str, error: Option<&str>) -> Option<String> {
        if command != "compact" {
            return None;
        }
        Self::benign_compaction_message(error?)
    }

    /// Apply a command response to the app. The arguments are the response's
    /// own fields; `id` is what tells one `prompt` from another.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn on_response(
        &mut self,
        command: &str,
        success: bool,
        data: Option<&serde_json::Value>,
        error: Option<&str>,
        id: &str,
        refresh_sessions: &mut bool,
        cx: &mut Context<Self>,
    ) {
        // Provider-auth commands are handled here so success/failure and the
        // no-payload replies (`auth.cancel`) never fall through the
        // data-required branch below.
        if command.starts_with("auth.") {
            self.on_auth_response(command, success, data, error, cx);
            return;
        }
        // pi's `clear_queue` response carries the *dropped* items after an
        // empty `queue_update`. Never write that echo back into `self.queue`.
        if command == "clear_queue" {
            self.queue_wire.on_clear_response();
            self.flush_deferred_enqueues(cx);
        }
        if matches!(command, "steer" | "follow_up") {
            if let Some((entry, reclear)) =
                self.queue_wire.on_enqueue_response(id, command, success)
            {
                if !success && !entry.cancelled {
                    remove_one_queued(&mut self.queue, entry.mode, &entry.text);
                    if self.input.read(cx).text().trim().is_empty() {
                        self.input
                            .update(cx, |input, cx| input.set_text(entry.text, cx));
                    }
                }
                if reclear {
                    self.send_clear_queue();
                }
            }
            self.flush_deferred_enqueues(cx);
        }
        // Account quota/balance/spend from the `quota.*` namespace; carries no
        // secrets, so it merges straight into the cache.
        if command == "quota.list" {
            self.quota.on_response(success, data, error);
            // Stop the manual refresh's spin, held for its minimum duration so
            // an immediate reply still reads as a deliberate refresh.
            self.finish_quota_refresh(cx);
            cx.notify();
            return;
        }
        // The bundled bridge appends quota snapshots as session entries; poll
        // responses merge them and must not surface a stale-cursor error as a
        // user-facing banner.
        if command == "get_entries" {
            // A footer fork reads the whole tree to find the cut. That reply
            // must not move the quota cursor or clear the poll's in-flight flag.
            // A fork cancelled before this reply is the same full-tree dump.
            if self.take_ignored_fork_entries(id) {
                return;
            }
            if self.fork_entries_pending(id) {
                if success {
                    if let Some(data) = data {
                        self.on_fork_entries(id, data, cx);
                    } else {
                        self.reject_fork_point(command, id);
                    }
                } else {
                    self.reject_fork_point(command, id);
                }
                return;
            }
            self.on_entries_response(success, data, error, cx);
            return;
        }
        // Error handling (docs #error-handling): a failed command carries an
        // `error` string. Surface it, unwind command-specific optimistic
        // state, and stop — success paths below assume `data` is valid.
        if !success {
            self.abandon_fork_command(command, id);
            if matches!(command, "switch_session" | "get_state") {
                self.pending_session_rename = None;
                self.session_rename_ready = false;
            }
            // A message waiting on a resumed session never reached pi. That
            // process is on pi's empty startup session, not the one on screen,
            // so it goes away with the message handed back: the next send
            // starts a clean one instead of prompting the wrong session.
            if matches!(command, "switch_session" | "get_messages")
                && self
                    .pending_prompt
                    .as_ref()
                    .is_some_and(|pending| pending.resume_path.is_some())
            {
                self.cancel_waiting_prompt(cx);
            }
            // A failed replacement still ends the wait: pi refused it, so no
            // answer is coming and the gate must not outlive it.
            if matches!(command, "new_session" | "switch_session") {
                if command == "new_session" {
                    self.startup_model_pinned = false;
                }
                self.end_replacement(cx);
            }
            // A load that failed will never deliver what waits on it: an
            // owed compaction would otherwise leave the card on "Compacting…"
            // with no click handler and `compact_now` refusing forever.
            if matches!(command, "get_messages" | "switch_session") {
                self.pending_compact = None;
            }
            self.fail_staged(id);
            match Self::benign_compaction_refusal(command, error) {
                Some(notice) => self.toast_info(notice),
                None => self.on_command_failure(command, error, cx),
            }
            match command {
                "compact" => {
                    self.is_compacting = false;
                    self.refresh_context_stats();
                }
                // A failed switch can never complete the armed default push;
                // disarm so a later manual pick of the same model cannot be
                // overridden by the configured thinking level.
                "set_model" => {
                    self.default_model_armed = false;
                    // A refused startup correction means pi's registry has no
                    // such model: release the chips and say which pick died.
                    self.startup_model_refused(id, error, cx);
                    self.send(CommandBody::GetState, "get_state");
                }
                // These changed local UI optimistically; re-read pi's state.
                "cycle_model"
                | "set_thinking_level"
                | "cycle_thinking_level"
                | "set_session_name" => {
                    self.send(CommandBody::GetState, "get_state");
                }
                _ => {}
            }
            return;
        }
        // A staged choice leaves the store only on its success reply.
        if self.ack_staged(id) {
            *refresh_sessions = true;
        }
        // A successful retry clears the banner it raised earlier.
        self.clear_error_from(command);
        // Commands whose replies carry no payload (e.g. `set_thinking_level`
        // answers `{"success":true}` with no `data`). They still need their
        // follow-up state refresh, so they are matched BEFORE the
        // data-required branch below.
        match command {
            "set_model" => {
                // An accepted startup correction ends the hold and re-asks the
                // level pi reset when the switch landed; queue it before the
                // state read so that reply already carries both.
                self.startup_model_corrected(id, cx);
                self.send(CommandBody::GetState, "get_state");
                self.send(
                    CommandBody::GetAvailableThinkingLevels,
                    "get_available_thinking_levels",
                );
                return;
            }
            "cycle_model" | "set_thinking_level" | "cycle_thinking_level" => {
                self.send(CommandBody::GetState, "get_state");
                return;
            }
            // Agent-control commands: nothing more to do. `steer`/`follow_up`
            // already decremented enqueue tracking above. `clear_queue` carries
            // the dropped `{steering, followUp}` payload, which must not be
            // written back into the local mirror.
            "set_steering_mode"
            | "set_follow_up_mode"
            | "set_auto_compaction"
            | "set_auto_retry"
            | "abort_retry"
            | "steer"
            | "follow_up"
            | "clear_queue" => {
                return;
            }
            // The rename has no payload. A footer fork keeps its title locked
            // until this answer, then refreshes the sidebar from the file.
            "set_session_name" => {
                if self.complete_fork_rename(id, cx) {
                    *refresh_sessions = true;
                }
                return;
            }
            // A prompt pi hands to an *extension command* never starts a run:
            // the answer is `{disposition: "handled"}` and no `agent_start` or
            // `agent_settled` follows. The composer arms the live "Working
            // for…" row optimistically at the send, so without this the row and
            // the Stop button stay up for a turn that does not exist — the
            // `/review` custom UI case in issue #76, where cancelling the
            // surface could not help because there was nothing to settle.
            // `started` and `queued` both mean a run is coming or continuing.
            "prompt" => {
                let handled = data
                    .as_ref()
                    .and_then(|data| data.get("disposition"))
                    .and_then(Value::as_str)
                    == Some("handled");
                // `prompt` is also how Orbit sends extension commands (the
                // title regenerator), and pi answers those `handled` too. Only
                // the composer's own prompt was armed for a run, so only its
                // reply may unwind one — otherwise regenerating a title mid-run
                // would abandon the live row of a run still streaming.
                if self.prompt_in_flight.as_deref() != Some(id) {
                    return;
                }
                self.prompt_in_flight = None;
                if handled {
                    self.execution = SessionExecution::default();
                    self.busy = false;
                    self.transcript.abandon_reply();
                    // A post-X follow-up held for this turn must not flush
                    // `follow_up` onto an idle pi after `/review` (etc.) is
                    // handled without a run.
                    self.settle_local_queue(cx);
                    // The turn opened at the send never ran: close it so the
                    // next prompt starts a fresh one.
                    self.finish_turn(cx);
                }
                return;
            }
            // `compact` also has a result payload on success; clearing the
            // compacting state belongs before the data gate.
            "compact" => {
                self.is_compacting = false;
                self.toast_info(tr!("events.context_compacted"));
                self.refresh_context_stats();
                self.send(CommandBody::GetState, "get_state");
                return;
            }
            // `clone` duplicates the session and moves the process onto the
            // copy. Adopt the new file/id when pi reports them, then reload.
            // A footer fork of the last turn uses the same command and then
            // renames; a cancelled fork leaves the open session alone.
            "clone" => {
                let footer = self.footer_fork_switching(id);
                if footer && response_cancelled(data) {
                    self.fork_pending = None;
                    self.toast_info(tr!("transcript.fork_cancelled"));
                    return;
                }
                if !footer {
                    // A palette clone is a different session. Drop a held
                    // fork title so it cannot stick to this copy. An in-flight
                    // footer fork (still resolving its cut) keeps its id.
                    if matches!(self.fork_pending, Some(ForkPending::Holding { .. })) {
                        self.fork_pending = None;
                    }
                }
                self.execution = SessionExecution::default();
                self.adopt_branched_session(data, refresh_sessions, cx);
                if footer {
                    self.begin_fork_rename(cx);
                }
                return;
            }
            // `fork` cuts before a user message and switches onto the new
            // file, same as clone. The footer sends it for a reply that is
            // not the last turn.
            "fork" => {
                if response_cancelled(data) {
                    if self.footer_fork_switching(id) {
                        self.fork_pending = None;
                        self.toast_info(tr!("transcript.fork_cancelled"));
                    }
                    return;
                }
                let footer = self.footer_fork_switching(id);
                self.execution = SessionExecution::default();
                self.adopt_branched_session(data, refresh_sessions, cx);
                if footer {
                    self.begin_fork_rename(cx);
                }
                return;
            }
            _ => {}
        }

        let Some(data) = data else { return };
        match command {
            "get_state" => {
                // pi resolves `--model` as a *fuzzy pattern*, so a process can
                // come up on a neighbour of the model the composer asked for.
                // While the one exact correction is in flight the chips stay
                // on the pick — painting pi's answer here is what read as
                // "the model switched by itself".
                let held = self.startup_model_held(data);
                if !held {
                    if let Some(model) = data.get("model") {
                        if let Some(name) = model.get("name").and_then(serde_json::Value::as_str) {
                            self.model_label = name.to_string();
                        }
                        if let Some(id) = model.get("id").and_then(serde_json::Value::as_str) {
                            self.model_id = id.to_string();
                        }
                        if let Some(provider) =
                            model.get("provider").and_then(serde_json::Value::as_str)
                        {
                            self.model_provider = provider.to_string();
                        }
                    }
                }
                // The Git page labels its context with the provider/model.
                self.sync_review_context(cx);
                if !held {
                    if let Some(level) = data
                        .get("thinkingLevel")
                        .and_then(serde_json::Value::as_str)
                    {
                        self.thinking_label = level.to_string();
                    }
                }
                self.remember_reported_model(data);
                if let Some(file) = data.get("sessionFile").and_then(Value::as_str) {
                    self.adopt_session_file(PathBuf::from(file));
                }
                if let Some(id) = data.get("sessionId").and_then(Value::as_str) {
                    if self.session_id.as_deref() != Some(id) {
                        self.session_id = Some(id.to_string());
                        // Entry ids are per-session; a different session means
                        // the bridge cursor must start over.
                        self.reset_quota_entries();
                        self.recover_latest_turn(cx);
                        self.sync_review_context(cx);
                    }
                }
                // Agent control surface: streaming/compaction state, queue
                // modes, auto-compaction, and the session display name.
                let state = SessionState::from_value(data);
                self.execution.reconcile(&state);
                self.busy = state.is_streaming
                    || state.is_compacting
                    || self.execution.status().is_in_progress();
                self.is_compacting = state.is_compacting;
                self.follow_up_mode = state.follow_up_mode;
                self.auto_compaction = state.auto_compaction_enabled;
                // A footer fork has already shown `(N) title`. This reply
                // may be the `get_state` queued before `set_session_name`,
                // still carrying the copied name. Leave the fork title
                // alone until that rename answers.
                if !self.name_report_held() {
                    match &state.session_name {
                        Some(name) if self.session_name.as_deref() != Some(name.as_str()) => {
                            self.session_name = Some(name.clone());
                            let name = name.clone();
                            self.session_name_input
                                .update(cx, |input, cx| input.set_text(name, cx));
                        }
                        None if self.session_name.is_some() => {
                            self.session_name = None;
                            self.seed_session_name_input(cx);
                        }
                        _ => {}
                    }
                }
                if let Some(path) = self.pending_session_rename.as_ref() {
                    if self.current_session_path.as_ref() == Some(path)
                        && data
                            .get("sessionFile")
                            .and_then(Value::as_str)
                            .is_some_and(|file| std::path::Path::new(file) == path)
                    {
                        self.pending_session_rename = None;
                        self.session_rename_ready = true;
                        cx.notify();
                    }
                }
                self.sync_model_selector(cx);
                self.refresh_context_stats();
                // Capability probe: a pi that advertises `custom` will stream
                // `method:"custom"` frames for `ctx.ui.custom()`.
                if let Some(methods) = data
                    .get("capabilities")
                    .and_then(|caps| caps.get("extension_ui"))
                    .and_then(Value::as_array)
                {
                    self.custom_ui_supported = methods
                        .iter()
                        .any(|method| method.as_str() == Some("custom"));
                }
                // A `new_session` birth may still need its default model
                // pushed; this is a no-op otherwise.
                self.apply_default_model(cx);
                // The argv model is a request pi may have resolved loosely:
                // check what actually came up, and ask once for the exact one.
                self.reconcile_startup_model(data, cx);
            }
            "get_messages" => {
                self.apply_messages_snapshot(data, cx);
                // A message that resumed this session was waiting for pi to
                // load it: the snapshot replaced the transcript it sat on.
                self.deliver_resume_prompt(cx);
                // So was a Compact clicked while the session had no process.
                self.deliver_pending_compact(cx);
                self.refresh_context_stats();
            }
            "get_available_models" => {
                // Stamped with the process's own boot: pi reads its model
                // files once at start, so this catalog cannot vouch for an
                // edit made since, and the next launch must probe again.
                crate::catalog_cache::store_models_at(data, self.process_sources_stamp);
                self.apply_models_payload(data);
                self.sync_model_selector(cx);
                // The model catalog arrived after the session: retry a default
                // that could not be resolved yet.
                self.apply_default_model(cx);
                // The Git page sizes its commit-message prompt from the model's
                // context window, which only the catalog knows.
                self.sync_review_context(cx);
            }
            "get_available_thinking_levels" => {
                self.available_thinking_levels = data
                    .get("levels")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                self.sync_model_selector(cx);
            }
            "get_commands" => {
                if let Some(cwd) = self.current_workspace.as_deref() {
                    crate::catalog_cache::store_commands(cwd, data);
                }
                self.apply_commands_payload(data);
                // A `/` menu already open picks up the fresh command list.
                self.sync_autocomplete(cx);
            }
            "get_session_stats" => {
                self.context = ContextUsage::from_stats(data);
                self.session_usage = SessionUsage::from_stats(data);
                if let Some(file) = data.get("sessionFile").and_then(Value::as_str) {
                    self.adopt_session_file(PathBuf::from(file));
                }
                // A live reply replaces any stored snapshot, and becomes the next one.
                self.context_is_snapshot = false;
                if let Some(path) = &self.current_session_path {
                    crate::session_snapshot::put(path, data);
                }
            }
            "switch_session" => {
                // pi is on the requested session now: name reports may paint
                // again, and a click that arrived mid-switch runs.
                self.end_replacement(cx);
                if success {
                    // A resumed session keeps the model in its file (D-D).
                    self.default_model_armed = false;
                    self.reset_turns(cx);
                    self.discard_inflight_queue_on_replace(cx);
                    self.sync_review_context(cx);
                    // Load the session *before* the slower auth/quota probes,
                    // which would otherwise queue ahead of `get_messages` and
                    // push the transcript out by a network round trip.
                    self.send(CommandBody::GetMessages, "get_messages");
                    // Choices made while this session had no process go to pi now,
                    // ahead of the first prompt.
                    self.apply_staged_choices();
                    self.send(CommandBody::GetState, "get_state");
                    self.refresh_catalogs();
                    self.probe_auth();
                }
            }
            "new_session" => {
                // pi is on the new session now: name reports below describe it,
                // and a click that arrived meanwhile runs.
                self.end_replacement(cx);
                self.cancel_fork_followup();
                if !self.busy {
                    self.execution = SessionExecution::default();
                }
                self.transcript.clear_for_new_session();
                self.current_title = None;
                self.reset_session_name(cx);
                // A listed draft may already be bound to this file. Clearing
                // the path would make the next leave park the process as
                // pending and lose the file it just adopted.
                if !self.draft_file_is_bound() {
                    self.current_session_path = None;
                }
                self.added = 0;
                self.removed = 0;
                self.context = None;
                self.session_usage = None;
                // Widgets belong to the process that sent them.
                self.extension_widgets.clear();
                self.subagents = crate::subagent_status::SubagentBoard::default();
                self.subagent_scrolls.clear();
                self.subagent_list_scroll = ScrollHandle::new();
                self.subagent_scroll_placed = 0;
                self.subagent_list_rows = 0;
                self.clear_subagent_folds();
                self.reset_turns(cx);
                self.discard_inflight_queue_on_replace(cx);
                self.sync_review_context(cx);
                *refresh_sessions = true;
                // A fresh session is born on the default model; `set_model`
                // is pushed once `get_state` reports the new id (see
                // `apply_default_model`). A process started from a draft
                // that already chose a model keeps that argv instead.
                if self.startup_model_pinned {
                    self.startup_model_pinned = false;
                    self.default_model_armed = false;
                } else {
                    self.default_model_armed = true;
                }
                self.send(CommandBody::GetState, "get_state");
                self.refresh_catalogs();
            }
            _ => {
                let _ = success;
            }
        }
    }

    /// Apply a provider-auth command response to the [`AuthManager`] and act
    /// on any effects it returns.
    pub(super) fn on_auth_response(
        &mut self,
        command: &str,
        success: bool,
        data: Option<&serde_json::Value>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        match command {
            "auth.list" => {
                let before = self.auth.support();
                self.auth.on_list_response(success, data, error);
                if self.auth.support() != before && self.auth.support() == AuthSupport::Unsupported
                {
                    self.set_status(tr!("events.auth_rpc_missing"));
                }
            }
            "auth.status" => self.auth.on_status_response(success, data),
            "auth.login" => {
                self.auth.on_login_response(success, data, error);
                // Acknowledgement can arrive after a `started` event; make
                // sure the card reflects the manager either way.
            }
            "auth.logout" => {
                if let Some(provider) = data
                    .and_then(|data| data.get("provider"))
                    .and_then(serde_json::Value::as_str)
                {
                    self.auth.on_logout_response(success, provider);
                } else if success {
                    // Some servers answer without echoing the provider; the
                    // UI already refreshed via `auth_credentials_changed`.
                }
            }
            "auth.cancel" => {
                // Nothing to reconcile: the event stream confirms cancellation.
            }
            _ => {}
        }
        cx.notify();
    }

    /// Apply a `get_entries` poll for the quota bridge. A cursor can dangle
    /// when the session file changed underneath it; that is recovered by
    /// dropping the cursor and re-reading, never by a user-facing error.
    pub(super) fn on_entries_response(
        &mut self,
        success: bool,
        data: Option<&serde_json::Value>,
        error: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.quota_entries_inflight = false;
        if !success {
            let stale_cursor = self.quota_entries_cursor.is_some()
                && error.is_some_and(|error| error.contains("Entry not found"));
            if stale_cursor {
                self.quota_entries_cursor = None;
                self.quota_entries_next_poll = Instant::now();
                self.poll_quota_entries();
            }
            return;
        }
        let Some(entries) = data
            .and_then(|data| data.get("entries"))
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        // The cursor is the last appended entry id: entries arrive in append
        // order, so the next poll asks for strictly newer ones only.
        if let Some(id) = entries
            .last()
            .and_then(|entry| entry.get("id"))
            .and_then(serde_json::Value::as_str)
        {
            self.quota_entries_cursor = Some(id.to_string());
        }
        if self.quota.on_entries(entries) {
            // A snapshot landed: no need for fast bootstrap polls.
            self.quota_entries_bootstrap = 0;
            cx.notify();
        }
    }

    /// Move images the composer collected (clipboard paste) into the
    /// attachment queue. Called on tick and again at submit so a paste
    /// immediately followed by Enter still attaches.
    pub(super) fn drain_pasted_images(&mut self, cx: &mut Context<Self>) {
        if !self.input.read(cx).has_pasted_images() {
            return;
        }
        let pasted = self
            .input
            .update(cx, |input, _| std::mem::take(&mut input.pasted_images));
        for image in &pasted {
            if self.attachment_budget_exhausted() {
                self.report_attachment_limit();
                break;
            }
            let index = self.attachments.len();
            self.push_attachment(Attachment::from_image(image, index), cx);
        }
        if !pasted.is_empty() {
            cx.notify();
        }
    }

    /// Move long pastes the composer set aside into pill attachments. Called
    /// on tick and again at submit, so paste-then-Enter never loses the text.
    pub(super) fn drain_pasted_texts(&mut self, cx: &mut Context<Self>) {
        if !self.input.read(cx).has_pasted_texts() {
            return;
        }
        let pasted = self
            .input
            .update(cx, |input, _| std::mem::take(&mut input.pasted_texts));
        for text in pasted {
            if self.attachment_budget_exhausted() {
                self.report_attachment_limit();
                break;
            }
            self.pasted_texts.push(PastedText::new(text));
        }
        cx.notify();
    }

    /// Keep the chat composer's placeholder honest about what Enter does: an
    /// idle send starts a prompt, a send mid-run queues a follow-up or steers
    /// the live turn, depending on the configured send mode. The key changes
    /// only when that answer changes, so the heartbeat stays quiet.
    pub(super) fn sync_composer_placeholder(&mut self, cx: &mut Context<Self>) {
        let key = if self.is_running() {
            match theme::get(cx).ui.composer_send_mode {
                SendMode::FollowUp => Some("composer.placeholder_follow_up"),
                SendMode::Steer => Some("composer.placeholder_steer"),
            }
        } else {
            None
        };
        self.input
            .update(cx, |input, cx| input.set_placeholder_key(key, cx));
    }

    /// Post the "turn finished" notification for a settle and leave the
    /// session's row marked until the user actually reads it. Nothing is posted
    /// while the window is frontmost except the in-app toast, and a turn the
    /// user aborted holds no news. `session` is the file path that a banner
    /// click routes back to.
    ///
    /// Both settle paths meet here — the open session's and a parked background
    /// run's — which is what makes the review mark cover every session that
    /// finishes out of sight, not just the one on screen (issue #83).
    pub(super) fn notify_turn_finished(
        &mut self,
        session: Option<&Path>,
        title: Option<&str>,
        summary: Option<&transcript::TurnSummary>,
        cx: &mut Context<Self>,
    ) {
        let Some(summary) = summary else { return };
        if summary.aborted {
            return;
        }
        // The mark outlives the toast: it is the only cue left once the toast
        // has gone, and the only one that says *which* session it was.
        if let Some(path) = session {
            let in_front = self.transcript_in_front(cx);
            if should_mark_review(in_front, path, self.current_session_path.as_deref()) {
                let mark = if summary.failed {
                    ReviewMark::Failed
                } else {
                    ReviewMark::Done
                };
                if self.awaiting_review.insert(path.to_path_buf(), mark) != Some(mark) {
                    cx.notify();
                }
            }
        }
        let (subtitle, fallback, kind) = if summary.failed {
            (
                tr!("events.notify_turn_failed"),
                tr!("events.notify_turn_failed_body"),
                ToastKind::Error,
            )
        } else {
            (
                tr!("events.notify_turn_finished"),
                tr!("events.notify_turn_finished_body"),
                ToastKind::Success,
            )
        };
        let body = if summary.body.trim().is_empty() {
            fallback.as_str()
        } else {
            summary.body.as_str()
        };
        self.post_notification(session, title, &subtitle, body, kind);
    }

    /// Whether a full-page surface has replaced the chat column: Files, Git,
    /// Usage, Worktrees, or a full-width right dock. Mirrors the renderer's own
    /// rule in `view.rs` (`feature_open` / `pane_full`) — keep the two in step,
    /// since a page that hides the transcript must also stop the review mark
    /// from being spent.
    fn chat_column_covered(&self, cx: &App) -> bool {
        // Same gate the renderer puts on a full-width dock: it only owns the
        // page once its dependencies are ready and the dock is showing.
        let review_covers_chat = self.dock_full_width(cx);
        self.file_viewer.read(cx).is_open()
            || self.git_open
            || self.usage_open
            || self.worktrees_open
            || review_covers_chat
    }

    /// Whether the transcript is the surface genuinely in front of the user:
    /// the window is frontmost and no full-page surface has replaced the chat
    /// area. A settle out of sight leaves a review mark even when the window is
    /// active (issue #83).
    fn transcript_in_front(&self, cx: &App) -> bool {
        self.window_active && !self.settings_open && !self.chat_column_covered(cx)
    }

    /// A run is blocked on an extension dialog — the one event a user cannot
    /// discover once they have left the window.
    fn notify_input_needed(&mut self, method: &str, value: &Value) {
        if !matches!(method, "select" | "confirm" | "input" | "editor") {
            return;
        }
        let title = self
            .current_title
            .clone()
            .or_else(|| self.session_name.clone());
        let waiting_body = tr!("events.notify_waiting_body");
        let body = value
            .get("title")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(&waiting_body);
        let session = self.current_session_path.clone();
        self.post_notification(
            session.as_deref(),
            title.as_deref(),
            &tr!("events.notify_waiting"),
            body,
            ToastKind::Warning,
        );
    }

    /// Deliver a background notification through every enabled channel.
    /// While the window is frontmost the banner and sound stay quiet — the
    /// in-app toast carries the event instead.
    fn post_notification(
        &mut self,
        session: Option<&Path>,
        title: Option<&str>,
        subtitle: &str,
        body: &str,
        kind: ToastKind,
    ) {
        let prefs = self.notification_prefs;
        if !prefs.desktop && !prefs.sound && !prefs.toasts {
            return;
        }
        if self.window_active {
            if prefs.toasts {
                let title = title
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or("Orbit Pi");
                self.push_toast(
                    kind,
                    title,
                    Some(notifications::preview(
                        body,
                        notifications::BODY_PREVIEW_CHARS,
                    )),
                );
            }
            return;
        }
        if prefs.desktop {
            let title = title
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("Orbit Pi");
            notifications::notify(session, title, subtitle, body);
        }
        if prefs.sound {
            notifications::play_sound();
        }
    }

    /// pi moved onto a forked or cloned session. Clear the open transcript and
    /// reload it from the new file. The caller renames a footer fork after this.
    pub(super) fn adopt_branched_session(
        &mut self,
        data: Option<&serde_json::Value>,
        refresh_sessions: &mut bool,
        cx: &mut Context<Self>,
    ) {
        self.transcript.clear();
        self.current_title = None;
        self.reset_session_name(cx);
        self.current_session_path = None;
        self.added = 0;
        self.removed = 0;
        self.context = None;
        self.session_usage = None;
        self.reset_turns(cx);
        self.discard_inflight_queue_on_replace(cx);
        self.session_id = None;
        // A branch mirrors its source session; it is not put on the default model.
        self.default_model_armed = false;
        // Nor is it the session a Compact was clicked on: the fork's own
        // `get_messages` must not deliver a compaction owed to another file.
        self.pending_compact = None;
        if let Some(data) = data {
            if let Some(file) = data.get("sessionFile").and_then(Value::as_str) {
                self.adopt_session_file(PathBuf::from(file));
            }
            if let Some(id) = data.get("sessionId").and_then(Value::as_str) {
                self.session_id = Some(id.to_string());
                self.reset_quota_entries();
            }
        }
        self.sync_review_context(cx);
        *refresh_sessions = true;
        self.send(CommandBody::GetMessages, "get_messages");
        self.send(CommandBody::GetState, "get_state");
        self.refresh_catalogs();
    }

    /// Re-point the workspace watcher when the active workspace moves.
    /// Comparing the last-attempted dir (rather than the live watcher) means
    /// a backend that failed to start is retried on the next workspace
    /// change, not every heartbeat. The branch chip belongs to the workspace,
    /// so it refetches here too.
    pub(super) fn sync_workspace_watcher(&mut self, cx: &mut Context<Self>) {
        if self.workspace_watch_dir == self.current_workspace {
            return;
        }
        self.workspace_watch_dir = self.current_workspace.clone();
        self.workspace_watcher = self
            .current_workspace
            .as_deref()
            .and_then(watch::WorkspaceWatcher::start);
        self.refresh_branch_status(cx);
        // The status bar's "Work in" chip and the Worktrees page both read the
        // repository's worktree list, so keep it current on every workspace
        // move (one background `git worktree list`).
        self.refresh_worktrees(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_updates_wait_for_the_stream_cadence() {
        assert!(is_stream_delta(&Event::ToolExecutionUpdate {
            value: serde_json::json!({}),
        }));
        assert!(!is_stream_delta(&Event::ToolExecutionStart {
            value: serde_json::json!({}),
        }));
        assert!(!is_stream_delta(&Event::ToolExecutionEnd {
            value: serde_json::json!({}),
        }));
        assert!(!is_stream_delta(&Event::MessageEnd {
            value: serde_json::json!({}),
        }));
        assert!(forces_visual_commit(&Event::MessageEnd {
            value: serde_json::json!({}),
        }));
        assert!(forces_visual_commit(&Event::ToolExecutionEnd {
            value: serde_json::json!({}),
        }));
        assert!(forces_visual_commit(&Event::AgentSettled { aborted: false }));
        assert!(!forces_visual_commit(&Event::ToolExecutionUpdate {
            value: serde_json::json!({}),
        }));
    }

    /// The review mark is only spent when the chat column is genuinely in front
    /// of the user. Every surface that replaces the chat area has to count as
    /// "covered", or a settle behind it would look read (issue #83).
    #[gpui::test]
    fn a_full_page_surface_is_not_the_transcript_in_front(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeKey::ORBIT));
            let app = cx.new(OrbitApp::new);
            app.update(cx, |app, cx| {
                app.window_active = true;
                assert!(
                    app.transcript_in_front(cx),
                    "a frontmost window on the chat is the transcript in front"
                );

                app.git_open = true;
                assert!(!app.transcript_in_front(cx), "the Git page covers it");
                app.git_open = false;

                app.usage_open = true;
                assert!(!app.transcript_in_front(cx), "the Usage page covers it");
                app.usage_open = false;

                app.worktrees_open = true;
                assert!(!app.transcript_in_front(cx), "the Worktrees page covers it");
                app.worktrees_open = false;

                app.settings_open = true;
                assert!(!app.transcript_in_front(cx), "Settings covers it");
                app.settings_open = false;

                app.window_active = false;
                assert!(!app.transcript_in_front(cx), "a background window is not");
                app.window_active = true;
                assert!(app.transcript_in_front(cx));
            });
        });
    }
}
