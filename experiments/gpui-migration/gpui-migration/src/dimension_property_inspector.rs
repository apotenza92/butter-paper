//! Retained core-property presentation for one exact selected Dimension.
//!
//! The workspace owns document authority, annotation identity, revision validation, and history.

use crate::annotation_model::{
    DimensionAppearance, MarkupId, StraightLineAppearance, TextBoxStyle,
};
use crate::property_controls::{
    PropertyInspectorPanel, PropertyNumericInput, PropertySliderInput, canonical_picker_opacity,
    format_property_number, format_property_percentage, parse_property_percentage,
    property_color_picker,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
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

pub const DIMENSION_PROPERTY_INSPECTOR_ID: &str = "dimension-property-inspector";
pub const DIMENSION_INSPECTOR_LOCKED_ID: &str = "dimension-property-inspector-locked";
pub const DIMENSION_INSPECTOR_OFFSET_ID: &str = "dimension-property-offset";
pub const DIMENSION_INSPECTOR_WIDTH_ID: &str = "dimension-property-stroke-width";
pub const DIMENSION_INSPECTOR_FONT_SIZE_ID: &str = "dimension-property-font-size";
pub const DIMENSION_INSPECTOR_OPACITY_ID: &str = "dimension-property-opacity";
pub const DIMENSION_INSPECTOR_STROKE_COLOR_ID: &str = "dimension-property-stroke-color";
pub const DIMENSION_INSPECTOR_TEXT_COLOR_ID: &str = "dimension-property-text-color";
const DIMENSION_INSPECTOR_HEADER_ID: &str = "dimension-property-inspector-header";
const DIMENSION_INSPECTOR_SCROLL_ID: &str = "dimension-property-inspector-scroll";
const DIMENSION_INSPECTOR_OPACITY_INPUT_ID: &str = "dimension-property-opacity-input";

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub offset_pt: f64,
    pub show_offset: bool,
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
    embedded: bool,
    open_sections: [bool; 2],
    offset: gpui::Entity<InputState>,
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    font_size: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    stroke_color: gpui::Entity<ColorPickerState>,
    text_color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl DimensionPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let offset = cx.new(|cx| InputState::new(window, cx).default_value("24").step(1.));
        let width = cx.new(|cx| {
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
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("default Dimension stroke color parses"),
            )
        });
        let text_color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("default Dimension text color parses"),
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
                    this.edit_appearance(cx, |line, text| {
                        rebuild_appearance(line, text, None, None, Some(value), None, None)
                    })
                },
            ),
            numeric_subscription(&offset, window, cx, NumericProperty::Offset),
            numeric_subscription(&width, window, cx, NumericProperty::Width),
            numeric_subscription(&font_size, window, cx, NumericProperty::FontSize),
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
                    SliderEvent::Release(value) => this.edit_appearance(cx, |line, text| {
                        rebuild_appearance(
                            line,
                            text,
                            None,
                            None,
                            None,
                            Some(value.start() as f64 / 100.),
                            None,
                        )
                    }),
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
                            .map(|snapshot| snapshot.appearance.line().opacity())
                            .unwrap_or(1.);
                        input.update(cx, |input, cx| {
                            input.set_value(format_property_percentage(canonical), window, cx)
                        });
                        return;
                    };
                    this.edit_appearance(cx, |line, text| {
                        rebuild_appearance(line, text, None, None, None, Some(opacity), None)
                    });
                },
            ),
            cx.subscribe(&stroke_color, |this, _, _: &ColorPickerEvent, cx| {
                if !this.syncing {
                    cx.notify();
                }
            }),
            cx.subscribe(&text_color, |this, _, _: &ColorPickerEvent, cx| {
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
            offset,
            width,
            width_slider,
            font_size,
            opacity,
            opacity_input,
            stroke_color,
            text_color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&DimensionPropertySnapshot> {
        self.snapshot.as_ref()
    }
    pub fn opacity_input(&self) -> gpui::Entity<InputState> {
        self.opacity_input.clone()
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
        snapshot: DimensionPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.width_slider.update(cx, |slider, cx| {
            slider.set_value(
                snapshot.appearance.line().stroke_width_pt() as f32,
                window,
                cx,
            )
        });
        for (input, value) in [
            (&self.offset, snapshot.offset_pt),
            (&self.width, snapshot.appearance.line().stroke_width_pt()),
            (&self.font_size, snapshot.appearance.text().font_size_pt()),
        ] {
            input.update(cx, |input, cx| {
                input.set_value(format_property_number(value), window, cx)
            });
        }
        self.opacity.update(cx, |slider, cx| {
            slider.set_value(
                (snapshot.appearance.line().opacity() * 100.) as f32,
                window,
                cx,
            )
        });
        self.opacity_input.update(cx, |input, cx| {
            input.set_value(
                format_property_percentage(snapshot.appearance.line().opacity()),
                window,
                cx,
            )
        });
        if let Ok(mut color) = try_parse_color(snapshot.appearance.line().stroke_color()) {
            color.a = snapshot.appearance.line().opacity() as f32;
            self.stroke_color
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
        if let Ok(mut color) = try_parse_color(snapshot.appearance.text().color()) {
            color.a = snapshot.appearance.line().opacity() as f32;
            self.text_color
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }

    fn apply_color(&mut self, text_color: bool, cx: &mut Context<Self>) {
        let state = if text_color {
            &self.text_color
        } else {
            &self.stroke_color
        };
        let Some(color) = state.read(cx).value() else {
            return;
        };
        let opacity = canonical_picker_opacity(
            color.a,
            self.snapshot
                .as_ref()
                .map(|snapshot| snapshot.appearance.line().opacity())
                .unwrap_or(1.),
        );
        let color = rgb_hex(color);
        self.edit_appearance(cx, |line, text| {
            rebuild_appearance(
                line,
                text,
                (!text_color).then_some(color.as_str()),
                text_color.then_some(color.as_str()),
                None,
                Some(opacity),
                None,
            )
        });
    }

    fn edit_appearance(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&StraightLineAppearance, &TextBoxStyle) -> Option<DimensionAppearance>,
    ) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if let Some(appearance) = edit(snapshot.appearance.line(), snapshot.appearance.text()) {
            self.emit_patch(DimensionPropertyPatch::Appearance(appearance), cx);
        }
    }

    fn emit_patch(&mut self, patch: DimensionPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, DimensionPropertyPatch::Locked(_)))
            || patch_matches(&patch, snapshot)
        {
            return;
        }
        cx.emit(DimensionPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            patch,
        });
    }
}

impl EventEmitter<DimensionPropertyEvent> for DimensionPropertyInspector {}

impl Render for DimensionPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(DIMENSION_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let lock = cx.entity().downgrade();
        let stroke = cx.entity().downgrade();
        let text = cx.entity().downgrade();
        let details = Field::new().child(
            div().id(DIMENSION_INSPECTOR_LOCKED_ID).child(
                Switch::new("dimension-property-locked-switch")
                    .label("Locked")
                    .checked(snapshot.locked)
                    .disabled(snapshot.mutation_disabled)
                    .on_click(move |value, _, cx| {
                        let _ = lock.update(cx, |this, cx| {
                            this.emit_patch(DimensionPropertyPatch::Locked(*value), cx)
                        });
                    }),
            ),
        );
        let mut appearance = v_flex().gap_3();
        if snapshot.show_offset {
            appearance = appearance.child(
                Field::new().label("Offset").child(
                    PropertyNumericInput::new(
                        DIMENSION_INSPECTOR_OFFSET_ID,
                        "Offset",
                        &self.offset,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            );
        }
        let appearance = appearance
            .child(color_field(
                DIMENSION_INSPECTOR_STROKE_COLOR_ID,
                "Stroke colour",
                &self.stroke_color,
                disabled,
                stroke,
                false,
                cx,
            ))
            .child(crate::property_controls::property_pair(
                color_field(
                    DIMENSION_INSPECTOR_TEXT_COLOR_ID,
                    "Text colour",
                    &self.text_color,
                    disabled,
                    text,
                    true,
                    cx,
                ),
                Field::new().label("Font size").child(
                    PropertyNumericInput::new(
                        DIMENSION_INSPECTOR_FONT_SIZE_ID,
                        "Font size",
                        &self.font_size,
                    )
                    .suffix("pt")
                    .disabled(disabled),
                ),
            ))
            .child(
                Field::new().label("Stroke width").child(
                    PropertySliderInput::new("Stroke width", &self.width_slider, &self.width)
                        .input_id(DIMENSION_INSPECTOR_WIDTH_ID)
                        .suffix("pt")
                        .disabled(disabled),
                ),
            )
            .child(
                Field::new().label("Opacity").child(
                    PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                        .slider_id(DIMENSION_INSPECTOR_OPACITY_ID)
                        .input_id(DIMENSION_INSPECTOR_OPACITY_INPUT_ID)
                        .suffix("%")
                        .disabled(disabled),
                ),
            );
        let open_sections = self.open_sections;
        let section_control = cx.entity().downgrade();
        let accordion = Accordion::new("dimension-property-sections")
            .bordered(false)
            .multiple(true)
            .item(|item| item.open(open_sections[0]).title("Details").child(details))
            .item(|item| {
                item.open(open_sections[1])
                    .title("Appearance")
                    .child(appearance)
            })
            .on_toggle_click(move |indexes, _, cx| {
                let _ = section_control.update(cx, |inspector, cx| {
                    inspector.open_sections = [indexes.contains(&0), indexes.contains(&1)];
                    cx.notify();
                });
            });
        PropertyInspectorPanel::new(
            DIMENSION_PROPERTY_INSPECTOR_ID,
            DIMENSION_INSPECTOR_HEADER_ID,
            DIMENSION_INSPECTOR_SCROLL_ID,
            "Dimension properties",
        )
        .child(accordion)
        .into_any_element()
    }
}

#[derive(Clone, Copy)]
enum NumericProperty {
    Offset,
    Width,
    FontSize,
}

fn numeric_subscription(
    input: &gpui::Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<DimensionPropertyInspector>,
    property: NumericProperty,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |this, input, event: &InputEvent, _, cx| {
            if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                return;
            }
            let Ok(value) = input.read(cx).value().parse::<f64>() else {
                return;
            };
            if !value.is_finite() {
                return;
            }
            match property {
                NumericProperty::Offset => {
                    this.emit_patch(DimensionPropertyPatch::OffsetPt(value), cx)
                }
                NumericProperty::Width if (0.25..=24.).contains(&value) => {
                    this.edit_appearance(cx, |line, text| {
                        rebuild_appearance(line, text, None, None, Some(value), None, None)
                    })
                }
                NumericProperty::FontSize if (6. ..=72.).contains(&value) => {
                    this.edit_appearance(cx, |line, text| {
                        rebuild_appearance(line, text, None, None, None, None, Some(value))
                    })
                }
                _ => {}
            }
        },
    )
}

fn color_field(
    id: &'static str,
    label: &'static str,
    state: &gpui::Entity<ColorPickerState>,
    disabled: bool,
    inspector: gpui::WeakEntity<DimensionPropertyInspector>,
    text_color: bool,
    cx: &App,
) -> AnyElement {
    let button_id = if text_color {
        "dimension-apply-text-color"
    } else {
        "dimension-apply-stroke-color"
    };
    let picker = if disabled {
        let value = state
            .read(cx)
            .value()
            .map(rgb_hex)
            .unwrap_or_else(|| "Unavailable".into());
        Button::new(button_id)
            .label(value)
            .disabled(true)
            .into_any_element()
    } else {
        property_color_picker(state, "Colour").into_any_element()
    };
    let apply = (!disabled).then(|| {
        Button::new(button_id)
            .label("Apply colour")
            .on_click(move |_, _, cx| {
                let _ = inspector.update(cx, |this, cx| this.apply_color(text_color, cx));
            })
    });
    Field::new()
        .label(label)
        .child(div().id(id).child(picker).children(apply))
        .into_any_element()
}

fn rebuild_appearance(
    line: &StraightLineAppearance,
    text: &TextBoxStyle,
    stroke_color: Option<&str>,
    text_color: Option<&str>,
    width: Option<f64>,
    opacity: Option<f64>,
    font_size: Option<f64>,
) -> Option<DimensionAppearance> {
    let opacity = opacity.unwrap_or(line.opacity());
    let line = StraightLineAppearance::new(
        stroke_color.unwrap_or(line.stroke_color()),
        width.unwrap_or(line.stroke_width_pt()),
        opacity,
        line.stroke_style(),
    )
    .ok()?;
    let text = TextBoxStyle::new(
        text.font_family(),
        font_size.unwrap_or(text.font_size_pt()),
        text_color.unwrap_or(text.color()),
        opacity,
    )
    .and_then(|style| style.with_weight_and_alignment(text.weight(), text.alignment()))
    .ok()?;
    DimensionAppearance::new(line, text).ok()
}

fn patch_matches(patch: &DimensionPropertyPatch, snapshot: &DimensionPropertySnapshot) -> bool {
    match patch {
        DimensionPropertyPatch::Locked(value) => *value == snapshot.locked,
        DimensionPropertyPatch::OffsetPt(value) => *value == snapshot.offset_pt,
        DimensionPropertyPatch::Appearance(value) => value == &snapshot.appearance,
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

    #[test]
    fn picker_alpha_updates_both_dimension_parts_and_preserves_other_fields() {
        let line = StraightLineAppearance::new("#112233", 2.5, 0.8, StrokeStyle::Dashed).unwrap();
        let text = TextBoxStyle::new("Helvetica", 18., "#445566", 0.8).unwrap();
        for text_colour in [false, true] {
            let colour = try_parse_color("#00ff0066").unwrap();
            let hex = rgb_hex(colour);
            let next = rebuild_appearance(
                &line,
                &text,
                (!text_colour).then_some(hex.as_str()),
                text_colour.then_some(hex.as_str()),
                None,
                Some(canonical_picker_opacity(colour.a, line.opacity())),
                None,
            )
            .unwrap();
            assert!((next.line().opacity() - 102. / 255.).abs() < 0.0001);
            assert_eq!(next.line().opacity(), next.text().opacity());
            assert_eq!(next.line().stroke_width_pt(), line.stroke_width_pt());
            assert_eq!(next.line().stroke_style(), line.stroke_style());
            assert_eq!(next.text().font_family(), text.font_family());
            assert_eq!(next.text().font_size_pt(), text.font_size_pt());
            assert_eq!(
                next.line().stroke_color(),
                if text_colour { "#112233" } else { "#00ff00" }
            );
            assert_eq!(
                next.text().color(),
                if text_colour { "#00ff00" } else { "#445566" }
            );
        }
        let unchanged = rebuild_appearance(
            &line,
            &text,
            None,
            None,
            None,
            Some(canonical_picker_opacity(0.8_f64 as f32, 0.8)),
            None,
        )
        .unwrap();
        assert_eq!(unchanged, DimensionAppearance::new(line, text).unwrap());
    }
}
