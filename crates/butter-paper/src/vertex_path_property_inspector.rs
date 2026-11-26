//! Retained property presentation for one exact selected Polyline or Polygon.
//! The workspace owns annotation authority, validation, and history.

use crate::annotation_model::{MarkupId, MeasurementPathKind, RectangleAppearance, VertexPathKind};
use crate::property_controls::{
    FieldPhase, PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyPreviewEvent,
    PropertySliderInput, PropertyStepInput, canonical_picker_opacity, format_property_number,
    format_property_percentage, parse_property_percentage, property_body, property_lock_toggle,
    property_row, property_slider_row, subscribe_property_color,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    Disableable as _,
    color_picker::ColorPickerState,
    h_flex,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
    switch::Switch,
    try_parse_color,
};

use crate::document_workspace::DocumentId;

pub const VERTEX_PATH_PROPERTY_INSPECTOR_ID: &str = "vertex-path-property-inspector";
pub const VERTEX_PATH_INSPECTOR_LOCKED_ID: &str = "vertex-path-property-inspector-locked";
pub const VERTEX_PATH_INSPECTOR_STROKE_COLOR_ID: &str =
    "vertex-path-property-inspector-stroke-color";
pub const VERTEX_PATH_INSPECTOR_WIDTH_ID: &str = "vertex-path-property-inspector-width";
pub const VERTEX_PATH_INSPECTOR_OPACITY_ID: &str = "vertex-path-property-inspector-opacity";
pub const VERTEX_PATH_INSPECTOR_FILL_COLOR_ID: &str = "vertex-path-property-inspector-fill-color";
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
    /// A Polylength or Area: measured with the page scale.
    pub fn measures(self) -> bool {
        matches!(self, Self::Polylength | Self::Area)
    }

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
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    stroke_color: gpui::Entity<ColorPickerState>,
    fill_color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl VertexPathPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.25)
                .max(24.)
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
        let mut subscriptions = vec![
            crate::property_controls::subscribe_property_slider(
                &width_slider,
                &width,
                window,
                cx,
                |this| !this.syncing,
                |this, value, cx| {
                    this.preview_patch(VertexPathPropertyPatch::StrokeWidthPt(value), cx)
                },
                |this, value, cx| {
                    this.end_preview(cx);
                    this.emit_patch(VertexPathPropertyPatch::StrokeWidthPt(value), cx)
                },
            ),
            cx.subscribe_in(&width, window, |this, input, event: &InputEvent, _, cx| {
                let Some(phase) = FieldPhase::of(event) else {
                    return;
                };
                let Ok(value) = input.read(cx).value().parse::<f64>() else {
                    this.field_unparsed(phase, cx);
                    return;
                };
                if value.is_finite() && (0.25..=24.).contains(&value) {
                    this.apply_field(VertexPathPropertyPatch::StrokeWidthPt(value), phase, cx);
                } else {
                    this.field_unparsed(phase, cx);
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
                        this.preview_patch(
                            VertexPathPropertyPatch::Opacity(value.start() as f64 / 100.),
                            cx,
                        );
                        cx.notify();
                    }
                    SliderEvent::Change(_) => {}
                    SliderEvent::Release(value) => {
                        this.end_preview(cx);
                        this.emit_patch(
                            VertexPathPropertyPatch::Opacity(value.start() as f64 / 100.),
                            cx,
                        )
                    }
                },
            ),
            cx.subscribe_in(
                &opacity_input,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    let Some(phase) = FieldPhase::of(event) else {
                        return;
                    };
                    let Some(opacity) = parse_property_percentage(input.read(cx).value().as_ref())
                    else {
                        this.field_unparsed(phase, cx);
                        if phase == FieldPhase::Preview {
                            return;
                        }
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
                    this.apply_field(VertexPathPropertyPatch::Opacity(opacity), phase, cx);
                },
            ),
        ];
        subscriptions.push(crate::property_controls::subscribe_number_steps(
            &width, 0.5, 0.25, 24., window, cx,
        ));
        subscriptions.extend(subscribe_property_color(
            &stroke_color,
            cx,
            |this: &mut Self, cx| this.apply_color(false, cx),
        ));
        subscriptions.extend(subscribe_property_color(
            &fill_color,
            cx,
            |this: &mut Self, cx| this.apply_color(true, cx),
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
            stroke_color,
            fill_color,
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
    }
    /// The event that applies `patch`, unless it changes nothing or the
    /// markup cannot be edited.
    fn patch_event(&self, patch: VertexPathPropertyPatch) -> Option<VertexPathPropertyEvent> {
        let snapshot = self.snapshot.as_ref()?;
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, VertexPathPropertyPatch::Locked(_)))
            || patch_matches(&patch, snapshot)
        {
            return None;
        }
        Some(VertexPathPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            expected_kind: snapshot.kind,
            patch,
        })
    }

    fn emit_patch(&mut self, patch: VertexPathPropertyPatch, cx: &mut Context<Self>) {
        if let Some(event) = self.patch_event(patch) {
            cx.emit(event);
        }
    }

    /// Shows `patch` on the canvas while a slider is dragged.
    fn preview_patch(&mut self, patch: VertexPathPropertyPatch, cx: &mut Context<Self>) {
        cx.emit(match self.patch_event(patch) {
            Some(event) => PropertyPreviewEvent::Show(event),
            None => PropertyPreviewEvent::Clear,
        });
    }

    /// Shows the committed markup again, as a drag ends.
    fn end_preview(&mut self, cx: &mut Context<Self>) {
        cx.emit(PropertyPreviewEvent::<VertexPathPropertyEvent>::Clear);
    }

    /// A typed value: shown on the markup as it is typed, committed when the
    /// field is left or Enter is pressed.
    fn apply_field(
        &mut self,
        patch: VertexPathPropertyPatch,
        phase: FieldPhase,
        cx: &mut Context<Self>,
    ) {
        match phase {
            FieldPhase::Preview => self.preview_patch(patch, cx),
            FieldPhase::Commit => {
                self.end_preview(cx);
                self.emit_patch(patch, cx);
            }
        }
    }

    /// Typed text that is not a valid value: a partial one keeps the last
    /// preview, and leaving the field ends it.
    fn field_unparsed(&mut self, phase: FieldPhase, cx: &mut Context<Self>) {
        match phase {
            FieldPhase::Preview => {
                cx.emit(PropertyPreviewEvent::<VertexPathPropertyEvent>::Pending)
            }
            FieldPhase::Commit => self.end_preview(cx),
        }
    }
}
impl EventEmitter<VertexPathPropertyEvent> for VertexPathPropertyInspector {}
impl EventEmitter<PropertyPreviewEvent<VertexPathPropertyEvent>> for VertexPathPropertyInspector {}

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
        let weak = cx.entity().downgrade();
        let content_only = self.embedded;
        let mut lock = Some({
            let weak = weak.clone();
            property_lock_toggle(
                VERTEX_PATH_INSPECTOR_LOCKED_ID,
                snapshot.locked,
                snapshot.mutation_disabled,
                move |locked, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.emit_patch(VertexPathPropertyPatch::Locked(locked), cx)
                    });
                },
            )
        });
        let mut top = PropertyGroup::new();
        if content_only && let Some(lock) = lock.take() {
            // No header here, so the lock sits with the other rows.
            top = top.row(property_row("Locked", lock));
        }
        let mut appearance = PropertyGroup::new()
            .row(property_row(
                "Line colour",
                PropertyColor::new(
                    VERTEX_PATH_INSPECTOR_STROKE_COLOR_ID,
                    "Line colour",
                    &self.stroke_color,
                )
                .disabled(disabled),
            ))
            .row(property_row(
                "Line width",
                PropertyStepInput::new(VERTEX_PATH_INSPECTOR_WIDTH_ID, &self.width)
                    .suffix("pt")
                    .disabled(disabled),
            ));
        appearance = appearance.row(property_slider_row(
            "Opacity",
            PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                .slider_id(VERTEX_PATH_INSPECTOR_OPACITY_ID)
                .input_id(VERTEX_PATH_INSPECTOR_OPACITY_INPUT_ID)
                .suffix("%")
                .disabled(disabled),
        ));
        if snapshot.kind.supports_fill() {
            let has_fill = snapshot.appearance.fill_color().is_some();
            let weak = weak.clone();
            appearance = appearance.row(property_row(
                "Fill",
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .id(VERTEX_PATH_INSPECTOR_NO_FILL_ID)
                            .debug_selector(|| VERTEX_PATH_INSPECTOR_NO_FILL_ID.into())
                            .flex_none()
                            .child(
                                Switch::new("vertex-path-property-inspector-fill-switch")
                                    .checked(has_fill)
                                    .disabled(disabled)
                                    .tooltip(if has_fill {
                                        "Remove the fill"
                                    } else {
                                        "Fill it"
                                    })
                                    .on_click(move |fill, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            if *fill {
                                                this.apply_color(true, cx)
                                            } else {
                                                this.emit_patch(
                                                    VertexPathPropertyPatch::FillColor(None),
                                                    cx,
                                                )
                                            }
                                        });
                                    }),
                            ),
                    )
                    // Picking a colour fills it.
                    .child(
                        PropertyColor::new(
                            VERTEX_PATH_INSPECTOR_FILL_COLOR_ID,
                            "Fill colour",
                            &self.fill_color,
                        )
                        .disabled(disabled),
                    ),
            ));
        }
        let mut panel = PropertyInspectorPanel::new(
            VERTEX_PATH_PROPERTY_INSPECTOR_ID,
            VERTEX_PATH_INSPECTOR_HEADER_ID,
            VERTEX_PATH_INSPECTOR_SCROLL_ID,
            snapshot.kind.label(),
        )
        .content_only(content_only);
        if let Some(lock) = lock {
            panel = panel.header_lock(lock);
        }
        panel
            .child(
                property_body()
                    .children((!top.is_empty()).then_some(top))
                    .child(appearance),
            )
            .into_any_element()
    }
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
