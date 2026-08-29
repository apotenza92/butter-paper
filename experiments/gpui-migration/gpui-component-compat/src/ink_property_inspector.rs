//! Retained presentation state for one selected Pen or Highlight.
//!
//! The inspector owns only GPUI Component entities and disclosure state. The
//! workspace owns annotation identity, validation, persistence, and history.

use butter_paper_gpui_gallery::annotation_model::{BlendMode, InkTool, MarkupId, PenAppearance};
use gpui::{
    AnyElement, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, StyledExt as _,
    accordion::Accordion,
    button::Button,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    form::Field,
    h_flex,
    input::{InputEvent, InputState, NumberInput},
    scroll::ScrollableElement as _,
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
    try_parse_color, v_flex,
};

use crate::document_workspace::DocumentId;

pub const INK_PROPERTY_INSPECTOR_ID: &str = "ink-property-inspector";
pub const INK_INSPECTOR_HEADER_ID: &str = "ink-property-inspector-header";
pub const INK_INSPECTOR_SCROLL_ID: &str = "ink-property-inspector-scroll";
pub const INK_INSPECTOR_ACCORDION_ID: &str = "ink-property-inspector-sections";
pub const INK_INSPECTOR_DETAILS_ID: &str = "ink-property-inspector-details";
pub const INK_INSPECTOR_APPEARANCE_ID: &str = "ink-property-inspector-appearance";
pub const INK_INSPECTOR_LOCKED_ID: &str = "ink-property-inspector-locked";
pub const INK_INSPECTOR_COLOR_ID: &str = "ink-property-inspector-color";
pub const INK_INSPECTOR_COLOR_TRIGGER_ID: &str = "ink-property-inspector-color-trigger";
pub const INK_INSPECTOR_APPLY_COLOR_ID: &str = "ink-property-inspector-apply-color";
pub const INK_INSPECTOR_WIDTH_ID: &str = "ink-property-inspector-width";
pub const INK_INSPECTOR_OPACITY_ID: &str = "ink-property-inspector-opacity";
pub const INK_INSPECTOR_OPACITY_TRACK_ID: &str = "ink-property-inspector-opacity-track";
pub const INK_INSPECTOR_WIDTH_PX: f32 = 300.;

#[derive(Clone, Debug, PartialEq)]
pub struct InkPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub tool: InkTool,
    pub appearance: PenAppearance,
    pub smooth_curves: bool,
    pub blend_mode: BlendMode,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InkPropertyPatch {
    Locked(bool),
    Appearance(PenAppearance),
    WidthPt(f64),
    Opacity(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct InkPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub expected_tool: InkTool,
    pub patch: InkPropertyPatch,
}

pub struct InkPropertyInspector {
    snapshot: Option<InkPropertySnapshot>,
    syncing: bool,
    open: bool,
    open_sections: [bool; 2],
    width: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl InkPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.01)
                .step(0.25)
        });
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("the built-in Pen color must parse"),
            )
        });
        let subscriptions = vec![
            cx.subscribe_in(&width, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f64>() else {
                    return;
                };
                if value.is_finite() && value > 0. {
                    this.emit_patch(InkPropertyPatch::WidthPt(value), cx);
                }
            }),
            cx.subscribe(&opacity, |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) if !this.syncing => cx.notify(),
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => {
                    this.emit_patch(InkPropertyPatch::Opacity(value.start() as f64 / 100.), cx)
                }
            }),
            cx.subscribe(&color, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(_) = event;
                if !this.syncing {
                    cx.notify();
                }
            }),
        ];
        Self {
            snapshot: None,
            syncing: false,
            open: false,
            open_sections: [true, true],
            width,
            opacity,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&InkPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open == open {
            return;
        }
        self.open = open;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.take().is_some() {
            self.open = false;
            cx.notify();
        }
    }

    pub fn sync(
        &mut self,
        snapshot: InkPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.width.update(cx, |input, cx| {
            input.set_value(format_number(snapshot.appearance.width_pt()), window, cx);
        });
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((snapshot.appearance.opacity() * 100.) as f32, window, cx);
        });
        if let Ok(mut color) = try_parse_color(snapshot.appearance.color()) {
            color.a = snapshot.appearance.opacity() as f32;
            self.color
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }

    pub fn width_input(&self) -> gpui::Entity<InputState> {
        self.width.clone()
    }

    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.color.clone()
    }

    pub fn opacity_slider(&self) -> gpui::Entity<SliderState> {
        self.opacity.clone()
    }

    fn apply_preview_color(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let Some(color) = self.color.read(cx).value() else {
            return;
        };
        let Ok(appearance) = PenAppearance::new(
            rgb_hex(color),
            snapshot.appearance.width_pt(),
            color.a as f64,
        ) else {
            return;
        };
        self.emit_patch(InkPropertyPatch::Appearance(appearance), cx);
    }

    fn emit_patch(&mut self, patch: InkPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, InkPropertyPatch::Locked(_)))
            || patch_matches_snapshot(&patch, snapshot)
        {
            return;
        }
        cx.emit(InkPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            expected_tool: snapshot.tool,
            patch,
        });
    }
}

impl EventEmitter<InkPropertyEvent> for InkPropertyInspector {}

impl Render for InkPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open) else {
            return div()
                .id(INK_PROPERTY_INSPECTOR_ID)
                .debug_selector(|| INK_PROPERTY_INSPECTOR_ID.into())
                .hidden()
                .into_any_element();
        };
        let mutation_disabled = snapshot.mutation_disabled;
        let property_disabled = mutation_disabled || snapshot.locked;
        let lock_control = cx.entity().downgrade();
        let section_control = cx.entity().downgrade();
        let open_sections = self.open_sections;
        let opacity_value = self.opacity.read(cx).value().start();

        let details = v_flex()
            .id(INK_INSPECTOR_DETAILS_ID)
            .debug_selector(|| INK_INSPECTOR_DETAILS_ID.into())
            .gap_3()
            .child(
                Field::new().label("Locked").child(
                    div()
                        .id(INK_INSPECTOR_LOCKED_ID)
                        .debug_selector(|| INK_INSPECTOR_LOCKED_ID.into())
                        .child(
                            Switch::new("ink-property-inspector-locked-switch")
                                .label("Locked")
                                .checked(snapshot.locked)
                                .disabled(mutation_disabled)
                                .on_click(move |locked, _, cx| {
                                    let _ = lock_control.update(cx, |inspector, cx| {
                                        inspector.emit_patch(InkPropertyPatch::Locked(*locked), cx);
                                    });
                                }),
                        ),
                ),
            );
        let appearance = v_flex()
            .id(INK_INSPECTOR_APPEARANCE_ID)
            .debug_selector(|| INK_INSPECTOR_APPEARANCE_ID.into())
            .gap_3()
            .child(color_field(
                &self.color,
                snapshot.appearance.color(),
                snapshot.appearance.opacity(),
                property_disabled,
                cx.entity().downgrade(),
            ))
            .child(
                Field::new().label("Line Width").child(
                    div()
                        .id(INK_INSPECTOR_WIDTH_ID)
                        .debug_selector(|| INK_INSPECTOR_WIDTH_ID.into())
                        .child(
                            NumberInput::new(&self.width)
                                .suffix("pt")
                                .disabled(property_disabled),
                        ),
                ),
            )
            .child(
                Field::new().label("Opacity").child(
                    h_flex()
                        .id(INK_INSPECTOR_OPACITY_ID)
                        .debug_selector(|| INK_INSPECTOR_OPACITY_ID.into())
                        .gap_2()
                        .child(
                            div()
                                .id(INK_INSPECTOR_OPACITY_TRACK_ID)
                                .debug_selector(|| INK_INSPECTOR_OPACITY_TRACK_ID.into())
                                .flex_1()
                                .child(Slider::new(&self.opacity).disabled(property_disabled)),
                        )
                        .child(
                            div()
                                .w(px(44.))
                                .text_right()
                                .text_sm()
                                .child(format!("{opacity_value:.0}%")),
                        ),
                ),
            );
        let accordion = Accordion::new(INK_INSPECTOR_ACCORDION_ID)
            .multiple(true)
            .bordered(false)
            .item(|item| item.open(open_sections[0]).title("Details").child(details))
            .item(|item| {
                item.open(open_sections[1])
                    .title("Appearance")
                    .child(appearance)
            })
            .on_toggle_click(move |open, _, cx| {
                let _ = section_control.update(cx, |inspector, cx| {
                    inspector.open_sections = [open.contains(&0), open.contains(&1)];
                    cx.notify();
                });
            });
        v_flex()
            .id(INK_PROPERTY_INSPECTOR_ID)
            .debug_selector(|| INK_PROPERTY_INSPECTOR_ID.into())
            .w(px(INK_INSPECTOR_WIDTH_PX))
            .h_full()
            .min_h_0()
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .id(INK_INSPECTOR_HEADER_ID)
                    .debug_selector(|| INK_INSPECTOR_HEADER_ID.into())
                    .px_3()
                    .py_2()
                    .font_semibold()
                    .child(match snapshot.tool {
                        InkTool::Pen => "Pen",
                        InkTool::Highlight => "Highlight",
                    }),
            )
            .child(
                div()
                    .id(INK_INSPECTOR_SCROLL_ID)
                    .debug_selector(|| INK_INSPECTOR_SCROLL_ID.into())
                    .h_full()
                    .min_h_0()
                    .p_3()
                    .overflow_y_scrollbar()
                    .child(accordion),
            )
            .into_any_element()
    }
}

fn color_field(
    state: &gpui::Entity<ColorPickerState>,
    canonical_color: &str,
    canonical_opacity: f64,
    disabled: bool,
    inspector: gpui::WeakEntity<InkPropertyInspector>,
) -> AnyElement {
    let control = if disabled {
        div()
            .w_full()
            .px_2()
            .py_1()
            .border_1()
            .opacity(0.5)
            .child(format!(
                "{} · {:.0}%",
                canonical_color,
                canonical_opacity * 100.
            ))
            .into_any_element()
    } else {
        div()
            .id(INK_INSPECTOR_COLOR_TRIGGER_ID)
            .debug_selector(|| INK_INSPECTOR_COLOR_TRIGGER_ID.into())
            .child(ColorPicker::new(state).label("Color"))
            .into_any_element()
    };
    Field::new()
        .label("Color")
        .child(
            div()
                .id(INK_INSPECTOR_COLOR_ID)
                .debug_selector(|| INK_INSPECTOR_COLOR_ID.into())
                .child(control)
                .child(
                    div()
                        .id(INK_INSPECTOR_APPLY_COLOR_ID)
                        .debug_selector(|| INK_INSPECTOR_APPLY_COLOR_ID.into())
                        .child(
                            Button::new("ink-property-inspector-apply-color-button")
                                .label("Apply color")
                                .disabled(disabled)
                                .on_click(move |_, _, cx| {
                                    let _ = inspector.update(cx, |inspector, cx| {
                                        inspector.apply_preview_color(cx);
                                    });
                                }),
                        ),
                ),
        )
        .into_any_element()
}

fn rgb_hex(color: gpui::Hsla) -> String {
    let rgb = gpui::Rgba::from(color);
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgb.r * 255.).round() as u8,
        (rgb.g * 255.).round() as u8,
        (rgb.b * 255.).round() as u8,
    )
}

fn format_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.6}");
        formatted.trim_end_matches('0').trim_end_matches('.').into()
    }
}

fn patch_matches_snapshot(patch: &InkPropertyPatch, snapshot: &InkPropertySnapshot) -> bool {
    match patch {
        InkPropertyPatch::Locked(value) => snapshot.locked == *value,
        InkPropertyPatch::Appearance(value) => snapshot.appearance == *value,
        InkPropertyPatch::WidthPt(value) => snapshot.appearance.width_pt() == *value,
        InkPropertyPatch::Opacity(value) => snapshot.appearance.opacity() == *value,
    }
}
