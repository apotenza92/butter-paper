//! Retained presentation state for exactly one selected Text Box.
//!
//! The inspector owns only component state, color preview, and disclosure.
//! The workspace owns annotation identity, selection, validation, and history.

use crate::annotation_model::{MarkupId, TextAlignment, TextBoxStyle};
use crate::property_controls::{
    PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
    PropertySliderInput, PropertyTextEditEvent, PropertyTextField, canonical_picker_opacity,
    format_property_number, format_property_percentage, parse_property_percentage, property_body,
    property_color_and_size, property_lock_toggle, property_row, property_slider_row,
    subscribe_property_color,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonGroup},
    color_picker::ColorPickerState,
    input::{InputEvent, InputState, TextareaState},
    slider::{SliderEvent, SliderState},
    try_parse_color,
};

use crate::document_workspace::DocumentId;

pub const TEXT_BOX_PROPERTY_INSPECTOR_ID: &str = "text-box-property-inspector";
pub const TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID: &str = "text-box-property-color-trigger";
pub const TEXT_BOX_INSPECTOR_TEXT_ID: &str = "text-box-property-text";
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
    /// The markup's text as last synced, once the workspace has given it.
    content_baseline: Option<String>,
    content: gpui::Entity<TextareaState>,
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
        let content = cx.new(|cx| TextareaState::new(window, cx).rows(2).soft_wrap(true));
        let opacity_input_for_slider = opacity_input.clone();
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
        ];
        subscriptions.extend(subscribe_property_color(
            &color,
            cx,
            |this: &mut Self, cx| this.apply_preview_color(cx),
        ));
        Self {
            snapshot: None,
            syncing: false,
            open: false,
            embedded: false,
            content_baseline: None,
            content,
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
        self.content_baseline = None;
        if self.snapshot.take().is_some() {
            cx.notify();
        }
    }
    /// Shows the selected Text Box's text in the Text field. The workspace
    /// calls this after `sync`, and again whenever the text changes.
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
    pub fn content_input(&self) -> gpui::Entity<TextareaState> {
        self.content.clone()
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
impl EventEmitter<PropertyTextEditEvent> for TextBoxPropertyInspector {}

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
        let alignment = cx.entity().downgrade();
        let selected_alignment = match snapshot.style.alignment() {
            TextAlignment::Left => 0,
            TextAlignment::Center => 1,
            TextAlignment::Right => 2,
        };
        let text = self.content_baseline.is_some().then(|| {
            property_row(
                "Text",
                PropertyTextField::new(TEXT_BOX_INSPECTOR_TEXT_ID, &self.content)
                    .disabled(disabled),
            )
            .align_top()
            .into_any_element()
        });
        let font = property_row(
            "Font",
            property_color_and_size(
                PropertyColor::new(
                    TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID,
                    "Text colour",
                    &self.color,
                )
                .disabled(disabled),
                PropertyNumericInput::new(TEXT_BOX_INSPECTOR_SIZE_ID, "Font size", &self.size)
                    .suffix("pt")
                    .disabled(disabled),
            ),
        );
        let opacity = property_slider_row(
            "Opacity",
            PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                .row_id(TEXT_BOX_INSPECTOR_OPACITY_ID)
                .slider_id(TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID)
                .input_id(TEXT_BOX_INSPECTOR_OPACITY_INPUT_ID)
                .suffix("%")
                .disabled(disabled),
        );
        let align = property_row(
            "Align",
            div()
                .id(TEXT_BOX_INSPECTOR_ALIGNMENT_ID)
                .debug_selector(|| TEXT_BOX_INSPECTOR_ALIGNMENT_ID.into())
                .child(
                    ButtonGroup::new("text-box-property-alignment-group")
                        .outline()
                        .w_full()
                        .disabled(disabled)
                        .child(
                            Button::new("left")
                                .debug_selector(|| TEXT_BOX_INSPECTOR_ALIGNMENT_LEFT_ID.into())
                                .flex_1()
                                .label("Left")
                                .selected(selected_alignment == 0),
                        )
                        .child(
                            Button::new("center")
                                .debug_selector(|| TEXT_BOX_INSPECTOR_ALIGNMENT_CENTER_ID.into())
                                .flex_1()
                                .label("Centre")
                                .selected(selected_alignment == 1),
                        )
                        .child(
                            Button::new("right")
                                .debug_selector(|| TEXT_BOX_INSPECTOR_ALIGNMENT_RIGHT_ID.into())
                                .flex_1()
                                .label("Right")
                                .selected(selected_alignment == 2),
                        )
                        .on_click(move |indexes, _, cx| {
                            let Some(index) = indexes.first().copied() else {
                                return;
                            };
                            let next = [
                                TextAlignment::Left,
                                TextAlignment::Center,
                                TextAlignment::Right,
                            ][index.min(2)];
                            let _ = alignment.update(cx, |this, cx| {
                                this.edit_style(cx, |style| {
                                    rebuild_style(style, None, None, None, Some(next))
                                })
                            });
                        }),
                ),
        );
        let mut rows = PropertyGroup::new();
        if let Some(text) = text {
            rows = rows.row(text);
        }
        PropertyInspectorPanel::new(
            TEXT_BOX_PROPERTY_INSPECTOR_ID,
            TEXT_BOX_INSPECTOR_HEADER_ID,
            TEXT_BOX_INSPECTOR_SCROLL_ID,
            "Text Box",
        )
        .header_lock(property_lock_toggle(
            TEXT_BOX_INSPECTOR_LOCKED_ID,
            snapshot.locked,
            snapshot.mutation_disabled,
            move |locked, _, cx| {
                let _ = lock.update(cx, |this, cx| {
                    this.emit_patch(TextBoxPropertyPatch::Locked(locked), cx)
                });
            },
        ))
        .child(property_body().child(rows.row(font).row(opacity).row(align)))
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
