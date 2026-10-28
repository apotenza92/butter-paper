//! Application-wide record of open transient overlays (popovers).
//!
//! A click outside an open overlay only dismisses it. Canvas handlers run
//! before the overlay's own outside-click dismissal, so they consult this
//! record to avoid starting a selection or edit with that same click.
//!
//! An overlay is recorded by the view that draws it, as it draws it. A view
//! starts each draw with [`begin_overlay_render`], and only overlays it
//! records in that latest draw count, while the view still exists. An overlay
//! that stops being drawn while open (its tab closed, its branch no longer
//! taken, its window gone) therefore stops counting, instead of leaving every
//! canvas press ignored until the app restarts.

use gpui::{AnyWeakEntity, App, Context, EntityId, Global, SharedString, Window};
use gpui_component::{GlobalState, Root, WindowExt as _};
use std::collections::HashMap;

struct OwnedOverlay {
    owner: AnyWeakEntity,
    /// The owner's draw that recorded it.
    generation: u64,
}

#[derive(Default)]
struct OpenOverlays {
    /// Overlays recorded without an owning view, which only an explicit
    /// close clears.
    unowned: HashMap<SharedString, ()>,
    owned: HashMap<(EntityId, SharedString), OwnedOverlay>,
    /// Each owning view's current draw.
    generations: HashMap<EntityId, u64>,
}

impl Global for OpenOverlays {}

/// Starts a new draw of the view that owns overlays: only overlays it
/// records from now on count as open. Call it first in the view's `render`.
pub fn begin_overlay_render<T: 'static>(cx: &mut Context<T>) {
    let owner = cx.entity_id();
    let overlays = cx.default_global::<OpenOverlays>();
    *overlays.generations.entry(owner).or_default() += 1;
    overlays.owned.retain(|(id, _), _| *id != owner);
}

/// Record that the overlay identified by `id`, owned by no view, opened or
/// closed.
pub fn set_overlay_open(id: impl Into<SharedString>, open: bool, cx: &mut App) {
    let id = id.into();
    let overlays = cx.default_global::<OpenOverlays>();
    if open {
        overlays.unowned.insert(id, ());
    } else {
        overlays.unowned.remove(&id);
    }
}

/// Record a controlled overlay's open state as its view draws it, and return
/// it, so the record follows programmatic opens and closes as well as user
/// toggles.
pub fn sync_overlay_open<T: 'static>(
    id: impl Into<SharedString>,
    open: bool,
    cx: &mut Context<T>,
) -> bool {
    let key = (cx.entity_id(), id.into());
    let owner = cx.weak_entity().into();
    let overlays = cx.default_global::<OpenOverlays>();
    if open {
        let generation = *overlays.generations.entry(key.0).or_default();
        overlays
            .owned
            .insert(key, OwnedOverlay { owner, generation });
    } else {
        overlays.owned.remove(&key);
    }
    open
}

/// Whether any transient overlay is currently open.
pub fn any_overlay_open(cx: &App) -> bool {
    let Some(overlays) = cx.try_global::<OpenOverlays>() else {
        return false;
    };
    !overlays.unowned.is_empty()
        || overlays.owned.iter().any(|((owner_id, _), overlay)| {
            overlays.generations.get(owner_id) == Some(&overlay.generation)
                && overlay.owner.upgrade().is_some()
        })
}

/// Whether a press belongs to an open popup or dialog rather than to the
/// document: our own overlays, gpui-component popovers, selects and context
/// menus, and dialogs or sheets. Such a press only acts inside the popup or
/// dismisses it, so it must never also start a selection, edit or tab drag.
pub fn press_owned_by_overlay(window: &mut Window, cx: &mut App) -> bool {
    if any_overlay_open(cx)
        || (cx.has_global::<GlobalState>() && GlobalState::is_in_deferred_context(cx))
    {
        return true;
    }
    matches!(window.root::<Root>(), Some(Some(_)))
        && (window.has_active_dialog(cx) || window.has_active_sheet(cx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, Render};

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
        });
    }

    struct Owner;
    impl Render for Owner {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            gpui::Empty
        }
    }

    #[gpui::test]
    fn an_overlay_counts_only_while_its_owner_still_draws_it(cx: &mut gpui::TestAppContext) {
        let owner = cx.update(|cx| cx.new(|_| Owner));
        cx.update(|cx| {
            owner.update(cx, |_, cx| {
                begin_overlay_render(cx);
                assert!(sync_overlay_open("prompt", true, cx));
            });
            assert!(any_overlay_open(cx));
            // The next draw no longer draws it (its tab closed): it stops
            // counting without ever being drawn closed.
            owner.update(cx, |_, cx| begin_overlay_render(cx));
            assert!(!any_overlay_open(cx));
            owner.update(cx, |_, cx| {
                begin_overlay_render(cx);
                sync_overlay_open("prompt", true, cx);
                assert!(!sync_overlay_open("prompt", false, cx));
            });
            assert!(!any_overlay_open(cx));
            owner.update(cx, |_, cx| {
                begin_overlay_render(cx);
                sync_overlay_open("prompt", true, cx);
            });
        });
        // A view that goes away takes its overlays with it.
        drop(owner);
        cx.run_until_parked();
        cx.update(|cx| assert!(!any_overlay_open(cx)));
    }
}
