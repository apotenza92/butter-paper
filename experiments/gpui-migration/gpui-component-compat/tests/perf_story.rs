use std::{collections::BTreeMap, path::PathBuf};

use butter_paper_gpui_component_compat::perf_scenario::{PerfRunConfig, PerfRunConfigError};

fn exact_environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("BP_GPUI_PERF_SCENARIO".into(), "open-pdf".into()),
        ("BP_GPUI_PERF_ITERATION".into(), "3".into()),
        ("BP_GPUI_INPUT_LANE".into(), "native-x11-xtest".into()),
        (
            "BP_GPUI_FIXTURE_IDS".into(),
            r#"["bp-single-page-v1"]"#.into(),
        ),
        ("BP_GPUI_CACHE_DIR".into(), "/tmp/bp-perf-run-3".into()),
        (
            "BP_GPUI_COMPAT_PROFILE".into(),
            "longbridge-gpui-component-v1".into(),
        ),
        (
            "BP_GPUI_V4_MANIFEST_ID".into(),
            "bp-perf-v4-decision-1".into(),
        ),
        (
            "BP_PDF_WORKER_EXE".into(),
            "/candidate/bin/pdf-worker".into(),
        ),
        (
            "BP_PDFIUM_LIBRARY".into(),
            "/candidate/lib/libpdfium.so".into(),
        ),
    ])
}

#[test]
fn exact_open_story_config_binds_one_pdf_and_the_small_open_command() {
    let config = PerfRunConfig::parse(
        exact_environment(),
        [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
    )
    .unwrap();
    assert_eq!(config.scenario, "open-pdf");
    assert_eq!(config.iteration, 3);
    assert_eq!(config.input_lane, "native-x11-xtest");
    assert_eq!(config.compat_profile, "longbridge-gpui-component-v1");
    assert_eq!(config.v4_manifest_id, "bp-perf-v4-decision-1");
    assert_eq!(config.fixture_ids, ["bp-single-page-v1"]);
    assert_eq!(config.command_id, "small:open-settle");
    assert_eq!(
        config.pdfs,
        [PathBuf::from("/fixtures/bp-single-page-v1.pdf")]
    );
    assert_eq!(config.cache_directory, PathBuf::from("/tmp/bp-perf-run-3"));
    assert_eq!(
        config.worker_executable,
        PathBuf::from("/candidate/bin/pdf-worker")
    );
    assert_eq!(
        config.pdfium_library,
        PathBuf::from("/candidate/lib/libpdfium.so")
    );
}

#[test]
fn story_config_fails_closed_for_unknown_scenario_fixture_or_missing_runtime() {
    let mut unknown_scenario = exact_environment();
    unknown_scenario.insert("BP_GPUI_PERF_SCENARIO".into(), "zoom".into());
    assert_eq!(
        PerfRunConfig::parse(
            unknown_scenario,
            [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
        ),
        Err(PerfRunConfigError::UnsupportedScenario("zoom".into()))
    );

    let mut unknown_fixture = exact_environment();
    unknown_fixture.insert("BP_GPUI_FIXTURE_IDS".into(), r#"["unknown"]"#.into());
    assert_eq!(
        PerfRunConfig::parse(
            unknown_fixture,
            [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
        ),
        Err(PerfRunConfigError::UnsupportedFixture("unknown".into()))
    );

    let mut missing_worker = exact_environment();
    missing_worker.remove("BP_PDF_WORKER_EXE");
    assert_eq!(
        PerfRunConfig::parse(
            missing_worker,
            [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
        ),
        Err(PerfRunConfigError::Missing("BP_PDF_WORKER_EXE"))
    );

    let mut missing_parent_contract = exact_environment();
    missing_parent_contract.remove("BP_GPUI_V4_MANIFEST_ID");
    assert_eq!(
        PerfRunConfig::parse(
            missing_parent_contract,
            [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
        ),
        Err(PerfRunConfigError::Missing("BP_GPUI_V4_MANIFEST_ID"))
    );

    let mut moving_profile = exact_environment();
    moving_profile.insert("BP_GPUI_COMPAT_PROFILE".into(), "moving-main".into());
    assert_eq!(
        PerfRunConfig::parse(
            moving_profile,
            [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
        ),
        Err(PerfRunConfigError::UnsupportedCompatProfile(
            "moving-main".into()
        ))
    );
}

#[test]
fn native_x11_longbridge_profile_is_the_capture_signal_lane() {
    let native = PerfRunConfig::parse(
        exact_environment(),
        [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
    )
    .unwrap();
    assert!(native.requires_presented_crop_signal());

    let mut semantic_environment = exact_environment();
    semantic_environment.insert("BP_GPUI_INPUT_LANE".into(), "semantic-diagnostic".into());
    let semantic = PerfRunConfig::parse(
        semantic_environment,
        [PathBuf::from("/fixtures/bp-single-page-v1.pdf")],
    )
    .unwrap();
    assert!(!semantic.requires_presented_crop_signal());
}
