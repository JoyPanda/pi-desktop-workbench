//! The "finished while you were elsewhere" mark (issue #83): which settles
//! leave it, which view spends it, and whether a marked row survives a
//! collapsed project group.

use super::sidebar::*;
use super::*;

fn store_session(name: &str, cwd: &str, modified_secs: u64) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(format!("/store/{name}.jsonl")),
        id: name.into(),
        cwd: PathBuf::from(cwd),
        title: format!("{name} title"),
        first_message: "preview".into(),
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(modified_secs),
    }
}

fn rows_for(
    sessions: &[SessionInfo],
    collapsed: &HashSet<String>,
    review: &HashSet<PathBuf>,
) -> Vec<usize> {
    build_sidebar_rows(
        sessions,
        &[PathBuf::from("/work/alpha")],
        WorkspaceSort::Manual,
        &HashMap::new(),
        "none",
        collapsed,
        &HashSet::new(),
        &HashMap::new(),
        &HashSet::new(),
        &None,
        &HashSet::new(),
        review,
    )
    .iter()
    .filter_map(|row| match row {
        SideRow::Session(ix) => Some(*ix),
        _ => None,
    })
    .collect()
}

#[test]
fn a_marked_session_stays_visible_under_a_collapsed_group() {
    let sessions = vec![
        store_session("a1", "/work/alpha", 30),
        store_session("a2", "/work/alpha", 20),
        store_session("a3", "/work/alpha", 10),
    ];
    let collapsed: HashSet<String> = ["alpha".to_string()].into_iter().collect();
    let marked: HashSet<PathBuf> = [sessions[1].path.clone()].into_iter().collect();

    // Without the mark a collapsed group hides every row the user did not
    // open, pin, or start — the finished session disappears before it can be
    // read.
    assert!(rows_for(&sessions, &collapsed, &HashSet::new()).is_empty());

    // With it, the row stays reachable under the header.
    assert_eq!(rows_for(&sessions, &collapsed, &marked), vec![1]);
}

#[test]
fn a_settle_in_the_session_being_read_leaves_no_mark() {
    let open = PathBuf::from("/store/a1.jsonl");
    assert!(!should_mark_review(true, &open, Some(&open)));
}

#[test]
fn a_settle_out_of_sight_leaves_a_mark() {
    let open = PathBuf::from("/store/a1.jsonl");
    let parked = PathBuf::from("/store/a2.jsonl");

    // A background run, while the user reads another session in a frontmost
    // window.
    assert!(should_mark_review(true, &parked, Some(&open)));
    // The open session, while Orbit is behind another app — or while a
    // full-page surface covers the chat area. Both arrive as `in_front: false`
    // (see `transcript_in_front`), so the input is the same and the rule is the
    // same: not in front means the settle is news.
    assert!(should_mark_review(false, &open, Some(&open)));
    // Nothing open at all (the window was closed): still news.
    assert!(should_mark_review(false, &parked, None));
}

#[test]
fn opening_a_session_spends_its_mark_only_in_the_foreground() {
    let open = PathBuf::from("/store/a1.jsonl");
    let mut marks: HashMap<PathBuf, ReviewMark> =
        [(open.clone(), ReviewMark::Done)].into_iter().collect();

    // Selecting a row in a background window is not reading it: the mark
    // survives, so it is still there when the user comes back.
    assert!(!clear_review_mark(&mut marks, Some(&open), false));
    assert!(marks.contains_key(&open));

    // Shown in a foreground window: spent, and reported as a change so the
    // sidebar repaints without the glyph.
    assert!(clear_review_mark(&mut marks, Some(&open), true));
    assert!(marks.is_empty());
    // Clearing an unmarked session changes nothing.
    assert!(!clear_review_mark(&mut marks, Some(&open), true));
    assert!(!clear_review_mark(&mut marks, None, true));
}

#[test]
fn a_failed_turn_is_marked_differently_from_a_finished_one() {
    // The mark is what the row paints, so the two outcomes must not collapse
    // into one value.
    assert_ne!(ReviewMark::Done, ReviewMark::Failed);
}
