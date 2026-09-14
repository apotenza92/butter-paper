//! Retained Highlight tool-defaults presentation.
//!
//! The workspace owns the per-document defaults. This panel owns only the
//! stock control entities, disclosure state, and the intent events emitted by
//! those controls.

use crate::accessible_button::accessible_icon_button;
use crate::document_workspace::{DocumentId, PenAnnotationDefaults};
use crate::property_controls::PropertySliderInput;
use gpui::{
    App, AppContext as _, ClickEvent, Context, EventEmitter, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, StyledExt as _, WindowExt as _,
    accordion::Accordion,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    dialog::{DialogAction, DialogClose, DialogFooter},
    form::Field,
    h_flex,
    input::{InputEvent, InputState},
    scroll::ScrollableElement as _,
    slider::{SliderEvent, SliderState},
    try_parse_color, v_flex,
};

pub const HIGHLIGHT_DEFAULTS_PANEL_ID: &str = "highlight-defaults-panel";
pub const HIGHLIGHT_DEFAULTS_HEADER_ID: &str = "highlight-defaults-panel-header";
pub const HIGHLIGHT_DEFAULTS_CLOSE_ID: &str = "highlight-defaults-panel-close";
pub const HIGHLIGHT_DEFAULTS_SCROLL_ID: &str = "highlight-defaults-panel-scroll";
pub const HIGHLIGHT_DEFAULTS_ACCORDION_ID: &str = "highlight-defaults-panel-accordion";
pub const HIGHLIGHT_DEFAULTS_APPEARANCE_ID: &str = "highlight-defaults-panel-appearance";
pub const HIGHLIGHT_DEFAULTS_COLOUR_ID: &str = "highlight-defaults-panel-colour";
pub const HIGHLIGHT_DEFAULTS_COLOUR_TRIGGER_ID: &str = "highlight-defaults-panel-colour-trigger";
pub const HIGHLIGHT_DEFAULTS_WIDTH_ID: &str = "highlight-defaults-panel-width";
pub const HIGHLIGHT_DEFAULTS_OPACITY_ID: &str = "highlight-defaults-panel-opacity";
pub const HIGHLIGHT_DEFAULTS_OPACITY_SLIDER_ID: &str = "highlight-defaults-panel-opacity-slider";
pub const HIGHLIGHT_DEFAULTS_OPACITY_INPUT_ID: &str = "highlight-defaults-panel-opacity-input";
pub const HIGHLIGHT_DEFAULTS_RESET_ID: &str = "highlight-defaults-panel-reset";
pub const HIGHLIGHT_DEFAULTS_RESET_BUTTON_ID: &str = "highlight-defaults-panel-reset-button";
pub const HIGHLIGHT_DEFAULTS_RESET_CANCEL_ID: &str = "highlight-defaults-panel-reset-cancel";
pub const HIGHLIGHT_DEFAULTS_RESET_CONFIRM_ID: &str = "highlight-defaults-panel-reset-confirm";

const DEFAULT_HIGHLIGHT_COLOUR: &str = "#ffff00";
const DEFAULT_HIGHLIGHT_WIDTH_PT: f64 = 12.;
const DEFAULT_HIGHLIGHT_OPACITY: f64 = 1.;

#[derive(Clone, Debug, PartialEq)]
pub enum HighlightDefaultsEvent {
    Change {
        document_id: DocumentId,
        defaults: PenAnnotationDefaults,
    },
    Close,
}

pub struct HighlightDefaultsPanel {
    document_id: Option<DocumentId>,
    defaults: PenAnnotationDefaults,
    disabled: bool,
    syncing: bool,
    appearance_open: bool,
    reset_open: bool,
    width: gpui::Entity<InputState>,
    width_slider: gpui::Entity<SliderState>,
    opacity: gpui::Entity<InputState>,
    opacity_slider: gpui::Entity<SliderState>,
    colour: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl HighlightDefaultsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let defaults = default_highlight_defaults();
        let width = cx.new(|cx| {
            InputState::new(window, cx).default_value(format_number(defaults.width_pt))
        });
        let width_slider = cx.new(|_| {
            SliderState::new()
                .min(1.)
                .max(48.)
                .step(1.)
                .default_value(12.)
        });
        let opacity = cx.new(|cx| {
            InputState::new(window, cx).default_value(format_percentage(defaults.opacity))
        });
        let opacity_slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(5.)
                .default_value(100.)
        });
        let colour = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color(DEFAULT_HIGHLIGHT_COLOUR)
                    .expect("the built-in Highlight colour must parse"),
            )
        });

        let subscriptions = vec![
            cx.subscribe(&width_slider, |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(value) = event else {
                    return;
                };
                let mut defaults = this.defaults.clone();
                defaults.width_pt = f64::from(value.start());
                this.emit_change(defaults, cx);
            }),
            cx.subscribe_in(
                &width,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let Some(width_pt) = parse_positive_finite(input.read(cx).value().as_ref())
                    else {
                        input.update(cx, |input, cx| {
                            input.set_value(format_number(this.defaults.width_pt), window, cx)
                        });
                        return;
                    };
                    let mut defaults = this.defaults.clone();
                    defaults.width_pt = width_pt;
                    this.emit_change(defaults, cx);
                },
            ),
            cx.subscribe_in(
                &opacity,
                window,
                |this, input, event: &InputEvent, window, cx| {
                    if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let Some(opacity) = parse_opacity_percent(input.read(cx).value().as_ref())
                    else {
                        input.update(cx, |input, cx| {
                            input.set_value(format_percentage(this.defaults.opacity), window, cx)
                        });
                        return;
                    };
                    let mut defaults = this.defaults.clone();
                    defaults.opacity = opacity;
                    this.emit_change(defaults, cx);
                },
            ),
            cx.subscribe(&opacity_slider, |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(value) = event else {
                    return;
                };
                let opacity = f64::from(value.start()) / 100.;
                if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) {
                    return;
                }
                let mut defaults = this.defaults.clone();
                defaults.opacity = opacity;
                this.emit_change(defaults, cx);
            }),
            cx.subscribe(&colour, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(colour)) = event else {
                    return;
                };
                let mut defaults = this.defaults.clone();
                defaults.color = rgb_hex(*colour);
                defaults.opacity = f64::from(colour.a);
                this.emit_change(defaults, cx);
            }),
        ];

        Self {
            document_id: None,
            defaults,
            disabled: false,
            syncing: false,
            appearance_open: true,
            reset_open: false,
            width,
            width_slider,
            opacity,
            opacity_slider,
            colour,
            _subscriptions: subscriptions,
        }
    }

    pub fn sync(
        &mut self,
        document_id: DocumentId,
        defaults: PenAnnotationDefaults,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id == Some(document_id)
            && self.defaults == defaults
            && self.disabled == disabled
        {
            return;
        }
        self.syncing = true;
        self.document_id = Some(document_id);
        self.defaults = defaults.clone();
        self.disabled = disabled;

        self.width.update(cx, |input, cx| {
            input.set_value(format_number(defaults.width_pt), window, cx);
        });
        self.width_slider.update(cx, |slider, cx| {
            slider.set_value(defaults.width_pt as f32, window, cx)
        });
        self.opacity.update(cx, |input, cx| {
            input.set_value(format_percentage(defaults.opacity), window, cx);
        });
        self.opacity_slider.update(cx, |slider, cx| {
            slider.set_value(opacity_percentage(defaults.opacity), window, cx);
        });
        if let Ok(mut colour) = try_parse_color(&defaults.color) {
            colour.a = defaults.opacity as f32;
            self.colour
                .update(cx, |picker, cx| picker.set_value(colour, window, cx));
        }

        self.syncing = false;
        cx.notify();
    }

    pub fn document_id(&self) -> Option<DocumentId> {
        self.document_id
    }

    pub fn defaults(&self) -> &PenAnnotationDefaults {
        &self.defaults
    }

    pub const fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub const fn is_appearance_open(&self) -> bool {
        self.appearance_open
    }

    pub const fn is_reset_open(&self) -> bool {
        self.reset_open
    }

    pub fn width_input(&self) -> gpui::Entity<InputState> {
        self.width.clone()
    }

    pub fn opacity_input(&self) -> gpui::Entity<InputState> {
        self.opacity.clone()
    }

    pub fn opacity_slider(&self) -> gpui::Entity<SliderState> {
        self.opacity_slider.clone()
    }

    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.colour.clone()
    }

    fn emit_change(&self, defaults: PenAnnotationDefaults, cx: &mut Context<Self>) {
        let Some(document_id) = self.document_id else {
            return;
        };
        if !can_emit_change(self.document_id, self.disabled, self.syncing)
            || defaults == self.defaults
        {
            return;
        }
        cx.emit(HighlightDefaultsEvent::Change {
            document_id,
            defaults,
        });
    }

    fn emit_close(&self, cx: &mut Context<Self>) {
        if !self.syncing && !self.disabled {
            cx.emit(HighlightDefaultsEvent::Close);
        }
    }
}

impl EventEmitter<HighlightDefaultsEvent> for HighlightDefaultsPanel {}

impl Render for HighlightDefaultsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controls_disabled = self.disabled || self.document_id.is_none();
        let panel = cx.entity().downgrade();
        let accordion_panel = panel.clone();
        let reset_panel = panel.clone();
        let close = accessible_icon_button(
            Button::new(HIGHLIGHT_DEFAULTS_CLOSE_ID)
                .debug_selector(|| HIGHLIGHT_DEFAULTS_CLOSE_ID.into())
                .accessibility_id(HIGHLIGHT_DEFAULTS_CLOSE_ID)
                .ghost()
                .xsmall()
                .icon(IconName::Close)
                .tooltip("Close Highlight properties")
                .disabled(self.disabled)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.emit_close(cx);
                })),
            "Close Highlight properties",
        );

        let appearance = v_flex()
            .id(HIGHLIGHT_DEFAULTS_APPEARANCE_ID)
            .debug_selector(|| HIGHLIGHT_DEFAULTS_APPEARANCE_ID.into())
            .gap_3()
            .child(colour_field(
                &self.colour,
                &self.defaults.color,
                self.defaults.opacity,
                controls_disabled,
            ))
            .child(
                Field::new().label("Stroke width").child(
                    PropertySliderInput::new("Stroke width", &self.width_slider, &self.width)
                        .row_id(HIGHLIGHT_DEFAULTS_WIDTH_ID)
                        .suffix("pt").disabled(controls_disabled),
                ),
            )
            .child(
                Field::new().label("Opacity").child(
                    PropertySliderInput::new("Opacity", &self.opacity_slider, &self.opacity)
                        .row_id(HIGHLIGHT_DEFAULTS_OPACITY_ID)
                        .slider_id(HIGHLIGHT_DEFAULTS_OPACITY_SLIDER_ID)
                        .input_id(HIGHLIGHT_DEFAULTS_OPACITY_INPUT_ID)
                        .suffix("%").disabled(controls_disabled),
                ),
            );

        let reset = v_flex()
            .id(HIGHLIGHT_DEFAULTS_RESET_ID)
            .debug_selector(|| HIGHLIGHT_DEFAULTS_RESET_ID.into())
            .gap_2()
            .child(
                Button::new(HIGHLIGHT_DEFAULTS_RESET_BUTTON_ID)
                    .debug_selector(|| HIGHLIGHT_DEFAULTS_RESET_BUTTON_ID.into())
                    .accessibility_id(HIGHLIGHT_DEFAULTS_RESET_BUTTON_ID)
                    .outline()
                    .label("Reset properties…")
                    .disabled(controls_disabled)
                    .on_click(move |_, window, cx| {
                        let Some(document_id) = reset_panel
                            .read_with(cx, |panel, _| {
                                if panel.disabled || panel.syncing {
                                    None
                                } else {
                                    panel.document_id
                                }
                            })
                            .ok()
                            .flatten()
                        else {
                            return;
                        };
                        open_reset_confirmation(reset_panel.clone(), document_id, window, cx);
                    }),
            );

        let accordion = Accordion::new("highlight-defaults-panel-sections")
            .bordered(false)
            .multiple(true)
            .disabled(self.disabled)
            .item(|item| {
                item.open(self.appearance_open)
                    .title("Appearance")
                    .child(appearance)
            })
            .item(|item| item.open(self.reset_open).title("Reset").child(reset))
            .on_toggle_click(move |open, _, cx| {
                let _ = accordion_panel.update(cx, |panel, cx| {
                    let appearance_open = open.contains(&0);
                    let reset_open = open.contains(&1);
                    if panel.appearance_open != appearance_open || panel.reset_open != reset_open {
                        panel.appearance_open = appearance_open;
                        panel.reset_open = reset_open;
                        cx.notify();
                    }
                });
            });

        v_flex()
            .id(HIGHLIGHT_DEFAULTS_PANEL_ID)
            .debug_selector(|| HIGHLIGHT_DEFAULTS_PANEL_ID.into())
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .text_sm()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .id(HIGHLIGHT_DEFAULTS_HEADER_ID)
                    .debug_selector(|| HIGHLIGHT_DEFAULTS_HEADER_ID.into())
                    .w_full()
                    .h_12()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .font_semibold()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().w_5().flex_none())
                    .child(div().flex_1().text_center().child("Highlight"))
                    .child(close),
            )
            .child(
                div()
                    .id(HIGHLIGHT_DEFAULTS_SCROLL_ID)
                    .debug_selector(|| HIGHLIGHT_DEFAULTS_SCROLL_ID.into())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .w_full()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .id(HIGHLIGHT_DEFAULTS_ACCORDION_ID)
                                    .debug_selector(|| HIGHLIGHT_DEFAULTS_ACCORDION_ID.into())
                                    .w_full()
                                    .child(accordion),
                            ),
                    ),
            )
    }
}

fn colour_field(
    state: &gpui::Entity<ColorPickerState>,
    canonical_colour: &str,
    canonical_opacity: f64,
    disabled: bool,
) -> gpui::AnyElement {
    let control = if disabled {
        Button::new(HIGHLIGHT_DEFAULTS_COLOUR_TRIGGER_ID)
            .debug_selector(|| HIGHLIGHT_DEFAULTS_COLOUR_TRIGGER_ID.into())
            .accessibility_id(HIGHLIGHT_DEFAULTS_COLOUR_TRIGGER_ID)
            .label(format!(
                "{} · {:.0}%",
                canonical_colour,
                canonical_opacity * 100.
            ))
            .disabled(true)
            .into_any_element()
    } else {
        div()
            .id(HIGHLIGHT_DEFAULTS_COLOUR_TRIGGER_ID)
            .debug_selector(|| HIGHLIGHT_DEFAULTS_COLOUR_TRIGGER_ID.into())
            // The stock featured swatches repeat colours in its full palette,
            // producing duplicate colour-derived accessibility IDs on macOS.
            // Use the supported palette-only configuration; no component fork.
            .child(
                ColorPicker::new(state)
                    .featured_colors(Vec::new())
                    .label(canonical_colour.to_uppercase()),
            )
            .into_any_element()
    };

    Field::new()
        .label("Stroke")
        .child(
            div()
                .id(HIGHLIGHT_DEFAULTS_COLOUR_ID)
                .debug_selector(|| HIGHLIGHT_DEFAULTS_COLOUR_ID.into())
                .child(control),
        )
        .into_any_element()
}

fn open_reset_confirmation(
    panel: gpui::WeakEntity<HighlightDefaultsPanel>,
    captured_document_id: DocumentId,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_alert_dialog(cx, move |alert, _, _| {
        let panel = panel.clone();
        alert
            .close_button(false)
            .title("Reset Highlight properties?")
            .description("Reset the Highlight defaults to yellow, 12 pt and 100% opacity.")
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new(HIGHLIGHT_DEFAULTS_RESET_CANCEL_ID)
                                .debug_selector(|| HIGHLIGHT_DEFAULTS_RESET_CANCEL_ID.into())
                                .accessibility_id(HIGHLIGHT_DEFAULTS_RESET_CANCEL_ID)
                                .outline()
                                .label("Cancel"),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new(HIGHLIGHT_DEFAULTS_RESET_CONFIRM_ID)
                                .debug_selector(|| HIGHLIGHT_DEFAULTS_RESET_CONFIRM_ID.into())
                                .accessibility_id(HIGHLIGHT_DEFAULTS_RESET_CONFIRM_ID)
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
                            captured_document_id,
                            panel.disabled,
                            panel.syncing,
                        ) {
                            return true;
                        }
                        cx.emit(HighlightDefaultsEvent::Change {
                            document_id: captured_document_id,
                            defaults: default_highlight_defaults(),
                        });
                        true
                    })
                    .unwrap_or(true)
            })
    });
}

fn default_highlight_defaults() -> PenAnnotationDefaults {
    PenAnnotationDefaults {
        color: DEFAULT_HIGHLIGHT_COLOUR.to_owned(),
        width_pt: DEFAULT_HIGHLIGHT_WIDTH_PT,
        opacity: DEFAULT_HIGHLIGHT_OPACITY,
    }
}

fn parse_positive_finite(value: &str) -> Option<f64> {
    let value = value.trim().parse::<f64>().ok()?;
    (value.is_finite() && (1. ..=48.).contains(&value)).then_some(value)
}

fn parse_opacity_percent(value: &str) -> Option<f64> {
    let value = value.trim().parse::<f64>().ok()?;
    (value.is_finite() && (0. ..=100.).contains(&value)).then_some(value / 100.)
}

fn can_apply_reset(
    current_document_id: Option<DocumentId>,
    captured_document_id: DocumentId,
    disabled: bool,
    syncing: bool,
) -> bool {
    !disabled && !syncing && current_document_id == Some(captured_document_id)
}

fn can_emit_change(document_id: Option<DocumentId>, disabled: bool, syncing: bool) -> bool {
    document_id.is_some() && !disabled && !syncing
}

fn opacity_percentage(opacity: f64) -> f32 {
    if opacity.is_finite() {
        (opacity * 100.).clamp(0., 100.) as f32
    } else {
        0.
    }
}

fn format_percentage(opacity: f64) -> String {
    format_number(opacity * 100.)
}

fn format_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.6}");
        formatted.trim_end_matches('0').trim_end_matches('.').into()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_positive_or_non_finite_widths() {
        assert_eq!(parse_positive_finite("12"), Some(12.));
        assert_eq!(parse_positive_finite("0"), None);
        assert_eq!(parse_positive_finite("-1"), None);
        assert_eq!(parse_positive_finite("NaN"), None);
        assert_eq!(parse_positive_finite("inf"), None);
        assert_eq!(parse_positive_finite("49"), None);
        assert_eq!(parse_positive_finite("0.5"), None);
    }

    #[test]
    fn accepts_only_opacity_percentages_in_range() {
        assert_eq!(parse_opacity_percent("0"), Some(0.));
        assert_eq!(parse_opacity_percent("50"), Some(0.5));
        assert_eq!(parse_opacity_percent("100"), Some(1.));
        assert_eq!(parse_opacity_percent("-1"), None);
        assert_eq!(parse_opacity_percent("101"), None);
        assert_eq!(parse_opacity_percent("NaN"), None);
    }

    #[test]
    fn reset_requires_the_captured_document_snapshot() {
        let first = DocumentId::new(1);
        let second = DocumentId::new(2);
        assert!(can_apply_reset(Some(first), first, false, false));
        assert!(!can_apply_reset(Some(second), first, false, false));
        assert!(!can_apply_reset(Some(first), first, true, false));
        assert!(!can_apply_reset(Some(first), first, false, true));
    }

    #[test]
    fn changes_require_a_document_and_an_enabled_non_syncing_panel() {
        let document = DocumentId::new(1);
        assert!(can_emit_change(Some(document), false, false));
        assert!(!can_emit_change(None, false, false));
        assert!(!can_emit_change(Some(document), true, false));
        assert!(!can_emit_change(Some(document), false, true));
    }

    #[test]
    fn reset_defaults_are_the_highlight_defaults() {
        assert_eq!(
            default_highlight_defaults(),
            PenAnnotationDefaults {
                color: "#ffff00".to_owned(),
                width_pt: 12.,
                opacity: 1.,
            }
        );
    }
}
