//! Sizes typed while drawing on a page with a scale, AutoCAD-style: a line
//! takes a length and an angle, a box its width and height. Tab moves
//! between the two, Enter places the markup. A typed size holds the pointer
//! to it (`TypedSize`); what it does not fix still follows the pointer.

use crate::annotation_adapter::{TypedSize, TypedSizeAnchor, TypedSizeShape};
use crate::annotation_model::{PageScale, PdfPoint, group_measurement_thousands};
use crate::page_scale_catalog::{parse_length, scale_label};

/// How the page lies on screen: the screen angle of the page's +x axis,
/// anticlockwise in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenFrame {
    pub x_axis_angle: f64,
}

impl ScreenFrame {
    pub const UPRIGHT: Self = Self { x_axis_angle: 0. };

    /// Whether the page's x axis runs across the screen rather than up it.
    fn x_runs_across(self) -> bool {
        self.x_axis_angle.cos().abs() >= self.x_axis_angle.sin().abs()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SizeEntry {
    pub document_id: u64,
    pub anchor: TypedSizeAnchor,
    fields: [String; 2],
    active: usize,
}

/// One field as the box beside the pointer shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SizeEntryField {
    pub label: &'static str,
    /// What was typed, or the size the pointer gives when nothing was.
    pub text: String,
    pub typed: bool,
    /// Typed but not a size yet (`3.`, `36'-`), or not one at all.
    pub incomplete: bool,
    pub active: bool,
}

impl SizeEntry {
    pub fn new(document_id: u64, anchor: TypedSizeAnchor) -> Self {
        Self {
            document_id,
            anchor,
            fields: [String::new(), String::new()],
            active: 0,
        }
    }

    /// Whether this entry still belongs to the markup being drawn: a path's
    /// next vertex starts a new segment with nothing typed.
    pub fn is_for(&self, document_id: u64, anchor: &TypedSizeAnchor) -> bool {
        self.document_id == document_id
            && self.anchor.page_index == anchor.page_index
            && self.anchor.anchor == anchor.anchor
            && self.anchor.shape == anchor.shape
    }

    pub fn has_text(&self) -> bool {
        self.fields.iter().any(|field| !field.is_empty())
    }

    /// Adds a typed character to the active field. Returns whether it was
    /// one a size can contain.
    pub fn type_char(&mut self, character: char) -> bool {
        let allowed = character.is_ascii_alphanumeric()
            || matches!(character, '.' | ',' | '\'' | '"' | '-' | '/' | ' ' | '°');
        if !allowed || self.fields[self.active].len() >= 24 {
            return false;
        }
        if character == ' ' && self.fields[self.active].is_empty() {
            return false;
        }
        self.fields[self.active].push(if character == ',' { '.' } else { character });
        true
    }

    /// Removes the active field's last character. Returns whether there was
    /// one.
    pub fn backspace(&mut self) -> bool {
        self.fields[self.active].pop().is_some()
    }

    pub fn next_field(&mut self) {
        self.active = (self.active + 1) % self.fields.len();
    }

    pub fn previous_field(&mut self) {
        self.active = (self.active + self.fields.len() - 1) % self.fields.len();
    }

    /// The sizes typed, in the page's frame, for the drawing code.
    pub fn typed_size(&self, scale: &PageScale, frame: ScreenFrame) -> TypedSize {
        let length = |text: &str| {
            parse_length(text, scale.real_units)
                .ok()
                .map(|real| real / scale.scale_x)
        };
        match self.anchor.shape {
            TypedSizeShape::Line => TypedSize {
                length: length(&self.fields[0]),
                angle: parse_angle(&self.fields[1])
                    .map(|degrees| degrees.to_radians() - frame.x_axis_angle),
                ..TypedSize::default()
            },
            TypedSizeShape::Box => {
                let real = |text: &str, scale_factor: f64| {
                    parse_length(text, scale.real_units)
                        .ok()
                        .map(|real| real / scale_factor)
                };
                let (across, up) = (&self.fields[0], &self.fields[1]);
                if frame.x_runs_across() {
                    TypedSize {
                        extent_x: real(across, scale.scale_x),
                        extent_y: real(up, scale.scale_y),
                        ..TypedSize::default()
                    }
                } else {
                    TypedSize {
                        extent_x: real(up, scale.scale_x),
                        extent_y: real(across, scale.scale_y),
                        ..TypedSize::default()
                    }
                }
            }
        }
    }

    /// The two fields, showing what was typed or what the pointer measures.
    pub fn fields(
        &self,
        current: PdfPoint,
        scale: &PageScale,
        frame: ScreenFrame,
    ) -> [SizeEntryField; 2] {
        let measured = measured_texts(self.anchor, current, scale, frame);
        let labels = field_labels(self.anchor.shape);
        let typed = self.typed_size(scale, frame);
        let complete = match self.anchor.shape {
            TypedSizeShape::Line => [typed.length.is_some(), typed.angle.is_some()],
            TypedSizeShape::Box => {
                if frame.x_runs_across() {
                    [typed.extent_x.is_some(), typed.extent_y.is_some()]
                } else {
                    [typed.extent_y.is_some(), typed.extent_x.is_some()]
                }
            }
        };
        std::array::from_fn(|index| {
            let text = &self.fields[index];
            SizeEntryField {
                label: labels[index],
                text: if text.is_empty() {
                    measured[index].clone()
                } else if complete[index] && !names_its_unit(text) {
                    // A bare number is in the page's unit, or degrees.
                    let is_angle = self.anchor.shape == TypedSizeShape::Line && index == 1;
                    if is_angle {
                        format!("{text}°")
                    } else {
                        format!("{text} {}", scale.real_units.as_str())
                    }
                } else {
                    text.clone()
                },
                typed: !text.is_empty(),
                incomplete: !text.is_empty() && !complete[index],
                active: index == self.active,
            }
        })
    }
}

fn names_its_unit(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_ascii_alphabetic() || matches!(character, '\'' | '"' | '°'))
}

fn field_labels(shape: TypedSizeShape) -> [&'static str; 2] {
    match shape {
        TypedSizeShape::Line => ["Length", "Angle"],
        TypedSizeShape::Box => ["Width", "Height"],
    }
}

/// What the pointer measures from the anchor, as the page's scale writes it.
pub fn measured_texts(
    anchor: TypedSizeAnchor,
    current: PdfPoint,
    scale: &PageScale,
    frame: ScreenFrame,
) -> [String; 2] {
    let dx = current.x - anchor.anchor.x;
    let dy = current.y - anchor.anchor.y;
    let length = |real: f64| {
        format!(
            "{} {}",
            group_measurement_thousands(&scale.precision.format(real)),
            scale.real_units.as_str()
        )
    };
    match anchor.shape {
        TypedSizeShape::Line => {
            let real = (dx * scale.scale_x).hypot(dy * scale.scale_y);
            let degrees = if dx == 0. && dy == 0. {
                0.
            } else {
                (dy.atan2(dx) + frame.x_axis_angle)
                    .to_degrees()
                    .rem_euclid(360.)
            };
            [length(real), format_angle(degrees)]
        }
        TypedSizeShape::Box => {
            let (x, y) = ((dx * scale.scale_x).abs(), (dy * scale.scale_y).abs());
            if frame.x_runs_across() {
                [length(x), length(y)]
            } else {
                [length(y), length(x)]
            }
        }
    }
}

/// The page scale as the box beside the pointer names it.
pub fn scale_text(scale: &PageScale) -> String {
    scale_label(scale)
}

fn format_angle(degrees: f64) -> String {
    let rounded = (degrees * 10.).round() / 10.;
    let rounded = if rounded >= 360. { 0. } else { rounded };
    let text = format!("{rounded:.1}");
    format!("{}°", text.trim_end_matches('0').trim_end_matches('.'))
}

/// Degrees, anticlockwise from the right of the screen.
fn parse_angle(text: &str) -> Option<f64> {
    let text = text.trim().trim_end_matches('°').trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_model::{ScalePrecision, ScaleUnit};
    use crate::page_scale_catalog::standard_for;

    /// 1:100 in metres: one paper centimetre is one metre.
    fn metric_scale() -> PageScale {
        standard_for(100., ScaleUnit::M).unwrap().page_scale(
            0,
            ScaleUnit::M,
            ScalePrecision::decimal(0.001).unwrap(),
        )
    }

    fn anchor(shape: TypedSizeShape) -> TypedSizeAnchor {
        TypedSizeAnchor {
            page_index: 0,
            anchor: PdfPoint { x: 100., y: 100. },
            current: PdfPoint { x: 100., y: 100. },
            shape,
        }
    }

    fn typed(entry: &mut SizeEntry, text: &str) {
        for character in text.chars() {
            assert!(entry.type_char(character), "{character} is typeable");
        }
    }

    const POINTS_PER_CM: f64 = 72. / 2.54;

    #[test]
    fn a_typed_length_is_exact_at_the_page_scale_and_keeps_the_pointer_direction() {
        let scale = metric_scale();
        let mut entry = SizeEntry::new(1, anchor(TypedSizeShape::Line));
        typed(&mut entry, "3.2");
        let size = entry.typed_size(&scale, ScreenFrame::UPRIGHT);
        // 3.2 m at 1:100 is 3.2 cm of paper.
        assert!((size.length.unwrap() - 3.2 * POINTS_PER_CM).abs() < 1e-9);
        assert_eq!(size.angle, None);
        let end = size.resolve(
            PdfPoint { x: 100., y: 100. },
            PdfPoint { x: 100., y: 400. },
            TypedSizeShape::Line,
        );
        assert!((end.x - 100.).abs() < 1e-9);
        assert!((end.y - (100. + 3.2 * POINTS_PER_CM)).abs() < 1e-9);
    }

    #[test]
    fn a_length_may_name_its_own_unit_and_tab_moves_to_the_angle() {
        let scale = metric_scale();
        let mut entry = SizeEntry::new(1, anchor(TypedSizeShape::Line));
        typed(&mut entry, "3200mm");
        entry.next_field();
        typed(&mut entry, "90");
        let size = entry.typed_size(&scale, ScreenFrame::UPRIGHT);
        assert!((size.length.unwrap() - 3.2 * POINTS_PER_CM).abs() < 1e-9);
        assert!((size.angle.unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        // The pointer no longer decides anything.
        let end = size.resolve(
            PdfPoint { x: 100., y: 100. },
            PdfPoint { x: 500., y: -20. },
            TypedSizeShape::Line,
        );
        assert!((end.x - 100.).abs() < 1e-9);
        assert!((end.y - (100. + 3.2 * POINTS_PER_CM)).abs() < 1e-9);
    }

    #[test]
    fn a_box_takes_its_typed_width_on_the_pointers_side_and_follows_it_for_the_height() {
        let scale = metric_scale();
        let mut entry = SizeEntry::new(1, anchor(TypedSizeShape::Box));
        typed(&mut entry, "2");
        let size = entry.typed_size(&scale, ScreenFrame::UPRIGHT);
        let corner = size.resolve(
            PdfPoint { x: 100., y: 100. },
            PdfPoint { x: 40., y: 130. },
            TypedSizeShape::Box,
        );
        assert!((corner.x - (100. - 2. * POINTS_PER_CM)).abs() < 1e-9);
        assert_eq!(corner.y, 130.);
    }

    #[test]
    fn a_page_turned_a_quarter_swaps_width_and_height() {
        let scale = metric_scale();
        let mut entry = SizeEntry::new(1, anchor(TypedSizeShape::Box));
        typed(&mut entry, "2");
        entry.next_field();
        typed(&mut entry, "1");
        let turned = ScreenFrame {
            x_axis_angle: -std::f64::consts::FRAC_PI_2,
        };
        let size = entry.typed_size(&scale, turned);
        assert!((size.extent_y.unwrap() - 2. * POINTS_PER_CM).abs() < 1e-9);
        assert!((size.extent_x.unwrap() - POINTS_PER_CM).abs() < 1e-9);
    }

    #[test]
    fn untyped_fields_show_what_the_pointer_measures_and_partial_text_is_incomplete() {
        let scale = metric_scale();
        let mut entry = SizeEntry::new(1, anchor(TypedSizeShape::Line));
        let current = PdfPoint {
            x: 100. + 3. * POINTS_PER_CM,
            y: 100. + 3. * POINTS_PER_CM,
        };
        let [length, angle] = entry.fields(current, &scale, ScreenFrame::UPRIGHT);
        assert_eq!(length.text, "4.243 m");
        assert!(!length.typed && length.active);
        assert_eq!(angle.text, "45°");
        typed(&mut entry, "3.2k");
        let [length, _] = entry.fields(current, &scale, ScreenFrame::UPRIGHT);
        assert!(length.typed && length.incomplete);
        assert!(entry.backspace());
        let [length, _] = entry.fields(current, &scale, ScreenFrame::UPRIGHT);
        assert!(!length.incomplete, "3.2 is a length");
        assert_eq!(length.text, "3.2 m");
        entry.next_field();
        typed(&mut entry, "30");
        let [_, angle] = entry.fields(current, &scale, ScreenFrame::UPRIGHT);
        assert_eq!(angle.text, "30°");
    }

    #[test]
    fn keys_a_size_cannot_contain_are_refused() {
        let mut entry = SizeEntry::new(1, anchor(TypedSizeShape::Line));
        assert!(!entry.type_char(' '));
        assert!(!entry.type_char('+'));
        assert!(!entry.type_char('\t'));
        assert!(!entry.has_text());
        assert!(entry.type_char('7'));
        assert!(entry.type_char(','));
        assert!(entry.has_text());
    }
}
