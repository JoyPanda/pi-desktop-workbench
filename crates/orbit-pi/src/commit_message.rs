//! Conventional commit message generation.
//!
//! The primary path is a **one-shot, tool-free pi call**: `pi -p --no-tools
//! --no-session …` processes the prompt and exits, so the diff never touches
//! the user's active session and no tool can run in the workspace. Extensions
//! stay enabled so the user's custom providers resolve. If that fails (pi
//! missing, no credentials, timeout), [`heuristic`] derives a plain message
//! from the changed file statuses so the button always produces something
//! editable; the caller reports *why* it fell back instead of passing the
//! result off as generated.
//!
//! Two properties matter for the message to describe what is actually being
//! committed:
//!
//! - [`Scope`] picks the diff source, so the prompt never describes the index
//!   while the commit records the whole worktree — and an untracked file gets a
//!   synthesized `/dev/null` diff instead of a bare path, since `git diff`
//!   omits untracked files entirely.
//! - [`prepare`] fits every file into one prompt budget. A single long file
//!   otherwise spends the whole budget on the top of the file — usually the
//!   part a large change leaves untouched — and the model never sees the hunks
//!   that changed.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::git::{self, StatusRow};

/// Cap on one file's diff in the prompt.
const MAX_FILE_DIFF_BYTES: usize = 64 * 1024;
/// Every file keeps at least this much of its diff, however big the change
/// set, so a commit of hundreds of files still describes each of them.
const MIN_FILE_DIFF_BYTES: usize = 1024;
/// Cap on one file's ORIGINAL CODE excerpt.
const MAX_ORIGINAL_BYTES: usize = 48 * 1024;
/// A per-file source excerpt below this is not worth its bytes; the budget goes
/// to the diffs instead.
const MIN_ORIGINAL_BYTES: usize = 1024;
/// Lines of context kept around each hunk in an ORIGINAL CODE excerpt.
const ORIGINAL_CONTEXT_LINES: usize = 24;
/// Untracked files are read from disk up to this size to build their diff.
const MAX_UNTRACKED_BYTES: usize = 1024 * 1024;
/// Room reserved inside a capped block for the "this was truncated" marker, so
/// a truncated diff or excerpt never exceeds its budget.
const TRUNCATION_MARKER_BYTES: usize = 64;
const TIMEOUT: Duration = Duration::from_secs(180);

/// How much diff and source text one prompt may carry, in bytes.
///
/// Sized from the selected model's context window when the catalog reports one:
/// how many changed files fit is a property of the model, not of the commit. A
/// 1M-token window can describe a few hundred files; a 32k window cannot, and
/// the prompt has to shrink rather than overflow and fail.
#[derive(Clone, Copy, Debug)]
struct Budget {
    diff: usize,
    original: usize,
}

impl Budget {
    /// Content budget when the model's window is unknown (no catalog entry yet,
    /// or a provider that does not report one): room for a hundred-plus files,
    /// while staying inside the smallest window any shipped provider uses.
    const DEFAULT_TOTAL: usize = 256 * 1024;
    /// Never claim less than this, however small the window: below it the
    /// per-file floor alone would eat the budget.
    const MIN_TOTAL: usize = 24 * 1024;
    /// Ceiling, so one huge commit cannot turn into a multi-megabyte prompt.
    const MAX_TOTAL: usize = 2 * 1024 * 1024;
    /// Diff text and source code are punctuation-heavy: roughly three bytes per
    /// token is a safe estimate for a byte budget.
    const BYTES_PER_TOKEN: usize = 3;

    /// `reserved` is what the prompt already spends outside the per-file
    /// sections (the changed-file list and the commit-style samples).
    fn for_context(context_tokens: Option<u64>, reserved: usize) -> Self {
        let total = context_tokens
            .map(|tokens| {
                (tokens as usize)
                    .saturating_mul(Self::BYTES_PER_TOKEN)
                    // Half the window goes to change content; the rest carries
                    // the system prompt, the file list, and the answer.
                    .saturating_div(2)
            })
            .unwrap_or(Self::DEFAULT_TOTAL)
            .clamp(Self::MIN_TOTAL, Self::MAX_TOTAL)
            .saturating_sub(reserved)
            .max(Self::MIN_TOTAL / 2);
        Self {
            diff: total / 2,
            original: total * 3 / 10,
        }
    }
}

/// What the generated message has to describe: the same change set the commit
/// will record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Only the index (`git diff --cached`) — the default when anything is
    /// staged, and what a plain commit records.
    Staged,
    /// The whole working tree, untracked files included — what the commit
    /// button records when *Include unstaged changes* is on.
    Worktree,
}

impl Scope {
    /// The patch `git` provides for this scope.
    fn patch(self, cwd: &Path) -> Result<String, String> {
        match self {
            Self::Staged => git::staged_patch(cwd),
            Self::Worktree => git::worktree_patch(cwd),
        }
    }

    /// Whether a status row belongs to this scope's change set.
    pub fn matches(self, row: &StatusRow) -> bool {
        match self {
            Self::Staged => row.staged(),
            Self::Worktree => row.staged() || row.unstaged(),
        }
    }
}

/// Recent commit subjects sampled for style, kept separate so the prompt can
/// prefer the author's own conventions over the repository's.
#[derive(Default)]
struct RecentCommits {
    repository: Vec<String>,
    user: Vec<String>,
}

/// One changed file as the prompt sees it: its previous content (for context)
/// and its diff (the source of truth).
struct Change {
    path: String,
    /// File content at HEAD, replaced by [`prepare`] with a line-numbered
    /// excerpt. `None` when there is no previous version (a new or binary
    /// file).
    head: Option<String>,
    diff: String,
}

/// Everything a generation needs beyond the repository and its change set.
pub struct Options<'a> {
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub scope: Scope,
    /// How many messages were generated for this change set already, and the
    /// last one shown, so a regeneration can differ instead of repeating
    /// itself.
    pub attempt: usize,
    pub previous: Option<&'a str>,
    /// The selected model's context window in tokens, when the catalog reports
    /// one. Decides how many changed files fit in the prompt.
    pub context_window: Option<u64>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            scope: Scope::Staged,
            attempt: 0,
            previous: None,
            context_window: None,
        }
    }
}

/// Bytes set aside for the commit-style samples before the diff budget is sized.
const RECENT_COMMITS_RESERVE: usize = 8 * 1024;

/// Generate a conventional commit message for `options.scope` at `cwd`.
/// Blocking; call on the background executor.
pub fn generate(cwd: &Path, options: &Options<'_>, rows: &[StatusRow]) -> Result<String, String> {
    let patch = options.scope.patch(cwd)?;
    let mut changes = collect_changes(cwd, &patch, rows, options.scope);
    // The file list grows with the change set, so it is charged against the
    // budget before the per-file sections get their share.
    let rows_text = staged_summary(rows);
    let budget = Budget::for_context(
        options.context_window,
        rows_text.len() + RECENT_COMMITS_RESERVE,
    );
    let dropped = prepare(&mut changes, budget);
    let recent = recent_commits(cwd);
    let repo_name = cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let branch = git::current_branch(cwd);
    let system = system_prompt();
    let prompt = user_prompt(
        &repo_name,
        branch.as_deref(),
        &rows_text,
        &changes,
        &recent,
        dropped,
        options,
    );
    dump_prompt(&system, &prompt);
    let raw = run_pi(cwd, options.provider, options.model, &system, &prompt)?;
    process_response(&raw).ok_or_else(|| tr!("commit_message.no_message"))
}

/// A no-model fallback: a Conventional Commit `type(scope): description` plus a
/// bulleted body built from the changed file names — never a bare file count.
pub fn heuristic(rows: &[StatusRow]) -> String {
    let total = rows.len();
    let mut added = 0usize;
    let mut deleted = 0usize;
    let mut untracked = 0usize;
    for row in rows {
        if row.untracked() {
            untracked += 1;
        } else {
            match row.staged_badge() {
                'A' => added += 1,
                'D' => deleted += 1,
                _ => {}
            }
        }
    }
    let kind = if deleted > 0 && added == 0 && untracked == 0 && total == deleted {
        "chore"
    } else if added > 0 || untracked > 0 {
        "feat"
    } else {
        "chore"
    };
    let verb = if total > 0 && deleted == total {
        "remove"
    } else if total > 0 && added + untracked == total {
        "add"
    } else {
        "update"
    };
    let description = describe_change(rows, verb);
    let subject = match common_scope(rows) {
        Some(scope) => format!("{kind}({scope}): {description}"),
        None => format!("{kind}: {description}"),
    };
    match describe_body(rows) {
        Some(body) => format!("{subject}\n\n{body}"),
        None => subject,
    }
}

/// The fallback's subject tail: the changed areas named in prose, so it reads
/// as a description (`update git panel and commit message`) rather than a count.
fn describe_change(rows: &[StatusRow], verb: &str) -> String {
    let names = distinct_names(rows);
    match names.len() {
        0 => format!("{verb} files"),
        1 => format!("{verb} {}", names[0]),
        _ => format!("{verb} {} and {}", names[0], names[1]),
    }
}

/// The fallback's body: one past-tense bullet per change group, matching the
/// generated shape (subject, blank line, then `- ` bullets ending with a
/// period).
fn describe_body(rows: &[StatusRow]) -> Option<String> {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for row in rows {
        let name = humanize(&row.path);
        match if row.untracked() {
            'A'
        } else {
            row.staged_badge()
        } {
            'A' => added.push(name),
            'D' => removed.push(name),
            _ => changed.push(name),
        }
    }
    let mut lines: Vec<String> = Vec::new();
    if !changed.is_empty() {
        lines.push(format!("- Updated {}.", join_names(&changed)));
    }
    if !added.is_empty() {
        lines.push(format!("- Added {}.", join_names(&added)));
    }
    if !removed.is_empty() {
        lines.push(format!("- Removed {}.", join_names(&removed)));
    }
    if lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

/// Distinct human-readable names, in first-seen order.
fn distinct_names(rows: &[StatusRow]) -> Vec<String> {
    let mut names = Vec::new();
    for row in rows {
        let name = humanize(&row.path);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// A file path reduced to a spaced, human word (`commit_message.rs` →
/// `commit message`), falling back to the raw path when there is no stem.
fn humanize(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().replace(['_', '-'], " "))
        .unwrap_or_else(|| path.to_string())
}

/// `a, b, and c` — an Oxford-comma list for the fallback prose.
fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// One fallback summary line: `M path (+n -m)`, untracked files marked `U`.
fn describe_row(row: &StatusRow) -> String {
    let badge = if row.untracked() {
        'U'
    } else {
        row.staged_badge()
    };
    let (adds, dels) = if row.untracked() || row.staged_additions + row.staged_deletions == 0 {
        (row.unstaged_additions, row.unstaged_deletions)
    } else {
        (row.staged_additions, row.staged_deletions)
    };
    let stat = if adds > 0 || dels > 0 {
        format!(" (+{adds} -{dels})")
    } else {
        String::new()
    };
    format!("{badge} {}{stat}", row.path)
}

fn common_scope(rows: &[StatusRow]) -> Option<String> {
    let mut parts = rows
        .iter()
        .map(|row| row.path.split('/').collect::<Vec<_>>());
    let first = parts.next()?;
    if first.len() < 2 {
        return None;
    }
    let mut common = first.len() - 1;
    for parts in parts {
        let mut shared = 0;
        while shared < common && shared + 1 < parts.len() && parts[shared] == first[shared] {
            shared += 1;
        }
        common = shared;
        if common == 0 {
            return None;
        }
    }
    (common >= 1).then(|| first[common - 1].to_string())
}

/// A compact, ordered list of the changed files with their change kind and
/// line deltas. Gives the model the file-level map even when a diff is
/// truncated, and pairs with the diffs so the body can name what actually
/// changed.
fn staged_summary(rows: &[StatusRow]) -> String {
    rows.iter()
        .map(|row| format!("  {}", describe_row(row)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The system rules, mirroring VS Code Copilot's git commit message prompt.
fn system_prompt() -> String {
    String::from(
        "You are an AI programming assistant, helping a software developer come up with the best git commit message for their code changes.\n\
         You excel in interpreting the purpose behind code changes to craft succinct, clear commit messages that adhere to the repository's guidelines.\n\n\
         # First, think step-by-step:\n\
         1. Analyze the CODE CHANGES thoroughly to understand what's been modified.\n\
         2. Use the ORIGINAL CODE to understand the context of the CODE CHANGES. Use the line numbers to map the CODE CHANGES to the ORIGINAL CODE.\n\
         3. Identify the purpose of the changes to answer the *why* for the commit message, also considering the optionally provided RECENT USER COMMITS.\n\
         4. Review the provided RECENT REPOSITORY COMMITS to identify established commit message conventions. Focus on the format and style, ignoring commit-specific details like refs, tags, and authors.\n\
         5. Generate a thoughtful and succinct commit message for the given CODE CHANGES. It MUST follow the established writing conventions.\n\
         6. Remove any meta information like issue references, tags, or author names from the commit message. The developer will add them.\n\
         7. Now only show your message, wrapped with a single markdown ```text codeblock! Do not provide any explanations or details.\n",
    )
}

/// The user message, mirroring VS Code Copilot's tagged prompt structure:
/// repository context, recent commits, per-file original code + diff, and the
/// closing reminder.
fn user_prompt(
    repo_name: &str,
    branch: Option<&str>,
    rows_text: &str,
    changes: &[Change],
    recent: &RecentCommits,
    dropped: usize,
    options: &Options<'_>,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("<repository-context>\n# REPOSITORY DETAILS:\n");
    prompt.push_str(&format!("Repository name: {repo_name}\n"));
    prompt.push_str(&format!(
        "Branch name: {}\n",
        branch.unwrap_or("(detached HEAD)")
    ));
    prompt.push_str("</repository-context>\n\n");

    if !recent.user.is_empty() {
        prompt.push_str(
            "<user-commits>\n# RECENT USER COMMITS (For reference only, do not copy!):\n",
        );
        for subject in &recent.user {
            prompt.push_str(&format!("- {subject}\n"));
        }
        prompt.push_str("</user-commits>\n\n");
    }
    if !recent.repository.is_empty() {
        prompt.push_str(
            "<recent-commits>\n# RECENT REPOSITORY COMMITS (For reference only, do not copy!):\n",
        );
        for subject in &recent.repository {
            prompt.push_str(&format!("- {subject}\n"));
        }
        prompt.push_str("</recent-commits>\n\n");
    }

    // A regeneration that returned the same message is worse than useless, so a
    // second attempt is told what it already said.
    if let Some(previous) = options
        .previous
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        prompt.push_str(&format!(
            "<previous-attempt>\n\
             # PREVIOUS ATTEMPT (attempt {}; the user asked again, write a different message):\n\
             ```text\n{previous}\n```\n\
             </previous-attempt>\n\n",
            options.attempt,
        ));
    }

    prompt.push_str("<changes>\n# CHANGED FILES (status and line counts):\n");
    if rows_text.is_empty() {
        prompt.push_str("  (none)\n");
    }
    prompt.push_str(rows_text);
    for change in changes {
        prompt.push_str(&format!("\n<file path=\"{}\">\n", change.path));
        prompt.push_str("<original-code>\n# ORIGINAL CODE:\n");
        match &change.head {
            Some(head) => {
                prompt.push_str(&format!("// File: {}\n", change.path));
                prompt.push_str(head);
                if !head.ends_with('\n') {
                    prompt.push('\n');
                }
            }
            None => prompt.push_str(&format!(
                "// File: {}\n// (new file — no previous version)\n",
                change.path
            )),
        }
        prompt.push_str("</original-code>\n<code-changes>\n# CODE CHANGES:\n```diff\n");
        if change.diff.is_empty() {
            prompt.push_str("(no textual diff: binary or unchanged content)\n");
        } else {
            prompt.push_str(&change.diff);
            if !change.diff.ends_with('\n') {
                prompt.push('\n');
            }
        }
        prompt.push_str("```\n</code-changes>\n</file>\n");
    }
    if dropped > 0 {
        prompt.push_str(&format!(
            "\n# {dropped} changed file(s) above have no diff excerpt here: the prompt budget for this model is spent. The file list is complete.\n",
        ));
    }
    prompt.push_str("</changes>\n\n");

    prompt.push_str(
        "<reminder>\n\
         Now generate a commit message that describes the CODE CHANGES.\n\
         DO NOT COPY commits from RECENT COMMITS, but use them as reference for the commit style.\n\
         ONLY return a single markdown code block, NO OTHER PROSE!\n\
         ```text\n\
         commit message goes here\n\
         ```\n\
         </reminder>",
    );
    prompt
}

/// Sample the last 5 repository commits and the last 5 commits by the current
/// author, matching VS Code's commit-message context.
fn recent_commits(cwd: &Path) -> RecentCommits {
    let repository = git::history(cwd, 5, 0)
        .map(recent_subjects)
        .unwrap_or_default();
    let user = git::user_name(cwd)
        .and_then(|name| git::history_by_author(cwd, &name, 5).ok())
        .map(recent_subjects)
        .unwrap_or_default();
    RecentCommits { repository, user }
}

fn recent_subjects(entries: Vec<git::CommitEntry>) -> Vec<String> {
    entries
        .into_iter()
        .map(|commit| commit.subject)
        .filter(|subject| !subject.is_empty())
        .collect()
}

/// Pair every changed file with its previous content and its diff.
fn collect_changes(cwd: &Path, patch: &str, rows: &[StatusRow], scope: Scope) -> Vec<Change> {
    let mut changes: Vec<Change> = split_diffs(patch)
        .into_iter()
        .map(|(path, diff)| Change {
            head: git::file_at_head(cwd, &path),
            path,
            diff,
        })
        .collect();
    for row in rows {
        if changes.iter().any(|change| change.path == row.path) {
            continue;
        }
        // Binary files and anything git omitted still appear. An untracked file
        // is real content the commit will record, so in the worktree scope it
        // gets a synthesized diff instead of an empty one: `git diff` never
        // shows untracked files, and a missing diff here means the model sees a
        // path and nothing else.
        let mut change = Change {
            path: row.path.clone(),
            head: None,
            diff: String::new(),
        };
        if scope == Scope::Worktree && row.untracked() {
            change.diff = untracked_diff(cwd, &row.path).unwrap_or_default();
        } else {
            change.head = git::file_at_head(cwd, &row.path);
        }
        changes.push(change);
    }
    changes
}

/// A `/dev/null` diff for an untracked file, built from its working-tree
/// content. `None` for a binary, empty, or unreadable file.
fn untracked_diff(cwd: &Path, path: &str) -> Option<String> {
    let mut file = std::fs::File::open(cwd.join(path)).ok()?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_UNTRACKED_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let truncated = file
        .metadata()
        .map(|meta| meta.len() as usize > bytes.len())
        .unwrap_or(false);
    let text = String::from_utf8_lossy(&bytes);
    // The same test `git` uses to call a file binary.
    if text.is_empty() || text.contains('\u{0}') {
        return None;
    }
    let count = text.lines().count();
    let mut diff = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{count} @@\n"
    );
    for line in text.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    if truncated {
        diff.push_str(&format!(
            "+… [file truncated at {MAX_UNTRACKED_BYTES} bytes]\n"
        ));
    }
    Some(diff)
}

/// Fit every change into the prompt budget: a fair share of the diff allowance
/// per file, and a line-numbered ORIGINAL CODE excerpt windowed around the
/// file's hunks (see [`original_excerpt`]).
///
/// Returns how many trailing files were left without a section because the
/// budget could not even give them [`MIN_FILE_DIFF_BYTES`]. They are still named
/// in the changed-file list; the caller says so in the prompt.
fn prepare(changes: &mut Vec<Change>, budget: Budget) -> usize {
    let count = changes.len().max(1);
    let original_share = budget.original / count;
    // A share smaller than a hunk window is not worth its bytes; the space goes
    // to the diffs, which are what the message is written from.
    let originals = original_share >= MIN_ORIGINAL_BYTES;
    let diff_total = if originals {
        budget.diff
    } else {
        budget.diff + budget.original
    };
    let share = (diff_total / count).clamp(MIN_FILE_DIFF_BYTES, MAX_FILE_DIFF_BYTES);
    let mut diff_budget = diff_total;
    let mut original_budget = budget.original;
    let mut served = 0;
    for change in changes.iter_mut() {
        let allowance = share.min(diff_budget);
        if allowance < MIN_FILE_DIFF_BYTES {
            break;
        }
        let ranges = hunk_old_ranges(&change.diff);
        if let Some(head) = change.head.take() {
            let allowance = original_share.min(MAX_ORIGINAL_BYTES).min(original_budget);
            if allowance < MIN_ORIGINAL_BYTES {
                change.head = Some(
                    "// (original code omitted to stay inside the prompt budget)\n".to_string(),
                );
            } else {
                let excerpt = original_excerpt(&head, &ranges, allowance);
                original_budget = original_budget.saturating_sub(excerpt.len());
                change.head = Some(excerpt);
            }
        }
        change.diff = truncate_diff(&change.diff, allowance);
        diff_budget = diff_budget.saturating_sub(change.diff.len());
        served += 1;
    }
    let dropped = changes.len() - served;
    changes.truncate(served);
    dropped
}

/// The old-side line ranges a unified diff touches: `@@ -a,b +c,d @@` yields
/// the inclusive range `a..=a+b-1`. A pure addition (`b == 0`) keeps the line
/// it is inserted after, so the excerpt still shows where it landed.
fn hunk_old_ranges(diff: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for line in diff.lines() {
        let Some(rest) = line.strip_prefix("@@ -") else {
            continue;
        };
        let Some((old, _)) = rest.split_once(" +") else {
            continue;
        };
        let (start, count) = match old.split_once(',') {
            Some((start, count)) => (start, count.parse::<usize>().unwrap_or(1)),
            None => (old, 1usize),
        };
        let Ok(start) = start.parse::<usize>() else {
            continue;
        };
        let first = start.max(1);
        let last = (start + count.saturating_sub(1)).max(first);
        ranges.push((first, last));
    }
    ranges
}

/// Inclusive line windows around the hunks, each padded by
/// [`ORIGINAL_CONTEXT_LINES`] and merged when they meet.
fn merge_windows(ranges: &[(usize, usize)], line_count: usize) -> Vec<(usize, usize)> {
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for &(start, end) in ranges {
        let from = start.saturating_sub(ORIGINAL_CONTEXT_LINES).max(1);
        let to = (end + ORIGINAL_CONTEXT_LINES).min(line_count);
        match windows.last_mut() {
            Some(last) if from <= last.1 + 1 => last.1 = last.1.max(to),
            _ => windows.push((from, to)),
        }
    }
    windows
}

/// Keep at most the first and last few windows of a file with many hunks, so
/// assembling the excerpt stays linear in the budget rather than in the diff.
fn thin_windows(windows: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    const KEEP: usize = 4;
    if windows.len() <= KEEP * 2 {
        return windows;
    }
    let mut kept: Vec<(usize, usize)> = windows[..KEEP].to_vec();
    kept.extend_from_slice(&windows[windows.len() - KEEP..]);
    kept
}

/// ORIGINAL CODE for one file: line-numbered windows around the diff's hunks.
///
/// This is what makes a long file legible. The whole file rarely fits, and the
/// head of the file is exactly the part a large change tends to leave alone;
/// the system prompt also tells the model to map the diff's line numbers onto
/// the original code, which needs the numbers to be there.
fn original_excerpt(head: &str, ranges: &[(usize, usize)], max_bytes: usize) -> String {
    let lines: Vec<&str> = head.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let windows = if ranges.is_empty() {
        vec![(1, lines.len())]
    } else {
        thin_windows(merge_windows(ranges, lines.len()))
    };
    let width = windows
        .last()
        .map(|(_, end)| *end)
        .unwrap_or(1)
        .to_string()
        .len();
    let mut text = String::new();
    for (index, (start, end)) in windows.iter().copied().enumerate() {
        if index > 0 {
            text.push_str("// …\n");
        }
        for number in start..=end.min(lines.len()) {
            text.push_str(&format!(
                "{number:>width$}: {}\n",
                lines[number - 1],
                width = width
            ));
        }
    }
    truncate_middle(&text, max_bytes)
}

/// Keep the head and the tail of `text`, dropping the middle: a change can sit
/// at either end of the region being sent, so a head-only cut is the wrong
/// shape.
fn truncate_middle(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let budget = max.saturating_sub(TRUNCATION_MARKER_BYTES);
    let half = budget / 2;
    let mut head_end = half.min(text.len());
    while head_end > 0 && !text.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = text.len().saturating_sub(half);
    while tail_start < text.len() && !text.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let omitted = tail_start.saturating_sub(head_end);
    format!(
        "{}\n// … [{omitted} bytes omitted] …\n{}",
        &text[..head_end],
        &text[tail_start..]
    )
}

/// Keep a long diff's file header plus its first and last hunks. Cutting at a
/// fixed byte offset keeps only the top of the file — the part a big change
/// usually leaves untouched — so the model would never read the change itself.
fn truncate_diff(diff: &str, max: usize) -> String {
    if diff.len() <= max {
        return diff.to_string();
    }
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            hunks.push(String::new());
        }
        match hunks.last_mut() {
            Some(hunk) => hunk.push_str(line),
            None => header.push_str(line),
        }
    }
    let budget = max.saturating_sub(header.len() + TRUNCATION_MARKER_BYTES);
    if hunks.len() < 2 || budget == 0 {
        return truncate_middle(diff, max);
    }
    let half = budget / 2;
    let mut head = String::new();
    let mut front = 0;
    while front < hunks.len() && head.len() + hunks[front].len() <= half {
        head.push_str(&hunks[front]);
        front += 1;
    }
    if front == 0 {
        return truncate_middle(diff, max);
    }
    let mut tail = String::new();
    let mut back = hunks.len();
    while back > front && tail.len() + hunks[back - 1].len() <= budget - head.len() {
        tail.insert_str(0, &hunks[back - 1]);
        back -= 1;
    }
    let mut out = header;
    out.push_str(&head);
    if back > front {
        out.push_str(&format!("\n… [{} hunk(s) omitted] …\n", back - front));
    }
    out.push_str(&tail);
    out
}

/// Split a unified diff into `(path, chunk)` pairs at `diff --git` boundaries.
fn split_diffs(patch: &str) -> Vec<(String, String)> {
    let mut diffs: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            if let Some(done) = current.take() {
                diffs.push(done);
            }
            let path = line
                .trim_end()
                .rsplit(" b/")
                .next()
                .unwrap_or_default()
                .to_string();
            current = Some((path, line.to_string()));
        } else if let Some((_, diff)) = current.as_mut() {
            diff.push_str(line);
        }
    }
    if let Some(done) = current.take() {
        diffs.push(done);
    }
    diffs
}

/// `ORBIT_COMMIT_PROMPT_DUMP=/abs/path` writes the exact system and user
/// messages the model sees, so a wrong commit message can be traced to its
/// input instead of guessed at.
fn dump_prompt(system: &str, prompt: &str) {
    let Ok(path) = std::env::var("ORBIT_COMMIT_PROMPT_DUMP") else {
        return;
    };
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    let body = format!(
        "=== system ({} bytes) ===\n{system}\n\n=== user ({} bytes) ===\n{prompt}\n",
        system.len(),
        prompt.len()
    );
    let _ = std::fs::write(path, body);
}

fn run_pi(
    cwd: &Path,
    provider: Option<&str>,
    model: Option<&str>,
    system: &str,
    prompt: &str,
) -> Result<String, String> {
    let bin = orbit_rpc::pi_binary();
    let mut command = Command::new(&bin);
    command
        .arg("-p")
        // Extensions are deliberately left enabled: custom providers
        // (ollama-cloud, clinepass, …) are installed as pi extensions, so
        // `--no-extensions` makes `--provider`/`--model` fail with "Unknown
        // provider" and the call falls back to the generic heuristic. Every
        // other capability that could run code is still disabled here.
        .args([
            "--no-tools",
            "--no-session",
            "--no-skills",
            "--no-context-files",
            "--no-approve",
        ])
        .arg("--system-prompt")
        .arg(system)
        .env("PI_SKIP_VERSION_CHECK", "1")
        // `pi` is `#!/usr/bin/env node`; a bundled `.app` PATH lacks `node`.
        .env("PATH", orbit_rpc::augmented_path(Path::new(&bin).parent()))
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(provider) = provider.filter(|value| !value.is_empty()) {
        command.args(["--provider", provider]);
    }
    if let Some(model) = model.filter(|value| !value.is_empty()) {
        command.args(["--model", model]);
    }
    command.arg("--").arg(prompt);
    orbit_rpc::hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|err| tr!("errors.failed_to_run", bin = bin, error = err))?;
    let stdout = child.stdout.take().ok_or("pi stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("pi stderr unavailable")?;
    let out_handle = std::thread::spawn(move || read_to_end(stdout));
    let err_handle = std::thread::spawn(move || read_to_end(stderr));

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = out_handle.join().unwrap_or_default();
                let err = err_handle.join().unwrap_or_default();
                return if status.success() {
                    Ok(out)
                } else {
                    Err(tr!(
                        "errors.pi_exited_with",
                        status = status.to_string(),
                        stderr = first_line(&err)
                    ))
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(tr!("errors.commit_generation_timed_out"));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(tr!("errors.pi_process_error", error = err)),
        }
    }
}

fn read_to_end(mut reader: impl Read) -> String {
    let mut buffer = Vec::new();
    let _ = reader.read_to_end(&mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(&tr!("commit_message.no_error_output"))
        .to_string()
}

/// Extract the first fenced ```text block from the model output, mirroring
/// VS Code Copilot's `processGeneratedCommitMessage`. Unlike a strict prefix
/// match this tolerates a preamble or trailing prose around the fence, which
/// models often add despite the instructions. Falls back to the raw text when
/// no fence is present.
///
/// The body is returned **verbatim**: a long change makes a model write a
/// structured message (headings, paragraphs, `BREAKING CHANGE:` footers) and
/// rewriting that into normalized bullets mangles it. VS Code returns the same
/// fenced body untouched.
fn process_response(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let fenced = extract_fence(trimmed, "```text").or_else(|| extract_fence(trimmed, "```"));
    let text = fenced.unwrap_or_else(|| trimmed.trim_matches('"').trim().to_string());
    (!text.is_empty()).then_some(text)
}

/// The body of the first fence opened with `marker` at the start of a line. An
/// unterminated fence still carries the message, so it is accepted.
fn extract_fence(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)?;
    if start != 0 && !text[..start].ends_with('\n') {
        return None;
    }
    let rest = &text[start + marker.len()..];
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    let body = match rest.find("\n```") {
        Some(end) => &rest[..end],
        None => rest.trim_end_matches('`'),
    };
    let body = body.trim();
    (!body.is_empty()).then(|| body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, index: char, worktree: char) -> StatusRow {
        StatusRow {
            path: path.into(),
            orig_path: None,
            index,
            worktree,
            staged_additions: 1,
            staged_deletions: 0,
            unstaged_additions: 0,
            unstaged_deletions: 0,
        }
    }

    #[test]
    fn extracts_a_single_text_fence() {
        assert_eq!(
            process_response("```text\nfeat(ui): add git panel\n```").as_deref(),
            Some("feat(ui): add git panel")
        );
        assert_eq!(
            process_response("\"fix: guard empty input\"").as_deref(),
            Some("fix: guard empty input")
        );
        // A preamble and trailing prose around the fence are tolerated.
        assert_eq!(
            process_response(
                "Here is the message:\n```text\nfix: guard input\n```\nLet me know if you want changes."
            )
            .as_deref(),
            Some("fix: guard input")
        );
        assert_eq!(process_response("   \n  "), None);
    }

    #[test]
    fn an_unterminated_fence_still_yields_the_message() {
        assert_eq!(
            process_response("```text\nfix: guard input\n").as_deref(),
            Some("fix: guard input")
        );
    }

    /// A long change makes the model write a multi-paragraph body; rewriting it
    /// into bullets and forcing periods mangles it, so the body is passed on as
    /// written.
    #[test]
    fn keeps_the_generated_body_verbatim() {
        let raw = "feat(git): read the worktree\n\n## Summary\nThe diff now comes from the worktree.\n\nBREAKING CHANGE: `generate` takes a scope.";
        assert_eq!(process_response(raw).as_deref(), Some(raw));
    }

    #[test]
    fn splits_a_patch_into_per_file_chunks() {
        let patch = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@\n-old\n+new\n\
                     diff --git a/b.rs b/b.rs\n--- /dev/null\n+++ b/b.rs\n@@\n+new\n";
        let diffs = split_diffs(patch);
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0].0, "a.rs");
        assert_eq!(diffs[1].0, "b.rs");
        assert!(diffs[1].1.contains("+new"));
    }

    #[test]
    fn reads_the_old_line_ranges_of_a_hunk() {
        let diff =
            "diff --git a/a.rs b/a.rs\n@@ -12,4 +12,5 @@\n-old\n+new\n@@ -40 +41,0 @@\n-gone\n";
        assert_eq!(hunk_old_ranges(diff), vec![(12, 15), (40, 40)]);
    }

    #[test]
    fn original_code_windows_around_a_deep_hunk_with_line_numbers() {
        let head: String = (1..=400).map(|line| format!("line {line}\n")).collect();
        let diff = "diff --git a/big.rs b/big.rs\n@@ -200,3 +200,3 @@\n-old\n+new\n";
        let excerpt = original_excerpt(&head, &hunk_old_ranges(diff), MAX_ORIGINAL_BYTES);
        assert!(excerpt.contains("200: line 200"), "{excerpt}");
        assert!(
            excerpt.contains("176: line 176"),
            "context before: {excerpt}"
        );
        assert!(
            excerpt.contains("226: line 226"),
            "context after: {excerpt}"
        );
        assert!(
            !excerpt.contains("1: line 1\n"),
            "the file head is not the change: {excerpt}"
        );
        assert!(!excerpt.contains("400: line 400\n"), "{excerpt}");
    }

    #[test]
    fn original_code_falls_back_to_the_whole_file_without_hunks() {
        assert_eq!(
            original_excerpt("alpha\nbeta\n", &[], MAX_ORIGINAL_BYTES),
            "1: alpha\n2: beta\n"
        );
    }

    #[test]
    fn long_diffs_keep_the_first_and_last_hunks() {
        let mut diff = String::from("diff --git a/big.rs b/big.rs\n");
        for hunk in 0..40 {
            diff.push_str(&format!(
                "@@ -{hunk},1 +{hunk},1 @@\n-old {hunk}\n+new {hunk}\n"
            ));
        }
        let shortened = truncate_diff(&diff, 400);
        assert!(shortened.len() < diff.len());
        assert!(shortened.contains("+new 0"), "{shortened}");
        assert!(shortened.contains("+new 39"), "{shortened}");
        assert!(shortened.contains("hunk(s) omitted"), "{shortened}");
        assert!(!shortened.contains("+new 20"), "{shortened}");
    }

    #[test]
    fn middle_truncation_keeps_both_ends() {
        let cut = truncate_middle(&"a".repeat(1000), 200);
        assert!(cut.starts_with("aaaa"), "{cut}");
        assert!(cut.ends_with("aaaa"), "{cut}");
        assert!(cut.contains("bytes omitted"), "{cut}");
        assert!(!cut.contains(&"a".repeat(300)), "{cut}");
    }

    /// A long change set must not overflow the model's context window: one
    /// budget is shared across the files instead of a per-file cap that adds
    /// up without limit.
    #[test]
    fn one_diff_budget_is_shared_across_files() {
        let budget = Budget::for_context(Some(128_000), 0);
        let mut changes: Vec<Change> = (0..6)
            .map(|file| Change {
                path: format!("f{file}.rs"),
                head: Some("1: x\n".repeat(20_000)),
                diff: big_diff(2_000),
            })
            .collect();
        let dropped = prepare(&mut changes, budget);
        assert_eq!(dropped, 0, "a small set serves every file");
        let diffs: usize = changes.iter().map(|change| change.diff.len()).sum();
        let heads: usize = changes
            .iter()
            .map(|change| change.head.as_deref().map(str::len).unwrap_or(0))
            .sum();
        assert!(diffs <= budget.diff + 128, "{diffs} bytes of diff");
        assert!(
            heads <= budget.original + 128,
            "{heads} bytes of original code"
        );
        for change in &changes {
            assert!(
                change.diff.len() <= MAX_FILE_DIFF_BYTES + 128,
                "{}: {} bytes",
                change.path,
                change.diff.len()
            );
            // The last files still say something rather than vanishing.
            assert!(!change.diff.is_empty(), "{}", change.path);
        }
    }

    /// The point of sizing the budget from the context window: a commit of a
    /// hundred-plus files still gets a description of every one of them.
    #[test]
    fn a_hundred_files_each_keep_a_diff() {
        let budget = Budget::for_context(Some(1_000_000), 0);
        let mut changes: Vec<Change> = (0..120)
            .map(|file| Change {
                path: format!("f{file}.rs"),
                head: Some("1: x\n".repeat(20_000)),
                diff: big_diff(200),
            })
            .collect();
        let dropped = prepare(&mut changes, budget);
        assert_eq!(dropped, 0, "120 files still fit a 1M-token window");
        assert_eq!(changes.len(), 120);
        for change in &changes {
            assert!(change.diff.starts_with("diff --git"), "{}", change.path);
            // A big diff keeps its first and last hunk, so the whole change is
            // still visible even at a small share.
            assert!(change.diff.contains("-old 0"), "{}", change.path);
            assert!(change.diff.contains("+new 199\n"), "{}", change.path);
        }
    }

    /// The same change set on a small window degrades instead of overflowing:
    /// every file is still listed, and the budget is respected.
    #[test]
    fn a_small_window_is_still_respected() {
        let budget = Budget::for_context(Some(32_000), 0);
        let mut changes: Vec<Change> = (0..120)
            .map(|file| Change {
                path: format!("f{file}.rs"),
                head: Some("1: x\n".repeat(20_000)),
                diff: big_diff(200),
            })
            .collect();
        let rows: Vec<StatusRow> = (0..120)
            .map(|file| row(&format!("f{file}.rs"), 'M', ' '))
            .collect();
        let rows_text = staged_summary(&rows);
        let dropped = prepare(&mut changes, budget);
        assert!(dropped > 0, "a 32k window cannot carry 120 big diffs");
        let sent: usize = changes.iter().map(|change| change.diff.len()).sum();
        // No source excerpts at this size, so their budget went to the diffs.
        assert!(
            sent <= budget.diff + budget.original + 128,
            "{sent} bytes of diff"
        );

        // Everything is still named, and the prompt says why the tail is short.
        let prompt = user_prompt(
            "orbit",
            Some("main"),
            &rows_text,
            &changes,
            &RecentCommits::default(),
            dropped,
            &Options::default(),
        );
        for file in 0..120 {
            assert!(
                prompt.contains(&format!("M f{file}.rs")),
                "f{file}.rs is listed"
            );
        }
        assert!(
            prompt.contains("have no diff excerpt here"),
            "the omission is stated"
        );
    }

    /// A diff with `hunks` one-line hunks, so truncation has hunks to drop.
    fn big_diff(hunks: usize) -> String {
        let mut diff = String::from("diff --git a/big.rs b/big.rs\n");
        for line in 0..hunks {
            diff.push_str(&format!(
                "@@ -{line},1 +{line},1 @@\n-old {line}\n+new {line}\n"
            ));
        }
        diff
    }

    #[test]
    fn an_untracked_file_gets_a_real_diff() {
        let root = std::env::temp_dir().join(format!("orbit-commit-msg-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("new.rs"), "fn main() {}\nsecond\n").unwrap();
        let diff = untracked_diff(&root, "new.rs").expect("a text file has a diff");
        assert!(diff.contains("--- /dev/null"), "{diff}");
        assert!(diff.contains("@@ -0,0 +1,2 @@"), "{diff}");
        assert!(diff.contains("+fn main() {}"), "{diff}");
        assert!(diff.contains("+second"), "{diff}");
        // A binary file has no textual diff to offer.
        std::fs::write(root.join("blob.bin"), [0u8, 1, 2]).unwrap();
        assert_eq!(untracked_diff(&root, "blob.bin"), None);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn heuristic_names_the_shared_scope() {
        let rows = vec![
            row("crates/orbit-pi/src/a.rs", 'M', ' '),
            row("crates/orbit-pi/src/b.rs", 'A', ' '),
        ];
        let message = heuristic(&rows);
        assert!(message.starts_with("feat(src):"), "{message}");
    }

    #[test]
    fn heuristic_reads_as_a_conventional_description() {
        let rows = vec![
            row("crates/orbit-pi/src/git_panel.rs", 'M', ' '),
            row("crates/orbit-pi/src/commit_message.rs", 'A', ' '),
        ];
        let message = heuristic(&rows);
        // Conventional subject with a description, not a file count.
        assert_eq!(
            message.lines().next().unwrap(),
            "feat(src): update git panel and commit message"
        );
        // One past-tense bullet per change group, each ending with a period.
        assert!(
            message.contains("\n\n- Updated git panel.\n- Added commit message."),
            "{message}"
        );
        assert!(!message.contains("- M "), "{message}");
        assert!(!message.contains("2 files"), "{message}");
    }

    #[test]
    fn prompt_follows_the_vscode_structure() {
        let rows = vec![row("src/lib.rs", 'M', ' ')];
        let changes = vec![Change {
            path: "src/lib.rs".into(),
            head: Some("1: fn old() {}\n".into()),
            diff: "diff --git a/src/lib.rs b/src/lib.rs\n".into(),
        }];
        let recent = RecentCommits {
            repository: vec!["feat: add the review pane".into()],
            user: vec!["fix: guard empty input".into()],
        };
        let user = user_prompt(
            "orbit",
            Some("main"),
            &staged_summary(&rows),
            &changes,
            &recent,
            0,
            &Options {
                attempt: 1,
                previous: Some("chore: update files"),
                ..Options::default()
            },
        );
        assert!(user.contains("<repository-context>"), "{user}");
        assert!(user.contains("Repository name: orbit"), "{user}");
        assert!(user.contains("Branch name: main"), "{user}");
        assert!(user.contains("<user-commits>"), "{user}");
        assert!(user.contains("<recent-commits>"), "{user}");
        assert!(user.contains("feat: add the review pane"), "{user}");
        assert!(user.contains("<previous-attempt>"), "{user}");
        assert!(user.contains("chore: update files"), "{user}");
        assert!(user.contains("  M src/lib.rs (+1 -0)"), "{user}");
        assert!(
            user.contains("<original-code>") && user.contains("1: fn old() {}"),
            "{user}"
        );
        assert!(user.contains("<code-changes>"), "{user}");
        assert!(user.contains("```diff"), "{user}");
        assert!(user.contains("<reminder>"), "{user}");
        assert!(
            user.contains("ONLY return a single markdown code block"),
            "{user}"
        );
        // The unstaged diff is no longer a "context only" afterthought: the
        // scope decides what the commit records.
        assert!(!user.contains("UNSTAGED CHANGES"), "{user}");

        let system = system_prompt();
        assert!(system.contains("think step-by-step"), "{system}");
        assert!(
            system.contains("single markdown ```text codeblock"),
            "{system}"
        );
    }
}
