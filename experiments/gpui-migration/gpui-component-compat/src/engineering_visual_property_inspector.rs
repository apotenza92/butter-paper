//! Retained property presentation for one selected Arc, Cloud, or Snapshot.
//!
//! The workspace owns identity, revision validation, persistence, and history.

use butter_paper_gpui_gallery::annotation_model::{MarkupId, RectangleAppearance};
use gpui::{
    AnyElement, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, StyledExt as _, accordion::Accordion, button::Button,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState}, form::Field, h_flex,
    input::{InputEvent, InputState, NumberInput}, scroll::ScrollableElement as _,
    slider::{Slider, SliderEvent, SliderState}, switch::Switch, try_parse_color, v_flex,
};

use crate::document_workspace::DocumentId;

pub const ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID: &str =
    "engineering-visual-property-inspector";
pub const ENGINEERING_VISUAL_INSPECTOR_LOCKED_ID: &str =
    "engineering-visual-property-inspector-locked";
pub const ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID: &str =
    "engineering-visual-property-inspector-color-trigger";
pub const ENGINEERING_VISUAL_INSPECTOR_APPLY_COLOR_ID: &str =
    "engineering-visual-property-inspector-apply-color";
pub const ENGINEERING_VISUAL_INSPECTOR_WIDTH_ID: &str =
    "engineering-visual-property-inspector-width";
pub const ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID: &str =
    "engineering-visual-property-inspector-opacity-track";
pub const ENGINEERING_VISUAL_INSPECTOR_INTENSITY_ID: &str =
    "engineering-visual-property-inspector-intensity";
pub const ENGINEERING_VISUAL_INSPECTOR_WIDTH_PX: f32 = 300.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineeringVisualPropertyKind {
    Arc,
    Cloud,
    Snapshot,
}

impl EngineeringVisualPropertyKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Arc => "Arc",
            Self::Cloud => "Cloud",
            Self::Snapshot => "Snapshot",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineeringVisualPropertyValues {
    Arc { appearance: RectangleAppearance },
    Cloud {
        appearance: RectangleAppearance,
        intensity: f64,
    },
    Snapshot { opacity: f64 },
}

impl EngineeringVisualPropertyValues {
    pub const fn kind(&self) -> EngineeringVisualPropertyKind {
        match self {
            Self::Arc { .. } => EngineeringVisualPropertyKind::Arc,
            Self::Cloud { .. } => EngineeringVisualPropertyKind::Cloud,
            Self::Snapshot { .. } => EngineeringVisualPropertyKind::Snapshot,
        }
    }

    fn appearance(&self) -> Option<&RectangleAppearance> {
        match self {
            Self::Arc { appearance } | Self::Cloud { appearance, .. } => Some(appearance),
            Self::Snapshot { .. } => None,
        }
    }

    fn opacity(&self) -> f64 {
        match self {
            Self::Arc { appearance } | Self::Cloud { appearance, .. } => appearance.opacity(),
            Self::Snapshot { opacity } => *opacity,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineeringVisualPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub values: EngineeringVisualPropertyValues,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineeringVisualPropertyPatch {
    Locked(bool),
    Color(String),
    WidthPt(f64),
    Opacity(f64),
    CloudIntensity(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineeringVisualPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_revision: u64,
    pub expected_kind: EngineeringVisualPropertyKind,
    pub patch: EngineeringVisualPropertyPatch,
}

pub struct EngineeringVisualPropertyInspector {
    snapshot: Option<EngineeringVisualPropertySnapshot>,
    syncing: bool,
    open: bool,
    open_sections: [bool; 2],
    width: gpui::Entity<InputState>,
    intensity: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

impl EngineeringVisualPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.25)
                .max(24.)
                .step(0.25)
        });
        let intensity = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.)
                .max(4.)
                .step(0.25)
        });
        let opacity = cx.new(|_| {
            SliderState::new().min(0.).max(100.).step(1.).default_value(100.)
        });
        let color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("the built-in visual color must parse"),
            )
        });
        let subscriptions = vec![
            cx.subscribe_in(&width, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f64>() else { return };
                if value.is_finite() && (0.25..=24.).contains(&value) {
                    this.emit_patch(EngineeringVisualPropertyPatch::WidthPt(value), cx);
                }
            }),
            cx.subscribe_in(&intensity, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f64>() else { return };
                if value.is_finite() && (0.0..=4.).contains(&value) {
                    this.emit_patch(EngineeringVisualPropertyPatch::CloudIntensity(value), cx);
                }
            }),
            cx.subscribe(&opacity, |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) if !this.syncing => cx.notify(),
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => this.emit_patch(
                    EngineeringVisualPropertyPatch::Opacity(value.start() as f64 / 100.), cx,
                ),
            }),
            cx.subscribe(&color, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(_) = event;
                if !this.syncing { cx.notify(); }
            }),
        ];
        Self {
            snapshot: None,
            syncing: false,
            open: false,
            open_sections: [true, true],
            width,
            intensity,
            opacity,
            color,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&EngineeringVisualPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open != open { self.open = open; cx.notify(); }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.take().is_some() { self.open = false; cx.notify(); }
    }

    pub fn sync(
        &mut self,
        snapshot: EngineeringVisualPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        if let Some(appearance) = snapshot.values.appearance() {
            self.width.update(cx, |input, cx| {
                input.set_value(format_number(appearance.stroke_width_pt()), window, cx);
            });
            if let Ok(color) = try_parse_color(appearance.stroke_color()) {
                self.color.update(cx, |picker, cx| picker.set_value(color, window, cx));
            }
        }
        if let EngineeringVisualPropertyValues::Cloud { intensity, .. } = &snapshot.values {
            self.intensity.update(cx, |input, cx| {
                input.set_value(format_number(*intensity), window, cx);
            });
        }
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((snapshot.values.opacity() * 100.) as f32, window, cx);
        });
        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }

    pub fn width_input(&self) -> gpui::Entity<InputState> { self.width.clone() }
    pub fn intensity_input(&self) -> gpui::Entity<InputState> { self.intensity.clone() }
    pub fn opacity_slider(&self) -> gpui::Entity<SliderState> { self.opacity.clone() }
    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> { self.color.clone() }

    fn apply_preview_color(&mut self, cx: &mut Context<Self>) {
        let Some(color) = self.color.read(cx).value() else { return };
        // Color alpha is deliberately ignored. Opacity has its own Release-only control.
        self.emit_patch(EngineeringVisualPropertyPatch::Color(rgb_hex(color)), cx);
    }

    fn emit_patch(&mut self, patch: EngineeringVisualPropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else { return };
        if self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, EngineeringVisualPropertyPatch::Locked(_)))
            || patch_matches_snapshot(&patch, snapshot)
        { return; }
        cx.emit(EngineeringVisualPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_revision: snapshot.expected_revision,
            expected_kind: snapshot.values.kind(),
            patch,
        });
    }
}

impl EventEmitter<EngineeringVisualPropertyEvent> for EngineeringVisualPropertyInspector {}

impl Render for EngineeringVisualPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open) else {
            return div().id(ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID)
                .debug_selector(|| ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID.into())
                .hidden().into_any_element();
        };
        let kind = snapshot.values.kind();
        let disabled = snapshot.mutation_disabled || snapshot.locked;
        let lock_control = cx.entity().downgrade();
        let section_control = cx.entity().downgrade();
        let opacity_value = self.opacity.read(cx).value().start();
        let details = v_flex().gap_3()
            .child(Field::new().label("Type").child(div().child(kind.label())))
            .child(Field::new().label("Locked").child(
                div().id(ENGINEERING_VISUAL_INSPECTOR_LOCKED_ID)
                    .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_LOCKED_ID.into()).child(
                    Switch::new("engineering-visual-property-inspector-locked-switch")
                        .label("Locked").checked(snapshot.locked)
                        .disabled(snapshot.mutation_disabled)
                        .on_click(move |locked, _, cx| {
                            let _ = lock_control.update(cx, |inspector, cx| {
                                inspector.emit_patch(EngineeringVisualPropertyPatch::Locked(*locked), cx);
                            });
                        }),
                ),
            ));
        let mut appearance = v_flex().gap_3();
        if let Some(value) = snapshot.values.appearance() {
            appearance = appearance
                .child(color_field(&self.color, value.stroke_color(), disabled, cx.entity().downgrade()))
                .child(Field::new().label("Line Width").child(
                    div().id(ENGINEERING_VISUAL_INSPECTOR_WIDTH_ID)
                        .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_WIDTH_ID.into()).child(
                        NumberInput::new(&self.width).suffix("pt").disabled(disabled),
                    ),
                ));
        }
        appearance = appearance.child(Field::new().label("Opacity").child(
            h_flex().gap_2()
                .child(div().id(ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID)
                    .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID.into()).flex_1()
                    .child(Slider::new(&self.opacity).disabled(disabled)))
                .child(div().w(px(44.)).text_right().text_sm().child(format!("{opacity_value:.0}%"))),
        ));
        if matches!(snapshot.values, EngineeringVisualPropertyValues::Cloud { .. }) {
            appearance = appearance.child(Field::new().label("Intensity").child(
                div().id(ENGINEERING_VISUAL_INSPECTOR_INTENSITY_ID)
                    .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_INTENSITY_ID.into()).child(
                    NumberInput::new(&self.intensity).disabled(disabled),
                ),
            ));
        }
        let open_sections = self.open_sections;
        let accordion = Accordion::new("engineering-visual-property-inspector-sections")
            .multiple(true).bordered(false)
            .item(|item| item.open(open_sections[0]).title("Details").child(details))
            .item(|item| item.open(open_sections[1]).title("Appearance").child(appearance))
            .on_toggle_click(move |open, _, cx| {
                let _ = section_control.update(cx, |inspector, cx| {
                    inspector.open_sections = [open.contains(&0), open.contains(&1)];
                    cx.notify();
                });
            });
        v_flex().id(ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID)
            .debug_selector(|| ENGINEERING_VISUAL_PROPERTY_INSPECTOR_ID.into())
            .w(px(ENGINEERING_VISUAL_INSPECTOR_WIDTH_PX)).h_full().min_h_0().flex_none()
            .border_l_1().border_color(cx.theme().border).bg(cx.theme().background)
            .child(div().px_3().py_2().font_semibold().child(format!("{} properties", kind.label())))
            .child(div().h_full().min_h_0().p_3().overflow_y_scrollbar().child(accordion))
            .into_any_element()
    }
}

fn color_field(
    state: &gpui::Entity<ColorPickerState>, canonical_color: &str, disabled: bool,
    inspector: gpui::WeakEntity<EngineeringVisualPropertyInspector>,
) -> AnyElement {
    let control = if disabled {
        div().w_full().px_2().py_1().border_1().opacity(0.5)
            .child(canonical_color.to_owned()).into_any_element()
    } else {
        div().id(ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID)
            .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_COLOR_TRIGGER_ID.into())
            .child(ColorPicker::new(state).label("Color")).into_any_element()
    };
    Field::new().label("Color").child(div().child(control).child(
        div().id(ENGINEERING_VISUAL_INSPECTOR_APPLY_COLOR_ID)
            .debug_selector(|| ENGINEERING_VISUAL_INSPECTOR_APPLY_COLOR_ID.into()).child(
            Button::new("engineering-visual-property-inspector-apply-color-button")
                .label("Apply color").disabled(disabled).on_click(move |_, _, cx| {
                    let _ = inspector.update(cx, |inspector, cx| inspector.apply_preview_color(cx));
                }),
        ),
    )).into_any_element()
}

fn rgb_hex(color: gpui::Hsla) -> String {
    let rgb = gpui::Rgba::from(color);
    format!("#{:02x}{:02x}{:02x}", (rgb.r * 255.).round() as u8,
        (rgb.g * 255.).round() as u8, (rgb.b * 255.).round() as u8)
}

fn format_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON { format!("{value:.0}") } else {
        let formatted = format!("{value:.6}");
        formatted.trim_end_matches('0').trim_end_matches('.').into()
    }
}

fn patch_matches_snapshot(
    patch: &EngineeringVisualPropertyPatch, snapshot: &EngineeringVisualPropertySnapshot,
) -> bool {
    match patch {
        EngineeringVisualPropertyPatch::Locked(value) => snapshot.locked == *value,
        EngineeringVisualPropertyPatch::Color(value) =>
            snapshot.values.appearance().is_some_and(|appearance| appearance.stroke_color() == value),
        EngineeringVisualPropertyPatch::WidthPt(value) =>
            snapshot.values.appearance().is_some_and(|appearance| appearance.stroke_width_pt() == *value),
        EngineeringVisualPropertyPatch::Opacity(value) => snapshot.values.opacity() == *value,
        EngineeringVisualPropertyPatch::CloudIntensity(value) => matches!(
            snapshot.values, EngineeringVisualPropertyValues::Cloud { intensity, .. } if intensity == *value
        ),
    }
}
