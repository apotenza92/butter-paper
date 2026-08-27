use gpui::{
    Anchor, AppContext as _, ClickEvent, Context, EventEmitter, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _, Subscription, Window,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonGroup},
    h_flex,
    input::{InputEvent, InputState, NumberInput},
    popover::Popover,
    v_flex,
};

pub const CAD_VIEW_GROUP_ID: &str = "viewer-cad-view-controls";
pub const CAD_VIEW_PRIMARY_ID: &str = "viewer-cad-view";
pub const CAD_VIEW_SETTINGS_ID: &str = "viewer-cad-view-settings";
pub const CAD_VIEW_POPOVER_ID: &str = "viewer-cad-settings";
pub const CAD_VIEW_LABEL: &str = "CAD View";
pub const CAD_VIEW_SETTINGS_LABEL: &str = "CAD View settings";
pub const CAD_VIEW_DESCRIPTION: &str =
    "Organise drawing sheets. Mousewheel always zooms in CAD View.";
pub const CAD_ORGANISATION_GROUP_ID: &str = "viewer-cad-organisation";
pub const CAD_ORGANISATION_COLUMNS_ID: &str = "viewer-cad-organisation-columns";
pub const CAD_ORGANISATION_ROWS_ID: &str = "viewer-cad-organisation-rows";
pub const PAGES_PER_COLUMN_ID: &str = "viewer-pages-per-column";
pub const PAGES_PER_ROW_ID: &str = "viewer-pages-per-row";

pub const DEFAULT_PAGES_PER_COLUMN: usize = 10;
pub const MIN_PAGES_PER_COLUMN: usize = 1;
pub const MAX_PAGES_PER_COLUMN: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CadViewOrganisation {
    Columns,
    Rows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CadViewControlEvent {
    Activated,
}

/// Retains application-owned CAD View activation and Popover state while the
/// pinned GPUI Component controls own presentation and overlay behavior.
pub struct CadViewControl {
    active: bool,
    disabled: bool,
    settings_open: bool,
    primary_activations: usize,
    organisation: CadViewOrganisation,
    pages_per_column: usize,
    organisation_changes: usize,
    page_count_changes: usize,
    pages_input: gpui::Entity<InputState>,
    popover_focus: gpui::FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl CadViewControl {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pages_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(DEFAULT_PAGES_PER_COLUMN.to_string())
                .step(1.)
                .min(MIN_PAGES_PER_COLUMN as f64)
                .max(MAX_PAGES_PER_COLUMN as f64)
        });
        let _subscriptions = vec![cx.subscribe_in(
            &pages_input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let value = input.read(cx).value();
                let Ok(value) = value.parse::<f64>() else {
                    return;
                };
                let next = clamp_pages_per_column(value);
                if next != this.pages_per_column {
                    this.pages_per_column = next;
                    this.page_count_changes += 1;
                    cx.notify();
                }
            },
        )];

        Self {
            active: false,
            disabled: false,
            settings_open: false,
            primary_activations: 0,
            organisation: CadViewOrganisation::Columns,
            pages_per_column: DEFAULT_PAGES_PER_COLUMN,
            organisation_changes: 0,
            page_count_changes: 0,
            pages_input,
            popover_focus: cx.focus_handle(),
            _subscriptions,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn is_settings_open(&self) -> bool {
        self.settings_open
    }

    pub fn primary_activations(&self) -> usize {
        self.primary_activations
    }

    pub fn organisation(&self) -> CadViewOrganisation {
        self.organisation
    }

    pub fn pages_per_column(&self) -> usize {
        self.pages_per_column
    }

    pub fn organisation_changes(&self) -> usize {
        self.organisation_changes
    }

    pub fn page_count_changes(&self) -> usize {
        self.page_count_changes
    }

    pub fn set_organisation(&mut self, organisation: CadViewOrganisation, cx: &mut Context<Self>) {
        if organisation == self.organisation {
            return;
        }
        self.organisation = organisation;
        self.organisation_changes += 1;
        cx.notify();
    }

    pub fn set_pages_per_column(
        &mut self,
        count: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = clamp_pages_per_column(count);
        if next == self.pages_per_column {
            return;
        }
        self.pages_per_column = next;
        self.page_count_changes += 1;
        self.pages_input.update(cx, |input, cx| {
            input.set_value(next.to_string(), window, cx);
        });
        cx.notify();
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        if disabled {
            self.settings_open = false;
        }
        cx.notify();
    }

    pub fn reset_for_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = false;
        self.settings_open = false;
        self.organisation = CadViewOrganisation::Columns;
        self.pages_per_column = DEFAULT_PAGES_PER_COLUMN;
        self.pages_input.update(cx, |input, cx| {
            input.set_value(DEFAULT_PAGES_PER_COLUMN.to_string(), window, cx);
        });
        cx.notify();
    }
}

pub fn clamp_pages_per_column(count: f64) -> usize {
    if !count.is_finite() {
        return DEFAULT_PAGES_PER_COLUMN;
    }
    (count.round() as isize).clamp(MIN_PAGES_PER_COLUMN as isize, MAX_PAGES_PER_COLUMN as isize)
        as usize
}

impl EventEmitter<CadViewControlEvent> for CadViewControl {}

impl Render for CadViewControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active && !self.disabled;
        let disabled = self.disabled;
        let settings_open = self.settings_open;
        let organisation = self.organisation;
        let pages_input = self.pages_input.clone();
        let popover_focus = self.popover_focus.clone();

        let primary = Button::new(CAD_VIEW_PRIMARY_ID)
            .small()
            .debug_selector(|| CAD_VIEW_PRIMARY_ID.into())
            .accessibility_id(CAD_VIEW_PRIMARY_ID)
            .icon(IconName::LayoutDashboard)
            .tooltip(CAD_VIEW_LABEL)
            .selected(active)
            .toggled(active)
            .disabled(disabled)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                if this.active {
                    return;
                }
                this.active = true;
                this.primary_activations += 1;
                cx.emit(CadViewControlEvent::Activated);
                cx.notify();
            }));

        let settings = Button::new(CAD_VIEW_SETTINGS_ID)
            .small()
            .debug_selector(|| CAD_VIEW_SETTINGS_ID.into())
            .accessibility_id(CAD_VIEW_SETTINGS_ID)
            .icon(IconName::ChevronDown)
            .tooltip(CAD_VIEW_SETTINGS_LABEL)
            .selected(active)
            .disabled(disabled);

        let settings = if disabled {
            settings.into_any_element()
        } else {
            let control = cx.entity().downgrade();
            let control_for_content = cx.entity().downgrade();
            Popover::new("viewer-cad-settings-popover")
                .anchor(Anchor::TopLeft)
                .open(settings_open)
                .track_focus(&popover_focus)
                .on_open_change(move |open, _, cx| {
                    let _ = control.update(cx, |control, cx| {
                        control.settings_open = *open;
                        cx.notify();
                    });
                })
                .trigger(settings)
                .content(move |_, _, cx| {
                    let columns_control = control_for_content.clone();
                    let rows_control = control_for_content.clone();
                    let keyboard_control = control_for_content.clone();
                    let count_id = match organisation {
                        CadViewOrganisation::Columns => PAGES_PER_COLUMN_ID,
                        CadViewOrganisation::Rows => PAGES_PER_ROW_ID,
                    };
                    let count_label = match organisation {
                        CadViewOrganisation::Columns => "Pages/column",
                        CadViewOrganisation::Rows => "Pages/row",
                    };

                    let organisation_group = ButtonGroup::new(CAD_ORGANISATION_GROUP_ID)
                        .small()
                        .outline()
                        .child(
                            Button::new(CAD_ORGANISATION_COLUMNS_ID)
                                .debug_selector(|| CAD_ORGANISATION_COLUMNS_ID.into())
                                .accessibility_id(CAD_ORGANISATION_COLUMNS_ID)
                                .label("Columns")
                                .selected(organisation == CadViewOrganisation::Columns),
                        )
                        .child(
                            Button::new(CAD_ORGANISATION_ROWS_ID)
                                .debug_selector(|| CAD_ORGANISATION_ROWS_ID.into())
                                .accessibility_id(CAD_ORGANISATION_ROWS_ID)
                                .label("Rows")
                                .selected(organisation == CadViewOrganisation::Rows),
                        )
                        .on_click(move |selected, _, cx| {
                            let Some(selected) = selected.first().copied() else {
                                return;
                            };
                            let next = if selected == 0 {
                                CadViewOrganisation::Columns
                            } else {
                                CadViewOrganisation::Rows
                            };
                            let _ = if next == CadViewOrganisation::Columns {
                                columns_control.update(cx, |control, cx| {
                                    control.set_organisation(next, cx);
                                })
                            } else {
                                rows_control.update(cx, |control, cx| {
                                    control.set_organisation(next, cx);
                                })
                            };
                        });

                    v_flex()
                        .id(CAD_VIEW_POPOVER_ID)
                        .debug_selector(|| CAD_VIEW_POPOVER_ID.into())
                        .track_focus(&popover_focus)
                        .on_key_down(move |event: &KeyDownEvent, _, cx| {
                            let modifiers = event.keystroke.modifiers;
                            if modifiers.alt || modifiers.control || modifiers.platform {
                                return;
                            }
                            let next = match event.keystroke.key.as_str() {
                                "left" => Some(match organisation {
                                    CadViewOrganisation::Columns => CadViewOrganisation::Rows,
                                    CadViewOrganisation::Rows => CadViewOrganisation::Columns,
                                }),
                                "right" => Some(match organisation {
                                    CadViewOrganisation::Columns => CadViewOrganisation::Rows,
                                    CadViewOrganisation::Rows => CadViewOrganisation::Columns,
                                }),
                                "home" => Some(CadViewOrganisation::Columns),
                                "end" => Some(CadViewOrganisation::Rows),
                                _ => None,
                            };
                            if let Some(next) = next {
                                let _ = keyboard_control.update(cx, |control, cx| {
                                    control.set_organisation(next, cx);
                                });
                                cx.stop_propagation();
                            }
                        })
                        .w_64()
                        .gap_3()
                        .child(gpui::div().font_semibold().child(CAD_VIEW_LABEL))
                        .child(
                            gpui::div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(CAD_VIEW_DESCRIPTION),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    gpui::div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Organise by"),
                                )
                                .child(organisation_group),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .gap_3()
                                .child(gpui::div().text_sm().child(count_label))
                                .child(
                                    h_flex()
                                        .id(count_id)
                                        .debug_selector(move || count_id.into())
                                        .w_24()
                                        .child(NumberInput::new(&pages_input).small()),
                                ),
                        )
                })
                .into_any_element()
        };

        h_flex()
            .id(CAD_VIEW_GROUP_ID)
            .debug_selector(|| CAD_VIEW_GROUP_ID.into())
            .flex_shrink_0()
            .gap_1()
            .child(primary)
            .child(settings)
    }
}
