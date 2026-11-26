//! Retained presentation for the active annotation tool's defaults.
//!
//! The workspace owns the per-document defaults. This panel retains only
//! stock control state, input drafts, disclosure state, and target-bound
//! intent events.

use crate::{
    accessible_button::close_icon_button,
    annotation_adapter::AnnotationTool,
    cloud_plus_routing::CLOUD_PLUS_MAX_LANDING_PT,
    document_workspace::{DocumentId, MeasureFamily},
    property_controls::{
        PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
        PropertyResetConfirm, PropertyResetIds, PropertySliderInput, format_property_geometry,
        format_property_number, format_property_percentage, property_body, property_color_and_size,
        property_number, property_row, property_slider_row, subscribe_property_color,
    },
    tool_properties::{ToolProperties, ToolPropertyField, ToolPropertyRange},
};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IndexPath,
    color_picker::ColorPickerState,
    h_flex,
    input::{InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    slider::{SliderEvent, SliderState},
    switch::Switch,
    try_parse_color, v_flex,
};

pub const TOOL_DEFAULTS_PANEL_ID: &str = "tool-defaults-panel";
pub const TOOL_DEFAULTS_HEADER_ID: &str = "tool-defaults-panel-header";
pub const TOOL_DEFAULTS_CLOSE_ID: &str = "tool-defaults-panel-close";
pub const TOOL_DEFAULTS_SCROLL_ID: &str = "tool-defaults-panel-scroll";
pub const TOOL_DEFAULTS_ACCORDION_ID: &str = "tool-defaults-panel-accordion";
pub const TOOL_DEFAULTS_APPEARANCE_ID: &str = "tool-defaults-panel-appearance";
pub const TOOL_DEFAULTS_COLOUR_ID: &str = "tool-defaults-panel-colour";
pub const TOOL_DEFAULTS_FILL_ID: &str = "tool-defaults-panel-fill";
pub const TOOL_DEFAULTS_FILL_PICKER_ID: &str = "tool-defaults-panel-fill-picker";
pub const TOOL_DEFAULTS_FILL_ENABLED_ID: &str = "tool-defaults-panel-fill-enabled";
pub const TOOL_DEFAULTS_WIDTH_ID: &str = "tool-defaults-panel-width";
pub const TOOL_DEFAULTS_WIDTH_SLIDER_ID: &str = "tool-defaults-panel-width-slider";
pub const TOOL_DEFAULTS_WIDTH_INPUT_ID: &str = "tool-defaults-panel-width-input";
pub const TOOL_DEFAULTS_OPACITY_ID: &str = "tool-defaults-panel-opacity";
pub const TOOL_DEFAULTS_OPACITY_SLIDER_ID: &str = "tool-defaults-panel-opacity-slider";
pub const TOOL_DEFAULTS_OPACITY_INPUT_ID: &str = "tool-defaults-panel-opacity-input";
pub const TOOL_DEFAULTS_FONT_SIZE_ID: &str = "tool-defaults-panel-font-size";
pub const TOOL_DEFAULTS_FONT_FAMILY_ID: &str = "tool-defaults-panel-font-family";
pub const TOOL_DEFAULTS_SMOOTH_CURVES_ID: &str = "tool-defaults-panel-smooth-curves";
pub const TOOL_DEFAULTS_SMOOTH_CURVES_SWITCH_ID: &str = "tool-defaults-panel-smooth-curves-switch";
pub const TOOL_DEFAULTS_CLOUD_STYLE_ID: &str = "tool-defaults-panel-cloud-style";
pub const TOOL_DEFAULTS_CLOUD_STYLE_SWITCH_ID: &str = "tool-defaults-panel-cloud-style-switch";
pub const TOOL_DEFAULTS_CLOUD_INTENSITY_ID: &str = "tool-defaults-panel-cloud-intensity";
pub const TOOL_DEFAULTS_CLOUD_INTENSITY_SLIDER_ID: &str =
    "tool-defaults-panel-cloud-intensity-slider";
pub const TOOL_DEFAULTS_CLOUD_INTENSITY_INPUT_ID: &str =
    "tool-defaults-panel-cloud-intensity-input";
pub const TOOL_DEFAULTS_LANDING_ID: &str = "tool-defaults-panel-landing";
pub const TOOL_DEFAULTS_EMPTY_ID: &str = "tool-defaults-panel-empty";
pub const TOOL_DEFAULTS_MEASURE_ID: &str = "tool-defaults-panel-measure";
pub const TOOL_DEFAULTS_RESET_ID: &str = "tool-defaults-panel-reset";
pub const TOOL_DEFAULTS_RESET_BUTTON_ID: &str = "tool-defaults-panel-reset-button";
pub const TOOL_DEFAULTS_RESET_CANCEL_ID: &str = "tool-defaults-panel-reset-cancel";
pub const TOOL_DEFAULTS_RESET_CONFIRM_ID: &str = "tool-defaults-panel-reset-confirm";
pub const TOOL_DEFAULTS_RESET_POPOVER_ID: &str = "tool-defaults-panel-reset-popover";

const FALLBACK_COLOUR: &str = "#ff0000";
const FALLBACK_FILL_COLOUR: &str = "#ffffff";
const DEFAULT_FONT_FAMILY: &str = "Helvetica";

#[derive(Clone, Debug, PartialEq)]
pub enum ToolDefaultsEvent {
    Change {
        document_id: DocumentId,
        tool: AnnotationTool,
        properties: ToolProperties,
    },
    /// The Measure switch of Dimension, Polyline or Polygon.
    Measure {
        document_id: DocumentId,
        family: MeasureFamily,
        measure: bool,
    },
    Close,
}

struct ToolDefaultControls {
    width: Entity<InputState>,
    width_slider: Entity<SliderState>,
    opacity: Entity<InputState>,
    opacity_slider: Entity<SliderState>,
    font_size: Entity<InputState>,
    cloud_intensity: Entity<InputState>,
    cloud_intensity_slider: Entity<SliderState>,
    landing: Entity<InputState>,
    colour: Entity<ColorPickerState>,
    fill_colour: Entity<ColorPickerState>,
    font_family: Entity<SelectState<Vec<SharedString>>>,
}

pub struct ToolDefaultsPanel {
    document_id: Option<DocumentId>,
    tool: Option<AnnotationTool>,
    properties: ToolProperties,
    /// The Measure switch, for a tool that has one.
    measure: Option<bool>,
    disabled: bool,
    syncing: bool,
    controls: ToolDefaultControls,
    _subscriptions: Vec<Subscription>,
}

impl ToolDefaultsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tool = AnnotationTool::Select;
        let properties = ToolProperties::for_tool(tool);
        let (controls, subscriptions) = Self::build_controls(tool, &properties, window, cx);

        Self {
            document_id: None,
            tool: None,
            properties,
            measure: None,
            disabled: false,
            syncing: false,
            controls,
            _subscriptions: subscriptions,
        }
    }

    pub fn sync(
        &mut self,
        document_id: DocumentId,
        tool: AnnotationTool,
        properties: ToolProperties,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id == Some(document_id)
            && self.tool == Some(tool)
            && self.properties == properties
            && self.disabled == disabled
        {
            return;
        }

        self.syncing = true;
        let tool_changed = self.tool != Some(tool);
        self.document_id = Some(document_id);
        self.tool = Some(tool);
        self.properties = properties.clone();
        self.disabled = disabled;

        if tool_changed {
            let (controls, subscriptions) = Self::build_controls(tool, &properties, window, cx);
            self.controls = controls;
            self._subscriptions = subscriptions;
        } else {
            self.sync_controls(tool, &properties, window, cx);
        }

        self.syncing = false;
        cx.notify();
    }

    /// Shows the chosen tool's Measure switch, on or off, or hides it.
    pub fn set_measure(&mut self, measure: Option<bool>, cx: &mut Context<Self>) {
        if self.measure != measure {
            self.measure = measure;
            cx.notify();
        }
    }

    pub fn measure(&self) -> Option<bool> {
        self.measure
    }

    fn render_measure(&self, disabled: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let family = MeasureFamily::of(self.tool?)?;
        let measure = self.measure?;
        let document_id = self.document_id?;
        let panel = cx.entity().downgrade();
        Some(
            div()
                .id(TOOL_DEFAULTS_MEASURE_ID)
                .debug_selector(|| TOOL_DEFAULTS_MEASURE_ID.into())
                .child(
                    property_row(
                        "Measure",
                        v_flex()
                            .gap_1()
                            .child(
                                Switch::new("tool-defaults-panel-measure-switch")
                                    .checked(measure)
                                    .disabled(disabled)
                                    .tooltip(family.measure_hint())
                                    .on_click(move |measure, _, cx| {
                                        let _ = panel.update(cx, |_, cx| {
                                            cx.emit(ToolDefaultsEvent::Measure {
                                                document_id,
                                                family,
                                                measure: *measure,
                                            })
                                        });
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(match (family, measure) {
                                        (MeasureFamily::Dimension, true) => {
                                            "Measures where the page has a scale; elsewhere, type its text."
                                        }
                                        (MeasureFamily::Dimension, false) => "Type its text.",
                                        (_, true) => "Measures with the page scale.",
                                        (_, false) => "Turn on to measure with the page scale.",
                                    }),
                            ),
                    )
                    .align_top(),
                )
                .into_any_element(),
        )
    }

    pub fn document_id(&self) -> Option<DocumentId> {
        self.document_id
    }

    pub fn tool(&self) -> Option<AnnotationTool> {
        self.tool
    }

    pub fn properties(&self) -> &ToolProperties {
        &self.properties
    }

    pub const fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn input(&self, field: ToolPropertyField) -> Option<Entity<InputState>> {
        let tool = self.tool?;
        if !ToolProperties::is_applicable(tool, field) {
            return None;
        }
        match field {
            ToolPropertyField::WidthPt => Some(self.controls.width.clone()),
            ToolPropertyField::Opacity => Some(self.controls.opacity.clone()),
            ToolPropertyField::FontSizePt => Some(self.controls.font_size.clone()),
            ToolPropertyField::CloudIntensity => Some(self.controls.cloud_intensity.clone()),
            ToolPropertyField::LandingPt => Some(self.controls.landing.clone()),
            ToolPropertyField::Colour
            | ToolPropertyField::FillColour
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves
            | ToolPropertyField::CloudStyle => None,
        }
    }

    pub fn slider(&self, field: ToolPropertyField) -> Option<Entity<SliderState>> {
        let tool = self.tool?;
        if !ToolProperties::is_applicable(tool, field) {
            return None;
        }
        match field {
            ToolPropertyField::WidthPt => Some(self.controls.width_slider.clone()),
            ToolPropertyField::Opacity => Some(self.controls.opacity_slider.clone()),
            ToolPropertyField::CloudIntensity => Some(self.controls.cloud_intensity_slider.clone()),
            ToolPropertyField::Colour
            | ToolPropertyField::FillColour
            | ToolPropertyField::FontSizePt
            | ToolPropertyField::LandingPt
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves
            | ToolPropertyField::CloudStyle => None,
        }
    }

    pub fn width_input(&self) -> Entity<InputState> {
        self.controls.width.clone()
    }

    pub fn opacity_input(&self) -> Entity<InputState> {
        self.controls.opacity.clone()
    }

    pub fn opacity_slider(&self) -> Entity<SliderState> {
        self.controls.opacity_slider.clone()
    }

    pub fn font_size_input(&self) -> Entity<InputState> {
        self.controls.font_size.clone()
    }

    pub fn cloud_intensity_input(&self) -> Entity<InputState> {
        self.controls.cloud_intensity.clone()
    }

    pub fn color_picker(&self) -> Entity<ColorPickerState> {
        self.controls.colour.clone()
    }

    pub fn fill_color_picker(&self) -> Entity<ColorPickerState> {
        self.controls.fill_colour.clone()
    }

    fn build_controls(
        tool: AnnotationTool,
        properties: &ToolProperties,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (ToolDefaultControls, Vec<Subscription>) {
        let width_range = displayed_range(tool, ToolPropertyField::WidthPt);
        let opacity_range = displayed_range(tool, ToolPropertyField::Opacity);
        let cloud_range = displayed_range(tool, ToolPropertyField::CloudIntensity);

        let width = cx.new(|cx| {
            InputState::new(window, cx).default_value(format_property_number(properties.width_pt))
        });
        let width_slider = cx.new(|_| slider_state(width_range, properties.width_pt));
        let opacity = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format_property_percentage(properties.opacity))
        });
        let opacity_slider = cx.new(|_| slider_state(opacity_range, properties.opacity * 100.));
        let font_size = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format_property_number(properties.font_size_pt))
        });
        let cloud_intensity = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format_property_number(properties.cloud_intensity))
        });
        let cloud_intensity_slider =
            cx.new(|_| slider_state(cloud_range, properties.cloud_intensity));
        let landing = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(format_property_geometry(properties.landing_pt))
        });
        let colour = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(parsed_colour(
                &properties.colour,
                FALLBACK_COLOUR,
                properties.opacity,
            ))
        });
        let fill_colour = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(parsed_colour(
                properties
                    .fill_colour
                    .as_deref()
                    .unwrap_or(FALLBACK_FILL_COLOUR),
                FALLBACK_FILL_COLOUR,
                properties.fill_opacity,
            ))
        });
        let font_family_items = font_family_items(&properties.font_family);
        let font_family = cx
            .new(|cx| SelectState::new(font_family_items, Some(IndexPath::default()), window, cx));

        let mut subscriptions = vec![
            numeric_input_subscription(&width, ToolPropertyField::WidthPt, window, cx),
            numeric_slider_subscription(&width_slider, ToolPropertyField::WidthPt, cx),
            numeric_input_subscription(&opacity, ToolPropertyField::Opacity, window, cx),
            numeric_slider_subscription(&opacity_slider, ToolPropertyField::Opacity, cx),
            numeric_input_subscription(&font_size, ToolPropertyField::FontSizePt, window, cx),
            numeric_input_subscription(
                &cloud_intensity,
                ToolPropertyField::CloudIntensity,
                window,
                cx,
            ),
            numeric_slider_subscription(
                &cloud_intensity_slider,
                ToolPropertyField::CloudIntensity,
                cx,
            ),
            numeric_input_subscription(&landing, ToolPropertyField::LandingPt, window, cx),
            cx.subscribe_in(
                &font_family,
                window,
                |this, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                    let SelectEvent::Confirm(Some(font_family)) = event else {
                        return;
                    };
                    let Some(tool) = this.tool else {
                        return;
                    };
                    if !ToolProperties::is_applicable(tool, ToolPropertyField::FontFamily) {
                        return;
                    }
                    let mut properties = this.properties.clone();
                    properties.font_family = font_family.to_string();
                    this.emit_change(properties, cx);
                },
            ),
        ];
        subscriptions.extend(subscribe_property_color(
            &colour,
            cx,
            |this: &mut Self, cx| {
                let Some(colour) = this.controls.colour.read(cx).value() else {
                    return;
                };
                let Some(tool) = this.tool else {
                    return;
                };
                if !ToolProperties::is_applicable(tool, ToolPropertyField::Colour) {
                    return;
                }
                let mut properties = this.properties.clone();
                properties.colour = rgb_hex(colour);
                properties.opacity = f64::from(colour.a);
                this.emit_change(properties, cx);
            },
        ));
        subscriptions.extend(subscribe_property_color(
            &fill_colour,
            cx,
            |this: &mut Self, cx| {
                let Some(colour) = this.controls.fill_colour.read(cx).value() else {
                    return;
                };
                let Some(tool) = this.tool else {
                    return;
                };
                if !ToolProperties::is_applicable(tool, ToolPropertyField::FillColour) {
                    return;
                }
                let mut properties = this.properties.clone();
                properties.fill_colour = Some(rgb_hex(colour));
                properties.fill_opacity = f64::from(colour.a);
                this.emit_change(properties, cx);
            },
        ));

        (
            ToolDefaultControls {
                width,
                width_slider,
                opacity,
                opacity_slider,
                font_size,
                cloud_intensity,
                cloud_intensity_slider,
                landing,
                colour,
                fill_colour,
                font_family,
            },
            subscriptions,
        )
    }

    fn sync_controls(
        &self,
        tool: AnnotationTool,
        properties: &ToolProperties,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.controls.width.update(cx, |input, cx| {
            input.set_value(format_property_number(properties.width_pt), window, cx)
        });
        self.controls.width_slider.update(cx, |slider, cx| {
            slider.set_value(
                safe_slider_value(
                    displayed_range(tool, ToolPropertyField::WidthPt),
                    properties.width_pt,
                ),
                window,
                cx,
            )
        });
        self.controls.opacity.update(cx, |input, cx| {
            input.set_value(format_property_percentage(properties.opacity), window, cx)
        });
        self.controls.opacity_slider.update(cx, |slider, cx| {
            slider.set_value(
                safe_slider_value(
                    displayed_range(tool, ToolPropertyField::Opacity),
                    properties.opacity * 100.,
                ),
                window,
                cx,
            )
        });
        self.controls.font_size.update(cx, |input, cx| {
            input.set_value(format_property_number(properties.font_size_pt), window, cx)
        });
        self.controls.cloud_intensity.update(cx, |input, cx| {
            input.set_value(
                format_property_number(properties.cloud_intensity),
                window,
                cx,
            )
        });
        self.controls
            .cloud_intensity_slider
            .update(cx, |slider, cx| {
                slider.set_value(
                    safe_slider_value(
                        displayed_range(tool, ToolPropertyField::CloudIntensity),
                        properties.cloud_intensity,
                    ),
                    window,
                    cx,
                )
            });

        self.controls.landing.update(cx, |input, cx| {
            input.set_value(format_property_geometry(properties.landing_pt), window, cx)
        });

        if let Ok(mut colour) = try_parse_color(&properties.colour) {
            colour.a = properties.opacity.clamp(0., 1.) as f32;
            self.controls
                .colour
                .update(cx, |picker, cx| picker.set_value(colour, window, cx));
        }
        if let Ok(mut fill) = try_parse_color(
            properties
                .fill_colour
                .as_deref()
                .unwrap_or(FALLBACK_FILL_COLOUR),
        ) {
            fill.a = properties.fill_opacity.clamp(0., 1.) as f32;
            self.controls
                .fill_colour
                .update(cx, |picker, cx| picker.set_value(fill, window, cx));
        }

        let items = font_family_items(&properties.font_family);
        self.controls.font_family.update(cx, |select, cx| {
            select.set_items(items, window, cx);
            select.set_selected_value(
                &SharedString::from(properties.font_family.clone()),
                window,
                cx,
            );
        });
    }

    fn handle_numeric_input(
        &self,
        field: ToolPropertyField,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
            return;
        }
        let value = input.read(cx).value();
        let Some(properties) =
            properties_with_displayed_number(self.tool, &self.properties, field, value.as_ref())
        else {
            input.update(cx, |input, cx| {
                input.set_value(self.formatted_numeric_value(field), window, cx)
            });
            return;
        };
        self.emit_change(properties, cx);
    }

    fn formatted_numeric_value(&self, field: ToolPropertyField) -> String {
        match field {
            ToolPropertyField::WidthPt => format_property_number(self.properties.width_pt),
            ToolPropertyField::Opacity => format_property_percentage(self.properties.opacity),
            ToolPropertyField::FontSizePt => format_property_number(self.properties.font_size_pt),
            ToolPropertyField::CloudIntensity => {
                format_property_number(self.properties.cloud_intensity)
            }
            ToolPropertyField::LandingPt => format_property_geometry(self.properties.landing_pt),
            ToolPropertyField::Colour
            | ToolPropertyField::FillColour
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves
            | ToolPropertyField::CloudStyle => String::new(),
        }
    }

    fn emit_change(&self, properties: ToolProperties, cx: &mut Context<Self>) {
        let (Some(document_id), Some(tool)) = (self.document_id, self.tool) else {
            return;
        };
        if !can_emit_change(self.document_id, self.tool, self.disabled, self.syncing)
            || properties == self.properties
            || properties.validate_for_tool(tool).is_err()
        {
            return;
        }
        cx.emit(ToolDefaultsEvent::Change {
            document_id,
            tool,
            properties,
        });
    }

    fn render_colour_control(&self, disabled: bool) -> PropertyColor {
        let label = match self.tool {
            Some(AnnotationTool::TextBox) => "Text colour",
            Some(
                AnnotationTool::CloudPlus | AnnotationTool::Callout | AnnotationTool::Dimension,
            ) => "Text and line colour",
            _ => "Line colour",
        };
        PropertyColor::new(TOOL_DEFAULTS_COLOUR_ID, label, &self.controls.colour).disabled(disabled)
    }

    fn render_fill_row(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let panel = cx.entity().downgrade();
        let fill_enabled = self.properties.fill_colour.is_some();
        property_row(
            "Fill",
            h_flex()
                .id(TOOL_DEFAULTS_FILL_ID)
                .debug_selector(|| TOOL_DEFAULTS_FILL_ID.into())
                .w_full()
                .gap_3()
                .items_center()
                .child(
                    Switch::new(TOOL_DEFAULTS_FILL_ENABLED_ID)
                        .checked(fill_enabled)
                        .disabled(disabled)
                        .tooltip(if fill_enabled {
                            "Draw without a fill"
                        } else {
                            "Fill it"
                        })
                        .on_click(move |enabled, _, cx| {
                            let _ = panel.update(cx, |panel, cx| {
                                let Some(tool) = panel.tool else {
                                    return;
                                };
                                if !ToolProperties::is_applicable(
                                    tool,
                                    ToolPropertyField::FillColour,
                                ) {
                                    return;
                                }
                                let mut properties = panel.properties.clone();
                                properties.fill_colour = if *enabled {
                                    let colour = panel.controls.fill_colour.read(cx).value();
                                    if let Some(colour) = colour {
                                        properties.fill_opacity = f64::from(colour.a);
                                        Some(rgb_hex(colour))
                                    } else {
                                        properties.fill_opacity = 1.;
                                        Some(FALLBACK_FILL_COLOUR.into())
                                    }
                                } else {
                                    None
                                };
                                panel.emit_change(properties, cx);
                            });
                        }),
                )
                // Picking a colour fills it.
                .child(
                    PropertyColor::new(
                        TOOL_DEFAULTS_FILL_PICKER_ID,
                        "Fill colour",
                        &self.controls.fill_colour,
                    )
                    .disabled(disabled),
                ),
        )
        .into_any_element()
    }

    fn render_appearance(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(tool) = self.tool else {
            return div().into_any_element();
        };
        let applies = |field| ToolProperties::is_applicable(tool, field);
        let mut rows = PropertyGroup::new();
        let width = || {
            div()
                .id(TOOL_DEFAULTS_WIDTH_ID)
                .debug_selector(|| TOOL_DEFAULTS_WIDTH_ID.into())
                .child(
                    PropertyNumericInput::new(
                        TOOL_DEFAULTS_WIDTH_INPUT_ID,
                        "Line width",
                        &self.controls.width,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                )
        };
        let font_size = || {
            PropertyNumericInput::new(
                TOOL_DEFAULTS_FONT_SIZE_ID,
                "Font size",
                &self.controls.font_size,
            )
            .suffix("pt")
            .disabled(disabled)
        };
        // A colour shares its row with the size that goes with it: a line's
        // width, or a Text Box's font size.
        let mut font_size_shown = false;
        if applies(ToolPropertyField::Colour) {
            let colour = self.render_colour_control(disabled);
            if applies(ToolPropertyField::WidthPt) {
                let label = match tool {
                    AnnotationTool::CloudPlus
                    | AnnotationTool::Callout
                    | AnnotationTool::Dimension => "Colour",
                    _ => "Line",
                };
                rows = rows.row(property_row(
                    label,
                    property_color_and_size(colour, width()),
                ));
            } else if applies(ToolPropertyField::FontSizePt) {
                font_size_shown = true;
                rows = rows.row(property_row(
                    "Font",
                    property_color_and_size(colour, font_size()),
                ));
            } else {
                rows = rows.row(property_row("Colour", colour));
            }
        } else if applies(ToolPropertyField::WidthPt) {
            rows = rows.row(property_row("Line width", property_number(width())));
        }
        if applies(ToolPropertyField::FontSizePt) && !font_size_shown {
            rows = rows.row(property_row("Font size", property_number(font_size())));
        }
        if applies(ToolPropertyField::FontFamily) {
            rows = rows.row(property_row(
                "Typeface",
                div()
                    .id(TOOL_DEFAULTS_FONT_FAMILY_ID)
                    .debug_selector(|| TOOL_DEFAULTS_FONT_FAMILY_ID.into())
                    .child(Select::new(&self.controls.font_family).disabled(disabled)),
            ));
        }
        if applies(ToolPropertyField::Opacity) {
            rows = rows.row(property_slider_row(
                "Opacity",
                PropertySliderInput::new(
                    "Opacity",
                    &self.controls.opacity_slider,
                    &self.controls.opacity,
                )
                .row_id(TOOL_DEFAULTS_OPACITY_ID)
                .slider_id(TOOL_DEFAULTS_OPACITY_SLIDER_ID)
                .input_id(TOOL_DEFAULTS_OPACITY_INPUT_ID)
                .suffix("%")
                .disabled(disabled),
            ));
        }
        if applies(ToolPropertyField::FillColour) {
            rows = rows.row(self.render_fill_row(disabled, cx));
        }
        if applies(ToolPropertyField::SmoothCurves) {
            let panel = cx.entity().downgrade();
            rows = rows.row(property_row(
                "Smoothing",
                div()
                    .id(TOOL_DEFAULTS_SMOOTH_CURVES_ID)
                    .debug_selector(|| TOOL_DEFAULTS_SMOOTH_CURVES_ID.into())
                    .child(
                        Switch::new(TOOL_DEFAULTS_SMOOTH_CURVES_SWITCH_ID)
                            .checked(self.properties.smooth_curves)
                            .disabled(disabled)
                            .tooltip("Smooth the curves you draw")
                            .on_click(move |smooth, _, cx| {
                                let _ = panel.update(cx, |panel, cx| {
                                    let Some(tool) = panel.tool else {
                                        return;
                                    };
                                    if !ToolProperties::is_applicable(
                                        tool,
                                        ToolPropertyField::SmoothCurves,
                                    ) {
                                        return;
                                    }
                                    let mut properties = panel.properties.clone();
                                    properties.smooth_curves = *smooth;
                                    panel.emit_change(properties, cx);
                                });
                            }),
                    ),
            ));
        }
        if applies(ToolPropertyField::CloudStyle) {
            let panel = cx.entity().downgrade();
            rows = rows.row(property_row(
                "Cloud",
                div()
                    .id(TOOL_DEFAULTS_CLOUD_STYLE_ID)
                    .debug_selector(|| TOOL_DEFAULTS_CLOUD_STYLE_ID.into())
                    .child(
                        Switch::new(TOOL_DEFAULTS_CLOUD_STYLE_SWITCH_ID)
                            .checked(self.properties.cloud_style)
                            .disabled(disabled)
                            .tooltip("Draw the outline as a cloud")
                            .on_click(move |cloud, _, cx| {
                                let _ = panel.update(cx, |panel, cx| {
                                    let Some(tool) = panel.tool else {
                                        return;
                                    };
                                    if !ToolProperties::is_applicable(
                                        tool,
                                        ToolPropertyField::CloudStyle,
                                    ) {
                                        return;
                                    }
                                    let mut properties = panel.properties.clone();
                                    properties.cloud_style = *cloud;
                                    panel.emit_change(properties, cx);
                                });
                            }),
                    ),
            ));
        }
        // A polygon's cloud bump size applies only once it has a cloud
        // outline. The row shows either way, indented under Cloud and
        // disabled until Cloud is on, so the panel keeps its rows.
        if applies(ToolPropertyField::CloudIntensity) {
            let depends_on_cloud = applies(ToolPropertyField::CloudStyle);
            let intensity = property_slider_row(
                "Bump size",
                PropertySliderInput::new(
                    "Cloud bump size",
                    &self.controls.cloud_intensity_slider,
                    &self.controls.cloud_intensity,
                )
                .row_id(TOOL_DEFAULTS_CLOUD_INTENSITY_ID)
                .slider_id(TOOL_DEFAULTS_CLOUD_INTENSITY_SLIDER_ID)
                .input_id(TOOL_DEFAULTS_CLOUD_INTENSITY_INPUT_ID)
                .disabled(disabled || (depends_on_cloud && !self.properties.cloud_style)),
            );
            rows = rows.row(if depends_on_cloud {
                intensity.dependent()
            } else {
                intensity
            });
        }
        if applies(ToolPropertyField::LandingPt) {
            rows = rows.row(property_row(
                "Landing",
                property_number(
                    PropertyNumericInput::new(
                        TOOL_DEFAULTS_LANDING_ID,
                        "Landing length",
                        &self.controls.landing,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            ));
        }

        div()
            .id(TOOL_DEFAULTS_APPEARANCE_ID)
            .debug_selector(|| TOOL_DEFAULTS_APPEARANCE_ID.into())
            .child(rows)
            .into_any_element()
    }
}

impl EventEmitter<ToolDefaultsEvent> for ToolDefaultsPanel {}

impl Render for ToolDefaultsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.tool;
        let controls_disabled = self.disabled || self.document_id.is_none() || tool.is_none();
        let close_label = tool.map_or_else(
            || "Close tool properties".to_owned(),
            |tool| format!("Close {} properties", tool.label()),
        );
        // Closing only hides the panel, so it stays available while the
        // panel's controls are inert.
        let close = close_icon_button(TOOL_DEFAULTS_CLOSE_ID, close_label)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ToolDefaultsEvent::Close)));

        let title = match tool {
            None | Some(AnnotationTool::Select) => "Properties",
            Some(tool) => MeasureFamily::of(tool).map_or(tool.label(), MeasureFamily::label),
        };
        let measure = self.render_measure(controls_disabled, cx);
        let content = match tool {
            Some(tool) if !ToolProperties::applicable_fields(tool).is_empty() => {
                let reset = cx.entity().downgrade();
                let appearance = self.render_appearance(controls_disabled, cx);
                let reset_content = PropertyResetConfirm::new(
                    PropertyResetIds {
                        root: TOOL_DEFAULTS_RESET_ID,
                        button: TOOL_DEFAULTS_RESET_BUTTON_ID,
                        popover: TOOL_DEFAULTS_RESET_POPOVER_ID,
                        cancel: TOOL_DEFAULTS_RESET_CANCEL_ID,
                        confirm: TOOL_DEFAULTS_RESET_CONFIRM_ID,
                    },
                    format!("{:?}-{}", self.document_id, tool.label()),
                    format!(
                        "Reset {} properties? Other tools won't change.",
                        tool.label()
                    ),
                    {
                        let captured = self.document_id.map(|document_id| (document_id, tool));
                        move |_, cx| {
                            if let Some((document_id, tool)) = captured {
                                apply_reset(&reset, document_id, tool, cx);
                            }
                        }
                    },
                )
                .disabled(controls_disabled);
                property_body()
                    .children(measure)
                    .child(appearance)
                    .child(reset_content)
                    .into_any_element()
            }
            Some(tool) => div()
                .id(TOOL_DEFAULTS_EMPTY_ID)
                .debug_selector(|| TOOL_DEFAULTS_EMPTY_ID.into())
                .p_3()
                .text_color(cx.theme().muted_foreground)
                .child(match tool {
                    AnnotationTool::Select => "Select a markup to change its properties.",
                    AnnotationTool::Length | AnnotationTool::Image => {
                        "Properties for this tool are not available yet."
                    }
                    _ => "This tool has no configurable properties.",
                })
                .into_any_element(),
            None => div().into_any_element(),
        };

        PropertyInspectorPanel::new(
            TOOL_DEFAULTS_PANEL_ID,
            TOOL_DEFAULTS_HEADER_ID,
            TOOL_DEFAULTS_SCROLL_ID,
            title,
        )
        .header_trailing(close)
        .child(
            div()
                .id(TOOL_DEFAULTS_ACCORDION_ID)
                .debug_selector(|| TOOL_DEFAULTS_ACCORDION_ID.into())
                .w_full()
                .child(content),
        )
    }
}

fn numeric_input_subscription(
    input: &Entity<InputState>,
    field: ToolPropertyField,
    window: &mut Window,
    cx: &mut Context<ToolDefaultsPanel>,
) -> Subscription {
    cx.subscribe_in(input, window, move |panel, input, event, window, cx| {
        panel.handle_numeric_input(field, input, event, window, cx);
    })
}

fn numeric_slider_subscription(
    slider: &Entity<SliderState>,
    field: ToolPropertyField,
    cx: &mut Context<ToolDefaultsPanel>,
) -> Subscription {
    cx.subscribe(slider, move |panel, _, event: &SliderEvent, cx| {
        let SliderEvent::Change(value) = event else {
            return;
        };
        let Some(properties) = properties_with_displayed_value(
            panel.tool,
            &panel.properties,
            field,
            f64::from(value.start()),
        ) else {
            return;
        };
        panel.emit_change(properties, cx);
    })
}

/// Resets the tool's defaults, if the panel still shows the document and
/// tool the confirmation named.
fn apply_reset(
    panel: &gpui::WeakEntity<ToolDefaultsPanel>,
    captured_document_id: DocumentId,
    captured_tool: AnnotationTool,
    cx: &mut App,
) {
    let _ = panel.update(cx, |panel, cx| {
        if !can_apply_reset(
            panel.document_id,
            panel.tool,
            captured_document_id,
            captured_tool,
            panel.disabled,
            panel.syncing,
        ) {
            return;
        }
        let properties = ToolProperties::for_tool(captured_tool);
        if properties != panel.properties {
            cx.emit(ToolDefaultsEvent::Change {
                document_id: captured_document_id,
                tool: captured_tool,
                properties,
            });
        }
    });
}

fn displayed_range(tool: AnnotationTool, field: ToolPropertyField) -> ToolPropertyRange {
    let mut range = ToolProperties::range(tool, field).unwrap_or(match field {
        ToolPropertyField::WidthPt => ToolPropertyRange {
            min: 0.25,
            max: 24.,
            step: 0.25,
        },
        ToolPropertyField::Opacity => ToolPropertyRange {
            min: 0.,
            max: 1.,
            step: 0.05,
        },
        ToolPropertyField::FontSizePt => ToolPropertyRange {
            min: 6.,
            max: 72.,
            step: 1.,
        },
        ToolPropertyField::CloudIntensity => ToolPropertyRange {
            min: 0.,
            max: 4.,
            step: 0.25,
        },
        ToolPropertyField::LandingPt => ToolPropertyRange {
            min: 0.,
            max: CLOUD_PLUS_MAX_LANDING_PT,
            step: 0.1,
        },
        ToolPropertyField::Colour
        | ToolPropertyField::FillColour
        | ToolPropertyField::FontFamily
        | ToolPropertyField::SmoothCurves
        | ToolPropertyField::CloudStyle => ToolPropertyRange {
            min: 0.,
            max: 1.,
            step: 1.,
        },
    });
    if field == ToolPropertyField::Opacity {
        range.min *= 100.;
        range.max *= 100.;
        range.step *= 100.;
    }
    range
}

fn slider_state(range: ToolPropertyRange, value: f64) -> SliderState {
    SliderState::new()
        .min(range.min as f32)
        .max(range.max as f32)
        .step(range.step as f32)
        .default_value(safe_slider_value(range, value))
}

fn safe_slider_value(range: ToolPropertyRange, value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(range.min, range.max) as f32
    } else {
        range.min as f32
    }
}

fn properties_with_displayed_number(
    tool: Option<AnnotationTool>,
    properties: &ToolProperties,
    field: ToolPropertyField,
    value: &str,
) -> Option<ToolProperties> {
    let value = value.trim().parse::<f64>().ok()?;
    properties_with_displayed_value(tool, properties, field, value)
}

fn properties_with_displayed_value(
    tool: Option<AnnotationTool>,
    properties: &ToolProperties,
    field: ToolPropertyField,
    displayed_value: f64,
) -> Option<ToolProperties> {
    let tool = tool?;
    if !displayed_value.is_finite() || !ToolProperties::is_applicable(tool, field) {
        return None;
    }
    let range = displayed_range(tool, field);
    if !(range.min..=range.max).contains(&displayed_value) {
        return None;
    }

    let mut next = properties.clone();
    match field {
        ToolPropertyField::WidthPt => next.width_pt = displayed_value,
        ToolPropertyField::Opacity => next.opacity = displayed_value / 100.,
        ToolPropertyField::FontSizePt => next.font_size_pt = displayed_value,
        ToolPropertyField::CloudIntensity => next.cloud_intensity = displayed_value,
        ToolPropertyField::LandingPt => next.landing_pt = displayed_value,
        ToolPropertyField::Colour
        | ToolPropertyField::FillColour
        | ToolPropertyField::FontFamily
        | ToolPropertyField::SmoothCurves
        | ToolPropertyField::CloudStyle => return None,
    }
    next.validate_for_tool(tool).ok()?;
    Some(next)
}

fn font_family_items(font_family: &str) -> Vec<SharedString> {
    let mut items = vec![SharedString::from(font_family.to_owned())];
    if font_family != DEFAULT_FONT_FAMILY {
        items.push(DEFAULT_FONT_FAMILY.into());
    }
    items
}

fn parsed_colour(value: &str, fallback: &str, opacity: f64) -> Hsla {
    let mut colour = try_parse_color(value)
        .or_else(|_| try_parse_color(fallback))
        .expect("the built-in fallback colour must parse");
    colour.a = if opacity.is_finite() {
        opacity.clamp(0., 1.) as f32
    } else {
        1.
    };
    colour
}

fn rgb_hex(colour: Hsla) -> String {
    let rgb = gpui::Rgba::from(colour);
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgb.r * 255.).round() as u8,
        (rgb.g * 255.).round() as u8,
        (rgb.b * 255.).round() as u8,
    )
}

fn can_emit_change(
    document_id: Option<DocumentId>,
    tool: Option<AnnotationTool>,
    disabled: bool,
    syncing: bool,
) -> bool {
    document_id.is_some() && tool.is_some() && !disabled && !syncing
}

fn can_apply_reset(
    current_document_id: Option<DocumentId>,
    current_tool: Option<AnnotationTool>,
    captured_document_id: DocumentId,
    captured_tool: AnnotationTool,
    disabled: bool,
    syncing: bool,
) -> bool {
    !disabled
        && !syncing
        && current_document_id == Some(captured_document_id)
        && current_tool == Some(captured_tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn close_stays_available_while_the_controls_are_inert(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let (_, cx) = cx.add_window_view({
            let slot = slot.clone();
            move |window, cx| {
                let panel = cx.new(|cx| ToolDefaultsPanel::new(window, cx));
                slot.replace(Some(panel.clone()));
                gpui_component::Root::new(panel, window, cx)
            }
        });
        let panel = slot.borrow_mut().take().unwrap();
        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        cx.update(|_, cx| {
            let closed = closed.clone();
            cx.subscribe(&panel, move |_, event: &ToolDefaultsEvent, _| {
                if matches!(event, ToolDefaultsEvent::Close) {
                    closed.set(true);
                }
            })
            .detach();
        });
        cx.update(|window, cx| {
            panel.update(cx, |panel, cx| {
                let tool = AnnotationTool::Rectangle;
                let properties = ToolProperties::for_tool(tool);
                panel.sync(DocumentId::new(1), tool, properties, true, window, cx);
            });
            window.draw(cx).clear(cx);
        });

        // Saving makes the panel's controls inert; hiding the panel is not
        // a document edit, so its X still works, as every other X does.
        let close = cx.debug_bounds(TOOL_DEFAULTS_CLOSE_ID).unwrap();
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        assert!(closed.get());
    }

    #[test]
    fn numeric_values_use_model_applicability_and_ranges() {
        let highlight = ToolProperties::for_tool(AnnotationTool::Highlight);
        assert_eq!(
            properties_with_displayed_number(
                Some(AnnotationTool::Highlight),
                &highlight,
                ToolPropertyField::WidthPt,
                "48",
            )
            .map(|properties| properties.width_pt),
            Some(48.)
        );
        assert!(
            properties_with_displayed_number(
                Some(AnnotationTool::Highlight),
                &highlight,
                ToolPropertyField::WidthPt,
                "48.1",
            )
            .is_none()
        );
        assert_eq!(
            properties_with_displayed_number(
                Some(AnnotationTool::Image),
                &ToolProperties::for_tool(AnnotationTool::Image),
                ToolPropertyField::Opacity,
                "50",
            )
            .map(|properties| properties.opacity),
            Some(0.5),
        );
    }

    #[test]
    fn opacity_converts_between_model_fraction_and_gui_percentage() {
        let properties = ToolProperties::for_tool(AnnotationTool::Pen);
        let changed = properties_with_displayed_value(
            Some(AnnotationTool::Pen),
            &properties,
            ToolPropertyField::Opacity,
            62.5,
        )
        .expect("62.5% is a valid opacity");
        assert_eq!(changed.opacity, 0.625);
        assert!(
            properties_with_displayed_value(
                Some(AnnotationTool::Pen),
                &properties,
                ToolPropertyField::Opacity,
                f64::NAN,
            )
            .is_none()
        );
    }

    #[test]
    fn reset_requires_the_same_document_and_tool() {
        let document_id = DocumentId::new(1);
        let other_document_id = DocumentId::new(2);
        assert!(can_apply_reset(
            Some(document_id),
            Some(AnnotationTool::Pen),
            document_id,
            AnnotationTool::Pen,
            false,
            false,
        ));
        assert!(!can_apply_reset(
            Some(other_document_id),
            Some(AnnotationTool::Pen),
            document_id,
            AnnotationTool::Pen,
            false,
            false,
        ));
        assert!(!can_apply_reset(
            Some(document_id),
            Some(AnnotationTool::Highlight),
            document_id,
            AnnotationTool::Pen,
            false,
            false,
        ));
        assert!(!can_apply_reset(
            Some(document_id),
            Some(AnnotationTool::Pen),
            document_id,
            AnnotationTool::Pen,
            true,
            false,
        ));
    }

    #[test]
    fn colour_state_preserves_the_supplied_alpha_channel() {
        let colour = parsed_colour("#336699", FALLBACK_COLOUR, 0.375);
        assert_eq!(colour.a, 0.375);
    }
}
