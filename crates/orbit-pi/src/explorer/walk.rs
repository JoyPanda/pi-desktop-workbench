//! Off-thread workspace snapshot for the Explorer and the composer's `@`
//! mentions.
//!
//! One recursive pass per (workspace) change, run on the background executor.
//! The [`ignore`] crate — ripgrep's walker, already in the dependency graph —
//! is what makes nested `.gitignore` / `.ignore` / global-ignore handling
//! correct; a hand-rolled `read_dir` walk cannot read a nested ignore file
//! without descending into it first.
//!
//! Nothing here touches a frame: it returns plain relative paths that
//! [`super::tree`] turns into an in-memory index and that
//! [`crate::mentions::MentionIndex`] ranks for `@`.
//!
//! Two deliberate choices, both matching what VS Code and Zed do once they
//! stop hiding dotfiles:
//!
//! - **Hidden entries are indexed, not skipped** (VS Code passes `--hidden`;
//!   Zed indexes dotfiles and only *flags* them via `hidden_files`). Callers
//!   that want them out of sight filter on [`Entry::is_hidden`].
//! - **`.gitignore` applies even outside a git repository** (`require_git(false)`,
//!   VS Code's `--no-require-git`), while ignore files in *ancestor*
//!   directories do not (`parents(false)`, VS Code's `--no-ignore-parent`).
//!   A stray `~/.gitignore` containing `*` would otherwise silently blank the
//!   whole index — the trap Codex's `require_git(true)` exists to avoid.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use ignore::{WalkBuilder, WalkState};

/// Hard caps so a pathological tree stays bounded. The callers surface
/// [`Snapshot::truncated`] honestly rather than silently dropping files.
pub const MAX_ENTRIES: usize = 200_000;
pub const MAX_DEPTH: usize = 24;

/// Version-control and OS metadata that is never workspace content. `.git` is
/// not in `.gitignore` (git treats it specially), so indexing hidden entries
/// would otherwise drag tens of thousands of object files into the index.
/// Compared case-insensitively: Windows and macOS filesystems are.
const METADATA_NAMES: &[&str] = &[".git", ".svn", ".hg", ".jj", ".DS_Store", "Thumbs.db"];

/// One workspace entry. `path` is workspace-relative and `/`-separated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub path: String,
    pub is_dir: bool,
    /// A path segment starting with `.`. Indexed either way; the Explorer
    /// hides these by default, `@` searches them.
    pub is_hidden: bool,
}

/// Workspace-relative paths (always `/`-separated) for every visible entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Snapshot {
    pub entries: Vec<Entry>,
    /// The entry cap was hit; the tree is incomplete.
    pub truncated: bool,
}

/// Recursively snapshot `root`, honoring ignore files.
///
/// Symlinks are never followed: a link back up the tree would otherwise walk
/// forever, and a project panel has no business resolving them.
pub fn snapshot(root: &Path) -> Snapshot {
    let mut out = Snapshot::default();
    if !root.is_dir() {
        return out;
    }

    let mut builder = WalkBuilder::new(root);
    builder
        // `false` = do not skip hidden entries.
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        // Only this directory's ignore files (and its children's) count.
        .parents(false)
        // Honor `.gitignore` in a plain folder too, not only inside a repo.
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(MAX_DEPTH))
        .filter_entry(|entry| {
            // The predicate sees the walk root as well; never prune it.
            if entry.depth() == 0 {
                return true;
            }
            let Some(name) = entry.file_name().to_str() else {
                return true;
            };
            !METADATA_NAMES
                .iter()
                .any(|skip| skip.eq_ignore_ascii_case(name))
        });

    let entries = Mutex::new(Vec::with_capacity(1024));
    let seen = AtomicUsize::new(0);
    let truncated = AtomicBool::new(false);

    builder.build_parallel().run(|| {
        Box::new(|result| {
            let Ok(entry) = result else {
                return WalkState::Continue;
            };
            // Depth 0 is the workspace root itself.
            if entry.depth() == 0 {
                return WalkState::Continue;
            }
            let Some(file_type) = entry.file_type() else {
                return WalkState::Continue;
            };
            // Symlink (not followed) or other special file — skip it.
            if !file_type.is_dir() && !file_type.is_file() {
                return WalkState::Continue;
            }
            let Ok(relative) = entry.path().strip_prefix(root) else {
                return WalkState::Continue;
            };
            let Some(relative) = relative_path(relative) else {
                return WalkState::Continue;
            };
            if seen.fetch_add(1, Ordering::Relaxed) >= MAX_ENTRIES {
                truncated.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }
            entries.lock().expect("snapshot buffer").push(Entry {
                is_hidden: is_hidden_path(&relative),
                is_dir: file_type.is_dir(),
                path: relative,
            });
            WalkState::Continue
        })
    });

    out.entries = entries.into_inner().expect("snapshot buffer");
    // Parallel completion order is arbitrary; every consumer wants a stable one.
    out.entries.sort_by(|a, b| a.path.cmp(&b.path));
    out.truncated = truncated.load(Ordering::Relaxed);
    out
}

/// A `/`-separated relative path as a string. `None` for the root (empty) or a
/// non-UTF-8 path.
fn relative_path(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for component in path.components() {
        parts.push(component.as_os_str().to_str()?.to_owned());
    }
    Some(parts.join("/"))
}

/// A path with any segment starting with `.` — dotfiles and dot-directories.
pub fn is_hidden_path(path: &str) -> bool {
    path.split('/').any(|segment| segment.starts_with('.'))
}

/// Join a relative snapshot path back onto its workspace root.
pub fn absolute(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        if !part.is_empty() {
            path.push(part);
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orbit-explorer-walk-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(dir.join("empty")).unwrap();
        fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.join("README.md"), "# hi\n").unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "x\n").unwrap();
        fs::write(dir.join(".hidden"), "x\n").unwrap();
        fs::write(dir.join(".gitignore"), "ignored.txt\nbuild/\n").unwrap();
        fs::write(dir.join("ignored.txt"), "x\n").unwrap();
        fs::create_dir_all(dir.join("build")).unwrap();
        fs::write(dir.join("build/out.js"), "x\n").unwrap();
        dir
    }

    fn has(snapshot: &Snapshot, path: &str) -> bool {
        snapshot.entries.iter().any(|entry| entry.path == path)
    }

    fn entry<'a>(snapshot: &'a Snapshot, path: &str) -> &'a Entry {
        snapshot
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .unwrap_or_else(|| panic!("{path} is not indexed"))
    }

    #[test]
    fn snapshot_honors_gitignore_without_a_git_dir() {
        let root = workspace("ignore");
        let snap = snapshot(&root);
        assert!(has(&snap, "src/main.rs"));
        assert!(has(&snap, "README.md"));
        assert!(!has(&snap, "ignored.txt"));
        assert!(!has(&snap, "build/out.js"));
        assert!(!has(&snap, "build"));
        assert!(has(&snap, "empty"));
        assert!(!snap.truncated);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hidden_entries_are_indexed_and_flagged() {
        let root = workspace("hidden");
        let snap = snapshot(&root);
        // Indexed either way — the Explorer hides them in the view, `@` finds them.
        assert!(has(&snap, ".hidden"));
        assert!(has(&snap, ".gitignore"));
        assert!(entry(&snap, ".hidden").is_hidden);
        assert!(entry(&snap, ".gitignore").is_hidden);
        assert!(!entry(&snap, "src/main.rs").is_hidden);
        assert!(!entry(&snap, "src").is_hidden);
        // A hidden directory marks its children hidden too.
        fs::create_dir_all(root.join(".claude")).unwrap();
        fs::write(root.join(".claude/settings.json"), "{}").unwrap();
        let snap = snapshot(&root);
        assert!(entry(&snap, ".claude/settings.json").is_hidden);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn vcs_metadata_is_skipped_case_insensitively() {
        let root = workspace("metadata");
        for name in [".git", ".svn", ".hg", ".jj"] {
            fs::create_dir_all(root.join(name)).unwrap();
            fs::write(root.join(name).join("payload"), "x\n").unwrap();
        }
        fs::write(root.join(".DS_Store"), "x\n").unwrap();
        fs::write(root.join("Thumbs.db"), "x\n").unwrap();
        fs::write(root.join("thumbs.db"), "x\n").unwrap();
        let snap = snapshot(&root);
        for name in [".git", ".svn", ".hg", ".jj"] {
            assert!(!has(&snap, name), "{name} directory is not content");
            assert!(!has(&snap, &format!("{name}/payload")));
        }
        assert!(!has(&snap, ".DS_Store"));
        assert!(!has(&snap, "Thumbs.db"));
        assert!(!has(&snap, "thumbs.db"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ancestor_gitignore_is_not_consulted() {
        let outer = workspace("ancestor");
        let inner = outer.join("project");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("keep.txt"), "x\n").unwrap();
        // The outer directory ignores the inner project outright.
        fs::write(outer.join(".gitignore"), "project/\n").unwrap();
        let snap = snapshot(&inner);
        assert!(
            has(&snap, "keep.txt"),
            "an ancestor .gitignore must not blank a workspace"
        );
        let _ = fs::remove_dir_all(&outer);
    }

    #[test]
    fn snapshot_does_not_follow_symlinks() {
        let root = workspace("symlink");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("src/loop")).unwrap();
            let snap = snapshot(&root);
            assert!(!snap.entries.iter().any(|e| e.path.contains("loop")));
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn entries_are_sorted_and_typed() {
        let root = workspace("sorted");
        let snap = snapshot(&root);
        let mut sorted = snap.entries.clone();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(snap.entries, sorted);
        assert!(entry(&snap, "src").is_dir);
        assert!(!entry(&snap, "src/main.rs").is_dir);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn absolute_rebuilds_a_relative_path() {
        let root = Path::new("/tmp/ws");
        assert_eq!(
            absolute(root, "src/main.rs"),
            Path::new("/tmp/ws/src/main.rs")
        );
        assert_eq!(absolute(root, ""), Path::new("/tmp/ws"));
    }
}
