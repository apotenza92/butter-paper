//! Retained presentation state for exactly one selected measurement markup.
//!
//! The inspector owns disclosure and component state only. The workspace owns
//! selection identity, page-scale authority, validation, history, and save.

use butter_paper_gpui_gallery::annotation_model::{
    AnnotationKind, MarkupId, PageScale, ScalePrecision, ScalePrecisionMode,
};
use gpui::{
    Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, StyledExt as _, button::Button, form::Field,
    scroll::ScrollableElement as _, switch::Switch, v_flex,
};

use crate::document_workspace::DocumentId;

pub const MEASUREMENT_PROPERTY_INSPECTOR_ID: &str = "measurement-property-inspector";
pub const MEASUREMENT_INSPECTOR_KIND_ID: &str = "measurement-property-kind";
pub const MEASUREMENT_INSPECTOR_CAPTION_ID: &str = "measurement-property-caption";
pub const MEASUREMENT_INSPECTOR_UNIT_ID: &str = "measurement-property-unit";
pub const MEASUREMENT_INSPECTOR_PRECISION_ID: &str = "measurement-property-precision";
pub const MEASUREMENT_INSPECTOR_SHOW_CAPTION_ID: &str = "measurement-property-show-caption";
pub const MEASUREMENT_INSPECTOR_SET_PAGE_SCALE_ID: &str = "measurement-property-set-page-scale";

#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementPropertySnapshot {
    pub document_id: DocumentId,
    pub annotation_id: MarkupId,
    pub annotation_kind: AnnotationKind,
    pub expected_revision: u64,
    pub page_index: u32,
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
    ShowCaption(bool),
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

pub struct MeasurementPropertyInspector {
    snapshot: Option<MeasurementPropertySnapshot>,
    open: bool,
}

impl MeasurementPropertyInspector {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            open: false,
        }
    }

    pub fn snapshot(&self) -> Option<&MeasurementPropertySnapshot> {
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
        if self.snapshot.take().is_some() || self.open {
            self.open = false;
            cx.notify();
        }
    }

    pub fn sync(&mut self, snapshot: MeasurementPropertySnapshot, cx: &mut Context<Self>) {
        if self.snapshot.as_ref() != Some(&snapshot) {
            self.snapshot = Some(snapshot);
            cx.notify();
        }
    }

    fn emit_action(&mut self, action: MeasurementPropertyAction, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        if snapshot.mutation_disabled
            || matches!(action, MeasurementPropertyAction::ShowCaption(_)) && snapshot.locked
            || matches!(action, MeasurementPropertyAction::ShowCaption(value) if value == snapshot.show_caption)
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
        let Some(snapshot) = self.snapshot.clone().filter(|_| self.open) else {
            return div()
                .id(MEASUREMENT_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let show_caption = cx.entity().downgrade();
        let page_scale = cx.entity().downgrade();
        let read_only = |id: &'static str, label: &'static str, value: String| {
            Field::new().label(label).child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .w_full()
                    .px_2()
                    .py_1()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(value),
            )
        };
        v_flex()
            .id(MEASUREMENT_PROPERTY_INSPECTOR_ID)
            .debug_selector(|| MEASUREMENT_PROPERTY_INSPECTOR_ID.into())
            .w(px(300.))
            .h_full()
            .min_h_0()
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(div().px_3().py_2().font_semibold().child("Measurement"))
            .child(
                v_flex()
                    .h_full()
                    .min_h_0()
                    .gap_3()
                    .p_3()
                    .overflow_y_scrollbar()
                    .child(
                        Button::new(MEASUREMENT_INSPECTOR_SET_PAGE_SCALE_ID)
                            .debug_selector(|| MEASUREMENT_INSPECTOR_SET_PAGE_SCALE_ID.into())
                            .label("Set Page Scale…")
                            .disabled(snapshot.mutation_disabled)
                            .on_click(move |_, _, cx| {
                                let _ = page_scale.update(cx, |inspector, cx| {
                                    inspector
                                        .emit_action(MeasurementPropertyAction::OpenPageScale, cx)
                                });
                            }),
                    )
                    .child(read_only(
                        MEASUREMENT_INSPECTOR_KIND_ID,
                        "Type",
                        measurement_kind_label(snapshot.annotation_kind).into(),
                    ))
                    .child(read_only(
                        MEASUREMENT_INSPECTOR_CAPTION_ID,
                        "Measured value",
                        snapshot.caption,
                    ))
                    .child(read_only(
                        MEASUREMENT_INSPECTOR_UNIT_ID,
                        "Unit",
                        snapshot.unit,
                    ))
                    .child(read_only(
                        MEASUREMENT_INSPECTOR_PRECISION_ID,
                        "Precision",
                        precision_label(snapshot.precision),
                    ))
                    .child(
                        Field::new().label("Caption").child(
                            div()
                                .id(MEASUREMENT_INSPECTOR_SHOW_CAPTION_ID)
                                .debug_selector(|| MEASUREMENT_INSPECTOR_SHOW_CAPTION_ID.into())
                                .child(
                                    Switch::new("measurement-property-show-caption-switch")
                                        .label("Show caption")
                                        .checked(snapshot.show_caption)
                                        .disabled(snapshot.mutation_disabled || snapshot.locked)
                                        .on_click(move |value, _, cx| {
                                            let _ = show_caption.update(cx, |inspector, cx| {
                                                inspector.emit_action(
                                                    MeasurementPropertyAction::ShowCaption(*value),
                                                    cx,
                                                )
                                            });
                                        }),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }
}

fn measurement_kind_label(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Length => "Length",
        AnnotationKind::Polylength => "Polylength",
        AnnotationKind::Area => "Area",
        _ => "Measurement",
    }
}

fn precision_label(precision: ScalePrecision) -> String {
    match precision.mode {
        ScalePrecisionMode::Decimal => format!("{}", precision.value),
        ScalePrecisionMode::Fraction => format!("1/{:.0}", precision.value),
    }
}
