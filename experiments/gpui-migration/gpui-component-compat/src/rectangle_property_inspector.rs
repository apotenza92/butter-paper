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

use butter_paper_gpui_gallery::annotation_model::{
    MarkupId, PdfRect, RectangleAppearance, StrokeStyle,
};
use gpui::{
    AnyElement, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Disableable as _, IndexPath, StyledExt as _,
    accordion::Accordion,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    form::Field,
    h_flex,
    input::{InputEvent, InputState, NumberInput},
    scroll::ScrollableElement as _,
    select::{Select, SelectEvent, SelectState},
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
    try_parse_color, v_flex,
};

use crate::document_workspace::DocumentId;

pub const RECTANGLE_PROPERTY_INSPECTOR_ID: &str = "rectangle-property-inspector";
pub const RECTANGLE_INSPECTOR_PANEL_ID: &str = RECTANGLE_PROPERTY_INSPECTOR_ID;
pub const RECTANGLE_INSPECTOR_HEADER_ID: &str = "rectangle-property-inspector-header";
pub const RECTANGLE_INSPECTOR_SCROLL_ID: &str = "rectangle-property-inspector-scroll";
pub const RECTANGLE_INSPECTOR_EMPTY_ID: &str = "rectangle-property-inspector-empty";
pub const RECTANGLE_INSPECTOR_ACCORDION_ID: &str = "rectangle-property-inspector-sections";
pub const RECTANGLE_INSPECTOR_DETAILS_SECTION_ID: &str = "rectangle-property-inspector-details";
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
pub const ELLIPSE_INSPECTOR_DETAILS_SECTION_ID: &str = "ellipse-property-inspector-details";
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
    details: &'static str,
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
    details: RECTANGLE_INSPECTOR_DETAILS_SECTION_ID,
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
    details: ELLIPSE_INSPECTOR_DETAILS_SECTION_ID,
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
    open_sections: [bool; 3],
    stroke_width: gpui::Entity<InputState>,
    x: gpui::Entity<InputState>,
    y: gpui::Entity<InputState>,
    width: gpui::Entity<InputState>,
    height: gpui::Entity<InputState>,
    rotation: gpui::Entity<InputState>,
    opacity: gpui::Entity<SliderState>,
    fill_opacity: gpui::Entity<SliderState>,
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
                .step(0.25)
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
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let fill_opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
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

        subscriptions.push(cx.subscribe(
            &opacity,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) => {}
                SliderEvent::Release(value) => this.emit_patch(
                    RectanglePropertyPatch::Opacity(value.start() as f64 / 100.),
                    cx,
                ),
            },
        ));
        subscriptions.push(
            cx.subscribe(
                &fill_opacity,
                |this, _, event: &SliderEvent, cx| match event {
                    SliderEvent::Change(_) => {}
                    SliderEvent::Release(value) => this.emit_patch(
                        RectanglePropertyPatch::FillOpacity(value.start() as f64 / 100.),
                        cx,
                    ),
                },
            ),
        );
        subscriptions.push(
            cx.subscribe(&stroke_color, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                this.emit_patch(RectanglePropertyPatch::StrokeColor(opaque_hex(*color)), cx);
            }),
        );
        subscriptions.push(
            cx.subscribe(&fill_color, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                this.emit_patch(
                    RectanglePropertyPatch::FillColor(Some(opaque_hex(*color))),
                    cx,
                );
            }),
        );
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
            open_sections: [true, true, true],
            stroke_width,
            x,
            y,
            width,
            height,
            rotation,
            opacity,
            fill_opacity,
            stroke_color,
            fill_color,
            stroke_style,
            _subscriptions: subscriptions,
        }
    }

    pub fn snapshot(&self) -> Option<&RectanglePropertySnapshot> {
        self.snapshot.as_ref()
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

        for (input, value) in [
            (&self.stroke_width, stroke_width),
            (&self.x, rect.x),
            (&self.y, rect.y),
            (&self.width, rect.width),
            (&self.height, rect.height),
            (&self.rotation, rotation),
        ] {
            input.update(cx, |input, cx| {
                input.set_value(format_number(value), window, cx);
            });
        }
        self.opacity.update(cx, |slider, cx| {
            slider.set_value((opacity * 100.) as f32, window, cx);
        });
        self.fill_opacity.update(cx, |slider, cx| {
            slider.set_value((fill_opacity * 100.) as f32, window, cx);
        });
        if let Ok(color) = try_parse_color(&stroke_color) {
            self.stroke_color.update(cx, |picker, cx| {
                picker.set_value(color, window, cx);
            });
        }
        if let Ok(color) = try_parse_color(&fill_color) {
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

    pub fn visible_stable_ids(&self) -> Vec<&'static str> {
        if !self.open {
            return Vec::new();
        }
        let ids = self.presentation;
        if self.snapshot.is_none() {
            return vec![ids.panel, ids.header, ids.empty];
        }
        vec![
            ids.panel,
            ids.header,
            ids.scroll,
            ids.accordion,
            ids.details,
            ids.locked,
            ids.appearance,
            ids.stroke_color,
            ids.opacity,
            ids.stroke_width,
            ids.stroke_style,
            ids.fill_enabled,
            ids.fill_color,
            ids.fill_opacity,
            ids.layout,
            ids.x,
            ids.y,
            ids.width,
            ids.height,
            ids.rotation,
        ]
    }

    fn handle_number_input(
        &mut self,
        property: NumericProperty,
        input: &gpui::Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
            return;
        }
        let Ok(value) = input.read(cx).value().parse::<f64>() else {
            return;
        };
        if !value.is_finite() {
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
        self.emit_patch(patch, cx);
    }

    fn emit_patch(&mut self, patch: RectanglePropertyPatch, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if self.syncing
            || snapshot.mutation_disabled
            || (snapshot.locked && !matches!(patch, RectanglePropertyPatch::Locked(_)))
            || patch_matches_snapshot(&patch, snapshot)
        {
            return;
        }
        cx.emit(RectanglePropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            expected_kind: self.presentation.kind,
            patch,
        });
    }
}

impl EventEmitter<RectanglePropertyEvent> for RectanglePropertyInspector {}

impl Render for RectanglePropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ids = self.presentation;
        if !self.open {
            return div()
                .id(ids.panel)
                .debug_selector(move || ids.panel.into())
                .hidden()
                .into_any_element();
        }

        let Some(snapshot) = self.snapshot.clone() else {
            return v_flex()
                .id(ids.panel)
                .debug_selector(move || ids.panel.into())
                .w(px(RECTANGLE_INSPECTOR_WIDTH_PX))
                .h_full()
                .min_h_0()
                .flex_none()
                .border_l_1()
                .border_color(cx.theme().border)
                .child(inspector_header(ids))
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
        let section_state = self.open_sections;
        let section_control = cx.entity().downgrade();
        let lock_control = cx.entity().downgrade();
        let fill_control = cx.entity().downgrade();
        let fill_color = self.fill_color.clone();

        let details = v_flex()
            .id(ids.details)
            .debug_selector(move || ids.details.into())
            .gap_3()
            .child(
                Field::new().label("Locked").child(
                    div()
                        .id(ids.locked)
                        .debug_selector(move || ids.locked.into())
                        .w(px(36.))
                        .h(px(20.))
                        .child(
                            Switch::new(match ids.kind {
                                RectangularShapePropertyKind::Rectangle => {
                                    "rectangle-property-inspector-locked-switch"
                                }
                                RectangularShapePropertyKind::Ellipse => {
                                    "ellipse-property-inspector-locked-switch"
                                }
                            })
                            .label("Locked")
                            .checked(snapshot.locked)
                            .disabled(mutation_disabled)
                            .w(px(36.))
                            .h(px(20.))
                            .overflow_hidden()
                            .on_click(move |locked, _, cx| {
                                let _ = lock_control.update(cx, |inspector, cx| {
                                    inspector
                                        .emit_patch(RectanglePropertyPatch::Locked(*locked), cx);
                                });
                            }),
                        ),
                ),
            );

        let appearance = v_flex()
            .id(ids.appearance)
            .debug_selector(move || ids.appearance.into())
            .gap_3()
            .child(color_field(
                ids.stroke_color,
                "Color",
                &self.stroke_color,
                property_disabled,
            ))
            .child(slider_field(
                ids.opacity,
                "Opacity",
                &self.opacity,
                snapshot.appearance.opacity() * 100.,
                property_disabled,
            ))
            .child(number_field(
                ids.stroke_width,
                "Line Width",
                &self.stroke_width,
                "pt",
                property_disabled,
            ))
            .child(
                Field::new().label("Line Style").child(
                    div()
                        .id(ids.stroke_style)
                        .debug_selector(move || ids.stroke_style.into())
                        .child(Select::new(&self.stroke_style).disabled(property_disabled)),
                ),
            )
            .child(
                Field::new().label("Fill").child(
                    div()
                        .id(ids.fill_enabled)
                        .debug_selector(move || ids.fill_enabled.into())
                        .w(px(36.))
                        .h(px(20.))
                        .child(
                            Switch::new(match ids.kind {
                                RectangularShapePropertyKind::Rectangle => {
                                    "rectangle-property-inspector-fill-enabled-switch"
                                }
                                RectangularShapePropertyKind::Ellipse => {
                                    "ellipse-property-inspector-fill-enabled-switch"
                                }
                            })
                            .label("Fill")
                            .checked(fill_enabled)
                            .disabled(property_disabled)
                            .w(px(36.))
                            .h(px(20.))
                            .overflow_hidden()
                            .on_click(move |enabled, _, cx| {
                                let color = fill_color
                                    .read(cx)
                                    .value()
                                    .map(opaque_hex)
                                    .unwrap_or_else(|| "#ffffff".into());
                                let patch =
                                    RectanglePropertyPatch::FillColor((*enabled).then_some(color));
                                let _ = fill_control.update(cx, |inspector, cx| {
                                    inspector.emit_patch(patch, cx);
                                });
                            }),
                        ),
                ),
            )
            .child(color_field(
                ids.fill_color,
                "Fill Color",
                &self.fill_color,
                property_disabled || !fill_enabled,
            ))
            .child(slider_field(
                ids.fill_opacity,
                "Fill Opacity",
                &self.fill_opacity,
                snapshot.appearance.fill_opacity() * 100.,
                property_disabled || !fill_enabled,
            ));

        let layout = v_flex()
            .id(ids.layout)
            .debug_selector(move || ids.layout.into())
            .gap_3()
            .child(number_field(ids.x, "X", &self.x, "pt", property_disabled))
            .child(number_field(ids.y, "Y", &self.y, "pt", property_disabled))
            .child(number_field(
                ids.width,
                "Width",
                &self.width,
                "pt",
                property_disabled,
            ))
            .child(number_field(
                ids.height,
                "Height",
                &self.height,
                "pt",
                property_disabled,
            ))
            .child(number_field(
                ids.rotation,
                "Rotation",
                &self.rotation,
                "°",
                property_disabled,
            ));

        let accordion = Accordion::new(ids.accordion)
            .multiple(true)
            .bordered(false)
            .item(|item| item.open(section_state[0]).title("Details").child(details))
            .item(|item| {
                item.open(section_state[1])
                    .title("Appearance")
                    .child(appearance)
            })
            .item(|item| item.open(section_state[2]).title("Layout").child(layout))
            .on_toggle_click(move |open, _, cx| {
                let _ = section_control.update(cx, |inspector, cx| {
                    inspector.open_sections =
                        [open.contains(&0), open.contains(&1), open.contains(&2)];
                    cx.notify();
                });
            });

        v_flex()
            .id(ids.panel)
            .debug_selector(move || ids.panel.into())
            .w(px(RECTANGLE_INSPECTOR_WIDTH_PX))
            .h_full()
            .min_h_0()
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(inspector_header(ids))
            .child(
                div()
                    .id(ids.scroll)
                    .debug_selector(move || ids.scroll.into())
                    .h_full()
                    .min_h_0()
                    .p_3()
                    .overflow_y_scrollbar()
                    .id(ids.scroll)
                    .child(accordion),
            )
            .into_any_element()
    }
}

fn inspector_header(ids: &'static PropertyInspectorPresentation) -> impl IntoElement {
    div()
        .id(ids.header)
        .debug_selector(move || ids.header.into())
        .px_3()
        .py_2()
        .font_semibold()
        .child(ids.title)
}

fn number_field(
    id: &'static str,
    label: &'static str,
    state: &gpui::Entity<InputState>,
    suffix: &'static str,
    disabled: bool,
) -> AnyElement {
    Field::new()
        .label(label)
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .child(NumberInput::new(state).suffix(suffix).disabled(disabled)),
        )
        .into_any_element()
}

fn slider_field(
    id: &'static str,
    label: &'static str,
    state: &gpui::Entity<SliderState>,
    value: f64,
    disabled: bool,
) -> AnyElement {
    Field::new()
        .label(label)
        .child(
            h_flex()
                .id(id)
                .debug_selector(move || id.into())
                .gap_2()
                .child(Slider::new(state).disabled(disabled))
                .child(
                    div()
                        .w(px(44.))
                        .text_right()
                        .text_sm()
                        .child(format!("{value:.0}%")),
                ),
        )
        .into_any_element()
}

fn color_field(
    id: &'static str,
    label: &'static str,
    state: &gpui::Entity<ColorPickerState>,
    disabled: bool,
) -> AnyElement {
    Field::new()
        .label(label)
        .child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .when(disabled, |field| field.opacity(0.5))
                .child(ColorPicker::new(state).label(label)),
        )
        .into_any_element()
}

fn opaque_hex(color: gpui::Hsla) -> String {
    let hex = color.to_hex();
    hex.get(..7).unwrap_or(hex.as_str()).to_ascii_lowercase()
}

fn format_number(value: f64) -> String {
    if value.fract().abs() <= f64::EPSILON {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.6}");
        formatted.trim_end_matches('0').trim_end_matches('.').into()
    }
}

fn patch_matches_snapshot(
    patch: &RectanglePropertyPatch,
    snapshot: &RectanglePropertySnapshot,
) -> bool {
    match patch {
        RectanglePropertyPatch::Locked(value) => snapshot.locked == *value,
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
        assert_eq!(format_number(12.), "12");
        assert_eq!(format_number(12.5), "12.5");
        assert_eq!(format_number(0.000_001), "0.000001");
    }

    #[test]
    fn advertises_only_the_three_working_native_stroke_styles() {
        assert_eq!(STROKE_STYLE_SOLID, "Solid");
        assert_eq!(STROKE_STYLE_DASHED, "Dashed");
        assert_eq!(STROKE_STYLE_DOTTED, "Dotted");
    }
}
