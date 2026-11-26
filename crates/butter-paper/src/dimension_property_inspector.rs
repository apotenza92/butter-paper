//! Retained core-property presentation for one exact selected Dimension.
//!
//! The workspace owns document authority, annotation identity, revision validation, and history.

use crate::annotation_model::{
    DimensionAppearance, MarkupId, StraightLineAppearance, TextBoxStyle,
};
use crate::property_controls::{
    PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
    PropertySliderInput, PropertyTextEditEvent, canonical_picker_opacity, format_property_number,
    format_property_percentage, parse_property_percentage, property_body, property_color_and_size,
    property_lock_toggle, property_number, property_row, property_slider_row,
    subscribe_property_color,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    Disableable as _,
    button::Button,
    color_picker::ColorPickerState,
    input::{Input, InputEvent, InputState},
    slider::{SliderEvent, SliderState},
    try_parse_color,
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
pub const DIMENSION_INSPECTOR_TEXT_ID: &str = "dimension-property-text";
const DIMENSION_INSPECTOR_HEADER_ID: &str = "dimension-property-inspector-header";
const DIMENSION_INSPECTOR_SCROLL_ID: &str = "dimension-property-inspector-scroll";
const DIMENSION_INSPECTOR_OPACITY_INPUT_ID: &str = "dimension-property-opacity-input";

pub const DIMENSION_INSPECTOR_RESET_CAPTION_ID: &str = "dimension-property-reset-caption";

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub offset_pt: f64,
    /// Whether the markup has an offset of its own: a Dimension does, a
    /// Length does not. Its Offset row shows either way, disabled without.
    pub show_offset: bool,
    /// Only the label's text: a path's own panel above it already shows its
    /// lock, line and opacity. A Polyline, Polygon, Polylength or Area.
    pub text_only: bool,
    pub appearance: DimensionAppearance,
    /// A Dimension's caption was moved from where it usually sits.
    pub caption_moved: bool,
    /// The markup measures (a Length, Polylength or Area). A Length's text is
    /// its measurement, so its Text field shows it disabled, and a path's
    /// label (Font and Reset text position) applies only while it measures.
    pub measuring: bool,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DimensionPropertyPatch {
    Locked(bool),
    OffsetPt(f64),
    Appearance(DimensionAppearance),
    ResetCaptionPosition,
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
    /// The Dimension's text as last synced, once the workspace has given it.
    content_baseline: Option<String>,
    content: gpui::Entity<InputState>,
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
        let mut subscriptions = vec![
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
        ];
        // A caption is one line: Enter commits it.
        let content = cx.new(|cx| InputState::new(window, cx));
        subscriptions.push(cx.subscribe_in(
            &content,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.emit_content(cx);
                }
            },
        ));
        subscriptions.extend(subscribe_property_color(
            &stroke_color,
            cx,
            |this: &mut Self, cx| this.apply_color(false, cx),
        ));
        subscriptions.extend(subscribe_property_color(
            &text_color,
            cx,
            |this: &mut Self, cx| this.apply_color(true, cx),
        ));
        Self {
            snapshot: None,
            syncing: false,
            open: false,
            embedded: false,
            content_baseline: None,
            content,
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
        self.content_baseline = None;
        if self.snapshot.take().is_some() {
            cx.notify();
        }
    }
    /// Shows the selected Dimension's text in the Text field. The workspace
    /// calls this after `sync`, and again whenever the text changes; a
    /// Length's Text field shows its measurement, disabled.
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
    pub fn content_input(&self) -> gpui::Entity<InputState> {
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
            || snapshot.measuring
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
impl EventEmitter<PropertyTextEditEvent> for DimensionPropertyInspector {}

impl Render for DimensionPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(DIMENSION_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        // A path's label exists only while it measures: shown the same
        // before, disabled, so measuring adds no row.
        let label_disabled = disabled || (snapshot.text_only && !snapshot.measuring);
        let font = property_row(
            "Font",
            property_color_and_size(
                PropertyColor::new(
                    DIMENSION_INSPECTOR_TEXT_COLOR_ID,
                    "Text colour",
                    &self.text_color,
                )
                .disabled(label_disabled),
                PropertyNumericInput::new(
                    DIMENSION_INSPECTOR_FONT_SIZE_ID,
                    "Font size",
                    &self.font_size,
                )
                .suffix("pt")
                .disabled(label_disabled),
            ),
        );
        let reset = {
            let reset = cx.entity().downgrade();
            div().child(
                Button::new(DIMENSION_INSPECTOR_RESET_CAPTION_ID)
                    .debug_selector(|| DIMENSION_INSPECTOR_RESET_CAPTION_ID.into())
                    .label("Reset text position")
                    .outline()
                    // Enabled once the text has been moved from where it sits.
                    .disabled(label_disabled || !snapshot.caption_moved)
                    .on_click(move |_, _, cx| {
                        let _ = reset.update(cx, |this, cx| {
                            this.emit_patch(DimensionPropertyPatch::ResetCaptionPosition, cx)
                        });
                    }),
            )
        };
        if snapshot.text_only {
            // A path's label: the path's own panel above already shows its
            // lock, line and opacity.
            return PropertyInspectorPanel::new(
                DIMENSION_PROPERTY_INSPECTOR_ID,
                DIMENSION_INSPECTOR_HEADER_ID,
                DIMENSION_INSPECTOR_SCROLL_ID,
                "Label",
            )
            .content_only(self.embedded)
            .child(property_body().child(PropertyGroup::new().row(font).row(reset)))
            .into_any_element();
        }
        let content_only = self.embedded;
        let mut lock = Some({
            let lock = cx.entity().downgrade();
            property_lock_toggle(
                DIMENSION_INSPECTOR_LOCKED_ID,
                snapshot.locked,
                snapshot.mutation_disabled,
                move |locked, _, cx| {
                    let _ = lock.update(cx, |this, cx| {
                        this.emit_patch(DimensionPropertyPatch::Locked(locked), cx)
                    });
                },
            )
        });
        let mut top = PropertyGroup::new();
        if self.content_baseline.is_some() {
            // A Length's text is its measurement: shown, but not typed.
            top = top.row(property_row(
                "Text",
                div()
                    .id(DIMENSION_INSPECTOR_TEXT_ID)
                    .debug_selector(|| DIMENSION_INSPECTOR_TEXT_ID.into())
                    .w_full()
                    .child(
                        Input::new(&self.content)
                            .aria_label("Text")
                            .disabled(disabled || snapshot.measuring),
                    ),
            ));
        }
        if content_only && let Some(lock) = lock.take() {
            // No header here, so the lock sits with the other rows.
            top = top.row(property_row("Locked", lock));
        }
        let mut appearance = PropertyGroup::new().row(property_row(
            "Line",
            property_color_and_size(
                PropertyColor::new(
                    DIMENSION_INSPECTOR_STROKE_COLOR_ID,
                    "Line colour",
                    &self.stroke_color,
                )
                .disabled(disabled),
                PropertyNumericInput::new(DIMENSION_INSPECTOR_WIDTH_ID, "Line width", &self.width)
                    .suffix("pt")
                    .disabled(disabled),
            ),
        ));
        appearance = appearance.row(font);
        // A Length has no offset of its own: its row shows, disabled.
        appearance = appearance.row(property_row(
            "Offset",
            property_number(
                PropertyNumericInput::new(DIMENSION_INSPECTOR_OFFSET_ID, "Offset", &self.offset)
                    .suffix("pt")
                    .disabled(disabled || !snapshot.show_offset),
            ),
        ));
        appearance = appearance.row(property_slider_row(
            "Opacity",
            PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                .slider_id(DIMENSION_INSPECTOR_OPACITY_ID)
                .input_id(DIMENSION_INSPECTOR_OPACITY_INPUT_ID)
                .suffix("%")
                .disabled(disabled),
        ));
        appearance = appearance.row(reset);
        let mut body = property_body();
        if !top.is_empty() {
            body = body.child(top);
        }
        let mut panel = PropertyInspectorPanel::new(
            DIMENSION_PROPERTY_INSPECTOR_ID,
            DIMENSION_INSPECTOR_HEADER_ID,
            DIMENSION_INSPECTOR_SCROLL_ID,
            "Dimension",
        )
        .content_only(content_only);
        if let Some(lock) = lock {
            panel = panel.header_lock(lock);
        }
        panel.child(body.child(appearance)).into_any_element()
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
    .and_then(|style| style.with_layout_metrics(text.line_height_pt(), text.inset_pt()))
    .ok()?;
    DimensionAppearance::new(line, text).ok()
}

fn patch_matches(patch: &DimensionPropertyPatch, snapshot: &DimensionPropertySnapshot) -> bool {
    match patch {
        DimensionPropertyPatch::Locked(value) => *value == snapshot.locked,
        DimensionPropertyPatch::OffsetPt(value) => *value == snapshot.offset_pt,
        DimensionPropertyPatch::Appearance(value) => value == &snapshot.appearance,
        DimensionPropertyPatch::ResetCaptionPosition => !snapshot.caption_moved,
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
    fn caption_layout_font_edit_preserves_imported_line_height_and_inset() {
        let line = StraightLineAppearance::new("#112233", 2., 1., StrokeStyle::Solid).unwrap();
        let text = TextBoxStyle::new("Tinos", 24., "#445566", 1.)
            .unwrap()
            .with_layout_metrics(13.8, 3.)
            .unwrap();
        let next = rebuild_appearance(&line, &text, None, None, None, None, Some(30.)).unwrap();
        assert_eq!(next.text().font_size_pt(), 30.);
        assert_eq!(next.text().line_height_pt(), 13.8);
        assert_eq!(next.text().inset_pt(), 3.);
    }

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
