//! Composer autocomplete — `/` command and `@` file mentions.
//!
//! Typing `/` at the start of a word (the beginning of the composer, or right
//! after whitespace, with the caret still inside that word) opens a
//! slash-command menu fed by pi's `get_commands`; typing `@` after whitespace
//! (or at the start) opens a workspace file menu. Both are filtered live by
//! the text between the sigil and the caret.
//!
//! The trigger lives entirely in the composer's text: detection is a pure
//! function of (content, cursor), so the popup opens/closes by itself as the
//! text changes — no extra state to keep in sync. Keyboard interaction is
//! negotiated through a shared `AutocompleteState`: the composer redirects
//! ↑/↓ to highlight movement, while Enter/Escape are intercepted by the app
//! (they already dispatch there as `Submit`/`AbortRun`).

use std::ops::Range;
use std::path::Path;
use std::rc::Rc;

use crate::explorer::walk;

/// Which surface the active trigger opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    /// `/command` — caret is inside a command token at a word boundary.
    Slash,
    /// `@file` — the token ends at the caret, starts after whitespace.
    At,
}

/// An active autocomplete trigger: `content[start..end]` is the token being
/// completed (including the sigil); `query` is the text after the sigil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub start: usize,
    pub end: usize,
    pub query: String,
}

/// Detect an autocomplete trigger at `cursor` in `content`.
///
/// - Slash: the word under the caret starts with `/`, and that `/` sits at
///   the start of the message or right after whitespace. The caret must still
///   be inside the word. The token runs to the next whitespace (so a caret in
///   the middle of `/review` replaces the whole word); `query` is only the
///   text between the sigil and the caret.
/// - At: an `@` preceded by line start or whitespace, with no whitespace
///   between it and the caret — `@query` is the token. A `/` inside
///   `@src/main.rs` stays part of the file token.
///
/// The menu is looser than paint. Any `/` at a word boundary opens it; a
/// path like `/usr/local` still does, and the menu stays shut when the query
/// matches no command. [`tokenize_mentions`] only colors names that look like
/// commands.
pub fn detect_trigger(content: &str, cursor: usize) -> Option<Trigger> {
    let cursor = cursor.min(content.len());
    if let Some(trigger) = detect_slash(content, cursor) {
        return Some(trigger);
    }
    // At: last `@` before the caret; must start a token and run unbroken
    // (no whitespace) to the caret.
    let head = &content[..cursor];
    let at = head.rfind('@')?;
    if !(at == 0 || content[..at].ends_with(char::is_whitespace)) {
        return None;
    }
    let query = &content[at + 1..cursor];
    if query.chars().any(char::is_whitespace) {
        return None;
    }
    Some(Trigger {
        kind: TriggerKind::At,
        start: at,
        end: cursor,
        query: query.to_string(),
    })
}

/// Slash trigger for the word containing `cursor`, if that word is a `/…`
/// token at a boundary (message start or whitespace).
fn detect_slash(content: &str, cursor: usize) -> Option<Trigger> {
    let start = token_start_at(content, cursor)?;
    if !content[start..cursor].starts_with('/') {
        return None;
    }
    let query = &content[start + 1..cursor];
    let rest = &content[start + 1..];
    let end = start + 1 + rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(Trigger {
        kind: TriggerKind::Slash,
        start,
        end,
        query: query.to_string(),
    })
}

/// Byte index where the token containing `cursor` begins.
///
/// `None` when the caret sits on the boundary itself, so there is no token
/// under it yet (including a caret parked before the first character).
fn token_start_at(content: &str, cursor: usize) -> Option<usize> {
    let start = content[..cursor]
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    (start < cursor).then_some(start)
}

/// Which composer token a span paints as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionKind {
    /// `/command` — a command token at a word boundary.
    Command,
    /// `@file` — a workspace reference.
    File,
}

/// A paint-colored span of the composer text. Byte offsets into the raw
/// content, so the caller can slice runs without disturbing the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionSpan {
    pub range: Range<usize>,
    pub kind: MentionKind,
}

/// Locate the tokens the composer paints in their own color: every `/command`
/// at a word boundary and every `@file` mention. Pure, sorted and
/// non-overlapping, so the editor's caret/selection model is untouched — this
/// is paint only.
///
/// The rules mirror [`detect_trigger`]: a slash token opens at the start of
/// the message or right after whitespace, and must look like a command name
/// (a path like `/usr/local` stays plain); an `@` must sit at a token boundary
/// and runs unbroken to the next whitespace.
pub fn tokenize_mentions(content: &str) -> Vec<MentionSpan> {
    let mut spans = Vec::new();

    // `/command`: the whole word, only when every character after the sigil
    // reads as a command name. A `/` glued to the previous word (`http://`,
    // `please/review`) is not a command.
    for (slash, _) in content.match_indices('/') {
        if slash != 0 && !content[..slash].ends_with(char::is_whitespace) {
            continue;
        }
        let end = content[slash..]
            .find(char::is_whitespace)
            .map(|off| slash + off)
            .unwrap_or(content.len());
        let name = &content[slash + 1..end];
        if is_command_name(name) {
            spans.push(MentionSpan {
                range: slash..end,
                kind: MentionKind::Command,
            });
        }
    }

    // `@file` mentions: each `@` at a token boundary, running to whitespace.
    for (at, _) in content.match_indices('@') {
        if at != 0 && !content[..at].ends_with(char::is_whitespace) {
            continue;
        }
        let end = content[at..]
            .find(char::is_whitespace)
            .map(|off| at + off)
            .unwrap_or(content.len());
        if end > at + 1 {
            spans.push(MentionSpan {
                range: at..end,
                kind: MentionKind::File,
            });
        }
    }

    spans.sort_by_key(|span| span.range.start);
    spans
}

/// A `/command` name: non-empty, and only the characters pi command names use.
/// A slash or other punctuation (`/usr/local`) keeps the word plain text.
fn is_command_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
}

/// Where a `/` command comes from — the scope badge shown on its row.
///
/// pi reports provenance in `source` (`extension`/`prompt`/`skill`) plus a
/// `sourceInfo` of `scope`/`origin`/`path`; Orbit folds that into the five
/// labels the autocomplete menu shows on the right of each command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandScope {
    /// pi's built-in commands and installed packages (`npm:`/`git:`) — part
    /// of the setup, available in every session.
    Builtin,
    /// A skill, invoked as `/skill:<name>`.
    Skill,
    /// One of Orbit's bundled extensions (materialized under `~/.orbit-pi/`).
    Orbit,
    /// A user-authored extension or prompt template (a top-level file in
    /// `~/.pi/agent/` or `.agents/`, not an installed package).
    Custom,
    /// Project-local (workspace `.pi/` or `.agents/`); the badge is the
    /// project folder's name.
    Project(String),
}

impl CommandScope {
    /// The badge text shown on the row (the project name is dynamic).
    pub fn label(&self) -> String {
        match self {
            Self::Builtin => tr!("autocomplete.scope_builtin"),
            Self::Skill => tr!("autocomplete.scope_skills"),
            Self::Orbit => tr!("autocomplete.scope_orbit"),
            Self::Custom => tr!("autocomplete.scope_custom"),
            Self::Project(name) => name.clone(),
        }
    }
}

/// Classify a `get_commands` entry into a scope badge. Pure so the mapping is
/// unit-testable; `workspace` names the project badge, `home` locates Orbit's
/// bundled extensions.
pub fn classify_scope(
    source: &str,
    scope: Option<&str>,
    origin: Option<&str>,
    path: Option<&str>,
    workspace: Option<&Path>,
    home: Option<&Path>,
) -> CommandScope {
    // Skills win over their scope: a project skill still reads as a skill.
    if source == "skill" {
        return CommandScope::Skill;
    }
    // Orbit's bundled extensions load with `--extension`, which pi reports as
    // temporary; the install path is the only reliable tell.
    if let (Some(home), Some(path)) = (home, path) {
        if Path::new(path).starts_with(home.join(".orbit-pi")) {
            return CommandScope::Orbit;
        }
    }
    if scope == Some("project") {
        let name = workspace
            .and_then(|dir| dir.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| tr!("autocomplete.scope_project"));
        return CommandScope::Project(name);
    }
    // Installed packages (`npm:`/`git:`) and pi's own built-ins are part of
    // the setup — "builtin". A synthetic path (`<inline:…>`, `<sdk:…>`) is
    // built in too. Only a user-authored top-level file is "custom".
    if origin == Some("package")
        || source == "builtin"
        || path.is_some_and(|path| path.starts_with('<'))
    {
        return CommandScope::Builtin;
    }
    if scope == Some("user") {
        return CommandScope::Custom;
    }
    CommandScope::Builtin
}

/// One slash command from pi's `get_commands` (extensions + skills).
#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// Where this command came from — rendered as the row's scope badge.
    pub scope: CommandScope,
}

/// One selectable row of the autocomplete menu.
#[derive(Debug, Clone)]
pub enum AcEntry {
    /// A pi command — commit inserts `/name ` into the composer.
    Command {
        name: String,
        description: String,
        scope: CommandScope,
    },
    /// A workspace file — commit inserts `@path ` (pi expands @-mentions).
    File { path: String },
}

/// Fuzzy subsequence score: `Some(score)` when every char of `needle`
/// appears in `haystack` in order (case handled by the caller). Rewards
/// consecutive runs and word-boundary hits, penalizes gaps — so `appx`
/// matches `src/App.tsx` and `main` matches `src/main.rs` better than
/// `crates/app/main.rs`.
///
/// Allocation-free: `@` scores every workspace candidate on each keystroke, so
/// the old `Vec<char>` copy of the haystack is not affordable. Char indices
/// (not byte offsets) keep the scoring identical for non-ASCII paths.
fn fuzzy_match(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut score = 0i32;
    // Byte offset to resume scanning from, and its char index.
    let mut search_from = 0usize;
    let mut char_at_from = 0usize;
    let mut prev: Option<usize> = None;
    for c in needle.chars() {
        let mut hit = None;
        for (i, (offset, h)) in haystack[search_from..].char_indices().enumerate() {
            if h == c {
                hit = Some((search_from + offset, char_at_from + i));
                break;
            }
        }
        let (byte, index) = hit?;
        if prev == Some(index.wrapping_sub(1)) {
            score += 3; // consecutive run
        }
        if index == 0 || haystack[..byte].ends_with('/') {
            score += 2; // start of the path or a segment
        }
        let gap = index - prev.map(|p| p + 1).unwrap_or(0);
        score -= gap.min(12) as i32;
        prev = Some(index);
        search_from = byte + c.len_utf8();
        char_at_from = index + 1;
    }
    Some(score)
}

/// Filter the slash-command catalog by `query` (fuzzy subsequence on the
/// name). Returns at most `limit` entries. Files are ranked by
/// [`MentionIndex::query`] instead — they come from the workspace index, not
/// from this catalog.
pub fn filter_entries(query: &str, commands: &[SlashCommand], limit: usize) -> Vec<AcEntry> {
    let q = query.to_lowercase();
    let mut scored: Vec<(i32, usize)> = Vec::new();
    for (ix, command) in commands.iter().enumerate() {
        if let Some(score) = fuzzy_match(&q, &command.name.to_lowercase()) {
            scored.push((score + 10, ix));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    scored
        .into_iter()
        .map(|(_, ix)| {
            let command = &commands[ix];
            AcEntry::Command {
                name: command.name.clone(),
                description: command.description.clone(),
                scope: command.scope.clone(),
            }
        })
        .collect()
}

/// Every `@`-mentionable workspace entry, pre-scored for cheap per-keystroke
/// queries: lowercased paths, basename offsets and the hidden bit are computed
/// once at build time, because a keystroke pays the scan 30× a second while the
/// workspace changes a few times a minute.
///
/// The candidate list comes from [`crate::explorer::walk::snapshot`] — the same
/// walk the Explorer renders, so `@` and the tree can never disagree about what
/// exists. Nothing here spawns a process: shelling out to `fd` per keystroke was
/// the 300–700 ms stall this replaced.
pub struct MentionIndex {
    /// Display paths, shallowest first (depth, then path).
    paths: Vec<String>,
    /// `paths[i]` lowercased.
    lower: Vec<String>,
    /// Byte offset of the basename inside `lower[i]`.
    name_at: Vec<u32>,
    hidden: Vec<bool>,
    /// Indices into `paths`, sorted by path — membership tests for recents.
    by_path: Vec<u32>,
}

impl MentionIndex {
    pub fn build(entries: &[walk::Entry]) -> Self {
        let mut order: Vec<&walk::Entry> = entries.iter().collect();
        order.sort_by(|a, b| {
            path_depth(&a.path)
                .cmp(&path_depth(&b.path))
                .then_with(|| a.path.cmp(&b.path))
        });
        let mut paths = Vec::with_capacity(order.len());
        let mut lower = Vec::with_capacity(order.len());
        let mut name_at = Vec::with_capacity(order.len());
        let mut hidden = Vec::with_capacity(order.len());
        for entry in &order {
            let lowered = entry.path.to_lowercase();
            let offset = lowered
                .rfind('/')
                .map(|i| i + 1)
                .unwrap_or(0)
                .min(u32::MAX as usize) as u32;
            name_at.push(offset);
            lower.push(lowered);
            hidden.push(entry.is_hidden);
            paths.push(entry.path.clone());
        }
        let mut by_path: Vec<u32> = (0..paths.len() as u32).collect();
        by_path.sort_by(|a, b| paths[*a as usize].cmp(&paths[*b as usize]));
        Self {
            paths,
            lower,
            name_at,
            hidden,
            by_path,
        }
    }

    /// Test/seed constructor: paths only, as if every one were a file.
    #[cfg(test)]
    pub fn from_paths(paths: Vec<String>) -> Self {
        let entries: Vec<walk::Entry> = paths
            .into_iter()
            .map(|path| walk::Entry {
                is_hidden: walk::is_hidden_path(&path),
                is_dir: false,
                path,
            })
            .collect();
        Self::build(&entries)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether the workspace still has `path` (recents drop deleted files).
    pub fn contains(&self, path: &str) -> bool {
        self.by_path
            .binary_search_by(|ix| self.paths[*ix as usize].as_str().cmp(path))
            .is_ok()
    }

    /// The top `limit` paths for `query`, best first.
    ///
    /// An empty query is "shallowest first" (the recents head is the caller's
    /// job); otherwise a basename hit (weighted 2×) outranks a path hit, and
    /// ties fall to non-hidden, then shallowness, then path.
    pub fn query(&self, query: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        if query.is_empty() {
            return self.paths.iter().take(limit).cloned().collect();
        }
        let needle = query.to_lowercase();
        let mut scored: Vec<(i32, bool, usize)> = Vec::new();
        for (ix, lowered) in self.lower.iter().enumerate() {
            let name = &lowered[self.name_at[ix] as usize..];
            let score = fuzzy_match(&needle, name)
                .map(|score| score * 2)
                .or_else(|| fuzzy_match(&needle, lowered));
            if let Some(score) = score {
                scored.push((score, self.hidden[ix], ix));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        scored.truncate(limit);
        scored
            .into_iter()
            .map(|(_, _, ix)| self.paths[ix].clone())
            .collect()
    }
}

/// Segments in a `/`-separated path (`a/b/c` → 3).
fn path_depth(path: &str) -> usize {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

/// Shared keyboard-negotiation state between the composer (which owns
/// ↑/↓) and the app (which owns Enter/Escape and renders the menu).
#[derive(Debug, Default)]
pub struct AutocompleteState {
    pub open: bool,
    pub highlighted: usize,
    pub count: usize,
}

impl AutocompleteState {
    /// Cycle the highlight, wrapping at both ends.
    pub fn move_highlight(&mut self, delta: i32) {
        if self.count == 0 {
            self.highlighted = 0;
            return;
        }
        let count = self.count as i32;
        self.highlighted = (self.highlighted as i32 + delta).rem_euclid(count) as usize;
    }
}

pub type SharedAutocomplete = Rc<std::cell::RefCell<AutocompleteState>>;

/// File-type badge for an @-mention row: a short extension label plus
/// language-brand colors tuned for dark and light surfaces (like VS Code's
/// colored file icons — `tsx`/`ts` blue, `jsx`/`js` yellow, `html` orange…).
/// Returns `(label, dark_hex, light_hex)`; unknown extensions fall back to
/// their (uppercased) extension or a generic gray file.
pub fn file_type_badge(path: &str) -> (&'static str, u32, u32) {
    // Lowercased extension (no dot). Filenames without extensions —
    // Dockerfile, Makefile, .env — get named badges below.
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    if ext == path.to_lowercase() {
        // No dot at all — treat as an extensionless file (Makefile-style).
        return ("FILE", 0x8A8A94, 0x6E6E78);
    }
    match ext.as_str() {
        // Docs
        "md" | "markdown" | "mdx" => ("MD", 0x519ABA, 0x3B72A8),
        "txt" => ("TXT", 0x8A8A94, 0x6E6E78),
        "pdf" => ("PDF", 0xD93831, 0xB02A24),
        // TypeScript family
        "ts" => ("TS", 0x3178C6, 0x2A66A8),
        "tsx" => ("TSX", 0x3178C6, 0x2A66A8),
        "d.ts" => ("TS", 0x3178C6, 0x2A66A8),
        // JavaScript family
        "js" | "mjs" | "cjs" => ("JS", 0xF1E05A, 0xA9941A),
        "jsx" => ("JSX", 0xF1E05A, 0xB29310),
        "json" | "jsonc" | "json5" => ("JSON", 0xCBCB41, 0x8F8F1E),
        // Web
        "html" | "htm" => ("HTML", 0xE44D26, 0xC43D1B),
        "css" => ("CSS", 0x563D7C, 0x563D7C),
        "scss" | "sass" => ("SCSS", 0xC6538C, 0xA8447A),
        "less" => ("LESS", 0x2A5D8F, 0x2A5D8F),
        "vue" => ("VUE", 0x41B883, 0x35986C),
        "svelte" => ("SVELTE", 0xFF3E00, 0xD63400),
        "astro" => ("ASTRO", 0xFF5D01, 0xD64E00),
        // Systems / backend
        "rs" => ("RS", 0xDEA584, 0xB5744F),
        "py" => ("PY", 0x3572A5, 0x2A5B85),
        "go" => ("GO", 0x00ADD8, 0x008DB2),
        "rb" => ("RB", 0xCC342D, 0xA82A24),
        "php" => ("PHP", 0x4F5D95, 0x414C7C),
        "java" => ("JAVA", 0xB07219, 0x8E5B13),
        "kt" | "kts" => ("KT", 0xA97BFF, 0x7C4FD9),
        "swift" => ("SWIFT", 0xF05138, 0xC93F28),
        "c" | "h" => ("C", 0x8D8D9B, 0x62626E),
        "cpp" | "hpp" | "cc" | "cxx" => ("C++", 0xF34B7D, 0xD22A5E),
        "cs" => ("CS", 0x178600, 0x106B00),
        // Shell / config
        "sh" | "bash" | "zsh" | "fish" => ("SH", 0x89E051, 0x4E9424),
        "toml" => ("TOML", 0x9C4121, 0x7C331A),
        "yaml" | "yml" => ("YML", 0xCB171E, 0xA11218),
        "lock" => ("LOCK", 0x7D7D7D, 0x6E6E78),
        "env" => ("ENV", 0xECD53F, 0x8F8414),
        // Data / query
        "sql" => ("SQL", 0xDD7C37, 0xB85F20),
        "csv" | "tsv" => ("CSV", 0x237346, 0x1B5C38),
        // Images
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => ("IMG", 0xA074C4, 0x7C4FA0),
        "svg" => ("SVG", 0xFFB13B, 0xD68F1F),
        // Known-but-unlisted extensions get a neutral badge; anything else
        // falls back to a generic file.
        "hbs" => ("HBS", 0x8A8A94, 0x6E6E78),
        "gitignore" | "gitattributes" => ("GIT", 0xF14E32, 0xD13E22),
        _ => ("FILE", 0x8A8A94, 0x6E6E78),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_scope_folds_pi_provenance() {
        let home = Path::new("/home/u");
        let workspace = Path::new("/work/orbit");
        let classify = |source, scope, origin, path| {
            classify_scope(source, scope, origin, path, Some(workspace), Some(home))
        };
        // Skills win over their scope, project or user.
        assert_eq!(
            classify("skill", Some("project"), None, None),
            CommandScope::Skill
        );
        assert_eq!(
            classify("skill", Some("user"), None, None),
            CommandScope::Skill
        );
        // Orbit's bundled extensions live under `~/.orbit-pi/` and arrive as
        // temporary CLI extensions.
        assert_eq!(
            classify(
                "extension",
                Some("temporary"),
                Some("top-level"),
                Some("/home/u/.orbit-pi/title-extension/index.js")
            ),
            CommandScope::Orbit
        );
        // Project commands badge with the workspace folder name.
        assert_eq!(
            classify(
                "extension",
                Some("project"),
                Some("top-level"),
                Some("/work/orbit/.pi/extensions/x.ts")
            ),
            CommandScope::Project("orbit".to_string())
        );
        // User-scoped, hand-written extensions/prompts read as custom; an
        // installed package of the same scope is built in.
        assert_eq!(
            classify(
                "extension",
                Some("user"),
                Some("top-level"),
                Some("/home/u/.pi/agent/extensions/x.ts")
            ),
            CommandScope::Custom
        );
        assert_eq!(
            classify(
                "extension",
                Some("user"),
                Some("package"),
                Some("/home/u/.pi/agent/npm/node_modules/x/index.ts")
            ),
            CommandScope::Builtin
        );
        // Anything synthetic or unplaceable falls back to builtin.
        assert_eq!(
            classify(
                "extension",
                Some("temporary"),
                None,
                Some("<inline:llama.cpp>")
            ),
            CommandScope::Builtin
        );
        assert_eq!(classify("", None, None, None), CommandScope::Builtin);
    }

    #[test]
    fn slash_triggers_at_a_word_boundary_before_whitespace() {
        let t = detect_trigger("/rev", 4).unwrap();
        assert_eq!(t.kind, TriggerKind::Slash);
        assert_eq!(t.query, "rev");
        assert_eq!(t.start, 0);
        assert_eq!(t.end, 4);
        // Query cut at the caret; the replacement still covers the whole word.
        let mid = detect_trigger("/review", 3).unwrap();
        assert_eq!(mid.query, "re");
        assert_eq!(mid.start, 0);
        assert_eq!(mid.end, 7);
        // Caret past the word closes the trigger.
        assert!(detect_trigger("/rev ", 5).is_none());
        // A slash after whitespace opens on that token, not only at column 0.
        let inline = detect_trigger("run /rev", 8).unwrap();
        assert_eq!(inline.kind, TriggerKind::Slash);
        assert_eq!(inline.query, "rev");
        assert_eq!(inline.start, 4);
        assert_eq!(inline.end, 8);
        // Leading space and a new line are boundaries too.
        assert_eq!(detect_trigger(" /rev", 5).unwrap().start, 1);
        assert_eq!(detect_trigger("done\n/re", 8).unwrap().query, "re");
        // No whitespace before the slash keeps it plain text.
        assert!(detect_trigger("please/review", 13).is_none());
        assert!(detect_trigger("http://example", 14).is_none());
        // Caret moved into a later word closes the menu.
        assert!(detect_trigger("请看 /skill 这里", "请看 /skill 这里".len()).is_none());
        // Empty query right after the sigil still triggers.
        assert_eq!(detect_trigger("/", 1).unwrap().query, "");
        assert_eq!(detect_trigger("see /", 5).unwrap().query, "");
        // Trigger survives command names with trailing content later on.
        let leading = detect_trigger("/review focus on app.rs", 2).unwrap();
        assert_eq!(leading.kind, TriggerKind::Slash);
        assert_eq!(leading.query, "r");
        assert_eq!(leading.end, 7);
        // A `/` inside an @file stays a file mention.
        let file = detect_trigger("look at @src/comp", 17).unwrap();
        assert_eq!(file.kind, TriggerKind::At);
        assert_eq!(file.query, "src/comp");
        // The caret in a later @ token does not reopen the earlier slash.
        let later = detect_trigger("see /cmd @fi", 12).unwrap();
        assert_eq!(later.kind, TriggerKind::At);
        assert_eq!(later.query, "fi");
    }

    #[test]
    fn slash_trigger_respects_caret_edges() {
        // Caret before a leading slash is not inside the token. The old
        // detector sliced `content[1..0]` here.
        assert!(detect_trigger("/rev", 0).is_none());
        // Caret sitting on the space before `/` has not entered the token.
        assert!(detect_trigger("run /rev", 4).is_none());
        // Tab is whitespace, same as a space or a newline.
        let tab = detect_trigger("a\t/re", 5).unwrap();
        assert_eq!(tab.kind, TriggerKind::Slash);
        assert_eq!(tab.start, 2);
        assert_eq!(tab.query, "re");
        // Ideographic space is whitespace too.
        let wide = detect_trigger("a\u{3000}/re", 7).unwrap();
        assert_eq!(wide.start, 4);
        assert_eq!(wide.query, "re");
        // Caret mid-word still replaces through the end of that word.
        let mid = detect_trigger("please /review notes", 10).unwrap();
        assert_eq!(mid.kind, TriggerKind::Slash);
        assert_eq!(mid.query, "re");
        assert_eq!(mid.start, 7);
        assert_eq!(mid.end, 14);
        // A later @file that itself contains `/` stays a file mention.
        let file = detect_trigger("/cmd @src/ma", 12).unwrap();
        assert_eq!(file.kind, TriggerKind::At);
        assert_eq!(file.start, 5);
        assert_eq!(file.query, "src/ma");
    }

    #[test]
    fn at_trigger_runs_from_token_start_to_caret() {
        let t = detect_trigger("look at @src/comp", 17).unwrap();
        assert_eq!(t.kind, TriggerKind::At);
        assert_eq!(t.start, 8);
        assert_eq!(t.query, "src/comp");
        // Whitespace between @ and caret closes it.
        assert!(detect_trigger("a b @c d", 8).is_none());
        // Mid-word @ does not trigger ("b@" preceded by "c").
        assert!(detect_trigger("abc@x", 5).is_none());
        // @ at the very start triggers.
        assert_eq!(detect_trigger("@", 1).unwrap().query, "");
        // A second @ supersedes the first.
        let t = detect_trigger("@one @tw", 8).unwrap();
        assert_eq!(t.start, 5);
        assert_eq!(t.query, "tw");
        // Newline also starts a token (multi-line composer).
        let t = detect_trigger("done\n@re", 8).unwrap();
        assert_eq!(t.query, "re");
    }

    #[test]
    fn tokenize_colors_leading_command_and_files() {
        let spans = tokenize_mentions("/review fix @src/main.rs and @lib.rs");
        assert_eq!(
            spans,
            vec![
                MentionSpan {
                    range: 0..7,
                    kind: MentionKind::Command,
                },
                MentionSpan {
                    range: 12..24,
                    kind: MentionKind::File,
                },
                MentionSpan {
                    range: 29..36,
                    kind: MentionKind::File,
                },
            ]
        );
    }

    #[test]
    fn tokenize_leaves_paths_and_emails_plain() {
        // A path is not a command, even after whitespace; a mid-word `@` is
        // not a mention. `http://` and `please/review` have no word boundary.
        assert!(tokenize_mentions("/usr/local/bin").is_empty());
        assert!(tokenize_mentions("see /usr/local/bin").is_empty());
        assert!(tokenize_mentions("mail me at a@b.com").is_empty());
        assert!(tokenize_mentions("see http://example.com").is_empty());
        assert!(tokenize_mentions("please/review").is_empty());
        // A lone sigil has no payload yet.
        assert!(tokenize_mentions("/").is_empty());
        assert!(tokenize_mentions("@").is_empty());
    }

    #[test]
    fn tokenize_colors_inline_commands_in_order() {
        let content = "see @src/main.rs then /skill:impeccable";
        let spans = tokenize_mentions(content);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].kind, MentionKind::File);
        assert_eq!(&content[spans[0].range.clone()], "@src/main.rs");
        assert_eq!(spans[1].kind, MentionKind::Command);
        assert_eq!(&content[spans[1].range.clone()], "/skill:impeccable");
        // A command on its own line is colored too.
        let multiline = tokenize_mentions("done\n/review");
        assert_eq!(multiline[0].range, 5..12);
        assert_eq!(multiline[0].kind, MentionKind::Command);
        let tabbed = tokenize_mentions("done\t/review");
        assert_eq!(tabbed[0].range, 5..12);
        assert_eq!(tabbed[0].kind, MentionKind::Command);
    }

    #[test]
    fn tokenize_handles_multiline_and_repeats() {
        let spans = tokenize_mentions("done\n@re @two/three");
        assert_eq!(
            spans.iter().map(|s| s.range.clone()).collect::<Vec<_>>(),
            vec![5..8, 9..19]
        );
        // A `@` inside an existing mention does not open a second span.
        assert_eq!(tokenize_mentions("@a@b").len(), 1);
    }

    fn index_of(paths: &[&str]) -> MentionIndex {
        MentionIndex::from_paths(paths.iter().map(|path| path.to_string()).collect())
    }

    fn command(name: &str) -> SlashCommand {
        SlashCommand {
            name: name.to_string(),
            description: String::new(),
            scope: CommandScope::Builtin,
        }
    }

    #[test]
    fn filter_ranks_command_prefixes_first() {
        let commands = vec![
            command("review"),
            command("review-branch"),
            command("impersonate"),
        ];
        let names: Vec<String> = filter_entries("review", &commands, 10)
            .into_iter()
            .map(|entry| match entry {
                AcEntry::Command { name, .. } => name,
                AcEntry::File { path } => path,
            })
            .collect();
        assert_eq!(names, vec!["review", "review-branch"]);
        assert!(filter_entries("zzq", &commands, 10).is_empty());
    }

    #[test]
    fn index_ranks_basename_hits_first() {
        let index = index_of(&["crates/app/main.rs", "src/main.rs", "README.md", "main.py"]);
        // Basename hits (weighted 2×) first; equal scores fall to the
        // shallowest path, so `main.py` beats `src/main.rs`.
        assert_eq!(
            index.query("main", 10),
            vec!["main.py", "src/main.rs", "crates/app/main.rs"]
        );
        // A path-only hit still matches, below every basename hit.
        assert_eq!(index.query("crates", 10), vec!["crates/app/main.rs"]);
        // No subsequence → no match.
        assert!(index.query("zzq", 10).is_empty());
    }

    #[test]
    fn index_is_fuzzy_and_case_insensitive() {
        // No literal "appx" anywhere, yet a-p-p-x appears in order.
        let index = index_of(&["src/App.tsx"]);
        assert_eq!(index.query("appx", 10), vec!["src/App.tsx"]);
        assert_eq!(index.query("APPX", 10), vec!["src/App.tsx"]);

        let index = index_of(&["Big/FILE.Rs"]);
        assert_eq!(index.query("file", 10), vec!["Big/FILE.Rs"]);
    }

    #[test]
    fn index_empty_query_is_shallow_then_capped() {
        let index = index_of(&["a/b/deep.rs", "root.rs", "a/mid.rs"]);
        assert_eq!(
            index.query("", 10),
            vec!["root.rs", "a/mid.rs", "a/b/deep.rs"]
        );
        let many: Vec<String> = (0..50).map(|i| format!("f{i}.rs")).collect();
        let index = MentionIndex::from_paths(many);
        assert_eq!(index.query("", 8).len(), 8);
        assert!(index.query("", 0).is_empty());
    }

    #[test]
    fn index_hidden_entries_lose_ties_only() {
        let index = index_of(&["src/.env", "src/env.txt"]);
        // Same score class: the visible file wins the tie.
        assert_eq!(index.query("env", 10), vec!["src/env.txt", "src/.env"]);
        // A hidden file still wins on a strictly better score.
        let index = index_of(&[".env", "src/config.toml"]);
        assert_eq!(index.query("env", 10), vec![".env"]);
    }

    #[test]
    fn index_contains_reports_workspace_files() {
        let index = index_of(&["src/main.rs", "README.md"]);
        assert!(index.contains("src/main.rs"));
        assert!(!index.contains("src/gone.rs"));
        assert_eq!(index.len(), 2);
        assert!(!MentionIndex::from_paths(Vec::new()).contains("x"));
    }

    #[test]
    fn fuzzy_match_rewards_runs_and_boundaries() {
        // A consecutive run and a segment start beat a scattered match.
        let run = fuzzy_match("main", "main.rs").unwrap();
        let scattered = fuzzy_match("main", "m/a/i/n.rs").unwrap();
        assert!(run > scattered, "{run} should beat {scattered}");
        assert!(fuzzy_match("main", "src/main.rs").unwrap() > 0);
        assert_eq!(fuzzy_match("", "anything"), Some(0));
        assert_eq!(fuzzy_match("zzq", "src/main.rs"), None);
    }

    /// The implementation this replaced — it allocated a `Vec<char>` per call.
    /// Kept as an oracle: the allocation-free rewrite must not change scoring.
    fn fuzzy_match_reference(needle: &str, haystack: &str) -> Option<i32> {
        if needle.is_empty() {
            return Some(0);
        }
        let haystack: Vec<char> = haystack.chars().collect();
        let mut score = 0i32;
        let mut search_start = 0usize;
        let mut prev_hit: Option<usize> = None;
        for c in needle.chars() {
            let found = haystack[search_start..]
                .iter()
                .position(|h| *h == c)
                .map(|p| p + search_start)?;
            if prev_hit == Some(found.wrapping_sub(1)) {
                score += 3;
            }
            if found == 0 || haystack[found - 1] == '/' {
                score += 2;
            }
            let gap = found - prev_hit.map(|p| p + 1).unwrap_or(0);
            score -= gap.min(12) as i32;
            prev_hit = Some(found);
            search_start = found + 1;
        }
        Some(score)
    }

    #[test]
    fn fuzzy_match_scores_identically_to_the_old_implementation() {
        let alphabet: Vec<char> = "abc/._资源角色控制器Main".chars().collect();
        let mut seed: u64 = 0x5eed;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..400 {
            let haystack_len = next() % 24 + 1;
            let haystack: String = (0..haystack_len)
                .map(|_| alphabet[next() % alphabet.len()])
                .collect();
            let needle_len = next() % 5;
            let needle: String = (0..needle_len)
                .map(|_| alphabet[next() % alphabet.len()])
                .collect();
            assert_eq!(
                fuzzy_match(&needle, &haystack),
                fuzzy_match_reference(&needle, &haystack),
                "needle {needle:?} haystack {haystack:?}"
            );
        }
    }

    #[test]
    fn fuzzy_match_counts_chars_not_bytes() {
        // CJK: one char each, so the run bonus must still fire.
        let cjk = fuzzy_match("角色", "角色控制器.rs").unwrap();
        assert!(cjk > 0);
        // An ASCII needle still finds a match after a multi-byte segment.
        assert!(fuzzy_match("main", "资源/主城/main.rs").is_some());
    }

    #[test]
    fn highlight_wraps_both_ways() {
        let mut s = AutocompleteState {
            open: true,
            highlighted: 0,
            count: 3,
        };
        s.move_highlight(-1);
        assert_eq!(s.highlighted, 2);
        s.move_highlight(1);
        assert_eq!(s.highlighted, 0);
        s.move_highlight(5);
        assert_eq!(s.highlighted, 2);
        s.count = 0;
        s.move_highlight(1);
        assert_eq!(s.highlighted, 0);
    }

    #[test]
    fn file_badges_map_by_extension() {
        // Requested types: md, jsx, html, ts, tsx — each gets its own badge.
        for (path, want) in [
            ("README.md", "MD"),
            ("src/App.jsx", "JSX"),
            ("index.html", "HTML"),
            ("src/lib.ts", "TS"),
            ("src/App.tsx", "TSX"),
            ("app.rs", "RS"),
            ("main.py", "PY"),
            ("styles.scss", "SCSS"),
            ("logo.png", "IMG"),
            ("Cargo.toml", "TOML"),
        ] {
            let (label, _, _) = file_type_badge(path);
            assert_eq!(label, want, "badge for {path}");
        }
        // Same-family extensions share a color (TS/TSX blue, JS/JSX yellow).
        let (_, ts_dark, ts_light) = file_type_badge("a.ts");
        let (_, tsx_dark, tsx_light) = file_type_badge("b.tsx");
        assert_eq!(ts_dark, tsx_dark);
        assert_eq!(ts_light, tsx_light);
        // Case-insensitive; extensionless files get the generic badge.
        assert_eq!(file_type_badge("NOTES.MD").0, "MD");
        assert_eq!(file_type_badge("Makefile").0, "FILE");
        // Dark and light palettes both carry a color.
        let (_, dark, light) = file_type_badge("x.html");
        assert_ne!(dark, 0);
        assert_ne!(light, 0);
    }
}
