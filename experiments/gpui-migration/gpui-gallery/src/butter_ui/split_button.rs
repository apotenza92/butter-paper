use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, IntoElement, KeyDownEvent, ParentElement as _, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div,
    point, prelude::*, px,
};

use super::{Button, ButtonGroupPosition};

type PopupKeyListener = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct SplitButton {
    id: SharedString,
    label: SharedString,
    primary: Button,
    menu_trigger: Button,
    popup: Option<AnyElement>,
    on_popup_key: Option<PopupKeyListener>,
}

impl SplitButton {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        primary: Button,
        menu_trigger: Button,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            primary,
            menu_trigger,
            popup: None,
            on_popup_key: None,
        }
    }

    pub fn popup(mut self, popup: impl IntoElement) -> Self {
        self.popup = Some(popup.into_any_element());
        self
    }

    pub fn on_popup_key(
        mut self,
        listener: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_popup_key = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for SplitButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let popup_open = self.popup.is_some();
        let trigger_focus = self.menu_trigger.focus_handle(window, cx);
        div()
            .id(self.id.clone())
            .debug_selector({
                let id = self.id.clone();
                move || id.to_string()
            })
            .role(Role::Group)
            .aria_label(self.label)
            .relative()
            .flex()
            .flex_none()
            .items_stretch()
            .child(self.primary.group_position(ButtonGroupPosition::First))
            .child(
                self.menu_trigger
                    .group_position(ButtonGroupPosition::Last)
                    .expanded(popup_open),
            )
            .when_some(
                self.on_popup_key.filter(|_| popup_open),
                |element, listener| {
                    element.on_key_down(move |event, window, cx| {
                        listener(event, window, cx);
                        if matches!(event.keystroke.key.as_str(), "escape" | "enter") {
                            trigger_focus.focus(window, cx);
                        }
                    })
                },
            )
            .when_some(self.popup, |element, popup| {
                element.child(
                    deferred(
                        anchored()
                            .anchor(Anchor::TopLeft)
                            .offset(point(px(0.0), px(36.0)))
                            .snap_to_window_with_margin(px(8.0))
                            .child(popup),
                    )
                    .priority(100),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::butter_ui::{ButtonVariant, PopupMenu, PopupMenuItem};
    use gpui::{Context, Modifiers, Render, TestAppContext, size};

    struct SplitButtonHarness {
        primary_clicks: usize,
        menu_open: bool,
        selected_option: bool,
        keyboard_activations: usize,
    }

    impl Render for SplitButtonHarness {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let split = SplitButton::new(
                "tested-split",
                "Test split control",
                Button::icon("tested-primary", "file-plus", "Create")
                    .variant(ButtonVariant::Outline)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.primary_clicks += 1;
                        cx.notify();
                    })),
                Button::icon("tested-menu-trigger", "chevron-down", "Choose")
                    .variant(ButtonVariant::Outline)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.menu_open = !this.menu_open;
                        cx.notify();
                    })),
            )
            .on_popup_key(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.menu_open = false;
                    cx.stop_propagation();
                    cx.notify();
                } else if event.keystroke.key == "down" {
                    this.keyboard_activations += 1;
                    cx.stop_propagation();
                    cx.notify();
                } else if event.keystroke.key == "enter" {
                    this.keyboard_activations += 1;
                    this.menu_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }
            }));
            div().p_4().flex().child(if self.menu_open {
                split.popup(
                    PopupMenu::new("tested-menu", "Choices")
                        .child(
                            PopupMenuItem::new("tested-option", "Option").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.selected_option = true;
                                    this.menu_open = false;
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            PopupMenuItem::new("tested-disabled-option", "Unavailable")
                                .disabled(true)
                                .on_click(cx.listener(|this, _, _, _| {
                                    this.primary_clicks += 100;
                                })),
                        ),
                )
            } else {
                split
            })
        }
    }

    #[gpui::test]
    fn primary_and_menu_segments_dispatch_independently(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| SplitButtonHarness {
            primary_clicks: 0,
            menu_open: false,
            selected_option: false,
            keyboard_activations: 0,
        });
        cx.simulate_resize(size(px(360.0), px(240.0)));
        cx.run_until_parked();

        let primary = cx
            .debug_bounds("tested-primary")
            .expect("primary split segment should render");
        let split = cx
            .debug_bounds("tested-split")
            .expect("split control frame should render");
        assert!(split.contains(&primary.origin));
        cx.simulate_click(primary.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| view.read(cx).primary_clicks), 1);
        assert!(!cx.update(|_, cx| view.read(cx).menu_open));

        let menu_trigger = cx
            .debug_bounds("tested-menu-trigger")
            .expect("menu split segment should render");
        assert_eq!(primary.origin.y, menu_trigger.origin.y);
        assert_eq!(primary.size.height, menu_trigger.size.height);
        assert_eq!(primary.origin.x + primary.size.width, menu_trigger.origin.x);
        assert_eq!(split.origin, primary.origin);
        assert_eq!(
            split.size.width,
            primary.size.width + menu_trigger.size.width
        );
        cx.simulate_click(menu_trigger.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| view.read(cx).menu_open));
        assert!(cx.debug_bounds("tested-menu").is_some());

        cx.simulate_keystrokes("escape");
        assert!(!cx.update(|_, cx| view.read(cx).menu_open));
        assert!(cx.debug_bounds("tested-menu").is_none());

        let menu_trigger = cx
            .debug_bounds("tested-menu-trigger")
            .expect("menu split segment should still render");
        cx.simulate_click(menu_trigger.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| view.read(cx).menu_open));

        cx.simulate_keystrokes("down enter");
        assert_eq!(cx.update(|_, cx| view.read(cx).keyboard_activations), 2);
        assert!(!cx.update(|_, cx| view.read(cx).menu_open));

        let menu_trigger = cx
            .debug_bounds("tested-menu-trigger")
            .expect("menu split segment should still render after keyboard activation");
        cx.simulate_click(menu_trigger.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| view.read(cx).menu_open));

        let disabled_option = cx
            .debug_bounds("tested-disabled-option")
            .expect("disabled popup option should render");
        cx.simulate_click(disabled_option.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| view.read(cx).primary_clicks), 1);
        assert!(cx.update(|_, cx| view.read(cx).menu_open));

        let option = cx
            .debug_bounds("tested-option")
            .expect("popup option should render");
        cx.simulate_click(option.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| view.read(cx).selected_option));
        assert!(!cx.update(|_, cx| view.read(cx).menu_open));
    }
}
