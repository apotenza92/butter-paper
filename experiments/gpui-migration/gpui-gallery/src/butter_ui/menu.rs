use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, CursorStyle, IntoElement, MouseDownEvent, ParentElement as _,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::*, px,
};

use super::{ButterTheme, Icon};

type MenuDismissListener = Rc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;
type MenuItemClickListener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PopupMenu {
    id: SharedString,
    label: SharedString,
    children: Vec<AnyElement>,
    note: Option<SharedString>,
    autofocus: bool,
    on_dismiss: Option<MenuDismissListener>,
}

impl PopupMenu {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
            note: None,
            autofocus: true,
            on_dismiss: None,
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn on_dismiss(
        mut self,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for PopupMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = ButterTheme::light();
        let visible_label = self.label.clone();
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        if self.autofocus {
            focus_handle.focus(window, cx);
        }
        div()
            .id(self.id.clone())
            .debug_selector({
                let id = self.id.clone();
                move || id.to_string()
            })
            .role(Role::Menu)
            .aria_label(self.label)
            .track_focus(&focus_handle)
            .w(px(210.0))
            .p_1()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .text_color(theme.foreground)
            .shadow_lg()
            .when_some(self.on_dismiss, |element, listener| {
                element.on_mouse_down_out(move |event, window, cx| listener(event, window, cx))
            })
            .child(
                div()
                    .px_2()
                    .pt_1()
                    .pb(px(2.0))
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child(visible_label),
            )
            .children(self.children)
            .when_some(self.note, |element, note| {
                element.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(note),
                )
            })
    }
}

#[derive(IntoElement)]
pub struct PopupMenuItem {
    id: SharedString,
    label: SharedString,
    selected: bool,
    active: bool,
    disabled: bool,
    on_click: Option<MenuItemClickListener>,
}

impl PopupMenuItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected: false,
            active: false,
            disabled: false,
            on_click: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(listener));
        self
    }
}

impl RenderOnce for PopupMenuItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = ButterTheme::light();
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let disabled = self.disabled;
        let selected = self.selected;
        let active = self.active;
        let on_click = self.on_click;

        div()
            .id(self.id.clone())
            .debug_selector({
                let id = self.id.clone();
                move || id.to_string()
            })
            .role(Role::MenuItem)
            .aria_label(self.label.clone())
            .aria_selected(selected)
            .when(active, |element| element.aria_active_descendant())
            .when(!disabled, |element| {
                element.track_focus(&focus_handle.tab_stop(true))
            })
            .h(px(32.0))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.transparent)
            .bg(if selected || active {
                theme.muted
            } else {
                theme.transparent
            })
            .text_size(px(13.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .child(div().w(px(16.0)).flex_none().when(selected, |element| {
                element.child(Icon::new("check", theme.foreground).size(14.0))
            }))
            .child(self.label)
            .when(!disabled, |element| {
                element
                    .hover(|style| style.bg(theme.muted))
                    .focus_visible(|style| style.bg(theme.muted).border_color(theme.ring))
            })
            .when_some(on_click.filter(|_| !disabled), |element, listener| {
                element.on_click(move |event, window, cx| listener(event, window, cx))
            })
    }
}
