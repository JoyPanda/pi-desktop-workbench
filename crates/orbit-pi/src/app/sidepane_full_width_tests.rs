//! The hand-off between a full-page Review pane and the sessions sidebar.

use super::*;

/// Expanding the Review pane hides the session view but opens the sessions
/// sidebar, so the reader can still switch sessions while the review owns the
/// page.
#[gpui::test]
fn expanding_review_opens_the_sessions_sidebar(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // The pane is gated on the runtime probe; the test host is not
            // asked to have pi installed just to lay the window out.
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.sidebar_visible = false;
            app.sidepane.update(cx, |pane, cx| pane.toggle(cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        !cx.update(|_, cx| app.read(cx).sidebar_visible),
        "the sidebar starts closed for this test"
    );

    // The rising edge is handled in the render, so expanding takes one draw.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.sidepane
                .update(cx, |pane, cx| pane.toggle_full_width(cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.sidepane.read(cx).is_full_width());
        assert!(
            app.sidebar_visible,
            "expanding the review opens the sessions sidebar"
        );
        assert_eq!(
            app.sidepane.read(cx).width(),
            viewport.width - app.sidebar_width,
            "the expanded review shares the screen with the sidebar instead of covering it"
        );
    });
}

/// An expanded pane fills the whole page beside the open sidebar, and docking
/// gives the session view back at the width the reader dialed in.
#[gpui::test]
fn expanding_fills_the_page_beside_the_sidebar(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.sidepane.update(cx, |pane, cx| {
                pane.toggle(cx);
                pane.set_width(px(400.), cx);
            });
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());

    // Docked: the pane keeps the width the reader dialed in.
    assert_eq!(
        cx.update(|_, cx| app.read(cx).sidepane.read(cx).width()),
        px(400.)
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.sidepane
                .update(cx, |pane, cx| pane.toggle_full_width(cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let sidebar_width = cx.update(|_, cx| app.read(cx).sidebar_width);
    assert_eq!(
        cx.update(|_, cx| app.read(cx).sidepane.read(cx).width()),
        viewport.width - sidebar_width,
        "an expanded pane fills the page left beside the sessions sidebar"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.sidepane
                .update(cx, |pane, cx| pane.leave_full_width(cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert_eq!(
        cx.update(|_, cx| app.read(cx).sidepane.read(cx).width()),
        px(400.),
        "docking restores the reader's width"
    );
}

/// A wide docked Review pane must never spill past the window's right edge.
/// The pane is clamped to the space left beside the sidebar, and the chat
/// column yields by shrinking rather than forcing the row to overflow: a body
/// that refuses to shrink below its content pushes the pane (the last flex
/// child) clean out of the viewport.
#[gpui::test]
fn a_wide_docked_review_stays_inside_the_window(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.sidebar_visible = true;
            app.sidepane.update(cx, |pane, cx| {
                pane.toggle(cx);
                // The widest a left-edge drag reaches at the reserve clamp.
                pane.set_width(viewport.width - px(PANE_MAX_RESERVE), cx);
            });
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());

    let pane = cx.debug_bounds("side-pane").expect("pane laid out");
    assert!(
        pane.right() <= viewport.width + px(1.),
        "the docked review overflows the window: pane {pane:?}, viewport {viewport:?}"
    );
}

/// Changes and Files are tabs of one right dock: it hugs the window's right
/// edge with no gutter, shows the active tab's panel, and closing a tab
/// closes the panel behind it.
#[gpui::test]
fn the_right_dock_hosts_changes_and_files_as_tabs(cx: &mut gpui::TestAppContext) {
    use super::right_dock::DockTab;
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.sidepane.update(cx, |pane, cx| {
                pane.toggle(cx);
                pane.set_width(px(400.), cx);
            });
        });
    });
    // Let the Review pane's notification reach the app first, as a click would.
    cx.update(|_, cx| app.update(cx, |app, cx| app.open_files_tab(cx)));
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    cx.update(|_, cx| {
        let dock = &app.read(cx).right_dock;
        assert_eq!(dock.tabs(), &[DockTab::Changes, DockTab::Files]);
        assert!(dock.is_active(&DockTab::Files), "the newest tab is shown");
    });
    let dock = cx.debug_bounds("right-dock").expect("dock laid out");
    assert_eq!(dock.right(), viewport.width, "flush with the window edge");
    assert_eq!(dock.size.width, px(400.));
    assert!(
        cx.debug_bounds("side-pane").is_none(),
        "only the active tab's panel is painted"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| app.activate_dock_tab(&DockTab::Changes, cx));
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let pane = cx.debug_bounds("side-pane").expect("Changes is shown");
    let dock = cx.debug_bounds("right-dock").expect("dock laid out");
    assert!(
        pane.left() >= dock.left() && pane.right() <= dock.right(),
        "the panel sits inside the dock: pane {pane:?}, dock {dock:?}"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| app.close_dock_tab(&DockTab::Changes, cx));
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(
            !app.sidepane.read(cx).is_open(),
            "closing the tab closes Review"
        );
        assert_eq!(app.right_dock.tabs(), &[DockTab::Files]);
        assert!(app.right_dock.is_active(&DockTab::Files));
    });
}

/// Full width is the dock's, not Review's: it works on any tab, hiding the dock
/// gives the window back, and the dock returns at its docked width.
#[gpui::test]
fn full_width_applies_to_any_tab_and_hiding_ends_it(cx: &mut gpui::TestAppContext) {
    use super::right_dock::DockTab;
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_files_tab(cx);
            app.sidepane
                .update(cx, |pane, cx| pane.set_width(px(400.), cx));
        });
    });
    draw(cx);
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.right_dock.is_active(&DockTab::Files));
        assert!(!app.sidepane.read(cx).is_open(), "Review was never opened");
    });

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_dock_full_width(cx)));
    draw(cx);
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.dock_full_width(cx), "Files can go full width too");
        assert!(app.sidebar_visible, "the sessions sidebar opens beside it");
    });
    let dock = cx.debug_bounds("right-dock").expect("dock laid out");
    let sidebar_width = cx.update(|_, cx| app.read(cx).sidebar_width);
    assert_eq!(dock.size.width, viewport.width - sidebar_width);

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_dock_collapsed(cx)));
    draw(cx);
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.dock_visible());
        assert!(
            !app.sidepane.read(cx).is_full_width(),
            "hiding ends full width"
        );
        assert_eq!(app.right_dock.tabs(), &[DockTab::Files], "tabs are kept");
    });

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_dock_collapsed(cx)));
    draw(cx);
    let dock = cx.debug_bounds("right-dock").expect("dock is back");
    assert_eq!(dock.size.width, px(400.), "back at the docked width");
}

/// The palette's Show/Hide Side Panel opens Changes on an empty dock, then
/// hides and shows the whole dock while keeping its tabs.
#[gpui::test]
fn side_panel_toggle_opens_then_collapses_the_dock(cx: &mut gpui::TestAppContext) {
    use super::right_dock::DockTab;
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
        });
    });

    for (step, expect_visible) in [(0, true), (1, false), (2, true)] {
        cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_side_panel(cx)));
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        cx.update(|_, cx| {
            let app = app.read(cx);
            assert_eq!(app.dock_visible(), expect_visible, "after toggle {step}");
            assert_eq!(app.right_dock.tabs(), &[DockTab::Changes]);
        });
        assert_eq!(
            cx.debug_bounds("right-dock").is_some(),
            expect_visible,
            "the dock paints only while visible (toggle {step})"
        );
    }
}

/// Full width belongs to the dock, so it outlives any one tab: closing Review
/// while Files is full width keeps it, and closing the last tab gives the
/// window back and restores the docked width.
#[gpui::test]
fn full_width_follows_the_dock_not_a_tab(cx: &mut gpui::TestAppContext) {
    use super::right_dock::DockTab;
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.sidepane
                .update(cx, |pane, cx| pane.set_width(px(400.), cx));
        });
    });
    cx.update(|_, cx| app.update(cx, |app, cx| app.open_changes_tab(false, cx)));
    cx.update(|_, cx| app.update(cx, |app, cx| app.open_files_tab(cx)));
    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_dock_full_width(cx)));
    draw(cx);
    cx.update(|_, cx| assert!(app.read(cx).dock_full_width(cx)));

    cx.update(|_, cx| app.update(cx, |app, cx| app.close_dock_tab(&DockTab::Changes, cx)));
    draw(cx);
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.right_dock.tabs(), &[DockTab::Files]);
        assert!(
            app.dock_full_width(cx),
            "closing Review keeps the dock wide"
        );
    });

    cx.update(|_, cx| app.update(cx, |app, cx| app.close_dock_tab(&DockTab::Files, cx)));
    draw(cx);
    cx.update(|_, cx| {
        let pane = app.read(cx).sidepane.read(cx);
        assert!(
            !pane.is_full_width(),
            "an empty dock cannot stay full width"
        );
        assert_eq!(pane.width(), px(400.), "the docked width is restored");
    });
}

/// A file opened from a full-width Files tab lands in the main-area viewer, so
/// the dock must give the window back or the viewer would open out of sight.
#[gpui::test]
fn opening_a_file_ends_full_width(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_files_tab(cx);
            app.toggle_dock_full_width(cx);
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    cx.update(|_, cx| assert!(app.read(cx).dock_full_width(cx)));

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.open_file_in_viewer(
                std::env::temp_dir().join("orbit-dock-test.txt"),
                "orbit-dock-test.txt".to_string(),
                cx,
            );
        });
    });
    cx.update(|_, cx| {
        assert!(!app.read(cx).dock_full_width(cx));
    });
}

/// "Show Side Panel" while a page hides the dock (Usage) brings the dock back
/// by leaving that page, rather than collapsing a dock nobody can see.
#[gpui::test]
fn show_side_panel_under_usage_leaves_usage(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeKey};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_files_tab(cx);
            app.usage_open = true;
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    cx.update(|_, cx| assert!(!app.read(cx).dock_visible(), "Usage hides the dock"));

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_side_panel(cx)));
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(!app.usage_open, "Show leaves the Usage page");
        assert!(app.dock_visible(), "and the dock is showing, not collapsed");
    });
}
