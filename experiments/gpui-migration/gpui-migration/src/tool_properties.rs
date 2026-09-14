//! Session-owned defaults for annotation tools.
//!
//! This model mirrors only properties that the native annotation model can
//! apply when an annotation is created. It deliberately contains no GPUI
//! state and does not claim support for reference controls that have no native
//! persistence or rendering path.

use crate::annotation_adapter::AnnotationTool;
use crate::annotation_model::AnnotationError;

pub const TOOL_FONT_FAMILY_OPTIONS: &[ToolFontFamilyOption] = &[
    ToolFontFamilyOption {
        value: "Arimo",
        label: "Arial",
    },
    ToolFontFamilyOption {
        value: "Roboto Mono",
        label: "Courier New",
    },
    ToolFontFamilyOption {
        value: "Helvetica",
        label: "Helvetica",
    },
    ToolFontFamilyOption {
        value: "Tinos",
        label: "Times New Roman",
    },
];

const NO_FIELDS: &[ToolPropertyField] = &[];
const SHAPE_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::WidthPt,
    ToolPropertyField::FillColour,
    ToolPropertyField::Opacity,
];
const LINE_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::WidthPt,
    ToolPropertyField::Opacity,
];
const PEN_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::WidthPt,
    ToolPropertyField::Opacity,
    ToolPropertyField::SmoothCurves,
];
const HIGHLIGHT_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::WidthPt,
    ToolPropertyField::Opacity,
];
const CLOUD_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::WidthPt,
    ToolPropertyField::Opacity,
    ToolPropertyField::CloudIntensity,
];
const CLOUD_PLUS_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::FontSizePt,
    ToolPropertyField::Opacity,
    ToolPropertyField::CloudIntensity,
];
const CALLOUT_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::FontSizePt,
    ToolPropertyField::Opacity,
];
const DIMENSION_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::WidthPt,
    ToolPropertyField::FontSizePt,
    ToolPropertyField::Opacity,
];
const TEXT_BOX_FIELDS: &[ToolPropertyField] = &[
    ToolPropertyField::Colour,
    ToolPropertyField::FontSizePt,
    ToolPropertyField::FontFamily,
    ToolPropertyField::Opacity,
];
const OPACITY_FIELDS: &[ToolPropertyField] = &[ToolPropertyField::Opacity];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ToolPropertyField {
    Colour,
    WidthPt,
    FillColour,
    Opacity,
    FontSizePt,
    FontFamily,
    SmoothCurves,
    CloudIntensity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolPropertyRange {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolFontFamilyOption {
    pub value: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolProperties {
    pub colour: String,
    pub width_pt: f64,
    pub fill_colour: Option<String>,
    pub fill_opacity: f64,
    pub opacity: f64,
    pub font_size_pt: f64,
    pub font_family: String,
    pub smooth_curves: bool,
    pub cloud_intensity: f64,
}

impl ToolProperties {
    pub fn for_tool(tool: AnnotationTool) -> Self {
        Self {
            colour: match tool {
                AnnotationTool::Highlight => "#ffff00",
                _ => "#ff0000",
            }
            .into(),
            width_pt: match tool {
                AnnotationTool::Arrow => 0.5,
                AnnotationTool::Highlight => 12.0,
                _ => 1.0,
            },
            fill_colour: None,
            fill_opacity: 1.0,
            opacity: 1.0,
            font_size_pt: 12.0,
            font_family: "Helvetica".into(),
            smooth_curves: true,
            cloud_intensity: 2.0,
        }
    }

    pub fn applicable_fields(tool: AnnotationTool) -> &'static [ToolPropertyField] {
        match tool {
            AnnotationTool::Rectangle | AnnotationTool::Ellipse | AnnotationTool::Polygon => {
                SHAPE_FIELDS
            }
            AnnotationTool::Arc
            | AnnotationTool::Line
            | AnnotationTool::Arrow
            | AnnotationTool::Polyline
            | AnnotationTool::Polylength
            | AnnotationTool::Area => LINE_FIELDS,
            AnnotationTool::Pen => PEN_FIELDS,
            AnnotationTool::Highlight => HIGHLIGHT_FIELDS,
            AnnotationTool::Cloud => CLOUD_FIELDS,
            AnnotationTool::CloudPlus => CLOUD_PLUS_FIELDS,
            AnnotationTool::Callout => CALLOUT_FIELDS,
            AnnotationTool::Dimension => DIMENSION_FIELDS,
            AnnotationTool::TextBox => TEXT_BOX_FIELDS,
            AnnotationTool::Snapshot => OPACITY_FIELDS,
            AnnotationTool::Select
            | AnnotationTool::Redact
            | AnnotationTool::Length
            | AnnotationTool::Image => NO_FIELDS,
        }
    }

    pub fn is_applicable(tool: AnnotationTool, field: ToolPropertyField) -> bool {
        Self::applicable_fields(tool).contains(&field)
    }

    pub fn range(tool: AnnotationTool, field: ToolPropertyField) -> Option<ToolPropertyRange> {
        if !Self::is_applicable(tool, field) {
            return None;
        }
        match field {
            ToolPropertyField::WidthPt if tool == AnnotationTool::Highlight => {
                Some(ToolPropertyRange {
                    min: 1.0,
                    max: 48.0,
                    step: 1.0,
                })
            }
            ToolPropertyField::WidthPt => Some(ToolPropertyRange {
                min: 0.25,
                max: 24.0,
                step: 0.25,
            }),
            ToolPropertyField::Opacity => Some(ToolPropertyRange {
                min: 0.0,
                max: 1.0,
                step: 0.05,
            }),
            ToolPropertyField::FontSizePt => Some(ToolPropertyRange {
                min: 6.0,
                max: 72.0,
                step: 1.0,
            }),
            ToolPropertyField::CloudIntensity => Some(ToolPropertyRange {
                min: 0.0,
                max: 4.0,
                step: 0.25,
            }),
            ToolPropertyField::Colour
            | ToolPropertyField::FillColour
            | ToolPropertyField::FontFamily
            | ToolPropertyField::SmoothCurves => None,
        }
    }

    pub fn validate_for_tool(&self, tool: AnnotationTool) -> Result<(), AnnotationError> {
        for (name, value) in [
            ("width", self.width_pt),
            ("fill opacity", self.fill_opacity),
            ("opacity", self.opacity),
            ("font size", self.font_size_pt),
            ("cloud intensity", self.cloud_intensity),
        ] {
            if !value.is_finite() {
                return Err(AnnotationError::InvalidAppearance(format!(
                    "tool {name} must be finite"
                )));
            }
        }
        validate_colour(&self.colour, "tool colour")?;
        if let Some(fill_colour) = &self.fill_colour {
            validate_colour(fill_colour, "tool fill colour")?;
        }
        validate_font_family(&self.font_family)?;
        if Self::is_applicable(tool, ToolPropertyField::FillColour)
            && !(0.0..=1.0).contains(&self.fill_opacity)
        {
            return Err(AnnotationError::InvalidAppearance(
                "fill opacity must be between 0 and 1".into(),
            ));
        }
        if Self::is_applicable(tool, ToolPropertyField::FontFamily)
            && !TOOL_FONT_FAMILY_OPTIONS
                .iter()
                .any(|option| option.value == self.font_family)
        {
            return Err(AnnotationError::InvalidAppearance(
                "font family must be one of the supported annotation fonts".into(),
            ));
        }

        for field in Self::applicable_fields(tool) {
            let value = match field {
                ToolPropertyField::WidthPt => Some(self.width_pt),
                ToolPropertyField::Opacity => Some(self.opacity),
                ToolPropertyField::FontSizePt => Some(self.font_size_pt),
                ToolPropertyField::CloudIntensity => Some(self.cloud_intensity),
                ToolPropertyField::Colour
                | ToolPropertyField::FillColour
                | ToolPropertyField::FontFamily
                | ToolPropertyField::SmoothCurves => None,
            };
            if let (Some(value), Some(range)) = (value, Self::range(tool, *field))
                && !(range.min..=range.max).contains(&value)
            {
                return Err(AnnotationError::InvalidAppearance(format!(
                    "{} must be between {} and {}",
                    field.label(),
                    range.min,
                    range.max
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn canonicalized_for_tool(
        &self,
        tool: AnnotationTool,
    ) -> Result<Self, AnnotationError> {
        self.validate_for_tool(tool)?;
        let mut canonical = Self::for_tool(tool);
        for field in Self::applicable_fields(tool) {
            match field {
                ToolPropertyField::Colour => canonical.colour = self.colour.to_ascii_lowercase(),
                ToolPropertyField::WidthPt => canonical.width_pt = self.width_pt,
                ToolPropertyField::FillColour => {
                    canonical.fill_colour = self
                        .fill_colour
                        .as_ref()
                        .map(|colour| colour.to_ascii_lowercase());
                    canonical.fill_opacity = self.fill_opacity;
                }
                ToolPropertyField::Opacity => canonical.opacity = self.opacity,
                ToolPropertyField::FontSizePt => canonical.font_size_pt = self.font_size_pt,
                ToolPropertyField::FontFamily => {
                    canonical.font_family.clone_from(&self.font_family)
                }
                ToolPropertyField::SmoothCurves => canonical.smooth_curves = self.smooth_curves,
                ToolPropertyField::CloudIntensity => {
                    canonical.cloud_intensity = self.cloud_intensity
                }
            }
        }
        Ok(canonical)
    }
}

impl ToolPropertyField {
    fn label(self) -> &'static str {
        match self {
            Self::Colour => "colour",
            Self::WidthPt => "width",
            Self::FillColour => "fill colour",
            Self::Opacity => "opacity",
            Self::FontSizePt => "font size",
            Self::FontFamily => "font family",
            Self::SmoothCurves => "smooth curves",
            Self::CloudIntensity => "cloud intensity",
        }
    }
}

fn validate_colour(colour: &str, field: &str) -> Result<(), AnnotationError> {
    let valid = colour.len() == 7
        && colour.starts_with('#')
        && colour[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(AnnotationError::InvalidAppearance(format!(
            "{field} must be a six-digit hex colour"
        )))
    }
}

fn validate_font_family(font_family: &str) -> Result<(), AnnotationError> {
    if font_family.is_empty()
        || font_family.len() > crate::annotation_model::MAX_FONT_FAMILY_BYTES
        || font_family.chars().any(char::is_control)
    {
        return Err(AnnotationError::InvalidAppearance(
            "font family must be non-empty text within the native model limit".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applicability_matches_the_working_native_subset() {
        assert_eq!(
            ToolProperties::applicable_fields(AnnotationTool::Rectangle),
            SHAPE_FIELDS
        );
        assert_eq!(
            ToolProperties::applicable_fields(AnnotationTool::Pen),
            PEN_FIELDS
        );
        assert_eq!(
            ToolProperties::applicable_fields(AnnotationTool::Dimension),
            DIMENSION_FIELDS
        );
        assert_eq!(
            ToolProperties::applicable_fields(AnnotationTool::TextBox),
            TEXT_BOX_FIELDS
        );
        for unsupported in [
            AnnotationTool::Select,
            AnnotationTool::Redact,
            AnnotationTool::Length,
            AnnotationTool::Image,
        ] {
            assert!(ToolProperties::applicable_fields(unsupported).is_empty());
        }
    }

    #[test]
    fn defaults_and_ranges_match_the_reference_contract() {
        assert_eq!(
            ToolProperties::for_tool(AnnotationTool::Arrow).width_pt,
            0.5
        );
        assert_eq!(
            ToolProperties::for_tool(AnnotationTool::Highlight).colour,
            "#ffff00"
        );
        assert_eq!(
            ToolProperties::range(AnnotationTool::Highlight, ToolPropertyField::WidthPt),
            Some(ToolPropertyRange {
                min: 1.0,
                max: 48.0,
                step: 1.0,
            })
        );
        assert_eq!(
            ToolProperties::range(AnnotationTool::Rectangle, ToolPropertyField::WidthPt),
            Some(ToolPropertyRange {
                min: 0.25,
                max: 24.0,
                step: 0.25,
            })
        );
        assert_eq!(
            ToolProperties::range(AnnotationTool::CloudPlus, ToolPropertyField::WidthPt),
            None
        );
        assert_eq!(
            TOOL_FONT_FAMILY_OPTIONS
                .iter()
                .map(|option| option.value)
                .collect::<Vec<_>>(),
            ["Arimo", "Roboto Mono", "Helvetica", "Tinos"]
        );
    }

    #[test]
    fn validation_rejects_non_finite_out_of_range_and_invalid_colours() {
        let mut properties = ToolProperties::for_tool(AnnotationTool::Rectangle);
        properties.width_pt = f64::NAN;
        assert!(
            properties
                .validate_for_tool(AnnotationTool::Rectangle)
                .is_err()
        );

        let mut properties = ToolProperties::for_tool(AnnotationTool::Highlight);
        properties.width_pt = 0.5;
        assert!(
            properties
                .validate_for_tool(AnnotationTool::Highlight)
                .is_err()
        );

        let mut properties = ToolProperties::for_tool(AnnotationTool::Rectangle);
        properties.fill_colour = Some("transparent".into());
        assert!(
            properties
                .validate_for_tool(AnnotationTool::Rectangle)
                .is_err()
        );
    }

    #[test]
    fn canonicalization_discards_inapplicable_values() {
        let mut properties = ToolProperties::for_tool(AnnotationTool::Snapshot);
        properties.colour = "#ABCDEF".into();
        properties.opacity = 0.45;
        let canonical = properties
            .canonicalized_for_tool(AnnotationTool::Snapshot)
            .unwrap();
        assert_eq!(canonical.colour, "#ff0000");
        assert_eq!(canonical.opacity, 0.45);
    }
}
