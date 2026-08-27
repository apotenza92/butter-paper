use butter_paper_gpui_gallery::annotation_model::{
    PageScale, PageScaleApplyTarget, PdfPoint, ScalePrecision, ScalePrecisionMode, ScalePreset,
    ScaleSource, ScaleUnit, built_in_scale_presets, parse_page_scale_ranges,
};
use gpui::{
    App, AppContext as _, Context, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, IndexPath, Selectable as _, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    input::{Input, InputState, NumberInput},
    select::{Select, SelectEvent, SelectState},
    v_flex,
};

use crate::document_workspace::{DocumentId, DocumentWorkspace};

pub const PAGE_SCALE_TRIGGER_ID: &str = "measure-set-page-scale";
pub const PAGE_SCALE_DIALOG_ID: &str = "page-scale-dialog";
pub const PAGE_SCALE_DIALOG_BODY_ID: &str = "page-scale-dialog-body";
pub const PAGE_SCALE_CLOSE_ID: &str = "page-scale-close";
pub const PAGE_SCALE_METHOD_PRESET_ID: &str = "page-scale-method-preset";
pub const PAGE_SCALE_METHOD_CUSTOM_ID: &str = "page-scale-method-custom";
pub const PAGE_SCALE_METHOD_CALIBRATE_ID: &str = "page-scale-method-calibrate";
pub const PAGE_SCALE_PRESET_SELECT_ID: &str = "page-scale-preset-select";
pub const PAGE_SCALE_DELETE_PRESET_ID: &str = "page-scale-delete-preset";
pub const PAGE_SCALE_CUSTOM_PDF_LENGTH_ID: &str = "page-scale-custom-pdf-length";
pub const PAGE_SCALE_CUSTOM_PDF_UNITS_ID: &str = "page-scale-custom-pdf-units";
pub const PAGE_SCALE_CUSTOM_REAL_LENGTH_ID: &str = "page-scale-custom-real-length";
pub const PAGE_SCALE_CUSTOM_REAL_UNITS_ID: &str = "page-scale-custom-real-units";
pub const PAGE_SCALE_SEPARATE_Y_ID: &str = "page-scale-separate-y";
pub const PAGE_SCALE_Y_PDF_LENGTH_ID: &str = "page-scale-y-custom-pdf-length";
pub const PAGE_SCALE_Y_REAL_LENGTH_ID: &str = "page-scale-y-custom-real-length";
pub const PAGE_SCALE_PAGES_ID: &str = "page-scale-pages";
pub const PAGE_SCALE_RANGE_ID: &str = "page-scale-range";
pub const PAGE_SCALE_PRECISION_MODE_ID: &str = "page-scale-precision-mode";
pub const PAGE_SCALE_PRECISION_VALUE_ID: &str = "page-scale-precision-value";
pub const PAGE_SCALE_SAVE_PRESET_ID: &str = "page-scale-save-preset";
pub const PAGE_SCALE_PICK_ID: &str = "page-scale-pick-calibration";
pub const PAGE_SCALE_KNOWN_LENGTH_ID: &str = "page-scale-calibrate-real-length";
pub const PAGE_SCALE_UNIT_GROUP_ID: &str = "page-scale-calibrate-real-units";
pub const PAGE_SCALE_UNIT_M_ID: &str = "page-scale-unit-m";
pub const PAGE_SCALE_UNIT_CM_ID: &str = "page-scale-unit-cm";
pub const PAGE_SCALE_UNIT_MM_ID: &str = "page-scale-unit-mm";
pub const PAGE_SCALE_CANCEL_ID: &str = "page-scale-cancel";
pub const PAGE_SCALE_APPLY_ID: &str = "page-scale-apply";
pub const PAGE_SCALE_ERROR_ID: &str = "page-scale-error";
pub const PAGE_SCALE_PICK_STATUS_ID: &str = "page-scale-pick-status";
pub const PAGE_SCALE_PICK_ALERT_ID: &str = "page-scale-pick-alert";
pub const PAGE_SCALE_PICK_CANCEL_ID: &str = "page-scale-pick-cancel";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationPointDisposition {
    Ignored,
    FirstPoint,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageScaleMode {
    Preset,
    Custom,
    Calibrate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageScalePagesMode {
    Current,
    All,
    Custom,
}

/// Application-owned transient state for the calibration journey. GPUI
/// Component owns only the dialog, buttons, and numeric-input presentation.
pub struct PageScaleControl {
    owner: WeakEntity<DocumentWorkspace>,
    target: Option<(DocumentId, u32)>,
    pick_document_id: Option<DocumentId>,
    start: Option<PdfPoint>,
    end: Option<PdfPoint>,
    picking: bool,
    mode: PageScaleMode,
    pages_mode: PageScalePagesMode,
    pdf_units: ScaleUnit,
    real_units: ScaleUnit,
    separate_y: bool,
    precision_mode: ScalePrecisionMode,
    precision_value: f64,
    save_preset: bool,
    selected_preset_id: String,
    next_preset_sequence: u64,
    pdf_length: gpui::Entity<InputState>,
    known_length: gpui::Entity<InputState>,
    y_pdf_length: gpui::Entity<InputState>,
    y_real_length: gpui::Entity<InputState>,
    custom_range: gpui::Entity<InputState>,
    preset_select: gpui::Entity<SelectState<Vec<SharedString>>>,
    pages_select: gpui::Entity<SelectState<Vec<SharedString>>>,
    pdf_units_select: gpui::Entity<SelectState<Vec<SharedString>>>,
    real_units_select: gpui::Entity<SelectState<Vec<SharedString>>>,
    precision_mode_select: gpui::Entity<SelectState<Vec<SharedString>>>,
    precision_value_select: gpui::Entity<SelectState<Vec<SharedString>>>,
    error: Option<String>,
    applied_count: usize,
}

impl PageScaleControl {
    pub fn new(
        owner: WeakEntity<DocumentWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let pdf_length = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.000_001)
        });
        let known_length = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.000_001)
                .step(1.)
        });
        let y_pdf_length = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.000_001)
        });
        let y_real_length = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1")
                .min(0.000_001)
        });
        let custom_range = cx.new(|cx| InputState::new(window, cx).placeholder("1-3, 5, 9"));
        let preset_select = cx.new(|cx| {
            SelectState::new(
                built_in_scale_presets()
                    .into_iter()
                    .map(|preset| SharedString::from(preset.name))
                    .collect::<Vec<_>>(),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let pages_select = cx.new(|cx| {
            SelectState::new(
                vec!["Current (1)".into(), "All Pages".into(), "Custom".into()],
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let unit_items = || -> Vec<SharedString> {
            vec![
                "in".into(),
                "ft".into(),
                "mm".into(),
                "cm".into(),
                "m".into(),
            ]
        };
        let pdf_units_select = cx.new(|cx| {
            SelectState::new(unit_items(), Some(IndexPath::default().row(3)), window, cx)
        });
        let real_units_select = cx.new(|cx| {
            SelectState::new(unit_items(), Some(IndexPath::default().row(4)), window, cx)
        });
        let precision_mode_select = cx.new(|cx| {
            SelectState::new(
                vec!["Decimal".into(), "Fraction".into()],
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let precision_value_select = cx.new(|cx| {
            SelectState::new(
                decimal_precision_labels(),
                Some(IndexPath::default().row(3)),
                window,
                cx,
            )
        });

        cx.subscribe_in(&pages_select, window, |control, _, event, _, cx| {
            let SelectEvent::Confirm(value) = event;
            control.pages_mode = match value.as_deref() {
                Some("All Pages") => PageScalePagesMode::All,
                Some("Custom") => PageScalePagesMode::Custom,
                _ => PageScalePagesMode::Current,
            };
            control.error = None;
            cx.notify();
        })
        .detach();
        cx.subscribe_in(&pdf_units_select, window, |control, _, event, _, cx| {
            let SelectEvent::Confirm(value) = event;
            if let Some(value) = value {
                if let Ok(unit) = ScaleUnit::parse(value.as_ref()) {
                    control.pdf_units = unit;
                }
            }
            control.error = None;
            cx.notify();
        })
        .detach();
        cx.subscribe_in(&real_units_select, window, |control, _, event, _, cx| {
            let SelectEvent::Confirm(value) = event;
            if let Some(value) = value {
                if let Ok(unit) = ScaleUnit::parse(value.as_ref()) {
                    control.real_units = unit;
                }
            }
            control.error = None;
            cx.notify();
        })
        .detach();
        cx.subscribe_in(
            &precision_mode_select,
            window,
            |control, _, event, window, cx| {
                let SelectEvent::Confirm(value) = event;
                control.precision_mode = if value.as_deref() == Some("Fraction") {
                    ScalePrecisionMode::Fraction
                } else {
                    ScalePrecisionMode::Decimal
                };
                control.precision_value = if control.precision_mode == ScalePrecisionMode::Fraction
                {
                    16.
                } else {
                    0.001
                };
                let labels = if control.precision_mode == ScalePrecisionMode::Fraction {
                    fraction_precision_labels()
                } else {
                    decimal_precision_labels()
                };
                control.precision_value_select.update(cx, |select, cx| {
                    select.set_items(labels, window, cx);
                    select.set_selected_index(Some(IndexPath::default().row(3)), window, cx);
                });
                control.error = None;
                cx.notify();
            },
        )
        .detach();
        cx.subscribe_in(
            &precision_value_select,
            window,
            |control, _, event, _, cx| {
                let SelectEvent::Confirm(value) = event;
                if let Some(value) = value {
                    let raw = value.as_ref().strip_prefix("1/").unwrap_or(value.as_ref());
                    if let Ok(parsed) = raw.parse::<f64>() {
                        control.precision_value = parsed;
                    }
                }
                control.error = None;
                cx.notify();
            },
        )
        .detach();
        cx.subscribe_in(&preset_select, window, |control, _, event, _, cx| {
            let SelectEvent::Confirm(value) = event;
            if let Some(label) = value {
                if let Some(document_id) = control.target.map(|target| target.0) {
                    if let Some(preset) = control
                        .available_presets(document_id, cx)
                        .into_iter()
                        .find(|preset| preset_label(preset) == label.as_ref())
                    {
                        control.selected_preset_id = preset.id;
                    }
                }
            }
            control.error = None;
            cx.notify();
        })
        .detach();
        Self {
            owner,
            target: None,
            pick_document_id: None,
            start: None,
            end: None,
            picking: false,
            mode: PageScaleMode::Preset,
            pages_mode: PageScalePagesMode::Current,
            pdf_units: ScaleUnit::Cm,
            real_units: ScaleUnit::M,
            separate_y: false,
            precision_mode: ScalePrecisionMode::Decimal,
            precision_value: 0.001,
            save_preset: false,
            selected_preset_id: "one-to-1".into(),
            next_preset_sequence: 1,
            pdf_length,
            known_length,
            y_pdf_length,
            y_real_length,
            custom_range,
            preset_select,
            pages_select,
            pdf_units_select,
            real_units_select,
            precision_mode_select,
            precision_value_select,
            error: None,
            applied_count: 0,
        }
    }

    pub fn known_length_input(&self) -> gpui::Entity<InputState> {
        self.known_length.clone()
    }

    pub fn pdf_length_input(&self) -> gpui::Entity<InputState> {
        self.pdf_length.clone()
    }

    pub fn y_pdf_length_input(&self) -> gpui::Entity<InputState> {
        self.y_pdf_length.clone()
    }

    pub fn y_real_length_input(&self) -> gpui::Entity<InputState> {
        self.y_real_length.clone()
    }

    pub fn custom_range_input(&self) -> gpui::Entity<InputState> {
        self.custom_range.clone()
    }

    pub fn mode(&self) -> PageScaleMode {
        self.mode
    }

    pub fn pages_mode(&self) -> PageScalePagesMode {
        self.pages_mode
    }

    /// Deterministic mirror of the conditional stable-ID branches rendered by
    /// the real GPUI Component dialog. GPUI's modal test host does not expose
    /// nested dialog children through `debug_bounds`, so this seam keeps the
    /// conditional render contract testable without claiming native pixels.
    pub fn visible_stable_ids(&self) -> Vec<&'static str> {
        let mut ids = vec![
            PAGE_SCALE_DIALOG_ID,
            PAGE_SCALE_DIALOG_BODY_ID,
            PAGE_SCALE_CLOSE_ID,
            PAGE_SCALE_METHOD_PRESET_ID,
            PAGE_SCALE_METHOD_CUSTOM_ID,
            PAGE_SCALE_METHOD_CALIBRATE_ID,
        ];
        match self.mode {
            PageScaleMode::Preset => ids.push(PAGE_SCALE_PRESET_SELECT_ID),
            PageScaleMode::Custom => {
                ids.extend([
                    PAGE_SCALE_CUSTOM_PDF_LENGTH_ID,
                    PAGE_SCALE_CUSTOM_PDF_UNITS_ID,
                    PAGE_SCALE_CUSTOM_REAL_LENGTH_ID,
                    PAGE_SCALE_CUSTOM_REAL_UNITS_ID,
                    PAGE_SCALE_SEPARATE_Y_ID,
                ]);
                if self.separate_y {
                    ids.extend([PAGE_SCALE_Y_PDF_LENGTH_ID, PAGE_SCALE_Y_REAL_LENGTH_ID]);
                }
            }
            PageScaleMode::Calibrate => ids.extend([
                PAGE_SCALE_PICK_STATUS_ID,
                PAGE_SCALE_PICK_ID,
                PAGE_SCALE_KNOWN_LENGTH_ID,
                PAGE_SCALE_UNIT_GROUP_ID,
            ]),
        }
        ids.push(PAGE_SCALE_PAGES_ID);
        if self.pages_mode == PageScalePagesMode::Custom {
            ids.push(PAGE_SCALE_RANGE_ID);
        }
        ids.extend([PAGE_SCALE_PRECISION_MODE_ID, PAGE_SCALE_PRECISION_VALUE_ID]);
        if self.mode != PageScaleMode::Preset {
            ids.push(PAGE_SCALE_SAVE_PRESET_ID);
        }
        ids.extend([PAGE_SCALE_CANCEL_ID, PAGE_SCALE_APPLY_ID]);
        ids
    }

    pub fn is_picking(&self) -> bool {
        self.picking
    }

    pub fn target(&self) -> Option<(DocumentId, u32)> {
        self.target
    }

    pub fn points(&self) -> (Option<PdfPoint>, Option<PdfPoint>) {
        (self.start, self.end)
    }

    pub fn applied_count(&self) -> usize {
        self.applied_count
    }

    pub fn pick_instruction(&self) -> Option<&'static str> {
        self.picking.then_some(if self.start.is_some() {
            "Click the second point of the known distance. Hold Shift to constrain the line."
        } else {
            "Click the first point of a known distance."
        })
    }

    pub fn open_for(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.target = Some((document_id, page_index));
        self.pick_document_id = Some(document_id);
        self.start = None;
        self.end = None;
        self.picking = false;
        self.mode = PageScaleMode::Preset;
        self.pages_mode = PageScalePagesMode::Current;
        self.pdf_units = ScaleUnit::Cm;
        self.real_units = ScaleUnit::M;
        self.separate_y = false;
        self.precision_mode = ScalePrecisionMode::Decimal;
        self.precision_value = 0.001;
        self.save_preset = false;
        self.selected_preset_id = "one-to-1".into();
        self.error = None;
        self.pdf_length
            .update(cx, |input, cx| input.set_value("1", window, cx));
        self.known_length
            .update(cx, |input, cx| input.set_value("1", window, cx));
        self.y_pdf_length
            .update(cx, |input, cx| input.set_value("1", window, cx));
        self.y_real_length
            .update(cx, |input, cx| input.set_value("1", window, cx));
        self.custom_range
            .update(cx, |input, cx| input.set_value("", window, cx));
        let presets = self.available_presets(document_id, cx);
        self.selected_preset_id = presets
            .first()
            .map(|preset| preset.id.clone())
            .unwrap_or_else(|| "one-to-1".into());
        let preset_labels = presets
            .iter()
            .map(preset_label)
            .map(SharedString::from)
            .collect::<Vec<_>>();
        self.preset_select.update(cx, |select, cx| {
            select.set_items(preset_labels, window, cx);
            select.set_selected_index(Some(IndexPath::default()), window, cx);
        });
        self.pages_select.update(cx, |select, cx| {
            select.set_items(
                vec![
                    format!("Current ({})", page_index + 1).into(),
                    "All Pages".into(),
                    "Custom".into(),
                ],
                window,
                cx,
            );
            select.set_selected_index(Some(IndexPath::default()), window, cx);
        });
        self.open_dialog(window, cx);
    }

    pub fn cancel_pick(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.picking {
            return false;
        }
        self.picking = false;
        self.pick_document_id = None;
        self.start = None;
        self.end = None;
        cx.notify();
        true
    }

    pub fn record_point(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        constrain_orthogonal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CalibrationPointDisposition {
        if !self.picking {
            return CalibrationPointDisposition::Ignored;
        }
        if self.pick_document_id != Some(document_id) {
            return CalibrationPointDisposition::Ignored;
        }
        if self.start.is_some() && self.target.map(|target| target.1) != Some(page_index) {
            self.error = Some("Pick both calibration points on the same page.".into());
            cx.notify();
            return CalibrationPointDisposition::Ignored;
        }
        if self.start.is_none() {
            self.target = Some((document_id, page_index));
            self.start = Some(point);
            cx.notify();
            return CalibrationPointDisposition::FirstPoint;
        }
        let start = self.start.expect("the first calibration point is retained");
        let end = if constrain_orthogonal {
            constrain_point(start, point)
        } else {
            point
        };
        self.end = Some(end);
        self.picking = false;
        self.pick_document_id = None;
        self.error = None;
        self.open_dialog(window, cx);
        cx.notify();
        CalibrationPointDisposition::Completed
    }

    pub fn begin_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PageScaleMode::Calibrate;
        self.pick_document_id = self.target.map(|target| target.0);
        self.target = None;
        self.start = None;
        self.end = None;
        self.picking = true;
        self.error = None;
        window.close_dialog(cx);
        cx.notify();
    }

    fn select_mode(&mut self, mode: PageScaleMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.error = None;
        cx.notify();
    }

    pub fn configure_custom_for_test(
        &mut self,
        pdf_units: ScaleUnit,
        real_units: ScaleUnit,
        separate_y: bool,
        precision: ScalePrecision,
        pages_mode: PageScalePagesMode,
        save_preset: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = PageScaleMode::Custom;
        self.pdf_units = pdf_units;
        self.real_units = real_units;
        self.separate_y = separate_y;
        self.precision_mode = precision.mode;
        self.precision_value = precision.value;
        self.pages_mode = pages_mode;
        self.save_preset = save_preset;
        self.sync_selects(window, cx);
        self.error = None;
        cx.notify();
    }

    pub fn apply(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((document_id, page_index)) = self.target else {
            self.error = Some("Choose a document page before applying scale.".into());
            cx.notify();
            return false;
        };
        let page_count = self
            .owner
            .read_with(cx, |workspace, cx| workspace.page_count(document_id, cx))
            .ok()
            .flatten()
            .unwrap_or(0);
        let scale = match self.build_scale(document_id, page_index, cx) {
            Ok(scale) => scale,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return false;
            }
        };
        let target = match self.build_target(page_index, page_count, cx) {
            Ok(target) => target,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return false;
            }
        };
        let saved_preset = self.save_preset.then(|| {
            let id = format!("scale-native-{}", self.next_preset_sequence);
            ScalePreset {
                id,
                name: scale.name.clone(),
                pdf_units: scale.pdf_units,
                real_units: scale.real_units,
                scale_x: scale.scale_x,
                scale_y: scale.scale_y,
                source: scale.source,
                built_in: false,
            }
        });
        let result = self.owner.update(cx, |workspace, cx| {
            workspace.apply_page_scale_with_preset(document_id, scale, target, saved_preset, cx)
        });
        match result {
            Ok(Ok(_)) => {
                self.error = None;
                self.applied_count = self.applied_count.saturating_add(1);
                if self.save_preset {
                    self.next_preset_sequence = self.next_preset_sequence.saturating_add(1);
                }
                cx.notify();
                true
            }
            Ok(Err(error)) => {
                self.error = Some(error);
                cx.notify();
                false
            }
            Err(_) => {
                self.error = Some("The document workspace is no longer available.".into());
                cx.notify();
                false
            }
        }
    }

    fn available_presets(&self, document_id: DocumentId, cx: &App) -> Vec<ScalePreset> {
        let mut presets = self
            .owner
            .read_with(cx, |workspace, cx| workspace.scale_presets(document_id, cx))
            .unwrap_or_default();
        presets.extend(built_in_scale_presets());
        presets
    }

    fn selected_user_preset_id(&self, cx: &App) -> Option<String> {
        let document_id = self.target?.0;
        self.available_presets(document_id, cx)
            .into_iter()
            .find(|preset| preset.id == self.selected_preset_id && !preset.built_in)
            .map(|preset| preset.id)
    }

    fn delete_selected_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((document_id, _)) = self.target else {
            return false;
        };
        let Some(preset_id) = self.selected_user_preset_id(cx) else {
            return false;
        };
        let result = self.owner.update(cx, |workspace, cx| {
            workspace.delete_scale_preset(document_id, &preset_id, cx)
        });
        match result {
            Ok(Ok(true)) => {
                self.selected_preset_id = "one-to-1".into();
                let labels = self
                    .available_presets(document_id, cx)
                    .iter()
                    .map(preset_label)
                    .map(SharedString::from)
                    .collect::<Vec<_>>();
                self.preset_select.update(cx, |select, cx| {
                    select.set_items(labels, window, cx);
                    let selected: SharedString = "1:1".into();
                    select.set_selected_value(&selected, window, cx);
                });
                self.error = None;
                cx.notify();
                true
            }
            Ok(Ok(false)) => false,
            Ok(Err(error)) => {
                self.error = Some(error);
                cx.notify();
                false
            }
            Err(_) => {
                self.error = Some("The document workspace is no longer available.".into());
                cx.notify();
                false
            }
        }
    }

    fn build_scale(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Result<PageScale, String> {
        let precision = match self.precision_mode {
            ScalePrecisionMode::Decimal => ScalePrecision::decimal(self.precision_value),
            ScalePrecisionMode::Fraction => {
                ScalePrecision::fraction(self.precision_value.round() as u16)
            }
        }
        .map_err(|error| error.to_string())?;
        match self.mode {
            PageScaleMode::Preset => {
                let presets = self.available_presets(document_id, cx);
                let preset = presets
                    .iter()
                    .find(|preset| {
                        preset.id == self.selected_preset_id
                            || preset.name == self.selected_preset_id
                    })
                    .or_else(|| presets.first())
                    .ok_or_else(|| "Select a scale preset.".to_owned())?;
                PageScale::from_factors(
                    page_index,
                    ScaleSource::Preset,
                    preset.name.clone(),
                    preset.pdf_units,
                    preset.real_units,
                    preset.scale_x,
                    preset.scale_y,
                    precision,
                )
                .map_err(|error| error.to_string())
            }
            PageScaleMode::Custom => {
                let pdf_length =
                    read_positive(self.pdf_length.read(cx).value().as_ref(), "PDF length")?;
                let real_length =
                    read_positive(self.known_length.read(cx).value().as_ref(), "Real length")?;
                let y_lengths = if self.separate_y {
                    Some((
                        read_positive(self.y_pdf_length.read(cx).value().as_ref(), "Y PDF length")?,
                        read_positive(
                            self.y_real_length.read(cx).value().as_ref(),
                            "Y real length",
                        )?,
                    ))
                } else {
                    None
                };
                PageScale::custom(
                    page_index,
                    format!(
                        "{} {} = {} {}",
                        format_scale_number(pdf_length),
                        self.pdf_units.as_str(),
                        format_scale_number(real_length),
                        self.real_units.as_str()
                    ),
                    self.pdf_units,
                    self.real_units,
                    pdf_length,
                    real_length,
                    y_lengths,
                    precision,
                )
                .map_err(|error| error.to_string())
            }
            PageScaleMode::Calibrate => {
                let (Some(start), Some(end)) = (self.start, self.end) else {
                    return Err("Pick two points on the current PDF page.".into());
                };
                let known_length =
                    read_positive(self.known_length.read(cx).value().as_ref(), "Real length")?;
                PageScale::calibrated(
                    page_index,
                    start,
                    end,
                    known_length,
                    self.real_units,
                    precision,
                )
                .map_err(|error| error.to_string())
            }
        }
    }

    fn build_target(
        &self,
        page_index: u32,
        page_count: u32,
        cx: &App,
    ) -> Result<PageScaleApplyTarget, String> {
        match self.pages_mode {
            PageScalePagesMode::Current => Ok(PageScaleApplyTarget::Current(page_index)),
            PageScalePagesMode::All => Ok(PageScaleApplyTarget::All),
            PageScalePagesMode::Custom => {
                parse_page_scale_ranges(self.custom_range.read(cx).value().as_ref(), page_count)
                    .map(PageScaleApplyTarget::Ranges)
                    .map_err(|error| error.to_string())
            }
        }
    }

    fn sync_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pdf_units = SharedString::from(self.pdf_units.as_str());
        self.pdf_units_select.update(cx, |select, cx| {
            select.set_selected_value(&pdf_units, window, cx)
        });
        let real_units = SharedString::from(self.real_units.as_str());
        self.real_units_select.update(cx, |select, cx| {
            select.set_selected_value(&real_units, window, cx)
        });
        let precision_mode: SharedString = match self.precision_mode {
            ScalePrecisionMode::Decimal => "Decimal".into(),
            ScalePrecisionMode::Fraction => "Fraction".into(),
        };
        self.precision_mode_select.update(cx, |select, cx| {
            select.set_selected_value(&precision_mode, window, cx)
        });
        let precision_labels = if self.precision_mode == ScalePrecisionMode::Fraction {
            fraction_precision_labels()
        } else {
            decimal_precision_labels()
        };
        let precision_value = precision_label(self.precision_mode, self.precision_value);
        self.precision_value_select.update(cx, |select, cx| {
            select.set_items(precision_labels, window, cx);
            select.set_selected_value(&precision_value, window, cx);
        });
        let pages_label: SharedString = match self.pages_mode {
            PageScalePagesMode::Current => self
                .target
                .map(|(_, page_index)| format!("Current ({})", page_index + 1).into())
                .unwrap_or_else(|| "Current (1)".into()),
            PageScalePagesMode::All => "All Pages".into(),
            PageScalePagesMode::Custom => "Custom".into(),
        };
        self.pages_select.update(cx, |select, cx| {
            select.set_selected_value(&pages_label, window, cx)
        });
    }

    pub fn cancel_for_document(&mut self, document_id: DocumentId, cx: &mut Context<Self>) -> bool {
        let owns_document = self.target.is_some_and(|target| target.0 == document_id)
            || self.pick_document_id == Some(document_id);
        if !owns_document {
            return false;
        }
        self.target = None;
        self.pick_document_id = None;
        self.start = None;
        self.end = None;
        self.picking = false;
        self.error = None;
        cx.notify();
        true
    }

    fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let control = cx.weak_entity();
        let action_control = control.clone();
        let content_control = control.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .close_button(false)
                .overlay_closable(true)
                .w(px(520.))
                .max_w_full()
                .on_ok({
                    let action_control = action_control.clone();
                    move |_, _, cx| {
                        action_control
                            .update(cx, |control, cx| control.apply(cx))
                            .unwrap_or(false)
                    }
                })
                .content({
                    let content_control = content_control.clone();
                    move |content, window, cx| {
                        build_dialog_content(content, &content_control, window, cx)
                    }
                })
        });
    }
}

fn build_dialog_content(
    content: gpui_component::dialog::DialogContent,
    control: &WeakEntity<PageScaleControl>,
    _window: &mut Window,
    cx: &mut App,
) -> gpui_component::dialog::DialogContent {
    let snapshot = control
        .read_with(cx, |control, _| {
            (
                control.target,
                control.start,
                control.end,
                control.error.clone(),
            )
        })
        .expect("the page scale control must outlive its dialog");
    let (target, start, end, error) = snapshot;
    let page_label = target.map_or_else(
        || "No page selected".to_owned(),
        |(_, page_index)| format!("Page {}", page_index + 1),
    );
    let point_label = match (start, end) {
        (Some(start), Some(end)) => format!(
            "From ({:.2}, {:.2}) to ({:.2}, {:.2})",
            start.x, start.y, end.x, end.y
        ),
        _ => "Pick two endpoints on the PDF.".to_owned(),
    };
    content
        .child(
            v_flex()
                .id(PAGE_SCALE_DIALOG_ID)
                .debug_selector(|| PAGE_SCALE_DIALOG_ID.into())
                .gap_4()
                .p_4()
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new().child("Set Page Scale"))
                        .child(DialogDescription::new().child(format!(
                            "{page_label}. Choose a scale and the pages it applies to."
                        )))
                        .child(
                            DialogClose::new().child(
                                Button::new(PAGE_SCALE_CLOSE_ID)
                                    .debug_selector(|| PAGE_SCALE_CLOSE_ID.into())
                                    .label("Close")
                                    .ghost(),
                            ),
                        ),
                )
                .child(build_page_scale_dialog_body(control, point_label, cx))
                .when_some(error, |this, error| {
                    this.child(
                        div()
                            .id(PAGE_SCALE_ERROR_ID)
                            .debug_selector(|| PAGE_SCALE_ERROR_ID.into())
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(error),
                    )
                }),
        )
        .child(
            DialogFooter::new()
                .p_4()
                .child(
                    DialogClose::new().child(
                        Button::new(PAGE_SCALE_CANCEL_ID)
                            .debug_selector(|| PAGE_SCALE_CANCEL_ID.into())
                            .label("Cancel")
                            .outline(),
                    ),
                )
                .child(
                    DialogAction::new().child(
                        Button::new(PAGE_SCALE_APPLY_ID)
                            .debug_selector(|| PAGE_SCALE_APPLY_ID.into())
                            .label("Apply Scale")
                            .primary(),
                    ),
                ),
        )
}

fn build_page_scale_dialog_body(
    control: &WeakEntity<PageScaleControl>,
    point_label: String,
    cx: &mut App,
) -> impl gpui::IntoElement {
    let (
        mode,
        can_delete_preset,
        pages_mode,
        separate_y,
        save_preset,
        pdf_length,
        known_length,
        y_pdf_length,
        y_real_length,
        custom_range,
        preset_select,
        pages_select,
        pdf_units_select,
        real_units_select,
        precision_mode_select,
        precision_value_select,
    ) = control
        .read_with(cx, |control, app| {
            (
                control.mode,
                control.selected_user_preset_id(app).is_some(),
                control.pages_mode,
                control.separate_y,
                control.save_preset,
                control.pdf_length.clone(),
                control.known_length.clone(),
                control.y_pdf_length.clone(),
                control.y_real_length.clone(),
                control.custom_range.clone(),
                control.preset_select.clone(),
                control.pages_select.clone(),
                control.pdf_units_select.clone(),
                control.real_units_select.clone(),
                control.precision_mode_select.clone(),
                control.precision_value_select.clone(),
            )
        })
        .expect("the page scale control must outlive its dialog");
    let preset_mode_control = control.clone();
    let custom_mode_control = control.clone();
    let calibrate_mode_control = control.clone();
    let delete_preset_control = control.clone();
    let pick_control = control.clone();
    let separate_y_control = control.clone();
    let save_preset_control = control.clone();
    let custom_real_length = known_length.clone();

    v_flex()
        .id(PAGE_SCALE_DIALOG_BODY_ID)
        .debug_selector(|| PAGE_SCALE_DIALOG_BODY_ID.into())
        .max_h(px(560.))
        .overflow_y_scroll()
        .gap_4()
        .child(
            ButtonGroup::new("page-scale-method")
                .child(
                    Button::new(PAGE_SCALE_METHOD_PRESET_ID)
                        .debug_selector(|| PAGE_SCALE_METHOD_PRESET_ID.into())
                        .label("Preset")
                        .selected(mode == PageScaleMode::Preset)
                        .on_click(move |_, _, cx| {
                            let _ = preset_mode_control.update(cx, |control, cx| {
                                control.select_mode(PageScaleMode::Preset, cx)
                            });
                        }),
                )
                .child(
                    Button::new(PAGE_SCALE_METHOD_CUSTOM_ID)
                        .debug_selector(|| PAGE_SCALE_METHOD_CUSTOM_ID.into())
                        .label("Custom")
                        .selected(mode == PageScaleMode::Custom)
                        .on_click(move |_, _, cx| {
                            let _ = custom_mode_control.update(cx, |control, cx| {
                                control.select_mode(PageScaleMode::Custom, cx)
                            });
                        }),
                )
                .child(
                    Button::new(PAGE_SCALE_METHOD_CALIBRATE_ID)
                        .debug_selector(|| PAGE_SCALE_METHOD_CALIBRATE_ID.into())
                        .label("Calibrate")
                        .selected(mode == PageScaleMode::Calibrate)
                        .on_click(move |_, _, cx| {
                            let _ = calibrate_mode_control.update(cx, |control, cx| {
                                control.select_mode(PageScaleMode::Calibrate, cx)
                            });
                        }),
                ),
        )
        .when(mode == PageScaleMode::Preset, |this| {
            this.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child("Scale"))
                    .child(
                        div()
                            .id(PAGE_SCALE_PRESET_SELECT_ID)
                            .debug_selector(|| PAGE_SCALE_PRESET_SELECT_ID.into())
                            .child(Select::new(&preset_select)),
                    )
                    .when(can_delete_preset, |this| {
                        this.child(
                            Button::new(PAGE_SCALE_DELETE_PRESET_ID)
                                .debug_selector(|| PAGE_SCALE_DELETE_PRESET_ID.into())
                                .label("Delete")
                                .outline()
                                .on_click(move |_, window, cx| {
                                    let _ = delete_preset_control.update(cx, |control, cx| {
                                        control.delete_selected_preset(window, cx)
                                    });
                                }),
                        )
                    }),
            )
        })
        .when(mode == PageScaleMode::Custom, |this| {
            this.child(
                v_flex()
                    .gap_3()
                    .child(div().text_sm().child("Scale equation"))
                    .child(
                        div()
                            .id(PAGE_SCALE_CUSTOM_PDF_LENGTH_ID)
                            .debug_selector(|| PAGE_SCALE_CUSTOM_PDF_LENGTH_ID.into())
                            .child(NumberInput::new(&pdf_length)),
                    )
                    .child(
                        div()
                            .id(PAGE_SCALE_CUSTOM_PDF_UNITS_ID)
                            .debug_selector(|| PAGE_SCALE_CUSTOM_PDF_UNITS_ID.into())
                            .child(Select::new(&pdf_units_select)),
                    )
                    .child(
                        div()
                            .id(PAGE_SCALE_CUSTOM_REAL_LENGTH_ID)
                            .debug_selector(|| PAGE_SCALE_CUSTOM_REAL_LENGTH_ID.into())
                            .child(NumberInput::new(&custom_real_length)),
                    )
                    .child(
                        div()
                            .id(PAGE_SCALE_CUSTOM_REAL_UNITS_ID)
                            .debug_selector(|| PAGE_SCALE_CUSTOM_REAL_UNITS_ID.into())
                            .child(Select::new(&real_units_select)),
                    )
                    .child(
                        Checkbox::new(PAGE_SCALE_SEPARATE_Y_ID)
                            .debug_selector(|| PAGE_SCALE_SEPARATE_Y_ID.into())
                            .checked(separate_y)
                            .label("Separate Y scale")
                            .on_click(move |checked, _, cx| {
                                let checked = *checked;
                                let _ = separate_y_control.update(cx, |control, cx| {
                                    control.separate_y = checked;
                                    control.error = None;
                                    cx.notify();
                                });
                            }),
                    )
                    .when(separate_y, |this| {
                        this.child(
                            div()
                                .id(PAGE_SCALE_Y_PDF_LENGTH_ID)
                                .debug_selector(|| PAGE_SCALE_Y_PDF_LENGTH_ID.into())
                                .child(NumberInput::new(&y_pdf_length)),
                        )
                        .child(
                            div()
                                .id(PAGE_SCALE_Y_REAL_LENGTH_ID)
                                .debug_selector(|| PAGE_SCALE_Y_REAL_LENGTH_ID.into())
                                .child(NumberInput::new(&y_real_length)),
                        )
                    }),
            )
        })
        .when(mode == PageScaleMode::Calibrate, |this| {
            this.child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(PAGE_SCALE_PICK_STATUS_ID)
                            .debug_selector(|| PAGE_SCALE_PICK_STATUS_ID.into())
                            .text_sm()
                            .child(point_label),
                    )
                    .child(
                        Button::new(PAGE_SCALE_PICK_ID)
                            .debug_selector(|| PAGE_SCALE_PICK_ID.into())
                            .label("Pick Two Points")
                            .on_click(move |_, window, cx| {
                                let _ = pick_control.update(cx, |control, cx| {
                                    control.begin_pick(window, cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .id(PAGE_SCALE_KNOWN_LENGTH_ID)
                            .debug_selector(|| PAGE_SCALE_KNOWN_LENGTH_ID.into())
                            .child(NumberInput::new(&known_length)),
                    )
                    .child(
                        div()
                            .id(PAGE_SCALE_UNIT_GROUP_ID)
                            .debug_selector(|| PAGE_SCALE_UNIT_GROUP_ID.into())
                            .child(Select::new(&real_units_select)),
                    ),
            )
        })
        .child(
            v_flex()
                .gap_1()
                .child(div().text_sm().child("Pages"))
                .child(
                    div()
                        .id(PAGE_SCALE_PAGES_ID)
                        .debug_selector(|| PAGE_SCALE_PAGES_ID.into())
                        .child(Select::new(&pages_select)),
                ),
        )
        .when(pages_mode == PageScalePagesMode::Custom, |this| {
            this.child(
                div()
                    .id(PAGE_SCALE_RANGE_ID)
                    .debug_selector(|| PAGE_SCALE_RANGE_ID.into())
                    .child(Input::new(&custom_range)),
            )
        })
        .child(
            div()
                .id(PAGE_SCALE_PRECISION_MODE_ID)
                .debug_selector(|| PAGE_SCALE_PRECISION_MODE_ID.into())
                .child(Select::new(&precision_mode_select)),
        )
        .child(
            div()
                .id(PAGE_SCALE_PRECISION_VALUE_ID)
                .debug_selector(|| PAGE_SCALE_PRECISION_VALUE_ID.into())
                .child(Select::new(&precision_value_select)),
        )
        .when(mode != PageScaleMode::Preset, |this| {
            this.child(
                Checkbox::new(PAGE_SCALE_SAVE_PRESET_ID)
                    .debug_selector(|| PAGE_SCALE_SAVE_PRESET_ID.into())
                    .checked(save_preset)
                    .label("Add preset")
                    .on_click(move |checked, _, cx| {
                        let checked = *checked;
                        let _ = save_preset_control.update(cx, |control, cx| {
                            control.save_preset = checked;
                            control.error = None;
                            cx.notify();
                        });
                    }),
            )
        })
}

fn decimal_precision_labels() -> Vec<SharedString> {
    ["1", "0.1", "0.01", "0.001", "0.0001", "0.00001", "0.000001"]
        .into_iter()
        .map(SharedString::from)
        .collect()
}

fn fraction_precision_labels() -> Vec<SharedString> {
    ["1/2", "1/4", "1/8", "1/16", "1/32", "1/64"]
        .into_iter()
        .map(SharedString::from)
        .collect()
}

fn precision_label(mode: ScalePrecisionMode, value: f64) -> SharedString {
    match mode {
        ScalePrecisionMode::Decimal => format_scale_number(value).into(),
        ScalePrecisionMode::Fraction => format!("1/{}", value.round() as u16).into(),
    }
}

fn preset_label(preset: &ScalePreset) -> String {
    if preset.built_in {
        preset.name.clone()
    } else {
        format!("{} (saved)", preset.name)
    }
}

fn read_positive(value: &str, label: &str) -> Result<f64, String> {
    let value = value
        .parse::<f64>()
        .map_err(|_| format!("{label} must be a positive number."))?;
    if !value.is_finite() || value <= 0. {
        return Err(format!("{label} must be a positive number."));
    }
    Ok(value)
}

fn format_scale_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn constrain_point(start: PdfPoint, end: PdfPoint) -> PdfPoint {
    if (end.x - start.x).abs() >= (end.y - start.y).abs() {
        PdfPoint {
            x: end.x,
            y: start.y,
        }
    } else {
        PdfPoint {
            x: start.x,
            y: end.y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::constrain_point;
    use butter_paper_gpui_gallery::annotation_model::PdfPoint;

    #[test]
    fn calibration_shift_constraint_uses_the_dominant_axis() {
        let start = PdfPoint { x: 10., y: 20. };
        assert_eq!(
            constrain_point(start, PdfPoint { x: 80., y: 40. }),
            PdfPoint { x: 80., y: 20. }
        );
        assert_eq!(
            constrain_point(start, PdfPoint { x: 20., y: 100. }),
            PdfPoint { x: 10., y: 100. }
        );
    }
}
