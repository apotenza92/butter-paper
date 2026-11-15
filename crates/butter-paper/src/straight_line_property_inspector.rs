//! Retained property presentation for one exact selected Line or Arrow.
//!
//! The workspace owns annotation identity, revision validation, and history.

use crate::annotation_model::{LineKind, MarkupId, StraightLineAppearance};
use crate::property_controls::{
    PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
    PropertySliderInput, canonical_picker_opacity, format_property_number,
    format_property_percentage, parse_property_percentage, property_body, property_color_and_size,
    property_lock_toggle, property_row, property_slider_row, subscribe_property_color,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    color_picker::ColorPickerState,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
    try_parse_color,
};

use crate::document_workspace::DocumentId;

pub const STRAIGHT_LINE_PROPERTY_INSPECTOR_ID: &str = "straight-line-property-inspector";
pub const STRAIGHT_LINE_INSPECTOR_LOCKED_ID: &str = "straight-line-property-inspector-locked";
pub const STRAIGHT_LINE_INSPECTOR_COLOR_TRIGGER_ID: &str =
    "straight-line-property-inspector-color-trigger";
pub const STRAIGHT_LINE_INSPECTOR_WIDTH_ID: &str = "straight-line-property-inspector-width";
pub const STRAIGHT_LINE_INSPECTOR_OPACITY_TRACK_ID: &str =
    "straight-line-property-inspector-opacity-track";
const STRAIGHT_LINE_INSPECTOR_HEADER_ID: &str = "straight-line-property-inspector-header";
const STRAIGHT_LINE_INSPECTOR_SCROLL_ID: &str = "straight-line-property-inspector-scroll";
const STRAIGHT_LINE_INSPECTOR_OPACITY_INPUT_ID: &str =
    "straight-line-property-inspector-opacity-input";

#[derive(Clone, Debug, PartialEq)]
pub struct StraightLinePropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub kind: LineKind,
    pub appearance: StraightLineAppearance,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StraightLinePropertyPatch {
    Locked(bool),
    ColorAndOpacity { color: String, opacity: f64 },
    Color(String),
    WidthPt(f64),
    Opacity(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StraightLinePropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub expected_kind: LineKind,
    pub patch: StraightLinePropertyPatch,
}

pub struct StraightLinePropertyInspector {
    snapshot: Option<StraightLinePropertySnapshot>,
    syncing: bool,
    open: bool,
    embedded: bool,
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl StraightLinePropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.25)
                .max(24.)
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
                try_parse_color("#ff0000").expect("the built-in Line color must parse"),
            )
        });
        let opacity_input_for_slider = opacity_input.clone();
        let mut subscriptions = vec![
            crate::property_controls::subscribe_property_slider(
                &width_slider,
                &width,
                window,
                cx,
                |this| !this.syncing,
                |this, value, cx| this.emit_patch(StraightLinePropertyPatch::WidthPt(value), cx),
            ),
            cx.subscribe_in(&width, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f64>() else {
                    return;
                };
                if value.is_finite() && (0.25..=24.).contains(&value) {
                    this.emit_patch(StraightLinePropertyPatch::WidthPt(value), cx);
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
                    SliderEvent::Release(value) => this.emit_patch(
                        StraightLinePropertyPatch::Opacity(value.start() as f64 / 100.),
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
                            .map(|snapshot| snapshot.appearance.opacity())
                            .unwrap_or(1.);
                        input.update(cx, |input, cx| {
                            input.set_value(format_property_percentage(canonical), window, cx)
                        });
                        return;
                    };
                    this.emit_patch(StraightLinePropertyPatch::Opacity(opacity), cx);
                },
            ),
        ];
        subscriptions.extend(subscribe_property_color(
            &color,
            cx,
            |this: &mut Self, cx| this.apply_preview_color(cx),
        ));
        Self {
            snapshot: None,
            syncing: false,
            open: false,
            embedded: false,
            width,
            width_slider,
            opacity,
            opacity_input,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&StraightLinePropertySnapshot> {
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
        snapshot: StraightLinePropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.width_slider.update(cx, |slider, cx| {
            slider.set_value((snapshot.appearance.stroke_width_pt()) as f32, window, cx)
        });
        self.width.update(cx, |input, cx| {
            input.set_value(
                format_property_number(snapshot.appearance.stroke_width_pt()),
                window,
                cx,
            );
        });
        if let Ok(mut color) = try_parse_color(snapshot.appearance.stroke_color()) {
            color.a = snapshot.appearance.opacity() as f32;
            self.color
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
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
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let Some(color) = self.color.read(cx).value() else {
            return;
        };
        self.emit_patch(
            StraightLinePropertyPatch::ColorAndOpacity {
                color: rgb_hex(color),
                opacity: canonical_picker_opacity(color.a, snapshot.appearance.opacity()),
            },
            cx,
        );
    }

    fn emit_patch(&mut self, patch: StraightLinePropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, StraightLinePropertyPatch::Locked(_)))
            || patch_matches_snapshot(&patch, snapshot)
        {
            return;
        }
        cx.emit(StraightLinePropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            expected_kind: snapshot.kind,
            patch,
        });
    }
}

impl EventEmitter<StraightLinePropertyEvent> for StraightLinePropertyInspector {}

impl Render for StraightLinePropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(STRAIGHT_LINE_PROPERTY_INSPECTOR_ID)
                .debug_selector(|| STRAIGHT_LINE_PROPERTY_INSPECTOR_ID.into())
                .hidden()
                .into_any_element();
        };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let lock_control = cx.entity().downgrade();
        let line = property_row(
            "Line",
            property_color_and_size(
                PropertyColor::new(
                    STRAIGHT_LINE_INSPECTOR_COLOR_TRIGGER_ID,
                    "Line colour",
                    &self.color,
                )
                .disabled(disabled),
                PropertyNumericInput::new(
                    STRAIGHT_LINE_INSPECTOR_WIDTH_ID,
                    "Line width",
                    &self.width,
                )
                .suffix("pt")
                .disabled(disabled),
            ),
        );
        let opacity = property_slider_row(
            "Opacity",
            PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                .slider_id(STRAIGHT_LINE_INSPECTOR_OPACITY_TRACK_ID)
                .input_id(STRAIGHT_LINE_INSPECTOR_OPACITY_INPUT_ID)
                .suffix("%")
                .disabled(disabled),
        );
        PropertyInspectorPanel::new(
            STRAIGHT_LINE_PROPERTY_INSPECTOR_ID,
            STRAIGHT_LINE_INSPECTOR_HEADER_ID,
            STRAIGHT_LINE_INSPECTOR_SCROLL_ID,
            match snapshot.kind {
                LineKind::Line => "Line",
                LineKind::Arrow => "Arrow",
            },
        )
        .header_lock(property_lock_toggle(
            STRAIGHT_LINE_INSPECTOR_LOCKED_ID,
            snapshot.locked,
            snapshot.mutation_disabled,
            move |locked, _, cx| {
                let _ = lock_control.update(cx, |inspector, cx| {
                    inspector.emit_patch(StraightLinePropertyPatch::Locked(locked), cx);
                });
            },
        ))
        .child(property_body().child(PropertyGroup::new().row(line).row(opacity)))
        .into_any_element()
    }
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
    patch: &StraightLinePropertyPatch,
    snapshot: &StraightLinePropertySnapshot,
) -> bool {
    match patch {
        StraightLinePropertyPatch::Locked(value) => snapshot.locked == *value,
        StraightLinePropertyPatch::ColorAndOpacity { color, opacity } => {
            snapshot.appearance.stroke_color() == color && snapshot.appearance.opacity() == *opacity
        }
        StraightLinePropertyPatch::Color(value) => snapshot.appearance.stroke_color() == value,
        StraightLinePropertyPatch::WidthPt(value) => {
            snapshot.appearance.stroke_width_pt() == *value
        }
        StraightLinePropertyPatch::Opacity(value) => snapshot.appearance.opacity() == *value,
    }
}
