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

/// Filter the catalog by `query` (case-insensitive substring; prefix
/// matches on the title rank first). Returns at most `limit` entries.
/// Fuzzy subsequence score: `Some(score)` when every char of `needle`
/// appears in `haystack` in order (case handled by the caller). Rewards
/// consecutive runs and word-boundary hits, penalizes gaps — so `appx`
/// matches `src/App.tsx` and `main` matches `src/main.rs` better than
/// `crates/app/main.rs`.
fn fuzzy_match(needle: &str, haystack: &str) -> Option<i32> {
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
            score += 3; // consecutive run
        }
        if found == 0 || haystack[found - 1] == '/' {
            score += 2; // start of the path or a segment
        }
        let gap = found - prev_hit.map(|p| p + 1).unwrap_or(0);
        score -= gap.min(12) as i32;
        prev_hit = Some(found);
        search_start = found + 1;
    }
    Some(score)
}

/// Filter the catalog by `query` with fuzzy (subsequence) matching —
/// basename matches outrank path matches, higher scores rank first, ties
/// keep catalog order. Returns at most `limit` entries.
pub fn filter_entries(
    query: &str,
    files: &[String],
    commands: &[SlashCommand],
    limit: usize,
) -> Vec<AcEntry> {
    let q = query.to_lowercase();
    let mut scored: Vec<(i32, usize, AcEntry)> = Vec::new();
    for (ix, command) in commands.iter().enumerate() {
        let name = command.name.to_lowercase();
        if let Some(score) = fuzzy_match(&q, &name) {
            scored.push((
                score + 10,
                ix,
                AcEntry::Command {
                    name: command.name.clone(),
                    description: command.description.clone(),
                    scope: command.scope.clone(),
                },
            ));
        }
    }
    for (ix, path) in files.iter().enumerate() {
        let basename = path.rsplit('/').next().unwrap_or(path).to_lowercase();
        // A basename hit (weighted 2×) outranks the same hit deep in the
        // path, so `app` surfaces `src/App.tsx` over `crates/app/…`.
        let score = fuzzy_match(&q, &basename)
            .map(|s| s * 2)
            .into_iter()
            .chain(fuzzy_match(&q, &path.to_lowercase()))
            .max();
        if let Some(score) = score {
            scored.push((
                score,
                ix + commands.len(),
                AcEntry::File { path: path.clone() },
            ));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    scored.into_iter().map(|(_, _, entry)| entry).collect()
}

/// Walk the workspace for @-mentionable files (relative, `/`-separated).
/// Skips VCS/dependency/build dirs and dot directories, caps depth and
/// count so huge trees stay responsive.
pub fn list_workspace_files(root: &Path) -> Vec<String> {
    const MAX_FILES: usize = 4_000;
    const MAX_DEPTH: usize = 12;
    const SKIP_DIRS: &[&str] = &[
        ".git",
        ".pi",
        ".hg",
        ".svn",
        "node_modules",
        "target",
        "dist",
        "build",
        "vendor",
        "__pycache__",
        ".next",
        ".cache",
    ];
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= MAX_FILES || depth > MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push((path, depth + 1));
            } else if !name.starts_with('.') && out.len() < MAX_FILES {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// What `fd` did with an `@` query.
pub(crate) enum FdSearch {
    /// `fd` ran. An empty list is a real "nothing matched", not a failure.
    Hits(Vec<String>),
    /// No `fd` binary to run. The caller keeps the bounded directory walk.
    Missing,
}

/// The regex `fd` should apply. A slash in the query searches the whole
/// path; otherwise `fd` matches the file name. Same shape as pi's
/// `buildFdPathQuery`.
pub(crate) fn fd_pattern(query: &str) -> String {
    let normalized = query.replace('\\', "/");
    if !normalized.contains('/') {
        return escape_regex(&normalized);
    }
    let trailing = normalized.ends_with('/');
    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        return normalized;
    }
    let mut pattern = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(escape_regex)
        .collect::<Vec<_>>()
        .join("[\\\\/]");
    if trailing {
        pattern.push_str("[\\\\/]");
    }
    pattern
}

fn escape_regex(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(
            ch,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// pi's filename score: exact, then prefix, then substring, then path.
fn score_path(path: &str, query: &str, is_dir: bool) -> i32 {
    if query.is_empty() {
        return 1;
    }
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let name = file_name.to_lowercase();
    let query = query.to_lowercase();
    let mut score = if name == query {
        100
    } else if name.starts_with(&query) {
        80
    } else if name.contains(&query) {
        50
    } else if path.to_lowercase().contains(&query) {
        30
    } else {
        0
    };
    if is_dir && score > 0 {
        score += 10;
    }
    score
}

fn path_depth(path: &str) -> usize {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .count()
}

/// Search `root` with `fd`, the same walker pi's TUI uses for `@`.
///
/// `fd` applies the pattern while it walks and skips ignored directories
/// (Unity `Temp` / `Library`, `node_modules`, …), so a name match is not
/// crowded out by the first few thousand generated files.
pub(crate) fn search_fd(root: &Path, query: &str) -> FdSearch {
    let Some(fd) = crate::onboarding::locate("fd") else {
        return FdSearch::Missing;
    };
    let mut command = std::process::Command::new(&fd);
    command
        .current_dir(root)
        .arg("--base-directory")
        .arg(root)
        .arg("--max-results")
        .arg("100")
        .arg("--type")
        .arg("f")
        .arg("--type")
        .arg("d")
        .arg("--follow")
        .arg("--hidden")
        .arg("--exclude")
        .arg(".git")
        .arg("--exclude")
        .arg(".git/*")
        .arg("--exclude")
        .arg(".git/**")
        .arg("--ignore-case")
        .arg("--color")
        .arg("never");
    let normalized = query.replace('\\', "/");
    if normalized.contains('/') {
        command.arg("--full-path");
    }
    if !query.is_empty() {
        command.arg(fd_pattern(query));
    }
    orbit_rpc::hide_console(&mut command);
    let Ok(output) = command.output() else {
        return FdSearch::Hits(Vec::new());
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut ranked: Vec<(i32, usize, usize, String)> = Vec::new();
    for line in text.lines() {
        let display = line.trim().replace('\\', "/");
        if display.is_empty()
            || display == ".git"
            || display.starts_with(".git/")
            || display.contains("/.git/")
        {
            continue;
        }
        let is_dir = display.ends_with('/');
        let path = display.trim_end_matches('/').to_string();
        let score = score_path(&path, query, is_dir);
        if !query.is_empty() && score == 0 {
            continue;
        }
        ranked.push((score, path_depth(&path), path.len(), path));
    }
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    let hits: Vec<String> = ranked.into_iter().map(|(_, _, _, path)| path).collect();
    FdSearch::Hits(hits)
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

    #[test]
    fn filter_ranks_prefix_matches_first() {
        let files = vec![
            "crates/app/main.rs".to_string(),
            "src/main.rs".to_string(),
            "README.md".to_string(),
            "main.py".to_string(),
        ];
        let entries = filter_entries("main", &files, &[], 10);
        // Basename prefix matches first (stable by catalog order), then
        // path-contains matches.
        let title = |e: &AcEntry| match e {
            AcEntry::Command { name, .. } => name.clone(),
            AcEntry::File { path } => path.clone(),
        };
        assert_eq!(
            entries.iter().map(title).collect::<Vec<_>>(),
            // All three basenames start with "main" — rank ties break by
            // catalog order. "README.md" (contains-match) is excluded.
            vec![
                "crates/app/main.rs".to_string(),
                "src/main.rs".to_string(),
                "main.py".to_string()
            ]
        );
    }

    #[test]
    fn filter_is_fuzzy_and_basename_first() {
        // Subsequence match: no exact substring of "appx" appears in the
        // path, yet fuzzy finds a-p-p-x in order.
        let files = vec!["src/App.tsx".to_string()];
        assert_eq!(filter_entries("appx", &files, &[], 10).len(), 1);
        // Basename hits outrank path hits: `app` ranks src/App.tsx (basename)
        // above crates/app/main.rs (path-only hit).
        let files = vec!["crates/app/main.rs".to_string(), "src/App.tsx".to_string()];
        assert_eq!(
            filter_entries("app", &files, &[], 10)
                .first()
                .map(|e| match e {
                    AcEntry::File { path } => path.as_str(),
                    _ => "",
                }),
            Some("src/App.tsx")
        );
        // No subsequence → no match ("zzq" can't be spelled from these).
        assert!(filter_entries("zzq", &files, &[], 10).is_empty());
    }

    #[test]
    fn filter_is_case_insensitive_and_capped() {
        let files = vec!["Big/FILE.Rs".to_string()];
        assert_eq!(filter_entries("file", &files, &[], 10).len(), 1);
        assert!(filter_entries("zzz", &files, &[], 10).is_empty());
        let many: Vec<String> = (0..50).map(|i| format!("f{i}.rs")).collect();
        assert_eq!(filter_entries("", &many, &[], 8).len(), 8);
    }

    #[test]
    fn fd_pattern_matches_pi_file_and_path_queries() {
        assert_eq!(fd_pattern("MetaData"), "MetaData");
        assert_eq!(fd_pattern("a.b"), "a\\.b");
        // A slash searches the path, with either separator.
        assert_eq!(fd_pattern("src/lib"), "src[\\\\/]lib");
        assert_eq!(fd_pattern("src/lib/"), "src[\\\\/]lib[\\\\/]");
        // Filename substring outranks a path-only accident, and a name that
        // does not contain the query scores nothing.
        assert!(score_path("Assets/FooMetaData.cs", "MetaData", false) >= 50);
        assert_eq!(
            score_path("Temp/Agent/_adhoc/RunDataRegenAll.cs", "MetaData", false),
            0
        );
        assert!(score_path("metadata.bytes", "MetaData", false) >= 50);
    }

    #[test]
    fn workspace_walk_skips_ignorable_dirs() {
        let root = std::env::temp_dir().join(format!("orbit-mentions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join(".git/refs")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        std::fs::write(root.join(".env"), "").unwrap();
        std::fs::write(root.join("node_modules/pkg/i.js"), "").unwrap();
        std::fs::write(root.join(".git/refs/x"), "").unwrap();
        let files = list_workspace_files(&root);
        assert_eq!(files, vec!["src/lib.rs".to_string()]);
        std::fs::remove_dir_all(&root).ok();
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
