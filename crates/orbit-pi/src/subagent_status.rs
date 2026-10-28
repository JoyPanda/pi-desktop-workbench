//! Background subagent status for the pi-subagents RPC host protocol.
//!
//! The extension pushes a bounded `pi-subagents.async-status-snapshot` on the
//! `subagent-async` widget (`PI_SUBAGENT_ASYNC_JSON:`). That snapshot is a
//! tree of who is running, not their output. Output arrives on demand:
//! `/subagents-inspect-rpc` answers on the `subagent-inspect` widget
//! (`PI_SUBAGENT_INSPECT_JSON:`), which hosts buffer and do not render.
//!
//! This module owns the parsed board: stable leaf order, the one open
//! transcript, per-leaf scroll anchors, and which inspect requests are due.
//! The shell paints it.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const ASYNC_WIDGET_KEY: &str = "subagent-async";
pub const INSPECT_WIDGET_KEY: &str = "subagent-inspect";

const ASYNC_PREFIXES: [&str; 2] = ["PI_SUBAGENT_ASYNC_JSON:", "SELESAI_SUBAGENT_ASYNC_JSON:"];
const INSPECT_PREFIXES: [&str; 2] = [
    "PI_SUBAGENT_INSPECT_JSON:",
    "SELESAI_SUBAGENT_INSPECT_JSON:",
];

const LINES_DEFAULT: u32 = 80;
const LINES_EXPANDED: u32 = 200;

/// Salt shared with the subagent markdown renderer. Fold keys are
/// `(message index, FOLD_SALT, block index)` while painted, and content
/// hashes once reconciled.
pub const FOLD_SALT: u64 = 0;

const TERMINAL_STATES: [&str; 4] = ["complete", "failed", "stopped", "rejected"];

/// A subagent row the list can show. A run with zero or one child is one row.
/// A run with two or more children is a group of rows, one per child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    pub id: String,
    pub run_id: String,
    pub child_id: Option<String>,
    pub label: String,
    /// Run label when this leaf is one of several children; the list prints it
    /// once above the first of those children.
    pub group: Option<String>,
    pub state: String,
    pub started_at: i64,
    pub updated_at: i64,
}

impl Leaf {
    pub fn terminal(&self) -> bool {
        TERMINAL_STATES.contains(&self.state.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRow {
    Group(String),
    Agent(String),
    Omitted(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    ToolCall,
    ToolResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMessage {
    pub role: String,
    pub kind: MessageKind,
    pub text: String,
    pub name: Option<String>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cache {
    messages: Vec<TranscriptMessage>,
    final_output: Option<String>,
    truncated_messages: bool,
    /// Set when the latest inspect failed and the previous body was kept.
    stale: bool,
    /// `error.code` from that failed reply, shown beside the title.
    error_code: Option<String>,
    fetched_updated_at: Option<i64>,
    fetched_terminal: bool,
    lines: u32,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            final_output: None,
            truncated_messages: false,
            stale: false,
            error_code: None,
            fetched_updated_at: None,
            fetched_terminal: false,
            lines: 0,
        }
    }
}

/// Where the reader left a transcript. `following` sticks to the newest line.
/// Otherwise `from_bottom` is how many messages sit at or below the anchor,
/// so a sliding tail window does not jump the place they were reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub following: bool,
    pub from_bottom: usize,
}

impl Default for Anchor {
    fn default() -> Self {
        Self {
            following: true,
            from_bottom: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Inflight {
    request_id: String,
    lines: u32,
}

/// An inspect the shell should send. One leaf has at most one in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub leaf_id: String,
    pub run_id: String,
    pub child_id: Option<String>,
    pub lines: u32,
}

impl Due {
    pub fn command(&self, request_id: &str) -> String {
        match &self.child_id {
            Some(child) => format!(
                "/subagents-inspect-rpc {request_id} {} {child} --lines {}",
                self.run_id, self.lines
            ),
            None => format!(
                "/subagents-inspect-rpc {request_id} {} --lines {}",
                self.run_id, self.lines
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentBoard {
    order: Vec<String>,
    leaves: HashMap<String, Leaf>,
    /// The open transcript after its run left the snapshot.
    held: Option<Leaf>,
    caches: HashMap<String, Cache>,
    anchors: HashMap<String, Anchor>,
    /// Leaf ids whose reader is at the top of a truncated window.
    at_top: HashSet<String>,
    inflight: HashMap<String, Inflight>,
    request_leaf: HashMap<String, String>,
    next_request: u64,
    selected: Option<String>,
    open: bool,
    /// The reader closed the panel; do not auto-open again this session.
    explicitly_closed: bool,
    /// Review or Project took the dock; do not snap the panel back when they close.
    yielded: bool,
    omitted_runs: u64,
    list_following: bool,
    /// Bumps when the open transcript should be placed from its anchor.
    scroll_epoch: u64,
    /// Code blocks the reader expanded, keyed by leaf id then content hash.
    /// The same source stays open when a later inspect window still contains
    /// it; a different body starts folded.
    expanded_code: HashMap<String, HashSet<u64>>,
}

impl Default for SubagentBoard {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            leaves: HashMap::new(),
            held: None,
            caches: HashMap::new(),
            anchors: HashMap::new(),
            at_top: HashSet::new(),
            inflight: HashMap::new(),
            request_leaf: HashMap::new(),
            next_request: 1,
            selected: None,
            open: false,
            explicitly_closed: false,
            yielded: false,
            omitted_runs: 0,
            list_following: true,
            scroll_epoch: 0,
            expanded_code: HashMap::new(),
        }
    }
}

impl SubagentBoard {
    pub fn is_open(&self) -> bool {
        self.open && self.viewed().is_some()
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn list_following(&self) -> bool {
        self.list_following
    }

    pub fn set_list_following(&mut self, following: bool) {
        self.list_following = following;
    }

    /// The leaf whose transcript the panel shows: a live row, or the one kept
    /// after the snapshot dropped it.
    pub fn viewed(&self) -> Option<&Leaf> {
        let id = self.selected.as_deref()?;
        self.leaves
            .get(id)
            .or_else(|| self.held.as_ref().filter(|leaf| leaf.id == id))
    }

    pub fn anchor(&self, id: &str) -> Anchor {
        self.anchors.get(id).copied().unwrap_or_default()
    }

    pub fn scroll_epoch(&self) -> u64 {
        self.scroll_epoch
    }

    /// The list stays while any leaf is still running, or while the snapshot
    /// says more runs were left out. Once every listed run has ended and
    /// nothing was omitted, the list hides and an open transcript stays put.
    pub fn list_shown(&self) -> bool {
        self.omitted_runs > 0
            || self
                .order
                .iter()
                .any(|id| self.leaves.get(id).is_some_and(|leaf| !leaf.terminal()))
    }

    pub fn rows(&self) -> Vec<ListRow> {
        let mut rows = Vec::new();
        let mut prev_run: Option<String> = None;
        for id in &self.order {
            let Some(leaf) = self.leaves.get(id) else {
                continue;
            };
            if let Some(group) = leaf.group.as_deref() {
                if prev_run.as_deref() != Some(leaf.run_id.as_str()) {
                    rows.push(ListRow::Group(group.to_string()));
                }
            }
            prev_run = Some(leaf.run_id.clone());
            rows.push(ListRow::Agent(id.clone()));
        }
        if self.omitted_runs > 0 {
            rows.push(ListRow::Omitted(self.omitted_runs));
        }
        rows
    }

    /// Display label. The second and later leaves that share a name gain
    /// ` · 2`, ` · 3`, in first-seen order.
    pub fn display_label(&self, id: &str) -> String {
        let Some(leaf) = self.leaf(id) else {
            return String::new();
        };
        let mut ordinal = 0u32;
        let mut count = 0u32;
        for existing in &self.order {
            let Some(other) = self.leaves.get(existing) else {
                continue;
            };
            if other.label == leaf.label {
                count += 1;
                if other.id == leaf.id {
                    ordinal = count;
                }
            }
        }
        if let Some(held) = &self.held {
            if held.id == leaf.id && ordinal == 0 {
                ordinal = 1;
                count = 1;
            }
        }
        if count > 1 && ordinal > 1 {
            format!("{} · {ordinal}", leaf.label)
        } else {
            leaf.label.clone()
        }
    }

    pub fn preview(&self, id: &str) -> String {
        let Some(cache) = self.caches.get(id) else {
            return String::new();
        };
        if let Some(final_output) = cache
            .final_output
            .as_deref()
            .filter(|text| !text.is_empty())
        {
            return readable_preview(final_output);
        }
        let Some(message) = cache
            .messages
            .iter()
            .rev()
            .find(|message| !message.text.trim().is_empty())
        else {
            return String::new();
        };
        if message.kind == MessageKind::Text {
            readable_preview(&message.text)
        } else {
            let line = message
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            snippet(line.trim(), 96)
        }
    }

    pub fn code_expanded(&self, id: &str, hash: u64) -> bool {
        self.expanded_code
            .get(id)
            .is_some_and(|open| open.contains(&hash))
    }

    pub fn set_code_expanded(&mut self, id: &str, hash: u64, open: bool) {
        let entry = self.expanded_code.entry(id.to_string()).or_default();
        if open {
            entry.insert(hash);
        } else {
            entry.remove(&hash);
        }
        if entry.is_empty() {
            self.expanded_code.remove(id);
        }
    }

    /// Fold clicks are stored by position in the last painted window. A key
    /// that changed since it was seeded updates that block's content hash, so
    /// the next window can find the same code. Untouched keys, and hashes
    /// whose block left the window, stay as they were. Two copies of the same
    /// code share one hash: opening either opens both on the next paint.
    pub fn reconcile_code_folds(
        &mut self,
        id: &str,
        layout: &[(usize, usize, u64)],
        open: &HashSet<(usize, u64, usize)>,
        seeded: &HashSet<(usize, u64, usize)>,
    ) {
        for &(message_ix, block_ix, hash) in layout {
            let key = (message_ix, FOLD_SALT, block_ix);
            let now = open.contains(&key);
            if now != seeded.contains(&key) {
                self.set_code_expanded(id, hash, now);
            }
        }
    }

    pub fn messages(&self, id: &str) -> &[TranscriptMessage] {
        self.caches
            .get(id)
            .map(|cache| cache.messages.as_slice())
            .unwrap_or(&[])
    }

    pub fn final_output(&self, id: &str) -> Option<&str> {
        self.caches
            .get(id)
            .and_then(|cache| cache.final_output.as_deref())
            .filter(|text| !text.is_empty())
    }

    pub fn stale(&self, id: &str) -> bool {
        self.caches.get(id).is_some_and(|cache| cache.stale)
    }

    pub fn truncated(&self, id: &str) -> bool {
        self.caches
            .get(id)
            .is_some_and(|cache| cache.truncated_messages)
    }

    pub fn copy_text(&self, id: &str) -> String {
        let mut out = String::new();
        for message in self.messages(id) {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            if let Some(name) = &message.name {
                out.push_str(name);
                out.push('\n');
            }
            out.push_str(message.text.trim_end());
        }
        if let Some(final_output) = self.final_output(id) {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(final_output.trim_end());
        }
        out
    }

    /// Replace the live tree from one widget line. An empty payload drops the
    /// live rows. The open transcript stays when its run disappears.
    pub fn apply_async_lines(&mut self, lines: &[String]) {
        let payload = lines.iter().find_map(|line| async_payload(line));
        let Some(payload) = payload else {
            self.apply_runs(Vec::new(), 0);
            return;
        };
        let Ok(value) = serde_json::from_str::<Value>(&payload) else {
            return;
        };
        self.apply_snapshot_value(&value);
    }

    pub fn apply_snapshot_value(&mut self, value: &Value) {
        let omitted = value
            .get("omitted")
            .and_then(|omitted| omitted.get("runs"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let runs = value
            .get("runs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let parsed = runs.iter().filter_map(parse_run).flatten().collect();
        self.apply_runs(parsed, omitted);
    }

    fn apply_runs(&mut self, incoming: Vec<Leaf>, omitted: u64) {
        self.omitted_runs = omitted;
        let incoming_ids: HashSet<String> = incoming.iter().map(|leaf| leaf.id.clone()).collect();
        // Keep first-seen order. New leaves append, sorted by start then id.
        self.order.retain(|id| incoming_ids.contains(id));
        let mut fresh: Vec<Leaf> = Vec::new();
        for leaf in incoming {
            if let Some(existing) = self.leaves.get_mut(&leaf.id) {
                *existing = leaf;
            } else if self.order.iter().any(|id| id == &leaf.id) {
                self.leaves.insert(leaf.id.clone(), leaf);
            } else {
                fresh.push(leaf);
            }
        }
        fresh.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        for leaf in fresh {
            self.order.push(leaf.id.clone());
            self.leaves.insert(leaf.id.clone(), leaf);
        }
        // Drop leaves the snapshot no longer carries, except the open one.
        let open_id = self
            .open
            .then(|| self.selected.clone())
            .flatten()
            .filter(|id| !incoming_ids.contains(id));
        if let Some(id) = open_id {
            if self.held.as_ref().map(|leaf| &leaf.id) != Some(&id) {
                if let Some(leaf) = self.leaves.get(&id).cloned() {
                    self.held = Some(leaf);
                }
            }
        } else if self
            .selected
            .as_ref()
            .is_some_and(|id| incoming_ids.contains(id))
        {
            self.held = None;
        }
        self.leaves.retain(|id, _| incoming_ids.contains(id));
        self.order.retain(|id| self.leaves.contains_key(id));
        let held = self.held.as_ref().map(|leaf| leaf.id.clone());
        let selected = self.selected.clone();
        self.expanded_code.retain(|id, _| {
            self.leaves.contains_key(id)
                || held.as_ref() == Some(id)
                || selected.as_ref() == Some(id)
        });
    }

    /// `docks_free` is false when Review or Project already owns the right dock.
    /// Passing `false` skips the check the caller already made; the board still
    /// refuses when the reader dismissed or yielded the panel.
    pub fn consider_auto_open(&mut self, docks_free: bool) {
        if self.open || self.explicitly_closed || self.yielded || !docks_free {
            return;
        }
        if self.order.len() == 1 {
            self.selected = Some(self.order[0].clone());
            self.open = true;
            self.scroll_epoch = self.scroll_epoch.saturating_add(1);
        }
    }

    /// Open `id`, or close the panel when it is already showing `id`.
    pub fn click(&mut self, id: &str) -> bool {
        if !self.leaves.contains_key(id)
            && self.held.as_ref().map(|leaf| leaf.id.as_str()) != Some(id)
        {
            return self.open;
        }
        if self.open && self.selected.as_deref() == Some(id) {
            self.close_explicit();
            return false;
        }
        self.selected = Some(id.to_string());
        self.open = true;
        self.yielded = false;
        self.scroll_epoch = self.scroll_epoch.saturating_add(1);
        true
    }

    pub fn close_explicit(&mut self) {
        self.open = false;
        self.explicitly_closed = true;
    }

    /// Review or Project replaced this panel. The reader's place is kept, and
    /// closing that other dock does not snap this one back open.
    pub fn yield_dock(&mut self) {
        if self.open {
            self.open = false;
            self.yielded = true;
        }
    }

    pub fn note_follow(&mut self, id: &str, following: bool, from_bottom: usize) {
        self.anchors.insert(
            id.to_string(),
            Anchor {
                following,
                from_bottom,
            },
        );
        if following {
            self.at_top.remove(id);
        }
    }

    pub fn set_at_top(&mut self, id: &str, at_top: bool) {
        if at_top {
            self.at_top.insert(id.to_string());
        } else {
            self.at_top.remove(id);
        }
    }

    pub fn due(&self) -> Vec<Due> {
        let mut due = Vec::new();
        for id in &self.order {
            let Some(leaf) = self.leaves.get(id) else {
                continue;
            };
            if self.inflight.contains_key(id) {
                continue;
            }
            let cache = self.caches.get(id);
            let lines = if self.at_top.contains(id)
                && cache.is_some_and(|cache| cache.truncated_messages)
                && cache.is_some_and(|cache| cache.lines < LINES_EXPANDED)
            {
                LINES_EXPANDED
            } else {
                LINES_DEFAULT
            };
            let needs = match cache {
                None => true,
                Some(cache) => {
                    let moved = cache.fetched_updated_at != Some(leaf.updated_at);
                    let ended = leaf.terminal() && !cache.fetched_terminal;
                    let expand = lines == LINES_EXPANDED && cache.lines < LINES_EXPANDED;
                    if leaf.terminal() && cache.fetched_terminal && !expand {
                        false
                    } else {
                        moved || ended || expand
                    }
                }
            };
            if needs {
                due.push(Due {
                    leaf_id: leaf.id.clone(),
                    run_id: leaf.run_id.clone(),
                    child_id: leaf.child_id.clone(),
                    lines,
                });
            }
        }
        due
    }

    pub fn alloc_request_id(&mut self) -> String {
        let id = format!("sa{}", self.next_request);
        self.next_request += 1;
        id
    }

    pub fn note_inflight(&mut self, leaf_id: &str, request_id: &str, lines: u32) {
        self.inflight.insert(
            leaf_id.to_string(),
            Inflight {
                request_id: request_id.to_string(),
                lines,
            },
        );
        self.request_leaf
            .insert(request_id.to_string(), leaf_id.to_string());
    }

    /// The inspect reply was cancelled while this session was parked, so it
    /// will never arrive. Forget the in-flight request so `due` can ask again.
    /// A late reply for the old id is ignored.
    pub fn drop_inflight(&mut self) {
        self.inflight.clear();
        self.request_leaf.clear();
    }

    /// Buffer one inspect widget. A retract (no payload) keeps the last body.
    /// A reply applies only to the leaf its `requestId` was issued for.
    pub fn apply_inspect_lines(&mut self, lines: &[String]) {
        let Some(payload) = lines.iter().find_map(|line| inspect_payload(line)) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<Value>(&payload) else {
            return;
        };
        self.apply_inspect_value(&value);
    }

    pub fn apply_inspect_value(&mut self, value: &Value) {
        let Some(request_id) = value.get("requestId").and_then(Value::as_str) else {
            return;
        };
        let Some(leaf_id) = self.request_leaf.get(request_id).cloned() else {
            return;
        };
        let lines = self
            .inflight
            .get(&leaf_id)
            .filter(|inflight| inflight.request_id == request_id)
            .map(|inflight| inflight.lines)
            .unwrap_or(LINES_DEFAULT);
        self.inflight.remove(&leaf_id);
        let error = value.get("error").and_then(|error| error.get("code"));
        let mut cache = self.caches.remove(&leaf_id).unwrap_or_default();
        if error.is_some() {
            cache.stale = true;
            cache.error_code = error.and_then(Value::as_str).map(str::to_string);
            cache.lines = cache.lines.max(lines);
            if let Some(leaf) = self.leaves.get(&leaf_id) {
                cache.fetched_updated_at = Some(leaf.updated_at);
                cache.fetched_terminal = leaf.terminal();
            }
        } else {
            cache.stale = false;
            cache.error_code = None;
            cache.messages = value
                .get("messages")
                .and_then(Value::as_array)
                .map(|messages| messages.iter().filter_map(parse_message).collect())
                .unwrap_or_default();
            cache.final_output = value
                .get("finalOutput")
                .and_then(Value::as_str)
                .map(str::to_string);
            cache.truncated_messages = value
                .get("truncated")
                .and_then(|truncated| truncated.get("messages"))
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0)
                || value
                    .get("truncated")
                    .and_then(|truncated| truncated.get("messages"))
                    .and_then(Value::as_bool)
                    == Some(true);
            cache.lines = lines;
            if let Some(leaf) = self.leaves.get(&leaf_id) {
                cache.fetched_updated_at = Some(leaf.updated_at);
                cache.fetched_terminal = leaf.terminal();
            }
            let len = cache.messages.len();
            let anchor = self.anchors.entry(leaf_id.clone()).or_default();
            if anchor.following {
                anchor.from_bottom = 0;
            } else {
                anchor.from_bottom = anchor.from_bottom.min(len);
            }
        }
        let viewing = self.selected.as_deref() == Some(leaf_id.as_str());
        self.caches.insert(leaf_id, cache);
        if viewing {
            self.scroll_epoch = self.scroll_epoch.saturating_add(1);
        }
    }

    pub fn state(&self, id: &str) -> Option<&str> {
        self.leaf(id).map(|leaf| leaf.state.as_str())
    }

    pub fn error_code(&self, id: &str) -> Option<&str> {
        self.caches
            .get(id)
            .and_then(|cache| cache.error_code.as_deref())
    }

    fn leaf(&self, id: &str) -> Option<&Leaf> {
        self.leaves
            .get(id)
            .or_else(|| self.held.as_ref().filter(|leaf| leaf.id == id))
    }
}

pub fn is_async_widget(key: &str, lines: &[String]) -> bool {
    key == ASYNC_WIDGET_KEY || lines.iter().any(|line| async_payload(line).is_some())
}

pub fn is_inspect_widget(key: &str, lines: &[String]) -> bool {
    key == INSPECT_WIDGET_KEY || lines.iter().any(|line| inspect_payload(line).is_some())
}

fn async_payload(line: &str) -> Option<String> {
    payload_after(line, &ASYNC_PREFIXES)
}

fn inspect_payload(line: &str) -> Option<String> {
    payload_after(line, &INSPECT_PREFIXES)
}

fn payload_after(line: &str, prefixes: &[&str]) -> Option<String> {
    let line = strip_ansi(line);
    let rest = prefixes.iter().find_map(|prefix| {
        line.trim()
            .strip_prefix(prefix)
            .map(str::trim)
            .filter(|rest| !rest.is_empty())
    })?;
    Some(rest.to_string())
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn parse_run(value: &Value) -> Option<Vec<Leaf>> {
    let run_id = value.get("id")?.as_str()?.to_string();
    let label = text_field(value, "label").unwrap_or_else(|| run_id.clone());
    let state = text_field(value, "state").unwrap_or_else(|| "running".to_string());
    let started_at = millis(value, "startedAt");
    let updated_at = millis(value, "updatedAt").max(started_at);
    let children = value
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if children.len() >= 2 {
        let mut leaves = Vec::new();
        for child in &children {
            let Some(child_id) = child.get("id").and_then(Value::as_str) else {
                continue;
            };
            leaves.push(Leaf {
                id: leaf_id(&run_id, Some(child_id)),
                run_id: run_id.clone(),
                child_id: Some(child_id.to_string()),
                label: text_field(child, "label").unwrap_or_else(|| label.clone()),
                group: Some(label.clone()),
                state: text_field(child, "state").unwrap_or_else(|| state.clone()),
                started_at: millis(child, "startedAt").max(started_at),
                updated_at: millis(child, "updatedAt").max(updated_at),
            });
        }
        return Some(leaves);
    }
    let child_id = children
        .first()
        .and_then(|child| child.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let child_updated = children
        .first()
        .map(|child| millis(child, "updatedAt"))
        .unwrap_or(0);
    Some(vec![Leaf {
        id: leaf_id(&run_id, child_id.as_deref()),
        run_id,
        child_id,
        label,
        group: None,
        state,
        started_at,
        updated_at: updated_at.max(child_updated),
    }])
}

fn leaf_id(run_id: &str, child_id: Option<&str>) -> String {
    match child_id {
        Some(child) => format!("{run_id}#{child}"),
        None => run_id.to_string(),
    }
}

fn parse_message(value: &Value) -> Option<TranscriptMessage> {
    let kind = match value.get("kind").and_then(Value::as_str).unwrap_or("text") {
        "toolCall" => MessageKind::ToolCall,
        "toolResult" => MessageKind::ToolResult,
        _ => MessageKind::Text,
    };
    Some(TranscriptMessage {
        role: text_field(value, "role").unwrap_or_default(),
        kind,
        text: text_field(value, "text").unwrap_or_default(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_error: value.get("isError").and_then(Value::as_bool) == Some(true),
    })
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn millis(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .or_else(|| value.get(key).and_then(Value::as_u64).map(|n| n as i64))
        .unwrap_or(0)
}

/// First readable line of a markdown body. Fence markers are skipped. A
/// heading contributes its text, not the hashes.
fn readable_preview(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("```") {
            continue;
        }
        let readable = strip_heading(trimmed);
        if readable.is_empty() {
            continue;
        }
        return snippet(readable, 96);
    }
    String::new()
}

fn strip_heading(line: &str) -> &str {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&hashes)
        && line
            .as_bytes()
            .get(hashes)
            .is_some_and(u8::is_ascii_whitespace)
    {
        return line[hashes..].trim();
    }
    line
}

fn snippet(text: &str, max: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.chars().count() >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(runs: Value, omitted_runs: u64) -> Value {
        json!({
            "kind": "pi-subagents.async-status-snapshot",
            "version": 1,
            "omitted": { "runs": omitted_runs, "children": 0 },
            "runs": runs,
        })
    }

    fn run(
        id: &str,
        label: &str,
        state: &str,
        started: i64,
        updated: i64,
        children: Value,
    ) -> Value {
        json!({
            "id": id,
            "kind": "subagent",
            "label": label,
            "state": state,
            "startedAt": started,
            "updatedAt": updated,
            "activity": { "currentTool": "code_query" },
            "children": children,
        })
    }

    fn child(id: &str, label: &str, updated: i64) -> Value {
        json!({
            "id": id,
            "kind": "step",
            "label": label,
            "state": "running",
            "startedAt": updated,
            "updatedAt": updated,
        })
    }

    #[test]
    fn screenshot_snapshot_is_one_reviewer_row() {
        let line = r#"PI_SUBAGENT_ASYNC_JSON:{"kind":"pi-subagents.async-status-snapshot","version":1,"omitted":{"runs":0,"children":0},"runs":[{"id":"5b00972b-528b-4208-b6c6-2f22e4fa8b9e","kind":"subagent","label":"reviewer","state":"running","startedAt":1793101321957,"updatedAt":1793101326306,"activity":{"currentTool":"code_query"},"turnCount":1,"children":[{"id":"step:0","kind":"step","label":"review","state":"running","startedAt":1793101321980,"updatedAt":1793101326306,"turnCount":1}]}]}"#;
        let mut board = SubagentBoard::default();
        board.apply_async_lines(&[line.to_string()]);
        board.consider_auto_open(true);
        let rows = board.rows();
        assert_eq!(rows.len(), 1);
        let ListRow::Agent(id) = &rows[0] else {
            panic!("expected one agent row");
        };
        let leaf = board
            .viewed()
            .unwrap_or_else(|| board.leaves.get(id).unwrap());
        assert_eq!(leaf.label, "reviewer");
        assert_eq!(leaf.child_id.as_deref(), Some("step:0"));
        assert!(leaf.group.is_none());
        assert!(
            board.is_open(),
            "a single agent opens when the dock is free"
        );
    }

    #[test]
    fn two_children_become_a_group_not_a_nested_run() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run(
                "run-a",
                "workflow",
                "running",
                10,
                20,
                json!([
                    child("step:0", "scout", 20),
                    child("step:1", "reviewer", 20)
                ])
            )]),
            0,
        ));
        assert!(!board.is_open(), "several leaves stay closed");
        assert_eq!(
            board.rows(),
            vec![
                ListRow::Group("workflow".into()),
                ListRow::Agent(leaf_id("run-a", Some("step:0"))),
                ListRow::Agent(leaf_id("run-a", Some("step:1"))),
            ]
        );
    }

    #[test]
    fn later_snapshot_keeps_first_seen_order() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([
                run("a", "reviewer", "running", 1, 10, json!([])),
                run("b", "reviewer", "running", 2, 11, json!([])),
            ]),
            0,
        ));
        board.apply_snapshot_value(&snapshot(
            json!([
                run("b", "reviewer", "running", 2, 99, json!([])),
                run("a", "reviewer", "running", 1, 12, json!([])),
            ]),
            0,
        ));
        let ids: Vec<_> = board
            .rows()
            .into_iter()
            .filter_map(|row| match row {
                ListRow::Agent(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(board.display_label("a"), "reviewer");
        assert_eq!(board.display_label("b"), "reviewer · 2");
        assert_eq!(board.selected_id(), None);
    }

    #[test]
    fn finishing_keeps_the_open_transcript_after_the_run_drops() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        board.consider_auto_open(true);
        let id = board.selected_id().unwrap().to_string();
        let request = board.alloc_request_id();
        board.note_inflight(&id, &request, 80);
        board.apply_inspect_value(&json!({
            "kind": "pi-subagents.inspect-reply",
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "looking" }],
            "finalOutput": "ship it",
        }));
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "complete", 1, 12, json!([]))]),
            0,
        ));
        assert_eq!(board.selected_id(), Some(id.as_str()));
        assert!(board.is_open());
        board.apply_snapshot_value(&snapshot(json!([]), 0));
        assert!(board.rows().is_empty());
        assert_eq!(board.selected_id(), Some(id.as_str()));
        assert_eq!(board.messages(&id)[0].text, "looking");
        assert_eq!(board.final_output(&id), Some("ship it"));
        assert!(board.due().is_empty());
    }

    #[test]
    fn detached_anchor_keeps_its_distance_from_the_bottom() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let id = board.order[0].clone();
        board.note_follow(&id, false, 4);
        let request = board.alloc_request_id();
        board.note_inflight(&id, &request, 80);
        let messages: Vec<_> = (0..10)
            .map(|n| json!({ "role": "assistant", "kind": "text", "text": format!("m{n}") }))
            .collect();
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": messages,
        }));
        assert_eq!(
            board.anchor(&id),
            Anchor {
                following: false,
                from_bottom: 4
            }
        );
        board.note_follow(&id, true, 0);
        let request = board.alloc_request_id();
        board.note_inflight(&id, &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "tail" }],
        }));
        assert!(board.anchor(&id).following);
        assert_eq!(board.anchor(&id).from_bottom, 0);
    }

    #[test]
    fn late_reply_updates_only_its_own_leaf() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([
                run("a", "one", "running", 1, 10, json!([])),
                run("b", "two", "running", 2, 11, json!([])),
            ]),
            0,
        ));
        board.click("b");
        let req_a = board.alloc_request_id();
        let req_b = board.alloc_request_id();
        board.note_inflight("a", &req_a, 80);
        board.note_inflight("b", &req_b, 80);
        board.apply_inspect_value(&json!({
            "requestId": req_a,
            "messages": [{ "role": "assistant", "kind": "text", "text": "from a" }],
        }));
        assert_eq!(board.messages("a")[0].text, "from a");
        assert!(board.messages("b").is_empty());
        assert_eq!(board.selected_id(), Some("b"));
    }

    #[test]
    fn explicit_close_blocks_auto_open() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        board.consider_auto_open(true);
        assert!(board.is_open());
        board.close_explicit();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 11, json!([]))]),
            0,
        ));
        assert!(!board.is_open());
    }

    #[test]
    fn inspect_widget_is_not_an_async_widget() {
        let lines = vec!["PI_SUBAGENT_INSPECT_JSON:{\"requestId\":\"sa1\"}".into()];
        assert!(is_inspect_widget("subagent-inspect", &lines));
        assert!(!is_async_widget("subagent-inspect", &lines));
        let async_lines = vec!["SELESAI_SUBAGENT_ASYNC_JSON:{\"runs\":[]}".into()];
        assert!(is_async_widget("other", &async_lines));
    }

    #[test]
    fn omitted_runs_are_a_trailing_row_and_commands_quote_the_child() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run(
                "run-a",
                "reviewer",
                "running",
                1,
                2,
                json!([child("step:0", "review", 2)])
            )]),
            3,
        ));
        assert_eq!(board.rows().last(), Some(&ListRow::Omitted(3)));
        let due = board.due();
        assert_eq!(due.len(), 1);
        assert_eq!(
            due[0].command("sa1"),
            "/subagents-inspect-rpc sa1 run-a step:0 --lines 80"
        );
    }

    #[test]
    fn a_cancelled_inflight_inspect_can_be_requested_again() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        assert!(board.due().is_empty());
        board.drop_inflight();
        assert_eq!(board.due().len(), 1);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "late" }],
        }));
        assert!(board.messages("a").is_empty());
    }

    #[test]
    fn unmatched_inspect_and_errors_do_not_wipe_a_body() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "kept" }],
        }));
        board.apply_inspect_value(&json!({
            "requestId": "nope",
            "messages": [{ "role": "assistant", "kind": "text", "text": "ignored" }],
        }));
        assert_eq!(board.messages("a")[0].text, "kept");
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "error": { "code": "stale", "message": "gone" },
        }));
        assert_eq!(board.messages("a")[0].text, "kept");
        assert!(board.stale("a"));
        assert_eq!(board.error_code("a"), Some("stale"));
        assert!(
            board.due().is_empty(),
            "a failed reply is still an answer for this revision"
        );
    }

    #[test]
    fn a_later_update_does_not_switch_the_open_leaf() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([
                run("a", "one", "running", 1, 10, json!([])),
                run("b", "two", "running", 2, 11, json!([])),
            ]),
            0,
        ));
        assert!(board.click("a"));
        board.apply_snapshot_value(&snapshot(
            json!([
                run("b", "two", "running", 2, 50, json!([])),
                run("a", "one", "running", 1, 12, json!([])),
            ]),
            0,
        ));
        assert!(board.is_open());
        assert_eq!(board.selected_id(), Some("a"));
        let ids: Vec<_> = board
            .rows()
            .into_iter()
            .filter_map(|row| match row {
                ListRow::Agent(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn inspect_is_due_again_only_when_the_leaf_moves() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "first" }],
        }));
        assert!(board.due().is_empty());
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 11, json!([]))]),
            0,
        ));
        assert_eq!(board.due().len(), 1);
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "second" }],
            "finalOutput": "done\nrest",
        }));
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "complete", 1, 12, json!([]))]),
            0,
        ));
        assert_eq!(
            board.due().len(),
            1,
            "the finishing revision is fetched once"
        );
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{ "role": "assistant", "kind": "text", "text": "second" }],
            "finalOutput": "done\nrest",
        }));
        assert!(board.due().is_empty());
        assert!(!board.list_shown());
        assert_eq!(board.preview("a"), "done");
    }

    #[test]
    fn yielding_the_dock_does_not_snap_the_panel_back() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        board.consider_auto_open(true);
        board.yield_dock();
        board.consider_auto_open(true);
        assert!(!board.is_open());
        assert_eq!(board.selected_id(), Some("a"));
    }

    #[test]
    fn an_empty_inspect_retract_keeps_the_body() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_lines(&[format!(
            r#"PI_SUBAGENT_INSPECT_JSON:{{"requestId":"{request}","messages":[{{"role":"assistant","kind":"text","text":"kept"}}]}}"#
        )]);
        board.apply_inspect_lines(&[]);
        assert_eq!(board.messages("a")[0].text, "kept");
    }

    #[test]
    fn preview_skips_a_leading_fence_and_heading_marks() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{
                "role": "assistant",
                "kind": "text",
                "text": "```json\n{ \"ok\": true }\n```"
            }],
        }));
        assert_eq!(board.preview("a"), "{ \"ok\": true }");
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "finalOutput": "# Report\n\nship it",
        }));
        assert_eq!(board.preview("a"), "Report");
    }

    #[test]
    fn an_expanded_code_block_follows_its_text_into_the_next_window() {
        let mut board = SubagentBoard::default();
        let hash = 7u64;
        let old_layout = [(0usize, 1usize, hash)];
        let mut open = HashSet::new();
        open.insert((0, FOLD_SALT, 1));
        board.reconcile_code_folds("a", &old_layout, &open, &HashSet::new());
        assert!(board.code_expanded("a", hash));

        let moved = [(2usize, 0usize, hash)];
        let mut seeded = HashSet::new();
        seeded.insert((2, FOLD_SALT, 0));
        board.reconcile_code_folds("a", &moved, &HashSet::new(), &seeded);
        assert!(
            !board.code_expanded("a", hash),
            "a painted block the reader folded closed stays closed"
        );
        board.reconcile_code_folds("a", &old_layout, &open, &HashSet::new());
        let elsewhere = [(4usize, 0usize, 99u64)];
        board.reconcile_code_folds("a", &elsewhere, &HashSet::new(), &HashSet::new());
        assert!(
            board.code_expanded("a", hash),
            "a block that left the window keeps the expansion"
        );
    }

    #[test]
    fn a_tool_result_preview_keeps_a_leading_hash() {
        let mut board = SubagentBoard::default();
        board.apply_snapshot_value(&snapshot(
            json!([run("a", "reviewer", "running", 1, 10, json!([]))]),
            0,
        ));
        let request = board.alloc_request_id();
        board.note_inflight("a", &request, 80);
        board.apply_inspect_value(&json!({
            "requestId": request,
            "messages": [{
                "kind": "toolResult",
                "name": "read",
                "text": "# not a heading\nrest"
            }],
        }));
        assert_eq!(board.preview("a"), "# not a heading");
    }
}
