//! Two-cycle native PDF qualification for the representative editor state.

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lopdf::{Dictionary, Document, Object};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    annotation_adapter::AnnotationAdapter,
    annotation_model::{
        ImageAnnotation, LengthAnnotation, PenAnnotation, RectangleAnnotation, TextBoxAnnotation,
    },
    pdf_engine::{PdfPersistenceError, PdfPersistenceSession, UntouchedAnnotation},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatorOutput {
    pub cycle: u8,
    pub command: &'static str,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterOracleReport {
    pub command: &'static str,
    pub renderer_version: String,
    pub page: u32,
    /// Device-pixel crop at 72 DPI in `[x, y, width, height]` order.
    pub crop: [u32; 4],
    pub source_crop_sha256: String,
    pub cycle_1_crop_sha256: String,
    pub cycle_2_crop_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedPersistenceArtifacts {
    pub cycle_1_pdf: PathBuf,
    pub cycle_2_pdf: PathBuf,
    pub source_crop: PathBuf,
    pub cycle_1_crop: PathBuf,
    pub cycle_2_crop: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistenceComparisonReport {
    pub completed_command_ids: Vec<&'static str>,
    pub unknown_probe_exact_after_cycle_1: bool,
    pub unknown_probe_exact_after_cycle_2: bool,
    pub typed_state_exact_after_cycle_1: bool,
    pub typed_state_exact_after_cycle_2: bool,
    pub untouched_annotation_count: usize,
    pub independent_validation_passed: bool,
    pub independent_visual_validation_passed: bool,
    pub cycle_1_sha256: String,
    pub cycle_2_sha256: String,
    pub validator_outputs: Vec<ValidatorOutput>,
    pub raster_oracle: RasterOracleReport,
    pub retained_artifacts: Option<RetainedPersistenceArtifacts>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PersistenceEvidenceReceipt {
    pub status: &'static str,
    pub completed_command_ids: Vec<&'static str>,
    pub typed_state_exact: bool,
    pub unknown_probes_exact: bool,
    pub untouched_annotation_count: usize,
    pub independent_pdf_validation_passed: bool,
    pub independent_visual_validation_passed: bool,
    pub validator_receipt_count: usize,
    pub cycle_1_sha256: String,
    pub cycle_2_sha256: String,
    pub source_crop_sha256: String,
    pub cycle_1_crop_sha256: String,
    pub cycle_2_crop_sha256: String,
    pub artifacts_retained: bool,
}

impl PersistenceComparisonReport {
    pub fn exact_receipt(&self) -> Result<PersistenceEvidenceReceipt, PersistenceComparisonError> {
        const COMMAND_IDS: [&str; 8] = [
            "unknown:import",
            "unknown:assert-cycle-1",
            "unknown:assert-cycle-2",
            "persistence:apply-fixed-state",
            "persistence:save-1",
            "persistence:reopen-1",
            "persistence:save-2",
            "persistence:reopen-2",
        ];
        let valid_sha256 =
            |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        let validator_receipts_exact = self.validator_outputs.len() == 4
            && self
                .validator_outputs
                .iter()
                .zip([(1, "qpdf"), (1, "pdfinfo"), (2, "qpdf"), (2, "pdfinfo")])
                .all(|(actual, expected)| {
                    (actual.cycle, actual.command) == expected
                        && (!actual.stdout.is_empty() || !actual.stderr.is_empty())
                });
        let typed_state_exact =
            self.typed_state_exact_after_cycle_1 && self.typed_state_exact_after_cycle_2;
        let unknown_probes_exact =
            self.unknown_probe_exact_after_cycle_1 && self.unknown_probe_exact_after_cycle_2;
        let raster_exact = self.raster_oracle.page == 1
            && self.raster_oracle.crop == [54, 250, 510, 430]
            && valid_sha256(&self.raster_oracle.source_crop_sha256)
            && valid_sha256(&self.raster_oracle.cycle_1_crop_sha256)
            && valid_sha256(&self.raster_oracle.cycle_2_crop_sha256)
            && self.raster_oracle.source_crop_sha256 != self.raster_oracle.cycle_1_crop_sha256
            && self.raster_oracle.cycle_1_crop_sha256 == self.raster_oracle.cycle_2_crop_sha256;
        if self.completed_command_ids != COMMAND_IDS
            || !typed_state_exact
            || !unknown_probes_exact
            || self.untouched_annotation_count == 0
            || !self.independent_validation_passed
            || !self.independent_visual_validation_passed
            || !validator_receipts_exact
            || !valid_sha256(&self.cycle_1_sha256)
            || !valid_sha256(&self.cycle_2_sha256)
            || !raster_exact
        {
            return Err(PersistenceComparisonError::Invalid(
                "two-cycle persistence evidence is incomplete".into(),
            ));
        }
        Ok(PersistenceEvidenceReceipt {
            status: "exact-passed",
            completed_command_ids: self.completed_command_ids.clone(),
            typed_state_exact,
            unknown_probes_exact,
            untouched_annotation_count: self.untouched_annotation_count,
            independent_pdf_validation_passed: true,
            independent_visual_validation_passed: true,
            validator_receipt_count: self.validator_outputs.len(),
            cycle_1_sha256: self.cycle_1_sha256.clone(),
            cycle_2_sha256: self.cycle_2_sha256.clone(),
            source_crop_sha256: self.raster_oracle.source_crop_sha256.clone(),
            cycle_1_crop_sha256: self.raster_oracle.cycle_1_crop_sha256.clone(),
            cycle_2_crop_sha256: self.raster_oracle.cycle_2_crop_sha256.clone(),
            artifacts_retained: self.retained_artifacts.is_some(),
        })
    }
}

pub struct PersistenceComparisonScenario;

impl PersistenceComparisonScenario {
    /// Runs the qualification against caller-owned output paths.
    ///
    /// The PDF outputs remain subject to the caller's lifecycle. Temporary
    /// raster-oracle files are removed after their hashes are recorded.
    pub fn execute(
        source: &Path,
        cycle_1: &Path,
        cycle_2: &Path,
        document_id: u64,
        adapter: &mut AnnotationAdapter,
    ) -> Result<PersistenceComparisonReport, PersistenceComparisonError> {
        let raster_scratch = RasterScratch::temporary(cycle_1.parent().ok_or_else(|| {
            PersistenceComparisonError::Invalid("cycle 1 path has no parent directory".into())
        })?)?;
        Self::execute_at_paths(
            source,
            cycle_1,
            cycle_2,
            document_id,
            adapter,
            raster_scratch,
            None,
        )
    }

    /// Runs the qualification and retains its two PDFs plus the three fixed
    /// raster crops in an explicitly supplied evidence directory.
    ///
    /// Existing artifacts are never replaced. This makes a rerun fail closed
    /// instead of silently changing evidence gathered by an earlier run.
    pub fn execute_with_evidence_directory(
        source: &Path,
        evidence_directory: &Path,
        document_id: u64,
        adapter: &mut AnnotationAdapter,
    ) -> Result<PersistenceComparisonReport, PersistenceComparisonError> {
        create_evidence_directory(evidence_directory)?;
        let cycle_1 = evidence_directory.join("cycle-1.pdf");
        let cycle_2 = evidence_directory.join("cycle-2.pdf");
        require_output_absent(&cycle_1, "PDF")?;
        require_output_absent(&cycle_2, "PDF")?;
        let raster_scratch = RasterScratch::retained(evidence_directory)?;
        let retained = RetainedPersistenceArtifacts {
            cycle_1_pdf: cycle_1.clone(),
            cycle_2_pdf: cycle_2.clone(),
            source_crop: raster_scratch.source.clone(),
            cycle_1_crop: raster_scratch.cycle_1.clone(),
            cycle_2_crop: raster_scratch.cycle_2.clone(),
        };
        Self::execute_at_paths(
            source,
            &cycle_1,
            &cycle_2,
            document_id,
            adapter,
            raster_scratch,
            Some(retained),
        )
    }

    fn execute_at_paths(
        source: &Path,
        cycle_1: &Path,
        cycle_2: &Path,
        document_id: u64,
        adapter: &mut AnnotationAdapter,
        raster_scratch: RasterScratch,
        retained_artifacts: Option<RetainedPersistenceArtifacts>,
    ) -> Result<PersistenceComparisonReport, PersistenceComparisonError> {
        let snapshot = adapter.snapshot(document_id).ok_or_else(|| {
            PersistenceComparisonError::Invalid("editor document is missing".into())
        })?;
        let source_document = document_probe(source)?;

        let mut first = PdfPersistenceSession::open(source)?;
        let untouched_annotations = first.untouched_annotations().to_vec();
        let source_unknown = unknown_probes(source, &untouched_annotations)?;
        for rectangle in snapshot.rectangles {
            first.add_rectangle(rectangle)?;
        }
        for pen in snapshot.pens {
            first.add_pen(pen)?;
        }
        for text in snapshot.text_boxes {
            first.add_text_box(text)?;
        }
        for length in snapshot.lengths {
            first.add_length(length)?;
        }
        for image in snapshot.images {
            first.add_image(image)?;
        }
        let expected_typed_state = TypedStateProbe::from_session(&first);
        first.save_as_ambient_for_development(cycle_1)?;
        let mut validator_outputs = validate_independently(cycle_1, 1)?;
        let cycle_1_sha256 = file_sha256(cycle_1)?;
        let cycle_1_unknown = unknown_probes(cycle_1, &untouched_annotations)? == source_unknown;
        if !cycle_1_unknown || document_probe(cycle_1)? != source_document {
            return Err(PersistenceComparisonError::Invalid(
                "cycle 1 changed unknown annotation probes or original document data".into(),
            ));
        }

        let reopened_1 = PdfPersistenceSession::open(cycle_1)?;
        require_exact_typed_state(&reopened_1, &expected_typed_state, 1)?;
        reopened_1.save_as_ambient_for_development(cycle_2)?;
        validator_outputs.extend(validate_independently(cycle_2, 2)?);
        let cycle_2_sha256 = file_sha256(cycle_2)?;
        let cycle_2_unknown = unknown_probes(cycle_2, &untouched_annotations)? == source_unknown;
        if !cycle_2_unknown || document_probe(cycle_2)? != source_document {
            return Err(PersistenceComparisonError::Invalid(
                "cycle 2 changed unknown annotation probes or original document data".into(),
            ));
        }
        require_exact_typed_state(
            &PdfPersistenceSession::open(cycle_2)?,
            &expected_typed_state,
            2,
        )?;
        let raster_oracle = validate_fixed_raster_crop(source, cycle_1, cycle_2, &raster_scratch)?;
        adapter.mark_saved(document_id)?;

        Ok(PersistenceComparisonReport {
            completed_command_ids: vec![
                "unknown:import",
                "unknown:assert-cycle-1",
                "unknown:assert-cycle-2",
                "persistence:apply-fixed-state",
                "persistence:save-1",
                "persistence:reopen-1",
                "persistence:save-2",
                "persistence:reopen-2",
            ],
            unknown_probe_exact_after_cycle_1: true,
            unknown_probe_exact_after_cycle_2: true,
            typed_state_exact_after_cycle_1: true,
            typed_state_exact_after_cycle_2: true,
            untouched_annotation_count: source_unknown.len(),
            independent_validation_passed: true,
            independent_visual_validation_passed: true,
            cycle_1_sha256,
            cycle_2_sha256,
            validator_outputs,
            raster_oracle,
            retained_artifacts,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TypedStateProbe {
    rectangles: Vec<RectangleAnnotation>,
    pens: Vec<PenAnnotation>,
    text_boxes: Vec<TextBoxAnnotation>,
    lengths: Vec<LengthAnnotation>,
    images: Vec<ImageAnnotation>,
}

impl TypedStateProbe {
    fn from_session(session: &PdfPersistenceSession) -> Self {
        Self {
            rectangles: session.rectangles().to_vec(),
            pens: session.pens().to_vec(),
            text_boxes: session.text_boxes().to_vec(),
            lengths: session.lengths().to_vec(),
            images: session.images().to_vec(),
        }
    }
}

fn require_exact_typed_state(
    session: &PdfPersistenceSession,
    expected: &TypedStateProbe,
    cycle: u8,
) -> Result<(), PersistenceComparisonError> {
    let rectangles_match = expected.rectangles == session.rectangles();
    let pens_match = expected.pens == session.pens();
    let text_matches = expected.text_boxes == session.text_boxes();
    let lengths_match = expected.lengths == session.lengths();
    let images_match = expected.images == session.images();
    if !(rectangles_match && pens_match && text_matches && lengths_match && images_match) {
        let pen_detail = expected.pens.first().and_then(|expected_pen| {
            session
                .pens()
                .iter()
                .find(|actual| actual.id == expected_pen.id)
                .map(|actual| {
                    let first_point_drift = expected_pen
                        .points()
                        .iter()
                        .zip(actual.points())
                        .enumerate()
                        .find(|(_, (expected, actual))| expected != actual)
                        .map(|(index, (expected, actual))| format!("{index}:{expected:?}!={actual:?}"))
                        .unwrap_or_else(|| format!("lengths:{}!={}", expected_pen.points().len(), actual.points().len()));
                    format!(
                        "points={} ({first_point_drift}), appearance={}, smooth={}, tool={}, blend={}, locked={}, page={}",
                        actual.points() == expected_pen.points(),
                        actual.appearance == expected_pen.appearance,
                        actual.smooth_curves == expected_pen.smooth_curves,
                        actual.tool() == expected_pen.tool(),
                        actual.blend_mode() == expected_pen.blend_mode(),
                        actual.locked == expected_pen.locked,
                        actual.page_index == expected_pen.page_index,
                    )
                })
        }).unwrap_or_else(|| "missing".into());
        return Err(PersistenceComparisonError::Invalid(format!(
            "cycle {cycle} reopened PDF changed full canonical typed state (rectangles={rectangles_match}, pens={pens_match} [{pen_detail}], text={text_matches}, lengths={lengths_match}, images={images_match})",
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnknownProbe {
    page_index: u32,
    annotation_index: usize,
    name: String,
    subtype: String,
    complete_dictionary: String,
    contents: Option<Vec<u8>>,
    dictionary_probe: Option<Vec<u8>>,
    appearance: Option<UnknownAppearanceProbe>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnknownAppearanceProbe {
    complete_appearance_dictionary: String,
    complete_appearance_graph: String,
    appearance_probe: Option<Vec<u8>>,
    appearance_bytes: Vec<u8>,
}

fn unknown_probes(
    path: &Path,
    expected: &[UntouchedAnnotation],
) -> Result<Vec<UnknownProbe>, PersistenceComparisonError> {
    let document = Document::load(path)?;
    let mut remaining = BTreeMap::<(String, String), usize>::new();
    for annotation in expected {
        *remaining
            .entry((annotation.name.clone(), annotation.subtype.clone()))
            .or_default() += 1;
    }
    let mut probes = Vec::with_capacity(expected.len());
    for (page_number, page_id) in document.get_pages() {
        let page = document.get_object(page_id)?.as_dict()?;
        let Ok(annotation_object) = page.get(b"Annots") else {
            continue;
        };
        let annotations = resolve(&document, annotation_object)?.as_array()?;
        for (annotation_index, annotation_object) in annotations.iter().enumerate() {
            let annotation = resolve(&document, annotation_object)?.as_dict()?;
            let name = annotation
                .get(b"NM")
                .ok()
                .and_then(|value| value.as_str().ok())
                .map(|value| String::from_utf8_lossy(value).into_owned())
                .unwrap_or_else(|| {
                    format!("page-{}-annotation-{annotation_index}", page_number - 1)
                });
            let subtype = annotation
                .get(b"Subtype")
                .ok()
                .and_then(|value| value.as_name().ok())
                .map(|value| String::from_utf8_lossy(value).into_owned())
                .unwrap_or_default();
            let key = (name.clone(), subtype.clone());
            let Some(count) = remaining.get_mut(&key) else {
                continue;
            };
            if *count == 0 {
                continue;
            }
            *count -= 1;
            probes.push(unknown_probe(
                &document,
                annotation,
                page_number - 1,
                annotation_index,
                name,
                subtype,
            )?);
        }
    }
    if remaining.values().any(|count| *count != 0) {
        return Err(PersistenceComparisonError::Invalid(format!(
            "PDF is missing untouched annotation identities: {remaining:?}"
        )));
    }
    Ok(probes)
}

fn unknown_probe(
    document: &Document,
    annotation: &Dictionary,
    page_index: u32,
    annotation_index: usize,
    name: String,
    subtype: String,
) -> Result<UnknownProbe, PersistenceComparisonError> {
    let appearance = annotation
        .get(b"AP")
        .ok()
        .map(|appearance_dictionary| {
            let appearance = resolve(document, appearance_dictionary)?
                .as_dict()?
                .get(b"N")?;
            let stream = resolve(document, appearance)?.as_stream()?;
            Ok::<_, PersistenceComparisonError>(UnknownAppearanceProbe {
                complete_appearance_dictionary: format!("{:?}", stream.dict),
                complete_appearance_graph: object_graph_debug(document, appearance),
                appearance_probe: stream
                    .dict
                    .get(b"BPStreamProbe")
                    .ok()
                    .and_then(|value| value.as_str().ok())
                    .map(ToOwned::to_owned),
                appearance_bytes: stream.content.clone(),
            })
        })
        .transpose()?;
    Ok(UnknownProbe {
        page_index,
        annotation_index,
        name,
        subtype,
        complete_dictionary: format!("{annotation:?}"),
        contents: annotation
            .get(b"Contents")
            .ok()
            .and_then(|value| value.as_str().ok())
            .map(ToOwned::to_owned),
        dictionary_probe: annotation
            .get(b"BPUnknown")
            .ok()
            .and_then(|value| value.as_str().ok())
            .map(ToOwned::to_owned),
        appearance,
    })
}

fn object_graph_debug(document: &Document, root: &Object) -> String {
    fn visit(document: &Document, object: &Object, visited: &mut Vec<(u32, u16)>) -> String {
        match object {
            Object::Reference(id) => {
                if visited.contains(id) {
                    return format!("cycle:{id:?}");
                }
                visited.push(*id);
                let value = document
                    .get_object(*id)
                    .map(|value| visit(document, value, visited))
                    .unwrap_or_else(|error| format!("missing:{error}"));
                format!("ref:{id:?}:{value}")
            }
            Object::Array(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| visit(document, value, visited))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Object::Dictionary(dictionary) => format!(
                "dict:{:?}:{{{}}}",
                dictionary,
                dictionary
                    .iter()
                    .map(|(key, value)| format!(
                        "{}={}",
                        String::from_utf8_lossy(key),
                        visit(document, value, visited)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Object::Stream(stream) => format!(
                "stream:{:?}:{}:{:?}",
                stream.dict,
                stream.content.len(),
                stream.content
            ),
            value => format!("{value:?}"),
        }
    }
    visit(document, root, &mut Vec::new())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentProbe {
    content: Vec<u8>,
    media_box: String,
    crop_box: String,
    rotation: String,
    info: String,
}

fn document_probe(path: &Path) -> Result<DocumentProbe, PersistenceComparisonError> {
    let document = Document::load(path)?;
    let page_id = *document
        .get_pages()
        .values()
        .next()
        .ok_or_else(|| PersistenceComparisonError::Invalid("PDF has no pages".into()))?;
    let page = document.get_object(page_id)?.as_dict()?;
    let content = match resolve(&document, page.get(b"Contents")?)? {
        Object::Stream(stream) => stream.content.clone(),
        object => format!("{object:?}").into_bytes(),
    };
    let info = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|value| resolve(&document, value).ok())
        .map(|value| format!("{value:?}"))
        .unwrap_or_default();
    Ok(DocumentProbe {
        content,
        media_box: format!("{:?}", page.get(b"MediaBox")?),
        crop_box: format!("{:?}", page.get(b"CropBox").ok()),
        rotation: format!("{:?}", page.get(b"Rotate").ok()),
        info,
    })
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Result<&'a Object, lopdf::Error> {
    match object {
        Object::Reference(id) => document.get_object(*id),
        object => Ok(object),
    }
}

fn validate_independently(
    path: &Path,
    cycle: u8,
) -> Result<Vec<ValidatorOutput>, PersistenceComparisonError> {
    let mut results = Vec::new();
    for command in ["qpdf", "pdfinfo"] {
        let mut process = Command::new(command);
        if command == "qpdf" {
            process.arg("--check");
        }
        let output = process
            .arg(path)
            .output()
            .map_err(PersistenceComparisonError::Io)?;
        if !output.status.success() {
            return Err(PersistenceComparisonError::Invalid(format!(
                "{command} rejected {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        results.push(ValidatorOutput {
            cycle,
            command,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(results)
}

const RASTER_PAGE: u32 = 1;
const RASTER_CROP: [u32; 4] = [54, 250, 510, 430];
static NEXT_RASTER_DIRECTORY_ID: AtomicU64 = AtomicU64::new(1);

struct RasterScratch {
    directory: PathBuf,
    source: PathBuf,
    cycle_1: PathBuf,
    cycle_2: PathBuf,
    remove_on_drop: bool,
}

impl RasterScratch {
    fn temporary(parent: &Path) -> Result<Self, PersistenceComparisonError> {
        for _ in 0..100 {
            let nonce = NEXT_RASTER_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(
                ".butter-paper-raster-{}-{nonce}",
                std::process::id()
            ));
            match fs::create_dir(&directory) {
                Ok(()) => return Ok(Self::new(directory, true)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(PersistenceComparisonError::Io(error)),
            }
        }
        Err(PersistenceComparisonError::Invalid(
            "could not reserve a unique raster-oracle directory".into(),
        ))
    }

    fn retained(directory: &Path) -> Result<Self, PersistenceComparisonError> {
        let scratch = Self::new(directory.to_path_buf(), false);
        for path in [&scratch.source, &scratch.cycle_1, &scratch.cycle_2] {
            require_output_absent(path, "raster evidence")?;
        }
        Ok(scratch)
    }

    fn new(directory: PathBuf, remove_on_drop: bool) -> Self {
        Self {
            source: directory.join("source-crop.ppm"),
            cycle_1: directory.join("cycle-1-crop.ppm"),
            cycle_2: directory.join("cycle-2-crop.ppm"),
            directory,
            remove_on_drop,
        }
    }
}

impl Drop for RasterScratch {
    fn drop(&mut self) {
        if !self.remove_on_drop {
            return;
        }
        for path in [&self.source, &self.cycle_1, &self.cycle_2] {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

fn create_evidence_directory(path: &Path) -> Result<(), PersistenceComparisonError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(PersistenceComparisonError::Invalid(format!(
                "evidence directory must not be a symbolic link: {}",
                path.display()
            )));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(PersistenceComparisonError::Invalid(format!(
                "evidence path is not a directory: {}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(PersistenceComparisonError::Io)?;
        }
        Err(error) => return Err(PersistenceComparisonError::Io(error)),
    }
    Ok(())
}

fn require_output_absent(
    path: &Path,
    artifact_kind: &str,
) -> Result<(), PersistenceComparisonError> {
    if path.exists() {
        return Err(PersistenceComparisonError::Invalid(format!(
            "refusing to replace existing {artifact_kind} {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_fixed_raster_crop(
    source: &Path,
    cycle_1: &Path,
    cycle_2: &Path,
    scratch: &RasterScratch,
) -> Result<RasterOracleReport, PersistenceComparisonError> {
    let version = Command::new("pdftoppm")
        .arg("-v")
        .output()
        .map_err(PersistenceComparisonError::Io)?;
    if !version.status.success() {
        return Err(PersistenceComparisonError::Invalid(format!(
            "pdftoppm -v failed: {}",
            String::from_utf8_lossy(&version.stderr)
        )));
    }
    let renderer_version = format!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    let source_crop_sha256 = render_fixed_crop(source, &scratch.source)?;
    let cycle_1_crop_sha256 = render_fixed_crop(cycle_1, &scratch.cycle_1)?;
    let cycle_2_crop_sha256 = render_fixed_crop(cycle_2, &scratch.cycle_2)?;
    if source_crop_sha256 == cycle_1_crop_sha256 {
        return Err(PersistenceComparisonError::Invalid(
            "fixed raster crop did not expose the applied editor state".into(),
        ));
    }
    if cycle_1_crop_sha256 != cycle_2_crop_sha256 {
        return Err(PersistenceComparisonError::Invalid(
            "fixed raster crop changed after the second save/reopen cycle".into(),
        ));
    }
    Ok(RasterOracleReport {
        command: "pdftoppm",
        renderer_version,
        page: RASTER_PAGE,
        crop: RASTER_CROP,
        source_crop_sha256,
        cycle_1_crop_sha256,
        cycle_2_crop_sha256,
    })
}

fn render_fixed_crop(pdf: &Path, output: &Path) -> Result<String, PersistenceComparisonError> {
    require_output_absent(output, "raster evidence")?;
    let prefix = output.with_extension("");
    let status = Command::new("pdftoppm")
        .args([
            "-f",
            "1",
            "-l",
            "1",
            "-r",
            "72",
            "-x",
            "54",
            "-y",
            "250",
            "-W",
            "510",
            "-H",
            "430",
            "-singlefile",
        ])
        .arg(pdf)
        .arg(&prefix)
        .output()
        .map_err(PersistenceComparisonError::Io)?;
    if !status.status.success() {
        return Err(PersistenceComparisonError::Invalid(format!(
            "pdftoppm rejected {}: {}",
            pdf.display(),
            String::from_utf8_lossy(&status.stderr)
        )));
    }
    let metadata = fs::metadata(output).map_err(PersistenceComparisonError::Io)?;
    if metadata.len() <= 16 {
        return Err(PersistenceComparisonError::Invalid(format!(
            "pdftoppm produced an empty raster oracle for {}",
            pdf.display()
        )));
    }
    file_sha256(output)
}

fn file_sha256(path: &Path) -> Result<String, PersistenceComparisonError> {
    let bytes = fs::read(path).map_err(PersistenceComparisonError::Io)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Debug)]
pub enum PersistenceComparisonError {
    Annotation(crate::annotation_model::AnnotationError),
    Invalid(String),
    Io(std::io::Error),
    Pdf(lopdf::Error),
    Persistence(PdfPersistenceError),
}

impl From<crate::annotation_model::AnnotationError> for PersistenceComparisonError {
    fn from(error: crate::annotation_model::AnnotationError) -> Self {
        Self::Annotation(error)
    }
}
impl From<lopdf::Error> for PersistenceComparisonError {
    fn from(error: lopdf::Error) -> Self {
        Self::Pdf(error)
    }
}
impl From<PdfPersistenceError> for PersistenceComparisonError {
    fn from(error: PdfPersistenceError) -> Self {
        Self::Persistence(error)
    }
}
impl fmt::Display for PersistenceComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Annotation(error) => error.fmt(formatter),
            Self::Invalid(message) => formatter.write_str(message),
            Self::Io(error) => error.fmt(formatter),
            Self::Pdf(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for PersistenceComparisonError {}
