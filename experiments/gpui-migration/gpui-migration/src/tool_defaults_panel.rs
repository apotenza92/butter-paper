//! Retained presentation for the active annotation tool's defaults.
//!
//! The workspace owns the per-document defaults. This panel retains only
//! stock control state, input drafts, disclosure state, and target-bound
//! intent events.

use crate::{
    accessible_button::accessible_icon_button,
    annotation_adapter::AnnotationTool,
    document_workspace::DocumentId,
    property_controls::{
        PropertyInspectorPanel, PropertyNumericInput, PropertySliderInput, format_property_number,
        format_property_percentage, property_color_picker,
    },
    tool_properties::{ToolProperties, ToolPropertyField, ToolPropertyRange},
};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, IndexPath, Sizable as _, WindowExt as _,
    accordion::Accordion,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPickerEvent, ColorPickerState},
    dialog::{DialogAction, DialogClose, DialogFooter},
    form::Field,
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
pub const TOOL_DEFAULTS_CLOUD_INTENSITY_ID: &str = "tool-defaults-panel-cloud-intensity";
pub const TOOL_DEFAULTS_CLOUD_INTENSITY_SLIDER_ID: &str =
    "tool-defaults-panel-cloud-intensity-slider";
pub const TOOL_DEFAULTS_CLOUD_INTENSITY_INPUT_ID: &str =
    "tool-defaults-panel-cloud-intensity-input";
pub const TOOL_DEFAULTS_EMPTY_ID: &str = "tool-defaults-panel-empty";
pub const TOOL_DEFAULTS_RESET_ID: &str = "tool-defaults-panel-reset";
pub const TOOL_DEFAULTS_RESET_BUTTON_ID: &str = "tool-defaults-panel-reset-button";
pub const TOOL_DEFAULTS_RESET_CANCEL_ID: &str = "tool-defaults-panel-reset-cancel";
pub const TOOL_DEFAULTS_RESET_CONFIRM_ID: &str = "tool-defaults-panel-reset-confirm";

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
    colour: Entity<ColorPickerState>,
    fill_colour: Entity<ColorPickerState>,
    font_family: Entity<SelectState<Vec<SharedString>>>,
}

pub struct ToolDefaultsPanel {
    document_id: Option<DocumentId>,
    tool: Option<AnnotationTool>,
    properties: ToolProperties,
    disabled: bool,
    syncing: bool,
    appearance_open: bool,
    reset_open: bool,
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
            disabled: false,
            syncing: false,
            appearance_open: true,
            reset_open: false,
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
            ToolPropertyField::Colour
            | ToolPropertyField::FillColour
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves => None,
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
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves => None,
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

        let subscriptions = vec![
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
            cx.subscribe(&colour, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(colour)) = event else {
                    return;
                };
                let Some(tool) = this.tool else {
                    return;
                };
                if !ToolProperties::is_applicable(tool, ToolPropertyField::Colour) {
                    return;
                }
                let mut properties = this.properties.clone();
                properties.colour = rgb_hex(*colour);
                properties.opacity = f64::from(colour.a);
                this.emit_change(properties, cx);
            }),
            cx.subscribe(&fill_colour, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(colour)) = event else {
                    return;
                };
                let Some(tool) = this.tool else {
                    return;
                };
                if !ToolProperties::is_applicable(tool, ToolPropertyField::FillColour) {
                    return;
                }
                let mut properties = this.properties.clone();
                properties.fill_colour = Some(rgb_hex(*colour));
                properties.fill_opacity = f64::from(colour.a);
                this.emit_change(properties, cx);
            }),
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

        (
            ToolDefaultControls {
                width,
                width_slider,
                opacity,
                opacity_slider,
                font_size,
                cloud_intensity,
                cloud_intensity_slider,
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
            ToolPropertyField::Colour
            | ToolPropertyField::FillColour
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves => String::new(),
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

    fn emit_close(&self, cx: &mut Context<Self>) {
        if !self.syncing && !self.disabled {
            cx.emit(ToolDefaultsEvent::Close);
        }
    }

    fn render_colour_field(&self, disabled: bool) -> AnyElement {
        let label = match self.tool {
            Some(AnnotationTool::TextBox) => "Text colour",
            Some(
                AnnotationTool::CloudPlus | AnnotationTool::Callout | AnnotationTool::Dimension,
            ) => "Text and stroke colour",
            _ => "Stroke colour",
        };
        let control = if disabled {
            Button::new(TOOL_DEFAULTS_COLOUR_ID)
                .debug_selector(|| TOOL_DEFAULTS_COLOUR_ID.into())
                .label(self.properties.colour.to_uppercase())
                .disabled(true)
                .into_any_element()
        } else {
            div()
                .id(TOOL_DEFAULTS_COLOUR_ID)
                .debug_selector(|| TOOL_DEFAULTS_COLOUR_ID.into())
                .child(property_color_picker(
                    &self.controls.colour,
                    self.properties.colour.to_uppercase(),
                ))
                .into_any_element()
        };

        Field::new().label(label).child(control).into_any_element()
    }

    fn render_fill_field(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let panel = cx.entity().downgrade();
        let fill_enabled = self.properties.fill_colour.is_some();
        let picker = if fill_enabled {
            let picker = if disabled {
                Button::new(TOOL_DEFAULTS_FILL_PICKER_ID)
                    .label(
                        self.properties
                            .fill_colour
                            .as_deref()
                            .unwrap_or(FALLBACK_FILL_COLOUR)
                            .to_uppercase(),
                    )
                    .disabled(true)
                    .into_any_element()
            } else {
                div()
                    .id(TOOL_DEFAULTS_FILL_PICKER_ID)
                    .debug_selector(|| TOOL_DEFAULTS_FILL_PICKER_ID.into())
                    .child(property_color_picker(
                        &self.controls.fill_colour,
                        self.properties
                            .fill_colour
                            .as_deref()
                            .unwrap_or(FALLBACK_FILL_COLOUR)
                            .to_uppercase(),
                    ))
                    .into_any_element()
            };
            Some(picker)
        } else {
            None
        };

        Field::new()
            .label("Fill colour")
            .child(
                v_flex()
                    .id(TOOL_DEFAULTS_FILL_ID)
                    .debug_selector(|| TOOL_DEFAULTS_FILL_ID.into())
                    .gap_2()
                    .child(
                        Switch::new(TOOL_DEFAULTS_FILL_ENABLED_ID)
                            .label("Fill")
                            .checked(fill_enabled)
                            .disabled(disabled)
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
                    .children(picker),
            )
            .into_any_element()
    }

    fn render_appearance(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(tool) = self.tool else {
            return div().into_any_element();
        };
        let mut content = div()
            .grid()
            .grid_cols(2)
            .id(TOOL_DEFAULTS_APPEARANCE_ID)
            .debug_selector(|| TOOL_DEFAULTS_APPEARANCE_ID.into())
            .gap_3();

        if ToolProperties::is_applicable(tool, ToolPropertyField::Colour) {
            content = content.child(self.render_colour_field(disabled));
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::WidthPt) {
            content = content.child(
                Field::new().col_span(2).label("Stroke width").child(
                    PropertySliderInput::new(
                        "Stroke width",
                        &self.controls.width_slider,
                        &self.controls.width,
                    )
                    .row_id(TOOL_DEFAULTS_WIDTH_ID)
                    .slider_id(TOOL_DEFAULTS_WIDTH_SLIDER_ID)
                    .input_id(TOOL_DEFAULTS_WIDTH_INPUT_ID)
                    .suffix("pt")
                    .disabled(disabled),
                ),
            );
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::FillColour) {
            content = content.child(self.render_fill_field(disabled, cx));
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::FontSizePt) {
            content = content.child(
                Field::new().label("Font size").child(
                    PropertyNumericInput::new(
                        TOOL_DEFAULTS_FONT_SIZE_ID,
                        "Font size",
                        &self.controls.font_size,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            );
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::FontFamily) {
            content = content.child(
                Field::new().label("Font family").child(
                    div()
                        .id(TOOL_DEFAULTS_FONT_FAMILY_ID)
                        .debug_selector(|| TOOL_DEFAULTS_FONT_FAMILY_ID.into())
                        .child(Select::new(&self.controls.font_family).disabled(disabled)),
                ),
            );
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::Opacity) {
            content = content.child(
                Field::new().col_span(2).label("Opacity").child(
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
                ),
            );
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::SmoothCurves) {
            let panel = cx.entity().downgrade();
            content = content.child(
                Field::new().child(
                    div()
                        .id(TOOL_DEFAULTS_SMOOTH_CURVES_ID)
                        .debug_selector(|| TOOL_DEFAULTS_SMOOTH_CURVES_ID.into())
                        .child(
                            Switch::new(TOOL_DEFAULTS_SMOOTH_CURVES_SWITCH_ID)
                                .label("Smooth curves")
                                .checked(self.properties.smooth_curves)
                                .disabled(disabled)
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
                ),
            );
        }
        if ToolProperties::is_applicable(tool, ToolPropertyField::CloudIntensity) {
            content = content.child(
                Field::new().col_span(2).label("Cloud intensity").child(
                    PropertySliderInput::new(
                        "Cloud intensity",
                        &self.controls.cloud_intensity_slider,
                        &self.controls.cloud_intensity,
                    )
                    .row_id(TOOL_DEFAULTS_CLOUD_INTENSITY_ID)
                    .slider_id(TOOL_DEFAULTS_CLOUD_INTENSITY_SLIDER_ID)
                    .input_id(TOOL_DEFAULTS_CLOUD_INTENSITY_INPUT_ID)
                    .disabled(disabled),
                ),
            );
        }

        content.into_any_element()
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
        let close = accessible_icon_button(
            Button::new(TOOL_DEFAULTS_CLOSE_ID)
                .debug_selector(|| TOOL_DEFAULTS_CLOSE_ID.into())
                .accessibility_id(TOOL_DEFAULTS_CLOSE_ID)
                .ghost()
                .xsmall()
                .icon(IconName::Close)
                .tooltip(close_label.clone())
                .disabled(self.disabled)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.emit_close(cx))),
            close_label,
        );

        let title = match tool {
            None | Some(AnnotationTool::Select) => "Properties",
            Some(tool) => tool.label(),
        };
        let content = match tool {
            Some(tool) if !ToolProperties::applicable_fields(tool).is_empty() => {
                let sections = cx.entity().downgrade();
                let reset = cx.entity().downgrade();
                let appearance = self.render_appearance(controls_disabled, cx);
                let reset_content = v_flex()
                    .id(TOOL_DEFAULTS_RESET_ID)
                    .debug_selector(|| TOOL_DEFAULTS_RESET_ID.into())
                    .gap_2()
                    .child(
                        Button::new(TOOL_DEFAULTS_RESET_BUTTON_ID)
                            .debug_selector(|| TOOL_DEFAULTS_RESET_BUTTON_ID.into())
                            .accessibility_id(TOOL_DEFAULTS_RESET_BUTTON_ID)
                            .outline()
                            .label("Reset properties…")
                            .disabled(controls_disabled)
                            .on_click(move |_, window, cx| {
                                let Some((document_id, tool)) = reset
                                    .read_with(cx, |panel, _| {
                                        if panel.disabled || panel.syncing {
                                            None
                                        } else {
                                            panel.document_id.zip(panel.tool)
                                        }
                                    })
                                    .ok()
                                    .flatten()
                                else {
                                    return;
                                };
                                open_reset_confirmation(
                                    reset.clone(),
                                    document_id,
                                    tool,
                                    window,
                                    cx,
                                );
                            }),
                    );

                Accordion::new("tool-defaults-panel-sections")
                    .bordered(false)
                    .multiple(true)
                    .disabled(self.disabled)
                    .item(|item| {
                        item.open(self.appearance_open)
                            .title("Appearance")
                            .child(appearance)
                    })
                    .item(|item| {
                        item.open(self.reset_open)
                            .title("Reset")
                            .child(reset_content)
                    })
                    .on_toggle_click(move |open, _, cx| {
                        let _ = sections.update(cx, |panel, cx| {
                            let appearance_open = open.contains(&0);
                            let reset_open = open.contains(&1);
                            if panel.appearance_open != appearance_open
                                || panel.reset_open != reset_open
                            {
                                panel.appearance_open = appearance_open;
                                panel.reset_open = reset_open;
                                cx.notify();
                            }
                        });
                    })
                    .into_any_element()
            }
            Some(tool) => div()
                .id(TOOL_DEFAULTS_EMPTY_ID)
                .debug_selector(|| TOOL_DEFAULTS_EMPTY_ID.into())
                .p_3()
                .text_color(cx.theme().muted_foreground)
                .child(match tool {
                    AnnotationTool::Select => {
                        "No editable properties are available for the current selection."
                    }
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

fn open_reset_confirmation(
    panel: gpui::WeakEntity<ToolDefaultsPanel>,
    captured_document_id: DocumentId,
    captured_tool: AnnotationTool,
    window: &mut Window,
    cx: &mut App,
) {
    let tool_label = captured_tool.label();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let panel = panel.clone();
        alert
            .close_button(false)
            .title(format!("Reset {tool_label} properties?"))
            .description(format!(
                "Restore the current document's {tool_label} defaults. Other tools will not change."
            ))
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new(TOOL_DEFAULTS_RESET_CANCEL_ID)
                                .debug_selector(|| TOOL_DEFAULTS_RESET_CANCEL_ID.into())
                                .accessibility_id(TOOL_DEFAULTS_RESET_CANCEL_ID)
                                .outline()
                                .label("Cancel"),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new(TOOL_DEFAULTS_RESET_CONFIRM_ID)
                                .debug_selector(|| TOOL_DEFAULTS_RESET_CONFIRM_ID.into())
                                .accessibility_id(TOOL_DEFAULTS_RESET_CONFIRM_ID)
                                .primary()
                                .label("Reset"),
                        ),
                    ),
            )
            .on_ok(move |_, _, cx| {
                panel
                    .update(cx, |panel, cx| {
                        if !can_apply_reset(
                            panel.document_id,
                            panel.tool,
                            captured_document_id,
                            captured_tool,
                            panel.disabled,
                            panel.syncing,
                        ) {
                            return true;
                        }
                        let properties = ToolProperties::for_tool(captured_tool);
                        if properties != panel.properties {
                            cx.emit(ToolDefaultsEvent::Change {
                                document_id: captured_document_id,
                                tool: captured_tool,
                                properties,
                            });
                        }
                        true
                    })
                    .unwrap_or(true)
            })
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
        ToolPropertyField::Colour
        | ToolPropertyField::FillColour
        | ToolPropertyField::FontFamily
        | ToolPropertyField::SmoothCurves => ToolPropertyRange {
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
        ToolPropertyField::Colour
        | ToolPropertyField::FillColour
        | ToolPropertyField::FontFamily
        | ToolPropertyField::SmoothCurves => return None,
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
        assert!(
            properties_with_displayed_number(
                Some(AnnotationTool::Image),
                &ToolProperties::for_tool(AnnotationTool::Image),
                ToolPropertyField::Opacity,
                "50",
            )
            .is_none()
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
