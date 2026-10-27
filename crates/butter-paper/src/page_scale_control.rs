//! The Page Scale dialog: which scale a page is drawn at (a standard scale,
//! one the page states, a custom ratio, or one measured from a line of
//! known length), what measurements read in, and which pages it applies to.
//!
//! The workspace owns each page's scale and its undo history; this owns the
//! dialog and, while one is picked, the calibration line.

use crate::accessible_button::close_icon_button;
use crate::annotation_model::{
    PageScale, PageScaleApplyTarget, PdfPoint, ScalePrecision, ScalePrecisionMode, ScaleSource,
    ScaleUnit, parse_page_scale_ranges,
};
use crate::dialog_buttons::{dialog_action_button, dialog_close_button};
use crate::page_scale_catalog::{
    STANDARD_SCALES, ScaleSystem, StandardScale, imperial_unit, nearest_standard, paper_label,
    paper_length_label, parse_length, ratio_label, ratio_of, scale_from_ratio, scale_label,
    standard_for, stated_scales, trimmed, with_unit,
};
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity,
    Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    dialog::{DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle},
    form::Field,
    h_flex,
    input::{Input, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectGroup, SelectItem, SelectState},
    v_flex,
};

use crate::document_workspace::{DocumentId, DocumentWorkspace};

pub const PAGE_SCALE_TRIGGER_ID: &str = "measure-set-page-scale";
pub const PAGE_SCALE_DIALOG_ID: &str = "page-scale-dialog";
pub const PAGE_SCALE_CLOSE_ID: &str = "page-scale-close";
pub const PAGE_SCALE_SELECT_ID: &str = "page-scale-select";
pub const PAGE_SCALE_CALIBRATE_ID: &str = "page-scale-calibrate";
pub const PAGE_SCALE_STATED_ID: &str = "page-scale-stated";
pub const PAGE_SCALE_CUSTOM_PAPER_ID: &str = "page-scale-custom-paper";
pub const PAGE_SCALE_CUSTOM_PAPER_UNIT_ID: &str = "page-scale-custom-paper-unit";
pub const PAGE_SCALE_CUSTOM_REAL_ID: &str = "page-scale-custom-real";
pub const PAGE_SCALE_KNOWN_LENGTH_ID: &str = "page-scale-known-length";
pub const PAGE_SCALE_RESULT_ID: &str = "page-scale-result";
pub const PAGE_SCALE_USE_NEAREST_ID: &str = "page-scale-use-nearest";
pub const PAGE_SCALE_UNIT_ID: &str = "page-scale-unit";
pub const PAGE_SCALE_PRECISION_ID: &str = "page-scale-precision";
pub const PAGE_SCALE_PAGES_THIS_ID: &str = "page-scale-pages-this";
pub const PAGE_SCALE_PAGES_ALL_ID: &str = "page-scale-pages-all";
pub const PAGE_SCALE_PAGES_RANGE_ID: &str = "page-scale-pages-range";
pub const PAGE_SCALE_RANGE_ID: &str = "page-scale-range";
pub const PAGE_SCALE_ERROR_ID: &str = "page-scale-error";
pub const PAGE_SCALE_CANCEL_ID: &str = "page-scale-cancel";
pub const PAGE_SCALE_APPLY_ID: &str = "page-scale-apply";
pub const PAGE_SCALE_PICK_HINT_ID: &str = "page-scale-pick-hint";
pub const PAGE_SCALE_PICK_CANCEL_ID: &str = "page-scale-pick-cancel";

const KEY_CUSTOM: &str = "custom";
const KEY_MEASURED: &str = "measured";
const KEY_NONE: &str = "none";
const UNITS: [ScaleUnit; 5] = [
    ScaleUnit::Mm,
    ScaleUnit::Cm,
    ScaleUnit::M,
    ScaleUnit::In,
    ScaleUnit::Ft,
];
const PAPER_UNITS: [ScaleUnit; 3] = [ScaleUnit::Mm, ScaleUnit::Cm, ScaleUnit::In];
const DECIMALS: [f64; 4] = [1., 0.1, 0.01, 0.001];
const FRACTIONS: [u16; 6] = [2, 4, 8, 16, 32, 64];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationPointDisposition {
    Ignored,
    FirstPoint,
    Completed,
}

/// The scale the dialog will apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleChoice {
    /// Nothing chosen yet, on a page without a scale.
    Unset,
    /// Remove the scale: measurements read paper size.
    None,
    /// An index into [`STANDARD_SCALES`].
    Standard(usize),
    Custom,
    /// From the calibration line and its known length.
    Measured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageScalePages {
    This,
    All,
    Range,
}

/// Everything about the page the dialog is for, read by the workspace
/// (the dialog cannot read the workspace while it is updating).
#[derive(Clone, Debug, PartialEq)]
pub struct PageScaleRequest {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub page_count: u32,
    /// The page as shown, in points.
    pub page_size: (f64, f64),
    pub current: Option<PageScale>,
}

/// The calibration line as the page draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationPreview {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub label: String,
}

/// What the dialog's scale inputs held when it opened.
#[derive(Clone, Debug, PartialEq)]
struct OpenedInputs {
    choice: ScaleChoice,
    display_unit: ScaleUnit,
    precision: (ScalePrecisionMode, u64),
    custom_paper_unit: ScaleUnit,
    custom_paper: String,
    custom_real: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorPlace {
    Scale,
    Pages,
    Apply,
}

#[derive(Clone, Debug)]
struct ScaleItem {
    key: SharedString,
    title: SharedString,
}

impl SelectItem for ScaleItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &SharedString {
        &self.key
    }
}

type ScaleList = SearchableVec<SelectGroup<ScaleItem>>;

fn standard_key(index: usize) -> SharedString {
    format!("standard-{index}").into()
}

fn unit_title(unit: ScaleUnit) -> &'static str {
    match unit {
        ScaleUnit::Mm => "Millimetres",
        ScaleUnit::Cm => "Centimetres",
        ScaleUnit::M => "Metres",
        ScaleUnit::In => "Inches",
        ScaleUnit::Ft => "Feet",
    }
}

fn precision_choices(unit: ScaleUnit) -> Vec<ScalePrecision> {
    let mut choices = DECIMALS
        .iter()
        .map(|value| ScalePrecision::decimal(*value).expect("a positive precision"))
        .collect::<Vec<_>>();
    if imperial_unit(unit) {
        choices.extend(
            FRACTIONS
                .iter()
                .map(|denominator| ScalePrecision::fraction(*denominator).expect("a fraction")),
        );
    }
    choices
}

fn precision_title(precision: ScalePrecision, unit: ScaleUnit) -> SharedString {
    match precision.mode {
        ScalePrecisionMode::Decimal => {
            format!("{} {}", trimmed(precision.value, 3), unit.as_str()).into()
        }
        ScalePrecisionMode::Fraction => {
            format!("1/{} {}", precision.value.round(), unit.as_str()).into()
        }
    }
}

/// A unit's own default precision: whole millimetres, centimetres to the
/// millimetre, metres and feet to the hundredth, inches to the sixteenth.
fn unit_default_precision(unit: ScaleUnit) -> ScalePrecision {
    match unit {
        ScaleUnit::Mm => ScalePrecision::decimal(1.),
        ScaleUnit::Cm => ScalePrecision::decimal(0.1),
        ScaleUnit::M | ScaleUnit::Ft => ScalePrecision::decimal(0.01),
        ScaleUnit::In => ScalePrecision::fraction(16),
    }
    .expect("a positive precision")
}

fn same_precision(left: ScalePrecision, right: ScalePrecision) -> bool {
    left.mode == right.mode && (left.value - right.value).abs() < 1e-9
}

fn distance(start: PdfPoint, end: PdfPoint) -> f64 {
    (end.x - start.x).hypot(end.y - start.y)
}

/// The end of a line from `start` held horizontal or vertical (Shift).
pub fn constrain_point(start: PdfPoint, end: PdfPoint) -> PdfPoint {
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

/// A length in `unit`, written for a field: up to six decimals.
fn length_text(value: f64) -> String {
    trimmed(value, 6)
}

pub struct PageScaleControl {
    owner: WeakEntity<DocumentWorkspace>,
    request: Option<PageScaleRequest>,
    choice: ScaleChoice,
    /// The page's text has been searched for the scales it states.
    stated_read: bool,
    stated: Vec<usize>,
    display_unit: ScaleUnit,
    precision: ScalePrecision,
    custom_paper_unit: ScaleUnit,
    pages: PageScalePages,
    picking: bool,
    start: Option<PdfPoint>,
    end: Option<PdfPoint>,
    hover: Option<PdfPoint>,
    /// The choice before Calibrate, restored when the pick is abandoned.
    choice_before_pick: ScaleChoice,
    error: Option<(ErrorPlace, String)>,
    /// The inputs as opened on a page with a scale: unchanged, Apply keeps
    /// that scale exactly, including what the dialog cannot show (a
    /// different vertical scale, an unlisted precision).
    opened: Option<OpenedInputs>,
    applied_count: usize,
    scale_select: Entity<SelectState<ScaleList>>,
    unit_select: Entity<SelectState<Vec<SharedString>>>,
    precision_select: Entity<SelectState<Vec<SharedString>>>,
    paper_unit_select: Entity<SelectState<Vec<SharedString>>>,
    custom_paper: Entity<InputState>,
    custom_real: Entity<InputState>,
    known_length: Entity<InputState>,
    range: Entity<InputState>,
}

impl PageScaleControl {
    pub fn new(
        owner: WeakEntity<DocumentWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scale_select = cx.new(|cx| {
            SelectState::new(scale_list(&[], false, false), None, window, cx).searchable(true)
        });
        let unit_select = cx.new(|cx| {
            SelectState::new(
                UNITS
                    .iter()
                    .map(|unit| SharedString::from(unit_title(*unit)))
                    .collect::<Vec<_>>(),
                None,
                window,
                cx,
            )
        });
        let precision_select = cx.new(|cx| SelectState::new(Vec::new(), None, window, cx));
        let paper_unit_select = cx.new(|cx| {
            SelectState::new(
                PAPER_UNITS
                    .iter()
                    .map(|unit| SharedString::from(unit.as_str()))
                    .collect::<Vec<_>>(),
                None,
                window,
                cx,
            )
        });
        let custom_paper = cx.new(|cx| InputState::new(window, cx).default_value("1"));
        let custom_real = cx.new(|cx| InputState::new(window, cx).placeholder("Real length"));
        let known_length = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 3600 mm"));
        let range = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 1-3, 5"));

        cx.subscribe_in(
            &scale_select,
            window,
            |control, _, event: &SelectEvent<ScaleList>, window, cx| {
                let SelectEvent::Confirm(Some(key)) = event else {
                    return;
                };
                let choice = match key.as_ref() {
                    KEY_CUSTOM => ScaleChoice::Custom,
                    KEY_MEASURED => ScaleChoice::Measured,
                    KEY_NONE => ScaleChoice::None,
                    key => match key
                        .strip_prefix("standard-")
                        .and_then(|index| index.parse().ok())
                    {
                        Some(index) => ScaleChoice::Standard(index),
                        None => return,
                    },
                };
                control.choose(choice, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &unit_select,
            window,
            |control, _, event: &SelectEvent<Vec<SharedString>>, window, cx| {
                let SelectEvent::Confirm(Some(title)) = event else {
                    return;
                };
                if let Some(unit) = UNITS
                    .iter()
                    .find(|unit| unit_title(**unit) == title.as_ref())
                {
                    control.set_display_unit(*unit, window, cx);
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &precision_select,
            window,
            |control, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(Some(title)) = event else {
                    return;
                };
                if let Some(precision) = precision_choices(control.display_unit)
                    .into_iter()
                    .find(|precision| precision_title(*precision, control.display_unit) == *title)
                {
                    control.precision = precision;
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &paper_unit_select,
            window,
            |control, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(Some(title)) = event else {
                    return;
                };
                if let Ok(unit) = ScaleUnit::parse(title.as_ref()) {
                    control.custom_paper_unit = unit;
                    control.error = None;
                    cx.notify();
                }
            },
        )
        .detach();
        for input in [&custom_paper, &custom_real, &known_length, &range] {
            cx.observe(input, |control, _, cx| {
                if control.error.is_some() {
                    control.error = None;
                }
                cx.notify();
            })
            .detach();
        }

        Self {
            owner,
            request: None,
            choice: ScaleChoice::Unset,
            stated_read: false,
            stated: Vec::new(),
            display_unit: ScaleUnit::M,
            precision: unit_default_precision(ScaleUnit::M),
            custom_paper_unit: ScaleUnit::Cm,
            pages: PageScalePages::This,
            picking: false,
            start: None,
            end: None,
            hover: None,
            choice_before_pick: ScaleChoice::Unset,
            error: None,
            opened: None,
            applied_count: 0,
            scale_select,
            unit_select,
            precision_select,
            paper_unit_select,
            custom_paper,
            custom_real,
            known_length,
            range,
        }
    }

    pub fn request(&self) -> Option<&PageScaleRequest> {
        self.request.as_ref()
    }

    pub fn target(&self) -> Option<(DocumentId, u32)> {
        self.request
            .as_ref()
            .map(|request| (request.document_id, request.page_index))
    }

    pub fn choice(&self) -> ScaleChoice {
        self.choice
    }

    pub fn pages(&self) -> PageScalePages {
        self.pages
    }

    pub fn display(&self) -> (ScaleUnit, ScalePrecision) {
        (self.display_unit, self.precision)
    }

    pub fn stated(&self) -> Vec<&'static StandardScale> {
        self.stated
            .iter()
            .map(|index| &STANDARD_SCALES[*index])
            .collect()
    }

    pub fn is_picking(&self) -> bool {
        self.picking
    }

    pub fn points(&self) -> (Option<PdfPoint>, Option<PdfPoint>) {
        (self.start, self.end)
    }

    pub fn applied_count(&self) -> usize {
        self.applied_count
    }

    pub fn known_length_input(&self) -> Entity<InputState> {
        self.known_length.clone()
    }

    pub fn custom_paper_input(&self) -> Entity<InputState> {
        self.custom_paper.clone()
    }

    pub fn custom_real_input(&self) -> Entity<InputState> {
        self.custom_real.clone()
    }

    pub fn range_input(&self) -> Entity<InputState> {
        self.range.clone()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(|(_, error)| error.as_str())
    }

    /// Whether the dialog still waits for the page's text, to look for the
    /// scales it states.
    pub fn wants_stated_scales(&self, document_id: DocumentId, page_index: u32) -> bool {
        !self.stated_read && self.target() == Some((document_id, page_index))
    }

    /// What the page says while a line is picked.
    pub fn pick_instruction(&self) -> Option<&'static str> {
        self.picking.then_some(if self.start.is_some() {
            "Click the other end. Hold Shift to keep the line straight."
        } else {
            "Click one end of a length you know, like a dimensioned wall."
        })
    }

    /// The calibration line: following the pointer while it is picked, and
    /// finished while its length is asked for.
    pub fn preview(&self) -> Option<CalibrationPreview> {
        let request = self.request.as_ref()?;
        let start = self.start?;
        let end = if self.picking {
            self.hover?
        } else {
            self.end.filter(|_| self.choice == ScaleChoice::Measured)?
        };
        let imperial = imperial_unit(self.display_unit);
        let paper = distance(start, end);
        let label = match request.current.as_ref().filter(|_| self.picking) {
            Some(current) => format!(
                "{} {}",
                trimmed(paper * current.scale_x, 2),
                current.real_units.as_str()
            ),
            None => paper_length_label(paper, imperial),
        };
        Some(CalibrationPreview {
            document_id: request.document_id,
            page_index: request.page_index,
            start,
            end,
            label,
        })
    }

    pub fn open(&mut self, request: PageScaleRequest, window: &mut Window, cx: &mut Context<Self>) {
        let reopened_page = self.target() == Some((request.document_id, request.page_index));
        if !reopened_page {
            self.stated_read = false;
            self.stated.clear();
        }
        self.picking = false;
        self.start = None;
        self.end = None;
        self.hover = None;
        self.error = None;
        self.pages = PageScalePages::This;
        self.range.update(cx, |input, cx| {
            input.set_value((request.page_index + 1).to_string(), window, cx)
        });
        self.known_length
            .update(cx, |input, cx| input.set_value("", window, cx));
        match request.current.as_ref() {
            Some(current) => {
                self.display_unit = current.real_units;
                self.precision = current.precision;
                let ratio = ratio_of(current);
                self.choice = match standard_for(ratio, current.real_units)
                    .filter(|_| (current.scale_y - current.scale_x).abs() <= current.scale_x * 1e-4)
                {
                    Some(standard) => ScaleChoice::Standard(standard_index(standard)),
                    None => {
                        self.fill_custom_from_ratio(ratio, window, cx);
                        ScaleChoice::Custom
                    }
                };
            }
            None => {
                // The scale the page states, when its text has been read.
                self.choice = match self.stated.first() {
                    Some(index) if reopened_page => ScaleChoice::Standard(*index),
                    _ => ScaleChoice::Unset,
                };
                let imperial = paper_is_imperial(request.page_size);
                self.display_unit = if imperial {
                    ScaleUnit::Ft
                } else {
                    ScaleUnit::M
                };
                self.precision = unit_default_precision(self.display_unit);
                if let ScaleChoice::Standard(index) = self.choice {
                    (self.display_unit, self.precision) = STANDARD_SCALES[index].default_display();
                }
                self.custom_paper_unit = if imperial {
                    ScaleUnit::In
                } else {
                    ScaleUnit::Cm
                };
                self.custom_paper
                    .update(cx, |input, cx| input.set_value("1", window, cx));
                self.custom_real
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        let has_scale = request.current.is_some();
        self.request = Some(request);
        self.opened = has_scale.then(|| self.opened_inputs(cx));
        self.sync_selects(window, cx);
        self.open_dialog(window, cx);
        cx.notify();
    }

    fn opened_inputs(&self, cx: &App) -> OpenedInputs {
        OpenedInputs {
            choice: self.choice,
            display_unit: self.display_unit,
            precision: (self.precision.mode, self.precision.value.to_bits()),
            custom_paper_unit: self.custom_paper_unit,
            custom_paper: self.custom_paper.read(cx).value().to_string(),
            custom_real: self.custom_real.read(cx).value().to_string(),
        }
    }

    /// The scales the page's text states, once it has been read.
    pub fn set_page_text(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.wants_stated_scales(document_id, page_index) {
            return;
        }
        let Some(request) = self.request.as_ref() else {
            return;
        };
        self.stated_read = true;
        self.stated = stated_scales(text, request.page_size)
            .into_iter()
            .take(4)
            .map(|stated| standard_index(stated.scale))
            .collect();
        // A page without a scale starts at the scale it states most, which
        // the list and its chip show; anything chosen already is kept.
        if self.choice == ScaleChoice::Unset
            && let Some(index) = self.stated.first().copied()
        {
            let (unit, precision) = STANDARD_SCALES[index].default_display();
            self.display_unit = unit;
            self.precision = precision;
            self.choice = ScaleChoice::Standard(index);
        }
        self.sync_selects(window, cx);
        cx.notify();
    }

    fn choose(&mut self, choice: ScaleChoice, window: &mut Window, cx: &mut Context<Self>) {
        if choice == ScaleChoice::Custom && self.choice != ScaleChoice::Custom {
            // Custom starts from the scale chosen so far.
            if let Some(ratio) = self.ratio(cx) {
                self.fill_custom_from_ratio(ratio, window, cx);
            }
        }
        if let ScaleChoice::Standard(index) = choice {
            let standard = &STANDARD_SCALES[index];
            if standard.system.imperial() != imperial_unit(self.display_unit) {
                let (unit, precision) = standard.default_display();
                self.display_unit = unit;
                self.precision = precision;
            }
        }
        self.choice = choice;
        self.error = None;
        self.sync_selects(window, cx);
        cx.notify();
    }

    /// Measures in `unit`, keeping the scale: typed real lengths are
    /// converted, so `1 cm = 1 m` stays 1:100 when metres become millimetres.
    fn set_display_unit(&mut self, unit: ScaleUnit, window: &mut Window, cx: &mut Context<Self>) {
        if unit == self.display_unit {
            return;
        }
        let previous = self.display_unit;
        for input in [&self.custom_real, &self.known_length] {
            let value = input.read(cx).value().to_string();
            if let Ok(length) = parse_length(&value, previous) {
                let converted = length * previous.points() / unit.points();
                input.update(cx, |input, cx| {
                    input.set_value(length_text(converted), window, cx)
                });
            }
        }
        self.display_unit = unit;
        self.precision = unit_default_precision(unit);
        self.error = None;
        self.sync_selects(window, cx);
        cx.notify();
    }

    pub fn choose_for_test(
        &mut self,
        choice: ScaleChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose(choice, window, cx);
    }

    pub fn set_display_for_test(
        &mut self,
        unit: ScaleUnit,
        precision: ScalePrecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_display_unit(unit, window, cx);
        self.precision = precision;
        self.sync_selects(window, cx);
    }

    pub fn set_pages(
        &mut self,
        pages: PageScalePages,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pages = pages;
        self.error = None;
        if pages == PageScalePages::Range {
            // Ready to type the pages over the suggested one.
            let range = self.range.clone();
            window.defer(cx, move |window, cx| {
                range.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            });
        }
        cx.notify();
    }

    fn fill_custom_from_ratio(&mut self, ratio: f64, window: &mut Window, cx: &mut Context<Self>) {
        let paper_unit = if imperial_unit(self.display_unit) {
            ScaleUnit::In
        } else {
            ScaleUnit::Cm
        };
        self.custom_paper_unit = paper_unit;
        let real = ratio * paper_unit.points() / self.display_unit.points();
        self.custom_paper
            .update(cx, |input, cx| input.set_value("1", window, cx));
        self.custom_real.update(cx, |input, cx| {
            input.set_value(length_text(real), window, cx)
        });
    }

    /// Real lengths per paper length for the current choice.
    fn ratio_result(&self, cx: &App) -> Result<f64, String> {
        match self.choice {
            ScaleChoice::Unset => {
                Err("Choose a scale, or calibrate one from a known length.".into())
            }
            ScaleChoice::None => Err("No scale".into()),
            ScaleChoice::Standard(index) => Ok(STANDARD_SCALES[index].ratio),
            ScaleChoice::Custom => {
                let paper = parse_length(
                    self.custom_paper.read(cx).value().as_ref(),
                    self.custom_paper_unit,
                )
                .map_err(|_| "Enter the paper length, like 1.".to_owned())?;
                let real = parse_length(
                    self.custom_real.read(cx).value().as_ref(),
                    self.display_unit,
                )
                .map_err(|_| "Enter the real length that paper length stands for.".to_owned())?;
                Ok(real * self.display_unit.points() / (paper * self.custom_paper_unit.points()))
            }
            ScaleChoice::Measured => {
                let (Some(start), Some(end)) = (self.start, self.end) else {
                    return Err("Calibrate: click both ends of a length you know.".into());
                };
                let known = parse_length(
                    self.known_length.read(cx).value().as_ref(),
                    self.display_unit,
                )?;
                Ok(known * self.display_unit.points() / distance(start, end))
            }
        }
    }

    fn ratio(&self, cx: &App) -> Option<f64> {
        self.ratio_result(cx).ok()
    }

    /// The scale Apply sets on the dialog's page; `None` removes it.
    pub fn resolved_scale(&self, cx: &App) -> Result<Option<PageScale>, String> {
        let Some(request) = self.request.as_ref() else {
            return Err("The page is no longer open.".into());
        };
        if self.choice == ScaleChoice::None {
            return Ok(None);
        }
        if let Some(current) = request.current.as_ref()
            && self.opened.as_ref() == Some(&self.opened_inputs(cx))
        {
            return Ok(Some(current.clone()));
        }
        let ratio = self.ratio_result(cx)?;
        let (source, name) = match self.choice {
            ScaleChoice::Standard(index) => {
                (ScaleSource::Preset, STANDARD_SCALES[index].label.to_owned())
            }
            ScaleChoice::Measured => (
                ScaleSource::Calibrated,
                ratio_label(ratio, self.display_unit),
            ),
            _ => (ScaleSource::Custom, ratio_label(ratio, self.display_unit)),
        };
        let scale = scale_from_ratio(
            request.page_index,
            source,
            &name,
            ratio,
            self.display_unit,
            self.precision,
        );
        // The same scale chosen again is kept exactly as the page has it,
        // so applying it changes nothing.
        if let Some(current) = request.current.as_ref()
            && same_scale(current, &scale)
        {
            return Ok(Some(current.clone()));
        }
        Ok(Some(scale))
    }

    fn target_pages(&self, cx: &App) -> Result<PageScaleApplyTarget, String> {
        let request = self.request.as_ref().ok_or("The page is no longer open.")?;
        match self.pages {
            PageScalePages::This => Ok(PageScaleApplyTarget::Current(request.page_index)),
            PageScalePages::All => Ok(PageScaleApplyTarget::All),
            PageScalePages::Range => {
                parse_page_scale_ranges(self.range.read(cx).value().as_ref(), request.page_count)
                    .map(PageScaleApplyTarget::Ranges)
                    .map_err(|error| error.to_string())
            }
        }
    }

    /// Sets (or removes) the scale on the chosen pages, as one undoable
    /// change. Returns whether the dialog may close.
    pub fn apply(&mut self, cx: &mut Context<Self>) -> bool {
        let scale = match self.resolved_scale(cx) {
            Ok(scale) => scale,
            Err(error) => return self.fail(ErrorPlace::Scale, error, cx),
        };
        let target = match self.target_pages(cx) {
            Ok(target) => target,
            Err(error) => return self.fail(ErrorPlace::Pages, error, cx),
        };
        let Some(request) = self.request.clone() else {
            return false;
        };
        let result = self.owner.update(cx, |workspace, cx| match scale {
            Some(scale) => workspace.apply_page_scale(request.document_id, scale, target, cx),
            None => workspace.remove_page_scale(request.document_id, target, cx),
        });
        match result {
            Ok(Ok(_)) => {
                self.error = None;
                self.applied_count = self.applied_count.saturating_add(1);
                self.start = None;
                self.end = None;
                cx.notify();
                true
            }
            Ok(Err(error)) => self.fail(ErrorPlace::Apply, error, cx),
            Err(_) => self.fail(
                ErrorPlace::Apply,
                "The document is no longer open.".into(),
                cx,
            ),
        }
    }

    fn fail(&mut self, place: ErrorPlace, error: String, cx: &mut Context<Self>) -> bool {
        self.error = Some((place, error));
        cx.notify();
        false
    }

    /// Hides the dialog so both ends of a known length can be clicked.
    pub fn begin_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.request.is_none() {
            return;
        }
        if !self.picking {
            self.choice_before_pick = self.choice;
        }
        self.picking = true;
        self.start = None;
        self.end = None;
        self.hover = None;
        self.error = None;
        window.close_dialog(cx);
        cx.notify();
    }

    /// Abandons the line being picked and shows the dialog as it was.
    pub fn cancel_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.picking {
            return false;
        }
        self.picking = false;
        self.start = None;
        self.end = None;
        self.hover = None;
        if self.choice == ScaleChoice::Measured {
            self.choice = self.choice_before_pick;
        }
        self.sync_selects(window, cx);
        self.open_dialog(window, cx);
        cx.notify();
        true
    }

    /// Whether a first calibration click on `page_index` would move the
    /// dialog to that page.
    pub fn picking_from(&self, document_id: DocumentId, page_index: u32) -> bool {
        self.picking
            && self.start.is_none()
            && self.request.as_ref().is_some_and(|request| {
                request.document_id == document_id && request.page_index != page_index
            })
    }

    /// Moves the dialog to another page of its document, keeping what is
    /// chosen: the calibration line's first end was clicked there.
    pub fn retarget(&mut self, request: PageScaleRequest, cx: &mut Context<Self>) {
        if self.request.as_ref().map(|current| current.document_id) != Some(request.document_id) {
            return;
        }
        self.stated_read = false;
        self.stated.clear();
        self.opened = None;
        self.request = Some(request);
        cx.notify();
    }

    /// Where the pointer is on the page while a line is picked.
    pub fn hover_point(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        constrain: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.picking || self.target() != Some((document_id, page_index)) {
            return;
        }
        let point = match (self.start, constrain) {
            (Some(start), true) => constrain_point(start, point),
            _ => point,
        };
        if self.hover != Some(point) {
            self.hover = Some(point);
            cx.notify();
        }
    }

    /// One end of the line: the first starts it, the second finishes it and
    /// asks for its length. Both must be on the dialog's page.
    pub fn record_point(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        constrain: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CalibrationPointDisposition {
        if !self.picking || self.target() != Some((document_id, page_index)) {
            return CalibrationPointDisposition::Ignored;
        }
        let Some(start) = self.start else {
            self.start = Some(point);
            self.hover = Some(point);
            cx.notify();
            return CalibrationPointDisposition::FirstPoint;
        };
        let end = if constrain {
            constrain_point(start, point)
        } else {
            point
        };
        if distance(start, end) < 1. {
            return CalibrationPointDisposition::Ignored;
        }
        self.end = Some(end);
        self.hover = None;
        self.picking = false;
        self.choice = ScaleChoice::Measured;
        // With a scale already set, the field starts at what the line
        // measures now: correcting it is often all that is needed.
        let current = self
            .request
            .as_ref()
            .and_then(|request| request.current.as_ref())
            .map(|current| with_unit(current, self.display_unit))
            .map(|current| trimmed(distance(start, end) * current.scale_x, 3))
            .unwrap_or_default();
        self.known_length
            .update(cx, |input, cx| input.set_value(current, window, cx));
        self.sync_selects(window, cx);
        self.open_dialog(window, cx);
        let known_length = self.known_length.clone();
        window.defer(cx, move |window, cx| {
            known_length.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
        });
        cx.notify();
        CalibrationPointDisposition::Completed
    }

    /// Forgets the dialog's page when its document closes or loses focus.
    pub fn cancel_for_document(&mut self, document_id: DocumentId, cx: &mut Context<Self>) -> bool {
        if self.request.as_ref().map(|request| request.document_id) != Some(document_id) {
            return false;
        }
        self.request = None;
        self.picking = false;
        self.start = None;
        self.end = None;
        self.hover = None;
        self.error = None;
        cx.notify();
        true
    }

    fn sync_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let measured = self.start.is_some() && self.end.is_some();
        let imperial_first = self
            .request
            .as_ref()
            .is_some_and(|request| paper_is_imperial(request.page_size));
        let list = scale_list(&self.stated, measured, imperial_first);
        let key: Option<SharedString> = match self.choice {
            ScaleChoice::Unset => None,
            ScaleChoice::None => Some(KEY_NONE.into()),
            ScaleChoice::Standard(index) => Some(standard_key(index)),
            ScaleChoice::Custom => Some(KEY_CUSTOM.into()),
            ScaleChoice::Measured => Some(KEY_MEASURED.into()),
        };
        self.scale_select.update(cx, |select, cx| {
            select.set_items(list, window, cx);
            match &key {
                Some(key) => select.set_selected_value(key, window, cx),
                None => select.set_selected_index(None, window, cx),
            }
        });
        let unit = SharedString::from(unit_title(self.display_unit));
        self.unit_select.update(cx, |select, cx| {
            select.set_selected_value(&unit, window, cx)
        });
        let precisions = precision_choices(self.display_unit);
        let titles = precisions
            .iter()
            .map(|precision| precision_title(*precision, self.display_unit))
            .collect::<Vec<_>>();
        let selected = precision_title(self.precision, self.display_unit);
        self.precision_select.update(cx, |select, cx| {
            select.set_items(titles, window, cx);
            select.set_selected_value(&selected, window, cx);
        });
        let paper_unit = SharedString::from(self.custom_paper_unit.as_str());
        self.paper_unit_select.update(cx, |select, cx| {
            select.set_selected_value(&paper_unit, window, cx)
        });
    }

    fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let control = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let apply_control = control.clone();
            let content_control = control.clone();
            dialog
                .close_button(false)
                .w(px(560.))
                .max_w_full()
                .on_ok(move |_, _, cx| {
                    apply_control
                        .update(cx, |control, cx| control.apply(cx))
                        .unwrap_or(false)
                })
                .content(move |content, _, cx| dialog_content(content, &content_control, cx))
        });
    }
}

fn standard_index(scale: &StandardScale) -> usize {
    STANDARD_SCALES
        .iter()
        .position(|candidate| candidate == scale)
        .expect("a standard scale is in the list")
}

/// Letter, Tabloid and the ARCH and ANSI sheets are drawn in inches.
fn paper_is_imperial(page_size: (f64, f64)) -> bool {
    let label = paper_label(page_size.0, page_size.1);
    label.ends_with(" in") && !label.starts_with(|c: char| c.is_ascii_digit())
}

fn same_scale(left: &PageScale, right: &PageScale) -> bool {
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(b.abs());
    left.real_units == right.real_units
        && close(left.scale_x, right.scale_x)
        && close(left.scale_y, right.scale_y)
        && same_precision(left.precision, right.precision)
}

fn scale_list(stated: &[usize], measured: bool, imperial_first: bool) -> ScaleList {
    let item = |key: SharedString, title: String| ScaleItem {
        key,
        title: title.into(),
    };
    let group = |system: ScaleSystem| {
        SelectGroup::new(system.title()).items(
            STANDARD_SCALES
                .iter()
                .enumerate()
                .filter(|(_, scale)| scale.system == system)
                .map(|(index, scale)| item(standard_key(index), scale.label.to_owned())),
        )
    };
    let mut groups = Vec::new();
    if !stated.is_empty() {
        groups.push(
            SelectGroup::new("Stated on this page").items(stated.iter().map(|index| {
                item(
                    standard_key(*index),
                    STANDARD_SCALES[*index].label.to_owned(),
                )
            })),
        );
    }
    let systems = if imperial_first {
        [
            ScaleSystem::Architectural,
            ScaleSystem::Engineering,
            ScaleSystem::Metric,
        ]
    } else {
        [
            ScaleSystem::Metric,
            ScaleSystem::Architectural,
            ScaleSystem::Engineering,
        ]
    };
    groups.extend(systems.into_iter().map(group));
    let mut other = SelectGroup::new("Other");
    if measured {
        other = other.item(item(
            KEY_MEASURED.into(),
            "Calibrated from your line".into(),
        ));
    }
    other = other
        .item(item(KEY_CUSTOM.into(), "Custom ratio".into()))
        .item(item(KEY_NONE.into(), "No scale".into()));
    groups.push(other);
    SearchableVec::new(groups)
}

fn dialog_content(
    content: gpui_component::dialog::DialogContent,
    control: &WeakEntity<PageScaleControl>,
    cx: &mut App,
) -> gpui_component::dialog::DialogContent {
    let Some(entity) = control.upgrade() else {
        return content;
    };
    let state = entity.read(cx);
    let Some(request) = state.request.clone() else {
        return content;
    };
    let description = {
        let mut parts = vec![
            format!("Page {} of {}", request.page_index + 1, request.page_count),
            paper_label(request.page_size.0, request.page_size.1),
        ];
        if let Some(current) = request.current.as_ref() {
            parts.push(format!("Currently {}", scale_label(current)));
        }
        parts.join(" · ")
    };
    content
        .child(
            v_flex()
                .id(PAGE_SCALE_DIALOG_ID)
                .debug_selector(|| PAGE_SCALE_DIALOG_ID.into())
                .relative()
                .gap_4()
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new().child("Page Scale"))
                        .child(DialogDescription::new().child(description))
                        .child(
                            // Inside the content: the stock body clips anything
                            // above it, leaving an outset X half unhoverable.
                            div().absolute().top_0().right_0().child(
                                DialogClose::new()
                                    .child(close_icon_button(PAGE_SCALE_CLOSE_ID, "Close")),
                            ),
                        ),
                )
                .child(
                    v_flex()
                        .id("page-scale-dialog-body")
                        .gap_5()
                        .max_h(px(520.))
                        .overflow_y_scroll()
                        .child(scale_section(&entity, &request, cx))
                        .when(entity.read(cx).choice != ScaleChoice::None, |body| {
                            body.child(display_section(&entity, cx))
                        })
                        .when(request.page_count > 1, |body| {
                            body.child(pages_section(&entity, &request, cx))
                        }),
                ),
        )
        .child(
            DialogFooter::new()
                .child(dialog_close_button(
                    Button::new(PAGE_SCALE_CANCEL_ID)
                        .debug_selector(|| PAGE_SCALE_CANCEL_ID.into())
                        .label("Cancel")
                        .outline(),
                ))
                .child(dialog_action_button(
                    Button::new(PAGE_SCALE_APPLY_ID)
                        .debug_selector(|| PAGE_SCALE_APPLY_ID.into())
                        .label("Apply")
                        .primary(),
                )),
        )
}

fn error_text(
    state: &PageScaleControl,
    place: ErrorPlace,
    cx: &App,
) -> Option<impl IntoElement + use<>> {
    let (error_place, error) = state.error.as_ref()?;
    (*error_place == place).then(|| {
        div()
            .id(PAGE_SCALE_ERROR_ID)
            .debug_selector(|| PAGE_SCALE_ERROR_ID.into())
            .text_sm()
            .text_color(cx.theme().danger)
            .child(error.clone())
    })
}

fn scale_section(
    entity: &Entity<PageScaleControl>,
    request: &PageScaleRequest,
    cx: &App,
) -> impl IntoElement {
    let state = entity.read(cx);
    let choice = state.choice;
    let calibrate = entity.downgrade();
    let stated = state.stated.clone();
    let measured_line = state.start.zip(state.end).map(|(start, end)| {
        paper_length_label(distance(start, end), imperial_unit(state.display_unit))
    });
    let result = state.ratio(cx).map(|ratio| {
        let (paper_unit, paper_name) = if imperial_unit(state.display_unit) {
            (ScaleUnit::In, "1 in")
        } else {
            (ScaleUnit::Cm, "1 cm")
        };
        let real = ratio * paper_unit.points() / state.display_unit.points();
        (
            ratio,
            format!(
                "{paper_name} on paper is {} {}",
                trimmed(real, 4),
                state.display_unit.as_str()
            ),
        )
    });
    let nearest = result
        .as_ref()
        .filter(|_| matches!(choice, ScaleChoice::Measured | ScaleChoice::Custom))
        .and_then(|(ratio, _)| nearest_standard(*ratio, state.display_unit))
        .filter(|(_, off)| *off > 1e-4);
    let unit = state.display_unit.as_str();
    Field::new().label("Scale").child(
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .id(PAGE_SCALE_SELECT_ID)
                            .debug_selector(|| PAGE_SCALE_SELECT_ID.into())
                            .flex_1()
                            .min_w_0()
                            .child(
                                Select::new(&state.scale_select)
                                    .placeholder("Choose a scale")
                                    .search_placeholder("Search scales"),
                            ),
                    )
                    .child(
                        Button::new(PAGE_SCALE_CALIBRATE_ID)
                            .debug_selector(|| PAGE_SCALE_CALIBRATE_ID.into())
                            .icon(
                                gpui_component::Icon::default()
                                    .path("icons/rail/ruler-dimension-line.svg"),
                            )
                            .label("Calibrate")
                            .outline()
                            .tooltip("Click both ends of a length you know on the page")
                            .on_click(move |_, window, cx| {
                                let _ = calibrate
                                    .update(cx, |control, cx| control.begin_pick(window, cx));
                            }),
                    ),
            )
            .when(!stated.is_empty(), |section| {
                section.child(
                    h_flex()
                        .id(PAGE_SCALE_STATED_ID)
                        .debug_selector(|| PAGE_SCALE_STATED_ID.into())
                        .flex_wrap()
                        .gap_1p5()
                        .items_center()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Stated on this page:"),
                        )
                        .children(stated.into_iter().map(|index| {
                            let choose = entity.downgrade();
                            Button::new(("page-scale-stated", index))
                                .label(STANDARD_SCALES[index].label)
                                .small()
                                .outline()
                                .selected(choice == ScaleChoice::Standard(index))
                                .on_click(move |_, window, cx| {
                                    let _ = choose.update(cx, |control, cx| {
                                        control.choose(ScaleChoice::Standard(index), window, cx)
                                    });
                                })
                        })),
                )
            })
            .when(choice == ScaleChoice::Custom, |section| {
                section.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .id(PAGE_SCALE_CUSTOM_PAPER_ID)
                                .debug_selector(|| PAGE_SCALE_CUSTOM_PAPER_ID.into())
                                .w(px(72.))
                                .child(Input::new(&state.custom_paper)),
                        )
                        .child(
                            div()
                                .id(PAGE_SCALE_CUSTOM_PAPER_UNIT_ID)
                                .debug_selector(|| PAGE_SCALE_CUSTOM_PAPER_UNIT_ID.into())
                                .w(px(80.))
                                .child(Select::new(&state.paper_unit_select)),
                        )
                        .child(div().text_sm().child("on paper ="))
                        .child(
                            div()
                                .id(PAGE_SCALE_CUSTOM_REAL_ID)
                                .debug_selector(|| PAGE_SCALE_CUSTOM_REAL_ID.into())
                                .flex_1()
                                .min_w_0()
                                .child(
                                    Input::new(&state.custom_real).suffix(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(unit),
                                    ),
                                ),
                        ),
                )
            })
            .when_some(
                measured_line.filter(|_| choice == ScaleChoice::Measured),
                |section, line| {
                    let again = entity.downgrade();
                    section.child(
                        v_flex()
                            .gap_1p5()
                            .child(div().text_sm().child(format!(
                                "Your line is {line} long on paper. How long is it really?"
                            )))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(PAGE_SCALE_KNOWN_LENGTH_ID)
                                            .debug_selector(|| PAGE_SCALE_KNOWN_LENGTH_ID.into())
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                Input::new(&state.known_length).suffix(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(unit),
                                                ),
                                            ),
                                    )
                                    .child(
                                        Button::new("page-scale-measure-again")
                                            .label("Measure Again")
                                            .ghost()
                                            .on_click(move |_, window, cx| {
                                                let _ = again.update(cx, |control, cx| {
                                                    control.begin_pick(window, cx)
                                                });
                                            }),
                                    ),
                            ),
                    )
                },
            )
            .when(
                choice == ScaleChoice::None && request.current.is_some(),
                |section| {
                    section.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Measurements made already keep their values."),
                    )
                },
            )
            .when_some(
                result.filter(|_| choice != ScaleChoice::Unset),
                |section, (_, text)| {
                    section.child(
                        h_flex()
                            .id(PAGE_SCALE_RESULT_ID)
                            .debug_selector(|| PAGE_SCALE_RESULT_ID.into())
                            .flex_wrap()
                            .gap_x_1()
                            .items_center()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if nearest.is_some() {
                                format!("{text},")
                            } else {
                                text
                            })
                            .when_some(nearest, |row, (standard, off)| {
                                let index = standard_index(standard);
                                let snap = entity.downgrade();
                                row.child(format!("{:.1}% from {}.", off * 100., standard.label))
                                    .child(
                                        Button::new(PAGE_SCALE_USE_NEAREST_ID)
                                            .debug_selector(|| PAGE_SCALE_USE_NEAREST_ID.into())
                                            .label(format!("Use {}", standard.label))
                                            .small()
                                            .link()
                                            .on_click(move |_, window, cx| {
                                                let _ = snap.update(cx, |control, cx| {
                                                    control.choose(
                                                        ScaleChoice::Standard(index),
                                                        window,
                                                        cx,
                                                    )
                                                });
                                            }),
                                    )
                            }),
                    )
                },
            )
            .children(error_text(state, ErrorPlace::Scale, cx)),
    )
}

fn display_section(entity: &Entity<PageScaleControl>, cx: &App) -> impl IntoElement {
    let state = entity.read(cx);
    Field::new().label("Measure in").child(
        h_flex()
            .gap_2()
            .child(
                div()
                    .id(PAGE_SCALE_UNIT_ID)
                    .debug_selector(|| PAGE_SCALE_UNIT_ID.into())
                    .flex_1()
                    .child(Select::new(&state.unit_select)),
            )
            .child(
                div()
                    .id(PAGE_SCALE_PRECISION_ID)
                    .debug_selector(|| PAGE_SCALE_PRECISION_ID.into())
                    .flex_1()
                    .child(Select::new(&state.precision_select)),
            ),
    )
}

fn pages_section(
    entity: &Entity<PageScaleControl>,
    request: &PageScaleRequest,
    cx: &App,
) -> impl IntoElement {
    let state = entity.read(cx);
    let pages = state.pages;
    let page_button = |id: &'static str, label: String, value: PageScalePages| {
        let set = entity.downgrade();
        Button::new(id)
            .debug_selector(move || id.into())
            .label(label)
            .selected(pages == value)
            .on_click(move |_, window, cx| {
                let _ = set.update(cx, |control, cx| control.set_pages(value, window, cx));
            })
    };
    Field::new().label("Apply to").child(
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        ButtonGroup::new("page-scale-pages")
                            .outline()
                            .child(page_button(
                                PAGE_SCALE_PAGES_THIS_ID,
                                format!("Page {}", request.page_index + 1),
                                PageScalePages::This,
                            ))
                            .child(page_button(
                                PAGE_SCALE_PAGES_ALL_ID,
                                format!("All {} Pages", request.page_count),
                                PageScalePages::All,
                            ))
                            .child(page_button(
                                PAGE_SCALE_PAGES_RANGE_ID,
                                "Some Pages".into(),
                                PageScalePages::Range,
                            )),
                    )
                    .when(pages == PageScalePages::Range, |row| {
                        row.child(
                            div()
                                .id(PAGE_SCALE_RANGE_ID)
                                .debug_selector(|| PAGE_SCALE_RANGE_ID.into())
                                .flex_1()
                                .min_w_0()
                                .child(Input::new(&state.range)),
                        )
                    }),
            )
            .children(error_text(state, ErrorPlace::Pages, cx))
            .children(error_text(state, ErrorPlace::Apply, cx)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::IndexPath;

    #[test]
    fn shift_holds_a_calibration_line_to_its_longer_axis() {
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

    #[test]
    fn imperial_sheets_list_imperial_scales_first() {
        assert!(paper_is_imperial((2592., 1728.)));
        assert!(paper_is_imperial((612., 792.)));
        assert!(!paper_is_imperial((2384., 1684.)));
        assert!(!paper_is_imperial((1000., 700.)));
        let list = scale_list(&[], false, true);
        let titles = (0..4)
            .filter_map(|section| {
                use gpui_component::select::SelectDelegate as _;
                list.item(IndexPath::default().section(section).row(0))
                    .map(|item| item.title.to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(titles[0], "12\" = 1'-0\"");
        assert_eq!(titles[3], "Custom ratio");
    }

    #[test]
    fn precision_lists_fractions_only_for_imperial_units() {
        assert_eq!(precision_choices(ScaleUnit::M).len(), 4);
        assert_eq!(precision_choices(ScaleUnit::Ft).len(), 10);
        assert_eq!(
            precision_title(ScalePrecision::fraction(16).unwrap(), ScaleUnit::In),
            "1/16 in"
        );
        assert_eq!(
            precision_title(ScalePrecision::decimal(0.01).unwrap(), ScaleUnit::M),
            "0.01 m"
        );
    }
}
