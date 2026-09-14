//! Application-level composition for Butter Paper property inspectors.
//!
//! The stock GPUI Component controls retain their own interaction, focus, and
//! accessibility behaviour. This module owns only the repeated inspector
//! layout and the selected-property numeric presentation policy.

use gpui::{
    AnyElement, App, Entity, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, Styled as _, Window, div,
};
use gpui_component::{
    ActiveTheme as _, StyledExt as _,
    color_picker::{ColorPicker, ColorPickerState},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
    slider::{Slider, SliderState},
    v_flex,
};

/// Paired properties share equal columns; sliders remain full-width siblings.
pub fn property_pair(left: impl IntoElement, right: impl IntoElement) -> impl IntoElement {
    div()
        .grid()
        .grid_cols(2)
        .gap_3()
        .w_full()
        .min_w_0()
        .child(div().min_w_0().child(left))
        .child(div().min_w_0().child(right))
}

/// Keep the displayed number live without producing an undo entry per frame.
/// The inspector supplies its sync guard and its identity-checked commit path.
pub fn subscribe_property_slider<T: 'static>(
    slider: &Entity<SliderState>,
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut gpui::Context<T>,
    enabled: impl Fn(&T) -> bool + 'static,
    commit: impl Fn(&mut T, f64, &mut gpui::Context<T>) + 'static,
) -> gpui::Subscription {
    let input = input.clone();
    cx.subscribe_in(
        slider,
        window,
        move |this, _, event: &gpui_component::slider::SliderEvent, window, cx| {
            if !enabled(this) {
                return;
            }
            match event {
                gpui_component::slider::SliderEvent::Change(value) => {
                    input.update(cx, |input, cx| {
                        input.set_value(format_property_number(value.start().into()), window, cx);
                    });
                }
                gpui_component::slider::SliderEvent::Release(value) => {
                    commit(this, value.start().into(), cx)
                }
            }
        },
    )
}

/// A selected-property header with a title that stays centred when an X is
/// placed in the trailing lane.
#[derive(IntoElement)]
pub struct PropertyInspectorHeader {
    id: &'static str,
    title: SharedString,
    trailing: Option<AnyElement>,
}

impl PropertyInspectorHeader {
    pub fn new(id: &'static str, title: impl Into<SharedString>) -> Self {
        Self {
            id,
            title: title.into(),
            trailing: None,
        }
    }

    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }
}

impl RenderOnce for PropertyInspectorHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let trailing = self.trailing;

        h_flex()
            .id(self.id)
            .debug_selector(move || self.id.into())
            .w_full()
            .h_12()
            .flex_shrink_0()
            .items_center()
            .px_3()
            .font_semibold()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().w_5().flex_none())
            .child(div().flex_1().min_w_0().text_center().child(self.title))
            .child(div().w_5().flex_none().children(trailing))
    }
}

/// The common selected-property panel surface and edge-owned scroll region.
#[derive(IntoElement)]
pub struct PropertyInspectorPanel {
    id: &'static str,
    header_id: &'static str,
    scroll_id: &'static str,
    title: SharedString,
    header_trailing: Option<AnyElement>,
    content: Option<AnyElement>,
    content_only: bool,
}

impl PropertyInspectorPanel {
    pub fn new(
        id: &'static str,
        header_id: &'static str,
        scroll_id: &'static str,
        title: impl Into<SharedString>,
    ) -> Self {
        Self {
            id,
            header_id,
            scroll_id,
            title: title.into(),
            header_trailing: None,
            content: None,
            content_only: false,
        }
    }

    pub fn header_trailing(mut self, trailing: impl IntoElement) -> Self {
        self.header_trailing = Some(trailing.into_any_element());
        self
    }

    /// Compose two domain editors beneath one sidebar header and scroll owner.
    pub fn content_only(mut self, content_only: bool) -> Self {
        self.content_only = content_only;
        self
    }

    pub fn child(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for PropertyInspectorPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.content_only {
            return div()
                .id(self.id)
                .debug_selector(move || self.id.into())
                .w_full()
                .min_w_0()
                .children(self.content)
                .into_any_element();
        }
        let mut header = PropertyInspectorHeader::new(self.header_id, self.title);
        if let Some(trailing) = self.header_trailing {
            header = header.trailing(trailing);
        }

        v_flex()
            .id(self.id)
            .debug_selector(move || self.id.into())
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .flex_none()
            .text_sm()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(header)
            .child(
                div()
                    .id(self.scroll_id)
                    .debug_selector(move || self.scroll_id.into())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .w_full()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .children(self.content),
                    ),
            )
            .into_any_element()
    }
}

/// A stock text input used for one numeric property, without stepper buttons.
#[derive(IntoElement)]
pub struct PropertyNumericInput {
    id: &'static str,
    label: SharedString,
    state: Entity<InputState>,
    suffix: Option<SharedString>,
    disabled: bool,
}

impl PropertyNumericInput {
    pub fn new(
        id: &'static str,
        label: impl Into<SharedString>,
        state: &Entity<InputState>,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            state: state.clone(),
            suffix: None,
            disabled: false,
        }
    }

    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertyNumericInput {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut input = Input::new(&self.state)
            .aria_label(self.label)
            .w_full()
            .disabled(self.disabled);
        if let Some(suffix) = self.suffix {
            input = input.suffix(suffix);
        }

        div()
            .id(self.id)
            .debug_selector(move || self.id.into())
            .w_full()
            .child(input)
    }
}

/// A stock slider paired with a stepper-free numeric input and unit suffix.
/// The owning inspector retains synchronisation, validation, and commit policy.
#[derive(IntoElement)]
pub struct PropertySliderInput {
    label: SharedString,
    slider: Entity<SliderState>,
    input: Entity<InputState>,
    row_id: Option<&'static str>,
    slider_id: Option<&'static str>,
    input_id: Option<&'static str>,
    suffix: Option<SharedString>,
    disabled: bool,
}

impl PropertySliderInput {
    pub fn new(
        label: impl Into<SharedString>,
        slider: &Entity<SliderState>,
        input: &Entity<InputState>,
    ) -> Self {
        Self {
            label: label.into(),
            slider: slider.clone(),
            input: input.clone(),
            row_id: None,
            slider_id: None,
            input_id: None,
            suffix: None,
            disabled: false,
        }
    }

    pub fn row_id(mut self, id: &'static str) -> Self {
        self.row_id = Some(id);
        self
    }

    pub fn slider_id(mut self, id: &'static str) -> Self {
        self.slider_id = Some(id);
        self
    }

    pub fn input_id(mut self, id: &'static str) -> Self {
        self.input_id = Some(id);
        self
    }

    pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PropertySliderInput {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut input = Input::new(&self.input)
            .aria_label(self.label)
            .w_full()
            .disabled(self.disabled);
        if let Some(suffix) = self.suffix {
            input = input.suffix(suffix);
        }

        let slider = div()
            .flex_1()
            .min_w_0()
            .child(Slider::new(&self.slider).disabled(self.disabled));
        let slider = if let Some(id) = self.slider_id {
            slider
                .id(id)
                .debug_selector(move || id.into())
                .into_any_element()
        } else {
            slider.into_any_element()
        };

        let input = div().w_20().flex_shrink_0().child(input);
        let input = if let Some(id) = self.input_id {
            input
                .id(id)
                .debug_selector(move || id.into())
                .into_any_element()
        } else {
            input.into_any_element()
        };

        let row = h_flex()
            .items_center()
            // The stock thumb and hover ring project beyond the track.
            .gap_4()
            .child(slider)
            .child(input);
        if let Some(id) = self.row_id {
            row.id(id)
                .debug_selector(move || id.into())
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

/// The supported palette-only picker configuration avoids duplicate
/// colour-derived accessibility IDs in the stock featured and full palettes.
pub fn property_color_picker(
    state: &Entity<ColorPickerState>,
    label: impl Into<SharedString>,
) -> ColorPicker {
    ColorPicker::new(state)
        .featured_colors(Vec::new())
        .label(label)
}

pub fn format_property_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into()
    }
}

pub fn format_property_percentage(value: f64) -> String {
    format_property_number(value * 100.)
}

/// Compact point coordinates for paired fields. Callers must recognise an
/// unchanged display string before committing, preserving the canonical f64.
pub fn format_property_geometry(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

pub fn parse_property_percentage(value: &str) -> Option<f64> {
    let percentage = value.trim().parse::<f64>().ok()?;
    (percentage.is_finite() && (0. ..=100.).contains(&percentage)).then_some(percentage / 100.)
}

/// Keeps the canonical model value when a picker still contains its exact
/// `f32` projection, avoiding precision-only property edits.
pub fn canonical_picker_opacity(picker_alpha: f32, canonical: f64) -> f64 {
    if picker_alpha == canonical as f32 {
        canonical
    } else {
        picker_alpha as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_property_numbers_without_precision_noise() {
        assert_eq!(format_property_geometry(111.500741), "111.5");
        assert_eq!(format_property_geometry(519.851703), "519.85");
        assert_eq!(format_property_geometry(-0.0001), "0");
        assert_eq!(format_property_geometry(100.), "100");
        assert_eq!(format_property_number(12.), "12");
        assert_eq!(format_property_number(12.5), "12.5");
        assert_eq!(format_property_percentage(0.625), "62.5");
        assert_eq!(parse_property_percentage("62.5"), Some(0.625));
        assert_eq!(parse_property_percentage("101"), None);
        assert_eq!(parse_property_percentage("not a number"), None);
    }

    #[test]
    fn picker_opacity_preserves_canonical_precision_until_alpha_changes() {
        assert_eq!(canonical_picker_opacity(0.8_f64 as f32, 0.8), 0.8);
        assert_eq!(canonical_picker_opacity(0.4, 0.8), f64::from(0.4_f32));
    }
}
