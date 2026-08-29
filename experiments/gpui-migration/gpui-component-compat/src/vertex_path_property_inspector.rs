//! Retained property presentation for one exact selected Polyline or Polygon.
//! The workspace owns annotation authority, validation, and history.

use butter_paper_gpui_gallery::annotation_model::{
    MarkupId, MeasurementPathKind, RectangleAppearance, VertexPathKind,
};
use gpui::{AnyElement, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window, div};
use gpui_component::{ActiveTheme as _, Disableable as _, StyledExt as _, accordion::Accordion, button::Button, color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState}, form::Field, h_flex, input::{InputEvent, InputState, NumberInput}, scroll::ScrollableElement as _, slider::{Slider, SliderEvent, SliderState}, switch::Switch, try_parse_color, v_flex};

use crate::document_workspace::DocumentId;

pub const VERTEX_PATH_PROPERTY_INSPECTOR_ID: &str = "vertex-path-property-inspector";
pub const VERTEX_PATH_INSPECTOR_LOCKED_ID: &str = "vertex-path-property-inspector-locked";
pub const VERTEX_PATH_INSPECTOR_STROKE_COLOR_ID: &str = "vertex-path-property-inspector-stroke-color";
pub const VERTEX_PATH_INSPECTOR_APPLY_STROKE_ID: &str = "vertex-path-property-inspector-apply-stroke";
pub const VERTEX_PATH_INSPECTOR_WIDTH_ID: &str = "vertex-path-property-inspector-width";
pub const VERTEX_PATH_INSPECTOR_OPACITY_ID: &str = "vertex-path-property-inspector-opacity";
pub const VERTEX_PATH_INSPECTOR_FILL_COLOR_ID: &str = "vertex-path-property-inspector-fill-color";
pub const VERTEX_PATH_INSPECTOR_APPLY_FILL_ID: &str = "vertex-path-property-inspector-apply-fill";
pub const VERTEX_PATH_INSPECTOR_NO_FILL_ID: &str = "vertex-path-property-inspector-no-fill";

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
pub enum VertexPathPropertyPatch { Locked(bool), StrokeColor(String), StrokeWidthPt(f64), Opacity(f64), FillColor(Option<String>) }

#[derive(Clone, Debug, PartialEq)]
pub struct VertexPathPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub expected_kind: PathPropertyKind,
    pub patch: VertexPathPropertyPatch,
}

pub struct VertexPathPropertyInspector {
    snapshot: Option<VertexPathPropertySnapshot>, syncing: bool, open: bool,
    open_sections: [bool; 2], width: gpui::Entity<InputState>, opacity: gpui::Entity<SliderState>,
    stroke_color: gpui::Entity<ColorPickerState>, fill_color: gpui::Entity<ColorPickerState>,
    fill_preview_available: bool,
    _subscriptions: Vec<Subscription>,
}

impl VertexPathPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|cx| InputState::new(window, cx).default_value("1").min(0.25).max(24.).step(0.25));
        let opacity = cx.new(|_| SliderState::new().min(0.).max(100.).step(1.).default_value(100.));
        let stroke_color = cx.new(|cx| ColorPickerState::new(window, cx).default_value(try_parse_color("#ff0000").unwrap()));
        let fill_color = cx.new(|cx| ColorPickerState::new(window, cx).default_value(try_parse_color("#ffffff").unwrap()));
        let subscriptions = vec![
            cx.subscribe_in(&width, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) { return; }
                let Ok(value) = input.read(cx).value().parse::<f64>() else { return; };
                if value.is_finite() && (0.25..=24.).contains(&value) { this.emit_patch(VertexPathPropertyPatch::StrokeWidthPt(value), cx); }
            }),
            cx.subscribe(&opacity, |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) if !this.syncing => cx.notify(), SliderEvent::Change(_) => {},
                SliderEvent::Release(value) => this.emit_patch(VertexPathPropertyPatch::Opacity(value.start() as f64 / 100.), cx),
            }),
            cx.subscribe(&stroke_color, |this, _, _: &ColorPickerEvent, cx| if !this.syncing { cx.notify(); }),
            cx.subscribe(&fill_color, |this, _, _: &ColorPickerEvent, cx| if !this.syncing { this.fill_preview_available = true; cx.notify(); }),
        ];
        Self { snapshot: None, syncing: false, open: false, open_sections: [true, true], width, opacity, stroke_color, fill_color, fill_preview_available: false, _subscriptions: subscriptions }
    }
    pub fn snapshot(&self) -> Option<&VertexPathPropertySnapshot> { self.snapshot.as_ref() }
    pub fn width_input(&self) -> gpui::Entity<InputState> { self.width.clone() }
    pub fn opacity_slider(&self) -> gpui::Entity<SliderState> { self.opacity.clone() }
    pub fn stroke_color_picker(&self) -> gpui::Entity<ColorPickerState> { self.stroke_color.clone() }
    pub fn fill_color_picker(&self) -> gpui::Entity<ColorPickerState> { self.fill_color.clone() }
    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) { if self.open != open { self.open = open; cx.notify(); } }
    pub fn clear(&mut self, cx: &mut Context<Self>) { if self.snapshot.take().is_some() { self.open = false; cx.notify(); } }
    pub fn sync(&mut self, snapshot: VertexPathPropertySnapshot, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        self.width.update(cx, |state, cx| state.set_value(format_number(snapshot.appearance.stroke_width_pt()), window, cx));
        if let Ok(value) = try_parse_color(snapshot.appearance.stroke_color()) { self.stroke_color.update(cx, |state, cx| state.set_value(value, window, cx)); }
        let fill = snapshot.appearance.fill_color().and_then(|value| try_parse_color(value).ok()).unwrap_or_else(|| try_parse_color("#ffffff").expect("the reset fill preview must parse"));
        self.fill_color.update(cx, |state, cx| state.set_value(fill, window, cx));
        self.fill_preview_available = false;
        self.opacity.update(cx, |state, cx| state.set_value((snapshot.appearance.opacity() * 100.) as f32, window, cx));
        self.snapshot = Some(snapshot); self.syncing = false; cx.notify();
    }
    fn apply_color(&mut self, fill: bool, cx: &mut Context<Self>) {
        if fill && !self.fill_preview_available { return; }
        let state = if fill { &self.fill_color } else { &self.stroke_color };
        let Some(color) = state.read(cx).value() else { return; };
        let value = rgb_hex(color);
        self.emit_patch(if fill { VertexPathPropertyPatch::FillColor(Some(value)) } else { VertexPathPropertyPatch::StrokeColor(value) }, cx);
        if fill { self.fill_preview_available = false; }
    }
    fn emit_patch(&mut self, patch: VertexPathPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else { return; };
        if self.syncing || snapshot.mutation_disabled || (snapshot.locked && !matches!(patch, VertexPathPropertyPatch::Locked(_))) || patch_matches(&patch, snapshot) { return; }
        cx.emit(VertexPathPropertyEvent { document_id: snapshot.document_id, annotation_id: snapshot.annotation_id.clone(), expected_revision: snapshot.expected_revision, expected_kind: snapshot.kind, patch });
    }
}
impl EventEmitter<VertexPathPropertyEvent> for VertexPathPropertyInspector {}

impl Render for VertexPathPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open) else { return div().id(VERTEX_PATH_PROPERTY_INSPECTOR_ID).debug_selector(|| VERTEX_PATH_PROPERTY_INSPECTOR_ID.into()).hidden().into_any_element(); };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let weak = cx.entity().downgrade(); let weak_lock = weak.clone(); let weak_stroke = weak.clone(); let weak_fill = weak.clone(); let weak_none = weak.clone(); let weak_sections = weak.clone();
        let details = v_flex()
            .gap_3()
            .child(Field::new().label("Type").child(div().child(
                snapshot.kind.label(),
            )))
            .child(Field::new().label("Locked").child(
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
                                    this.emit_patch(VertexPathPropertyPatch::Locked(*locked), cx)
                                });
                            }),
                    ),
            ));
        let opacity_value = self.opacity.read(cx).value().start();
        let mut appearance = v_flex().gap_3()
            .child(color_control(VERTEX_PATH_INSPECTOR_STROKE_COLOR_ID, "Stroke", &self.stroke_color, disabled))
            .child(div().id(VERTEX_PATH_INSPECTOR_APPLY_STROKE_ID).debug_selector(|| VERTEX_PATH_INSPECTOR_APPLY_STROKE_ID.into()).child(Button::new("vertex-path-apply-stroke-button").label("Apply").disabled(disabled).on_click(move |_, _, cx| { let _ = weak_stroke.update(cx, |this, cx| this.apply_color(false, cx)); })))
            .child(Field::new().label("Line Width").child(div().id(VERTEX_PATH_INSPECTOR_WIDTH_ID).debug_selector(|| VERTEX_PATH_INSPECTOR_WIDTH_ID.into()).child(NumberInput::new(&self.width).suffix("pt").disabled(disabled))))
            .child(Field::new().label("Opacity").child(h_flex().gap_2().child(div().id(VERTEX_PATH_INSPECTOR_OPACITY_ID).debug_selector(|| VERTEX_PATH_INSPECTOR_OPACITY_ID.into()).flex_1().child(Slider::new(&self.opacity).disabled(disabled))).child(div().w_12().text_right().text_sm().child(format!("{opacity_value:.0}%")))));
        if snapshot.kind.supports_fill() {
            appearance = appearance.child(color_control(VERTEX_PATH_INSPECTOR_FILL_COLOR_ID, "Fill", &self.fill_color, disabled))
                .child(div().id(VERTEX_PATH_INSPECTOR_APPLY_FILL_ID).debug_selector(|| VERTEX_PATH_INSPECTOR_APPLY_FILL_ID.into()).child(Button::new("vertex-path-apply-fill-button").label("Apply").disabled(disabled).on_click(move |_, _, cx| { let _ = weak_fill.update(cx, |this, cx| this.apply_color(true, cx)); })))
                .child(Button::new(VERTEX_PATH_INSPECTOR_NO_FILL_ID).debug_selector(|| VERTEX_PATH_INSPECTOR_NO_FILL_ID.into()).label("No fill").disabled(disabled).on_click(move |_, _, cx| { let _ = weak_none.update(cx, |this, cx| this.emit_patch(VertexPathPropertyPatch::FillColor(None), cx)); }));
        }
        let open = self.open_sections;
        let accordion = Accordion::new("vertex-path-property-inspector-sections").multiple(true).bordered(false).item(|item| item.open(open[0]).title("Details").child(details)).item(|item| item.open(open[1]).title("Appearance").child(appearance)).on_toggle_click(move |open, _, cx| { let _ = weak_sections.update(cx, |this, cx| { this.open_sections = [open.contains(&0), open.contains(&1)]; cx.notify(); }); });
        v_flex().id(VERTEX_PATH_PROPERTY_INSPECTOR_ID).debug_selector(|| VERTEX_PATH_PROPERTY_INSPECTOR_ID.into()).w_full().h_full().min_h_0().flex_none().border_l_1().border_color(cx.theme().border).bg(cx.theme().background).child(div().px_3().py_2().font_semibold().child(format!("{} properties", snapshot.kind.label()))).child(div().h_full().min_h_0().p_3().overflow_y_scrollbar().child(accordion)).into_any_element()
    }
}

fn color_control(id: &'static str, label: &'static str, state: &gpui::Entity<ColorPickerState>, disabled: bool) -> AnyElement {
    Field::new().label(label).child(if disabled { div().w_full().px_2().py_1().border_1().opacity(0.5).child("Color locked").into_any_element() } else { div().id(id).debug_selector(move || id.into()).child(ColorPicker::new(state).label(label)).into_any_element() }).into_any_element()
}
fn patch_matches(patch: &VertexPathPropertyPatch, snapshot: &VertexPathPropertySnapshot) -> bool { match patch { VertexPathPropertyPatch::Locked(v) => snapshot.locked == *v, VertexPathPropertyPatch::StrokeColor(v) => snapshot.appearance.stroke_color() == v, VertexPathPropertyPatch::StrokeWidthPt(v) => snapshot.appearance.stroke_width_pt() == *v, VertexPathPropertyPatch::Opacity(v) => snapshot.appearance.opacity() == *v, VertexPathPropertyPatch::FillColor(v) => snapshot.appearance.fill_color() == v.as_deref() } }
fn rgb_hex(color: gpui::Hsla) -> String { let rgb = gpui::Rgba::from(color); format!("#{:02x}{:02x}{:02x}", (rgb.r * 255.).round() as u8, (rgb.g * 255.).round() as u8, (rgb.b * 255.).round() as u8) }
fn format_number(value: f64) -> String { if value.fract() == 0. { format!("{value:.0}") } else { format!("{value}") } }
