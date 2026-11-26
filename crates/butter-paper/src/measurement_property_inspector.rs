//! Retained presentation state for the Measure section of one selected
//! Dimension, Polyline or Polygon, or of its measured form (a Length, a
//! Polylength or an Area).
//!
//! The section is the same for all six: its heading holds the Measure
//! checkbox, and its fields show disabled while the markup does not measure,
//! so turning Measure on or off adds and removes no row. The inspector owns
//! disclosure and component state only. The workspace owns selection
//! identity, page-scale authority, validation, history, and save.

use crate::annotation_model::{
    AnnotationKind, MEASUREMENT_LENGTH_UNITS, MarkupId, PageScale, ScalePrecision,
    ScalePrecisionMode, length_unit_metres,
};
use crate::property_controls::{
    PropertyGroup, PropertyInspectorPanel, PropertyOptionalSection, PropertyOptionalSectionIds,
    property_body, property_row,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IndexPath, Sizable as _,
    button::Button,
    h_flex,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};

use crate::document_workspace::DocumentId;

pub const MEASUREMENT_PROPERTY_INSPECTOR_ID: &str = "measurement-property-inspector";
pub const MEASUREMENT_INSPECTOR_KIND_ID: &str = "measurement-property-kind";
pub const MEASUREMENT_INSPECTOR_CAPTION_ID: &str = "measurement-property-caption";
pub const MEASUREMENT_INSPECTOR_UNIT_ID: &str = "measurement-property-unit";
pub const MEASUREMENT_INSPECTOR_PRECISION_ID: &str = "measurement-property-precision";
pub const MEASUREMENT_INSPECTOR_SHOW_CAPTION_ID: &str = "measurement-property-show-caption";
pub const MEASUREMENT_INSPECTOR_SET_PAGE_SCALE_ID: &str = "measurement-property-set-page-scale";
pub const MEASUREMENT_INSPECTOR_SECTION_ID: &str = "measurement-property-section";
pub const MEASUREMENT_INSPECTOR_MEASURE_ID: &str = "measurement-property-measure";
pub const MEASUREMENT_INSPECTOR_CHEVRON_ID: &str = "measurement-property-chevron";
pub const MEASUREMENT_INSPECTOR_BODY_ID: &str = "measurement-property-body";
const MEASUREMENT_INSPECTOR_HEADER_ID: &str = "measurement-property-inspector-header";
const MEASUREMENT_INSPECTOR_SCROLL_ID: &str = "measurement-property-inspector-scroll";

#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    /// A Dimension, Polyline or Polygon (not measuring) or the Length,
    /// Polylength or Area it becomes (measuring).
    pub annotation_kind: AnnotationKind,
    pub expected_revision: u64,
    pub page_index: u32,
    /// The measured value; empty while the markup does not measure.
    pub caption: String,
    pub page_scale: Option<PageScale>,
    pub unit: String,
    pub precision: ScalePrecision,
    pub show_caption: bool,
    pub locked: bool,
    pub mutation_disabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeasurementPropertyAction {
    /// Measuring on (a Dimension, Polyline or Polygon becomes a Length,
    /// Polylength or Area) or off.
    Measure(bool),
    ShowCaption(bool),
    /// Show the measurement in this unit (one of `MEASUREMENT_LENGTH_UNITS`).
    Unit(&'static str),
    OpenPageScale,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementPropertyEvent {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub annotation_kind: AnnotationKind,
    pub expected_revision: u64,
    pub page_index: u32,
    pub action: MeasurementPropertyAction,
}

impl MeasurementPropertySnapshot {
    /// Whether the markup measures: a Length, Polylength or Area.
    pub fn measuring(&self) -> bool {
        matches!(
            self.annotation_kind,
            AnnotationKind::Length | AnnotationKind::Polylength | AnnotationKind::Area
        )
    }
}

pub struct MeasurementPropertyInspector {
    snapshot: Option<MeasurementPropertySnapshot>,
    open: bool,
    embedded: bool,
    /// Whether the Measure section shows its fields. Checking Measure opens
    /// it; its chevron collapses or expands it either way.
    section_open: bool,
    unit_select: Entity<SelectState<Vec<SharedString>>>,
    _subscriptions: Vec<Subscription>,
}

/// A length unit's name as the unit menus show it.
pub fn length_unit_title(unit: &str) -> &'static str {
    match unit {
        "mm" => "Millimetres",
        "cm" => "Centimetres",
        "m" => "Metres",
        "km" => "Kilometres",
        "in" => "Inches",
        "ft" => "Feet",
        "yd" => "Yards",
        "mi" => "Miles",
        _ => "Other",
    }
}

impl MeasurementPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let unit_select = cx.new(|cx| {
            SelectState::new(
                MEASUREMENT_LENGTH_UNITS
                    .iter()
                    .map(|unit| SharedString::from(length_unit_title(unit)))
                    .collect::<Vec<_>>(),
                None,
                window,
                cx,
            )
        });
        let subscription = cx.subscribe_in(
            &unit_select,
            window,
            |this, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(Some(title)) = event else {
                    return;
                };
                if let Some(unit) = MEASUREMENT_LENGTH_UNITS
                    .iter()
                    .find(|unit| length_unit_title(unit) == title.as_ref())
                {
                    this.emit_action(MeasurementPropertyAction::Unit(unit), cx);
                }
            },
        );
        Self {
            snapshot: None,
            open: false,
            embedded: false,
            section_open: true,
            unit_select,
            _subscriptions: vec![subscription],
        }
    }

    pub fn snapshot(&self) -> Option<&MeasurementPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn set_embedded(&mut self) {
        self.embedded = true;
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the Measure section shows its fields.
    pub const fn section_open(&self) -> bool {
        self.section_open
    }

    pub fn set_section_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.section_open != open {
            self.section_open = open;
            cx.notify();
        }
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
        snapshot: MeasurementPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot.as_ref() != Some(&snapshot) {
            let index = MEASUREMENT_LENGTH_UNITS
                .iter()
                .position(|unit| *unit == snapshot.unit);
            self.unit_select.update(cx, |select, cx| {
                select.set_selected_index(index.map(IndexPath::new), window, cx);
            });
            self.snapshot = Some(snapshot);
            cx.notify();
        }
    }

    fn emit_action(&mut self, action: MeasurementPropertyAction, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if !(self.open || self.embedded)
            || snapshot.mutation_disabled
            || matches!(
                action,
                MeasurementPropertyAction::Measure(_)
                    | MeasurementPropertyAction::ShowCaption(_)
                    | MeasurementPropertyAction::Unit(_)
            ) && snapshot.locked
            || matches!(action, MeasurementPropertyAction::Measure(value) if value == snapshot.measuring())
            || matches!(
                action,
                MeasurementPropertyAction::ShowCaption(_) | MeasurementPropertyAction::Unit(_)
            ) && !snapshot.measuring()
            || matches!(action, MeasurementPropertyAction::ShowCaption(value) if value == snapshot.show_caption)
            || matches!(action, MeasurementPropertyAction::Unit(unit) if unit == snapshot.unit)
        {
            return;
        }
        cx.emit(MeasurementPropertyEvent {
            document_id: snapshot.document_id,
            annotation_id: snapshot.annotation_id.clone(),
            annotation_kind: snapshot.annotation_kind,
            expected_revision: snapshot.expected_revision,
            page_index: snapshot.page_index,
            action,
        });
    }
}

impl EventEmitter<MeasurementPropertyEvent> for MeasurementPropertyInspector {}

impl Render for MeasurementPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open || self.embedded) else {
            return div()
                .id(MEASUREMENT_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let measuring = snapshot.measuring();
        let weak = cx.entity().downgrade();
        let show_caption = weak.clone();
        let page_scale = weak.clone();
        let measure_checked = weak.clone();
        let section_toggled = weak.clone();
        let muted = cx.theme().muted_foreground;
        let unit_select = self.unit_select.clone();
        // Whatever stops the markup changing (a lock, a save) stops the
        // checkbox too.
        let frozen = snapshot.mutation_disabled || snapshot.locked;
        let section = PropertyOptionalSection::new(
            PropertyOptionalSectionIds {
                root: MEASUREMENT_INSPECTOR_SECTION_ID,
                checkbox: MEASUREMENT_INSPECTOR_MEASURE_ID,
                chevron: MEASUREMENT_INSPECTOR_CHEVRON_ID,
                body: MEASUREMENT_INSPECTOR_BODY_ID,
            },
            "Measure",
            measuring,
            self.section_open,
        )
        .disabled(frozen)
        .tooltip(measure_tooltip(snapshot.annotation_kind))
        .on_checked_change(move |checked, _, cx| {
            let _ = measure_checked.update(cx, |inspector, cx| {
                if checked {
                    inspector.section_open = true;
                }
                inspector.emit_action(MeasurementPropertyAction::Measure(checked), cx);
                cx.notify();
            });
        })
        .on_open_change(move |open, _, cx| {
            let _ =
                section_toggled.update(cx, |inspector, cx| inspector.set_section_open(open, cx));
        })
        .body(move |fields_disabled| {
            // While not measuring, the value is a dash and its name is what
            // the markup becomes, in the quiet colour a disabled field has.
            let read_only = |id: &'static str, value: String| {
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .when(fields_disabled, |value| value.text_color(muted))
                    .child(value)
            };
            let unit_known = length_unit_metres(&snapshot.unit).is_some();
            let value = if snapshot.caption.is_empty() {
                "—".to_owned()
            } else {
                snapshot.caption.clone()
            };
            PropertyGroup::new()
                .row(property_row(
                    "Scale",
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .when(fields_disabled, |scale| scale.text_color(muted))
                                .child(snapshot.page_scale.as_ref().map_or_else(
                                    || "None (paper size)".to_owned(),
                                    crate::page_scale_catalog::scale_label,
                                )),
                        )
                        .child(
                            Button::new(MEASUREMENT_INSPECTOR_SET_PAGE_SCALE_ID)
                                .debug_selector(|| MEASUREMENT_INSPECTOR_SET_PAGE_SCALE_ID.into())
                                .label("Change")
                                .small()
                                .outline()
                                .disabled(snapshot.mutation_disabled || !measuring)
                                .on_click(move |_, _, cx| {
                                    let _ = page_scale.update(cx, |inspector, cx| {
                                        inspector.emit_action(
                                            MeasurementPropertyAction::OpenPageScale,
                                            cx,
                                        )
                                    });
                                }),
                        ),
                ))
                .row(property_row(
                    "Value",
                    h_flex()
                        .gap_1()
                        .child(read_only(MEASUREMENT_INSPECTOR_CAPTION_ID, value))
                        .child(
                            div()
                                .id(MEASUREMENT_INSPECTOR_KIND_ID)
                                .debug_selector(|| MEASUREMENT_INSPECTOR_KIND_ID.into())
                                .text_sm()
                                .text_color(muted)
                                .child(measurement_kind_label(snapshot.annotation_kind)),
                        ),
                ))
                .row(property_row(
                    "Unit",
                    div()
                        .id(MEASUREMENT_INSPECTOR_UNIT_ID)
                        .debug_selector(|| MEASUREMENT_INSPECTOR_UNIT_ID.into())
                        .w_full()
                        .when(!unit_known, |unit| {
                            // A unit from another app that cannot be converted.
                            unit.text_sm()
                                .when(fields_disabled, |unit| unit.text_color(muted))
                                .child(snapshot.unit.clone())
                        })
                        .when(unit_known, |unit| {
                            unit.child(Select::new(&unit_select).small().disabled(fields_disabled))
                        }),
                ))
                .row(property_row(
                    "Precision",
                    read_only(
                        MEASUREMENT_INSPECTOR_PRECISION_ID,
                        precision_label(snapshot.precision),
                    ),
                ))
                .row(property_row(
                    "Caption",
                    div()
                        .id(MEASUREMENT_INSPECTOR_SHOW_CAPTION_ID)
                        .debug_selector(|| MEASUREMENT_INSPECTOR_SHOW_CAPTION_ID.into())
                        .child(
                            Switch::new("measurement-property-show-caption-switch")
                                .checked(snapshot.show_caption)
                                .disabled(fields_disabled)
                                .tooltip("Show the measured value on the page")
                                .on_click(move |value, _, cx| {
                                    let _ = show_caption.update(cx, |inspector, cx| {
                                        inspector.emit_action(
                                            MeasurementPropertyAction::ShowCaption(*value),
                                            cx,
                                        )
                                    });
                                }),
                        ),
                ))
                .into_any_element()
        });
        PropertyInspectorPanel::new(
            MEASUREMENT_PROPERTY_INSPECTOR_ID,
            MEASUREMENT_INSPECTOR_HEADER_ID,
            MEASUREMENT_INSPECTOR_SCROLL_ID,
            "Measure",
        )
        .content_only(self.embedded)
        .child(property_body().child(section))
        .into_any_element()
    }
}

/// What the Measure checkbox's tooltip says it does for this markup.
fn measure_tooltip(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Dimension => "Measure the line with the page scale (Length)",
        AnnotationKind::Length => "Off, the line keeps its measured text, which you can then edit",
        AnnotationKind::Polyline => "Measure its total length with the page scale (Polylength)",
        AnnotationKind::Polygon => "Measure its area with the page scale (Area)",
        AnnotationKind::Polylength => "Off, it becomes a Polyline again",
        _ => "Off, it becomes a Polygon again",
    }
}

/// What the markup measures as: the name of its measured form.
fn measurement_kind_label(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Dimension | AnnotationKind::Length => "Length",
        AnnotationKind::Polyline | AnnotationKind::Polylength => "Polylength",
        AnnotationKind::Polygon | AnnotationKind::Area => "Area",
        _ => "Measurement",
    }
}

fn precision_label(precision: ScalePrecision) -> String {
    match precision.mode {
        ScalePrecisionMode::Decimal => format!("{}", precision.value),
        ScalePrecisionMode::Fraction => format!("1/{:.0}", precision.value),
    }
}
