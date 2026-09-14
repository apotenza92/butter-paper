//! Retained property presentation for one exact selected Polyline or Polygon.
//! The workspace owns annotation authority, validation, and history.

use crate::annotation_model::{MarkupId, MeasurementPathKind, RectangleAppearance, VertexPathKind};
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

pub const VERTEX_PATH_PROPERTY_INSPECTOR_ID: &str = "vertex-path-property-inspector";
pub const VERTEX_PATH_INSPECTOR_LOCKED_ID: &str = "vertex-path-property-inspector-locked";
pub const VERTEX_PATH_INSPECTOR_STROKE_COLOR_ID: &str =
    "vertex-path-property-inspector-stroke-color";
pub const VERTEX_PATH_INSPECTOR_APPLY_STROKE_ID: &str =
    "vertex-path-property-inspector-apply-stroke";
pub const VERTEX_PATH_INSPECTOR_WIDTH_ID: &str = "vertex-path-property-inspector-width";
pub const VERTEX_PATH_INSPECTOR_OPACITY_ID: &str = "vertex-path-property-inspector-opacity";
pub const VERTEX_PATH_INSPECTOR_FILL_COLOR_ID: &str = "vertex-path-property-inspector-fill-color";
pub const VERTEX_PATH_INSPECTOR_APPLY_FILL_ID: &str = "vertex-path-property-inspector-apply-fill";
pub const VERTEX_PATH_INSPECTOR_NO_FILL_ID: &str = "vertex-path-property-inspector-no-fill";
const VERTEX_PATH_INSPECTOR_HEADER_ID: &str = "vertex-path-property-inspector-header";
const VERTEX_PATH_INSPECTOR_SCROLL_ID: &str = "vertex-path-property-inspector-scroll";
const VERTEX_PATH_INSPECTOR_OPACITY_INPUT_ID: &str = "vertex-path-property-inspector-opacity-input";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathPropertyKind {
    Polyline,
    Polygon,
    Polylength,
    Area,
}

impl PathPropertyKind {
    pub fn supports_fill(self) -> bool {
        matches!(self, Self::Polygon | Self::Area)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Polyline => "Polyline",
            Self::Polygon => "Polygon",
            Self::Polylength => "Polylength",
            Self::Area => "Area",
        }
    }
}

impl From<VertexPathKind> for PathPropertyKind {
    fn from(value: VertexPathKind) -> Self {
        match value {
            VertexPathKind::Polyline => Self::Polyline,
            VertexPathKind::Polygon => Self::Polygon,
        }
    }
}

impl From<MeasurementPathKind> for PathPropertyKind {
    fn from(value: MeasurementPathKind) -> Self {
        match value {
            MeasurementPathKind::Polylength => Self::Polylength,
            MeasurementPathKind::Area => Self::Area,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VertexPathPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub kind: PathPropertyKind,
    pub appearance: RectangleAppearance,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VertexPathPropertyPatch {
    Locked(bool),
    StrokeColorAndOpacity { color: String, opacity: f64 },
    FillColorAndOpacity { color: String, opacity: f64 },
    StrokeColor(String),
    StrokeWidthPt(f64),
    Opacity(f64),
    FillColor(Option<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VertexPathPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub expected_kind: PathPropertyKind,
    pub patch: VertexPathPropertyPatch,
}

pub struct VertexPathPropertyInspector {
    snapshot: Option<VertexPathPropertySnapshot>,
    syncing: bool,
    open: bool,
    embedded: bool,
    open_sections: [bool; 2],
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    stroke_color: gpui::Entity<ColorPickerState>,
    fill_color: gpui::Entity<ColorPickerState>,
    fill_preview_available: bool,
    _subscriptions: Vec<Subscription>,
}

impl VertexPathPropertyInspector {
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
        let stroke_color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(try_parse_color("#ff0000").unwrap())
        });
        let fill_color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(try_parse_color("#ffffff").unwrap())
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
                    this.emit_patch(VertexPathPropertyPatch::StrokeWidthPt(value), cx)
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
                    this.emit_patch(VertexPathPropertyPatch::StrokeWidthPt(value), cx);
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
                        VertexPathPropertyPatch::Opacity(value.start() as f64 / 100.),
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
                    this.emit_patch(VertexPathPropertyPatch::Opacity(opacity), cx);
                },
            ),
            cx.subscribe(&stroke_color, |this, _, _: &ColorPickerEvent, cx| {
                if !this.syncing {
                    cx.notify();
                }
            }),
            cx.subscribe(&fill_color, |this, _, _: &ColorPickerEvent, cx| {
                if !this.syncing {
                    this.fill_preview_available = true;
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
            stroke_color,
            fill_color,
            fill_preview_available: false,
            _subscriptions: subscriptions,
        }
    }
    pub fn snapshot(&self) -> Option<&VertexPathPropertySnapshot> {
        self.snapshot.as_ref()
    }
    pub fn set_embedded(&mut self) {
        self.embedded = true;
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
    pub fn stroke_color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.stroke_color.clone()
    }
    pub fn fill_color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.fill_color.clone()
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
        snapshot: VertexPathPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.width_slider.update(cx, |slider, cx| {
            slider.set_value((snapshot.appearance.stroke_width_pt()) as f32, window, cx)
        });
        self.width.update(cx, |state, cx| {
            state.set_value(
                format_property_number(snapshot.appearance.stroke_width_pt()),
                window,
                cx,
            )
        });
        if let Ok(mut value) = try_parse_color(snapshot.appearance.stroke_color()) {
            value.a = snapshot.appearance.opacity() as f32;
            self.stroke_color
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        let mut fill = snapshot
            .appearance
            .fill_color()
            .and_then(|value| try_parse_color(value).ok())
            .unwrap_or_else(|| {
                try_parse_color("#ffffff").expect("the reset fill preview must parse")
            });
        fill.a = snapshot.appearance.fill_opacity() as f32;
        self.fill_color
            .update(cx, |state, cx| state.set_value(fill, window, cx));
        self.fill_preview_available = false;
        self.opacity.update(cx, |state, cx| {
            state.set_value((snapshot.appearance.opacity() * 100.) as f32, window, cx)
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
    fn apply_color(&mut self, fill: bool, cx: &mut Context<Self>) {
        if fill && !self.fill_preview_available {
            return;
        }
        let state = if fill {
            &self.fill_color
        } else {
            &self.stroke_color
        };
        let Some(color) = state.read(cx).value() else {
            return;
        };
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let patch = if fill {
            VertexPathPropertyPatch::FillColorAndOpacity {
                color: rgb_hex(color),
                opacity: canonical_picker_opacity(color.a, snapshot.appearance.fill_opacity()),
            }
        } else {
            VertexPathPropertyPatch::StrokeColorAndOpacity {
                color: rgb_hex(color),
                opacity: canonical_picker_opacity(color.a, snapshot.appearance.opacity()),
            }
        };
        self.emit_patch(patch, cx);
        if fill {
            self.fill_preview_available = false;
        }
    }
    fn emit_patch(&mut self, patch: VertexPathPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, VertexPathPropertyPatch::Locked(_)))
            || patch_matches(&patch, snapshot)
        {
            return;
        }
        cx.emit(VertexPathPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            expected_kind: snapshot.kind,
            patch,
        });
    }
}
impl EventEmitter<VertexPathPropertyEvent> for VertexPathPropertyInspector {}

impl Render for VertexPathPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(VERTEX_PATH_PROPERTY_INSPECTOR_ID)
                .debug_selector(|| VERTEX_PATH_PROPERTY_INSPECTOR_ID.into())
                .hidden()
                .into_any_element();
        };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let disabled_reason = if snapshot.locked {
            "locked"
        } else {
            "unavailable while the document is busy"
        };
        let weak = cx.entity().downgrade();
        let weak_lock = weak.clone();
        let weak_stroke = weak.clone();
        let weak_fill = weak.clone();
        let weak_none = weak.clone();
        let weak_sections = weak.clone();
        let details = v_flex()
            .gap_3()
            .child(
                Field::new()
                    .label("Type")
                    .child(div().child(snapshot.kind.label())),
            )
            .child(
                Field::new().child(
                    div()
                        .id(VERTEX_PATH_INSPECTOR_LOCKED_ID)
                        .debug_selector(|| VERTEX_PATH_INSPECTOR_LOCKED_ID.into())
                        .child(
                            Switch::new("vertex-path-property-inspector-locked-switch")
                                .label("Locked")
                                .checked(snapshot.locked)
                                .disabled(snapshot.mutation_disabled)
                                .on_click(move |locked, _, cx| {
                                    let _ = weak_lock.update(cx, |this, cx| {
                                        this.emit_patch(
                                            VertexPathPropertyPatch::Locked(*locked),
                                            cx,
                                        )
                                    });
                                }),
                        ),
                ),
            );
        let mut appearance = v_flex()
            .gap_3()
            .child(color_control(
                VERTEX_PATH_INSPECTOR_STROKE_COLOR_ID,
                "Stroke",
                &self.stroke_color,
                disabled,
                disabled_reason,
            ))
            .child(
                div()
                    .id(VERTEX_PATH_INSPECTOR_APPLY_STROKE_ID)
                    .debug_selector(|| VERTEX_PATH_INSPECTOR_APPLY_STROKE_ID.into())
                    .child(
                        Button::new("vertex-path-apply-stroke-button")
                            .label("Apply")
                            .disabled(disabled)
                            .on_click(move |_, _, cx| {
                                let _ =
                                    weak_stroke.update(cx, |this, cx| this.apply_color(false, cx));
                            }),
                    ),
            )
            .child(
                Field::new().label("Line width").child(
                    PropertySliderInput::new("Line width", &self.width_slider, &self.width)
                        .input_id(VERTEX_PATH_INSPECTOR_WIDTH_ID)
                        .suffix("pt")
                        .disabled(disabled),
                ),
            )
            .child(
                Field::new().label("Opacity").child(
                    PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                        .slider_id(VERTEX_PATH_INSPECTOR_OPACITY_ID)
                        .input_id(VERTEX_PATH_INSPECTOR_OPACITY_INPUT_ID)
                        .suffix("%")
                        .disabled(disabled),
                ),
            );
        if snapshot.kind.supports_fill() {
            let fill_preview_available = self.fill_preview_available;
            let has_fill = snapshot.appearance.fill_color().is_some();
            appearance = appearance
                .child(color_control(
                    VERTEX_PATH_INSPECTOR_FILL_COLOR_ID,
                    "Fill",
                    &self.fill_color,
                    disabled,
                    disabled_reason,
                ))
                .child(
                    div()
                        .id(VERTEX_PATH_INSPECTOR_APPLY_FILL_ID)
                        .debug_selector(|| VERTEX_PATH_INSPECTOR_APPLY_FILL_ID.into())
                        .child(
                            Button::new("vertex-path-apply-fill-button")
                                .label("Apply")
                                .disabled(disabled || !fill_preview_available)
                                .on_click(move |_, _, cx| {
                                    let _ =
                                        weak_fill.update(cx, |this, cx| this.apply_color(true, cx));
                                }),
                        ),
                )
                .child(
                    Button::new(VERTEX_PATH_INSPECTOR_NO_FILL_ID)
                        .debug_selector(|| VERTEX_PATH_INSPECTOR_NO_FILL_ID.into())
                        .label("No fill")
                        .disabled(disabled || !has_fill)
                        .on_click(move |_, _, cx| {
                            let _ = weak_none.update(cx, |this, cx| {
                                this.emit_patch(VertexPathPropertyPatch::FillColor(None), cx)
                            });
                        }),
                );
        }
        let open = self.open_sections;
        let accordion = Accordion::new("vertex-path-property-inspector-sections")
            .bordered(false)
            .multiple(true)
            .item(|item| item.open(open[0]).title("Details").child(details))
            .item(|item| item.open(open[1]).title("Appearance").child(appearance))
            .on_toggle_click(move |open, _, cx| {
                let _ = weak_sections.update(cx, |this, cx| {
                    this.open_sections = [open.contains(&0), open.contains(&1)];
                    cx.notify();
                });
            });
        PropertyInspectorPanel::new(
            VERTEX_PATH_PROPERTY_INSPECTOR_ID,
            VERTEX_PATH_INSPECTOR_HEADER_ID,
            VERTEX_PATH_INSPECTOR_SCROLL_ID,
            format!("{} properties", snapshot.kind.label()),
        )
        .content_only(
            self.embedded
                && matches!(
                    snapshot.kind,
                    PathPropertyKind::Polylength | PathPropertyKind::Area
                ),
        )
        .child(accordion)
        .into_any_element()
    }
}

fn color_control(
    id: &'static str,
    label: &'static str,
    state: &gpui::Entity<ColorPickerState>,
    disabled: bool,
    disabled_reason: &'static str,
) -> AnyElement {
    Field::new()
        .label(label)
        .child(if disabled {
            Button::new(id)
                .debug_selector(move || id.into())
                .label(format!("{label}: {disabled_reason}"))
                .disabled(true)
                .into_any_element()
        } else {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .child(property_color_picker(state, label))
                .into_any_element()
        })
        .into_any_element()
}
fn patch_matches(patch: &VertexPathPropertyPatch, snapshot: &VertexPathPropertySnapshot) -> bool {
    match patch {
        VertexPathPropertyPatch::Locked(v) => snapshot.locked == *v,
        VertexPathPropertyPatch::StrokeColorAndOpacity { color, opacity } => {
            snapshot.appearance.stroke_color() == color && snapshot.appearance.opacity() == *opacity
        }
        VertexPathPropertyPatch::FillColorAndOpacity { color, opacity } => {
            snapshot.appearance.fill_color() == Some(color.as_str())
                && snapshot.appearance.fill_opacity() == *opacity
        }
        VertexPathPropertyPatch::StrokeColor(v) => snapshot.appearance.stroke_color() == v,
        VertexPathPropertyPatch::StrokeWidthPt(v) => snapshot.appearance.stroke_width_pt() == *v,
        VertexPathPropertyPatch::Opacity(v) => snapshot.appearance.opacity() == *v,
        VertexPathPropertyPatch::FillColor(v) => snapshot.appearance.fill_color() == v.as_deref(),
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
