//! Sidebar drafts: a new task the user opened but has not sent yet.
//!
//! Clicking New inserts a row immediately. Leaving that row drops it when the
//! composer is still empty, and keeps it when the composer has text or an
//! attachment. The first user message promotes the row into an ordinary
//! session. These functions are pure so the rules can be tested without a
//! pi process.

use std::path::PathBuf;

/// What happens to the open task when the user leaves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftFate {
    /// No user message, and the composer is empty. The row goes away.
    Discard,
    /// No user message, but the composer has content. The row stays.
    Keep,
    /// A user message has been sent. The row is a real session from here on.
    Promote,
}

/// Settle a listed draft from the two facts that matter at leave time:
/// whether a user message exists, and whether the saved composer is empty.
pub(crate) fn settle_listed_draft(has_user_message: bool, composer_empty: bool) -> DraftFate {
    if has_user_message {
        DraftFate::Promote
    } else if composer_empty {
        DraftFate::Discard
    } else {
        DraftFate::Keep
    }
}

/// An idle live process can take `new_session` in place only for the folder
/// it is already in, and only when that view is not an unsent draft. An
/// unsent draft owns the process; a different folder needs its own process.
pub(crate) fn reuse_live_process(
    can_reuse: bool,
    same_workspace: bool,
    unsent_draft: bool,
) -> bool {
    can_reuse && same_workspace && !unsent_draft
}

/// Clicking New again stays on the current row only when that row is already
/// an empty draft in the same workspace.
pub(crate) fn reuse_blank_draft(
    same_workspace: bool,
    is_current_draft: bool,
    promoted: bool,
    has_user_message: bool,
    composer_empty: bool,
) -> bool {
    same_workspace && is_current_draft && !promoted && !has_user_message && composer_empty
}

/// Path used for a draft before pi reports a session file. It is not a file
/// on disk; [`crate::sessions::is_header_only_session`] rejects it.
pub(crate) fn draft_placeholder(id: u64) -> PathBuf {
    PathBuf::from(format!("orbit-draft://{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_draft_is_discarded() {
        assert_eq!(settle_listed_draft(false, true), DraftFate::Discard);
    }

    #[test]
    fn a_draft_with_composer_content_is_kept() {
        assert_eq!(settle_listed_draft(false, false), DraftFate::Keep);
    }

    #[test]
    fn a_sent_message_promotes_even_when_the_composer_is_empty() {
        assert_eq!(settle_listed_draft(true, true), DraftFate::Promote);
        assert_eq!(settle_listed_draft(true, false), DraftFate::Promote);
    }

    #[test]
    fn only_the_same_workspace_blank_draft_is_reused() {
        assert!(reuse_blank_draft(true, true, false, false, true));
        assert!(!reuse_blank_draft(false, true, false, false, true));
        assert!(!reuse_blank_draft(true, true, false, false, false));
        assert!(!reuse_blank_draft(true, true, true, false, true));
        assert!(!reuse_blank_draft(true, false, false, false, true));
    }

    #[test]
    fn a_live_process_is_reused_only_in_its_own_workspace() {
        assert!(reuse_live_process(true, true, false));
        assert!(!reuse_live_process(true, false, false));
        assert!(!reuse_live_process(true, true, true));
        assert!(!reuse_live_process(false, true, false));
    }
}
