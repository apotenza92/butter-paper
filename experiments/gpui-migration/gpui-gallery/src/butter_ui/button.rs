use std::rc::Rc;

use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, ColorExt as _, CursorStyle, FocusHandle, Hsla,
    IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Toggled, Window, div, prelude::*, px,
};

use super::{ButterTheme, Icon, Tooltip};

type ButtonClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonVariant {
    #[default]
    Default,
    Outline,
    Secondary,
    Ghost,
    Destructive,
    Link,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonSize {
    XSmall,
    Small,
    #[default]
    Default,
    Large,
    Icon,
    IconXSmall,
    IconSmall,
    IconLarge,
}

impl ButtonSize {
    pub fn height(self) -> f32 {
        match self {
            Self::XSmall | Self::IconXSmall => 24.0,
            Self::Small | Self::IconSmall => 28.0,
            Self::Default | Self::Icon => 32.0,
            Self::Large | Self::IconLarge => 36.0,
        }
    }

    fn icon_size(self) -> f32 {
        match self {
            Self::XSmall | Self::IconXSmall => 12.0,
            Self::Small | Self::IconSmall => 14.0,
            _ => 16.0,
        }
    }

    fn horizontal_padding(self) -> f32 {
        match self {
            Self::XSmall => 8.0,
            Self::Small | Self::Default | Self::Large => 10.0,
            Self::Icon | Self::IconXSmall | Self::IconSmall | Self::IconLarge => 0.0,
        }
    }

    fn is_icon_only(self) -> bool {
        matches!(
            self,
            Self::Icon | Self::IconXSmall | Self::IconSmall | Self::IconLarge
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonGroupPosition {
    #[default]
    Only,
    First,
    Middle,
    Last,
}

#[derive(IntoElement)]
pub struct Button {
    id: SharedString,
    accessible_label: SharedString,
    label: Option<SharedString>,
    leading_icon_name: Option<SharedString>,
    trailing_icon_name: Option<SharedString>,
    theme: ButterTheme,
    variant: ButtonVariant,
    size: ButtonSize,
    group_position: ButtonGroupPosition,
    disabled: bool,
    toggled: Option<bool>,
    expanded: Option<bool>,
    tooltip: Option<SharedString>,
    on_click: Option<ButtonClickListener>,
}

impl Button {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            accessible_label: label.clone(),
            label: Some(label),
            leading_icon_name: None,
            trailing_icon_name: None,
            theme: ButterTheme::light(),
            variant: ButtonVariant::Default,
            size: ButtonSize::Default,
            group_position: ButtonGroupPosition::Only,
            disabled: false,
            toggled: None,
            expanded: None,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn icon(
        id: impl Into<SharedString>,
        icon_name: impl Into<SharedString>,
        accessible_label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            accessible_label: accessible_label.into(),
            label: None,
            leading_icon_name: Some(icon_name.into()),
            trailing_icon_name: None,
            theme: ButterTheme::light(),
            variant: ButtonVariant::Ghost,
            size: ButtonSize::Icon,
            group_position: ButtonGroupPosition::Only,
            disabled: false,
            toggled: None,
            expanded: None,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn theme(mut self, theme: ButterTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn leading_icon(mut self, icon_name: impl Into<SharedString>) -> Self {
        self.leading_icon_name = Some(icon_name.into());
        self
    }

    pub fn trailing_icon(mut self, icon_name: impl Into<SharedString>) -> Self {
        self.trailing_icon_name = Some(icon_name.into());
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn group_position(mut self, position: ButtonGroupPosition) -> Self {
        self.group_position = position;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn toggled(mut self, toggled: bool) -> Self {
        self.toggled = Some(toggled);
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub(crate) fn focus_handle(&self, window: &mut Window, cx: &mut App) -> FocusHandle {
        window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone()
    }

    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(listener));
        self
    }

    fn colors(&self) -> ButtonColors {
        let theme = self.theme;
        match self.variant {
            ButtonVariant::Default => ButtonColors {
                background: theme.primary,
                foreground: theme.primary_foreground,
                border: theme.transparent,
                hover_background: theme.primary.opacity(0.8),
            },
            ButtonVariant::Outline => ButtonColors {
                background: theme.background,
                foreground: theme.foreground,
                border: theme.border,
                hover_background: theme.muted,
            },
            ButtonVariant::Secondary => ButtonColors {
                background: theme.secondary,
                foreground: theme.secondary_foreground,
                border: theme.transparent,
                hover_background: theme.foreground.opacity(0.08),
            },
            ButtonVariant::Ghost => ButtonColors {
                background: theme.transparent,
                foreground: theme.foreground,
                border: theme.transparent,
                hover_background: theme.muted,
            },
            ButtonVariant::Destructive => ButtonColors {
                background: theme.destructive.opacity(0.1),
                foreground: theme.destructive,
                border: theme.transparent,
                hover_background: theme.destructive.opacity(0.2),
            },
            ButtonVariant::Link => ButtonColors {
                background: theme.transparent,
                foreground: theme.primary,
                border: theme.transparent,
                hover_background: theme.transparent,
            },
        }
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self.focus_handle(window, cx);
        let colors = self.colors();
        let height = self.size.height();
        let icon_size = self.size.icon_size();
        let icon_only = self.size.is_icon_only();
        let disabled = self.disabled;
        let on_click = self.on_click;
        let radius = px(10.0);
        let theme = self.theme;
        let link = self.variant == ButtonVariant::Link;
        let tooltip = self.tooltip;

        let content: AnyElement = div()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .when_some(self.leading_icon_name, |element, icon_name| {
                element.child(Icon::new(icon_name, colors.foreground).size(icon_size))
            })
            .when_some(self.label, |element, label| element.child(label))
            .when_some(self.trailing_icon_name, |element, icon_name| {
                element.child(Icon::new(icon_name, colors.foreground).size(icon_size))
            })
            .into_any_element();

        div()
            .id(self.id.clone())
            .debug_selector({
                let id = self.id.clone();
                move || id.to_string()
            })
            .role(Role::Button)
            .aria_label(self.accessible_label)
            .when_some(self.toggled, |element, toggled| {
                element.aria_toggled(if toggled {
                    Toggled::True
                } else {
                    Toggled::False
                })
            })
            .when_some(self.expanded, |element, expanded| {
                element.aria_expanded(expanded)
            })
            .when(!disabled, |element| {
                element.track_focus(&focus_handle.tab_stop(true))
            })
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(height))
            .when(icon_only, |element| element.w(px(height)))
            .when(!icon_only, |element| {
                element.px(px(self.size.horizontal_padding()))
            })
            .gap(px(6.0))
            .border_1()
            .border_color(colors.border)
            .bg(if self.toggled == Some(true) {
                theme.muted
            } else {
                colors.background
            })
            .text_color(colors.foreground)
            .text_size(px(match self.size {
                ButtonSize::XSmall | ButtonSize::IconXSmall => 12.0,
                ButtonSize::Small | ButtonSize::IconSmall => 12.8,
                _ => 14.0,
            }))
            .font_weight(gpui::FontWeight::MEDIUM)
            .whitespace_nowrap()
            .opacity(if disabled { 0.5 } else { 1.0 })
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .when(
                matches!(
                    self.group_position,
                    ButtonGroupPosition::Only | ButtonGroupPosition::First
                ),
                |element| element.rounded_l(radius),
            )
            .when(
                matches!(
                    self.group_position,
                    ButtonGroupPosition::Only | ButtonGroupPosition::Last
                ),
                |element| element.rounded_r(radius),
            )
            .when(
                matches!(
                    self.group_position,
                    ButtonGroupPosition::Middle | ButtonGroupPosition::Last
                ),
                |element| element.border_l_0(),
            )
            .when(!disabled, |element| {
                element
                    .hover(|style| {
                        let style = style
                            .bg(if self.toggled == Some(true) {
                                theme.muted
                            } else {
                                colors.hover_background
                            })
                            .text_color(colors.foreground);
                        if link {
                            style.text_decoration_1()
                        } else {
                            style
                        }
                    })
                    .active(|style| style.relative().top(px(1.0)))
                    .focus_visible(|style| {
                        style.border_color(theme.ring).shadow(vec![
                            BoxShadow::new(px(0.0), px(0.0), theme.ring.opacity(0.5))
                                .spread_radius(px(3.0)),
                        ])
                    })
            })
            .when_some(on_click.filter(|_| !disabled), |element, listener| {
                element.on_click(move |event, window, cx| listener(event, window, cx))
            })
            .when_some(tooltip, |element, tooltip| {
                element.tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
            })
            .child(content)
    }
}

#[derive(Clone, Copy)]
struct ButtonColors {
    background: Hsla,
    foreground: Hsla,
    border: Hsla,
    hover_background: Hsla,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Context, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, Render, TestAppContext, accesskit,
        canvas, size,
    };
    use std::sync::{Arc, Mutex};

    struct ButtonHarness {
        clicks: usize,
    }

    impl Render for ButtonHarness {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("enabled-button", "Enabled").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.clicks += 1;
                            cx.notify();
                        },
                    )),
                )
                .child(
                    Button::new("disabled-button", "Disabled")
                        .disabled(true)
                        .on_click(cx.listener(|this, _, _, _| this.clicks += 100)),
                )
                .child(
                    Button::icon("rectangle-tool", "draw-square", "Rectangle")
                        .tooltip("Rectangle (R)"),
                )
                .child(Button::new("link-button", "Link").variant(ButtonVariant::Link))
        }
    }

    #[test]
    fn nova_button_sizes_are_stable() {
        assert_eq!(ButtonSize::XSmall.height(), 24.0);
        assert_eq!(ButtonSize::Small.height(), 28.0);
        assert_eq!(ButtonSize::Default.height(), 32.0);
        assert_eq!(ButtonSize::Large.height(), 36.0);
        assert_eq!(ButtonSize::Icon.height(), 32.0);
    }

    #[test]
    fn toggles_reuse_the_muted_nova_surface() {
        let button = Button::icon("tool", "mouse-pointer-2", "Select").toggled(true);
        assert_eq!(button.toggled, Some(true));
        assert_eq!(button.variant, ButtonVariant::Ghost);
        assert_eq!(button.theme.muted, ButterTheme::light().muted);
        assert_eq!(button.colors().border, ButterTheme::light().transparent);
    }

    #[gpui::test]
    fn disabled_buttons_do_not_dispatch_clicks(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| ButtonHarness { clicks: 0 });
        cx.simulate_resize(size(px(320.0), px(120.0)));
        cx.run_until_parked();

        let enabled = cx
            .debug_bounds("enabled-button")
            .expect("enabled button should render");
        cx.simulate_click(enabled.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| view.read(cx).clicks), 1);

        let disabled = cx
            .debug_bounds("disabled-button")
            .expect("disabled button should render");
        cx.simulate_click(disabled.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| view.read(cx).clicks), 1);
    }

    #[gpui::test]
    fn enter_and_space_dispatch_native_keyboard_clicks(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| ButtonHarness { clicks: 0 });
        cx.simulate_resize(size(px(320.0), px(120.0)));
        cx.run_until_parked();

        let enabled = cx
            .debug_bounds("enabled-button")
            .expect("enabled button should render");
        cx.simulate_click(enabled.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|_, cx| view.update(cx, |this, _| this.clicks = 0));

        for key in ["enter", "space"] {
            let keystroke = Keystroke::parse(key).expect("activation key should parse");
            cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(KeyUpEvent { keystroke });
        }

        assert_eq!(cx.update(|_, cx| view.read(cx).clicks), 2);
    }

    #[gpui::test]
    fn enabled_button_exposes_accessible_click_action(cx: &mut TestAppContext) {
        type Captured = Arc<Mutex<Option<accesskit::Node>>>;

        struct A11yProbe {
            captured: Captured,
        }

        impl Render for A11yProbe {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let captured = self.captured.clone();
                canvas(
                    move |_, window, cx| {
                        let mut node = accesskit::Node::new(Role::Button);
                        Button::new("a11y-button", "Accessible button")
                            .on_click(|_, _, _| {})
                            .render(window, cx)
                            .into_element()
                            .write_a11y_info(&mut node);
                        *captured.lock().expect("capture lock should be available") = Some(node);
                    },
                    |_, _, _, _| {},
                )
            }
        }

        let captured: Captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| A11yProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let node = result
            .lock()
            .expect("capture lock should be available")
            .take()
            .expect("accessible node should be captured");

        assert_eq!(node.role(), Role::Button);
        assert_eq!(node.label(), Some("Accessible button"));
        assert!(node.supports_action(accesskit::Action::Click));
    }

    #[gpui::test]
    fn icon_button_tooltip_appears_after_the_native_delay(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| ButtonHarness { clicks: 0 });
        cx.simulate_resize(size(px(320.0), px(120.0)));
        cx.run_until_parked();

        let trigger = cx
            .debug_bounds("rectangle-tool")
            .expect("rectangle tool should render");
        cx.simulate_mouse_move(trigger.center(), None, Modifiers::none());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();

        assert!(cx.debug_bounds("tooltip-Rectangle (R)").is_some());
    }
}
