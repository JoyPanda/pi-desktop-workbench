use super::helpers::*;
use super::*;

use gpui::{point, Hsla, Pixels};

use crate::sessions::cap_chars;
use crate::shimmer::ShimmerText;
use crate::theme::tokens::{
    context_menu, list_item, picker, popover, BufferLineHeight, ButtonSize, DynamicSpacing,
    IconSize, Radius, TextSize,
};
use crate::workspace_mark::{self, WorkspaceMark};

/// Whether a workspace group is collapsed in the sidebar. The active
/// workspace is expanded by default; all others are collapsed unless the
/// user has toggled them.
pub(crate) fn is_workspace_group_collapsed(
    label: &str,
    working_label: &str,
    collapsed_workspaces: &HashSet<String>,
    expanded_workspace_groups: &HashSet<String>,
) -> bool {
    if label == working_label {
        collapsed_workspaces.contains(label)
    } else {
        !expanded_workspace_groups.contains(label)
    }
}

/// Toggle a workspace group's open/closed state against the defaults above.
pub(crate) fn toggle_workspace_group(
    label: String,
    working_label: &str,
    collapsed_workspaces: &mut HashSet<String>,
    expanded_workspace_groups: &mut HashSet<String>,
) {
    if label == working_label {
        if !collapsed_workspaces.remove(&label) {
            collapsed_workspaces.insert(label);
        }
    } else if !expanded_workspace_groups.remove(&label) {
        expanded_workspace_groups.insert(label);
    }
}

/// Sessions visible under one workspace group: at most `limit` rows, with
/// the open session swapped into the last slot when it falls outside.
pub(crate) fn visible_sessions_in_group(
    ixs: &[usize],
    sessions: &[SessionInfo],
    limit: usize,
    active_path: &Option<PathBuf>,
) -> Vec<usize> {
    if ixs.len() <= limit {
        return ixs.to_vec();
    }

    let mut indices: Vec<usize> = ixs.iter().take(limit).copied().collect();
    if let Some(active) = active_path {
        if let Some(active_ix) = sessions.iter().position(|s| &s.path == active) {
            if ixs.contains(&active_ix) && !indices.contains(&active_ix) {
                indices.pop();
                indices.push(active_ix);
                indices.sort_by_key(|ix| ixs.iter().position(|&i| i == *ix).unwrap_or(usize::MAX));
            }
        }
    }
    indices
}

/// Sidebar session list: `sessions` (newest-first, from disk) plus a
/// placeholder row for the open session when its file is not in the store
/// yet — see [`OrbitApp::sidebar_sessions`]. The placeholder carries the
/// workspace the pi process runs in, pi's live title when it has already
/// named the session, and a `now` stamp so it sorts to the top of the
/// sidebar. A session that hasn't started (`session_started` = false — no
/// user message sent yet) gets no placeholder: a draft is not listed.
pub(crate) fn sessions_with_placeholder(
    sessions: &[SessionInfo],
    current_path: Option<&Path>,
    current_title: Option<&str>,
    current_first_message: Option<&str>,
    current_workspace: Option<&Path>,
    session_started: bool,
) -> Vec<SessionInfo> {
    let mut rows = sessions.to_vec();
    let Some(path) = current_path else {
        return rows;
    };
    if rows.iter().any(|s| s.path == path) {
        return rows;
    }
    if !session_started {
        return rows;
    }
    let workspace = current_workspace
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    // The placeholder carries the same two lines a disk row would: pi's live
    // title (or the first message when pi hasn't named it yet) over the first
    // message preview.
    let first_message = cap_chars(current_first_message.unwrap_or_default(), 110);
    let title = current_title
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if first_message.is_empty() {
                tr!("menu.new_task")
            } else {
                cap_chars(current_first_message.unwrap_or_default(), 80)
            }
        });
    rows.insert(
        0,
        SessionInfo {
            path: path.to_path_buf(),
            id: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            cwd: workspace,
            title,
            first_message,
            modified: SystemTime::now(),
        },
    );
    rows
}

/// Build the grouped sidebar session list from Orbit's own project list. A
/// workspace appears because the user added it, not because pi happens to
/// have sessions there; sessions in unlisted folders are omitted entirely
/// (they stay on disk). Each group shows at most
/// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`] sessions plus one step of
/// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`] per "Show more" click, so a long
/// history never lands in one go.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_sidebar_rows(
    sessions: &[SessionInfo],
    workspaces: &[PathBuf],
    // Sidebar ordering for the workspace groups, plus the per-workspace
    // added-at stamps used by [`WorkspaceSort::DateAdded`].
    sort: WorkspaceSort,
    added_at: &HashMap<PathBuf, SystemTime>,
    working_label: &str,
    collapsed_workspaces: &HashSet<String>,
    expanded_workspace_groups: &HashSet<String>,
    expanded_session_groups: &HashMap<String, usize>,
    pinned: &HashSet<PathBuf>,
    active_path: &Option<PathBuf>,
    // Sessions with a live pi process that is currently mid-run. A running
    // session stays visible under a collapsed group header, like the open
    // session, so active work is never lost behind a collapse.
    running_paths: &HashSet<PathBuf>,
    // Sessions carrying a review mark. They stay visible under a collapsed
    // group header for the same reason running ones do — the whole point of
    // the mark is that the result is still unread, and a collapse that hides
    // it turns the mark back into a search (issue #83).
    review_paths: &HashSet<PathBuf>,
) -> Vec<SideRow> {
    let mut side_rows: Vec<SideRow> = Vec::new();
    for (label, cwd, ixs) in sidebar_groups(sessions, workspaces, sort, added_at, pinned) {
        emit_workspace_group(
            &mut side_rows,
            sessions,
            label,
            cwd,
            &ixs,
            working_label,
            collapsed_workspaces,
            expanded_workspace_groups,
            expanded_session_groups,
            pinned,
            active_path,
            running_paths,
            review_paths,
        );
    }
    side_rows
}

/// Order rank for a session row: pinned rows lead (0), everything else
/// follows (1). The sort is stable, so unpinned rows keep the store's
/// newest-first order.
///
/// Selecting a session does not change its rank. A new message moves
/// `modified`, and the next reload puts that session first among the
/// unpinned rows. A live run stays where its last message puts it; the
/// spinner and the collapsed-group reveal are separate from this rank.
fn session_lead_rank(path: &Path, pinned: &HashSet<PathBuf>) -> u8 {
    if pinned.contains(path) {
        0
    } else {
        1
    }
}

/// One group per listed project, in the order the user added them — an
/// empty project still gets a header (and its `+`) so a task can start
/// there. Groups are keyed by label, so two paths with the same basename
/// share one header (the first listed path wins). Attaches each session to
/// its project; a folder that is not listed simply finds no group. Used by
/// [`build_sidebar_rows`].
fn sidebar_groups(
    sessions: &[SessionInfo],
    workspaces: &[PathBuf],
    sort: WorkspaceSort,
    added_at: &HashMap<PathBuf, SystemTime>,
    pinned: &HashSet<PathBuf>,
) -> Vec<(String, PathBuf, Vec<usize>)> {
    let mut groups: Vec<(String, PathBuf, Vec<usize>)> = Vec::new();
    for ws in workspaces {
        let label = sessions::workspace_label(ws);
        if groups.iter().any(|(l, _, _)| *l == label) {
            continue;
        }
        groups.push((label, ws.clone(), Vec::new()));
    }
    for (ix, session) in sessions.iter().enumerate() {
        let label = sessions::workspace_label(&session.cwd);
        if let Some((_, _, ixs)) = groups.iter_mut().find(|(l, _, _)| *l == label) {
            ixs.push(ix);
        }
    }
    // Order the groups for the active sort mode before any rows are laid out;
    // a group's own session order is independent (pinned-first, then recency).
    sort_workspace_groups(&mut groups, sessions, sort, added_at);
    // Pinned sessions rise to the top of their project group; recency order
    // is preserved within each rank (the sort is stable). Doing this here
    // rather than in `load_sessions` keeps an Orbit-owned preference out of
    // the pi-store scan — and means a pin can never be hidden by the
    // per-group truncation below.
    for (_, _, ixs) in groups.iter_mut() {
        ixs.sort_by_key(|&ix| session_lead_rank(&sessions[ix].path, pinned));
    }
    groups
}

/// Lay one workspace group into rows: its header, then either the collapsed
/// always-visible rows or the capped session list with its overflow toggle.
#[allow(clippy::too_many_arguments)]
fn emit_workspace_group(
    side_rows: &mut Vec<SideRow>,
    sessions: &[SessionInfo],
    label: String,
    cwd: PathBuf,
    ixs: &[usize],
    working_label: &str,
    collapsed_workspaces: &HashSet<String>,
    expanded_workspace_groups: &HashSet<String>,
    expanded_session_groups: &HashMap<String, usize>,
    pinned: &HashSet<PathBuf>,
    active_path: &Option<PathBuf>,
    running_paths: &HashSet<PathBuf>,
    review_paths: &HashSet<PathBuf>,
) {
    let collapsed = is_workspace_group_collapsed(
        &label,
        working_label,
        collapsed_workspaces,
        expanded_workspace_groups,
    );
    side_rows.push(SideRow::Workspace {
        label: label.clone(),
        count: ixs.len(),
        collapsed,
        cwd,
    });
    if collapsed {
        // A collapsed group hides its sessions — except the open one,
        // any running (busy background) ones, and pinned ones. The live
        // session and a deliberate mark stay reachable under the header.
        // `ixs` is already leading-first, so the pinned rows keep their
        // place at the top.
        for &ix in ixs {
            let open = active_path.as_deref() == Some(sessions[ix].path.as_path());
            let running = running_paths.contains(&sessions[ix].path);
            if open
                || running
                || pinned.contains(&sessions[ix].path)
                || review_paths.contains(&sessions[ix].path)
            {
                side_rows.push(SideRow::Session(ix));
            }
        }
        return;
    }

    // Sessions start at the base cap and grow one step per "Show more"
    // click, so a group with a long history reveals three rows at a time.
    let extra = expanded_session_groups.get(&label).copied().unwrap_or(0);
    let limit = SIDEBAR_GROUP_SESSIONS_VISIBLE.saturating_add(extra);
    let visible = visible_sessions_in_group(ixs, sessions, limit, active_path);
    for ix in &visible {
        side_rows.push(SideRow::Session(*ix));
    }

    let hidden_count = ixs.len().saturating_sub(visible.len());
    if hidden_count > 0 {
        side_rows.push(SideRow::ShowMore {
            label: label.clone(),
            count: hidden_count.min(SIDEBAR_GROUP_SESSIONS_VISIBLE),
            // Past the base cap the same row carries the collapse
            // affordance on its right edge, so the group never needs a
            // second toggle row.
            can_collapse: extra > 0,
        });
    } else if extra > 0 {
        // Everything is shown — keep one quiet row to collapse the group
        // back to the base cap.
        side_rows.push(SideRow::ShowLess { label });
    }
}

/// Build the flat sidebar list: every listed project's sessions in one
/// ungrouped list, with no workspace headers, show-more, or collapse rows.
/// Pinned rows lead; the rest keep the store's newest-first order (the sort
/// is stable). Selecting a session does not move it. Used when the user
/// turns workspace grouping off; the same `SideRow::Session` rows mean the
/// render and keyboard paths are unchanged.
pub(crate) fn build_flat_sidebar_rows(
    sessions: &[SessionInfo],
    pinned: &HashSet<PathBuf>,
) -> Vec<SideRow> {
    let mut ixs: Vec<usize> = (0..sessions.len()).collect();
    // Stable: pins rise without disturbing the newest-first order of the rest.
    ixs.sort_by_key(|&ix| session_lead_rank(&sessions[ix].path, pinned));
    ixs.into_iter().map(SideRow::Session).collect()
}

/// Apply the archived-session filter. `Show` keeps every session, `Hide`
/// drops archived ones, and `Only` keeps just the archived ones. Runs before
/// row building so `SideRow::Session` indices stay aligned with the list the
/// render path receives.
pub(crate) fn filter_archived(
    sessions: Vec<SessionInfo>,
    filter: SidebarArchivedFilter,
    archived: &HashSet<PathBuf>,
) -> Vec<SessionInfo> {
    match filter {
        SidebarArchivedFilter::Show => sessions,
        SidebarArchivedFilter::Hide => sessions
            .into_iter()
            .filter(|session| !archived.contains(&session.path))
            .collect(),
        SidebarArchivedFilter::Only => sessions
            .into_iter()
            .filter(|session| archived.contains(&session.path))
            .collect(),
    }
}

/// Reorder `(label, cwd, session indices)` groups for the active sidebar
/// sort mode. `groups` is already deduped by label, so reordering never
/// merges two projects.
///
/// `LastUpdated` keys on each group's newest session activity — the group is
/// not necessarily in activity order after the pin sort, so it takes the max
/// rather than the first row. Opening a session does not move its project;
/// only a newer message does. `DateAdded` keys on the persisted added-at
/// stamp. Ties break on the label so the order is stable across reloads, and
/// unknown/empty keys sort last.
fn sort_workspace_groups(
    groups: &mut [(String, PathBuf, Vec<usize>)],
    sessions: &[SessionInfo],
    sort: WorkspaceSort,
    added_at: &HashMap<PathBuf, SystemTime>,
) {
    if sort == WorkspaceSort::Manual {
        return;
    }
    let label_key = |label: &str| label.to_lowercase();
    let by_label = |a: &(String, PathBuf, Vec<usize>), b: &(String, PathBuf, Vec<usize>)| {
        label_key(&a.0).cmp(&label_key(&b.0))
    };
    match sort {
        WorkspaceSort::Manual => {}
        WorkspaceSort::AlphabeticalAsc => groups.sort_by(|a, b| by_label(a, b)),
        WorkspaceSort::AlphabeticalDesc => groups.sort_by(|a, b| by_label(b, a)),
        WorkspaceSort::LastUpdated => groups.sort_by(|a, b| {
            group_updated(&b.2, sessions)
                .cmp(&group_updated(&a.2, sessions))
                .then_with(|| by_label(a, b))
        }),
        WorkspaceSort::DateAdded => groups.sort_by(|a, b| {
            added_at
                .get(&b.1)
                .cmp(&added_at.get(&a.1))
                .then_with(|| by_label(a, b))
        }),
        WorkspaceSort::SessionCount => {
            groups.sort_by(|a, b| b.2.len().cmp(&a.2.len()).then_with(|| by_label(a, b)))
        }
    }
}

/// Newest session activity in a group, or `None` for an empty project. Empty
/// groups sort last under [`WorkspaceSort::LastUpdated`].
fn group_updated(ixs: &[usize], sessions: &[SessionInfo]) -> Option<SystemTime> {
    ixs.iter().map(|&ix| sessions[ix].modified).max()
}

/// The scrolled-to workspace's header pinned over the session list, plus how
/// far the next group's header has pushed it up.
pub(crate) struct StickyHeader {
    /// Row index of the pinned [`SideRow::Workspace`] header.
    pub ix: usize,
    /// Pixels to shift the pinned row up (0 while it sits at the list top).
    pub top_offset: Pixels,
}

/// Resolve the sticky header for the session list: the header of the project
/// group that owns the row at the top of the viewport. Every expanded group
/// pins — not just the active one — so whichever long project the reader has
/// scrolled into keeps its name visible. The header scrolls away with its
/// section once the next group's header takes over (same behaviour as the
/// side pane's sticky file header).
pub(crate) fn sticky_sidebar_header(list: &ListState, rows: &[SideRow]) -> Option<StickyHeader> {
    let scroll_top = list.logical_scroll_top();
    // The last header at or above the top row is the pinned section. Clamp a
    // past-the-end offset (the list reads that way while following) to the
    // final row so a short list still resolves its group.
    let scan_end = scroll_top.item_ix.min(rows.len().checked_sub(1)?);
    let header_ix = rows[..=scan_end]
        .iter()
        .rposition(|row| matches!(row, SideRow::Workspace { .. }))?;
    // A collapsed group shows only its always-visible rows (the open, running,
    // pinned, or review-marked sessions), so there is no long history to label
    // — and pinning its header would mask the previous group's rows.
    if matches!(
        rows.get(header_ix),
        Some(SideRow::Workspace {
            collapsed: true,
            ..
        })
    ) {
        return None;
    }
    if scroll_top.item_ix == header_ix && scroll_top.offset_in_item <= px(0.) {
        // The real header is exactly at the top — nothing to pin over it.
        return None;
    }
    // The section ends at the next workspace header (or the list end).
    let next_header_ix = rows[header_ix + 1..]
        .iter()
        .position(|row| matches!(row, SideRow::Workspace { .. }))
        .map(|offset| header_ix + 1 + offset);
    // Push the pinned header up as the next group's header arrives. Item
    // bounds are in window coordinates, so compare against the list's
    // viewport; unmeasured items are too far away to need a push.
    let top_offset = next_header_ix
        .and_then(|next| {
            let bounds = list.bounds_for_item(next)?;
            let viewport = list.viewport_bounds();
            let y_in_viewport = bounds.origin.y - viewport.origin.y;
            (y_in_viewport < bounds.size.height).then_some(y_in_viewport - bounds.size.height)
        })
        .unwrap_or(px(0.));
    Some(StickyHeader {
        ix: header_ix,
        top_offset,
    })
}

/// The one glyph a session row spends on execution state.
///
/// The row keeps a *lane* for it — the leading 16px of every row, at the same
/// x — so the state of the whole list is one vertical scan away, instead of a
/// word at the end of each title competing with the name for the line. Glyph,
/// ink, and words resolve separately from the state, so the mapping is
/// testable without a theme and paints from exactly one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLane {
    /// A run is in flight; the glyph spins.
    Progress,
    /// The agent is blocked on a person, who is the only thing that can move it.
    Blocked(BlockReason),
    /// The last run finished and nobody has read the result yet.
    UnreadDone,
    /// A failed background run has not been opened yet.
    UnreadFailed,
    /// The completed result is visible or has been opened.
    Done,
    Failed,
    Stopped,
    /// Nothing has run yet.
    Idle,
    /// No execution state was ever observed — another client, an earlier launch.
    Unknown,
}

/// Which person the agent is waiting on. Two different problems, two glyphs: an
/// approval is a gate, a question is a conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockReason {
    Approval,
    Input,
}

/// The lane state for a row, from the two independent facts a row has: what the
/// process is doing (which may never have been observed) and whether its last
/// result was read. A live block outranks an unread mark — the agent is stuck
/// *now*, and a stale "unread" badge would bury that.
pub(crate) fn status_lane(status: Option<SessionStatus>, review: Option<ReviewMark>) -> StatusLane {
    match status {
        Some(SessionStatus::Running | SessionStatus::Retrying | SessionStatus::Compacting) => {
            StatusLane::Progress
        }
        Some(SessionStatus::WaitingApproval) => StatusLane::Blocked(BlockReason::Approval),
        Some(SessionStatus::WaitingInput) => StatusLane::Blocked(BlockReason::Input),
        Some(SessionStatus::Failed) if review == Some(ReviewMark::Failed) => {
            StatusLane::UnreadFailed
        }
        Some(SessionStatus::Failed | SessionStatus::Disconnected) => StatusLane::Failed,
        Some(SessionStatus::Stopped) => StatusLane::Stopped,
        Some(SessionStatus::Idle) => StatusLane::Idle,
        Some(SessionStatus::Completed) | None => match review {
            Some(ReviewMark::Failed) => StatusLane::UnreadFailed,
            Some(ReviewMark::Done) => StatusLane::UnreadDone,
            None if status.is_some() => StatusLane::Done,
            None => StatusLane::Unknown,
        },
    }
}

/// The lane glyph, or nothing. `Idle` and `Unknown` paint no glyph at all: a
/// session that has never run has no state to report, and a column of faint
/// dots would make the states that *do* matter harder to spot. The lane still
/// reserves its width either way, so every row's text stays on one x.
///
/// The spinner is the only glyph that moves: a run in flight is the only thing
/// on the row that is still happening.
pub(crate) fn status_lane_icon(lane: StatusLane) -> Option<&'static str> {
    Some(match lane {
        StatusLane::Progress => "icons/running-spinner.svg",
        StatusLane::Blocked(BlockReason::Approval) => "icons/lock.svg",
        StatusLane::Blocked(BlockReason::Input) => "icons/chat.svg",
        StatusLane::UnreadDone => "icons/eye.svg",
        StatusLane::Done => "icons/circle-check.svg",
        StatusLane::UnreadFailed | StatusLane::Failed => "icons/circle-x.svg",
        StatusLane::Stopped => "icons/stop.svg",
        StatusLane::Idle | StatusLane::Unknown => return None,
    })
}

/// The ink the lane glyph wears. Colour is never the only signal — the glyph's
/// shape already says what happened — but hue is what makes the column
/// scannable: a block and a failure jump, a read result recedes to mute ink.
pub(crate) fn status_lane_ink(lane: StatusLane, theme: &Theme) -> Hsla {
    match lane {
        StatusLane::Progress => theme.accent_text,
        StatusLane::Blocked(_) => theme.warning,
        StatusLane::UnreadDone | StatusLane::Done => theme.success,
        StatusLane::UnreadFailed | StatusLane::Failed => theme.error,
        StatusLane::Stopped => theme.text_muted,
        StatusLane::Idle | StatusLane::Unknown => theme.text_disabled,
    }
}

/// Attention states get words as well as a glyph, so colour is not the only
/// distinction between a result needing review and a viewed result.
pub(crate) fn status_lane_words(lane: StatusLane) -> Option<String> {
    match lane {
        StatusLane::Blocked(BlockReason::Approval) => Some(tr!("sidebar.status_approval")),
        StatusLane::Blocked(BlockReason::Input) => Some(tr!("sidebar.status_input")),
        StatusLane::UnreadDone => Some(tr!("sidebar.status_review_required")),
        StatusLane::UnreadFailed | StatusLane::Failed => Some(tr!("sidebar.status_failed")),
        _ => None,
    }
}

/// Review state takes precedence over the execution label: a completed run
/// needing attention must not announce itself as merely completed.
pub(crate) fn status_lane_tip(lane: StatusLane, status: Option<SessionStatus>) -> Option<String> {
    match lane {
        StatusLane::UnreadDone => Some(tr!("sidebar.review_finished")),
        StatusLane::UnreadFailed => Some(tr!("sidebar.review_failed")),
        StatusLane::Done => Some(tr!("sidebar.status_viewed")),
        _ => status.map(SessionStatus::label),
    }
}

/// A fixed status slot keeps the header aligned as execution state changes.
const SIDEBAR_STATUS_LANE: f32 = 16.;

/// Paint the status independently of hover, with a tooltip describing its meaning.
fn session_status_lane(
    id: usize,
    lane: StatusLane,
    status: Option<SessionStatus>,
    theme: Theme,
) -> AnyElement {
    let ink = status_lane_ink(lane, &theme);
    let size = IconSize::XSmall.px(&theme);
    let glyph: Option<AnyElement> = if lane == StatusLane::Progress {
        Some(crate::app::running_spinner(
            ElementId::NamedInteger("side-lane".into(), id as u64),
            size,
            ink,
            theme,
        ))
    } else {
        status_lane_icon(lane).map(|path| icon(path, size, ink).into_any_element())
    };
    let lane_el = div()
        .flex_none()
        .size(px(SIDEBAR_STATUS_LANE))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .children(glyph)
        .when(lane == StatusLane::UnreadFailed, |el| {
            el.child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .size(px(5.))
                    .rounded_full()
                    .bg(theme.warning),
            )
        });
    match status_lane_tip(lane, status) {
        Some(tip) => lane_el.tip(tip).into_any_element(),
        None => lane_el.into_any_element(),
    }
}

/// The two arrows a branch needs, as `(ahead, behind)` labels. A count of zero
/// has no label, and a branch with no upstream has no labels at all — `↑0 ↓0`
/// is a row of arrows that says nothing (`DESIGN.md` §Status Row).
pub(crate) fn divergence_labels(
    ahead_behind: Option<(usize, usize)>,
) -> (Option<String>, Option<String>) {
    let Some((ahead, behind)) = ahead_behind else {
        return (None, None);
    };
    (
        (ahead > 0).then(|| format!("↑{ahead}")),
        (behind > 0).then(|| format!("↓{behind}")),
    )
}

/// Whether a settled turn should leave a review mark on its row (issue #83).
///
/// `in_front` means the transcript is genuinely in front of the user right now:
/// the window is frontmost and no full-page surface covers the chat area. A
/// turn the user watched is not news — the transcript repainted in front of
/// them. Everything else leaves a mark: a background run, a parked session, or
/// a settle in the open session while Orbit sits behind another app (selecting
/// a row in a background window is not "reading" it).
pub(crate) fn should_mark_review(in_front: bool, settled: &Path, open: Option<&Path>) -> bool {
    !(in_front && open == Some(settled))
}

/// Spend the open session's review mark once that session is really on screen.
/// Returns whether anything changed, so the caller only repaints on a real
/// change. One rule for every path that can bring a session into view — a
/// click, a shortcut, the window coming forward, a page closing — keeps them
/// from drifting apart.
pub(crate) fn clear_review_mark(
    marks: &mut HashMap<PathBuf, ReviewMark>,
    open: Option<&Path>,
    in_front: bool,
) -> bool {
    if !in_front {
        return false;
    }
    open.is_some_and(|path| marks.remove(path).is_some())
}

/// One fact on a session row's metadata line: quiet, small, and never wider
/// than it needs to be.
fn meta_text(text: String, color: Hsla, theme: &Theme) -> impl IntoElement + use<> {
    div()
        .flex_none()
        .text_size(TextSize::XSmall.px(theme))
        .text_color(color)
        .child(text)
}

/// A project's glyph, in precedence order: the user's picked mark, then an
/// automatically detected logo, then `status_icon` (the workspace status resolved
/// in `view.rs`: open / running / archived / plain folder), then the plain
/// folder. A custom mark is an explicit choice, so neither a detected logo nor a
/// status glyph overrides it.
///
/// `status_icon` is resolved per frame rather than here: a workspace's status is
/// a whole-column fact (it is "running" when *any* of its sessions is), and a
/// row must not re-derive it. `None` means the map had no entry, not "no
/// status".
fn project_icon(
    mark: &WorkspaceMark,
    logo: Option<&Arc<Image>>,
    status_icon: Option<&SharedString>,
    size: IconSize,
    theme: &Theme,
) -> AnyElement {
    // Untinted projects inherit the theme accent; an explicitly neutral mark
    // stays neutral. The wash belongs to the mark, not the entire session.
    let tint = match mark.tint.as_deref() {
        None => theme.accent_text,
        key => workspace_mark::tint_color(theme, key),
    };
    let frame = div()
        .flex_none()
        .size(size.px(theme) + DynamicSpacing::Base08.px(theme))
        .rounded(Radius::Small.px(theme))
        .bg(tint.opacity(0.10))
        .flex()
        .items_center()
        .justify_center();
    let has_custom_icon = mark
        .icon
        .as_deref()
        .is_some_and(|icon| workspace_mark::PROJECT_ICONS.contains(&icon));
    if !has_custom_icon {
        if let Some(logo) = logo {
            return frame
                .child(
                    img(ImageSource::Image(logo.clone()))
                        .flex_none()
                        .size(size.px(theme))
                        .rounded(Radius::XSmall.px(theme))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element();
        }
    }
    let path = status_icon
        .cloned()
        .unwrap_or_else(|| workspace_mark::icon_path(mark.icon.as_deref()));
    frame
        .child(icon_dyn(path, size.px(theme), tint))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_side_row(
    rows: &Rc<Vec<SideRow>>,
    sessions_data: &Rc<Vec<SessionInfo>>,
    active_path: Option<&Path>,
    row_ix: usize,
    this: &Entity<OrbitApp>,
    agent_running: bool,
    running_paths: &Rc<HashSet<PathBuf>>,
    // Sessions the user pinned — they lead their project group and take a
    // small pin glyph on the title line.
    pinned_paths: &Rc<HashSet<PathBuf>>,
    // Sessions in the archive set — shown with a small archive glyph when the
    // filter includes them.
    archived_paths: &Rc<HashSet<PathBuf>>,
    // Flat "one list" mode has no workspace headers, so each row names its
    // workspace instead.
    show_workspace: bool,
    // Optional fields from the Show submenu. The title is always drawn.
    row_show: SidebarShow,
    // Every session with a live (running or warm-idle) pi process. Guards
    // delete, which would otherwise let an alive process recreate the file.
    live_paths: &Rc<HashSet<PathBuf>>,
    // Per-workspace sidebar marks (icon + tint), keyed by path.
    marks: &Rc<HashMap<PathBuf, WorkspaceMark>>,
    // Resolved per-workspace glyphs: the user's mark icon, or the folder
    // status (current / running / archived-only / idle). Computed once per
    // frame alongside the list so a header row never re-derives it.
    workspace_icons: &Rc<HashMap<PathBuf, SharedString>>,
    logos: &Rc<HashMap<PathBuf, Arc<Image>>>,
    // Branch per workspace, for the metadata line. Absent for a project that
    // is not a git repository (or sits on a detached HEAD).
    branches: &Rc<HashMap<PathBuf, BranchLine>>,
    // Sessions whose last turn settled while the user was looking elsewhere.
    // An unread glyph (plus its "Needs review" label) remains until the
    // session is shown in a foreground window (issue #83); the execution state
    // remains after that mark is spent.
    review_marks: &Rc<HashMap<PathBuf, ReviewMark>>,
    session_statuses: &Rc<HashMap<PathBuf, SessionStatus>>,
    session_menu: Option<&SessionMenu>,
    workspace_menu: Option<&WorkspaceMenu>,
    // Whether this row is the keyboard cursor (see `OrbitApp::sidebar_cursor`).
    // Painted with a focused surface, distinct from hover and selection.
    cursor: bool,
    theme: Theme,
) -> impl IntoElement {
    match &rows[row_ix] {
        SideRow::Workspace {
            label,
            count,
            collapsed,
            cwd,
        } => {
            let label = label.clone();
            let label_for_click = label.clone();
            let cwd_for_new = cwd.clone();
            let this_toggle = this.clone();
            let this_new = this.clone();
            let this_menu = this.clone();
            let menu = workspace_menu.filter(|m| m.label == label);
            let mark = marks.get(cwd).cloned().unwrap_or_default();
            let workspace_logo = logos.get(cwd);
            let workspace_icon = workspace_icons.get(cwd);
            // Outer shell: inter-group spacing only — horizontal inset comes
            // from the list's `px_2`, so the hover pill lines up with the
            // session rows' (inside the same container) and the chevron lands
            // under the "Projects" label. Hover lives on the inner card so the
            // highlight doesn't bleed into the padding. The header is the
            // parent row above its sessions: chevron + mark + name, the
            // session count pinned to the very end, and the row actions (a `…`
            // menu, then the new-session `+`) fading in to the count's left on
            // hover (all flex_none, so nothing shifts when they appear). The
            // name and tinted mark distinguish the group from its sessions;
            // the count stays visible while the contextual actions reveal.
            div()
                .w_full()
                .pt(DynamicSpacing::Base12.px(&theme))
                .pb(DynamicSpacing::Base02.px(&theme))
                .group("workspace-row")
                .child(
                    div()
                        .w_full()
                        .min_h(ButtonSize::Default.height(&theme))
                        .pl(DynamicSpacing::Base06.px(&theme))
                        .pr(DynamicSpacing::Base08.px(&theme))
                        .rounded(list_item::RADIUS.px(&theme))
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .cursor_pointer()
                        .border_1()
                        .border_color(theme.border.opacity(0.))
                        .when(cursor, |s| {
                            s.bg(theme.surface_hover).border_color(theme.focus_ring)
                        })
                        .hover(|s| s.bg(theme.surface_hover))
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            let label = label_for_click.clone();
                            this_toggle.update(cx, |app, cx| {
                                let working = app.workspace_label();
                                toggle_workspace_group(
                                    label,
                                    &working,
                                    &mut app.collapsed_workspaces,
                                    &mut app.expanded_workspace_groups,
                                );
                                cx.notify();
                            });
                        })
                        // Right-click anywhere on the header opens its
                        // actions menu at the pointer, context-menu style.
                        .on_mouse_down(MouseButton::Right, {
                            let label = label.clone();
                            let cwd = cwd.clone();
                            let this = this.clone();
                            move |event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                let menu = WorkspaceMenu {
                                    label: label.clone(),
                                    cwd: cwd.clone(),
                                    at: Some(event.position),
                                    picker: false,
                                };
                                this.update(cx, |app, cx| {
                                    app.open_workspace_menu_at(menu, window, cx);
                                });
                            }
                        })
                        .child(icon(
                            if *collapsed {
                                "icons/chevron-right.svg"
                            } else {
                                "icons/chevron-down.svg"
                            },
                            IconSize::XSmall.px(&theme),
                            theme.text_muted,
                        ))
                        .child(project_icon(
                            &mark,
                            workspace_logo,
                            workspace_icon,
                            IconSize::Medium,
                            &theme,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(TextSize::Default.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                // Keep the user's project hue on the name as
                                // well as its mark (issue #78).
                                .text_color(workspace_mark::label_ink(&theme, mark.tint.as_deref()))
                                .child(label.clone()),
                        )
                        // Row actions, revealed on hover: a `…` menu
                        // (remove / copy path) beside the direct new-task `+`.
                        .child(workspace_menu_button(
                            label.clone(),
                            cwd.clone(),
                            menu,
                            mark.clone(),
                            this_menu.clone(),
                            theme,
                        ))
                        .child(
                            icon_button_frame(
                                div().id(ElementId::Name(format!("workspace-new-{label}").into())),
                                &theme,
                                ButtonSize::Compact,
                            )
                            .tip(crate::commands::tooltip(
                                crate::commands::CommandId::NewSession,
                                false,
                            ))
                            .cursor_pointer()
                            // Revealed on row hover — the quiet default
                            // keeps group headers to just label + count.
                            .opacity(0.)
                            .group_hover("workspace-row", |s| s.opacity(1.))
                            .hover(|s| s.bg(theme.surface_hover))
                            .on_mouse_up(MouseButton::Left, {
                                let this = this_new.clone();
                                move |_, window, cx| {
                                    cx.stop_propagation();
                                    let cwd = cwd_for_new.clone();
                                    this.update(cx, |app, cx| {
                                        app.on_new_session_in_workspace(cwd, window, cx);
                                    });
                                }
                            })
                            .child(icon(
                                "icons/plus.svg",
                                ButtonSize::Compact.icon_size().px(&theme),
                                theme.text_muted,
                            )),
                        )
                        // Session count, pinned to the header's right edge.
                        // A small tonal counter, subordinate to the name. A project with
                        // nothing in it says so by having no rows under it;
                        // a `0` here would only be a number doing no work.
                        .when(*count > 0, |header| {
                            header.child(
                                div()
                                    .flex_none()
                                    .min_w(ButtonSize::Compact.height(&theme))
                                    .px(DynamicSpacing::Base04.px(&theme))
                                    .py(DynamicSpacing::Base02.px(&theme))
                                    .rounded(Radius::Small.px(&theme))
                                    .bg(theme.accent.opacity(0.08))
                                    .text_center()
                                    .text_size(TextSize::XSmall.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_secondary)
                                    .child(format!("{count}")),
                            )
                        }),
                )
                .into_any_element()
        }
        SideRow::ShowMore {
            label,
            count,
            can_collapse,
        } => {
            let this = this.clone();
            let label_for_click = label.clone();
            // One click reveals exactly one step (or the tail remainder), so
            // the group can never overshoot its session count.
            let step = *count;
            let can_collapse = *can_collapse;
            let this_for_collapse = this.clone();
            let label_for_collapse = label.clone();
            // Left side is the whole "show more" target; the collapse
            // chevron on the right is its own quiet button.
            let more = div()
                .flex_1()
                .h_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .cursor_pointer()
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let label = label_for_click.clone();
                    this.update(cx, |app, cx| {
                        app.expanded_session_groups
                            .entry(label)
                            .and_modify(|extra| *extra += step)
                            .or_insert(step);
                        cx.notify();
                    });
                })
                .child(icon(
                    "icons/chevron-down.svg",
                    IconSize::XSmall.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("sidebar.show_count_more", count = count)),
                );
            let mut row = div()
                .w_full()
                .h(px(26.))
                .pl(px(22.))
                .pr(DynamicSpacing::Base04.px(&theme))
                .flex()
                .items_center()
                .rounded(list_item::RADIUS.px(&theme))
                .when(cursor, |s| s.bg(theme.surface_hover))
                .hover(|s| s.bg(theme.surface_hover))
                .child(more);
            if can_collapse {
                row = row.child(
                    icon_button_frame(div(), &theme, ButtonSize::Compact)
                        .tip(tr!("sidebar.show_less"))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.surface_hover))
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let label = label_for_collapse.clone();
                            this_for_collapse.update(cx, |app, cx| {
                                app.expanded_session_groups.remove(&label);
                                cx.notify();
                            });
                        })
                        .child(icon(
                            "icons/chevron-up.svg",
                            IconSize::XSmall.px(&theme),
                            theme.text_muted,
                        )),
                );
            }
            row.into_any_element()
        }
        SideRow::ShowLess { label } => {
            let this = this.clone();
            let label_for_click = label.clone();
            div()
                .w_full()
                .h(px(26.))
                .pl(px(22.))
                .pr(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .rounded(list_item::RADIUS.px(&theme))
                .cursor_pointer()
                .when(cursor, |s| s.bg(theme.surface_hover))
                .hover(|s| s.bg(theme.surface_hover))
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let label = label_for_click.clone();
                    this.update(cx, |app, cx| {
                        app.expanded_session_groups.remove(&label);
                        cx.notify();
                    });
                })
                .child(icon(
                    "icons/chevron-up.svg",
                    IconSize::XSmall.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("sidebar.show_less")),
                )
                .into_any_element()
        }
        SideRow::Session(ix) => {
            let session = sessions_data[*ix].clone();
            let session_for_click = session.clone();
            let active = active_path == Some(session.path.as_path());
            // The row's place among the visible sessions is the number `⌘1…⌘9`
            // opens it by — the same order `navigable_sessions` cycles.
            let slot = rows[..row_ix]
                .iter()
                .filter(|row| matches!(row, SideRow::Session(_)))
                .count()
                + 1;
            let shortcut_hint = (slot <= 9).then(|| {
                tr!(
                    "sidebar.open_with_shortcut",
                    keys = crate::platform::shortcuts::label(&format!("secondary-{slot}"))
                )
            });
            // The open session runs live; parked (background) sessions run
            // in their own pi processes — both get the loader.
            let running = (active && agent_running) || running_paths.contains(&session.path);
            // Files from other clients or earlier launches have no observed
            // execution state. Do not label them idle or completed by guessing.
            let status = session_statuses.get(&session.path).copied();
            let pinned = pinned_paths.contains(&session.path);
            let archived = archived_paths.contains(&session.path);
            let this = this.clone();
            let this_for_row = this.clone();
            let this_for_menu = this.clone();
            let menu = session_menu.filter(|m| m.path == session.path);
            // Sessions with a live pi process (running or warm) must not be
            // deleted — the process would recreate the file mid-run.
            let deletable = !active && !running && !live_paths.contains(&session.path);
            // Keep titles still; the status icon communicates background activity.
            let title = session_title(*ix, session.title.clone().into(), theme, active, false);
            // Indented under its workspace group so the list reads as a
            // tree. The open session takes the `selection` fill with `selection_fg`
            // ink — the same selected-destination grammar as the nav rows;
            // row actions are revealed on hover. The first-message line is
            // the Show → Summary field, off unless the user turns it on, so
            // every grouped row is the same height.
            // Outer item carries the inter-row spacing (padding) and the click
            // handler; the inner card holds the background/hover so the gap
            // between cards stays clear. Padding (not margin) is used because
            // the list measures each item's border-box — margins are dropped.
            let mut row = div()
                .id(ElementId::NamedInteger("side-session".into(), *ix as u64))
                .w_full()
                .py(DynamicSpacing::Base02.px(&theme))
                .cursor_pointer()
                .when_some(shortcut_hint, |row, hint| row.tip(hint))
                // Right-click anywhere on the row opens its actions menu at
                // the pointer. That context menu is the row's only action
                // surface — there is no hover-revealed '…' to compete with
                // the title for the line.
                .on_mouse_down(MouseButton::Right, {
                    let session = session.clone();
                    let this = this.clone();
                    move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.update(cx, |app, cx| {
                            app.open_session_menu_at(
                                SessionMenu {
                                    path: session.path.clone(),
                                    title: session.title.clone(),
                                    deletable,
                                    confirm_delete: false,
                                    at: Some(event.position),
                                },
                                window,
                                cx,
                            );
                        });
                    }
                })
                .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                    let session = session_for_click.clone();
                    this_for_row.update(cx, |app, cx| {
                        // A click is a mouse hand-off: leave keyboard nav and
                        // hand focus back to the composer so typing resumes.
                        app.sidebar_cursor = None;
                        app.on_open_session(session, cx);
                        app.input.read(cx).focus(window);
                    });
                });
            // Status sits beside the task; workspace and branch sit below it.
            let mut card = div()
                .group("srow")
                .relative()
                .w_full()
                .px(DynamicSpacing::Base08.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .rounded(list_item::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.border.opacity(0.))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .when(active, |card| {
                    card.bg(theme.surface_selected)
                        .border_color(theme.accent.opacity(0.40))
                        .text_color(theme.selection_fg)
                })
                // Focus remains visible even on the selected session. Reserve
                // the border in every state so hover/focus never shifts text.
                .when(cursor, |card| {
                    card.border_color(theme.focus_ring)
                        .when(!active, |card| card.bg(theme.surface_hover))
                })
                .when(!active && !cursor, |card| {
                    card.hover(|s| s.bg(theme.surface_hover))
                });
            let age = sessions::relative_time(session.modified);
            // Summary is a whole-list choice, not a per-row guess, so every
            // row grows by the same line when it is on.
            let show_summary = row_show.summary;
            let show_updated = row_show.updated;
            // When enabled, line 2 is the prompt itself: the title is capped
            // and the row is narrower still, so a title that happens to be the
            // same sentence is cut, not redundant — the message line is the
            // only place the ask is readable.
            let message = session.first_message.trim().to_string();
            // Flat list has no workspace headers — name the workspace on the
            // row instead, with the workspace-status glyph the headers use
            // (open / running / archived). This row's own run is the status
            // lane's job, one column to the right.
            let workspace_label = sessions::workspace_label(&session.cwd);
            let mark = marks.get(&session.cwd).cloned().unwrap_or_default();
            let workspace_logo = logos.get(&session.cwd);
            let workspace_icon = workspace_icons.get(&session.cwd);
            let branch = branches.get(&session.cwd);
            let providers = sessions::cached_session_providers(&session.path);
            let review = review_marks
                .get(&session.path)
                .copied()
                // A run in flight supersedes the mark: the lane's spinner is
                // the current truth, and the settle that ends this run sets a
                // fresh one.
                .filter(|_| !running);
            // Resolve the glyph and attention label from the same state.
            let lane = status_lane(status, review);
            let meta_ink = if active {
                theme.text_secondary
            } else {
                theme.text_muted
            };
            // Keep the status or age visible even while the pointer is over the row.
            let slot_words = if status == Some(SessionStatus::Disconnected) {
                Some(SessionStatus::Disconnected.label())
            } else {
                status_lane_words(lane)
            };
            let slot_ink = if slot_words.is_some() {
                status_lane_ink(lane, &theme)
            } else {
                meta_ink
            };
            let has_state = !matches!(lane, StatusLane::Idle | StatusLane::Unknown);
            let row_slot = div()
                .flex_none()
                .max_w(px(110.))
                .truncate()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(slot_ink)
                .child(slot_words.unwrap_or_else(|| {
                    if show_updated {
                        age
                    } else {
                        String::new()
                    }
                }));
            let status_details = Some(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .when(pinned, |line| {
                        line.child(icon(
                            "icons/pin.svg",
                            IconSize::XSmall.px(&theme),
                            theme.warning,
                        ))
                    })
                    .when(archived, |line| {
                        line.child(icon(
                            "icons/archive.svg",
                            IconSize::XSmall.px(&theme),
                            meta_ink,
                        ))
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base02.px(&theme))
                            .px(DynamicSpacing::Base04.px(&theme))
                            .py(DynamicSpacing::Base01.px(&theme))
                            .rounded(Radius::Small.px(&theme))
                            .when(has_state, |badge| {
                                badge.bg(status_lane_ink(lane, &theme).opacity(0.10))
                            })
                            .child(session_status_lane(*ix, lane, status, theme))
                            .child(row_slot),
                    ),
            );
            card = card.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    // Keep session indicators beside the title.
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(title)
                            .children(status_details)
                            .when(active && show_workspace, |line| {
                                line.child(
                                    div()
                                        .flex_none()
                                        .px(DynamicSpacing::Base04.px(&theme))
                                        .py(DynamicSpacing::Base01.px(&theme))
                                        .rounded(Radius::Small.px(&theme))
                                        .bg(theme.accent.opacity(0.10))
                                        .text_size(TextSize::XSmall.px(&theme))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.accent_text)
                                        .child(tr!("sidebar.current")),
                                )
                            }),
                    )
                    // Line 2 — what was actually asked, one line. An empty
                    // message (a session with nothing sent yet) drops the line.
                    // The text sits in a flex row with `flex_1 min_w_0` like the
                    // title: `truncate()` on a bare block collapses the line to
                    // its ellipsis instead of filling the column.
                    .when(show_summary && !message.is_empty(), |col| {
                        col.child(
                            div().w_full().flex().items_center().child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(TextSize::Small.px(&theme))
                                    .line_height(BufferLineHeight::Standard.relative())
                                    .text_color(theme.text_secondary)
                                    .tip(message.clone())
                                    .child(message),
                            ),
                        )
                    })
                    // In the flat list, the workspace precedes the branch.
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .when(show_workspace, |line| {
                                line.child(
                                    div()
                                        .max_w(px(110.))
                                        .min_w_0()
                                        .flex()
                                        .flex_shrink()
                                        .items_center()
                                        .gap(DynamicSpacing::Base04.px(&theme))
                                        .child(project_icon(
                                            &mark,
                                            workspace_logo,
                                            workspace_icon,
                                            IconSize::XSmall,
                                            &theme,
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(TextSize::XSmall.px(&theme))
                                                .text_color(workspace_mark::label_ink(
                                                    &theme,
                                                    mark.tint.as_deref(),
                                                ))
                                                .tip(workspace_label.clone())
                                                .child(workspace_label),
                                        ),
                                )
                            })
                            .when_some(branch, |line, branch| {
                                let (ahead, behind) = divergence_labels(branch.ahead_behind);
                                line.child(icon(
                                    "icons/branch.svg",
                                    IconSize::XSmall.px(&theme),
                                    theme.syn_function,
                                ))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(TextSize::XSmall.px(&theme))
                                        .text_color(meta_ink)
                                        .tip(branch.name.clone())
                                        .child(branch.name.clone()),
                                )
                                // Colour is never the only signal: the arrow
                                // says which way the counts point, and both
                                // read as text before they read as hue.
                                .when_some(ahead, |line, ahead| {
                                    line.child(meta_text(ahead, theme.success, &theme))
                                })
                                .when_some(
                                    behind,
                                    |line, behind| {
                                        line.child(meta_text(behind, theme.warning, &theme))
                                    },
                                )
                            })
                            .when(branch.is_none(), |line| line.child(div().flex_1()))
                            .when(!providers.is_empty(), |line| {
                                let remaining = providers.len().saturating_sub(3);
                                let all_providers = providers
                                    .iter()
                                    .map(|entry| match &entry.model {
                                        Some(model) => format!("{} · {model}", entry.provider),
                                        None => entry.provider.clone(),
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                line.child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .children(providers.iter().take(3).enumerate().map(
                                            |(index, entry)| {
                                                let tint = provider_mark_color(
                                                    &entry.provider,
                                                    &theme,
                                                    meta_ink,
                                                );
                                                let label = match &entry.model {
                                                    Some(model) => {
                                                        format!("{} · {model}", entry.provider)
                                                    }
                                                    None => entry.provider.clone(),
                                                };
                                                let visible_count = providers.len().min(3);
                                                // Each badge owns its exposed portion as a
                                                // hover target; the last owns its full width.
                                                div()
                                                    .relative()
                                                    .flex_none()
                                                    .w(if index + 1 == visible_count {
                                                        IconSize::Medium.square(&theme)
                                                    } else {
                                                        IconSize::Small.px(&theme)
                                                    })
                                                    .h(IconSize::Medium.square(&theme))
                                                    .tip(label)
                                                    .child(
                                                        div()
                                                            .absolute()
                                                            .left_0()
                                                            .top_0()
                                                            .size(IconSize::Medium.square(&theme))
                                                            .rounded_full()
                                                            .bg(theme.surface)
                                                            .border_1()
                                                            .border_color(tint.opacity(0.35))
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(icon_dyn(
                                                                provider_icon(&entry.provider),
                                                                IconSize::Small.px(&theme),
                                                                tint,
                                                            )),
                                                    )
                                            },
                                        ))
                                        .when(remaining > 0, |icons| {
                                            icons.child(
                                                div()
                                                    .tip(all_providers)
                                                    .text_size(TextSize::XSmall.px(&theme))
                                                    .text_color(meta_ink)
                                                    .child(format!("+{remaining}")),
                                            )
                                        }),
                                )
                            }),
                    ),
            );
            // Row actions live on the right-click context menu — the same
            // gesture the rest of the app uses for row actions — so there is
            // no hover-revealed '…' and the title owns the whole line. The
            // popup still needs a stable element to hang off: a zero-size
            // anchor at the card's top-left, which is what the keyboard and
            // palette paths (no pointer origin) anchor to.
            card = card.children(menu.map(|menu| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(0.))
                    .child(session_menu_popup(menu, this_for_menu.clone(), theme))
            }));
            row = row.child(card);
            row.into_any_element()
        }
    }
}

/// A session row's title. Quiet rows render as one truncated line; a running
/// row paints the shadcn `shimmer` — a highlight band sweeping across the
/// glyphs — driven by `with_animation` (self-repainting, no app tick).
fn session_title(
    id: usize,
    title: SharedString,
    theme: Theme,
    active: bool,
    running: bool,
) -> AnyElement {
    let color = theme.text_primary;
    let weight = if active {
        FontWeight::SEMIBOLD
    } else {
        FontWeight::MEDIUM
    };
    let size = TextSize::Default.px(&theme);
    let line_height = BufferLineHeight::Standard.relative();
    if !running {
        return div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(size)
            .line_height(line_height)
            .font_weight(weight)
            .text_color(color)
            .tip(title.clone())
            .child(title)
            .into_any_element();
    }
    // Reduce-motion: keep the running signal (accent title) without the
    // perpetual sweep.
    if theme.ui.reduce_motion {
        return div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(size)
            .line_height(line_height)
            .font_weight(weight)
            .text_color(theme.accent_text)
            .child(title)
            .into_any_element();
    }
    // The sweep catches the accent: ink at the edges, accent through the band
    // (shadcn's highlight, spent on the one hue the system rations).
    let highlight = theme.accent;
    div()
        .flex_1()
        .min_w_0()
        .text_size(size)
        .line_height(line_height)
        .font_weight(weight)
        .child(ShimmerText::new(title, color, highlight).with_animation(
            ElementId::NamedInteger("side-shimmer".into(), id as u64),
            Animation::new(Duration::from_millis(2000)).repeat(),
            |mut text, delta| {
                text.phase = delta;
                text
            },
        ))
        .into_any_element()
}

// ── sidebar row-actions popup ──────────────────────────────────────────

/// The actions popup anchored to a session row: Copy path / Reveal in
/// Finder / Delete, or the delete confirmation once armed. Painted via
/// `deferred` + `anchored` (same convention as the composer pickers),
/// dismissed by any outside mouse-down.
pub(crate) fn session_menu_popup(
    menu: &SessionMenu,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let deletable = menu.deletable;
    let confirm = menu.confirm_delete;
    let pinned = crate::pins::contains(&menu.path);
    let archived = crate::archive::contains(&menu.path);

    let body: AnyElement = if confirm {
        // Delete confirmation — the destructive step gets a named victim.
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(tr!("sidebar.delete_this_session")),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!("sidebar.removes_the_session_file_from_disk")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(
                        button_frame(div().id("menu-cancel"), &theme, ButtonSize::Default)
                            .raised(theme.surface_elevated, &theme)
                            .cursor_pointer()
                            .hover(|s| s.raised(theme.surface_hover, &theme))
                            .text_color(theme.text_secondary)
                            .on_mouse_down(MouseButton::Left, {
                                let this = this.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    this.update(cx, |app, cx| app.on_menu_cancel(cx));
                                }
                            })
                            .child(tr!("sidebar.cancel")),
                    )
                    .child(
                        button_frame(div().id("menu-confirm-delete"), &theme, ButtonSize::Default)
                            .raised(theme.error, &theme)
                            .cursor_pointer()
                            .hover(|s| s.raised(theme.error_hover, &theme))
                            .text_color(theme.error_fg)
                            .on_mouse_down(MouseButton::Left, {
                                let this = this.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    this.update(cx, |app, cx| app.on_menu_delete_confirm(cx));
                                }
                            })
                            .child(tr!("sidebar.delete")),
                    ),
            )
            .into_any_element()
    } else {
        div()
            .w_full()
            .flex()
            .flex_col()
            .py(picker::list_padding_y(&theme))
            .gap(DynamicSpacing::Base01.px(&theme))
            .child(
                picker_entry(div().id("menu-rename"), &theme)
                    .h(picker::entry_height(&theme))
                    .flex_none()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay))
                    .text_color(theme.text_secondary)
                    .on_mouse_down(MouseButton::Left, {
                        let this = this.clone();
                        move |_, window, cx| {
                            cx.stop_propagation();
                            this.update(cx, |app, cx| app.on_menu_rename_session(window, cx));
                        }
                    })
                    .child(icon(
                        "icons/pencil.svg",
                        context_menu::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(tr!("explorer.rename")),
                    ),
            )
            .child(menu_item(
                "menu-pin",
                "icons/pin.svg",
                if pinned {
                    tr!("sidebar.unpin_session")
                } else {
                    tr!("sidebar.pin_session")
                },
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_toggle_pin(cx),
            ))
            .child(menu_item(
                "menu-archive",
                "icons/archive.svg",
                if archived {
                    tr!("sidebar.unarchive_session")
                } else {
                    tr!("sidebar.archive_session")
                },
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_toggle_archive(cx),
            ))
            .child(menu_item(
                "menu-copy-path",
                "icons/copy.svg",
                tr!("sidebar.copy_path"),
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_copy_path(cx),
            ))
            .child(menu_item(
                "menu-reveal",
                "icons/folder.svg",
                tr!("sidebar.reveal_in_finder"),
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_reveal(cx),
            ))
            .child(menu_item(
                "menu-clone",
                "icons/git-fork.svg",
                tr!("sidebar.clone_session"),
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_clone_session(cx),
            ))
            .when(deletable, |menu| {
                menu.child(context_menu_separator(&theme)).child(menu_item(
                    "menu-delete",
                    "icons/trash.svg",
                    tr!("sidebar.delete_session_menu"),
                    theme,
                    this.clone(),
                    true,
                    |app, cx| app.on_menu_delete_request(cx),
                ))
            })
            .into_any_element()
    };

    // The branch selector's picker shell sizes the action list to its
    // entries; the delete confirmation keeps its own fixed card.
    let popup = picker_surface(div(), &theme)
        .when(confirm, |pop| {
            pop.w(px(210.)).p(DynamicSpacing::Base12.px(&theme))
        })
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        // Any mouse-down outside dismisses; clicks inside are stopped by
        // the item handlers, so they never read as "outside".
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at = Some(Instant::now());
                    app.session_menu = None;
                    cx.notify();
                })
            }
        })
        .child(body);

    // Float the popup: `anchored` takes it out of the layout (no other row
    // moves). A right-clicked menu lands at the pointer in window
    // coordinates; a menu opened from the keyboard or the command palette
    // (no pointer origin) drops just under the row's title line, left-aligned
    // to its card — the standard app dropdown position. `deferred` paints it
    // above the rest of the list — same convention as the composer chip
    // pickers. `snap_to_window` keeps it inside the window near the edges.
    let anchor = if let Some(at) = menu.at {
        anchored().anchor(Corner::TopLeft).position(at)
    } else {
        anchored()
            .position_mode(AnchoredPositionMode::Local)
            .anchor(Corner::TopLeft)
            .offset(point(
                px(0.),
                TextSize::Default.px(&theme) + DynamicSpacing::Base06.px(&theme),
            ))
    };
    anchor
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(deferred(popup))
        .into_any_element()
}

/// The hover-revealed '…' button on a workspace group header. Clicking it
/// opens the header's actions popup; propagation stops so the header's
/// collapse toggle doesn't also fire.
pub(crate) fn workspace_menu_button(
    label: String,
    cwd: PathBuf,
    menu: Option<&WorkspaceMenu>,
    mark: WorkspaceMark,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let this_for_popup = this.clone();
    let label_for_click = label.clone();
    let cwd_for_click = cwd.clone();
    icon_button_frame(
        div().id(ElementId::Name(format!("workspace-more-{label}").into())),
        &theme,
        ButtonSize::Compact,
    )
    .group(BUTTON_GROUP)
    .relative()
    .tip(tr!("sidebar.more_actions"))
    .cursor_pointer()
    // Revealed on row hover, or while this header's menu is open.
    .opacity(if menu.is_some() { 1.0 } else { 0.0 })
    .group_hover("workspace-row", |s| s.opacity(1.))
    .active(|s| s.opacity(PRESS_DIM))
    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        let menu = WorkspaceMenu {
            label: label_for_click.clone(),
            cwd: cwd_for_click.clone(),
            at: None,
            picker: false,
        };
        this.update(cx, |app, cx| app.toggle_workspace_menu(menu, window, cx));
    })
    .child(icon(
        "icons/more.svg",
        ButtonSize::Compact.icon_size().px(&theme),
        theme.text_muted,
    ))
    .children(menu.map(|menu| {
        div()
            .absolute()
            .top_0()
            .left_0()
            .size(px(0.))
            .child(workspace_menu_popup(
                menu,
                mark.clone(),
                this_for_popup.clone(),
                theme,
            ))
    }))
}

/// One row of the workspace header menu, on the branch selector's picker
/// metrics ([`picker_entry`]): a leading glyph beside the label, with the
/// picker row's inset, padding, height, and radius. The click closes the menu
/// through the app action it runs.
fn workspace_menu_item<C, L>(
    id: &'static str,
    icon_path: &'static str,
    label: L,
    theme: Theme,
    this: Entity<OrbitApp>,
    on_click: C,
) -> impl IntoElement + use<C, L>
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let label: SharedString = label.into();
    picker_entry(div().id(id), &theme)
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_pointer()
        .hover(|s| s.bg(theme.surface_hover))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| (on_click)(app, cx));
        })
        .child(icon(
            icon_path,
            context_menu::ICON.px(&theme),
            theme.text_muted,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.text_secondary)
                .child(label),
        )
}

/// The actions popup anchored to a workspace header: Copy path / Icon & color /
/// Remove from sidebar, or — once `Icon & color` is chosen — the picker itself.
/// Uses the branch selector's picker surface and row metrics; deferred and
/// anchored like the session row menu, dismissed by any outside mouse-down.
pub(crate) fn workspace_menu_popup(
    menu: &WorkspaceMenu,
    mark: WorkspaceMark,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let body: AnyElement = if menu.picker {
        workspace_appearance_picker(menu, mark, this.clone(), theme)
    } else {
        // The action rows wear the branch selector's picker metrics — the
        // same Base04 inset, Base06 inner padding, 31px row, and Base01 row
        // gap — so the workspace menu and the branch/sort popovers read as
        // one family.
        div()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            .child(workspace_menu_item(
                "wm-copy-path",
                "icons/copy.svg",
                tr!("sidebar.copy_path"),
                theme,
                this.clone(),
                |app, cx| app.on_workspace_copy_path(cx),
            ))
            .child(workspace_menu_item(
                "wm-worktrees",
                "icons/branch.svg",
                tr!("worktree.page.title"),
                theme,
                this.clone(),
                |app, cx| {
                    app.workspace_menu = None;
                    app.open_worktrees(cx);
                },
            ))
            .child(workspace_menu_item(
                "wm-icon-color",
                "icons/contrast.svg",
                tr!("sidebar.icon_and_color"),
                theme,
                this.clone(),
                |app, cx| app.on_workspace_open_picker(cx),
            ))
            .child(workspace_menu_item(
                "wm-remove",
                "icons/minus.svg",
                tr!("sidebar.remove_from_sidebar"),
                theme,
                this.clone(),
                |app, cx| app.on_workspace_remove(cx),
            ))
            .into_any_element()
    };

    // Same picker surface as the branch selector (and the sidebar's sort
    // menu). One fixed width across both modes means opening the icon picker
    // never resizes the popover.
    let popup = picker_surface(div(), &theme)
        .w(workspace_picker_width(&theme))
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at = Some(Instant::now());
                    app.workspace_menu = None;
                    cx.notify();
                })
            }
        })
        .child(body);

    let anchor = if let Some(at) = menu.at {
        anchored().anchor(Corner::TopLeft).position(at)
    } else {
        anchored()
            .position_mode(AnchoredPositionMode::Local)
            .anchor(Corner::TopLeft)
            .offset(point(
                px(0.),
                ButtonSize::Compact.height(&theme) + popover::MENU_OFFSET,
            ))
    };
    anchor
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(deferred(popup))
        .into_any_element()
}

/// A grid cell's side: a square hit target that holds an [`IconSize::Medium`]
/// glyph with even padding all round.
fn workspace_picker_cell(theme: &Theme) -> Pixels {
    DynamicSpacing::Base24.px(theme)
}

/// Spacing between grid cells and tint swatches.
fn workspace_picker_gap(theme: &Theme) -> Pixels {
    DynamicSpacing::Base06.px(theme)
}

/// The inset from the surface edge to the picker's content. Used for the
/// header, swatches, and grid alike so their left and right edges line up.
fn workspace_picker_padding_x(theme: &Theme) -> Pixels {
    DynamicSpacing::Base16.px(theme)
}

/// The width the picker surface needs — the icon grid plus its horizontal
/// padding, so the content fills the popup exactly with no trailing gap.
fn workspace_picker_width(theme: &Theme) -> Pixels {
    let columns = workspace_mark::ICON_COLUMNS as f32;
    workspace_picker_cell(theme) * columns
        + workspace_picker_gap(theme) * (columns - 1.)
        + workspace_picker_padding_x(theme) * 2.
}

/// The icon-and-color picker shown in place of the workspace menu: a row of
/// semantic tint dots over a grid of curated HugeIcons. Picking either writes
/// the mark and persists it; `Reset` clears both back to the folder default.
fn workspace_appearance_picker(
    menu: &WorkspaceMenu,
    mark: WorkspaceMark,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let cell = workspace_picker_cell(&theme);
    let gap = workspace_picker_gap(&theme);

    let mut dots = div().flex().w_full().justify_between();
    for (key, color_of) in workspace_mark::TINTS {
        let selected = mark
            .tint
            .as_deref()
            .map_or(*key == "default", |tint| tint == *key);
        let cwd = menu.cwd.clone();
        let mark = mark.clone();
        let this = this.clone();
        dots = dots.child(
            div()
                .id(ElementId::Name(format!("wm-tint-{key}").into()))
                .size(IconSize::Medium.px(&theme))
                .flex_none()
                .rounded_full()
                .bg(color_of(&theme))
                .border_2()
                .border_color(if selected {
                    theme.text_primary
                } else {
                    theme.border_strong
                })
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let next = WorkspaceMark {
                        icon: mark.icon.clone(),
                        tint: Some((*key).to_string()),
                    };
                    this.update(cx, |app, cx| {
                        app.set_workspace_mark(cwd.clone(), next);
                        cx.notify();
                    });
                }),
        );
    }

    let ink = workspace_mark::tint_color(&theme, mark.tint.as_deref());
    let mut grid = div().flex().flex_col().gap(gap);
    for row in workspace_mark::PROJECT_ICONS.chunks(workspace_mark::ICON_COLUMNS) {
        let mut cells = div().flex().gap(gap);
        for stem in row {
            let selected = mark
                .icon
                .as_deref()
                .map_or(*stem == "folder", |icon| icon == *stem);
            let cwd = menu.cwd.clone();
            let mark = mark.clone();
            let this = this.clone();
            cells = cells.child(
                div()
                    .id(ElementId::Name(format!("wm-icon-{stem}").into()))
                    .size(cell)
                    .flex_none()
                    .rounded(list_item::RADIUS.px(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(selected, |s| s.bg(theme.surface_selected))
                    .hover(|s| s.bg(theme.surface_hover))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let next = WorkspaceMark {
                            icon: Some((*stem).to_string()),
                            tint: mark.tint.clone(),
                        };
                        this.update(cx, |app, cx| {
                            app.set_workspace_mark(cwd.clone(), next);
                            cx.notify();
                        });
                    })
                    .child(icon_dyn(
                        workspace_mark::icon_path(Some(stem)),
                        IconSize::Medium.px(&theme),
                        ink,
                    )),
            );
        }
        grid = grid.child(cells);
    }

    let reset_cwd = menu.cwd.clone();
    let this_reset = this.clone();
    let header = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(tr!("sidebar.icon_and_color")),
        )
        .child(
            div()
                .id("wm-mark-reset")
                .flex_none()
                .cursor_pointer()
                .px(DynamicSpacing::Base04.px(&theme))
                .py(DynamicSpacing::Base01.px(&theme))
                .rounded(list_item::RADIUS.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .hover(|s| s.bg(theme.surface_hover))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    this_reset.update(cx, |app, cx| {
                        app.set_workspace_mark(reset_cwd.clone(), WorkspaceMark::default());
                        cx.notify();
                    });
                })
                .child(tr!("sidebar.reset")),
        );

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
        .px(workspace_picker_padding_x(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .child(header)
        .child(dots)
        .child(grid)
        .into_any_element()
}

/// The Show row in the sidebar-options menu. It is not a check: hovering or
/// clicking it opens a submenu of the fields a session row can draw.
fn sidebar_show_row(
    show: SidebarShow,
    submenu_open: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement {
    let this_open = this.clone();
    let this_click = this.clone();
    picker_entry(div().id("sidebar-show"), &theme)
        .relative()
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_pointer()
        .when(submenu_open, |row| row.bg(theme.surface_hover))
        .when(!submenu_open, |row| {
            row.hover(|s| s.bg(theme.surface_hover))
        })
        .on_hover(move |hovered: &bool, _, cx| {
            if *hovered {
                this_open.update(cx, |app, cx| app.open_sidebar_show_menu(cx));
            }
        })
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this_click.update(cx, |app, cx| app.open_sidebar_show_menu(cx));
        })
        .child(icon(
            "icons/eye.svg",
            context_menu::ICON.px(&theme),
            theme.text_muted,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.text_secondary)
                .child(tr!("sidebar.show")),
        )
        .child(icon(
            "icons/chevron-right.svg",
            context_menu::ICON.px(&theme),
            theme.text_muted,
        ))
        .when(submenu_open, |row| {
            // Anchor at the row's right edge so the flyout sits beside the
            // parent menu, the way Cursor's Show submenu does.
            row.child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(0.))
                    .child(sidebar_show_popup(show, this, theme)),
            )
        })
}

/// The Show submenu: Updated (relative time) and Summary (first message).
/// Toggling a field leaves both menus open.
fn sidebar_show_popup(show: SidebarShow, this: Entity<OrbitApp>, theme: Theme) -> AnyElement {
    let popup = picker_surface(div(), &theme)
        .w(px(SHOW_POPOVER_W))
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.menu_dismissed_at = Some(Instant::now());
                    app.close_sidebar_options();
                    cx.notify();
                })
            }
        })
        .child(
            div()
                .py(picker::list_padding_y(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base01.px(&theme))
                .child(sidebar_choice_item(
                    "sidebar-show-updated",
                    "icons/clock.svg",
                    tr!("sidebar.show_updated"),
                    show.updated,
                    this.clone(),
                    theme,
                    false,
                    |app, cx| app.toggle_sidebar_show_updated(cx),
                ))
                .child(sidebar_choice_item(
                    "sidebar-show-summary",
                    "icons/chat.svg",
                    tr!("sidebar.show_summary"),
                    show.summary,
                    this,
                    theme,
                    false,
                    |app, cx| app.toggle_sidebar_show_summary(cx),
                )),
        );

    // The options menu is already a `deferred` layer. Nesting another one
    // calls `defer_draw` while that layer is being drawn, and GPUI aborts
    // with "cannot call defer_draw during deferred drawing". This flyout
    // anchors beside the Show row and paints with its parent instead.
    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(Corner::TopLeft)
        .offset(point(popover::MENU_OFFSET, px(0.)))
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(popup)
        .into_any_element()
}

/// The hover-revealed view control on the sidebar's header. Opens the
/// sidebar-options menu anchored below: how sessions are grouped, ordered,
/// and filtered (each active choice checked).
pub(crate) fn sidebar_options_button(
    group_by: SidebarGroupBy,
    sort: WorkspaceSort,
    archived_filter: SidebarArchivedFilter,
    show: SidebarShow,
    show_menu: bool,
    open: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let this_for_popup = this.clone();
    icon_button_frame(div().id("sidebar-options"), &theme, ButtonSize::Compact)
        .relative()
        .tip(tr!("sidebar.options"))
        .cursor_pointer()
        // Always visible: the sidebar's one view control, not a hover reveal.
        .when(open, |button| button.bg(theme.surface_active))
        .hover(|s| s.bg(theme.surface_hover))
        .active(|s| s.opacity(PRESS_DIM))
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| app.toggle_sidebar_sort_menu(window, cx));
        })
        .child(icon(
            "icons/sort.svg",
            ButtonSize::Compact.icon_size().px(&theme),
            theme.text_muted,
        ))
        .children(open.then(|| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size(px(0.))
                .child(sidebar_options_popup(
                    group_by,
                    sort,
                    archived_filter,
                    show,
                    show_menu,
                    this_for_popup,
                    theme,
                ))
        }))
}

/// One row of the sidebar-options menu, on the picker metrics the branch
/// selector uses: a leading glyph, the label, and a trailing check when the
/// choice is active. The closure is the action the row runs.
fn sidebar_choice_item<C>(
    id: &'static str,
    icon_path: &'static str,
    label: String,
    active: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
    // Hovering a parent-menu row closes the Show submenu, so the flyout
    // does not stay up over Group by / Order by / Filter. The Show fields
    // themselves pass false — hovering them must not dismiss the flyout.
    close_show_on_hover: bool,
    on_click: C,
) -> impl IntoElement
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    picker_entry(div().id(id), &theme)
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_pointer()
        .when(active, |row| {
            row.bg(theme.surface_selected)
                .text_color(theme.selection_fg)
        })
        .when(!active, |row| row.hover(|s| s.bg(theme.surface_hover)))
        .on_hover({
            let this = this.clone();
            move |hovered: &bool, _, cx| {
                if close_show_on_hover && *hovered {
                    this.update(cx, |app, cx| {
                        if app.sidebar_show_menu {
                            app.sidebar_show_menu = false;
                            cx.notify();
                        }
                    });
                }
            }
        })
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| on_click(app, cx));
        })
        .child(icon(
            icon_path,
            context_menu::ICON.px(&theme),
            if active {
                theme.text_primary
            } else {
                theme.text_muted
            },
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(if active {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .child(label),
        )
        .when(active, |row| {
            row.child(icon(
                "icons/check.svg",
                context_menu::ICON.px(&theme),
                theme.accent_text,
            ))
        })
}

/// Width of the sidebar-options popover. Wider than the branch picker's
/// compact width so the long "All conversations (show archived)" row reads
/// without truncating.
const OPTIONS_POPOVER_W: f32 = 300.;

/// Width of the Show submenu. The two field names are short.
const SHOW_POPOVER_W: f32 = 220.;

/// The sidebar-options menu, anchored below the header control. Uses the
/// picker surface (search-less) and `picker_entry` rows so it matches the
/// branch selector; an outside mouse-down dismisses it. Sections: Group by,
/// Order by, Show (a submenu of row fields), and Filter sessions.
pub(crate) fn sidebar_options_popup(
    group_by: SidebarGroupBy,
    sort: WorkspaceSort,
    archived_filter: SidebarArchivedFilter,
    show: SidebarShow,
    show_menu: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    // No `overflow_hidden`: the Show flyout is a child of the Show row and
    // draws past this box's right edge. Clipping to the menu would hide it.
    // Rows are inset, so the rounded surface still reads cleanly.
    let popup = picker_surface(div(), &theme)
        .w(px(OPTIONS_POPOVER_W))
        .flex()
        .flex_col()
        .occlude()
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // A click on the Show submenu lands outside this popup.
                    // That submenu dismisses both menus; doing it here would
                    // close them before the field toggle runs.
                    if app.sidebar_show_menu {
                        return;
                    }
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at = Some(Instant::now());
                    app.close_sidebar_options();
                    cx.notify();
                })
            }
        })
        .child(
            div()
                .py(picker::list_padding_y(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base01.px(&theme))
                .child(menu_header(tr!("sidebar.group_by"), &theme))
                .children(SidebarGroupBy::ALL.into_iter().map(|option| {
                    sidebar_choice_item(
                        option.as_str(),
                        option.icon_path(),
                        tr!(option.label_key()),
                        option == group_by,
                        this.clone(),
                        theme,
                        true,
                        move |app, cx| app.set_sidebar_group_by(option, cx),
                    )
                }))
                .child(menu_header(tr!("sidebar.order_by"), &theme))
                .children(WorkspaceSort::MENU.into_iter().map(|option| {
                    sidebar_choice_item(
                        option.as_str(),
                        option.icon_path(),
                        tr!(option.label_key()),
                        option == sort,
                        this.clone(),
                        theme,
                        true,
                        move |app, cx| app.set_workspace_sort(option, cx),
                    )
                }))
                .child(sidebar_show_row(show, show_menu, this.clone(), theme))
                .child(menu_header(tr!("sidebar.filter_sessions"), &theme))
                .children(SidebarArchivedFilter::ALL.into_iter().map(|option| {
                    sidebar_choice_item(
                        option.as_str(),
                        option.icon_path(),
                        tr!(option.label_key()),
                        option == archived_filter,
                        this.clone(),
                        theme,
                        true,
                        move |app, cx| app.set_sidebar_archived_filter(option, cx),
                    )
                }))
        );

    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(Corner::TopLeft)
        .offset(point(
            px(0.),
            ButtonSize::Compact.height(&theme) + popover::MENU_OFFSET,
        ))
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(deferred(popup))
        .into_any_element()
}

/// One entry of the sidebar's session / workspace menus, on Zed's context
/// menu metrics ([`context_menu_entry`]).
pub(crate) fn menu_item<C, L>(
    id: &'static str,
    icon_path: &'static str,
    label: L,
    theme: Theme,
    this: Entity<OrbitApp>,
    danger: bool,
    on_click: C,
) -> impl IntoElement + use<C, L>
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let label: SharedString = label.into();
    let on_click = on_click;
    let (hover_bg, text_color, icon_color) = if danger {
        (theme.error_hover, theme.error_fg, theme.error_fg)
    } else {
        (theme.overlay, theme.text_secondary, theme.text_muted)
    };
    picker_entry(div().id(id), &theme)
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_pointer()
        .when(danger, |s| s.bg(theme.error))
        .hover(move |s| s.bg(hover_bg))
        .text_color(text_color)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| (on_click)(app, cx));
        })
        .child(icon(icon_path, context_menu::ICON.px(&theme), icon_color))
        .child(div().flex_1().min_w_0().truncate().child(label.to_string()))
}

/// Reveal a session file in the OS file manager (macOS first, matching the
/// product's platform stance; other platforms get the containing folder).
pub(crate) fn reveal_in_file_manager(path: PathBuf) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(not(target_os = "macos"))]
    if let Some(dir) = path.parent() {
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
        let _ = dir;
    }
}

/// Empty-state panel for a fresh pi store: what this space is for and how
/// to fill it. No cards, no illustration — one calm statement.
pub(crate) fn empty_sessions_state(theme: Theme) -> impl IntoElement + use<> {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .px(DynamicSpacing::Base20.px(&theme))
        .pb(DynamicSpacing::Base40.px(&theme))
        .child(
            div()
                .size(px(36.))
                .rounded_full()
                .bg(theme.surface_elevated)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(
                    "icons/spark.svg",
                    IconSize::Medium.px(&theme),
                    theme.accent_text,
                )),
        )
        .child(
            div()
                .text_size(TextSize::Default.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(tr!("sidebar.no_projects_yet")),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .text_align(TextAlign::Center)
                .child(tr!("sidebar.pick_a_folder_to_start_your_first_task")),
        )
}

// ── controller ────────────────────────────────────────────────────
impl OrbitApp {
    /// Open the session row menu from a right-click, floating it at the
    /// pointer ([`SessionMenu::at`]). Always (re)positions the menu — a
    /// right-click while another row's menu is up moves it to the new row
    /// instead of dismissing.
    pub(super) fn open_session_menu_at(
        &mut self,
        menu: SessionMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_transient_popups(window, cx);
        self.workspace_menu = None;
        self.session_menu = Some(menu);
        cx.notify();
    }

    /// Close the sidebar view menu and its Show submenu together.
    fn close_sidebar_options(&mut self) {
        self.sidebar_sort_menu = false;
        self.sidebar_show_menu = false;
    }

    /// Dismiss window-level overlays (palette, pickers, popup selectors) so a
    /// row menu never opens underneath one — every open path above routes
    /// through here.
    fn dismiss_transient_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.branch_picker.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.workspace_picker.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.close_sidebar_options();
    }

    pub(super) fn on_menu_rename_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.session_menu.take() else {
            return;
        };
        if self.current_session_path.as_ref() == Some(&menu.path) {
            if self.pending_session_rename.is_none() {
                self.begin_session_rename(window, cx);
            }
            cx.notify();
            return;
        }
        let Some(session) = self
            .sessions
            .iter()
            .find(|session| session.path == menu.path)
            .cloned()
        else {
            cx.notify();
            return;
        };
        self.switch_to_session(session, true, cx);
        // A cold switch must finish before pi can safely accept a rename.
        if self.runtime_is_live() {
            self.pending_session_rename = Some(menu.path);
        }
        cx.notify();
    }

    pub(super) fn on_menu_copy_path(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.session_menu {
            cx.write_to_clipboard(ClipboardItem::new_string(
                menu.path.to_string_lossy().into_owned(),
            ));
        }
        self.session_menu = None;
        cx.notify();
    }

    pub(super) fn on_menu_reveal(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.session_menu {
            reveal_in_file_manager(menu.path.clone());
        }
        self.session_menu = None;
        cx.notify();
    }

    /// Duplicate the session the open row menu belongs to, so it shows up as
    /// its own row to branch off. The source is only read, so — unlike Delete —
    /// this is safe for a session with a live pi process.
    pub(super) fn on_menu_clone_session(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.session_menu.take() else {
            return;
        };
        match sessions::clone_session_file(&menu.path) {
            Ok(_) => {
                self.sessions = sessions::load_sessions();
                self.prune_workflow_store();
            }
            Err(err) => self.toast_error(tr!("sidebar.clone_failed", error = err)),
        }
        cx.notify();
    }

    /// Pin or unpin the session the open row menu belongs to. The pin lives
    /// in Orbit's own store (`~/.orbit-pi/pinned-sessions.json`); pi's
    /// session file is never touched.
    pub(super) fn on_menu_toggle_pin(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.take() {
            crate::pins::toggle(&menu.path);
        }
        cx.notify();
    }

    /// Archive or unarchive the session whose row menu is open, then re-run
    /// the archived filter so the row appears or leaves per the view choice.
    pub(super) fn on_menu_toggle_archive(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.take() {
            crate::archive::toggle(&menu.path);
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// First Delete click: swap the popup to the confirmation state.
    pub(super) fn on_menu_delete_request(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.as_mut() {
            menu.confirm_delete = true;
            cx.notify();
        }
    }

    /// Confirmed: remove the session file from disk and refresh the list.
    pub(super) fn on_menu_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.take() {
            // Defensive: a warm parked process would recreate the file.
            if self.lives.contains_key(&menu.path) {
                self.toast_warning(tr!("sidebar.live_process_delete"));
                cx.notify();
                return;
            }
            if let Err(err) = fs::remove_file(&menu.path) {
                self.toast_error(tr!("sidebar.delete_failed", error = err));
            } else {
                // A deleted session must not leave a stale pin behind.
                crate::pins::remove(&menu.path);
                crate::archive::remove(&menu.path);
                // Nor a review mark for a row that can never come back.
                self.awaiting_review.remove(&menu.path);
                self.session_outcomes.remove(&menu.path);
            }
            self.sessions = sessions::load_sessions();
            self.prune_workflow_store();
            cx.notify();
        }
    }

    pub(super) fn on_menu_cancel(&mut self, cx: &mut Context<Self>) {
        self.session_menu = None;
        cx.notify();
    }

    /// Open (or toggle closed) the sidebar's workspace-sort menu. Follows the
    /// same gesture guard as the row menus, so the dismissing click cannot
    /// immediately re-open it.
    pub(super) fn toggle_sidebar_sort_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.sidebar_sort_menu {
            self.close_sidebar_options();
            cx.notify();
            return;
        }
        self.dismiss_transient_popups(window, cx);
        self.session_menu = None;
        self.workspace_menu = None;
        self.sidebar_sort_menu = true;
        cx.notify();
    }

    /// Switch the workspace ordering and persist the choice with the project
    /// list. Re-sorting is pure UI: pi's session files are never touched.
    pub(super) fn set_workspace_sort(&mut self, sort: WorkspaceSort, cx: &mut Context<Self>) {
        self.close_sidebar_options();
        if self.workspace_sort != sort {
            self.workspace_sort = sort;
            self.persist_workspace_prefs();
        }
        cx.notify();
    }

    /// Choose how the sidebar arranges sessions, persisting the choice beside
    /// the project list. Regrouping is pure UI: pi's session files are never
    /// touched.
    pub(super) fn set_sidebar_group_by(
        &mut self,
        group_by: SidebarGroupBy,
        cx: &mut Context<Self>,
    ) {
        self.close_sidebar_options();
        if self.sidebar_group_by != group_by {
            self.sidebar_group_by = group_by;
            self.persist_workspace_prefs();
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// Choose which archived sessions the sidebar lists and persist it.
    pub(super) fn set_sidebar_archived_filter(
        &mut self,
        filter: SidebarArchivedFilter,
        cx: &mut Context<Self>,
    ) {
        self.close_sidebar_options();
        if self.sidebar_archived_filter != filter {
            self.sidebar_archived_filter = filter;
            self.persist_workspace_prefs();
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// Open the Show submenu. Hovering and clicking the Show row both call
    /// this; it stays open until another row is hovered or the menu closes.
    pub(super) fn open_sidebar_show_menu(&mut self, cx: &mut Context<Self>) {
        if !self.sidebar_show_menu {
            self.sidebar_show_menu = true;
            cx.notify();
        }
    }

    /// Flip the Updated field and persist it. The menus stay open.
    pub(super) fn toggle_sidebar_show_updated(&mut self, cx: &mut Context<Self>) {
        self.sidebar_show.updated = !self.sidebar_show.updated;
        self.persist_workspace_prefs();
        cx.notify();
    }

    /// Flip the Summary field and persist it. The menus stay open.
    pub(super) fn toggle_sidebar_show_summary(&mut self, cx: &mut Context<Self>) {
        self.sidebar_show.summary = !self.sidebar_show.summary;
        self.persist_workspace_prefs();
        cx.notify();
    }

    /// Open (or toggle closed) the row-actions popup for a workspace header.
    pub(super) fn toggle_workspace_menu(
        &mut self,
        menu: WorkspaceMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Same-click dismissal must not re-open (see toggle_workspace_menu).
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.workspace_menu.as_ref() == Some(&menu) {
            self.workspace_menu = None;
            cx.notify();
            return;
        }
        self.dismiss_transient_popups(window, cx);
        self.session_menu = None;
        self.workspace_menu = Some(menu);
        cx.notify();
    }

    /// Open the workspace header menu from a right-click, floating it at the
    /// pointer ([`WorkspaceMenu::at`]). See [`Self::open_session_menu_at`] for
    /// why this bypasses the toggle's gesture guard and equality check.
    pub(super) fn open_workspace_menu_at(
        &mut self,
        menu: WorkspaceMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_transient_popups(window, cx);
        self.session_menu = None;
        self.workspace_menu = Some(menu);
        cx.notify();
    }

    pub(super) fn on_workspace_copy_path(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.workspace_menu {
            cx.write_to_clipboard(ClipboardItem::new_string(
                menu.cwd.to_string_lossy().into_owned(),
            ));
        }
        self.workspace_menu = None;
        cx.notify();
    }

    /// Swap the open workspace menu for its icon/tint picker, in place.
    pub(super) fn on_workspace_open_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.workspace_menu.as_mut() {
            menu.picker = true;
        }
        cx.notify();
    }

    /// Remove the workspace the open header menu belongs to from Orbit's own
    /// project list. pi's session files are never touched — the folder can be
    /// added again by picking it to work in.
    pub(super) fn on_workspace_remove(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.workspace_menu.take() else {
            return;
        };
        self.remove_workspace(&menu.cwd, cx);
        // Forget the dropped group's view state so re-adding it starts fresh.
        self.collapsed_workspaces.remove(&menu.label);
        self.expanded_workspace_groups.remove(&menu.label);
        self.expanded_session_groups.remove(&menu.label);
        cx.notify();
    }

    /// Keep the row menu honest: close it when its session vanishes from
    /// the store (deleted externally, session ended, …).
    pub(super) fn sync_session_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.session_menu {
            if !menu.path.exists() {
                self.session_menu = None;
                cx.notify();
            }
        }
    }

    // ── keyboard navigation ───────────────────────────────────────────

    /// The sidebar rows as currently laid out — grouped by workspace, or one
    /// flat list when grouping is off. Shared by the render path and keyboard
    /// navigation so the cursor can never point at a row the user cannot see.
    pub(super) fn sidebar_rows(
        &self,
        sessions: &[SessionInfo],
        pinned: &HashSet<PathBuf>,
    ) -> Vec<SideRow> {
        let working = self.workspace_label();
        let running: HashSet<PathBuf> = self
            .lives
            .iter()
            .filter(|(_, parked)| parked.busy)
            .map(|(path, _)| path.clone())
            .collect();
        // A marked session is unread news; collapsing a group must not hide it
        // (issue #83). The flat list has no collapse, so it needs no set.
        let reviewed: HashSet<PathBuf> = self.awaiting_review.keys().cloned().collect();
        match self.sidebar_group_by {
            SidebarGroupBy::OneList => build_flat_sidebar_rows(sessions, pinned),
            SidebarGroupBy::Workspace => build_sidebar_rows(
                sessions,
                &self.workspaces,
                self.workspace_sort,
                &self.workspace_added_at,
                &working,
                &self.collapsed_workspaces,
                &self.expanded_workspace_groups,
                &self.expanded_session_groups,
                pinned,
                &self.current_session_path,
                &running,
                &reviewed,
            ),
        }
    }

    /// The sidebar's session list with the archived filter applied — the same
    /// list the render path paints, so keyboard navigation stays in step.
    pub(super) fn sidebar_sessions_filtered(&self) -> Vec<SessionInfo> {
        let sessions = self.sidebar_sessions();
        let archived: HashSet<PathBuf> = crate::archive::all().paths();
        filter_archived(sessions, self.sidebar_archived_filter, &archived)
    }

    /// The sidebar rows with the pinned snapshot read fresh — the same
    /// grouping, ordering, and collapse state the render path uses.
    fn sidebar_rows_for_nav(&self) -> Vec<SideRow> {
        let sessions = self.sidebar_sessions_filtered();
        let pinned: HashSet<PathBuf> = crate::pins::all().paths();
        self.sidebar_rows(&sessions, &pinned)
    }

    /// Row index of the open session in `rows`, when it is visible.
    fn active_session_row(&self, rows: &[SideRow]) -> Option<usize> {
        let sessions = self.sidebar_sessions();
        let active = self.current_session_path.as_ref()?;
        let session_ix = sessions
            .iter()
            .position(|session| &session.path == active)?;
        rows.iter()
            .position(|row| matches!(row, SideRow::Session(ix) if *ix == session_ix))
    }

    /// ⌘⇧B: reveal the sidebar if hidden, then focus it and place the cursor
    /// on the open session (or the first row).
    pub(super) fn on_focus_sessions(
        &mut self,
        _: &crate::FocusSessions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Settings is a full-window surface with its own nav; the sidebar is
        // not mounted, so there is nothing to focus.
        if self.settings_open {
            return;
        }
        if !self.sidebar_visible {
            self.sidebar_visible = true;
            self.sidebar_slide_gen = self.sidebar_slide_gen.wrapping_add(1);
        }
        let rows = self.sidebar_rows_for_nav();
        self.sidebar_cursor =
            (!rows.is_empty()).then(|| self.active_session_row(&rows).unwrap_or(0));
        window.focus(&self.sidebar_focus);
        cx.notify();
    }

    /// Visible sessions in the order the sidebar navigates them — the
    /// numbering `⌘1…⌘9` and session cycling share.
    pub(super) fn navigable_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sidebar_sessions();
        self.sidebar_rows_for_nav()
            .iter()
            .filter_map(|row| match row {
                SideRow::Session(ix) => sessions.get(*ix).cloned(),
                _ => None,
            })
            .collect()
    }

    /// `⌘1…⌘9`: open the Nth session the sidebar shows. A slot past the end
    /// is a no-op, so a stray number never switches the session by accident.
    pub(super) fn open_session_slot(&mut self, slot: usize, cx: &mut Context<Self>) {
        if slot == 0 {
            return;
        }
        let Some(session) = self.navigable_sessions().into_iter().nth(slot - 1) else {
            return;
        };
        self.on_open_session(session, cx);
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: cycle through the visible sessions, wrapping
    /// at both ends. With a single session there is nowhere to go.
    pub(super) fn cycle_session(&mut self, direction: isize, cx: &mut Context<Self>) {
        let sessions = self.navigable_sessions();
        if sessions.len() < 2 {
            return;
        }
        let current = self
            .current_session_path
            .as_ref()
            .and_then(|path| sessions.iter().position(|session| &session.path == path));
        let next = match current {
            Some(ix) => (ix as isize + direction).rem_euclid(sessions.len() as isize) as usize,
            None if direction > 0 => 0,
            None => sessions.len() - 1,
        };
        self.on_open_session(sessions[next].clone(), cx);
    }

    /// Action handler for ⌘1…⌘9.
    pub(super) fn on_open_session_slot(
        &mut self,
        action: &crate::OpenSessionSlot,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_session_slot(action.slot, cx);
    }

    /// Action handler for Ctrl+Tab.
    pub(super) fn on_next_session(
        &mut self,
        _: &crate::NextSession,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_session(1, cx);
    }

    /// Action handler for Ctrl+Shift+Tab.
    pub(super) fn on_prev_session(
        &mut self,
        _: &crate::PrevSession,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_session(-1, cx);
    }

    pub(super) fn on_sidebar_prev(
        &mut self,
        _: &crate::SidebarPrev,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_sidebar_cursor(|ix| ix.saturating_sub(1), cx);
    }

    pub(super) fn on_sidebar_next(
        &mut self,
        _: &crate::SidebarNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_sidebar_cursor(|ix| ix.saturating_add(1), cx);
    }

    pub(super) fn on_sidebar_home(
        &mut self,
        _: &crate::SidebarHome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_sidebar_cursor(|_| 0, cx);
    }

    pub(super) fn on_sidebar_end(
        &mut self,
        _: &crate::SidebarEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_sidebar_cursor(|_| usize::MAX, cx);
    }

    /// Move the cursor by `step`, clamped to the visible rows, and scroll it
    /// into view.
    fn move_sidebar_cursor(&mut self, step: impl Fn(usize) -> usize, cx: &mut Context<Self>) {
        let count = self.sidebar_rows_for_nav().len();
        self.sidebar_cursor = clamp_cursor(step(self.sidebar_cursor.unwrap_or(0)), count);
        if let Some(next) = self.sidebar_cursor {
            self.sidebar_list.scroll_to_reveal_item(next);
        }
        cx.notify();
    }

    /// Enter on the cursor row: open a session (returning focus to the
    /// composer), toggle a workspace group, or reveal/collapse a group's
    /// overflow — the same actions the row's click affordances perform.
    pub(super) fn on_sidebar_confirm(
        &mut self,
        _: &crate::SidebarConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.sidebar_cursor else {
            return;
        };
        let rows = self.sidebar_rows_for_nav();
        let Some(row) = rows.get(index) else {
            return;
        };
        match row {
            SideRow::Session(ix) => {
                let sessions = self.sidebar_sessions();
                if let Some(session) = sessions.get(*ix).cloned() {
                    self.on_open_session(session, cx);
                }
                // The row is a destination: hand focus back to the composer
                // so typing resumes immediately.
                self.sidebar_cursor = None;
                self.input.read(cx).focus(window);
            }
            SideRow::Workspace { label, .. } => {
                let working = self.workspace_label();
                toggle_workspace_group(
                    label.clone(),
                    &working,
                    &mut self.collapsed_workspaces,
                    &mut self.expanded_workspace_groups,
                );
                self.clamp_sidebar_cursor(cx);
            }
            SideRow::ShowMore { label, count, .. } => {
                self.expanded_session_groups
                    .entry(label.clone())
                    .and_modify(|extra| *extra += count)
                    .or_insert(*count);
                self.clamp_sidebar_cursor(cx);
            }
            SideRow::ShowLess { label } => {
                self.expanded_session_groups.remove(label);
                self.clamp_sidebar_cursor(cx);
            }
        }
        cx.notify();
    }

    /// Esc: dismiss an open row menu first, otherwise leave keyboard
    /// navigation and hand focus back to the composer.
    pub(super) fn on_sidebar_close(
        &mut self,
        _: &crate::SidebarClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_menu.take().is_some()
            || self.workspace_menu.take().is_some()
            || self.sidebar_sort_menu
        {
            self.close_sidebar_options();
            cx.notify();
            return;
        }
        self.sidebar_cursor = None;
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// The command palette's Delete Session: reveal the sidebar on the active
    /// session's row and arm the same confirmation the row menu shows, so the
    /// destructive step still needs an explicit confirm.
    pub(super) fn arm_active_session_delete(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.current_session_path.clone() else {
            return;
        };
        let sessions = self.sidebar_sessions();
        let Some(session_ix) = sessions.iter().position(|session| session.path == path) else {
            return;
        };
        let session = sessions[session_ix].clone();
        if !self.sidebar_visible {
            self.sidebar_visible = true;
            self.sidebar_slide_gen = self.sidebar_slide_gen.wrapping_add(1);
        }
        let rows = self.sidebar_rows_for_nav();
        self.sidebar_cursor = rows
            .iter()
            .position(|row| matches!(row, SideRow::Session(ix) if *ix == session_ix));
        if let Some(ix) = self.sidebar_cursor {
            self.sidebar_list.scroll_to_reveal_item(ix);
        }
        self.session_menu = Some(SessionMenu {
            path: session.path.clone(),
            title: session.title,
            deletable: !self.lives.contains_key(&session.path),
            confirm_delete: true,
            at: None,
        });
        window.focus(&self.sidebar_focus);
        cx.notify();
    }

    /// Keep the cursor inside the rows after one collapses the list under it.
    fn clamp_sidebar_cursor(&mut self, cx: &mut Context<Self>) {
        let count = self.sidebar_rows_for_nav().len();
        self.sidebar_cursor = self.sidebar_cursor.and_then(|ix| clamp_cursor(ix, count));
        cx.notify();
    }
}

/// Clamp a cursor position into `count` rows: `None` when there are none, the
/// nearest valid index otherwise.
fn clamp_cursor(index: usize, count: usize) -> Option<usize> {
    (count > 0).then(|| index.min(count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn row_rename_waits_for_the_target_session_state(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeKey::ORBIT));
            let app = cx.new(OrbitApp::new);
            app.update(cx, |app, cx| {
                let path = PathBuf::from("/tmp/rename-target.jsonl");
                app.current_session_path = Some(path.clone());
                app.pending_session_rename = Some(path);
                let mut refresh = false;
                app.on_response(
                    "get_state",
                    true,
                    Some(&serde_json::json!({"sessionFile": "/tmp/other.jsonl"})),
                    None,
                    "test",
                    &mut refresh,
                    cx,
                );
                assert!(!app.session_rename_ready);
                assert!(app.pending_session_rename.is_some());
            });
        });
    }

    #[gpui::test]
    fn row_rename_opens_after_target_session_state_arrives(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeKey::ORBIT));
            let app = cx.new(OrbitApp::new);
            app.update(cx, |app, cx| {
                let path = PathBuf::from("/tmp/rename-target.jsonl");
                app.current_session_path = Some(path.clone());
                app.pending_session_rename = Some(path);
                let mut refresh = false;
                app.on_response(
                    "get_state",
                    true,
                    Some(&serde_json::json!({"sessionFile": "/tmp/rename-target.jsonl"})),
                    None,
                    "test",
                    &mut refresh,
                    cx,
                );
                assert!(app.session_rename_ready);
                assert!(app.pending_session_rename.is_none());
            });
        });
    }

    #[gpui::test]
    fn failed_session_switch_cancels_pending_row_rename(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeKey::ORBIT));
            let app = cx.new(OrbitApp::new);
            app.update(cx, |app, cx| {
                app.pending_session_rename = Some(PathBuf::from("/tmp/rename-target.jsonl"));
                let mut refresh = false;
                app.on_response(
                    "switch_session",
                    false,
                    None,
                    Some("failed"),
                    "test",
                    &mut refresh,
                    cx,
                );
                assert!(app.pending_session_rename.is_none());
                assert!(!app.session_rename_ready);
            });
        });
    }

    #[test]
    fn cursor_clamps_to_the_visible_rows() {
        assert_eq!(clamp_cursor(0, 0), None);
        assert_eq!(clamp_cursor(3, 2), Some(1));
        assert_eq!(clamp_cursor(1, 5), Some(1));
    }

    /// A branch arrow appears only for a count that exists: `↑0 ↓0` is a row of
    /// arrows that says nothing.
    #[test]
    fn divergence_arrows_appear_only_for_a_non_zero_count() {
        assert_eq!(divergence_labels(None), (None, None));
        assert_eq!(divergence_labels(Some((0, 0))), (None, None));
        assert_eq!(
            divergence_labels(Some((2, 0))),
            (Some("↑2".to_string()), None)
        );
        assert_eq!(
            divergence_labels(Some((0, 1))),
            (None, Some("↓1".to_string()))
        );
        assert_eq!(
            divergence_labels(Some((3, 4))),
            (Some("↑3".to_string()), Some("↓4".to_string()))
        );
    }

    /// The lane is the sidebar's whole answer to "what is this row doing", so
    /// its mapping has to be exact: a live block outranks a stale unread mark,
    /// and a read result recedes instead of disappearing.
    #[test]
    fn status_lane_orders_a_block_over_an_unread_mark() {
        use StatusLane::*;
        assert_eq!(status_lane(Some(SessionStatus::Running), None), Progress);
        assert_eq!(status_lane(Some(SessionStatus::Retrying), None), Progress);
        assert_eq!(status_lane(Some(SessionStatus::Compacting), None), Progress);
        assert_eq!(
            status_lane(Some(SessionStatus::WaitingApproval), None),
            Blocked(BlockReason::Approval)
        );
        assert_eq!(
            status_lane(Some(SessionStatus::WaitingInput), None),
            Blocked(BlockReason::Input)
        );
        // A block is happening now; the unread mark describes a run that is
        // already over, so it cannot be allowed to win.
        assert_eq!(
            status_lane(Some(SessionStatus::WaitingApproval), Some(ReviewMark::Done)),
            Blocked(BlockReason::Approval)
        );
        assert_eq!(status_lane(Some(SessionStatus::Failed), None), Failed);
        assert_eq!(status_lane(Some(SessionStatus::Disconnected), None), Failed);
        assert_eq!(status_lane(Some(SessionStatus::Stopped), None), Stopped);
        assert_eq!(status_lane(Some(SessionStatus::Completed), None), Done);
        assert_eq!(
            status_lane(Some(SessionStatus::Completed), Some(ReviewMark::Done)),
            UnreadDone
        );
        // No process was ever observed (another client, an earlier launch): the
        // read mark is then all the row knows.
        assert_eq!(status_lane(None, Some(ReviewMark::Done)), UnreadDone);
        assert_eq!(status_lane(None, Some(ReviewMark::Failed)), UnreadFailed);
        assert_eq!(
            status_lane(Some(SessionStatus::Failed), Some(ReviewMark::Failed)),
            UnreadFailed
        );
        assert_eq!(status_lane(Some(SessionStatus::Idle), None), Idle);
        assert_eq!(status_lane(None, None), Unknown);
    }

    /// Attention states get words; quiet states leave room for the age.
    #[test]
    fn attention_states_get_words() {
        assert!(status_lane_words(StatusLane::Blocked(BlockReason::Approval)).is_some());
        assert!(status_lane_words(StatusLane::Blocked(BlockReason::Input)).is_some());
        assert!(status_lane_words(StatusLane::Failed).is_some());
        assert!(status_lane_words(StatusLane::UnreadDone).is_some());
        assert!(status_lane_words(StatusLane::UnreadFailed).is_some());
        for quiet in [
            StatusLane::Progress,
            StatusLane::Done,
            StatusLane::Stopped,
            StatusLane::Idle,
            StatusLane::Unknown,
        ] {
            assert_eq!(status_lane_words(quiet), None);
        }
    }

    #[test]
    fn review_mark_changes_icon_and_tooltip_when_result_is_opened() {
        let status = Some(SessionStatus::Completed);
        let unread = status_lane(status, Some(ReviewMark::Done));
        let viewed = status_lane(status, None);
        assert_eq!(unread, StatusLane::UnreadDone);
        assert_eq!(viewed, StatusLane::Done);
        assert_ne!(status_lane_icon(unread), status_lane_icon(viewed));
        assert_eq!(
            status_lane_tip(unread, status),
            Some(tr!("sidebar.review_finished"))
        );
        assert_eq!(
            status_lane_tip(viewed, status),
            Some(tr!("sidebar.status_viewed"))
        );

        let failed = status_lane(Some(SessionStatus::Failed), Some(ReviewMark::Failed));
        assert_eq!(
            status_lane_tip(failed, Some(SessionStatus::Failed)),
            Some(tr!("sidebar.review_failed"))
        );
        assert_eq!(
            status_lane_tip(StatusLane::Progress, Some(SessionStatus::Retrying)),
            Some(SessionStatus::Retrying.label())
        );
        assert_eq!(
            status_lane_tip(StatusLane::Failed, Some(SessionStatus::Disconnected)),
            Some(SessionStatus::Disconnected.label())
        );
        assert_eq!(status_lane_tip(StatusLane::Unknown, None), None);
    }

    /// Every lane that has a state paints a glyph from the icon set, a lane
    /// with nothing to say paints nothing, and the tooltip never invents a
    /// state the row does not know.
    #[test]
    fn only_lanes_with_a_state_paint_a_glyph() {
        for lane in [
            StatusLane::Progress,
            StatusLane::Blocked(BlockReason::Approval),
            StatusLane::Blocked(BlockReason::Input),
            StatusLane::UnreadDone,
            StatusLane::UnreadFailed,
            StatusLane::Done,
            StatusLane::Failed,
            StatusLane::Stopped,
        ] {
            let icon = status_lane_icon(lane).expect("a state needs a glyph");
            assert!(icon.starts_with("icons/"), "{lane:?} → {icon}");
        }
        assert_eq!(status_lane_icon(StatusLane::Idle), None);
        assert_eq!(status_lane_icon(StatusLane::Unknown), None);
        for lane in [
            StatusLane::Progress,
            StatusLane::Blocked(BlockReason::Approval),
            StatusLane::Blocked(BlockReason::Input),
            StatusLane::UnreadDone,
            StatusLane::UnreadFailed,
            StatusLane::Done,
            StatusLane::Failed,
            StatusLane::Stopped,
            StatusLane::Idle,
            StatusLane::Unknown,
        ] {
            if status_lane_tip(lane, None).is_some() {
                assert!(matches!(
                    lane,
                    StatusLane::UnreadDone | StatusLane::UnreadFailed | StatusLane::Done
                ));
            }
        }
        assert!(status_lane_tip(StatusLane::Progress, Some(SessionStatus::Running)).is_some());
    }
}
