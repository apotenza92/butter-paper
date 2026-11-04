//! The dismissal contract every application dialog keeps, checked on the
//! dialog as the window paints it: Escape, its Cancel or X button and a press
//! on the dimmed backdrop outside it each close it, and its footer is on
//! screen. Alert prompts (a decision with a commit or destructive action)
//! keep the backdrop press inert instead, so a stray click cannot decide.
//!
//! The window must paint `Root::render_dialog_layer`, as the application's
//! does, or the dialog has no hit targets.

use gpui::{Modifiers, VisualTestContext, point, px};
use std::time::Duration;

/// How a press outside the dialog behaves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Outside {
    /// Closes it, as for every ordinary dialog.
    Closes,
    /// Leaves it open: an alert prompt, or a dialog busy with work it must
    /// not drop.
    Stays,
}

/// Paints, lets the open animation finish and paints again.
pub fn settle(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn open(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        use gpui_component::WindowExt as _;
        window.has_active_dialog(cx)
    })
}

/// Checks the contract for the dialog whose content carries the debug
/// selector `dialog`, dismissed by the button `cancel`. `reopen` opens it
/// again the way the user does, after each dismissal. The dialog must be
/// open on entry; it is closed on return.
pub fn assert_dialog_dismissal(
    cx: &mut VisualTestContext,
    name: &str,
    dialog: &'static str,
    cancel: &'static str,
    outside: Outside,
    mut reopen: impl FnMut(&mut VisualTestContext),
) {
    let mut reopen = |cx: &mut VisualTestContext, after: &str| {
        reopen(cx);
        settle(cx);
        assert!(open(cx), "{name}: reopens after {after}");
    };
    settle(cx);
    assert!(open(cx), "{name}: is open");
    let viewport = cx.update(|window, _| window.viewport_size());
    let content = cx
        .debug_bounds(dialog)
        .unwrap_or_else(|| panic!("{name}: paints its content ({dialog})"));
    let button = cx
        .debug_bounds(cancel)
        .unwrap_or_else(|| panic!("{name}: paints its dismiss button ({cancel})"));
    assert!(
        button.right() <= viewport.width && button.bottom() <= viewport.height,
        "{name}: its dismiss button is inside the window"
    );

    // Escape.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!open(cx), "{name}: Escape closes it");
    reopen(cx, "Escape");

    // The dismiss button, clicked as the user does.
    let button = cx.debug_bounds(cancel).unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    settle(cx);
    assert!(!open(cx), "{name}: its dismiss button closes it");
    reopen(cx, "its dismiss button");

    // A press on the backdrop, beside the dialog and below the title bar.
    let content = cx.debug_bounds(dialog).unwrap_or(content);
    let beside = point(
        (content.left() / 2.).max(px(4.)),
        (content.top() + content.bottom()) / 2.,
    );
    assert!(
        beside.x < content.left(),
        "{name}: leaves backdrop beside it to press"
    );
    cx.simulate_click(beside, Modifiers::none());
    settle(cx);
    match outside {
        Outside::Closes => {
            assert!(!open(cx), "{name}: a press outside it closes it");
        }
        Outside::Stays => {
            assert!(open(cx), "{name}: a press outside it leaves it open");
            cx.simulate_keystrokes("escape");
            settle(cx);
            assert!(!open(cx), "{name}: Escape still closes it after that press");
        }
    }
}
