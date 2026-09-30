//! Application-wide record of open transient overlays (popovers).
//!
//! A click outside an open overlay only dismisses it. Canvas handlers run
//! before the overlay's own outside-click dismissal, so they consult this
//! record to avoid starting a selection or edit with that same click.

use gpui::{App, Global, SharedString};
use std::collections::HashSet;

#[derive(Default)]
struct OpenOverlays(HashSet<SharedString>);

impl Global for OpenOverlays {}

/// Record that the overlay identified by `id` opened or closed.
pub fn set_overlay_open(id: impl Into<SharedString>, open: bool, cx: &mut App) {
    let id = id.into();
    let overlays = cx.default_global::<OpenOverlays>();
    if open {
        overlays.0.insert(id);
    } else {
        overlays.0.remove(&id);
    }
}

/// Record a controlled overlay's rendered open state and return it, so the
/// record follows programmatic opens and closes as well as user toggles.
pub fn sync_overlay_open(id: impl Into<SharedString>, open: bool, cx: &mut App) -> bool {
    set_overlay_open(id, open, cx);
    open
}

/// Whether any transient overlay is currently open.
pub fn any_overlay_open(cx: &App) -> bool {
    cx.try_global::<OpenOverlays>()
        .is_some_and(|overlays| !overlays.0.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn tracks_open_overlays_by_identity(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert!(!any_overlay_open(cx));
            set_overlay_open("snap", true, cx);
            set_overlay_open("zoom", true, cx);
            set_overlay_open("snap", false, cx);
            assert!(any_overlay_open(cx));
            set_overlay_open("zoom", false, cx);
            assert!(!any_overlay_open(cx));
            set_overlay_open("never-opened", false, cx);
            assert!(!any_overlay_open(cx));

            assert!(sync_overlay_open("rendered", true, cx));
            assert!(any_overlay_open(cx));
            assert!(!sync_overlay_open("rendered", false, cx));
            assert!(!any_overlay_open(cx));
        });
    }
}
