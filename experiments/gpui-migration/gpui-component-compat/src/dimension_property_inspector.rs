//! Retained core-property presentation for one exact selected Dimension.
//!
//! The workspace owns document authority, annotation identity, revision validation, and history.

use butter_paper_gpui_gallery::annotation_model::{
    DimensionAppearance, MarkupId, StraightLineAppearance, TextBoxStyle,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, StyledExt as _, accordion::Accordion, button::Button,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState}, form::Field, h_flex,
    input::{InputEvent, InputState, NumberInput}, scroll::ScrollableElement as _,
    slider::{Slider, SliderEvent, SliderState}, switch::Switch, try_parse_color, v_flex,
};

use crate::document_workspace::DocumentId;

pub const DIMENSION_PROPERTY_INSPECTOR_ID: &str = "dimension-property-inspector";
pub const DIMENSION_INSPECTOR_LOCKED_ID: &str = "dimension-property-inspector-locked";
pub const DIMENSION_INSPECTOR_OFFSET_ID: &str = "dimension-property-offset";
pub const DIMENSION_INSPECTOR_WIDTH_ID: &str = "dimension-property-stroke-width";
pub const DIMENSION_INSPECTOR_FONT_SIZE_ID: &str = "dimension-property-font-size";
pub const DIMENSION_INSPECTOR_OPACITY_ID: &str = "dimension-property-opacity";
pub const DIMENSION_INSPECTOR_STROKE_COLOR_ID: &str = "dimension-property-stroke-color";
pub const DIMENSION_INSPECTOR_TEXT_COLOR_ID: &str = "dimension-property-text-color";

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub offset_pt: f64,
    pub appearance: DimensionAppearance,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DimensionPropertyPatch {
    Locked(bool),
    OffsetPt(f64),
    Appearance(DimensionAppearance),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub patch: DimensionPropertyPatch,
}

pub struct DimensionPropertyInspector {
    snapshot: Option<DimensionPropertySnapshot>,
    syncing: bool,
    open: bool,
    offset: gpui::Entity<InputState>,
    width: gpui::Entity<InputState>,
    font_size: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    stroke_color: gpui::Entity<ColorPickerState>,
    text_color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl DimensionPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let offset = cx.new(|cx| InputState::new(window, cx).default_value("24").step(1.));
        let width = cx.new(|cx| InputState::new(window, cx).default_value("1").min(0.25).max(24.).step(0.25));
        let font_size = cx.new(|cx| InputState::new(window, cx).default_value("12").min(6.).max(72.).step(1.));
        let opacity = cx.new(|_| SliderState::new().min(0.).max(100.).step(1.).default_value(100.));
        let stroke_color = cx.new(|cx| ColorPickerState::new(window, cx).default_value(try_parse_color("#ff0000").expect("default Dimension stroke color parses")));
        let text_color = cx.new(|cx| ColorPickerState::new(window, cx).default_value(try_parse_color("#ff0000").expect("default Dimension text color parses")));
        let subscriptions = vec![
            numeric_subscription(&offset, window, cx, NumericProperty::Offset),
            numeric_subscription(&width, window, cx, NumericProperty::Width),
            numeric_subscription(&font_size, window, cx, NumericProperty::FontSize),
            cx.subscribe(&opacity, |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) if !this.syncing => cx.notify(),
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => this.edit_appearance(cx, |line, text| {
                    rebuild_appearance(line, text, None, None, None, Some(value.start() as f64 / 100.), None)
                }),
            }),
            cx.subscribe(&stroke_color, |this, _, _: &ColorPickerEvent, cx| if !this.syncing { cx.notify(); }),
            cx.subscribe(&text_color, |this, _, _: &ColorPickerEvent, cx| if !this.syncing { cx.notify(); }),
        ];
        Self { snapshot: None, syncing: false, open: false, offset, width, font_size, opacity, stroke_color, text_color, _subscriptions: subscriptions }
    }

    pub fn snapshot(&self) -> Option<&DimensionPropertySnapshot> { self.snapshot.as_ref() }
    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) { if self.open != open { self.open = open; cx.notify(); } }
    pub fn clear(&mut self, cx: &mut Context<Self>) { if self.snapshot.take().is_some() { self.open = false; cx.notify(); } }
    pub fn sync(&mut self, snapshot: DimensionPropertySnapshot, window: &mut Window, cx: &mut Context<Self>) {
        self.syncing = true;
        for (input, value) in [(&self.offset, snapshot.offset_pt), (&self.width, snapshot.appearance.line().stroke_width_pt()), (&self.font_size, snapshot.appearance.text().font_size_pt())] {
            input.update(cx, |input, cx| input.set_value(format_number(value), window, cx));
        }
        self.opacity.update(cx, |slider, cx| slider.set_value((snapshot.appearance.line().opacity() * 100.) as f32, window, cx));
        if let Ok(color) = try_parse_color(snapshot.appearance.line().stroke_color()) { self.stroke_color.update(cx, |picker, cx| picker.set_value(color, window, cx)); }
        if let Ok(color) = try_parse_color(snapshot.appearance.text().color()) { self.text_color.update(cx, |picker, cx| picker.set_value(color, window, cx)); }
        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }

    fn apply_color(&mut self, text_color: bool, cx: &mut Context<Self>) {
        let state = if text_color { &self.text_color } else { &self.stroke_color };
        let Some(color) = state.read(cx).value() else { return; };
        let color = rgb_hex(color);
        self.edit_appearance(cx, |line, text| rebuild_appearance(line, text, (!text_color).then_some(color.as_str()), text_color.then_some(color.as_str()), None, None, None));
    }

    fn edit_appearance(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&StraightLineAppearance, &TextBoxStyle) -> Option<DimensionAppearance>) {
        let Some(snapshot) = self.snapshot.as_ref() else { return; };
        if let Some(appearance) = edit(snapshot.appearance.line(), snapshot.appearance.text()) { self.emit_patch(DimensionPropertyPatch::Appearance(appearance), cx); }
    }

    fn emit_patch(&mut self, patch: DimensionPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else { return; };
        if self.syncing || snapshot.mutation_disabled || (snapshot.locked && !matches!(patch, DimensionPropertyPatch::Locked(_))) || patch_matches(&patch, snapshot) { return; }
        cx.emit(DimensionPropertyEvent { document_id: snapshot.document_id, annotation_id: snapshot.annotation_id.clone(), expected_revision: snapshot.expected_revision, patch });
    }
}

impl EventEmitter<DimensionPropertyEvent> for DimensionPropertyInspector {}

impl Render for DimensionPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open) else { return div().id(DIMENSION_PROPERTY_INSPECTOR_ID).hidden().into_any_element(); };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let lock = cx.entity().downgrade();
        let stroke = cx.entity().downgrade();
        let text = cx.entity().downgrade();
        let details = Field::new().label("Locked").child(div().id(DIMENSION_INSPECTOR_LOCKED_ID).child(Switch::new("dimension-property-locked-switch").label("Locked").checked(snapshot.locked).disabled(snapshot.mutation_disabled).on_click(move |value, _, cx| { let _ = lock.update(cx, |this, cx| this.emit_patch(DimensionPropertyPatch::Locked(*value), cx)); })));
        let appearance = v_flex().gap_3()
            .child(Field::new().label("Offset").child(div().id(DIMENSION_INSPECTOR_OFFSET_ID).child(NumberInput::new(&self.offset).suffix("pt").disabled(disabled))))
            .child(color_field(DIMENSION_INSPECTOR_STROKE_COLOR_ID, "Stroke color", &self.stroke_color, disabled, stroke, false, cx))
            .child(color_field(DIMENSION_INSPECTOR_TEXT_COLOR_ID, "Text color", &self.text_color, disabled, text, true, cx))
            .child(Field::new().label("Stroke width").child(div().id(DIMENSION_INSPECTOR_WIDTH_ID).child(NumberInput::new(&self.width).suffix("pt").disabled(disabled))))
            .child(Field::new().label("Font size").child(div().id(DIMENSION_INSPECTOR_FONT_SIZE_ID).child(NumberInput::new(&self.font_size).suffix("pt").disabled(disabled))))
            .child(Field::new().label("Opacity").child(h_flex().gap_2().child(div().id(DIMENSION_INSPECTOR_OPACITY_ID).flex_1().child(Slider::new(&self.opacity).disabled(disabled))).child(format!("{:.0}%", self.opacity.read(cx).value().start()))));
        v_flex().id(DIMENSION_PROPERTY_INSPECTOR_ID).w_full().h_full().min_h_0().flex_none().border_l_1().border_color(cx.theme().border).bg(cx.theme().background)
            .child(div().px_3().py_2().font_semibold().child("Dimension properties"))
            .child(div().h_full().min_h_0().p_3().overflow_y_scrollbar().child(Accordion::new("dimension-property-sections").multiple(true).bordered(false).item(|item| item.open(true).title("Details").child(details)).item(|item| item.open(true).title("Appearance").child(appearance))))
            .into_any_element()
    }
}

#[derive(Clone, Copy)] enum NumericProperty { Offset, Width, FontSize }

fn numeric_subscription(input: &gpui::Entity<InputState>, window: &mut Window, cx: &mut Context<DimensionPropertyInspector>, property: NumericProperty) -> Subscription {
    cx.subscribe_in(input, window, move |this, input, event: &InputEvent, _, cx| {
        if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) { return; }
        let Ok(value) = input.read(cx).value().parse::<f64>() else { return; };
        if !value.is_finite() { return; }
        match property {
            NumericProperty::Offset => this.emit_patch(DimensionPropertyPatch::OffsetPt(value), cx),
            NumericProperty::Width if (0.25..=24.).contains(&value) => this.edit_appearance(cx, |line, text| rebuild_appearance(line, text, None, None, Some(value), None, None)),
            NumericProperty::FontSize if (6. ..=72.).contains(&value) => this.edit_appearance(cx, |line, text| rebuild_appearance(line, text, None, None, None, None, Some(value))),
            _ => {}
        }
    })
}

fn color_field(id: &'static str, label: &'static str, state: &gpui::Entity<ColorPickerState>, disabled: bool, inspector: gpui::WeakEntity<DimensionPropertyInspector>, text_color: bool, cx: &App) -> AnyElement {
    let button_id = if text_color { "dimension-apply-text-color" } else { "dimension-apply-stroke-color" };
    let picker = if disabled {
        let value = state.read(cx).value().map(rgb_hex).unwrap_or_else(|| "Unavailable".into());
        Button::new(button_id).label(format!("{label}: {value}")).disabled(true).into_any_element()
    } else {
        ColorPicker::new(state).label(label).into_any_element()
    };
    let apply = (!disabled).then(|| Button::new(button_id).label("Apply color").on_click(move |_, _, cx| { let _ = inspector.update(cx, |this, cx| this.apply_color(text_color, cx)); }));
    Field::new().label(label).child(div().id(id).child(picker).children(apply)).into_any_element()
}

fn rebuild_appearance(line: &StraightLineAppearance, text: &TextBoxStyle, stroke_color: Option<&str>, text_color: Option<&str>, width: Option<f64>, opacity: Option<f64>, font_size: Option<f64>) -> Option<DimensionAppearance> {
    let opacity = opacity.unwrap_or(line.opacity());
    let line = StraightLineAppearance::new(stroke_color.unwrap_or(line.stroke_color()), width.unwrap_or(line.stroke_width_pt()), opacity, line.stroke_style()).ok()?;
    let text = TextBoxStyle::new(text.font_family(), font_size.unwrap_or(text.font_size_pt()), text_color.unwrap_or(text.color()), opacity).and_then(|style| style.with_weight_and_alignment(text.weight(), text.alignment())).ok()?;
    DimensionAppearance::new(line, text).ok()
}

fn patch_matches(patch: &DimensionPropertyPatch, snapshot: &DimensionPropertySnapshot) -> bool { match patch { DimensionPropertyPatch::Locked(value) => *value == snapshot.locked, DimensionPropertyPatch::OffsetPt(value) => *value == snapshot.offset_pt, DimensionPropertyPatch::Appearance(value) => value == &snapshot.appearance } }
fn format_number(value: f64) -> String { if value.fract().abs() <= f64::EPSILON { format!("{value:.0}") } else { format!("{value:.6}").trim_end_matches('0').trim_end_matches('.').into() } }
fn rgb_hex(color: gpui::Hsla) -> String { let rgb = gpui::Rgba::from(color); format!("#{:02x}{:02x}{:02x}", (rgb.r * 255.).round() as u8, (rgb.g * 255.).round() as u8, (rgb.b * 255.).round() as u8) }
