use super::*;

/// Collapsed, there is no sidebar column to split, so Hide Sidebar takes the
/// fixed lead past the OS window buttons — the one position that is correct
/// at any window width. Back/Forward follow it in the same cluster, and a
/// hidden sidebar's width must not move either of them.
#[test]
fn collapsed_controls_take_the_fixed_lead() {
    assert_eq!(
        view::titlebar_cluster_left(),
        view::TRAFFIC_LIGHT_CLEARANCE + view::TITLEBAR_CONTROLS_LEAD
    );
    assert_eq!(
        view::titlebar_toggle_left(false, 200.),
        view::titlebar_cluster_left()
    );
    assert_eq!(
        view::titlebar_history_left(false, 0.),
        view::titlebar_history_left(false, SIDEBAR_DEFAULT_W),
        "a hidden sidebar must not shift the history pair"
    );
    assert_eq!(
        view::titlebar_history_left(false, 0.) + view::TITLEBAR_HISTORY_W + 6.,
        view::titlebar_cluster_left() + view::TITLEBAR_CONTROLS_W,
        "the collapsed pair must still end where the title inset expects"
    );
}

/// The title still has to clear the cluster once the sidebar is gone: its
/// leading inset must leave `TITLEBAR_TITLE_GAP` past the chips' edge on top
/// of the cluster's own trailing pad, or the two sit flush.
#[test]
fn collapsed_title_clears_the_controls() {
    let chips_end = view::titlebar_cluster_left() + view::TITLEBAR_CONTROLS_W;
    assert_eq!(
        view::TITLEBAR_LEADING - chips_end,
        view::TITLEBAR_TITLE_GAP,
        "the title must start a real gap past the controls"
    );
}

/// Open, Back/Forward right-align inside the sidebar's strip: whatever the
/// sidebar's width, their right edge lands `SIDEBAR_EDGE_PAD` off the
/// column's edge (clearing the 6px resize handle that lives against it).
#[test]
fn open_history_hugs_the_sidebar_edge() {
    for width in [SIDEBAR_MIN_W, SIDEBAR_DEFAULT_W, 420.] {
        let left = view::titlebar_history_left(true, width);
        assert_eq!(
            left + view::TITLEBAR_HISTORY_W + view::SIDEBAR_EDGE_PAD,
            width,
            "history must sit SIDEBAR_EDGE_PAD off the sidebar edge at {width}px"
        );
        assert!(
            left >= view::TRAFFIC_LIGHT_CLEARANCE,
            "at {width}px the history pair would ride into the OS window buttons"
        );
    }
}

/// Open, Hide Sidebar uses the New Task alignment unless that would cover the
/// OS window buttons. Collapsed, the alignment is ignored and the toggle
/// leads the cluster.
#[test]
fn open_toggle_clears_the_traffic_lights() {
    assert_eq!(
        view::titlebar_toggle_left(true, 0.),
        view::TRAFFIC_LIGHT_CLEARANCE
    );
    assert_eq!(view::titlebar_toggle_left(true, 200.), 200.);
    assert_eq!(
        view::titlebar_toggle_left(false, 200.),
        view::titlebar_cluster_left()
    );
}
