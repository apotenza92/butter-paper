//! Materialization of the frozen annotation-density fixture.
//!
//! The tracked fixture stores a compact recipe. This module expands that
//! recipe into the same PDF-space rectangles as the JavaScript fixture oracle
//! so a normal annotation document can load the complete data set on open.

use serde::Deserialize;

use crate::annotation_model::{
    Annotation, AnnotationError, MarkupId, PdfRect, RectangleAnnotation, RectangleAppearance,
    StrokeStyle,
};

const FIXTURE_ID: &str = "bp-annotation-density-v1";
const COMMAND_STREAM_SHA256: &str =
    "ab901d472b7b0ae90a621f928392397b3a53f93273d88a11bb5be08bf132586e";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DensityFixtureImportOutcome {
    pub fixture_id: String,
    pub page_count: u32,
    pub annotation_count: usize,
}

pub(crate) struct MaterializedDensityFixture {
    pub outcome: DensityFixtureImportOutcome,
    pub annotations: Vec<Annotation>,
}

#[derive(Deserialize)]
struct DensityFixtureSpec {
    schema_version: String,
    fixture_id: String,
    coordinate_space: String,
    document: DensityDocumentSpec,
    annotation_generator: DensityGeneratorSpec,
    artifact_sha256: DensityArtifactHashes,
}

#[derive(Deserialize)]
struct DensityDocumentSpec {
    page_count: u32,
    page_width: f64,
    page_height: f64,
}

#[derive(Deserialize)]
struct DensityGeneratorSpec {
    schema_version: String,
    annotation_type: String,
    empty_interaction_page: u32,
    dense_interaction_page: u32,
    dense_page_annotation_count: usize,
    pages_3_through_20_annotation_count: usize,
    pages_21_through_100_annotation_count: usize,
    total_annotation_count: usize,
}

#[derive(Deserialize)]
struct DensityArtifactHashes {
    commands: String,
}

pub(crate) fn materialize_density_fixture(
    fixture_json: &str,
) -> Result<MaterializedDensityFixture, AnnotationError> {
    let spec: DensityFixtureSpec = serde_json::from_str(fixture_json)
        .map_err(|error| AnnotationError::InvalidFixture(error.to_string()))?;
    validate_frozen_spec(&spec)?;

    let mut annotations = Vec::with_capacity(spec.annotation_generator.total_annotation_count);
    let mut global_index = 0_usize;
    for page_number in 2..=spec.document.page_count {
        let count = if page_number == spec.annotation_generator.dense_interaction_page {
            spec.annotation_generator.dense_page_annotation_count
        } else if page_number <= 20 {
            spec.annotation_generator
                .pages_3_through_20_annotation_count
        } else {
            spec.annotation_generator
                .pages_21_through_100_annotation_count
        };
        for local_index in 0..count {
            annotations.push(Annotation::Rectangle(RectangleAnnotation {
                id: MarkupId::new(format!(
                    "{}:p{page_number:03}:rectangle:{:04}",
                    spec.fixture_id,
                    local_index + 1
                ))?,
                page_index: page_number - 1,
                rect: density_bounds(page_number, local_index)?,
                rotation_degrees: 0.0,
                appearance: density_appearance(global_index)?,
                locked: false,
            }));
            global_index += 1;
        }
    }
    if annotations.len() != spec.annotation_generator.total_annotation_count {
        return Err(AnnotationError::InvalidFixture(format!(
            "density recipe produced {} annotations, expected {}",
            annotations.len(),
            spec.annotation_generator.total_annotation_count
        )));
    }

    Ok(MaterializedDensityFixture {
        outcome: DensityFixtureImportOutcome {
            fixture_id: spec.fixture_id,
            page_count: spec.document.page_count,
            annotation_count: annotations.len(),
        },
        annotations,
    })
}

fn validate_frozen_spec(spec: &DensityFixtureSpec) -> Result<(), AnnotationError> {
    let generator = &spec.annotation_generator;
    let valid = spec.schema_version == "bp-fixture-spec-v1"
        && spec.fixture_id == FIXTURE_ID
        && spec.coordinate_space == "pdf-points-bottom-left"
        && spec.document.page_count == 100
        && spec.document.page_width == 612.0
        && spec.document.page_height == 792.0
        && generator.schema_version == "bp-rectangle-density-recipe-v1"
        && generator.annotation_type == "rectangle"
        && generator.empty_interaction_page == 1
        && generator.dense_interaction_page == 2
        && generator.dense_page_annotation_count == 100
        && generator.pages_3_through_20_annotation_count == 10
        && generator.pages_21_through_100_annotation_count == 9
        && generator.total_annotation_count == 1_000
        && spec.artifact_sha256.commands == COMMAND_STREAM_SHA256;
    if valid {
        Ok(())
    } else {
        Err(AnnotationError::InvalidFixture(
            "density fixture does not match the frozen v1 recipe".into(),
        ))
    }
}

fn density_bounds(page_number: u32, local_index: usize) -> Result<PdfRect, AnnotationError> {
    let local_index = local_index as f64;
    if page_number == 2 {
        PdfRect::new(
            48.0 + (local_index % 10.0) * 51.0,
            96.0 + (local_index / 10.0).floor() * 60.0,
            36.0,
            24.0,
        )
    } else if page_number <= 20 {
        PdfRect::new(
            72.0 + (local_index % 5.0) * 96.0,
            180.0 + (local_index / 5.0).floor() * 180.0,
            48.0,
            36.0,
        )
    } else {
        PdfRect::new(
            96.0 + (local_index % 3.0) * 156.0,
            144.0 + (local_index / 3.0).floor() * 216.0,
            60.0,
            48.0,
        )
    }
}

fn density_appearance(global_index: usize) -> Result<RectangleAppearance, AnnotationError> {
    const COLORS: [&str; 4] = ["#1d6ed8", "#dc2626", "#14a55a", "#b155de"];
    let stroke_style = if global_index.is_multiple_of(5) {
        StrokeStyle::Dashed
    } else {
        StrokeStyle::Solid
    };
    RectangleAppearance::new(
        COLORS[global_index % COLORS.len()],
        1.0 + (global_index % 3) as f64 * 0.5,
        Some(COLORS[global_index % COLORS.len()]),
        1.0,
    )?
    .with_fill_opacity(0.08 + (global_index % 3) as f64 * 0.02)
    .map(|appearance| appearance.with_stroke_style(stroke_style))
}
