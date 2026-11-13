//! Sidebar drafts: a new task the user opened but has not sent yet.
//!
//! Clicking New inserts a row immediately and starts nothing; the first user
//! message is what starts a pi process. Leaving the row drops it when the
//! composer is still empty, and keeps it when the composer has text or an
//! attachment. The first user message promotes the row into an ordinary
//! session. The fate rule is pure so it can be tested without a pi process;
//! the one impure helper here is the diagnostic switch below.

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

/// Whether to trace when a New Task row is removed or clipped from the
/// sidebar (`ORBIT_DEBUG_DRAFTS=1`). A row can vanish because something settled
/// it away or because the group's history cap pushed it out, and the two look
/// identical on screen.
pub(crate) fn trace_enabled() -> bool {
    crate::bundled_extensions::env_flag_set("ORBIT_DEBUG_DRAFTS")
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
}
