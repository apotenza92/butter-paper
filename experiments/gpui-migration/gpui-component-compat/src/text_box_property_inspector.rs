//! Retained presentation state for exactly one selected Text Box.
//!
//! The inspector owns only component state, color preview, and disclosure.
//! The workspace owns annotation identity, selection, validation, and history.

use butter_paper_gpui_gallery::annotation_model::{MarkupId, TextAlignment, TextBoxStyle};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, StyledExt as _,
    accordion::Accordion,
    button::Button,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    form::Field,
    h_flex,
    input::{InputEvent, InputState, NumberInput},
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement as _,
    slider::{Slider, SliderEvent, SliderState},
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
    open_sections: [bool; 2],
    size: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
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
        let color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("the built-in Text Box color must parse"),
            )
        });
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
            cx.subscribe(&opacity, |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) if !this.syncing => cx.notify(),
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => this.edit_style(cx, |style| {
                    rebuild_style(style, None, None, Some(value.start() as f64 / 100.), None)
                }),
            }),
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
            open_sections: [true, true],
            size,
            opacity,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&TextBoxPropertySnapshot> {
        self.snapshot.as_ref()
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
            self.open = false;
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
            input.set_value(format_number(snapshot.style.font_size_pt()), window, cx)
        });
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((snapshot.style.opacity() * 100.) as f32, window, cx)
        });
        if let Ok(color) = try_parse_color(snapshot.style.color()) {
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
    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.color.clone()
    }

    fn apply_preview_color(&mut self, cx: &mut Context<Self>) {
        let Some(color) = self.color.read(cx).value() else {
            return;
        };
        self.edit_style(cx, |style| {
            rebuild_style(style, Some(rgb_hex(color)), None, None, None)
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
        if self.syncing
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
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open) else {
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
        let details = Field::new().label("Locked").child(
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
            div()
                .w_full()
                .px_2()
                .py_1()
                .border_1()
                .opacity(0.5)
                .child(snapshot.style.color().to_owned())
                .into_any_element()
        } else {
            div()
                .id(TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID)
                .debug_selector(|| TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID.into())
                .child(ColorPicker::new(&self.color).label("Text color"))
                .into_any_element()
        };
        let appearance = v_flex()
            .gap_3()
            .child(
                Field::new().label("Text color").child(
                    div().child(color_control).child(
                        div()
                            .id(TEXT_BOX_INSPECTOR_APPLY_COLOR_ID)
                            .debug_selector(|| TEXT_BOX_INSPECTOR_APPLY_COLOR_ID.into())
                            .child(
                                Button::new("text-box-property-apply-color-button")
                                    .label("Apply color")
                                    .disabled(disabled)
                                    .on_click(move |_, _, cx| {
                                        let _ = color
                                            .update(cx, |this, cx| this.apply_preview_color(cx));
                                    }),
                            ),
                    ),
                ),
            )
            .child(
                Field::new().label("Font size").child(
                    div()
                        .id(TEXT_BOX_INSPECTOR_SIZE_ID)
                        .debug_selector(|| TEXT_BOX_INSPECTOR_SIZE_ID.into())
                        .child(NumberInput::new(&self.size).suffix("pt").disabled(disabled)),
                ),
            )
            .child(
                Field::new().label("Opacity").child(
                    h_flex()
                        .id(TEXT_BOX_INSPECTOR_OPACITY_ID)
                        .debug_selector(|| TEXT_BOX_INSPECTOR_OPACITY_ID.into())
                        .gap_2()
                        .child(
                            div()
                                .id(TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID)
                                .debug_selector(|| TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID.into())
                                .flex_1()
                                .child(Slider::new(&self.opacity).disabled(disabled)),
                        )
                        .child(format!("{:.0}%", self.opacity.read(cx).value().start())),
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
            .multiple(true)
            .bordered(false)
            .item(|item| item.open(open[0]).title("Details").child(details))
            .item(|item| item.open(open[1]).title("Appearance").child(appearance))
            .on_toggle_click(move |indexes, _, cx| {
                let _ = sections.update(cx, |this, cx| {
                    this.open_sections = [indexes.contains(&0), indexes.contains(&1)];
                    cx.notify();
                });
            });
        v_flex()
            .id(TEXT_BOX_PROPERTY_INSPECTOR_ID)
            .debug_selector(|| TEXT_BOX_PROPERTY_INSPECTOR_ID.into())
            .w(px(300.))
            .h_full()
            .min_h_0()
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(div().px_3().py_2().font_semibold().child("Text Box"))
            .child(
                div()
                    .h_full()
                    .min_h_0()
                    .p_3()
                    .overflow_y_scrollbar()
                    .child(accordion),
            )
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
fn format_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into()
    }
}
