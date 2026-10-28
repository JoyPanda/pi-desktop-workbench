//! Last Turn review built from a conversation's `edit` / `write` tool calls.
//!
//! The docked Review pane's **Last Turn** source reads this module, not Git
//! checkpoints. Uncommitted and the other sources still diff the worktree.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::review::{self, LineKind, Snapshot, Source};

/// One `oldText` / `newText` pair inside a single `edit` tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub old: String,
    pub new: String,
}

/// One tool call that changed a file. Several `edit` pairs in the same call
/// share a base: they are located against the text from before the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileCall {
    Write(String),
    Edits(Vec<TextEdit>),
}

/// A file change recorded from one tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordedCall {
    pub path: String,
    pub call: FileCall,
}

/// Every turn's recorded calls, in turn order. The side pane holds this so
/// Last Turn can render without reading Git.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TurnRecords {
    pub turns: Vec<(usize, Vec<RecordedCall>)>,
}

impl TurnRecords {
    pub(crate) fn calls_for(&self, turn: usize) -> &[RecordedCall] {
        self.turns
            .iter()
            .find(|(number, _)| *number == turn)
            .map(|(_, calls)| calls.as_slice())
            .unwrap_or(&[])
    }

    /// Every recorded call, oldest turn first. All Changes folds this list
    /// with the same rules as one turn.
    pub(crate) fn all_calls(&self) -> Vec<RecordedCall> {
        self.turns
            .iter()
            .flat_map(|(_, calls)| calls.iter().cloned())
            .collect()
    }

    /// True when both recordings fold to the same diff. An empty turn appended
    /// at send time does not change All Changes.
    pub(crate) fn same_calls(&self, other: &Self) -> bool {
        self.turns
            .iter()
            .flat_map(|(_, calls)| calls)
            .eq(other.turns.iter().flat_map(|(_, calls)| calls))
    }
}

/// Where each numbered turn sits in the transcript.
///
/// `starts` is `(turn number, message index of the user message that opened
/// it)`. Steer and follow-up add user rows without calling [`TurnClock::begin`],
/// so those rows stay inside the open turn instead of starting another one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TurnClock {
    pub count: usize,
    pub open: bool,
    pub latest: Option<usize>,
    pub starts: Vec<(usize, usize)>,
}

impl TurnClock {
    /// Open a turn at `message_index` (the user message about to be appended).
    /// Returns false when a turn is already open — the caller must not start
    /// a second one.
    pub(crate) fn begin(&mut self, message_index: usize) -> bool {
        if self.open {
            return false;
        }
        self.count += 1;
        self.open = true;
        self.starts.push((self.count, message_index));
        self.latest = Some(self.count);
        true
    }

    /// Undo [`Self::begin`] when the user row was not actually appended.
    /// A resend of the same prompt dedups onto the previous row; keeping the
    /// extra start would renumber turns after a reload.
    pub(crate) fn cancel_begin(&mut self) {
        let Some((turn, _)) = self.starts.pop() else {
            self.open = false;
            return;
        };
        if self.count == turn {
            self.count -= 1;
        }
        self.open = false;
        self.latest = self.starts.last().map(|(turn, _)| *turn);
    }

    /// Close the open turn. `latest` stays on the last recorded start, not on
    /// `count`: a checkpoint ref may have raised `count` past any turn the
    /// conversation actually has.
    pub(crate) fn finish(&mut self) {
        self.open = false;
        self.latest = self.starts.last().map(|(turn, _)| *turn).or(self.latest);
    }

    /// Keep checkpoint ref numbers from colliding with a later capture.
    /// Does not move [`Self::latest`]: Last Turn is not those refs.
    pub(crate) fn note_checkpoint(&mut self, checkpoint_turn: usize) {
        self.count = self.count.max(checkpoint_turn);
    }

    /// Rebuild starts after the transcript is replaced (reload, compaction,
    /// fork). The last number is `max(count, user rows)` so a shorter
    /// transcript never points an existing Last Turn selection at a
    /// different turn.
    pub(crate) fn rebuild(&mut self, user_indices: &[usize]) {
        let count = user_indices.len();
        if count == 0 {
            self.starts.clear();
            // An open turn keeps the number `begin` stored. A checkpoint ref
            // must not become `latest` just because the snapshot has no user
            // rows yet.
            if !self.open {
                self.latest = None;
            }
            return;
        }
        let last = self.count.max(count);
        let first = last + 1 - count;
        self.starts = user_indices
            .iter()
            .enumerate()
            .map(|(offset, index)| (first + offset, *index))
            .collect();
        self.count = last;
        self.latest = Some(last);
    }

    /// `(turn, start message index, end message index)` for each recorded turn.
    pub(crate) fn ranges(&self, message_len: usize) -> Vec<(usize, usize, usize)> {
        self.starts
            .iter()
            .enumerate()
            .map(|(index, (turn, start))| {
                let end = self
                    .starts
                    .get(index + 1)
                    .map(|(_, next)| *next)
                    .unwrap_or(message_len);
                (*turn, *start, end)
            })
            .collect()
    }
}

/// One tool call as the transcript sees it, before it becomes a [`RecordedCall`].
pub(crate) struct ToolView<'a> {
    /// The assistant message was aborted or ended in an error.
    pub message_failed: bool,
    pub failed: bool,
    pub has_output: bool,
    pub name: &'a str,
    pub args: Option<&'a Value>,
}

/// Keep tool calls Last Turn can show. Aborted or errored messages only
/// contribute calls that actually produced a result.
pub(crate) fn recorded_calls<'a>(
    tools: impl IntoIterator<Item = ToolView<'a>>,
) -> Vec<RecordedCall> {
    let mut calls = Vec::new();
    for tool in tools {
        if tool.failed || (tool.message_failed && !tool.has_output) {
            continue;
        }
        if let Some(call) = call_from_tool(tool.name, tool.args) {
            calls.push(call);
        }
    }
    calls
}

fn call_from_tool(name: &str, args: Option<&Value>) -> Option<RecordedCall> {
    let args = args?;
    let path = args.get("path").and_then(Value::as_str)?.to_string();
    let call = match name {
        "write" => FileCall::Write(args.get("content").and_then(Value::as_str)?.to_string()),
        "edit" => {
            let edits = args.get("edits").and_then(Value::as_array)?;
            let pairs = edits
                .iter()
                .filter_map(|edit| {
                    Some(TextEdit {
                        old: edit.get("oldText").and_then(Value::as_str)?.to_string(),
                        new: edit.get("newText").and_then(Value::as_str)?.to_string(),
                    })
                })
                .collect::<Vec<_>>();
            if pairs.is_empty() {
                return None;
            }
            FileCall::Edits(pairs)
        }
        _ => return None,
    };
    Some(RecordedCall { path, call })
}

/// Kind of one line in a synthesized diff, without syntax tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiffLineKind {
    Context,
    Addition,
    Deletion,
}

/// Longest-common-subsequence line diff. Oversized inputs fall back to a
/// whole-block replacement so one edit cannot allocate a quadratic table.
pub(crate) fn diff_line_ops(
    old_lines: &[&str],
    new_lines: &[&str],
) -> Vec<(DiffLineKind, Option<usize>, Option<usize>)> {
    let (n, m) = (old_lines.len(), new_lines.len());
    let mut ops = Vec::new();
    if n.saturating_mul(m) > 250_000 {
        ops.extend((0..n).map(|index| (DiffLineKind::Deletion, Some(index), None)));
        ops.extend((0..m).map(|index| (DiffLineKind::Addition, None, Some(index))));
        return ops;
    }
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old_lines[i] == new_lines[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            ops.push((DiffLineKind::Context, Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            ops.push((DiffLineKind::Deletion, Some(i), None));
            i += 1;
        } else {
            ops.push((DiffLineKind::Addition, None, Some(j)));
            j += 1;
        }
    }
    while i < n {
        ops.push((DiffLineKind::Deletion, Some(i), None));
        i += 1;
    }
    while j < m {
        ops.push((DiffLineKind::Addition, None, Some(j)));
        j += 1;
    }
    ops
}

/// File path and full +/− counts for a folded call list. Skips patch text so
/// the chat card can match Review without tokenizing the diff.
pub(crate) fn file_counts(
    calls: &[RecordedCall],
    workspace: Option<&Path>,
) -> Vec<(String, u64, u64)> {
    let snapshot = snapshot_capped(Source::AllChanges, calls, workspace, 0);
    snapshot
        .files
        .into_iter()
        .map(|file| (file.path, file.additions, file.deletions))
        .collect()
}

/// Render Last Turn for `source` from the turn's tool calls.
pub(crate) fn snapshot_from_calls(
    source: Source,
    calls: &[RecordedCall],
    workspace: Option<&Path>,
) -> Snapshot {
    snapshot_capped(source, calls, workspace, review::MAX_RENDERED_DIFF_LINES)
}

fn snapshot_capped(
    source: Source,
    calls: &[RecordedCall],
    workspace: Option<&Path>,
    remaining: usize,
) -> Snapshot {
    let mut order = Vec::new();
    let mut grouped: HashMap<String, Vec<FileCall>> = HashMap::new();
    for call in calls {
        let Some(path) = normalize_review_path(&call.path, workspace) else {
            continue;
        };
        if !grouped.contains_key(&path) {
            order.push(path.clone());
        }
        grouped.entry(path).or_default().push(call.call.clone());
    }
    let mut numstat = String::new();
    let mut patch = String::new();
    let mut limit = EmitLimit {
        remaining,
        clipped: false,
    };
    for path in order {
        let Some(body) = render_file(&path, &grouped[&path], &mut limit) else {
            continue;
        };
        numstat.push_str(&format!("{}\t{}\t{}\n", body.added, body.deleted, path));
        patch.push_str(&body.patch);
    }
    let mut snapshot = review::parse_collected(source, &numstat, &patch, true);
    if limit.clipped {
        snapshot.truncated = true;
    }
    snapshot
}

struct RenderedFile {
    added: u64,
    deleted: u64,
    patch: String,
}

fn render_file(path: &str, calls: &[FileCall], limit: &mut EmitLimit) -> Option<RenderedFile> {
    let folded = fold_file(calls);
    let mut patch = String::new();
    let mut added = 0u64;
    let mut deleted = 0u64;
    let mut wrote_header = false;
    let mut emit_header = |patch: &mut String| {
        if wrote_header {
            return;
        }
        wrote_header = true;
        patch.push_str(&format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n"
        ));
    };
    match folded {
        Folded::Empty => return None,
        Folded::Written(content) => {
            let lines = diff_lines(&content);
            if lines.is_empty() {
                return None;
            }
            let mut hunk = String::new();
            if push_write_hunk(&mut hunk, &lines, &mut added, limit) {
                emit_header(&mut patch);
                patch.push_str(&hunk);
            }
        }
        Folded::Hunks(groups) => {
            for group in groups {
                for edit in group {
                    emit_edit_hunk(
                        &mut patch,
                        &mut emit_header,
                        &edit.old,
                        &edit.new,
                        &mut added,
                        &mut deleted,
                        limit,
                    );
                }
            }
        }
        Folded::Mixed { content, groups } => {
            let lines = diff_lines(&content);
            if !lines.is_empty() {
                let mut hunk = String::new();
                if push_write_hunk(&mut hunk, &lines, &mut added, limit) {
                    emit_header(&mut patch);
                    patch.push_str(&hunk);
                }
            }
            for group in groups {
                for edit in group {
                    emit_edit_hunk(
                        &mut patch,
                        &mut emit_header,
                        &edit.old,
                        &edit.new,
                        &mut added,
                        &mut deleted,
                        limit,
                    );
                }
            }
        }
    }
    if patch.is_empty() && added == 0 && deleted == 0 {
        return None;
    }
    Some(RenderedFile {
        added,
        deleted,
        patch,
    })
}

enum Folded {
    Empty,
    Written(String),
    Hunks(Vec<Vec<TextEdit>>),
    Mixed {
        content: String,
        groups: Vec<Vec<TextEdit>>,
    },
}

fn fold_file(calls: &[FileCall]) -> Folded {
    let mut folded = Folded::Empty;
    for call in calls {
        folded = match (folded, call) {
            (_, FileCall::Write(content)) => Folded::Written(content.clone()),
            (Folded::Empty, FileCall::Edits(edits)) => Folded::Hunks(vec![edits.clone()]),
            (Folded::Hunks(mut groups), FileCall::Edits(edits)) => {
                groups.push(edits.clone());
                Folded::Hunks(groups)
            }
            (Folded::Written(content), FileCall::Edits(edits)) => {
                if let Some(next) = apply_edits_once(&content, edits) {
                    Folded::Written(next)
                } else {
                    Folded::Mixed {
                        content,
                        groups: vec![edits.clone()],
                    }
                }
            }
            (
                Folded::Mixed {
                    content,
                    mut groups,
                },
                FileCall::Edits(edits),
            ) => {
                groups.push(edits.clone());
                Folded::Mixed { content, groups }
            }
        };
    }
    folded
}

/// Apply every edit to `base` as it stood before the call. Each `old` must
/// occur exactly once, and the matches must not overlap. Otherwise the call
/// cannot be folded into the written file.
fn apply_edits_once(base: &str, edits: &[TextEdit]) -> Option<String> {
    if edits.is_empty() {
        return Some(base.to_string());
    }
    let mut ranges: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
    for edit in edits {
        if edit.old.is_empty() {
            return None;
        }
        let mut found = None;
        let mut search = 0;
        while let Some(pos) = base[search..].find(&edit.old) {
            let at = search + pos;
            if found.is_some() {
                return None;
            }
            found = Some(at);
            // Step one character, not `old.len()`. "aa" occurs twice in "aaa";
            // advancing by the whole needle hides the overlap.
            let step = base[at..].chars().next().map(char::len_utf8).unwrap_or(1);
            search = at + step;
        }
        let at = found?;
        ranges.push((at, at + edit.old.len(), edit.new.as_str()));
    }
    ranges.sort_by_key(|range| range.0);
    for pair in ranges.windows(2) {
        if pair[1].0 < pair[0].1 {
            return None;
        }
    }
    let mut result = base.to_string();
    for (start, end, new) in ranges.into_iter().rev() {
        result.replace_range(start..end, new);
    }
    Some(result)
}

fn push_write_hunk(
    patch: &mut String,
    lines: &[&str],
    added: &mut u64,
    limit: &mut EmitLimit,
) -> bool {
    *added += lines.len() as u64;
    let mut body = String::new();
    let mut emitted = 0usize;
    for line in lines {
        if !limit.take() {
            break;
        }
        emitted += 1;
        body.push('+');
        body.push_str(line);
        body.push('\n');
    }
    if emitted == 0 {
        return false;
    }
    patch.push_str(&format!("@@ -0,0 +1,{emitted} @@\n"));
    patch.push_str(&body);
    true
}

fn emit_edit_hunk(
    patch: &mut String,
    emit_header: &mut impl FnMut(&mut String),
    old: &str,
    new: &str,
    added: &mut u64,
    deleted: &mut u64,
    limit: &mut EmitLimit,
) {
    let mut hunk = String::new();
    if !push_edit_hunk(&mut hunk, old, new, added, deleted, limit) {
        return;
    }
    emit_header(patch);
    patch.push_str(&hunk);
}

fn push_edit_hunk(
    patch: &mut String,
    old: &str,
    new: &str,
    added: &mut u64,
    deleted: &mut u64,
    limit: &mut EmitLimit,
) -> bool {
    let old_lines = diff_lines(old);
    let new_lines = diff_lines(new);
    let ops = diff_line_ops(&old_lines, &new_lines);
    if ops.is_empty() {
        return false;
    }
    let mut body = String::new();
    let mut emitted = 0usize;
    for (kind, old_ix, new_ix) in ops {
        let (marker, text) = match kind {
            DiffLineKind::Context => (' ', old_ix.and_then(|ix| old_lines.get(ix)).copied()),
            DiffLineKind::Deletion => {
                *deleted += 1;
                ('-', old_ix.and_then(|ix| old_lines.get(ix)).copied())
            }
            DiffLineKind::Addition => {
                *added += 1;
                ('+', new_ix.and_then(|ix| new_lines.get(ix)).copied())
            }
        };
        if !limit.take() {
            continue;
        }
        emitted += 1;
        body.push(marker);
        body.push_str(text.unwrap_or(""));
        body.push('\n');
    }
    if emitted == 0 {
        return false;
    }
    patch.push_str("@@\n");
    patch.push_str(&body);
    true
}

/// How many synthesized diff lines may still be highlighted on the UI thread.
/// Counts in numstat stay complete after the cap is hit.
struct EmitLimit {
    remaining: usize,
    clipped: bool,
}

impl EmitLimit {
    fn take(&mut self) -> bool {
        if self.remaining == 0 {
            self.clipped = true;
            false
        } else {
            self.remaining -= 1;
            true
        }
    }
}

fn diff_lines(text: &str) -> Vec<&str> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

/// Path used in both numstat and the `diff --git` header. Workspace-relative
/// when the file sits inside the workspace; backslashes become `/`. A path
/// the header parser would split or unescape is omitted.
pub(crate) fn normalize_review_path(path: &str, workspace: Option<&Path>) -> Option<String> {
    let mut path = path.replace('\\', "/");
    if let Some(root) = workspace {
        let root = root.to_string_lossy().replace('\\', "/");
        let root = root.trim_end_matches('/').to_string();
        let prefix = format!("{root}/");
        if !root.is_empty() {
            if let Some(rest) = strip_workspace_prefix(&path, &prefix) {
                path = rest;
            }
        }
    }
    while let Some(rest) = path.strip_prefix("./") {
        path = rest.to_string();
    }
    path = path.trim_start_matches('/').to_string();
    if path.is_empty()
        || path.contains('\t')
        || path.contains('"')
        || path.contains('\n')
        || path.contains('\r')
        || path.contains(" b/")
    {
        return None;
    }
    Some(path)
}

/// `prefix` includes the trailing slash. Windows drive letters compare
/// case-insensitively; other platforms keep a byte prefix.
fn strip_workspace_prefix(path: &str, prefix: &str) -> Option<String> {
    let head = path.get(..prefix.len())?;
    let matches = if cfg!(windows) {
        head.eq_ignore_ascii_case(prefix)
    } else {
        head == prefix
    };
    matches.then(|| path[prefix.len()..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edit_call(path: &str, pairs: &[(&str, &str)]) -> RecordedCall {
        RecordedCall {
            path: path.to_string(),
            call: FileCall::Edits(
                pairs
                    .iter()
                    .map(|(old, new)| TextEdit {
                        old: (*old).to_string(),
                        new: (*new).to_string(),
                    })
                    .collect(),
            ),
        }
    }

    fn write_call(path: &str, content: &str) -> RecordedCall {
        RecordedCall {
            path: path.to_string(),
            call: FileCall::Write(content.to_string()),
        }
    }

    fn snapshot(calls: &[RecordedCall]) -> Snapshot {
        snapshot_from_calls(Source::LastTurn { turn_count: 1 }, calls, None)
    }

    #[test]
    fn steer_user_row_stays_inside_the_open_turn() {
        let mut clock = TurnClock::default();
        assert!(clock.begin(0));
        let ranges = clock.ranges(4);
        assert_eq!(ranges, vec![(1, 0, 4)]);
        assert!(!clock.begin(2), "an open turn does not start another");
    }

    #[test]
    fn rebuild_after_compaction_does_not_renumber_downward() {
        let mut clock = TurnClock {
            count: 5,
            open: false,
            latest: Some(5),
            starts: vec![(1, 0), (2, 3), (3, 6), (4, 9), (5, 12)],
        };
        clock.rebuild(&[0, 4]);
        assert_eq!(clock.count, 5);
        assert_eq!(clock.latest, Some(5));
        assert_eq!(clock.starts, vec![(4, 0), (5, 4)]);
    }

    #[test]
    fn parking_a_clock_round_trips_the_open_turn() {
        let mut active = TurnClock::default();
        active.begin(0);
        let parked = std::mem::take(&mut active);
        assert!(active.begin(0));
        assert_eq!(active.latest, Some(1));
        let resumed = parked;
        assert_eq!(resumed.latest, Some(1));
        assert!(resumed.open);
        assert_eq!(resumed.starts, vec![(1, 0)]);
        assert_eq!(active.starts, vec![(1, 0)]);
    }

    #[test]
    fn checkpoint_number_does_not_replace_latest() {
        let mut clock = TurnClock::default();
        clock.begin(0);
        clock.finish();
        clock.note_checkpoint(7);
        assert_eq!(clock.count, 7);
        assert_eq!(clock.latest, Some(1));
        clock.finish();
        assert_eq!(
            clock.latest,
            Some(1),
            "a later settle stays on the recorded turn"
        );
    }

    #[test]
    fn rebuild_without_user_rows_does_not_point_at_a_checkpoint_number() {
        let mut clock = TurnClock::default();
        clock.note_checkpoint(7);
        clock.rebuild(&[]);
        assert!(clock.starts.is_empty());
        assert_eq!(clock.latest, None);
    }

    #[test]
    fn all_changes_folds_a_later_edit_into_the_earlier_write() {
        let calls = vec![
            RecordedCall {
                path: "a.rs".into(),
                call: FileCall::Write("alpha\n".into()),
            },
            RecordedCall {
                path: "a.rs".into(),
                call: FileCall::Edits(vec![TextEdit {
                    old: "alpha".into(),
                    new: "beta".into(),
                }]),
            },
        ];
        assert_eq!(file_counts(&calls, None), vec![("a.rs".into(), 1, 0)]);
        let snapshot = snapshot_from_calls(Source::AllChanges, &calls, None);
        assert_eq!(snapshot.files[0].additions, 1);
        assert_eq!(snapshot.files[0].deletions, 0);
        assert!(snapshot.lines.iter().any(|line| line.content == "beta"));
    }

    #[test]
    fn a_deduped_resend_does_not_keep_the_turn_it_opened() {
        let mut clock = TurnClock::default();
        assert!(clock.begin(0));
        clock.finish();
        assert!(clock.begin(1));
        clock.cancel_begin();
        assert!(!clock.open);
        assert_eq!(clock.count, 1);
        assert_eq!(clock.latest, Some(1));
        assert_eq!(clock.starts, vec![(1, 0)]);
    }

    #[test]
    fn two_edits_in_one_call_apply_against_the_pre_call_text() {
        let calls = [
            write_call("a.rs", "alpha beta gamma\n"),
            edit_call("a.rs", &[("alpha", "ALPHA"), ("gamma", "GAMMA")]),
        ];
        let snapshot = snapshot(&calls);
        let text: String = snapshot
            .lines
            .iter()
            .filter(|line| matches!(line.kind, LineKind::Addition))
            .map(|line| line.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(text, "ALPHA beta GAMMA");
        assert!(snapshot
            .lines
            .iter()
            .all(|line| !matches!(line.kind, LineKind::Deletion)));
    }

    #[test]
    fn a_repeated_old_text_falls_back_to_hunks() {
        let calls = [
            write_call("a.rs", "foo foo\n"),
            edit_call("a.rs", &[("foo", "bar")]),
        ];
        let snapshot = snapshot(&calls);
        let deleted = snapshot
            .lines
            .iter()
            .any(|line| matches!(line.kind, LineKind::Deletion) && line.content == "foo");
        assert!(deleted, "a non-unique match stays a hunk");
    }

    #[test]
    fn a_later_write_replaces_earlier_edits() {
        let calls = [
            edit_call("a.rs", &[("old", "new")]),
            write_call("a.rs", "final\n"),
        ];
        let snapshot = snapshot(&calls);
        let text: String = snapshot
            .lines
            .iter()
            .filter(|line| matches!(line.kind, LineKind::Addition))
            .map(|line| line.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(text, "final");
        assert!(snapshot
            .lines
            .iter()
            .all(|line| !matches!(line.kind, LineKind::Deletion)));
    }

    #[test]
    fn successive_writes_keep_the_last_body() {
        let calls = [
            write_call("a.rs", "first\n"),
            write_call("a.rs", "second\n"),
        ];
        let snapshot = snapshot(&calls);
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].additions, 1);
        assert!(snapshot.lines.iter().any(|line| line.content == "second"));
        assert!(snapshot.lines.iter().all(|line| line.content != "first"));
    }

    #[test]
    fn a_failed_tool_is_omitted_and_an_aborted_call_without_output_is_omitted() {
        let kept = json!({"path": "a.rs", "content": "ok\n"});
        let dropped = json!({"path": "b.rs", "content": "no\n"});
        let calls = recorded_calls([
            ToolView {
                message_failed: false,
                failed: true,
                has_output: true,
                name: "write",
                args: Some(&dropped),
            },
            ToolView {
                message_failed: true,
                failed: false,
                has_output: false,
                name: "write",
                args: Some(&dropped),
            },
            ToolView {
                message_failed: true,
                failed: false,
                has_output: true,
                name: "write",
                args: Some(&kept),
            },
        ]);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].path, "a.rs");
    }

    #[test]
    fn a_windows_absolute_path_is_workspace_relative_in_the_header() {
        let calls = [write_call(r"F:\proj\src\a.rs", "line\n")];
        let workspace = Path::new(r"F:\proj");
        let snapshot =
            snapshot_from_calls(Source::LastTurn { turn_count: 1 }, &calls, Some(workspace));
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].path, "src/a.rs");
        assert_eq!(snapshot.files[0].additions, 1);
    }

    #[test]
    fn a_write_numbers_its_first_line_from_one_and_an_empty_write_is_absent() {
        let written = snapshot(&[write_call("a.rs", "one\ntwo\n")]);
        let first = written
            .lines
            .iter()
            .find(|line| matches!(line.kind, LineKind::Addition))
            .expect("addition");
        assert_eq!(first.new_line, Some(1));
        assert_eq!(first.content, "one");
        let empty = snapshot(&[write_call("a.rs", "")]);
        assert!(empty.files.is_empty());
    }

    #[test]
    fn last_turn_snapshot_does_not_require_a_repository() {
        let snapshot = snapshot(&[edit_call("src/lib.rs", &[("old", "new")])]);
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].path, "src/lib.rs");
        assert!(snapshot
            .lines
            .iter()
            .any(|line| { matches!(line.kind, LineKind::Addition) && line.content == "new" }));
    }

    #[test]
    fn an_overlapping_needle_falls_back_to_a_hunk() {
        let calls = [
            write_call("a.rs", "aaa\n"),
            edit_call("a.rs", &[("aa", "b")]),
        ];
        let snapshot = snapshot(&calls);
        assert!(
            snapshot
                .lines
                .iter()
                .any(|line| { matches!(line.kind, LineKind::Deletion) && line.content == "aa" }),
            "aa occurs twice in aaa, so the edit stays a hunk"
        );
    }

    #[test]
    fn an_empty_edit_does_not_emit_a_header_only_file() {
        let snapshot = snapshot(&[edit_call("a.rs", &[("", "")])]);
        assert!(snapshot.files.is_empty());
    }

    #[test]
    fn a_capped_write_keeps_the_full_line_count() {
        let snapshot = snapshot_capped(
            Source::LastTurn { turn_count: 1 },
            &[write_call("a.rs", "one\ntwo\nthree\n")],
            None,
            1,
        );
        assert_eq!(snapshot.files[0].additions, 3);
        assert!(snapshot.truncated);
        let shown = snapshot
            .lines
            .iter()
            .filter(|line| matches!(line.kind, LineKind::Addition))
            .count();
        assert_eq!(shown, 1);
    }

    #[test]
    fn a_path_the_header_parser_would_split_is_omitted() {
        assert!(normalize_review_path("a\tb.rs", None).is_none());
        assert!(normalize_review_path("a\"b.rs", None).is_none());
        assert!(normalize_review_path("a\nb.rs", None).is_none());
        assert!(normalize_review_path("dir b/file.rs", None).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_prefix_match_ignores_drive_letter_case() {
        let calls = [write_call(r"f:\proj\src\a.rs", "line\n")];
        let workspace = Path::new(r"F:\proj");
        let snapshot =
            snapshot_from_calls(Source::LastTurn { turn_count: 1 }, &calls, Some(workspace));
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].path, "src/a.rs");
    }
}
