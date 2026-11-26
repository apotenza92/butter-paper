//! Retained presentation state for one exact selected Callout or Cloud+.
//!
//! The panel shows the markup's text, line, font, (for a Cloud+) fill,
//! cloud intensity and landing length, and opacity, laid out as the Dimension and Text Box
//! panels are. The workspace owns document authority, annotation identity,
//! revision validation, and history.

use crate::annotation_model::{
    CalloutAppearance, CloudPlusAppearance, MarkupId, RectangleAppearance, StraightLineAppearance,
    TextBoxStyle,
};
use crate::cloud_plus_routing::CLOUD_PLUS_MAX_LANDING_PT;
use crate::property_controls::{
    PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
    PropertySliderInput, PropertyTextEditEvent, PropertyTextField, canonical_picker_opacity,
    format_property_geometry, format_property_number, format_property_percentage,
    parse_property_percentage, property_body, property_color_and_size, property_lock_toggle,
    property_number, property_row, property_slider_row, subscribe_property_color,
    subscribe_property_slider,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    Disableable as _,
    color_picker::ColorPickerState,
    h_flex,
    input::{InputEvent, InputState, TextareaState},
    slider::SliderState,
    switch::Switch,
    try_parse_color,
};

use crate::document_workspace::DocumentId;

pub const CAPTION_PROPERTY_INSPECTOR_ID: &str = "caption-property-inspector";
pub const CAPTION_INSPECTOR_LOCKED_ID: &str = "caption-property-locked";
pub const CAPTION_INSPECTOR_TEXT_ID: &str = "caption-property-text";
pub const CAPTION_INSPECTOR_LINE_COLOR_ID: &str = "caption-property-line-color";
pub const CAPTION_INSPECTOR_LINE_WIDTH_ID: &str = "caption-property-line-width";
pub const CAPTION_INSPECTOR_TEXT_COLOR_ID: &str = "caption-property-text-color";
pub const CAPTION_INSPECTOR_FONT_SIZE_ID: &str = "caption-property-font-size";
pub const CAPTION_INSPECTOR_FILL_ID: &str = "caption-property-fill";
pub const CAPTION_INSPECTOR_FILL_ENABLED_ID: &str = "caption-property-fill-enabled";
pub const CAPTION_INSPECTOR_FILL_COLOR_ID: &str = "caption-property-fill-color";
pub const CAPTION_INSPECTOR_FILL_OPACITY_ID: &str = "caption-property-fill-opacity";
pub const CAPTION_INSPECTOR_INTENSITY_ID: &str = "caption-property-intensity";
pub const CAPTION_INSPECTOR_OPACITY_ID: &str = "caption-property-opacity";
pub const CAPTION_INSPECTOR_LANDING_ID: &str = "caption-property-landing";
const CAPTION_INSPECTOR_HEADER_ID: &str = "caption-property-inspector-header";
const CAPTION_INSPECTOR_SCROLL_ID: &str = "caption-property-inspector-scroll";
const CAPTION_INSPECTOR_FILL_SWITCH_ID: &str = "caption-property-fill-enabled-switch";
const CAPTION_INSPECTOR_FILL_OPACITY_SLIDER_ID: &str = "caption-property-fill-opacity-slider";
const CAPTION_INSPECTOR_FILL_OPACITY_INPUT_ID: &str = "caption-property-fill-opacity-input";
const CAPTION_INSPECTOR_INTENSITY_SLIDER_ID: &str = "caption-property-intensity-slider";
const CAPTION_INSPECTOR_INTENSITY_INPUT_ID: &str = "caption-property-intensity-input";
const CAPTION_INSPECTOR_OPACITY_SLIDER_ID: &str = "caption-property-opacity-slider";
const CAPTION_INSPECTOR_OPACITY_INPUT_ID: &str = "caption-property-opacity-input";

const FALLBACK_FILL_COLOUR: &str = "#ffffff";

/// What the panel edits on a Callout or a Cloud+.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptionPropertyValues {
    Callout(CalloutAppearance),
    CloudPlus {
        appearance: CloudPlusAppearance,
        intensity: f64,
        /// The length of the leader's level landing at the text.
        landing_pt: f64,
    },
}

impl CaptionPropertyValues {
    fn line(&self) -> (&str, f64, f64) {
        match self {
            Self::Callout(appearance) => (
                appearance.line().stroke_color(),
                appearance.line().stroke_width_pt(),
                appearance.line().opacity(),
            ),
            Self::CloudPlus { appearance, .. } => (
                appearance.cloud().stroke_color(),
                appearance.cloud().stroke_width_pt(),
                appearance.cloud().opacity(),
            ),
        }
    }

    fn text(&self) -> &TextBoxStyle {
        match self {
            Self::Callout(appearance) => appearance.text(),
            Self::CloudPlus { appearance, .. } => appearance.text(),
        }
    }

    fn opacity(&self) -> f64 {
        self.line().2
    }

    /// A Cloud+'s fill colour and fill opacity; a Callout has none.
    fn fill(&self) -> Option<(Option<&str>, f64)> {
        match self {
            Self::Callout(_) => None,
            Self::CloudPlus { appearance, .. } => Some((
                appearance.cloud().fill_color(),
                appearance.cloud().fill_opacity(),
            )),
        }
    }

    fn intensity(&self) -> Option<f64> {
        match self {
            Self::Callout(_) => None,
            Self::CloudPlus { intensity, .. } => Some(*intensity),
        }
    }

    fn landing_pt(&self) -> Option<f64> {
        match self {
            Self::Callout(_) => None,
            Self::CloudPlus { landing_pt, .. } => Some(*landing_pt),
        }
    }

    pub fn is_cloud_plus(&self) -> bool {
        matches!(self, Self::CloudPlus { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub values: CaptionPropertyValues,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CaptionPropertyPatch {
    Locked(bool),
    Values(Box<CaptionPropertyValues>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub patch: CaptionPropertyPatch,
}

/// One change to a Callout's or Cloud+'s style; `None` keeps a value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptionStyleEdit {
    pub line_color: Option<String>,
    pub line_width: Option<f64>,
    pub text_color: Option<String>,
    pub font_size: Option<f64>,
    pub opacity: Option<f64>,
    /// `Some(None)` removes a Cloud+'s fill.
    pub fill_color: Option<Option<String>>,
    pub fill_opacity: Option<f64>,
    pub intensity: Option<f64>,
    /// A Cloud+'s landing length, from 0 to `CLOUD_PLUS_MAX_LANDING_PT`.
    pub landing_pt: Option<f64>,
}

/// The values with `edit` applied, keeping everything it leaves alone,
/// including imported text metrics and stroke styles; `None` when the result
/// is not a valid style.
pub fn edit_caption_values(
    values: &CaptionPropertyValues,
    edit: &CaptionStyleEdit,
) -> Option<CaptionPropertyValues> {
    let (line_color, line_width, opacity) = values.line();
    let line_color = edit.line_color.as_deref().unwrap_or(line_color);
    let line_width = edit.line_width.unwrap_or(line_width);
    let opacity = edit.opacity.unwrap_or(opacity);
    let current_text = values.text();
    let text = TextBoxStyle::new(
        current_text.font_family(),
        edit.font_size.unwrap_or(current_text.font_size_pt()),
        edit.text_color.as_deref().unwrap_or(current_text.color()),
        opacity,
    )
    .and_then(|style| {
        style.with_weight_and_alignment(current_text.weight(), current_text.alignment())
    })
    .and_then(|style| {
        style.with_layout_metrics(current_text.line_height_pt(), current_text.inset_pt())
    })
    .ok()?;
    match values {
        CaptionPropertyValues::Callout(appearance) => {
            let line = StraightLineAppearance::new(
                line_color,
                line_width,
                opacity,
                appearance.line().stroke_style(),
            )
            .ok()?;
            CalloutAppearance::new(line, text)
                .ok()
                .map(CaptionPropertyValues::Callout)
        }
        CaptionPropertyValues::CloudPlus {
            appearance,
            intensity,
            landing_pt,
        } => {
            let cloud = appearance.cloud();
            let fill_color = edit
                .fill_color
                .clone()
                .unwrap_or_else(|| cloud.fill_color().map(str::to_owned));
            let cloud = RectangleAppearance::new(line_color, line_width, fill_color, opacity)
                .and_then(|next| {
                    next.with_fill_opacity(edit.fill_opacity.unwrap_or(cloud.fill_opacity()))
                })
                .ok()?
                .with_stroke_style(cloud.stroke_style());
            let leader = StraightLineAppearance::new(
                line_color,
                line_width,
                opacity,
                appearance.leader().stroke_style(),
            )
            .ok()?;
            let intensity = edit.intensity.unwrap_or(*intensity);
            if !(0. ..=4.).contains(&intensity) {
                return None;
            }
            let landing_pt = edit.landing_pt.unwrap_or(*landing_pt);
            if !(0. ..=CLOUD_PLUS_MAX_LANDING_PT).contains(&landing_pt) {
                return None;
            }
            CloudPlusAppearance::new(cloud, leader, text)
                .ok()
                .map(|appearance| CaptionPropertyValues::CloudPlus {
                    appearance,
                    intensity,
                    landing_pt,
                })
        }
    }
}

pub struct CaptionPropertyInspector {
    snapshot: Option<CaptionPropertySnapshot>,
    syncing: bool,
    open: bool,
    embedded: bool,
    /// The markup's text as last synced, once the workspace has given it.
    content_baseline: Option<String>,
    content: gpui::Entity<TextareaState>,
    line_color: gpui::Entity<ColorPickerState>,
    line_width: gpui::Entity<InputState>,
    text_color: gpui::Entity<ColorPickerState>,
    font_size: gpui::Entity<InputState>,
    fill_color: gpui::Entity<ColorPickerState>,
    fill_opacity: gpui::Entity<SliderState>,
    fill_opacity_input: gpui::Entity<InputState>,
    intensity: gpui::Entity<SliderState>,
    intensity_input: gpui::Entity<InputState>,
    landing: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl CaptionPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let content = cx.new(|cx| TextareaState::new(window, cx).rows(2).soft_wrap(true));
        let picker = |window: &mut Window, cx: &mut Context<Self>, hex: &str| {
            let colour = try_parse_color(hex).expect("a built-in caption colour parses");
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(colour))
        };
        let line_color = picker(window, cx, "#ff0000");
        let text_color = picker(window, cx, "#ff0000");
        let fill_color = picker(window, cx, FALLBACK_FILL_COLOUR);
        let line_width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.25)
                .max(24.)
                .step(0.25)
        });
        let font_size = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("12")
                .min(6.)
                .max(72.)
                .step(1.)
        });
        let percent_slider = |cx: &mut Context<Self>| {
            cx.new(|_| {
                SliderState::new()
                    .min(0.)
                    .max(100.)
                    .step(1.)
                    .default_value(100.)
            })
        };
        let opacity = percent_slider(cx);
        let fill_opacity = percent_slider(cx);
        let intensity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(4.)
                .step(0.25)
                .default_value(1.)
        });
        let opacity_input = cx.new(|cx| InputState::new(window, cx).default_value("100"));
        let fill_opacity_input = cx.new(|cx| InputState::new(window, cx).default_value("100"));
        let intensity_input = cx.new(|cx| InputState::new(window, cx).default_value("1"));
        let landing = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format_property_geometry(
                    crate::cloud_plus_routing::CLOUD_PLUS_LANDING_PT,
                ))
                .min(0.)
                .max(CLOUD_PLUS_MAX_LANDING_PT)
        });

        let mut subscriptions = vec![
            // Enter starts a new line; leaving the field, or Cmd/Ctrl+Enter,
            // commits it.
            cx.subscribe_in(
                &content,
                window,
                |this, content, event: &InputEvent, window, cx| match event {
                    InputEvent::Blur => this.emit_content(cx),
                    InputEvent::PressEnter {
                        secondary: true, ..
                    } => {
                        // The field has already taken the Enter as a new line.
                        let value = content.read(cx).value().to_string();
                        if let Some(value) = value.strip_suffix('\n') {
                            content.update(cx, |content, cx| {
                                content.set_value(value.to_owned(), window, cx)
                            });
                        }
                        this.emit_content(cx);
                    }
                    _ => {}
                },
            ),
            subscribe_property_slider(
                &opacity,
                &opacity_input,
                window,
                cx,
                |this| !this.syncing,
                |this, value, cx| {
                    this.edit_style(
                        CaptionStyleEdit {
                            opacity: Some(value / 100.),
                            ..Default::default()
                        },
                        cx,
                    )
                },
            ),
            subscribe_property_slider(
                &fill_opacity,
                &fill_opacity_input,
                window,
                cx,
                |this| !this.syncing,
                |this, value, cx| {
                    this.edit_style(
                        CaptionStyleEdit {
                            fill_opacity: Some(value / 100.),
                            ..Default::default()
                        },
                        cx,
                    )
                },
            ),
            subscribe_property_slider(
                &intensity,
                &intensity_input,
                window,
                cx,
                |this| !this.syncing,
                |this, value, cx| {
                    this.edit_style(
                        CaptionStyleEdit {
                            intensity: Some(value),
                            ..Default::default()
                        },
                        cx,
                    )
                },
            ),
            number_subscription(&line_width, window, cx, NumberField::LineWidth),
            number_subscription(&font_size, window, cx, NumberField::FontSize),
            number_subscription(&intensity_input, window, cx, NumberField::Intensity),
            number_subscription(&landing, window, cx, NumberField::Landing),
            percent_subscription(&opacity_input, window, cx, PercentField::Opacity),
            percent_subscription(&fill_opacity_input, window, cx, PercentField::FillOpacity),
        ];
        subscriptions.extend(subscribe_property_color(
            &line_color,
            cx,
            |this: &mut Self, cx| this.apply_color(ColorField::Line, cx),
        ));
        subscriptions.extend(subscribe_property_color(
            &text_color,
            cx,
            |this: &mut Self, cx| this.apply_color(ColorField::Text, cx),
        ));
        subscriptions.extend(subscribe_property_color(
            &fill_color,
            cx,
            |this: &mut Self, cx| this.apply_color(ColorField::Fill, cx),
        ));
        Self {
            snapshot: None,
            syncing: false,
            open: false,
            embedded: false,
            content_baseline: None,
            content,
            line_color,
            line_width,
            text_color,
            font_size,
            fill_color,
            fill_opacity,
            fill_opacity_input,
            intensity,
            intensity_input,
            landing,
            opacity,
            opacity_input,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&CaptionPropertySnapshot> {
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
        self.content_baseline = None;
        if self.snapshot.take().is_some() {
            cx.notify();
        }
    }
    pub fn content_input(&self) -> gpui::Entity<TextareaState> {
        self.content.clone()
    }
    pub fn line_width_input(&self) -> gpui::Entity<InputState> {
        self.line_width.clone()
    }
    pub fn font_size_input(&self) -> gpui::Entity<InputState> {
        self.font_size.clone()
    }
    pub fn opacity_input(&self) -> gpui::Entity<InputState> {
        self.opacity_input.clone()
    }
    pub fn intensity_input(&self) -> gpui::Entity<InputState> {
        self.intensity_input.clone()
    }
    pub fn landing_input(&self) -> gpui::Entity<InputState> {
        self.landing.clone()
    }
    pub fn line_color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.line_color.clone()
    }
    pub fn fill_color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.fill_color.clone()
    }

    /// Shows the markup's text in the Text field. The workspace calls this
    /// after `sync`, and again whenever the text changes.
    pub fn sync_content(&mut self, content: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.content_baseline.as_deref() == Some(content) {
            return;
        }
        self.syncing = true;
        self.content.update(cx, |input, cx| {
            input.set_value(content.to_owned(), window, cx)
        });
        self.content_baseline = Some(content.to_owned());
        self.syncing = false;
        cx.notify();
    }

    pub fn sync(
        &mut self,
        snapshot: CaptionPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        let values = &snapshot.values;
        let (line_color, line_width, opacity) = values.line();
        for (input, value) in [
            (&self.line_width, format_property_number(line_width)),
            (
                &self.font_size,
                format_property_number(values.text().font_size_pt()),
            ),
            (&self.opacity_input, format_property_percentage(opacity)),
        ] {
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((opacity * 100.) as f32, window, cx)
        });
        for (picker, hex) in [
            (&self.line_color, line_color),
            (&self.text_color, values.text().color()),
        ] {
            if let Ok(mut color) = try_parse_color(hex) {
                color.a = opacity as f32;
                picker.update(cx, |picker, cx| picker.set_value(color, window, cx));
            }
        }
        if let Some((fill_color, fill_opacity)) = values.fill() {
            if let Some(Ok(mut color)) = fill_color.map(try_parse_color) {
                color.a = fill_opacity as f32;
                self.fill_color
                    .update(cx, |picker, cx| picker.set_value(color, window, cx));
            }
            self.fill_opacity.update(cx, |slider, cx| {
                slider.set_value((fill_opacity * 100.) as f32, window, cx)
            });
            self.fill_opacity_input.update(cx, |input, cx| {
                input.set_value(format_property_percentage(fill_opacity), window, cx)
            });
        }
        if let Some(intensity) = values.intensity() {
            self.intensity.update(cx, |slider, cx| {
                slider.set_value(intensity as f32, window, cx)
            });
            self.intensity_input.update(cx, |input, cx| {
                input.set_value(format_property_number(intensity), window, cx)
            });
        }
        if let Some(landing_pt) = values.landing_pt() {
            self.landing.update(cx, |input, cx| {
                input.set_value(format_property_geometry(landing_pt), window, cx)
            });
        }
        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }

    fn emit_content(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let content = self.content.read(cx).value().to_string();
        if self.syncing
            || snapshot.mutation_disabled
            || snapshot.locked
            || self
                .content_baseline
                .as_deref()
                .is_none_or(|baseline| baseline == content)
        {
            return;
        }
        self.content_baseline = Some(content.clone());
        cx.emit(PropertyTextEditEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            content,
        });
    }

    fn apply_color(&mut self, field: ColorField, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let state = match field {
            ColorField::Line => &self.line_color,
            ColorField::Text => &self.text_color,
            ColorField::Fill => &self.fill_color,
        };
        let Some(color) = state.read(cx).value() else {
            return;
        };
        let hex = rgb_hex(color);
        let edit = match field {
            // The picker's alpha is the markup's opacity, which its line and
            // text share.
            ColorField::Line => CaptionStyleEdit {
                line_color: Some(hex),
                opacity: Some(canonical_picker_opacity(color.a, snapshot.values.opacity())),
                ..Default::default()
            },
            ColorField::Text => CaptionStyleEdit {
                text_color: Some(hex),
                opacity: Some(canonical_picker_opacity(color.a, snapshot.values.opacity())),
                ..Default::default()
            },
            // Picking a fill colour fills it.
            ColorField::Fill => CaptionStyleEdit {
                fill_color: Some(Some(hex)),
                fill_opacity: Some(canonical_picker_opacity(
                    color.a,
                    snapshot.values.fill().map_or(1., |(_, opacity)| opacity),
                )),
                ..Default::default()
            },
        };
        self.edit_style(edit, cx);
    }

    fn set_fill_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let fill_color = enabled.then(|| {
            self.fill_color
                .read(cx)
                .value()
                .map(rgb_hex)
                .unwrap_or_else(|| FALLBACK_FILL_COLOUR.into())
        });
        self.edit_style(
            CaptionStyleEdit {
                fill_color: Some(fill_color),
                ..Default::default()
            },
            cx,
        );
    }

    fn edit_style(&mut self, edit: CaptionStyleEdit, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if let Some(values) = edit_caption_values(&snapshot.values, &edit) {
            self.emit_patch(CaptionPropertyPatch::Values(Box::new(values)), cx);
        }
    }

    fn emit_patch(&mut self, patch: CaptionPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, CaptionPropertyPatch::Locked(_)))
            || match &patch {
                CaptionPropertyPatch::Locked(value) => *value == snapshot.locked,
                CaptionPropertyPatch::Values(values) => **values == snapshot.values,
            }
        {
            return;
        }
        cx.emit(CaptionPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            patch,
        });
    }
}

impl EventEmitter<CaptionPropertyEvent> for CaptionPropertyInspector {}
impl EventEmitter<PropertyTextEditEvent> for CaptionPropertyInspector {}

impl Render for CaptionPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(CAPTION_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let mut rows = PropertyGroup::new();
        if self.content_baseline.is_some() {
            rows = rows.row(
                property_row(
                    "Text",
                    PropertyTextField::new(CAPTION_INSPECTOR_TEXT_ID, &self.content)
                        .disabled(disabled),
                )
                .align_top(),
            );
        }
        rows = rows
            .row(property_row(
                "Line",
                property_color_and_size(
                    PropertyColor::new(
                        CAPTION_INSPECTOR_LINE_COLOR_ID,
                        "Line colour",
                        &self.line_color,
                    )
                    .disabled(disabled),
                    PropertyNumericInput::new(
                        CAPTION_INSPECTOR_LINE_WIDTH_ID,
                        "Line width",
                        &self.line_width,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            ))
            .row(property_row(
                "Font",
                property_color_and_size(
                    PropertyColor::new(
                        CAPTION_INSPECTOR_TEXT_COLOR_ID,
                        "Text colour",
                        &self.text_color,
                    )
                    .disabled(disabled),
                    PropertyNumericInput::new(
                        CAPTION_INSPECTOR_FONT_SIZE_ID,
                        "Font size",
                        &self.font_size,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            ));
        rows = rows.row(property_slider_row(
            "Opacity",
            PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                .row_id(CAPTION_INSPECTOR_OPACITY_ID)
                .slider_id(CAPTION_INSPECTOR_OPACITY_SLIDER_ID)
                .input_id(CAPTION_INSPECTOR_OPACITY_INPUT_ID)
                .suffix("%")
                .disabled(disabled),
        ));
        let mut fill_on = false;
        if let Some((fill_color, _)) = snapshot.values.fill() {
            fill_on = fill_color.is_some();
            let fill = cx.entity().downgrade();
            rows = rows.row(property_row(
                "Fill",
                h_flex()
                    .id(CAPTION_INSPECTOR_FILL_ID)
                    .debug_selector(|| CAPTION_INSPECTOR_FILL_ID.into())
                    .w_full()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .id(CAPTION_INSPECTOR_FILL_ENABLED_ID)
                            .debug_selector(|| CAPTION_INSPECTOR_FILL_ENABLED_ID.into())
                            .flex_none()
                            .child(
                                Switch::new(CAPTION_INSPECTOR_FILL_SWITCH_ID)
                                    .checked(fill_on)
                                    .disabled(disabled)
                                    .tooltip(if fill_on {
                                        "Remove the fill"
                                    } else {
                                        "Fill the cloud"
                                    })
                                    .on_click(move |enabled, _, cx| {
                                        let _ = fill.update(cx, |this, cx| {
                                            this.set_fill_enabled(*enabled, cx)
                                        });
                                    }),
                            ),
                    )
                    // Picking a colour fills it.
                    .child(
                        PropertyColor::new(
                            CAPTION_INSPECTOR_FILL_COLOR_ID,
                            "Fill colour",
                            &self.fill_color,
                        )
                        .disabled(disabled),
                    ),
            ));
        }
        // A Cloud+ shows its fill opacity with no fill too, disabled, so the
        // panel keeps its shape.
        if snapshot.values.fill().is_some() {
            rows = rows.row(
                property_slider_row(
                    "Fill opacity",
                    PropertySliderInput::new(
                        "Fill opacity",
                        &self.fill_opacity,
                        &self.fill_opacity_input,
                    )
                    .row_id(CAPTION_INSPECTOR_FILL_OPACITY_ID)
                    .slider_id(CAPTION_INSPECTOR_FILL_OPACITY_SLIDER_ID)
                    .input_id(CAPTION_INSPECTOR_FILL_OPACITY_INPUT_ID)
                    .suffix("%")
                    .disabled(disabled || !fill_on),
                )
                .dependent(),
            );
        }
        if snapshot.values.intensity().is_some() {
            rows = rows.row(property_slider_row(
                "Intensity",
                PropertySliderInput::new("Cloud intensity", &self.intensity, &self.intensity_input)
                    .row_id(CAPTION_INSPECTOR_INTENSITY_ID)
                    .slider_id(CAPTION_INSPECTOR_INTENSITY_SLIDER_ID)
                    .input_id(CAPTION_INSPECTOR_INTENSITY_INPUT_ID)
                    .disabled(disabled),
            ));
        }
        if snapshot.values.landing_pt().is_some() {
            rows = rows.row(property_row(
                "Landing",
                property_number(
                    PropertyNumericInput::new(
                        CAPTION_INSPECTOR_LANDING_ID,
                        "Landing length",
                        &self.landing,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            ));
        }
        let lock = cx.entity().downgrade();
        PropertyInspectorPanel::new(
            CAPTION_PROPERTY_INSPECTOR_ID,
            CAPTION_INSPECTOR_HEADER_ID,
            CAPTION_INSPECTOR_SCROLL_ID,
            if snapshot.values.is_cloud_plus() {
                "Cloud+"
            } else {
                "Callout"
            },
        )
        .header_lock(property_lock_toggle(
            CAPTION_INSPECTOR_LOCKED_ID,
            snapshot.locked,
            snapshot.mutation_disabled,
            move |locked, _, cx| {
                let _ = lock.update(cx, |this, cx| {
                    this.emit_patch(CaptionPropertyPatch::Locked(locked), cx)
                });
            },
        ))
        .child(property_body().child(rows))
        .into_any_element()
    }
}

#[derive(Clone, Copy)]
enum ColorField {
    Line,
    Text,
    Fill,
}

#[derive(Clone, Copy)]
enum NumberField {
    LineWidth,
    FontSize,
    Intensity,
    Landing,
}

#[derive(Clone, Copy)]
enum PercentField {
    Opacity,
    FillOpacity,
}

fn number_subscription(
    input: &gpui::Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<CaptionPropertyInspector>,
    field: NumberField,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |this, input, event: &InputEvent, window, cx| {
            if this.syncing || !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                return;
            }
            let text = input.read(cx).value();
            // The landing shows rounded; its unchanged text keeps the exact
            // length.
            if matches!(field, NumberField::Landing)
                && this
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.values.landing_pt())
                    .is_some_and(|landing| text.trim() == format_property_geometry(landing))
            {
                return;
            }
            let value = text.trim().parse::<f64>().ok();
            let edit = match (field, value) {
                (NumberField::LineWidth, Some(value)) if (0.25..=24.).contains(&value) => {
                    Some(CaptionStyleEdit {
                        line_width: Some(value),
                        ..Default::default()
                    })
                }
                (NumberField::FontSize, Some(value)) if (6. ..=72.).contains(&value) => {
                    Some(CaptionStyleEdit {
                        font_size: Some(value),
                        ..Default::default()
                    })
                }
                (NumberField::Intensity, Some(value)) if (0. ..=4.).contains(&value) => {
                    Some(CaptionStyleEdit {
                        intensity: Some(value),
                        ..Default::default()
                    })
                }
                (NumberField::Landing, Some(value))
                    if (0. ..=CLOUD_PLUS_MAX_LANDING_PT).contains(&value) =>
                {
                    Some(CaptionStyleEdit {
                        landing_pt: Some(value),
                        ..Default::default()
                    })
                }
                _ => None,
            };
            match edit {
                Some(edit) => this.edit_style(edit, cx),
                // Out of range: show the markup's value again.
                None => this.resync_inputs(window, cx),
            }
        },
    )
}

fn percent_subscription(
    input: &gpui::Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<CaptionPropertyInspector>,
    field: PercentField,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |this, input, event: &InputEvent, window, cx| {
            if this.syncing || !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                return;
            }
            match parse_property_percentage(input.read(cx).value().as_ref()) {
                Some(value) => this.edit_style(
                    match field {
                        PercentField::Opacity => CaptionStyleEdit {
                            opacity: Some(value),
                            ..Default::default()
                        },
                        PercentField::FillOpacity => CaptionStyleEdit {
                            fill_opacity: Some(value),
                            ..Default::default()
                        },
                    },
                    cx,
                ),
                None => this.resync_inputs(window, cx),
            }
        },
    )
}

impl CaptionPropertyInspector {
    fn resync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.snapshot.clone() {
            self.sync(snapshot, window, cx);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_model::StrokeStyle;

    fn callout() -> CaptionPropertyValues {
        let line = StraightLineAppearance::new("#112233", 1.5, 0.8, StrokeStyle::Dashed).unwrap();
        let text = TextBoxStyle::new("Tinos", 14., "#445566", 0.8)
            .unwrap()
            .with_layout_metrics(13.8, 3.)
            .unwrap();
        CaptionPropertyValues::Callout(CalloutAppearance::new(line, text).unwrap())
    }

    fn cloud_plus() -> CaptionPropertyValues {
        let cloud = RectangleAppearance::new("#112233", 1., Some("#ffeeaa"), 0.9)
            .unwrap()
            .with_fill_opacity(0.5)
            .unwrap();
        let leader = StraightLineAppearance::new("#112233", 1., 0.9, StrokeStyle::Solid).unwrap();
        let text = TextBoxStyle::new("Arimo", 12., "#ff0000", 0.9).unwrap();
        CaptionPropertyValues::CloudPlus {
            appearance: CloudPlusAppearance::new(cloud, leader, text).unwrap(),
            intensity: 1.,
            landing_pt: 19.8,
        }
    }

    #[test]
    fn a_callout_edit_keeps_its_line_and_text_opacity_together_and_its_metrics() {
        let next = edit_caption_values(
            &callout(),
            &CaptionStyleEdit {
                opacity: Some(0.4),
                font_size: Some(20.),
                ..Default::default()
            },
        )
        .unwrap();
        let CaptionPropertyValues::Callout(appearance) = next else {
            panic!("a Callout stays a Callout");
        };
        assert_eq!(appearance.line().opacity(), 0.4);
        assert_eq!(appearance.text().opacity(), 0.4);
        assert_eq!(appearance.text().font_size_pt(), 20.);
        assert_eq!(appearance.text().line_height_pt(), 13.8);
        assert_eq!(appearance.line().stroke_style(), StrokeStyle::Dashed);
        assert_eq!(appearance.line().stroke_color(), "#112233");
    }

    #[test]
    fn a_cloud_plus_edit_keeps_its_cloud_and_leader_one_stroke() {
        let next = edit_caption_values(
            &cloud_plus(),
            &CaptionStyleEdit {
                line_color: Some("#00ff00".into()),
                line_width: Some(2.5),
                intensity: Some(2.),
                ..Default::default()
            },
        )
        .unwrap();
        let CaptionPropertyValues::CloudPlus {
            appearance,
            intensity,
            landing_pt,
        } = next
        else {
            panic!("a Cloud+ stays a Cloud+");
        };
        assert_eq!(intensity, 2.);
        assert_eq!(landing_pt, 19.8, "a style edit keeps the landing");
        assert_eq!(appearance.cloud().stroke_color(), "#00ff00");
        assert_eq!(appearance.leader().stroke_color(), "#00ff00");
        assert_eq!(appearance.cloud().stroke_width_pt(), 2.5);
        assert_eq!(appearance.leader().stroke_width_pt(), 2.5);
        assert_eq!(appearance.cloud().fill_color(), Some("#ffeeaa"));
        assert_eq!(appearance.cloud().fill_opacity(), 0.5);
    }

    #[test]
    fn a_cloud_plus_fill_turns_off_and_out_of_range_intensity_is_refused() {
        let unfilled = edit_caption_values(
            &cloud_plus(),
            &CaptionStyleEdit {
                fill_color: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(unfilled.fill(), Some((None, 0.5)));
        assert!(
            edit_caption_values(
                &cloud_plus(),
                &CaptionStyleEdit {
                    intensity: Some(5.),
                    ..Default::default()
                },
            )
            .is_none()
        );
        assert_eq!(
            edit_caption_values(&callout(), &CaptionStyleEdit::default()),
            Some(callout())
        );
    }

    #[test]
    fn a_cloud_plus_landing_edit_takes_zero_and_refuses_out_of_range() {
        let landing = |value: f64| {
            edit_caption_values(
                &cloud_plus(),
                &CaptionStyleEdit {
                    landing_pt: Some(value),
                    ..Default::default()
                },
            )
            .and_then(|values| values.landing_pt())
        };
        assert_eq!(landing(36.), Some(36.));
        assert_eq!(landing(0.), Some(0.));
        assert_eq!(landing(-1.), None);
        assert_eq!(landing(CLOUD_PLUS_MAX_LANDING_PT + 1.), None);
        assert_eq!(callout().landing_pt(), None);
    }
}
