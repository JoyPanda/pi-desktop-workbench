//! Regression tests for live appearance switching: the active palette is a
//! `Theme` global, and every surface — the app's own view tree and the child
//! entities GPUI renders independently — must pick up the new palette without
//! a window recreation.

use super::*;
use crate::theme::{Theme, ThemeId, ThemeMode};
use gpui::size;

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::OrbitAzure));
        cx.new(OrbitApp::new)
    })
}

/// Switching the global palette repaints the window, including the panels
/// that own their element trees. The observer notifies each of them; a panic
/// here means a child was not reachable when the theme changed.
#[gpui::test]
fn switching_the_palette_repaints_every_surface(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();

    // First paint with the dark palette.
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
        app.clone().into_any_element()
    });

    cx.update(|_, cx| {
        // The settings control ends here; so does a system appearance change.
        let ui = theme::get(cx).ui;
        cx.set_global(Theme::for_id(ThemeId::OrbitAzureLight).with_ui(ui));
        assert_eq!(theme::get(cx).mode, ThemeMode::Light);
    });

    // The same window repaints in the light palette, no recreation.
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
        app.clone().into_any_element()
    });
}

/// The opt-in Operational system is a theme layer over the same shell: both
/// appearances must paint the full workbench without a second widget tree.
#[gpui::test]
fn operational_dark_and_light_paint_the_same_workbench(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    for id in [ThemeId::Operational, ThemeId::OperationalLight] {
        cx.update(|_, cx| {
            let ui = theme::get(cx).ui;
            cx.set_global(Theme::for_id(id).with_ui(ui));
        });
        let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
            app.clone().into_any_element()
        });
    }
}
