//! Retained presentation state for rectangular-shape property inspectors.
//!
//! The inspector deliberately does not own annotation state or history. It
//! renders an application-supplied [`RectanglePropertySnapshot`] and emits
//! identity-bound [`RectanglePropertyEvent`] values for `DocumentWorkspace` to
//! validate and apply. Rectangle and Ellipse use the same implementation with
//! distinct stable identifiers and copy. Hatch and Cloud controls are intentionally absent: the
//! maintained Electron implementation does not persist hatch settings, and the
//! native annotation model supports only the working Solid, Dashed, and Dotted
//! styles.

use crate::annotation_model::{MarkupId, PdfRect, RectangleAppearance, StrokeStyle};
use crate::property_controls::{
    FieldPhase, PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
    PropertyPreviewEvent, PropertySliderInput, PropertyStepInput, canonical_picker_opacity,
    format_property_number, format_property_percentage, parse_property_percentage, property_body,
    property_lock_toggle, property_number_pair_row, property_row, property_slider_row,
    subscribe_property_color,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Disableable as _, IndexPath,
    color_picker::ColorPickerState,
    h_flex,
    input::{InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    slider::{SliderEvent, SliderState},
    switch::Switch,
    try_parse_color,
};

use crate::document_workspace::DocumentId;

pub const RECTANGLE_PROPERTY_INSPECTOR_ID: &str = "rectangle-property-inspector";
pub const RECTANGLE_INSPECTOR_PANEL_ID: &str = RECTANGLE_PROPERTY_INSPECTOR_ID;
pub const RECTANGLE_INSPECTOR_HEADER_ID: &str = "rectangle-property-inspector-header";
pub const RECTANGLE_INSPECTOR_SCROLL_ID: &str = "rectangle-property-inspector-scroll";
pub const RECTANGLE_INSPECTOR_EMPTY_ID: &str = "rectangle-property-inspector-empty";
pub const RECTANGLE_INSPECTOR_ACCORDION_ID: &str = "rectangle-property-inspector-sections";
pub const RECTANGLE_INSPECTOR_APPEARANCE_SECTION_ID: &str =
    "rectangle-property-inspector-appearance";
pub const RECTANGLE_INSPECTOR_LAYOUT_SECTION_ID: &str = "rectangle-property-inspector-layout";
pub const RECTANGLE_INSPECTOR_LOCKED_ID: &str = "rectangle-property-inspector-locked";
pub const RECTANGLE_INSPECTOR_STROKE_COLOR_ID: &str = "rectangle-property-inspector-stroke-color";
pub const RECTANGLE_INSPECTOR_OPACITY_ID: &str = "rectangle-property-inspector-opacity";
pub const RECTANGLE_INSPECTOR_STROKE_WIDTH_ID: &str = "rectangle-property-inspector-stroke-width";
pub const RECTANGLE_INSPECTOR_STROKE_STYLE_ID: &str = "rectangle-property-inspector-stroke-style";
pub const RECTANGLE_INSPECTOR_FILL_ENABLED_ID: &str = "rectangle-property-inspector-fill-enabled";
pub const RECTANGLE_INSPECTOR_FILL_COLOR_ID: &str = "rectangle-property-inspector-fill-color";
pub const RECTANGLE_INSPECTOR_FILL_OPACITY_ID: &str = "rectangle-property-inspector-fill-opacity";
pub const RECTANGLE_INSPECTOR_X_ID: &str = "rectangle-property-inspector-x";
pub const RECTANGLE_INSPECTOR_Y_ID: &str = "rectangle-property-inspector-y";
pub const RECTANGLE_INSPECTOR_WIDTH_ID: &str = "rectangle-property-inspector-width";
pub const RECTANGLE_INSPECTOR_HEIGHT_ID: &str = "rectangle-property-inspector-height";
pub const RECTANGLE_INSPECTOR_ROTATION_ID: &str = "rectangle-property-inspector-rotation";

pub const ELLIPSE_PROPERTY_INSPECTOR_ID: &str = "ellipse-property-inspector";
pub const ELLIPSE_INSPECTOR_PANEL_ID: &str = ELLIPSE_PROPERTY_INSPECTOR_ID;
pub const ELLIPSE_INSPECTOR_HEADER_ID: &str = "ellipse-property-inspector-header";
pub const ELLIPSE_INSPECTOR_SCROLL_ID: &str = "ellipse-property-inspector-scroll";
pub const ELLIPSE_INSPECTOR_EMPTY_ID: &str = "ellipse-property-inspector-empty";
pub const ELLIPSE_INSPECTOR_ACCORDION_ID: &str = "ellipse-property-inspector-sections";
pub const ELLIPSE_INSPECTOR_APPEARANCE_SECTION_ID: &str = "ellipse-property-inspector-appearance";
pub const ELLIPSE_INSPECTOR_LAYOUT_SECTION_ID: &str = "ellipse-property-inspector-layout";
pub const ELLIPSE_INSPECTOR_LOCKED_ID: &str = "ellipse-property-inspector-locked";
pub const ELLIPSE_INSPECTOR_STROKE_COLOR_ID: &str = "ellipse-property-inspector-stroke-color";
pub const ELLIPSE_INSPECTOR_OPACITY_ID: &str = "ellipse-property-inspector-opacity";
pub const ELLIPSE_INSPECTOR_STROKE_WIDTH_ID: &str = "ellipse-property-inspector-stroke-width";
pub const ELLIPSE_INSPECTOR_STROKE_STYLE_ID: &str = "ellipse-property-inspector-stroke-style";
pub const ELLIPSE_INSPECTOR_FILL_ENABLED_ID: &str = "ellipse-property-inspector-fill-enabled";
pub const ELLIPSE_INSPECTOR_FILL_COLOR_ID: &str = "ellipse-property-inspector-fill-color";
pub const ELLIPSE_INSPECTOR_FILL_OPACITY_ID: &str = "ellipse-property-inspector-fill-opacity";
pub const ELLIPSE_INSPECTOR_X_ID: &str = "ellipse-property-inspector-x";
pub const ELLIPSE_INSPECTOR_Y_ID: &str = "ellipse-property-inspector-y";
pub const ELLIPSE_INSPECTOR_WIDTH_ID: &str = "ellipse-property-inspector-width";
pub const ELLIPSE_INSPECTOR_HEIGHT_ID: &str = "ellipse-property-inspector-height";
pub const ELLIPSE_INSPECTOR_ROTATION_ID: &str = "ellipse-property-inspector-rotation";

pub const RECTANGLE_INSPECTOR_WIDTH_PX: f32 = 300.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectangularShapePropertyKind {
    Rectangle,
    Ellipse,
}

#[derive(Clone, Copy)]
struct PropertyInspectorPresentation {
    kind: RectangularShapePropertyKind,
    title: &'static str,
    empty_copy: &'static str,
    panel: &'static str,
    header: &'static str,
    scroll: &'static str,
    empty: &'static str,
    accordion: &'static str,
    appearance: &'static str,
    layout: &'static str,
    locked: &'static str,
    stroke_color: &'static str,
    opacity: &'static str,
    stroke_width: &'static str,
    stroke_style: &'static str,
    fill_enabled: &'static str,
    fill_color: &'static str,
    fill_opacity: &'static str,
    x: &'static str,
    y: &'static str,
    width: &'static str,
    height: &'static str,
    rotation: &'static str,
}

const RECTANGLE_PRESENTATION: PropertyInspectorPresentation = PropertyInspectorPresentation {
    kind: RectangularShapePropertyKind::Rectangle,
    title: "Rectangle",
    empty_copy: "Select a Rectangle to view its properties.",
    panel: RECTANGLE_INSPECTOR_PANEL_ID,
    header: RECTANGLE_INSPECTOR_HEADER_ID,
    scroll: RECTANGLE_INSPECTOR_SCROLL_ID,
    empty: RECTANGLE_INSPECTOR_EMPTY_ID,
    accordion: RECTANGLE_INSPECTOR_ACCORDION_ID,
    appearance: RECTANGLE_INSPECTOR_APPEARANCE_SECTION_ID,
    layout: RECTANGLE_INSPECTOR_LAYOUT_SECTION_ID,
    locked: RECTANGLE_INSPECTOR_LOCKED_ID,
    stroke_color: RECTANGLE_INSPECTOR_STROKE_COLOR_ID,
    opacity: RECTANGLE_INSPECTOR_OPACITY_ID,
    stroke_width: RECTANGLE_INSPECTOR_STROKE_WIDTH_ID,
    stroke_style: RECTANGLE_INSPECTOR_STROKE_STYLE_ID,
    fill_enabled: RECTANGLE_INSPECTOR_FILL_ENABLED_ID,
    fill_color: RECTANGLE_INSPECTOR_FILL_COLOR_ID,
    fill_opacity: RECTANGLE_INSPECTOR_FILL_OPACITY_ID,
    x: RECTANGLE_INSPECTOR_X_ID,
    y: RECTANGLE_INSPECTOR_Y_ID,
    width: RECTANGLE_INSPECTOR_WIDTH_ID,
    height: RECTANGLE_INSPECTOR_HEIGHT_ID,
    rotation: RECTANGLE_INSPECTOR_ROTATION_ID,
};

const ELLIPSE_PRESENTATION: PropertyInspectorPresentation = PropertyInspectorPresentation {
    kind: RectangularShapePropertyKind::Ellipse,
    title: "Ellipse",
    empty_copy: "Select an Ellipse to view its properties.",
    panel: ELLIPSE_INSPECTOR_PANEL_ID,
    header: ELLIPSE_INSPECTOR_HEADER_ID,
    scroll: ELLIPSE_INSPECTOR_SCROLL_ID,
    empty: ELLIPSE_INSPECTOR_EMPTY_ID,
    accordion: ELLIPSE_INSPECTOR_ACCORDION_ID,
    appearance: ELLIPSE_INSPECTOR_APPEARANCE_SECTION_ID,
    layout: ELLIPSE_INSPECTOR_LAYOUT_SECTION_ID,
    locked: ELLIPSE_INSPECTOR_LOCKED_ID,
    stroke_color: ELLIPSE_INSPECTOR_STROKE_COLOR_ID,
    opacity: ELLIPSE_INSPECTOR_OPACITY_ID,
    stroke_width: ELLIPSE_INSPECTOR_STROKE_WIDTH_ID,
    stroke_style: ELLIPSE_INSPECTOR_STROKE_STYLE_ID,
    fill_enabled: ELLIPSE_INSPECTOR_FILL_ENABLED_ID,
    fill_color: ELLIPSE_INSPECTOR_FILL_COLOR_ID,
    fill_opacity: ELLIPSE_INSPECTOR_FILL_OPACITY_ID,
    x: ELLIPSE_INSPECTOR_X_ID,
    y: ELLIPSE_INSPECTOR_Y_ID,
    width: ELLIPSE_INSPECTOR_WIDTH_ID,
    height: ELLIPSE_INSPECTOR_HEIGHT_ID,
    rotation: ELLIPSE_INSPECTOR_ROTATION_ID,
};

const STROKE_STYLE_SOLID: &str = "Solid";
const STROKE_STYLE_DASHED: &str = "Dashed";
const STROKE_STYLE_DOTTED: &str = "Dotted";

/// Application-owned Rectangle state rendered by the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct RectanglePropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub rect: PdfRect,
    pub rotation_degrees: f64,
    pub appearance: RectangleAppearance,
    pub locked: bool,
    /// A document/session-level mutation gate, independent of annotation lock.
    pub mutation_disabled: bool,
}

/// One typed Rectangle mutation request. Geometry variants name the edited
/// field so the application can preserve the unedited values transactionally.
#[derive(Clone, Debug, PartialEq)]
pub enum RectanglePropertyPatch {
    Locked(bool),
    StrokeColorAndOpacity { color: String, opacity: f64 },
    FillColorAndOpacity { color: String, opacity: f64 },
    StrokeColor(String),
    Opacity(f64),
    StrokeWidthPt(f64),
    StrokeStyle(StrokeStyle),
    FillColor(Option<String>),
    FillOpacity(f64),
    X(f64),
    Y(f64),
    Width(f64),
    Height(f64),
    RotationDegrees(f64),
}

/// Every event carries both stable identities so stale events cannot be
/// applied after a tab, document, or selection change.
#[derive(Clone, Debug, PartialEq)]
pub struct RectanglePropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub expected_kind: RectangularShapePropertyKind,
    pub patch: RectanglePropertyPatch,
}

pub type EllipsePropertySnapshot = RectanglePropertySnapshot;
pub type EllipsePropertyPatch = RectanglePropertyPatch;
pub type EllipsePropertyEvent = RectanglePropertyEvent;
pub type EllipsePropertyInspector = RectanglePropertyInspector;

#[derive(Clone, Copy)]
enum NumericProperty {
    StrokeWidth,
    X,
    Y,
    Width,
    Height,
    Rotation,
}

/// Retains GPUI Component input and popup state only. Annotation state remains
/// in `NativeDocumentSession.annotations` and is refreshed through
/// [`Self::sync_from_snapshot`].
pub struct RectanglePropertyInspector {
    presentation: &'static PropertyInspectorPresentation,
    snapshot: Option<RectanglePropertySnapshot>,
    syncing: bool,
    open: bool,
    embedded: bool,
    stroke_width: gpui::Entity<InputState>,
    stroke_width_slider: gpui::Entity<SliderState>,
    rotation_slider: gpui::Entity<SliderState>,
    x: gpui::Entity<InputState>,
    y: gpui::Entity<InputState>,
    width: gpui::Entity<InputState>,
    height: gpui::Entity<InputState>,
    rotation: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    opacity_input: gpui::Entity<InputState>,
    fill_opacity: gpui::Entity<SliderState>,
    fill_opacity_input: gpui::Entity<InputState>,
    stroke_color: gpui::Entity<ColorPickerState>,
    fill_color: gpui::Entity<ColorPickerState>,
    stroke_style: gpui::Entity<SelectState<Vec<SharedString>>>,
    _subscriptions: Vec<Subscription>,
}

impl RectanglePropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_presentation(&RECTANGLE_PRESENTATION, window, cx)
    }

    pub fn new_ellipse(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_presentation(&ELLIPSE_PRESENTATION, window, cx)
    }

    fn new_with_presentation(
        presentation: &'static PropertyInspectorPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let stroke_width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.)
                .max(20.)
        });
        let x = cx.new(|cx| InputState::new(window, cx).default_value("0").step(1.));
        let y = cx.new(|cx| InputState::new(window, cx).default_value("0").step(1.));
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("0")
                .min(0.)
                .step(1.)
        });
        let height = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("0")
                .min(0.)
                .step(1.)
        });
        let rotation = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("0")
                .min(0.)
                .max(359.)
                .step(1.)
        });
        let stroke_width_slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(20.)
                .step(0.25)
                .default_value(1.)
        });
        let rotation_slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(359.)
                .step(1.)
                .default_value(0.)
        });
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let opacity_input = cx.new(|cx| InputState::new(window, cx).default_value("100"));
        let fill_opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let fill_opacity_input = cx.new(|cx| InputState::new(window, cx).default_value("100"));
        let stroke_color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ff0000").expect("the built-in stroke color must parse"),
            )
        });
        let fill_color = cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(
                try_parse_color("#ffffff").expect("the built-in fill color must parse"),
            )
        });
        let stroke_style = cx.new(|cx| {
            SelectState::new(
                vec![
                    STROKE_STYLE_SOLID.into(),
                    STROKE_STYLE_DASHED.into(),
                    STROKE_STYLE_DOTTED.into(),
                ],
                Some(IndexPath::default()),
                window,
                cx,
            )
        });

        let mut subscriptions = Vec::new();
        subscriptions.push(crate::property_controls::subscribe_property_slider(
            &stroke_width_slider,
            &stroke_width,
            window,
            cx,
            |this| !this.syncing,
            |this, value, cx| this.preview_patch(RectanglePropertyPatch::StrokeWidthPt(value), cx),
            |this, value, cx| {
                this.end_preview(cx);
                this.emit_patch(RectanglePropertyPatch::StrokeWidthPt(value), cx)
            },
        ));
        subscriptions.push(crate::property_controls::subscribe_property_slider(
            &rotation_slider,
            &rotation,
            window,
            cx,
            |this| !this.syncing,
            |this, value, cx| {
                this.preview_patch(RectanglePropertyPatch::RotationDegrees(value), cx)
            },
            |this, value, cx| {
                this.end_preview(cx);
                this.emit_patch(RectanglePropertyPatch::RotationDegrees(value), cx)
            },
        ));
        subscriptions.push(crate::property_controls::subscribe_number_steps(
            &stroke_width,
            0.5,
            0.,
            20.,
            window,
            cx,
        ));
        // The wheel steps each number by what its keys would.
        for (input, step, min, max) in [
            (&x, 1., -100_000., 100_000.),
            (&y, 1., -100_000., 100_000.),
            (&width, 1., 0., 100_000.),
            (&height, 1., 0., 100_000.),
            (&rotation, 1., 0., 359.),
        ] {
            crate::property_wheel::register_number_wheel(input, step, min, max, cx);
        }
        macro_rules! subscribe_number {
            ($input:expr, $property:expr) => {
                subscriptions.push(cx.subscribe_in(
                    &$input,
                    window,
                    move |this, input, event: &InputEvent, _, cx| {
                        this.handle_number_input($property, input, event, cx);
                    },
                ));
            };
        }
        subscribe_number!(stroke_width, NumericProperty::StrokeWidth);
        subscribe_number!(x, NumericProperty::X);
        subscribe_number!(y, NumericProperty::Y);
        subscribe_number!(width, NumericProperty::Width);
        subscribe_number!(height, NumericProperty::Height);
        subscribe_number!(rotation, NumericProperty::Rotation);

        let opacity_input_for_slider = opacity_input.clone();
        subscriptions.push(cx.subscribe_in(
            &opacity,
            window,
            move |this, _, event: &SliderEvent, window, cx| match event {
                SliderEvent::Change(value) if !this.syncing => {
                    opacity_input_for_slider.update(cx, |input, cx| {
                        input.set_value(format_property_number(value.start().into()), window, cx)
                    });
                    this.preview_patch(
                        RectanglePropertyPatch::Opacity(value.start() as f64 / 100.),
                        cx,
                    );
                    cx.notify();
                }
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => {
                    this.end_preview(cx);
                    this.emit_patch(
                        RectanglePropertyPatch::Opacity(value.start() as f64 / 100.),
                        cx,
                    )
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &opacity_input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                let Some(phase) = FieldPhase::of(event) else {
                    return;
                };
                let Some(opacity) = parse_property_percentage(input.read(cx).value().as_ref())
                else {
                    this.field_unparsed(phase, cx);
                    if phase == FieldPhase::Preview {
                        return;
                    }
                    let canonical = this
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.appearance.opacity())
                        .unwrap_or(1.);
                    input.update(cx, |input, cx| {
                        input.set_value(format_property_percentage(canonical), window, cx)
                    });
                    return;
                };
                this.apply_field(RectanglePropertyPatch::Opacity(opacity), phase, cx);
            },
        ));
        let fill_opacity_input_for_slider = fill_opacity_input.clone();
        subscriptions.push(cx.subscribe_in(
            &fill_opacity,
            window,
            move |this, _, event: &SliderEvent, window, cx| match event {
                SliderEvent::Change(value) if !this.syncing => {
                    fill_opacity_input_for_slider.update(cx, |input, cx| {
                        input.set_value(format_property_number(value.start().into()), window, cx)
                    });
                    this.preview_patch(
                        RectanglePropertyPatch::FillOpacity(value.start() as f64 / 100.),
                        cx,
                    );
                    cx.notify();
                }
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => {
                    this.end_preview(cx);
                    this.emit_patch(
                        RectanglePropertyPatch::FillOpacity(value.start() as f64 / 100.),
                        cx,
                    )
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &fill_opacity_input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                let Some(phase) = FieldPhase::of(event) else {
                    return;
                };
                let Some(opacity) = parse_property_percentage(input.read(cx).value().as_ref())
                else {
                    this.field_unparsed(phase, cx);
                    if phase == FieldPhase::Preview {
                        return;
                    }
                    let canonical = this
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.appearance.fill_opacity())
                        .unwrap_or(1.);
                    input.update(cx, |input, cx| {
                        input.set_value(format_property_percentage(canonical), window, cx)
                    });
                    return;
                };
                this.apply_field(RectanglePropertyPatch::FillOpacity(opacity), phase, cx);
            },
        ));
        subscriptions.extend(subscribe_property_color(
            &stroke_color,
            cx,
            |this: &mut Self, cx| {
                let (Some(color), Some(appearance)) = (
                    this.stroke_color.read(cx).value(),
                    this.snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.appearance.clone()),
                ) else {
                    return;
                };
                this.emit_patch(
                    RectanglePropertyPatch::StrokeColorAndOpacity {
                        color: opaque_hex(color),
                        opacity: canonical_picker_opacity(color.a, appearance.opacity()),
                    },
                    cx,
                );
            },
        ));
        subscriptions.extend(subscribe_property_color(
            &fill_color,
            cx,
            |this: &mut Self, cx| {
                let (Some(color), Some(appearance)) = (
                    this.fill_color.read(cx).value(),
                    this.snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.appearance.clone()),
                ) else {
                    return;
                };
                this.emit_patch(
                    RectanglePropertyPatch::FillColorAndOpacity {
                        color: opaque_hex(color),
                        opacity: canonical_picker_opacity(color.a, appearance.fill_opacity()),
                    },
                    cx,
                );
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &stroke_style,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                let style = match value.as_ref() {
                    STROKE_STYLE_DASHED => StrokeStyle::Dashed,
                    STROKE_STYLE_DOTTED => StrokeStyle::Dotted,
                    _ => StrokeStyle::Solid,
                };
                this.emit_patch(RectanglePropertyPatch::StrokeStyle(style), cx);
            },
        ));

        Self {
            presentation,
            snapshot: None,
            syncing: false,
            open: false,
            embedded: false,
            stroke_width,
            stroke_width_slider,
            rotation_slider,
            x,
            y,
            width,
            height,
            rotation,
            opacity,
            opacity_input,
            fill_opacity,
            fill_opacity_input,
            stroke_color,
            fill_color,
            stroke_style,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&RectanglePropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn set_embedded(&mut self) {
        self.embedded = true;
    }

    pub fn target(&self) -> Option<(DocumentId, MarkupId)> {
        self.snapshot
            .as_ref()
            .map(|snapshot| (snapshot.document_id, snapshot.annotation_id.clone()))
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.open = true;
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        cx.notify();
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.snapshot = None;
        cx.notify();
    }

    /// Replaces every visible value from application-owned state without
    /// emitting a mutation event.
    pub fn sync(
        &mut self,
        snapshot: RectanglePropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        let stroke_width = snapshot.appearance.stroke_width_pt();
        let opacity = snapshot.appearance.opacity();
        let fill_opacity = snapshot.appearance.fill_opacity();
        let stroke_color = snapshot.appearance.stroke_color().to_owned();
        let fill_color = snapshot
            .appearance
            .fill_color()
            .unwrap_or("#ffffff")
            .to_owned();
        let stroke_style = snapshot.appearance.stroke_style();
        let rect = snapshot.rect;
        let rotation = snapshot.rotation_degrees.rem_euclid(360.);
        self.stroke_width_slider.update(cx, |slider, cx| {
            slider.set_value(stroke_width as f32, window, cx)
        });
        self.rotation_slider.update(cx, |slider, cx| {
            slider.set_value(rotation as f32, window, cx)
        });

        for (input, value) in [
            (&self.x, rect.x),
            (&self.y, rect.y),
            (&self.width, rect.width),
            (&self.height, rect.height),
        ] {
            input.update(cx, |input, cx| {
                input.set_value(
                    crate::property_controls::format_property_geometry(value),
                    window,
                    cx,
                );
            });
        }
        for (input, value) in [
            (&self.stroke_width, stroke_width),
            (&self.rotation, rotation),
        ] {
            input.update(cx, |input, cx| {
                input.set_value(format_property_number(value), window, cx)
            });
        }
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((opacity * 100.) as f32, window, cx);
        });
        self.opacity_input.update(cx, |input, cx| {
            input.set_value(format_property_percentage(opacity), window, cx);
        });
        self.fill_opacity.update(cx, |slider, cx| {
            slider.set_value((fill_opacity * 100.) as f32, window, cx);
        });
        self.fill_opacity_input.update(cx, |input, cx| {
            input.set_value(format_property_percentage(fill_opacity), window, cx);
        });
        if let Ok(mut color) = try_parse_color(&stroke_color) {
            color.a = opacity as f32;
            self.stroke_color.update(cx, |picker, cx| {
                picker.set_value(color, window, cx);
            });
        }
        if let Ok(mut color) = try_parse_color(&fill_color) {
            color.a = fill_opacity as f32;
            self.fill_color.update(cx, |picker, cx| {
                picker.set_value(color, window, cx);
            });
        }
        let style_label: SharedString = match stroke_style {
            StrokeStyle::Solid => STROKE_STYLE_SOLID.into(),
            StrokeStyle::Dashed => STROKE_STYLE_DASHED.into(),
            StrokeStyle::Dotted => STROKE_STYLE_DOTTED.into(),
        };
        self.stroke_style.update(cx, |select, cx| {
            select.set_selected_value(&style_label, window, cx);
        });

        self.snapshot = Some(snapshot);
        self.syncing = false;
        cx.notify();
    }

    pub fn sync_from_snapshot(
        &mut self,
        snapshot: RectanglePropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync(snapshot, window, cx);
    }

    pub fn stroke_width_input(&self) -> gpui::Entity<InputState> {
        self.stroke_width.clone()
    }

    pub fn stroke_width_slider(&self) -> gpui::Entity<SliderState> {
        self.stroke_width_slider.clone()
    }

    pub fn rotation_slider(&self) -> gpui::Entity<SliderState> {
        self.rotation_slider.clone()
    }

    pub fn x_input(&self) -> gpui::Entity<InputState> {
        self.x.clone()
    }

    pub fn y_input(&self) -> gpui::Entity<InputState> {
        self.y.clone()
    }

    pub fn width_input(&self) -> gpui::Entity<InputState> {
        self.width.clone()
    }

    pub fn height_input(&self) -> gpui::Entity<InputState> {
        self.height.clone()
    }

    pub fn rotation_input(&self) -> gpui::Entity<InputState> {
        self.rotation.clone()
    }

    pub fn opacity_input(&self) -> gpui::Entity<InputState> {
        self.opacity_input.clone()
    }

    pub fn fill_opacity_input(&self) -> gpui::Entity<InputState> {
        self.fill_opacity_input.clone()
    }

    pub fn stroke_color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.stroke_color.clone()
    }

    pub fn fill_color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.fill_color.clone()
    }

    pub fn visible_stable_ids(&self) -> Vec<&'static str> {
        if !(self.open || self.embedded) {
            return Vec::new();
        }
        let ids = self.presentation;
        if self.snapshot.is_none() {
            return vec![ids.panel, ids.header, ids.empty];
        }
        let mut ids = vec![
            ids.panel,
            ids.header,
            ids.scroll,
            ids.accordion,
            ids.locked,
            ids.appearance,
            ids.stroke_color,
            ids.opacity,
            ids.stroke_width,
            ids.stroke_style,
            ids.fill_enabled,
            ids.fill_color,
            ids.layout,
            ids.x,
            ids.y,
            ids.width,
            ids.height,
            ids.rotation,
        ];
        // Fill opacity shows only once there is a fill.
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.appearance.fill_color().is_some())
        {
            ids.push(self.presentation.fill_opacity);
        }
        ids
    }

    fn handle_number_input(
        &mut self,
        property: NumericProperty,
        input: &gpui::Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(phase) = FieldPhase::of(event) else {
            return;
        };
        // A field that still shows the committed, rounded value changes
        // nothing: it must not turn the rounded text into the markup's value.
        if let Some(snapshot) = &self.snapshot {
            let canonical = match property {
                NumericProperty::X => Some(snapshot.rect.x),
                NumericProperty::Y => Some(snapshot.rect.y),
                NumericProperty::Width => Some(snapshot.rect.width),
                NumericProperty::Height => Some(snapshot.rect.height),
                _ => None,
            };
            if canonical.is_some_and(|value| {
                input.read(cx).value().as_ref()
                    == crate::property_controls::format_property_geometry(value)
            }) {
                self.end_preview(cx);
                return;
            }
        }
        let Ok(value) = input.read(cx).value().parse::<f64>() else {
            self.field_unparsed(phase, cx);
            return;
        };
        if !value.is_finite() {
            self.field_unparsed(phase, cx);
            return;
        }
        let patch = match property {
            NumericProperty::StrokeWidth => {
                RectanglePropertyPatch::StrokeWidthPt(value.clamp(0., 20.))
            }
            NumericProperty::X => RectanglePropertyPatch::X(value),
            NumericProperty::Y => RectanglePropertyPatch::Y(value),
            NumericProperty::Width => RectanglePropertyPatch::Width(value.max(0.)),
            NumericProperty::Height => RectanglePropertyPatch::Height(value.max(0.)),
            NumericProperty::Rotation => {
                RectanglePropertyPatch::RotationDegrees(value.rem_euclid(360.))
            }
        };
        self.apply_field(patch, phase, cx);
    }

    /// The event that applies `patch`, unless it changes nothing or the
    /// markup cannot be edited.
    fn patch_event(&self, patch: RectanglePropertyPatch) -> Option<RectanglePropertyEvent> {
        let snapshot = self.snapshot.as_ref()?;
        if !(self.open || self.embedded)
            || self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, RectanglePropertyPatch::Locked(_)))
            || patch_matches_snapshot(&patch, snapshot)
        {
            return None;
        }
        Some(RectanglePropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_kind: self.presentation.kind,
            patch,
        })
    }

    fn emit_patch(&mut self, patch: RectanglePropertyPatch, cx: &mut Context<Self>) {
        if let Some(event) = self.patch_event(patch) {
            cx.emit(event);
        }
    }

    /// Shows `patch` on the canvas while a slider is dragged.
    fn preview_patch(&mut self, patch: RectanglePropertyPatch, cx: &mut Context<Self>) {
        cx.emit(match self.patch_event(patch) {
            Some(event) => PropertyPreviewEvent::Show(event),
            None => PropertyPreviewEvent::Clear,
        });
    }

    /// Shows the committed markup again, as a drag ends.
    fn end_preview(&mut self, cx: &mut Context<Self>) {
        cx.emit(PropertyPreviewEvent::<RectanglePropertyEvent>::Clear);
    }

    /// A typed value: shown on the markup as it is typed, committed when the
    /// field is left or Enter is pressed.
    fn apply_field(
        &mut self,
        patch: RectanglePropertyPatch,
        phase: FieldPhase,
        cx: &mut Context<Self>,
    ) {
        match phase {
            FieldPhase::Preview => self.preview_patch(patch, cx),
            FieldPhase::Commit => {
                self.end_preview(cx);
                self.emit_patch(patch, cx);
            }
        }
    }

    /// Typed text that is not a valid value: a partial one keeps the last
    /// preview, and leaving the field ends it.
    fn field_unparsed(&mut self, phase: FieldPhase, cx: &mut Context<Self>) {
        match phase {
            FieldPhase::Preview => cx.emit(PropertyPreviewEvent::<RectanglePropertyEvent>::Pending),
            FieldPhase::Commit => self.end_preview(cx),
        }
    }
}

impl EventEmitter<RectanglePropertyEvent> for RectanglePropertyInspector {}
impl EventEmitter<PropertyPreviewEvent<RectanglePropertyEvent>> for RectanglePropertyInspector {}

impl Render for RectanglePropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ids = self.presentation;
        if !(self.open || self.embedded) {
            return div()
                .id(ids.panel)
                .debug_selector(move || ids.panel.into())
                .hidden()
                .into_any_element();
        }

        let Some(snapshot) = self.snapshot.clone() else {
            return PropertyInspectorPanel::new(ids.panel, ids.header, ids.scroll, ids.title)
                .child(
                    div()
                        .id(ids.empty)
                        .debug_selector(move || ids.empty.into())
                        .p_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(ids.empty_copy),
                )
                .into_any_element();
        };

        let mutation_disabled = snapshot.mutation_disabled;
        let property_disabled = mutation_disabled || snapshot.locked;
        let fill_enabled = snapshot.appearance.fill_color().is_some();
        let lock_control = cx.entity().downgrade();
        let fill_control = cx.entity().downgrade();
        let fill_color = self.fill_color.clone();
        let number = |id: &'static str, label: &'static str, prefix: &'static str, state| {
            PropertyNumericInput::new(id, label, state)
                .prefix(prefix)
                .disabled(property_disabled)
        };

        let appearance = PropertyGroup::new()
            .row(property_row(
                "Line colour",
                PropertyColor::new(ids.stroke_color, "Line colour", &self.stroke_color)
                    .disabled(property_disabled),
            ))
            .row(property_row(
                "Line width",
                PropertyStepInput::new(ids.stroke_width, &self.stroke_width)
                    .suffix("pt")
                    .disabled(property_disabled),
            ))
            .row(property_row(
                "Style",
                div()
                    .id(ids.stroke_style)
                    .debug_selector(move || ids.stroke_style.into())
                    .child(Select::new(&self.stroke_style).disabled(property_disabled)),
            ))
            .row(property_slider_row(
                "Opacity",
                PropertySliderInput::new("Opacity", &self.opacity, &self.opacity_input)
                    .row_id(ids.opacity)
                    .suffix("%")
                    .disabled(property_disabled),
            ))
            .row(property_row(
                "Fill",
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .id(ids.fill_enabled)
                            .debug_selector(move || ids.fill_enabled.into())
                            .flex_none()
                            .child(
                                Switch::new(match ids.kind {
                                    RectangularShapePropertyKind::Rectangle => {
                                        "rectangle-property-inspector-fill-enabled-switch"
                                    }
                                    RectangularShapePropertyKind::Ellipse => {
                                        "ellipse-property-inspector-fill-enabled-switch"
                                    }
                                })
                                .checked(fill_enabled)
                                .disabled(property_disabled)
                                .tooltip(if fill_enabled {
                                    "Remove the fill"
                                } else {
                                    "Fill it"
                                })
                                .on_click(
                                    move |enabled, _, cx| {
                                        let color = fill_color
                                            .read(cx)
                                            .value()
                                            .map(opaque_hex)
                                            .unwrap_or_else(|| "#ffffff".into());
                                        let patch = RectanglePropertyPatch::FillColor(
                                            (*enabled).then_some(color),
                                        );
                                        let _ = fill_control.update(cx, |inspector, cx| {
                                            inspector.emit_patch(patch, cx);
                                        });
                                    },
                                ),
                            ),
                    )
                    // Picking a colour fills it.
                    .child(
                        PropertyColor::new(ids.fill_color, "Fill colour", &self.fill_color)
                            .disabled(property_disabled),
                    ),
            ))
            // Shown with no fill too, disabled, so the panel keeps its shape.
            .row(
                property_slider_row(
                    "Fill opacity",
                    PropertySliderInput::new(
                        "Fill opacity",
                        &self.fill_opacity,
                        &self.fill_opacity_input,
                    )
                    .row_id(ids.fill_opacity)
                    .suffix("%")
                    .disabled(property_disabled || !fill_enabled),
                )
                .dependent(),
            );

        let layout = PropertyGroup::new()
            .row(property_number_pair_row(
                "Position (pt)",
                number(ids.x, "X", "X", &self.x),
                number(ids.y, "Y", "Y", &self.y),
            ))
            .row(property_number_pair_row(
                "Size (pt)",
                number(ids.width, "Width", "W", &self.width),
                number(ids.height, "Height", "H", &self.height),
            ))
            .row(property_slider_row(
                "Rotation",
                PropertySliderInput::new("Rotation", &self.rotation_slider, &self.rotation)
                    .slider_id(match ids.kind {
                        RectangularShapePropertyKind::Rectangle => {
                            "rectangle-property-inspector-rotation-slider"
                        }
                        RectangularShapePropertyKind::Ellipse => {
                            "ellipse-property-inspector-rotation-slider"
                        }
                    })
                    .input_id(ids.rotation)
                    .suffix("°")
                    .disabled(property_disabled),
            ));

        PropertyInspectorPanel::new(ids.panel, ids.header, ids.scroll, ids.title)
            .header_lock(property_lock_toggle(
                ids.locked,
                snapshot.locked,
                mutation_disabled,
                move |locked, _, cx| {
                    let _ = lock_control.update(cx, |inspector, cx| {
                        inspector.emit_patch(RectanglePropertyPatch::Locked(locked), cx);
                    });
                },
            ))
            .child(
                property_body()
                    .id(ids.accordion)
                    .debug_selector(move || ids.accordion.into())
                    .child(
                        div()
                            .id(ids.appearance)
                            .debug_selector(move || ids.appearance.into())
                            .child(appearance),
                    )
                    .child(
                        div()
                            .id(ids.layout)
                            .debug_selector(move || ids.layout.into())
                            .child(layout),
                    ),
            )
            .into_any_element()
    }
}

fn opaque_hex(color: gpui::Hsla) -> String {
    let hex = color.to_hex();
    hex.get(..7).unwrap_or(hex.as_str()).to_ascii_lowercase()
}

fn patch_matches_snapshot(
    patch: &RectanglePropertyPatch,
    snapshot: &RectanglePropertySnapshot,
) -> bool {
    match patch {
        RectanglePropertyPatch::Locked(value) => snapshot.locked == *value,
        RectanglePropertyPatch::StrokeColorAndOpacity { color, opacity } => {
            snapshot
                .appearance
                .stroke_color()
                .eq_ignore_ascii_case(color)
                && snapshot.appearance.opacity() == *opacity
        }
        RectanglePropertyPatch::FillColorAndOpacity { color, opacity } => {
            snapshot
                .appearance
                .fill_color()
                .is_some_and(|current| current.eq_ignore_ascii_case(color))
                && snapshot.appearance.fill_opacity() == *opacity
        }
        RectanglePropertyPatch::StrokeColor(value) => snapshot
            .appearance
            .stroke_color()
            .eq_ignore_ascii_case(value),
        RectanglePropertyPatch::Opacity(value) => snapshot.appearance.opacity() == *value,
        RectanglePropertyPatch::StrokeWidthPt(value) => {
            snapshot.appearance.stroke_width_pt() == *value
        }
        RectanglePropertyPatch::StrokeStyle(value) => snapshot.appearance.stroke_style() == *value,
        RectanglePropertyPatch::FillColor(value) => match (snapshot.appearance.fill_color(), value)
        {
            (None, None) => true,
            (Some(current), Some(value)) => current.eq_ignore_ascii_case(value),
            _ => false,
        },
        RectanglePropertyPatch::FillOpacity(value) => snapshot.appearance.fill_opacity() == *value,
        RectanglePropertyPatch::X(value) => snapshot.rect.x == *value,
        RectanglePropertyPatch::Y(value) => snapshot.rect.y == *value,
        RectanglePropertyPatch::Width(value) => snapshot.rect.width == *value,
        RectanglePropertyPatch::Height(value) => snapshot.rect.height == *value,
        RectanglePropertyPatch::RotationDegrees(value) => {
            snapshot.rotation_degrees.rem_euclid(360.) == value.rem_euclid(360.)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_stable_numeric_input_without_machine_specific_precision_noise() {
        assert_eq!(format_property_number(12.), "12");
        assert_eq!(format_property_number(12.5), "12.5");
        assert_eq!(format_property_number(0.000_001), "0.000001");
    }

    #[test]
    fn advertises_only_the_three_working_native_stroke_styles() {
        assert_eq!(STROKE_STYLE_SOLID, "Solid");
        assert_eq!(STROKE_STYLE_DASHED, "Dashed");
        assert_eq!(STROKE_STYLE_DOTTED, "Dotted");
    }
}
