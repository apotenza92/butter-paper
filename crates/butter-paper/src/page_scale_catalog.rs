//! Drawing scales: the standard ones, what a page's scale is called, the
//! scales a drawing states in its own text, the paper a page is printed on,
//! and lengths typed the way drawings write them (`36'-6"`).
//!
//! A scale is one number here, its ratio: how many real lengths one paper
//! length stands for (100 for 1:100, 48 for 1/4" = 1'-0").

use crate::annotation_model::{PageScale, ScalePrecision, ScaleSource, ScaleUnit};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleSystem {
    Metric,
    Architectural,
    Engineering,
}

impl ScaleSystem {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Metric => "Metric",
            Self::Architectural => "Architectural",
            Self::Engineering => "Engineering",
        }
    }

    pub const fn imperial(self) -> bool {
        !matches!(self, Self::Metric)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StandardScale {
    pub label: &'static str,
    pub system: ScaleSystem,
    pub ratio: f64,
}

const fn metric(label: &'static str, ratio: f64) -> StandardScale {
    StandardScale {
        label,
        system: ScaleSystem::Metric,
        ratio,
    }
}

const fn architectural(label: &'static str, ratio: f64) -> StandardScale {
    StandardScale {
        label,
        system: ScaleSystem::Architectural,
        ratio,
    }
}

const fn engineering(label: &'static str, ratio: f64) -> StandardScale {
    StandardScale {
        label,
        system: ScaleSystem::Engineering,
        ratio,
    }
}

/// The scales drawings are made at, in the order a list shows them.
pub const STANDARD_SCALES: &[StandardScale] = &[
    metric("1:1", 1.),
    metric("1:2", 2.),
    metric("1:5", 5.),
    metric("1:10", 10.),
    metric("1:20", 20.),
    metric("1:25", 25.),
    metric("1:50", 50.),
    metric("1:100", 100.),
    metric("1:200", 200.),
    metric("1:250", 250.),
    metric("1:500", 500.),
    metric("1:1000", 1000.),
    metric("1:1250", 1250.),
    metric("1:2000", 2000.),
    metric("1:2500", 2500.),
    metric("1:5000", 5000.),
    architectural("12\" = 1'-0\"", 1.),
    architectural("6\" = 1'-0\"", 2.),
    architectural("3\" = 1'-0\"", 4.),
    architectural("1 1/2\" = 1'-0\"", 8.),
    architectural("1\" = 1'-0\"", 12.),
    architectural("3/4\" = 1'-0\"", 16.),
    architectural("1/2\" = 1'-0\"", 24.),
    architectural("3/8\" = 1'-0\"", 32.),
    architectural("1/4\" = 1'-0\"", 48.),
    architectural("3/16\" = 1'-0\"", 64.),
    architectural("1/8\" = 1'-0\"", 96.),
    architectural("3/32\" = 1'-0\"", 128.),
    architectural("1/16\" = 1'-0\"", 192.),
    architectural("1/32\" = 1'-0\"", 384.),
    engineering("1\" = 10'", 120.),
    engineering("1\" = 20'", 240.),
    engineering("1\" = 30'", 360.),
    engineering("1\" = 40'", 480.),
    engineering("1\" = 50'", 600.),
    engineering("1\" = 60'", 720.),
    engineering("1\" = 100'", 1200.),
    engineering("1\" = 200'", 2400.),
];

impl StandardScale {
    /// This scale for a page, measuring in `unit` to `precision`.
    pub fn page_scale(
        &self,
        page_index: u32,
        unit: ScaleUnit,
        precision: ScalePrecision,
    ) -> PageScale {
        scale_from_ratio(
            page_index,
            ScaleSource::Preset,
            self.label,
            self.ratio,
            unit,
            precision,
        )
    }

    /// The unit and precision measurements at this scale read in by
    /// default: metres to the centimetre on plans, millimetres on details,
    /// decimal feet on imperial drawings.
    pub fn default_display(&self) -> (ScaleUnit, ScalePrecision) {
        default_display(self.system, self.ratio)
    }
}

pub fn default_display(system: ScaleSystem, ratio: f64) -> (ScaleUnit, ScalePrecision) {
    let decimal = |value| ScalePrecision::decimal(value).expect("a positive precision");
    match system {
        ScaleSystem::Metric if ratio < 50. => (ScaleUnit::Mm, decimal(1.)),
        ScaleSystem::Metric => (ScaleUnit::M, decimal(0.01)),
        ScaleSystem::Architectural => (ScaleUnit::Ft, decimal(0.01)),
        ScaleSystem::Engineering => (ScaleUnit::Ft, decimal(0.1)),
    }
}

/// A page scale of `ratio` real lengths per paper length, measuring in
/// `unit`.
pub fn scale_from_ratio(
    page_index: u32,
    source: ScaleSource,
    name: &str,
    ratio: f64,
    unit: ScaleUnit,
    precision: ScalePrecision,
) -> PageScale {
    let per_point = ratio / unit.points();
    PageScale {
        page_index,
        source,
        name: name.to_owned(),
        pdf_units: if imperial_unit(unit) {
            ScaleUnit::In
        } else {
            ScaleUnit::Cm
        },
        real_units: unit,
        scale_x: per_point,
        scale_y: per_point,
        precision,
    }
}

pub const fn imperial_unit(unit: ScaleUnit) -> bool {
    matches!(unit, ScaleUnit::In | ScaleUnit::Ft)
}

/// How many real lengths one paper length stands for on the page.
pub fn ratio_of(scale: &PageScale) -> f64 {
    scale.scale_x * scale.real_units.points()
}

/// The same scale, measuring in `unit` instead.
pub fn with_unit(scale: &PageScale, unit: ScaleUnit) -> PageScale {
    let factor = unit.points().recip() * scale.real_units.points();
    PageScale {
        real_units: unit,
        pdf_units: if imperial_unit(unit) {
            ScaleUnit::In
        } else {
            ScaleUnit::Cm
        },
        scale_x: scale.scale_x * factor,
        scale_y: scale.scale_y * factor,
        ..scale.clone()
    }
}

fn same_ratio(left: f64, right: f64) -> bool {
    ((left - right) / right).abs() < 1e-4
}

/// The standard scale with this ratio, preferring the measuring system of
/// `unit` (1:1 is both 1:1 and 12" = 1'-0").
pub fn standard_for(ratio: f64, unit: ScaleUnit) -> Option<&'static StandardScale> {
    let imperial = imperial_unit(unit);
    let matching = || {
        STANDARD_SCALES
            .iter()
            .filter(move |scale| same_ratio(scale.ratio, ratio))
    };
    matching()
        .find(|scale| scale.system.imperial() == imperial)
        .or_else(|| matching().next())
}

/// What a page's scale is called: a standard scale's own name, otherwise
/// its ratio (`1:99.6`, or `1" = 4.012'` on imperial drawings).
pub fn scale_label(scale: &PageScale) -> String {
    let ratio = ratio_of(scale);
    let horizontal = ratio_label(ratio, scale.real_units);
    if same_ratio(scale.scale_y, scale.scale_x) {
        return horizontal;
    }
    let vertical = ratio_label(scale.scale_y * scale.real_units.points(), scale.real_units);
    format!("{horizontal} across, {vertical} up")
}

pub fn ratio_label(ratio: f64, unit: ScaleUnit) -> String {
    if let Some(standard) = standard_for(ratio, unit) {
        return standard.label.to_owned();
    }
    if imperial_unit(unit) {
        format!("1\" = {}'", trimmed(ratio / 12., 3))
    } else {
        format!("1:{}", trimmed(ratio, 2))
    }
}

/// The standard scale nearest `ratio`, when within 3%: a calibrated line
/// is rarely clicked exactly, and a drawing is almost always at a standard
/// scale. Returns the scale and how far off the ratio is (0.004 = 0.4%).
pub fn nearest_standard(ratio: f64, unit: ScaleUnit) -> Option<(&'static StandardScale, f64)> {
    let imperial = imperial_unit(unit);
    STANDARD_SCALES
        .iter()
        .filter(|scale| scale.system.imperial() == imperial)
        .map(|scale| (scale, (ratio - scale.ratio).abs() / scale.ratio))
        .filter(|(_, off)| *off <= 0.03)
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

/// `value` with at most `decimals` decimals and no trailing zeros.
pub fn trimmed(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// A scale a page states in its own text.
#[derive(Clone, Debug, PartialEq)]
pub struct StatedScale {
    pub scale: &'static StandardScale,
    /// How often the page states it.
    pub count: usize,
}

static ARCHITECTURAL_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(\d+\s+\d+/\d+|\d+/\d+|\d+(?:\.\d+)?)\s*"\s*=\s*1\s*'\s*(?:-?\s*0\s*")?"#)
        .expect("a valid expression")
});
static ENGINEERING_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[^\d/.])1\s*"\s*=\s*(\d+)\s*'(?:\s*-?\s*0\s*")?"#)
        .expect("a valid expression")
});
static METRIC_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(scale\s*:?\s*)?\b1\s*:\s*(\d+(?:\.\d+)?)(?:\s*(?:@|at)\s*(A[0-4]|B[0-4]))?")
        .expect("a valid expression")
});

/// The standard scales a page's text states, most often stated first:
/// `1/4" = 1'-0"`, `1" = 20'`, and metric `1:100` when the drawing ties
/// it to a sheet (`1:100 @ A1`) or labels it (`SCALE 1:100`). A bare
/// `1:8` is more often a ramp's gradient than a scale. A scale stated for
/// another sheet size is converted to this page's: a drawing at
/// `1:100 @ A1` printed on A3 is at 1:200.
pub fn stated_scales(text: &str, page_size: (f64, f64)) -> Vec<StatedScale> {
    let text = text
        .replace(['\u{2019}', '\u{2032}', '\u{2018}'], "'")
        .replace(['\u{201D}', '\u{2033}', '\u{201C}'], "\"");
    let mut found: Vec<StatedScale> = Vec::new();
    let mut add = |ratio: f64, imperial: bool| {
        let Some(scale) = STANDARD_SCALES.iter().find(|scale| {
            scale.system.imperial() == imperial && (ratio - scale.ratio).abs() / scale.ratio < 0.01
        }) else {
            return;
        };
        match found.iter_mut().find(|stated| stated.scale == scale) {
            Some(stated) => stated.count += 1,
            None => found.push(StatedScale { scale, count: 1 }),
        }
    };
    for capture in ARCHITECTURAL_TEXT.captures_iter(&text) {
        if let Some(inches) = parse_inches(&capture[1])
            && inches > 0.
        {
            add(12. / inches, true);
        }
    }
    for capture in ENGINEERING_TEXT.captures_iter(&text) {
        if let Ok(feet) = capture[1].parse::<f64>()
            && feet > 1.
        {
            add(feet * 12., true);
        }
    }
    let page_long = page_size.0.max(page_size.1);
    for capture in METRIC_TEXT.captures_iter(&text) {
        let Ok(ratio) = capture[2].parse::<f64>() else {
            continue;
        };
        match capture.get(3) {
            Some(sheet) => {
                let Some(sheet_long) = iso_long_side_points(sheet.as_str()) else {
                    continue;
                };
                add(ratio * sheet_long / page_long, false);
            }
            None if capture.get(1).is_some() => add(ratio, false),
            None => {}
        }
    }
    found.sort_by_key(|stated| std::cmp::Reverse(stated.count));
    found
}

fn parse_inches(text: &str) -> Option<f64> {
    let mut total = 0.;
    for part in text.split_whitespace() {
        total += match part.split_once('/') {
            Some((numerator, denominator)) => {
                let denominator = denominator.parse::<f64>().ok()?;
                if denominator == 0. {
                    return None;
                }
                numerator.parse::<f64>().ok()? / denominator
            }
            None => part.parse::<f64>().ok()?,
        };
    }
    Some(total)
}

const POINTS_PER_MM: f64 = 72. / 25.4;

fn iso_long_side_points(name: &str) -> Option<f64> {
    PAPER_SIZES
        .iter()
        .find(|paper| paper.name.eq_ignore_ascii_case(name))
        .map(|paper| paper.long * paper.unit_points())
}

struct PaperSize {
    name: &'static str,
    short: f64,
    long: f64,
    metric: bool,
}

impl PaperSize {
    fn unit_points(&self) -> f64 {
        if self.metric { POINTS_PER_MM } else { 72. }
    }
}

const fn iso(name: &'static str, short: f64, long: f64) -> PaperSize {
    PaperSize {
        name,
        short,
        long,
        metric: true,
    }
}

const fn inches(name: &'static str, short: f64, long: f64) -> PaperSize {
    PaperSize {
        name,
        short,
        long,
        metric: false,
    }
}

const PAPER_SIZES: &[PaperSize] = &[
    iso("A0", 841., 1189.),
    iso("A1", 594., 841.),
    iso("A2", 420., 594.),
    iso("A3", 297., 420.),
    iso("A4", 210., 297.),
    iso("A5", 148., 210.),
    iso("B0", 1000., 1414.),
    iso("B1", 707., 1000.),
    iso("B2", 500., 707.),
    iso("B3", 353., 500.),
    iso("B4", 250., 353.),
    inches("Letter", 8.5, 11.),
    inches("Legal", 8.5, 14.),
    inches("Tabloid", 11., 17.),
    inches("ARCH A", 9., 12.),
    inches("ARCH B", 12., 18.),
    inches("ARCH C", 18., 24.),
    inches("ARCH D", 24., 36.),
    inches("ARCH E1", 30., 42.),
    inches("ARCH E", 36., 48.),
    inches("ANSI C", 17., 22.),
    inches("ANSI D", 22., 34.),
    inches("ANSI E", 34., 44.),
];

/// The standard sheet a page of `width_mm` × `height_mm` is cut to, in either
/// orientation (`A3` for 420 × 297 mm), with its place in the catalogue so
/// lists of sizes keep the catalogue's order. A size within a quarter of a
/// millimetre of a sheet is that sheet; anything else is no standard sheet.
pub fn paper_size_mm(width_mm: f64, height_mm: f64) -> Option<(usize, &'static str)> {
    let (short, long) = (width_mm.min(height_mm), width_mm.max(height_mm));
    PAPER_SIZES
        .iter()
        .enumerate()
        .find(|(_, paper)| {
            let unit = paper.unit_points() / POINTS_PER_MM;
            (paper.short * unit - short).abs() <= 0.25 && (paper.long * unit - long).abs() <= 0.25
        })
        .map(|(index, paper)| (index, paper.name))
}

/// The paper a page is, as its width and height read: `A1, 841 × 594 mm`,
/// `ARCH D, 36 × 24 in`, or just the size when it is no standard sheet.
pub fn paper_label(width_points: f64, height_points: f64) -> String {
    let (short, long) = (
        width_points.min(height_points),
        width_points.max(height_points),
    );
    let named = PAPER_SIZES.iter().find(|paper| {
        let unit = paper.unit_points();
        (paper.short * unit - short).abs() <= 3. && (paper.long * unit - long).abs() <= 3.
    });
    let size = |metric: bool| {
        if metric {
            format!(
                "{} × {} mm",
                trimmed(width_points / POINTS_PER_MM, 0),
                trimmed(height_points / POINTS_PER_MM, 0)
            )
        } else {
            format!(
                "{} × {} in",
                trimmed(width_points / 72., 2),
                trimmed(height_points / 72., 2)
            )
        }
    };
    match named {
        Some(paper) => format!("{}, {}", paper.name, size(paper.metric)),
        None => size(false),
    }
}

/// A length typed in `unit`. Feet and inches may be written as drawings
/// write them (`36'-6"`, `36' 6 1/2"`, `6"`), and any length may name its
/// own unit (`3600 mm` while measuring in metres).
pub fn parse_length(text: &str, unit: ScaleUnit) -> Result<f64, String> {
    let text = text
        .trim()
        .replace(['\u{2019}', '\u{2032}'], "'")
        .replace(['\u{201D}', '\u{2033}'], "\"");
    let invalid = || "Enter a length, like 12.5 or 36'-6\".".to_owned();
    if text.is_empty() {
        return Err("Enter the length of the line.".into());
    }
    let value_in = |value: f64, from: ScaleUnit| value * from.points() / unit.points();
    let length = if text.contains('\'') || text.contains('"') {
        let (feet, rest) = match text.split_once('\'') {
            Some((feet, rest)) => (parse_inches(feet.trim()).ok_or_else(invalid)?, rest),
            None => (0., text.as_str()),
        };
        let rest = rest.trim().trim_start_matches('-').trim();
        let inches = match rest.strip_suffix('"') {
            Some(inches) => parse_inches(inches.trim()).ok_or_else(invalid)?,
            None if rest.is_empty() => 0.,
            None => parse_inches(rest).ok_or_else(invalid)?,
        };
        value_in(feet * 12. + inches, ScaleUnit::In)
    } else {
        let split = text
            .find(|character: char| character.is_ascii_alphabetic())
            .unwrap_or(text.len());
        let (number, suffix) = text.split_at(split);
        let value = parse_inches(number.trim()).ok_or_else(invalid)?;
        match suffix.trim() {
            "" => value,
            suffix => value_in(value, ScaleUnit::parse(suffix).map_err(|_| invalid())?),
        }
    };
    if !length.is_finite() || length <= 0. {
        return Err("The length must be more than zero.".into());
    }
    Ok(length)
}

/// A paper length (in points) as a person reads it off the sheet.
pub fn paper_length_label(points: f64, imperial: bool) -> String {
    if imperial {
        format!("{} in", trimmed(points / 72., 2))
    } else {
        format!("{} mm", trimmed(points / POINTS_PER_MM, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(value: f64) -> ScalePrecision {
        ScalePrecision::decimal(value).unwrap()
    }

    #[test]
    fn standard_scales_measure_what_their_names_say() {
        let plan =
            standard_for(100., ScaleUnit::M)
                .unwrap()
                .page_scale(0, ScaleUnit::M, decimal(0.01));
        // 1 cm on paper is 1 m.
        assert!((plan.scale_x * ScaleUnit::Cm.points() - 1.).abs() < 1e-12);
        let quarter =
            standard_for(48., ScaleUnit::Ft)
                .unwrap()
                .page_scale(0, ScaleUnit::Ft, decimal(0.01));
        // A quarter inch on paper is one foot.
        assert!((quarter.scale_x * 18. - 1.).abs() < 1e-12);
        assert_eq!(scale_label(&quarter), "1/4\" = 1'-0\"");
        assert_eq!(scale_label(&with_unit(&plan, ScaleUnit::Mm)), "1:100");
        assert!((ratio_of(&with_unit(&plan, ScaleUnit::Mm)) - 100.).abs() < 1e-9);
    }

    #[test]
    fn a_scale_is_named_for_its_measuring_system() {
        let full_size = |unit| scale_from_ratio(0, ScaleSource::Custom, "x", 1., unit, decimal(1.));
        assert_eq!(scale_label(&full_size(ScaleUnit::Mm)), "1:1");
        assert_eq!(scale_label(&full_size(ScaleUnit::In)), "12\" = 1'-0\"");
        let odd = |unit| scale_from_ratio(0, ScaleSource::Custom, "x", 99.6, unit, decimal(1.));
        assert_eq!(scale_label(&odd(ScaleUnit::M)), "1:99.6");
        assert_eq!(scale_label(&odd(ScaleUnit::Ft)), "1\" = 8.3'");
    }

    #[test]
    fn a_calibrated_line_suggests_the_standard_scale_it_is_close_to() {
        let (scale, off) = nearest_standard(99.6, ScaleUnit::M).unwrap();
        assert_eq!(scale.label, "1:100");
        assert!((off - 0.004).abs() < 1e-9);
        assert_eq!(
            nearest_standard(47.5, ScaleUnit::Ft).unwrap().0.label,
            "1/4\" = 1'-0\""
        );
        assert!(nearest_standard(150., ScaleUnit::M).is_none());
    }

    #[test]
    fn scales_a_page_states_are_found_but_gradients_and_pitches_are_not() {
        let arch_d = (2592., 1728.);
        let found = stated_scales(
            "SECOND FLOOR PLAN 1/4\" = 1'-0\" FIRST FLOOR PLAN 1/4\" = 1'-0\" \
             ROOF 6\" / 1'-0\" DETAIL 1 1/2\" = 1'-0\" SITE 1\" = 20'",
            arch_d,
        );
        let labels = found
            .iter()
            .map(|stated| (stated.scale.label, stated.count))
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            [
                ("1/4\" = 1'-0\"", 2),
                ("1 1/2\" = 1'-0\"", 1),
                ("1\" = 20'", 1)
            ]
        );
        assert!(stated_scales("RAMP FALL 1:80 1:4 1:8", (2384., 1684.)).is_empty());
        assert_eq!(
            stated_scales("SCALE: 1:50", (842., 595.))[0].scale.label,
            "1:50"
        );
        assert!(
            stated_scales("1/8\u{201D} = 1\u{2019}-0\u{201D}", arch_d)[0]
                .scale
                .label
                == "1/8\" = 1'-0\""
        );
    }

    #[test]
    fn a_scale_for_another_sheet_size_is_converted_to_the_page() {
        let a1 = (2384., 1684.);
        let found = stated_scales("1:200 @ A3 GRAPHIC SCALE 1:100 @ A1", a1);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].scale.label, "1:100");
        assert_eq!(found[0].count, 2);
        let a3 = (1191., 842.);
        assert_eq!(stated_scales("1:100 @ A1", a3)[0].scale.label, "1:200");
    }

    #[test]
    fn pages_are_named_for_their_paper() {
        assert_eq!(paper_label(2383.94, 1683.78), "A1, 841 × 594 mm");
        assert_eq!(paper_label(2592., 1728.), "ARCH D, 36 × 24 in");
        assert_eq!(paper_label(612., 792.), "Letter, 8.5 × 11 in");
        assert_eq!(paper_label(1000., 700.), "13.89 × 9.72 in");
    }

    #[test]
    fn millimetre_sizes_are_named_for_their_sheet_in_either_orientation() {
        let name = |width, height| paper_size_mm(width, height).map(|(_, name)| name);
        assert_eq!(name(420., 297.), Some("A3"));
        assert_eq!(name(297., 420.), Some("A3"));
        assert_eq!(name(210., 297.), Some("A4"));
        assert_eq!(name(297., 210.), Some("A4"));
        assert_eq!(name(148., 210.), Some("A5"));
        assert_eq!(name(215.9, 279.4), Some("Letter"));
        assert_eq!(name(210., 297.5), None);
        assert_eq!(name(500., 300.), None);
        let order = |width, height| paper_size_mm(width, height).unwrap().0;
        assert!(order(420., 297.) < order(210., 297.), "A3 lists before A4");
    }

    #[test]
    fn lengths_are_read_as_drawings_write_them() {
        let feet = |text| parse_length(text, ScaleUnit::Ft).unwrap();
        assert_eq!(feet("36"), 36.);
        assert_eq!(feet("36'"), 36.);
        assert_eq!(feet("36'-6\""), 36.5);
        assert_eq!(feet("36' 6 1/2\""), 36. + 6.5 / 12.);
        assert_eq!(feet("6\""), 0.5);
        assert!((feet("1 1/2") - 1.5).abs() < 1e-12);
        assert!((parse_length("3600 mm", ScaleUnit::M).unwrap() - 3.6).abs() < 1e-12);
        assert!((parse_length("12.5", ScaleUnit::M).unwrap() - 12.5).abs() < 1e-12);
        assert!(parse_length("", ScaleUnit::M).is_err());
        assert!(parse_length("0", ScaleUnit::M).is_err());
        assert!(parse_length("abc", ScaleUnit::M).is_err());
        assert!(parse_length("3 parsecs", ScaleUnit::M).is_err());
    }
}
