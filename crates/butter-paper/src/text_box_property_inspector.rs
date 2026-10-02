//! Retained presentation state for exactly one selected Text Box.
//!
//! The inspector owns only component state, color preview, and disclosure.
//! The workspace owns annotation identity, selection, validation, and history.

use crate::annotation_model::{MarkupId, TextAlignment, TextBoxStyle};
use crate::property_controls::{
    PropertyInspectorPanel, PropertyNumericInput, PropertySliderInput, canonical_picker_opacity,
    format_property_number, format_property_percentage, parse_property_percentage,
    property_color_picker,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    Disableable as _,
    accordion::Accordion,
    button::Button,
    color_picker::{ColorPickerEvent, ColorPickerState},
    form::Field,
    input::{InputEvent, InputState},
    radio::{Radio, RadioGroup},
    slider::{SliderEvent, SliderState},
    switch::Switch,
    try_parse_color, v_flex,
};

use crate::document_workspace::DocumentId;

pub const TEXT_BOX_PROPERTY_INSPECTOR_ID: &str = "text-box-property-inspector";
pub const TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID: &str = "text-box-property-color-trigger";
pub const TEXT_BOX_INSPECTOR_APPLY_COLOR_ID: &str = "text-box-property-apply-color";
pub const TEXT_BOX_INSPECTOR_SIZE_ID: &str = "text-box-property-size";
pub const TEXT_BOX_INSPECTOR_OPACITY_ID: &str = "text-box-property-opacity";
pub const TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID: &str = "text-box-property-opacity-track";
pub const TEXT_BOX_INSPECTOR_ALIGNMENT_ID: &str = "text-box-property-alignment";
pub const TEXT_BOX_INSPECTOR_ALIGNMENT_LEFT_ID: &str = "text-box-property-alignment-left";
pub const TEXT_BOX_INSPECTOR_ALIGNMENT_CENTER_ID: &str = "text-box-property-alignment-center";
pub const TEXT_BOX_INSPECTOR_ALIGNMENT_RIGHT_ID: &str = "text-box-property-alignment-right";
pub const TEXT_BOX_INSPECTOR_LOCKED_ID: &str = "text-box-property-locked";
const TEXT_BOX_INSPECTOR_HEADER_ID: &str = "text-box-property-inspector-header";
const TEXT_BOX_INSPECTOR_SCROLL_ID: &str = "text-box-property-inspector-scroll";
const TEXT_BOX_INSPECTOR_OPACITY_INPUT_ID: &str = "text-box-property-opacity-input";

#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub style: TextBoxStyle,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextBoxPropertyPatch {
    Locked(bool),
    Style(TextBoxStyle),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub patch: TextBoxPropertyPatch,
}

pub struct TextBoxPropertyInspector {
    snapshot: Option<TextBoxPropertySnapshot>,
    syncing: bool,
    open: bool,
    embedded: bool,
    open_sections: [bool; 2],
    size: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl TextBoxPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let size = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("12")
                .min(6.)
                .max(72.)
                .step(1.)
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
                try_parse_color("#ff0000").expect("the built-in Text Box color must parse"),
            )
        });
        let opacity_input_for_slider = opacity_input.clone();
        let subscriptions = vec![
            cx.subscribe_in(&size, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f64>() else {
                    return;
                };
                if (6. ..=72.).contains(&value) {
                    this.edit_style(cx, |style| {
                        rebuild_style(style, None, Some(value), None, None)
                    });
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
                    SliderEvent::Release(value) => this.edit_style(cx, |style| {
                        rebuild_style(style, None, None, Some(value.start() as f64 / 100.), None)
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
                            .map(|snapshot| snapshot.style.opacity())
                            .unwrap_or(1.);
                        input.update(cx, |input, cx| {
                            input.set_value(format_property_percentage(canonical), window, cx)
                        });
                        return;
                    };
                    this.edit_style(cx, |style| {
                        rebuild_style(style, None, None, Some(opacity), None)
                    });
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
            size,
            opacity,
            opacity_input,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&TextBoxPropertySnapshot> {
        self.snapshot.as_ref()
    }
    pub fn set_embedded(&mut self) {
        self.embedded = true;
    }
    pub const fn is_open(&self) -> bool {
        self.open
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
        snapshot: TextBoxPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.size.update(cx, |input, cx| {
            input.set_value(
                format_property_number(snapshot.style.font_size_pt()),
                window,
                cx,
            )
        });
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((snapshot.style.opacity() * 100.) as f32, window, cx)
        });
        self.opacity_input.update(cx, |input, cx| {
            input.set_value(
                format_property_percentage(snapshot.style.opacity()),
                window,
                cx,
            )
        });
        if let Ok(mut color) = try_parse_color(snapshot.style.color()) {
            color.a = snapshot.style.opacity() as f32;
            self.color
                .update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }
    pub fn size_input(&self) -> gpui::Entity<InputState> {
        self.size.clone()
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
        let Some(color) = self.color.read(cx).value() else {
            return;
        };
        self.edit_style(cx, |style| {
            rebuild_style(
                style,
                Some(rgb_hex(color)),
                None,
                Some(canonical_picker_opacity(color.a, style.opacity())),
                None,
            )
        });
    }
    fn edit_style(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&TextBoxStyle) -> Option<TextBoxStyle>,
    ) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let Some(style) = edit(&snapshot.style) else {
            return;
        };
        self.emit_patch(TextBoxPropertyPatch::Style(style), cx);
    }
    fn emit_patch(&mut self, patch: TextBoxPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, TextBoxPropertyPatch::Locked(_)))
            || matches!(&patch, TextBoxPropertyPatch::Locked(value) if *value == snapshot.locked)
            || matches!(&patch, TextBoxPropertyPatch::Style(style) if style == &snapshot.style)
        {
            return;
        }
        cx.emit(TextBoxPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            patch,
        });
    }
}

impl EventEmitter<TextBoxPropertyEvent> for TextBoxPropertyInspector {}

impl Render for TextBoxPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(TEXT_BOX_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let lock = cx.entity().downgrade();
        let color = cx.entity().downgrade();
        let alignment = cx.entity().downgrade();
        let sections = cx.entity().downgrade();
        let selected_alignment = match snapshot.style.alignment() {
            TextAlignment::Left => 0,
            TextAlignment::Center => 1,
            TextAlignment::Right => 2,
        };
        let details = Field::new().child(
            div()
                .id(TEXT_BOX_INSPECTOR_LOCKED_ID)
                .debug_selector(|| TEXT_BOX_INSPECTOR_LOCKED_ID.into())
                .child(
                    Switch::new("text-box-property-locked-switch")
                        .label("Locked")
                        .checked(snapshot.locked)
                        .disabled(snapshot.mutation_disabled)
                        .on_click(move |value, _, cx| {
                            let _ = lock.update(cx, |this, cx| {
                                this.emit_patch(TextBoxPropertyPatch::Locked(*value), cx)
                            });
                        }),
                ),
        );
        let color_control = if disabled {
            Button::new(TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID)
                .debug_selector(|| TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID.into())
                .label(snapshot.style.color().to_owned())
                .disabled(true)
                .into_any_element()
        } else {
            div()
                .id(TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID)
                .debug_selector(|| TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID.into())
                .child(property_color_picker(&self.color, "Colour"))
                .into_any_element()
        };
        let appearance = v_flex()
            .gap_3()
            .child(crate::property_controls::property_pair(
                Field::new().label("Text colour").child(
                    div().child(color_control).child(
                        div()
                            .id(TEXT_BOX_INSPECTOR_APPLY_COLOR_ID)
                            .debug_selector(|| TEXT_BOX_INSPECTOR_APPLY_COLOR_ID.into())
                            .child(
                                Button::new("text-box-property-apply-color-button")
                                    .label("Apply colour")
                                    .disabled(disabled)
                                    .on_click(move |_, _, cx| {
                                        let _ = color
                                            .update(cx, |this, cx| this.apply_preview_color(cx));
                                    }),
                            ),
                    ),
                ),
                Field::new().label("Font size").child(
                    PropertyNumericInput::new(TEXT_BOX_INSPECTOR_SIZE_ID, "Font size", &self.size)
                        .suffix("pt")
                        .disabled(disabled),
                ),
            ))
            .child(
                Field::new().label("Opacity").child(
                    PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                        .row_id(TEXT_BOX_INSPECTOR_OPACITY_ID)
                        .slider_id(TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID)
                        .input_id(TEXT_BOX_INSPECTOR_OPACITY_INPUT_ID)
                        .suffix("%")
                        .disabled(disabled),
                ),
            )
            .child(
                Field::new().label("Horizontal alignment").child(
                    div()
                        .id(TEXT_BOX_INSPECTOR_ALIGNMENT_ID)
                        .debug_selector(|| TEXT_BOX_INSPECTOR_ALIGNMENT_ID.into())
                        .child(
                            RadioGroup::horizontal("text-box-property-alignment-group")
                                .selected_index(Some(selected_alignment))
                                .disabled(disabled)
                                .child(
                                    Radio::new("left")
                                        .debug_selector(|| {
                                            TEXT_BOX_INSPECTOR_ALIGNMENT_LEFT_ID.into()
                                        })
                                        .label("Left"),
                                )
                                .child(
                                    Radio::new("center")
                                        .debug_selector(|| {
                                            TEXT_BOX_INSPECTOR_ALIGNMENT_CENTER_ID.into()
                                        })
                                        .label("Center"),
                                )
                                .child(
                                    Radio::new("right")
                                        .debug_selector(|| {
                                            TEXT_BOX_INSPECTOR_ALIGNMENT_RIGHT_ID.into()
                                        })
                                        .label("Right"),
                                )
                                .on_click(move |index, _, cx| {
                                    let next = [
                                        TextAlignment::Left,
                                        TextAlignment::Center,
                                        TextAlignment::Right,
                                    ][*index];
                                    let _ = alignment.update(cx, |this, cx| {
                                        this.edit_style(cx, |style| {
                                            rebuild_style(style, None, None, None, Some(next))
                                        })
                                    });
                                }),
                        ),
                ),
            );
        let open = self.open_sections;
        let accordion = Accordion::new("text-box-property-sections")
            .bordered(false)
            .multiple(true)
            .item(|item| item.open(open[0]).title("Details").child(details))
            .item(|item| item.open(open[1]).title("Appearance").child(appearance))
            .on_toggle_click(move |indexes, _, cx| {
                let _ = sections.update(cx, |this, cx| {
                    this.open_sections = [indexes.contains(&0), indexes.contains(&1)];
                    cx.notify();
                });
            });
        PropertyInspectorPanel::new(
            TEXT_BOX_PROPERTY_INSPECTOR_ID,
            TEXT_BOX_INSPECTOR_HEADER_ID,
            TEXT_BOX_INSPECTOR_SCROLL_ID,
            "Text Box",
        )
        .child(accordion)
        .into_any_element()
    }
}

fn rebuild_style(
    style: &TextBoxStyle,
    color: Option<String>,
    size: Option<f64>,
    opacity: Option<f64>,
    alignment: Option<TextAlignment>,
) -> Option<TextBoxStyle> {
    TextBoxStyle::new(
        style.font_family(),
        size.unwrap_or(style.font_size_pt()),
        color.unwrap_or_else(|| style.color().to_owned()),
        opacity.unwrap_or(style.opacity()),
    )
    .and_then(|next| {
        next.with_weight_and_alignment(style.weight(), alignment.unwrap_or(style.alignment()))
    })
    .ok()
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
