//! Workspace width preferences composed around stock resizable panels.
//!
//! ResizableState scales all children proportionally when its container changes.
//! Sidebars instead retain a preferred width; the canvas absorbs that change.
//! Stock handles still own painting, hit testing and ordinary drag interactions.

use std::{cell::Cell, rc::Rc};

use gpui::{
    App, Bounds, DispatchPhase, Entity, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Pixels, Styled, Window, canvas, div, px,
};
use gpui_component::{ElementExt as _, resizable::ResizableState};

const DEFAULT_WIDTH_REMS: f32 = 18.75;

#[derive(Clone)]
pub(crate) struct SidebarSizing {
    panel_index: usize,
    preferred_rems: Rc<Cell<f32>>,
    layout: Rc<Cell<Option<(Pixels, Pixels, bool)>>>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl SidebarSizing {
    pub(crate) fn new(panel_index: usize) -> Self {
        Self {
            panel_index,
            preferred_rems: Rc::new(Cell::new(DEFAULT_WIDTH_REMS)),
            layout: Rc::new(Cell::new(None)),
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    pub(crate) fn remember_drag(&self, state: &Entity<ResizableState>, window: &Window, cx: &App) {
        if let Some(width) = state.read(cx).sizes().get(self.panel_index) {
            self.preferred_rems.set(*width / window.rem_size());
        }
    }

    pub(crate) fn preferred_width(&self, rem: Pixels) -> Pixels {
        rem * self.preferred_rems.get()
    }

    pub(crate) fn measure(&self) -> impl Fn(Bounds<Pixels>, &mut Window, &mut App) + 'static {
        let bounds = self.bounds.clone();
        move |measured, _, _| bounds.set(measured)
    }

    pub(crate) fn wrap(
        &self,
        group: impl IntoElement,
        state: &Entity<ResizableState>,
        visible: bool,
    ) -> impl IntoElement {
        let layout = self.layout.clone();
        let preference = self.preferred_rems.clone();
        let resize_state = state.clone();
        let index = self.panel_index;
        let reset = self.clone();
        let reset_state = state.clone();
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .on_prepaint(move |bounds, window, cx| {
                let key = (bounds.size.width, window.rem_size(), visible);
                if layout.replace(Some(key)) == Some(key) || !visible {
                    return;
                }
                let state = resize_state.clone();
                let preference = preference.clone();
                let layout = layout.clone();
                // Run after the stock group's prepaint has measured its children.
                // Reconcile container/scale changes, never each drag frame.
                window.defer(cx, move |window, cx| {
                    if layout.get() != Some(key) {
                        return;
                    }
                    state.update(cx, |state, cx| {
                        state.resize_panel(index, window.rem_size() * preference.get(), window, cx);
                    });
                });
            })
            .child(group)
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        if !visible {
                            return;
                        }
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture
                                || event.button != MouseButton::Left
                                || event.click_count != 2
                            {
                                return;
                            }
                            let bounds = reset.bounds.get();
                            let edge = if index == 0 {
                                bounds.right()
                            } else {
                                bounds.left()
                            };
                            // Match the stock divider's four-pixel padding plus one-pixel line.
                            if event.position.x < edge - px(4.)
                                || event.position.x > edge + px(5.)
                                || event.position.y < bounds.top()
                                || event.position.y >= bounds.bottom()
                            {
                                return;
                            }
                            reset.preferred_rems.set(DEFAULT_WIDTH_REMS);
                            reset_state.update(cx, |state, cx| {
                                state.resize_panel(
                                    index,
                                    window.rem_size() * DEFAULT_WIDTH_REMS,
                                    window,
                                    cx,
                                );
                            });
                            window.prevent_default();
                            cx.stop_propagation();
                        });
                    },
                )
                .absolute()
                .size(px(0.)),
            )
    }
}
