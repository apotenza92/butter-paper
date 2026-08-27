use std::{path::PathBuf, process::Command};

use butter_paper_gpui_gallery::{
    annotation_adapter::AnnotationAdapter,
    annotation_model::DecodedRgbaAsset,
    editor_comparison_scenario::{EditorComparisonScenario, RecordingEditorObserver},
    persistence_comparison_scenario::PersistenceComparisonScenario,
};

#[test]
fn representative_state_saves_and_reopens_twice_without_changing_unknown_probes() {
    let scratch = tempfile::tempdir().unwrap();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let generator = manifest.join("../performance/fixture-oracle.mjs");
    let fixtures = scratch.path().join("fixtures");
    let generated = Command::new("node")
        .arg(generator)
        .arg("generate")
        .arg("--output")
        .arg(&fixtures)
        .output()
        .expect("Node must run the public fixture generator");
    assert!(
        generated.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let mut adapter = AnnotationAdapter::default();
    EditorComparisonScenario::embedded()
        .unwrap()
        .execute(
            71,
            DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
            &mut adapter,
            &mut RecordingEditorObserver::default(),
        )
        .unwrap();
    let report = PersistenceComparisonScenario::execute(
        &fixtures.join("bp-annotation-all-v1.pdf"),
        &scratch.path().join("cycle-1.pdf"),
        &scratch.path().join("cycle-2.pdf"),
        71,
        &mut adapter,
    )
    .unwrap();

    assert_eq!(report.completed_command_ids.len(), 8);
    assert_eq!(report.completed_command_ids[0], "unknown:import");
    assert_eq!(report.completed_command_ids[7], "persistence:reopen-2");
    assert!(report.unknown_probe_exact_after_cycle_1);
    assert!(report.unknown_probe_exact_after_cycle_2);
    assert!(report.typed_state_exact_after_cycle_1);
    assert!(report.typed_state_exact_after_cycle_2);
    assert_eq!(report.untouched_annotation_count, 2);
    assert!(report.independent_validation_passed);
    assert!(report.independent_visual_validation_passed);
    assert_ne!(
        report.raster_oracle.source_crop_sha256, report.raster_oracle.cycle_1_crop_sha256,
        "the crop must prove that the saved editor state changed visible output"
    );
    assert_eq!(
        report.raster_oracle.cycle_1_crop_sha256, report.raster_oracle.cycle_2_crop_sha256,
        "the same fixed crop must remain pixel-exact after reopen/save cycle 2"
    );
    assert_eq!(report.cycle_1_sha256.len(), 64);
    assert_eq!(report.cycle_2_sha256.len(), 64);
    assert_eq!(report.validator_outputs.len(), 4);
    assert_eq!(report.validator_outputs[0].cycle, 1);
    assert_eq!(report.validator_outputs[0].command, "qpdf");
    assert_eq!(report.validator_outputs[3].cycle, 2);
    assert_eq!(report.validator_outputs[3].command, "pdfinfo");
    assert!(
        report
            .validator_outputs
            .iter()
            .all(|output| !output.stdout.is_empty() || !output.stderr.is_empty())
    );
    assert!(!adapter.is_dirty(71));
    let receipt = report.exact_receipt().unwrap();
    assert_eq!(receipt.status, "exact-passed");
    assert_eq!(receipt.completed_command_ids.len(), 8);
    assert!(receipt.typed_state_exact);
    assert!(receipt.unknown_probes_exact);
    assert!(receipt.independent_pdf_validation_passed);
    assert!(receipt.independent_visual_validation_passed);
    let mut incomplete = report.clone();
    incomplete.raster_oracle.cycle_2_crop_sha256 = "0".repeat(64);
    assert!(incomplete.exact_receipt().is_err());
}

#[test]
fn explicit_evidence_directory_retains_non_overwritten_pdf_and_raster_artifacts() {
    let scratch = tempfile::tempdir().unwrap();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let generator = manifest.join("../performance/fixture-oracle.mjs");
    let fixtures = scratch.path().join("fixtures");
    let generated = Command::new("node")
        .arg(generator)
        .arg("generate")
        .arg("--output")
        .arg(&fixtures)
        .output()
        .expect("Node must run the public fixture generator");
    assert!(generated.status.success());

    let mut adapter = AnnotationAdapter::default();
    EditorComparisonScenario::embedded()
        .unwrap()
        .execute(
            72,
            DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
            &mut adapter,
            &mut RecordingEditorObserver::default(),
        )
        .unwrap();
    let evidence = scratch.path().join("retained-evidence");
    let report = PersistenceComparisonScenario::execute_with_evidence_directory(
        &fixtures.join("bp-annotation-all-v1.pdf"),
        &evidence,
        72,
        &mut adapter,
    )
    .unwrap();

    let artifacts = report.retained_artifacts.as_ref().unwrap();
    assert_eq!(artifacts.cycle_1_pdf, evidence.join("cycle-1.pdf"));
    assert_eq!(artifacts.cycle_2_pdf, evidence.join("cycle-2.pdf"));
    for artifact in [
        &artifacts.cycle_1_pdf,
        &artifacts.cycle_2_pdf,
        &artifacts.source_crop,
        &artifacts.cycle_1_crop,
        &artifacts.cycle_2_crop,
    ] {
        assert!(
            artifact.is_file(),
            "missing retained artifact {}",
            artifact.display()
        );
    }

    let mut fresh_adapter = AnnotationAdapter::default();
    EditorComparisonScenario::embedded()
        .unwrap()
        .execute(
            73,
            DecodedRgbaAsset::new(512, 384, vec![0x80; 512 * 384 * 4]).unwrap(),
            &mut fresh_adapter,
            &mut RecordingEditorObserver::default(),
        )
        .unwrap();
    let error = PersistenceComparisonScenario::execute_with_evidence_directory(
        &fixtures.join("bp-annotation-all-v1.pdf"),
        &evidence,
        73,
        &mut fresh_adapter,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refusing to replace existing PDF")
    );
}
