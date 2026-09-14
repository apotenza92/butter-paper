//! Retained presentation state for one selected Pen or Highlight.
//!
//! The inspector owns only GPUI Component entities and disclosure state. The
//! workspace owns annotation identity, validation, persistence, and history.

use crate::annotation_model::{BlendMode, InkTool, MarkupId, PenAppearance};
use crate::property_controls::{
    PropertyInspectorPanel, PropertySliderInput, canonical_picker_opacity,
    format_property_number, format_property_percentage, parse_property_percentage,
    property_color_picker,
};
use gpui::{
    AnyElement, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    Disableable as _,
    accordion::Accordion,
    button::Button,
    color_picker::{ColorPickerEvent, ColorPickerState},
    form::Field,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
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
const INK_INSPECTOR_OPACITY_INPUT_ID: &str = "ink-property-inspector-opacity-input";

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
    embedded: bool,
    open_sections: [bool; 2],
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
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
        let width_slider = cx.new(|_| {
            SliderState::new()
                .min(0.01)
                .max(20.)
                .step(0.25)
                .default_value(1.)
        });
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let opacity_input = cx.new(|cx| InputState::new(window, cx).default_value("100"));
        let color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("the built-in Pen color must parse"),
            )
        });
        let opacity_input_for_slider = opacity_input.clone();
        let subscriptions = vec![
            crate::property_controls::subscribe_property_slider(
                &width_slider,
                &width,
                window,
                cx,
                |this| !this.syncing,
                |this, value, cx| this.emit_patch(InkPropertyPatch::WidthPt(value), cx),
            ),
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
            cx.subscribe_in(
                &opacity,
                window,
                move |this, _, event: &SliderEvent, window, cx| match event {
                    SliderEvent::Change(value) if !this.syncing => {
                        opacity_input_for_slider.update(cx, |input, cx| {
                            input.set_value(
                                format_property_number(value.start().into()),
                                window,
                                cx,
                            )
                        });
                        cx.notify();
                    }
                    SliderEvent::Change(_) => {}
                    SliderEvent::Release(value) => {
                        this.emit_patch(InkPropertyPatch::Opacity(value.start() as f64 / 100.), cx)
                    }
                },
            ),
            cx.subscribe_in(
                &opacity_input,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let Some(opacity) = parse_property_percentage(input.read(cx).value().as_ref())
                    else {
                        let canonical = this
                            .snapshot
                            .as_ref()
                            .map(|snapshot| snapshot.appearance.opacity())
                            .unwrap_or(1.);
                        input.update(cx, |input, cx| {
                            input.set_value(format_property_percentage(canonical), window, cx)
                        });
                        return;
                    };
                    this.emit_patch(InkPropertyPatch::Opacity(opacity), cx);
                },
            ),
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
            embedded: false,
            open_sections: [true, true],
            width,
            width_slider,
            opacity,
            opacity_input,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&InkPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn set_embedded(&mut self) {
        self.embedded = true;
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
        self.width_slider.update(cx, |slider, cx| {
            slider.set_value((snapshot.appearance.width_pt()) as f32, window, cx)
        });
        self.width.update(cx, |input, cx| {
            input.set_value(
                format_property_number(snapshot.appearance.width_pt()),
                window,
                cx,
            );
        });
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((snapshot.appearance.opacity() * 100.) as f32, window, cx);
        });
        self.opacity_input.update(cx, |input, cx| {
            input.set_value(
                format_property_percentage(snapshot.appearance.opacity()),
                window,
                cx,
            )
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

    pub fn width_slider(&self) -> gpui::Entity<SliderState> {
        self.width_slider.clone()
    }

    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.color.clone()
    }

    pub fn opacity_slider(&self) -> gpui::Entity<SliderState> {
        self.opacity.clone()
    }

    pub fn opacity_input(&self) -> gpui::Entity<InputState> {
        self.opacity_input.clone()
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
            canonical_picker_opacity(color.a, snapshot.appearance.opacity()),
        ) else {
            return;
        };
        self.emit_patch(InkPropertyPatch::Appearance(appearance), cx);
    }

    fn emit_patch(&mut self, patch: InkPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
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
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
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

        let details = v_flex()
            .id(INK_INSPECTOR_DETAILS_ID)
            .debug_selector(|| INK_INSPECTOR_DETAILS_ID.into())
            .gap_3()
            .child(
                Field::new().child(
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
                Field::new().label("Line width").child(
                    PropertySliderInput::new("Line width", &self.width_slider, &self.width)
                        .input_id(INK_INSPECTOR_WIDTH_ID)
                        .suffix("pt")
                        .disabled(property_disabled),
                ),
            )
            .child(
                Field::new().label("Opacity").child(
                    PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                        .row_id(INK_INSPECTOR_OPACITY_ID)
                        .slider_id(INK_INSPECTOR_OPACITY_TRACK_ID)
                        .input_id(INK_INSPECTOR_OPACITY_INPUT_ID)
                        .suffix("%")
                        .disabled(property_disabled),
                ),
            );
        let accordion = Accordion::new(INK_INSPECTOR_ACCORDION_ID)
            .bordered(false)
            .multiple(true)
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
        PropertyInspectorPanel::new(
            INK_PROPERTY_INSPECTOR_ID,
            INK_INSPECTOR_HEADER_ID,
            INK_INSPECTOR_SCROLL_ID,
            match snapshot.tool {
                InkTool::Pen => "Pen",
                InkTool::Highlight => "Highlight",
            },
        )
        .child(accordion)
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
        Button::new(INK_INSPECTOR_COLOR_TRIGGER_ID)
            .debug_selector(|| INK_INSPECTOR_COLOR_TRIGGER_ID.into())
            .label(format!(
                "{} · {:.0}%",
                canonical_color,
                canonical_opacity * 100.
            ))
            .disabled(true)
            .into_any_element()
    } else {
        div()
            .id(INK_INSPECTOR_COLOR_TRIGGER_ID)
            .debug_selector(|| INK_INSPECTOR_COLOR_TRIGGER_ID.into())
            .child(property_color_picker(state, "Colour"))
            .into_any_element()
    };
    Field::new()
        .label("Colour")
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

fn patch_matches_snapshot(patch: &InkPropertyPatch, snapshot: &InkPropertySnapshot) -> bool {
    match patch {
        InkPropertyPatch::Locked(value) => snapshot.locked == *value,
        InkPropertyPatch::Appearance(value) => snapshot.appearance == *value,
        InkPropertyPatch::WidthPt(value) => snapshot.appearance.width_pt() == *value,
        InkPropertyPatch::Opacity(value) => snapshot.appearance.opacity() == *value,
    }
}
