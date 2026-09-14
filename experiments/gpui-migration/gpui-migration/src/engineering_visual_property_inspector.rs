//! Retained property presentation for one selected Arc, Cloud, or Snapshot.
//!
//! The workspace owns identity, revision validation, persistence, and history.

use crate::annotation_model::{MarkupId, RectangleAppearance};
use crate::property_controls::{
    PropertyInspectorPanel, PropertyNumericInput, PropertySliderInput, canonical_picker_opacity,
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

pub const ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID: &str = "engineering-visual-property-inspector";
pub const ENGINEERING_VISUAL_INSPECTOR_LOCKED_ID: &str =
    "engineering-visual-property-inspector-locked";
pub const ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID: &str =
    "engineering-visual-property-inspector-color-trigger";
pub const ENGINEERING_VISUAL_INSPECTOR_APPLY_COLOR_ID: &str =
    "engineering-visual-property-inspector-apply-color";
pub const ENGINEERING_VISUAL_INSPECTOR_WIDTH_ID: &str =
    "engineering-visual-property-inspector-width";
pub const ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID: &str =
    "engineering-visual-property-inspector-opacity-track";
pub const ENGINEERING_VISUAL_INSPECTOR_INTENSITY_ID: &str =
    "engineering-visual-property-inspector-intensity";
pub const ENGINEERING_VISUAL_INSPECTOR_WIDTH_PX: f32 = 300.;
const ENGINEERING_VISUAL_INSPECTOR_HEADER_ID: &str = "engineering-visual-property-inspector-header";
const ENGINEERING_VISUAL_INSPECTOR_SCROLL_ID: &str = "engineering-visual-property-inspector-scroll";
const ENGINEERING_VISUAL_INSPECTOR_OPACITY_INPUT_ID: &str =
    "engineering-visual-property-inspector-opacity-input";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineeringVisualPropertyKind {
    Arc,
    Cloud,
    Snapshot,
}

impl EngineeringVisualPropertyKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Arc => "Arc",
            Self::Cloud => "Cloud",
            Self::Snapshot => "Snapshot",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineeringVisualPropertyValues {
    Arc {
        appearance: RectangleAppearance,
    },
    Cloud {
        appearance: RectangleAppearance,
        intensity: f64,
    },
    Snapshot {
        opacity: f64,
    },
}

impl EngineeringVisualPropertyValues {
    pub const fn kind(&self) -> EngineeringVisualPropertyKind {
        match self {
            Self::Arc { .. } => EngineeringVisualPropertyKind::Arc,
            Self::Cloud { .. } => EngineeringVisualPropertyKind::Cloud,
            Self::Snapshot { .. } => EngineeringVisualPropertyKind::Snapshot,
        }
    }

    fn appearance(&self) -> Option<&RectangleAppearance> {
        match self {
            Self::Arc { appearance } | Self::Cloud { appearance, .. } => Some(appearance),
            Self::Snapshot { .. } => None,
        }
    }

    fn opacity(&self) -> f64 {
        match self {
            Self::Arc { appearance } | Self::Cloud { appearance, .. } => appearance.opacity(),
            Self::Snapshot { opacity } => *opacity,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineeringVisualPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub values: EngineeringVisualPropertyValues,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineeringVisualPropertyPatch {
    Locked(bool),
    ColorAndOpacity { color: String, opacity: f64 },
    Color(String),
    WidthPt(f64),
    Opacity(f64),
    CloudIntensity(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineeringVisualPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub expected_kind: EngineeringVisualPropertyKind,
    pub patch: EngineeringVisualPropertyPatch,
}

pub struct EngineeringVisualPropertyInspector {
    snapshot: Option<EngineeringVisualPropertySnapshot>,
    syncing: bool,
    open: bool,
    embedded: bool,
    open_sections: [bool; 2],
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    intensity: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl EngineeringVisualPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.25)
                .max(24.)
                .step(0.25)
        });
        let intensity = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.)
                .max(4.)
                .step(0.25)
        });
        let width_slider = cx.new(|_| {
            SliderState::new()
                .min(0.25)
                .max(24.)
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
                try_parse_color("#ff0000").expect("the built-in visual color must parse"),
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
                |this, value, cx| {
                    this.emit_patch(EngineeringVisualPropertyPatch::WidthPt(value), cx)
                },
            ),
            cx.subscribe_in(&width, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f64>() else {
                    return;
                };
                if value.is_finite() && (0.25..=24.).contains(&value) {
                    this.emit_patch(EngineeringVisualPropertyPatch::WidthPt(value), cx);
                }
            }),
            cx.subscribe_in(
                &intensity,
                window,
                |this, input, event: &InputEvent, _, cx| {
                    if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let Ok(value) = input.read(cx).value().parse::<f64>() else {
                        return;
                    };
                    if value.is_finite() && (0.0..=4.).contains(&value) {
                        this.emit_patch(EngineeringVisualPropertyPatch::CloudIntensity(value), cx);
                    }
                },
            ),
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
                    SliderEvent::Release(value) => this.emit_patch(
                        EngineeringVisualPropertyPatch::Opacity(value.start() as f64 / 100.),
                        cx,
                    ),
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
                            .map(|snapshot| snapshot.values.opacity())
                            .unwrap_or(1.);
                        input.update(cx, |input, cx| {
                            input.set_value(format_property_percentage(canonical), window, cx)
                        });
                        return;
                    };
                    this.emit_patch(EngineeringVisualPropertyPatch::Opacity(opacity), cx);
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
            intensity,
            opacity,
            opacity_input,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&EngineeringVisualPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn set_embedded(&mut self) {
        self.embedded = true;
    }

    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open != open {
            self.open = open;
            cx.notify();
        }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.take().is_some() {
            cx.notify();
        }
    }

    pub fn sync(
        &mut self,
        snapshot: EngineeringVisualPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        if let Some(appearance) = snapshot.values.appearance() {
            self.width_slider.update(cx, |slider, cx| {
                slider.set_value(appearance.stroke_width_pt() as f32, window, cx)
            });
            self.width.update(cx, |input, cx| {
                input.set_value(
                    format_property_number(appearance.stroke_width_pt()),
                    window,
                    cx,
                );
            });
            if let Ok(mut color) = try_parse_color(appearance.stroke_color()) {
                color.a = appearance.opacity() as f32;
                self.color
                    .update(cx, |picker, cx| picker.set_value(color, window, cx));
            }
        }
        if let EngineeringVisualPropertyValues::Cloud { intensity, .. } = &snapshot.values {
            self.intensity.update(cx, |input, cx| {
                input.set_value(format_property_number(*intensity), window, cx);
            });
        }
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((snapshot.values.opacity() * 100.) as f32, window, cx);
        });
        self.opacity_input.update(cx, |input, cx| {
            input.set_value(
                format_property_percentage(snapshot.values.opacity()),
                window,
                cx,
            )
        });
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
    pub fn intensity_input(&self) -> gpui::Entity<InputState> {
        self.intensity.clone()
    }
    pub fn opacity_slider(&self) -> gpui::Entity<SliderState> {
        self.opacity.clone()
    }
    pub fn opacity_input(&self) -> gpui::Entity<InputState> {
        self.opacity_input.clone()
    }
    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.color.clone()
    }

    fn apply_preview_color(&mut self, cx: &mut Context<Self>) {
        let Some(appearance) = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.values.appearance())
        else {
            return;
        };
        let Some(color) = self.color.read(cx).value() else {
            return;
        };
        self.emit_patch(
            EngineeringVisualPropertyPatch::ColorAndOpacity {
                color: rgb_hex(color),
                opacity: canonical_picker_opacity(color.a, appearance.opacity()),
            },
            cx,
        );
    }

    fn emit_patch(&mut self, patch: EngineeringVisualPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, EngineeringVisualPropertyPatch::Locked(_)))
            || patch_matches_snapshot(&patch, snapshot)
        {
            return;
        }
        cx.emit(EngineeringVisualPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            expected_kind: snapshot.values.kind(),
            patch,
        });
    }
}

impl EventEmitter<EngineeringVisualPropertyEvent> for EngineeringVisualPropertyInspector {}

impl Render for EngineeringVisualPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID)
                .debug_selector(|| ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID.into())
                .hidden()
                .into_any_element();
        };
        let kind = snapshot.values.kind();
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let lock_control = cx.entity().downgrade();
        let section_control = cx.entity().downgrade();
        let details = v_flex()
            .gap_3()
            .child(Field::new().label("Type").child(div().child(kind.label())))
            .child(
                Field::new().child(
                    div()
                        .id(ENGINEERING_VISUAL_INSPECTOR_LOCKED_ID)
                        .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_LOCKED_ID.into())
                        .child(
                            Switch::new("engineering-visual-property-inspector-locked-switch")
                                .label("Locked")
                                .checked(snapshot.locked)
                                .disabled(snapshot.mutation_disabled)
                                .on_click(move |locked, _, cx| {
                                    let _ = lock_control.update(cx, |inspector, cx| {
                                        inspector.emit_patch(
                                            EngineeringVisualPropertyPatch::Locked(*locked),
                                            cx,
                                        );
                                    });
                                }),
                        ),
                ),
            );
        let mut appearance = v_flex().gap_3();
        if let Some(value) = snapshot.values.appearance() {
            appearance = appearance
                .child(color_field(
                    &self.color,
                    value.stroke_color(),
                    disabled,
                    cx.entity().downgrade(),
                ))
                .child(
                    Field::new().label("Line width").child(
                        PropertySliderInput::new("Line width", &self.width_slider, &self.width)
                            .input_id(ENGINEERING_VISUAL_INSPECTOR_WIDTH_ID)
                            .suffix("pt")
                            .disabled(disabled),
                    ),
                );
        }
        appearance = appearance.child(
            Field::new().label("Opacity").child(
                PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                    .slider_id(ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID)
                    .input_id(ENGINEERING_VISUAL_INSPECTOR_OPACITY_INPUT_ID)
                    .suffix("%")
                    .disabled(disabled),
            ),
        );
        if matches!(
            snapshot.values,
            EngineeringVisualPropertyValues::Cloud { .. }
        ) {
            appearance = appearance.child(
                Field::new().label("Intensity").child(
                    PropertyNumericInput::new(
                        ENGINEERING_VISUAL_INSPECTOR_INTENSITY_ID,
                        "Intensity",
                        &self.intensity,
                    )
                    .disabled(disabled),
                ),
            );
        }
        let open_sections = self.open_sections;
        let accordion = Accordion::new("engineering-visual-property-inspector-sections")
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
            ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID,
            ENGINEERING_VISUAL_INSPECTOR_HEADER_ID,
            ENGINEERING_VISUAL_INSPECTOR_SCROLL_ID,
            format!("{} properties", kind.label()),
        )
        .child(accordion)
        .into_any_element()
    }
}

fn color_field(
    state: &gpui::Entity<ColorPickerState>,
    canonical_color: &str,
    disabled: bool,
    inspector: gpui::WeakEntity<EngineeringVisualPropertyInspector>,
) -> AnyElement {
    let control = if disabled {
        Button::new(ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID)
            .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID.into())
            .label(format!("Color: {canonical_color}"))
            .disabled(true)
            .into_any_element()
    } else {
        div()
            .id(ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID)
            .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID.into())
            .child(property_color_picker(state, "Colour"))
            .into_any_element()
    };
    Field::new()
        .label("Colour")
        .child(
            div().child(control).child(
                div()
                    .id(ENGINEERING_VISUAL_INSPECTOR_APPLY_COLOR_ID)
                    .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_APPLY_COLOR_ID.into())
                    .child(
                        Button::new("engineering-visual-property-inspector-apply-color-button")
                            .label("Apply color")
                            .disabled(disabled)
                            .on_click(move |_, _, cx| {
                                let _ = inspector
                                    .update(cx, |inspector, cx| inspector.apply_preview_color(cx));
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
        (rgb.b * 255.).round() as u8
    )
}

fn patch_matches_snapshot(
    patch: &EngineeringVisualPropertyPatch,
    snapshot: &EngineeringVisualPropertySnapshot,
) -> bool {
    match patch {
        EngineeringVisualPropertyPatch::Locked(value) => snapshot.locked == *value,
        EngineeringVisualPropertyPatch::ColorAndOpacity { color, opacity } => {
            snapshot.values.appearance().is_some_and(|appearance| {
                appearance.stroke_color() == color && appearance.opacity() == *opacity
            })
        }
        EngineeringVisualPropertyPatch::Color(value) => snapshot
            .values
            .appearance()
            .is_some_and(|appearance| appearance.stroke_color() == value),
        EngineeringVisualPropertyPatch::WidthPt(value) => snapshot
            .values
            .appearance()
            .is_some_and(|appearance| appearance.stroke_width_pt() == *value),
        EngineeringVisualPropertyPatch::Opacity(value) => snapshot.values.opacity() == *value,
        EngineeringVisualPropertyPatch::CloudIntensity(value) => matches!(
            snapshot.values, EngineeringVisualPropertyValues::Cloud { intensity, .. } if intensity == *value
        ),
    }
}
