use butter_paper_gpui_component_compat::{
    document_workspace::{DocumentId, DocumentWorkspaceEvidenceSnapshot},
    perf_protocol::{PerfProtocol, PerfProtocolError, RecordingSink, fields},
    perf_scenario::{
        CaptureCleanupReason, CaptureOrchestrationCoordinator, LogicalBounds, LogicalSize,
        OpenPdfQualification, PageSizePoints, PresentedCropError, PresentedCropEvidence,
        PresentedCropEvidenceInput, PresentedCropHandshake, PresentedCropSignalDisposition,
        QualificationError, map_presented_crop_evidence, merge_presented_crop_open_events,
    },
};
use serde_json::{Value, json};

fn ready_snapshot(generation: u64) -> DocumentWorkspaceEvidenceSnapshot {
    DocumentWorkspaceEvidenceSnapshot {
        document_id: DocumentId::new(4),
        request_generation: generation,
        ready: true,
        failure: None,
        current_page: 0,
        requested_page: 0,
        page_count: 100,
        current_raster_width: 800,
        current_raster_height: 1035,
        current_raster_bytes: 3_312_000,
        current_raster_has_spatial_variation: true,
        rendered_device_pixel_ratio: Some(1.25),
        thumbnail_count: 100,
        worker_pid: Some(9_001),
        resource_present: true,
        viewer_generation: 3,
        viewer_tile_count: 1,
        viewer_cache_bytes: 3_312_000,
        annotation_revision: 0,
        annotation_dirty: false,
        presentation_error: None,
        recovery_pending: None,
    }
}

fn painted_crop_evidence() -> PresentedCropEvidence {
    PresentedCropEvidence {
        command_id: "viewer:open-each".into(),
        comparison_command_id: "small:open-settle".into(),
        fixture_id: "bp-single-page-v1".into(),
        page_id: "bp-single-page-v1:page:001".into(),
        page_size_points: PageSizePoints {
            width: 612.,
            height: 792.,
        },
        painted_outer_page_bounds_window_logical: LogicalBounds {
            x: 180.,
            y: 96.,
            width: 612.,
            height: 792.,
        },
        window_logical_size: LogicalSize {
            width: 1200.,
            height: 900.,
        },
        display_scale_factor: 1.,
        rendered_device_pixel_ratio: 1.,
        painted_request_generation: 7,
        painted_resource_generation: 11,
        painted_render_generation: 3,
        painted_state_sequence: 7,
    }
}

#[test]
fn protocol_emits_canonical_typed_json_through_an_injected_sink() {
    let sink = RecordingSink::default();
    let mut protocol = PerfProtocol::new("open-pdf", 42, sink);
    protocol
        .emit_at(
            "viewport-raster-completed",
            12.5,
            fields([
                ("pixel_width", json!(800)),
                ("surface_kind", json!("in-memory-bgra")),
            ]),
        )
        .unwrap();

    let lines = protocol.into_sink().into_lines();
    assert_eq!(lines.len(), 1);
    let event: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(event["schema_version"], 1);
    assert_eq!(event["runtime"], "gpui");
    assert_eq!(event["scenario"], "open-pdf");
    assert_eq!(event["event"], "viewport-raster-completed");
    assert_eq!(event["t_ms"], 12.5);
    assert_eq!(event["pid"], 42);
    assert_eq!(event["pixel_width"], 800);
    assert_eq!(event["surface_kind"], "in-memory-bgra");
}

#[test]
fn protocol_rejects_reserved_field_overrides_and_non_monotonic_time() {
    let sink = RecordingSink::default();
    let mut protocol = PerfProtocol::new("open-pdf", 42, sink);
    assert_eq!(
        protocol.emit_at(
            "first-frame",
            10.,
            fields([("event", json!("forged-terminal"))]),
        ),
        Err(PerfProtocolError::ReservedField("event".into()))
    );
    protocol
        .emit_at("first-frame", 10., Default::default())
        .unwrap();
    assert_eq!(
        protocol.emit_at("shell-ready", 9.999, Default::default()),
        Err(PerfProtocolError::NonMonotonicTime {
            previous_ms: 10.,
            actual_ms: 9.999,
        })
    );
    assert_eq!(
        protocol.emit_at("shell-ready", f64::NAN, Default::default()),
        Err(PerfProtocolError::InvalidTime)
    );
    assert_eq!(protocol.into_sink().into_lines().len(), 1);
}

#[test]
fn exact_open_requires_real_pixels_worker_identity_and_a_stable_250ms_generation() {
    let mut qualification = OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
    let first = qualification.observe(100., &ready_snapshot(7)).unwrap();
    assert_eq!(
        first.iter().map(|event| event.name).collect::<Vec<_>>(),
        vec!["pdf-open-completed", "viewport-raster-completed"]
    );
    assert!(
        qualification
            .observe(349.999, &ready_snapshot(7))
            .unwrap()
            .is_empty()
    );
    let mut changed_generation = ready_snapshot(7);
    changed_generation.viewer_generation = 4;
    assert!(
        qualification
            .observe(350., &changed_generation)
            .unwrap()
            .is_empty()
    );
    assert!(
        qualification
            .observe(599.999, &changed_generation)
            .unwrap()
            .is_empty()
    );
    let settled = qualification.observe(600., &changed_generation).unwrap();
    assert_eq!(
        settled.iter().map(|event| event.name).collect::<Vec<_>>(),
        vec!["viewer-generation-settled"]
    );
    assert!(!qualification.is_complete());
    let presented = qualification
        .confirm_presented(601., &changed_generation)
        .unwrap();
    assert_eq!(
        presented.iter().map(|event| event.name).collect::<Vec<_>>(),
        vec!["viewer-native-open-evidence", "comparison-command-complete"]
    );
    assert_eq!(presented[0].fields["command_id"], "viewer:open-each");
    assert_eq!(presented[0].fields["settled_current_generation_ms"], 250.);
    assert_eq!(presented[1].fields["command_id"], "small:open-settle");
    assert_eq!(first[1].fields["rendered_device_pixel_ratio"], 1.25);
    assert!(!qualification.is_complete());
    let cleaned = qualification.confirm_cleanup(true, true).unwrap();
    assert_eq!(
        cleaned.iter().map(|event| event.name).collect::<Vec<_>>(),
        vec!["resource-cleanup-complete", "scenario-complete"]
    );
    assert!(qualification.is_complete());
    assert_eq!(
        qualification.observe(351., &ready_snapshot(7)),
        Err(QualificationError::AlreadyComplete)
    );
}

#[test]
fn exact_open_rejects_a_different_generation_at_presentation_and_incomplete_cleanup() {
    let mut qualification = OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
    qualification.observe(100., &ready_snapshot(7)).unwrap();
    qualification.observe(350., &ready_snapshot(7)).unwrap();
    let mut changed = ready_snapshot(7);
    changed.viewer_generation = 9;
    assert_eq!(
        qualification.confirm_presented(351., &changed),
        Err(QualificationError::PresentedGenerationChanged {
            expected: 3,
            actual: 9,
        })
    );
    qualification
        .confirm_presented(351., &ready_snapshot(7))
        .unwrap();
    assert_eq!(
        qualification.confirm_cleanup(false, true),
        Err(QualificationError::CleanupIncomplete)
    );
}

#[test]
fn presentation_revalidates_the_complete_settled_snapshot_authority() {
    let rejected = [
        (
            {
                let mut snapshot = ready_snapshot(7);
                snapshot.document_id = DocumentId::new(5);
                snapshot
            },
            QualificationError::WrongDocument {
                expected: DocumentId::new(4),
                actual: DocumentId::new(5),
            },
        ),
        (
            ready_snapshot(8),
            QualificationError::StaleGeneration {
                expected: 7,
                actual: 8,
            },
        ),
        (
            {
                let mut snapshot = ready_snapshot(7);
                snapshot.ready = false;
                snapshot
            },
            QualificationError::DocumentNotReady,
        ),
        (
            {
                let mut snapshot = ready_snapshot(7);
                snapshot.resource_present = false;
                snapshot
            },
            QualificationError::MissingWorker,
        ),
        (
            {
                let mut snapshot = ready_snapshot(7);
                snapshot.current_raster_has_spatial_variation = false;
                snapshot
            },
            QualificationError::MissingRealPixels,
        ),
    ];

    for (snapshot, expected) in rejected {
        let mut qualification =
            OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
        qualification.observe(100., &ready_snapshot(7)).unwrap();
        qualification.observe(350., &ready_snapshot(7)).unwrap();
        assert_eq!(
            qualification.confirm_presented(351., &snapshot),
            Err(expected)
        );
    }
}

#[test]
fn exact_open_fails_closed_for_stale_generation_or_missing_real_resources() {
    let mut qualification = OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
    assert_eq!(
        qualification.observe(1., &ready_snapshot(8)),
        Err(QualificationError::StaleGeneration {
            expected: 7,
            actual: 8,
        })
    );

    let mut missing_worker = ready_snapshot(7);
    missing_worker.worker_pid = None;
    assert_eq!(
        OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle")
            .observe(1., &missing_worker),
        Err(QualificationError::MissingWorker)
    );

    let mut uniform = ready_snapshot(7);
    uniform.current_raster_has_spatial_variation = false;
    assert_eq!(
        OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle").observe(1., &uniform),
        Err(QualificationError::MissingRealPixels)
    );
}

#[test]
fn presented_crop_requires_signal_then_next_frame_before_cleanup() {
    let evidence = painted_crop_evidence();
    let mut handshake = PresentedCropHandshake::new();

    assert_eq!(
        handshake.authorize_cleanup(),
        Err(PresentedCropError::PresentationNotConfirmed)
    );
    let open = handshake.arm(evidence.clone()).unwrap();
    assert_eq!(open.name, "viewer-native-open-evidence");
    assert_eq!(open.fields["fixture_id"], "bp-single-page-v1");
    assert_eq!(open.fields["page_size_points"]["width"], 612.);
    assert_eq!(
        handshake.observe_signal(false),
        Ok(PresentedCropSignalDisposition::NoSignal)
    );
    assert_eq!(
        handshake.confirm_next_frame(&evidence),
        Err(PresentedCropError::FrameNotRequested)
    );
    assert_eq!(
        handshake.observe_signal(true),
        Ok(PresentedCropSignalDisposition::ScheduleNextFrame)
    );
    assert_eq!(
        handshake.observe_signal(true),
        Err(PresentedCropError::DuplicateSignal)
    );
    assert_eq!(
        handshake.authorize_cleanup(),
        Err(PresentedCropError::PresentationNotConfirmed)
    );

    let presented = handshake.confirm_next_frame(&evidence).unwrap();
    assert_eq!(presented.name, "viewer-native-presented-state");
    assert_eq!(presented.fields, open.fields);
    assert_eq!(
        handshake.confirm_next_frame(&evidence),
        Err(PresentedCropError::AlreadyConfirmed)
    );
    handshake.authorize_cleanup().unwrap();
    assert!(handshake.is_cleanup_authorized());
    assert_eq!(
        handshake.authorize_cleanup(),
        Err(PresentedCropError::CleanupAlreadyAuthorized)
    );
}

#[test]
fn presented_crop_rejects_every_frozen_painted_field_drift() {
    let evidence = painted_crop_evidence();
    let mut handshake = PresentedCropHandshake::new();
    handshake.arm(evidence.clone()).unwrap();
    handshake.observe_signal(true).unwrap();

    let mut drifted = Vec::new();
    let mut candidate = evidence.clone();
    candidate.command_id = "viewer:other".into();
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.comparison_command_id = "small:other".into();
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.fixture_id = "other-fixture".into();
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.page_id = "other-page".into();
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.page_size_points.width += 1.;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.painted_outer_page_bounds_window_logical.x += 1.;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.window_logical_size.width += 1.;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.display_scale_factor = 2.;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.rendered_device_pixel_ratio = 2.;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.painted_request_generation += 1;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.painted_resource_generation += 1;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.painted_render_generation += 1;
    drifted.push(candidate);
    let mut candidate = evidence.clone();
    candidate.painted_state_sequence += 1;
    drifted.push(candidate);

    for candidate in drifted {
        assert_eq!(
            handshake.confirm_next_frame(&candidate),
            Err(PresentedCropError::PaintedEvidenceDrift)
        );
    }
    assert!(!handshake.is_cleanup_authorized());
}

#[test]
fn presented_crop_rejects_invalid_or_non_frozen_open_evidence() {
    let mut handshake = PresentedCropHandshake::new();
    let mut wrong_fixture = painted_crop_evidence();
    wrong_fixture.fixture_id = "not-the-locked-fixture".into();
    assert_eq!(
        handshake.arm(wrong_fixture),
        Err(PresentedCropError::InvalidEvidence("fixture_id"))
    );

    let mut invalid_bounds = painted_crop_evidence();
    invalid_bounds
        .painted_outer_page_bounds_window_logical
        .width = f64::NAN;
    assert_eq!(
        handshake.arm(invalid_bounds),
        Err(PresentedCropError::InvalidEvidence(
            "painted_outer_page_bounds_window_logical"
        ))
    );
    assert_eq!(
        handshake.observe_signal(true),
        Err(PresentedCropError::NotArmed)
    );
}

#[test]
fn presented_crop_merges_into_exactly_one_qualified_open_event() {
    let mut qualification = OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
    qualification.observe(100., &ready_snapshot(7)).unwrap();
    qualification.observe(350., &ready_snapshot(7)).unwrap();
    let qualified = qualification
        .confirm_presented(351., &ready_snapshot(7))
        .unwrap();
    let mut handshake = PresentedCropHandshake::new();
    let crop = handshake.arm(painted_crop_evidence()).unwrap();

    let merged = merge_presented_crop_open_events(qualified, crop).unwrap();
    assert_eq!(
        merged
            .iter()
            .filter(|event| event.name == "viewer-native-open-evidence")
            .count(),
        1
    );
    assert_eq!(
        merged.iter().map(|event| event.name).collect::<Vec<_>>(),
        vec!["viewer-native-open-evidence", "comparison-command-complete"]
    );
    let open = &merged[0].fields;
    assert_eq!(open["document_opened"], true);
    assert_eq!(open["settled_current_generation_ms"], 250.);
    assert_eq!(open["fixture_id"], "bp-single-page-v1");
    assert_eq!(open["painted_request_generation"], 7);
    assert_eq!(open["painted_resource_generation"], 11);
    assert_eq!(open["painted_render_generation"], 3);
}

#[test]
fn presented_crop_merge_rejects_cross_authority_generation_or_command_drift() {
    let qualified_events = || {
        let mut qualification =
            OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
        qualification.observe(100., &ready_snapshot(7)).unwrap();
        qualification.observe(350., &ready_snapshot(7)).unwrap();
        qualification
            .confirm_presented(351., &ready_snapshot(7))
            .unwrap()
    };
    let crop_event =
        |evidence: PresentedCropEvidence| PresentedCropHandshake::new().arm(evidence).unwrap();

    let mut mismatched_completion = qualified_events();
    mismatched_completion[0]
        .fields
        .insert("completed_open_generation".into(), json!(8));
    assert_eq!(
        merge_presented_crop_open_events(
            mismatched_completion,
            crop_event(painted_crop_evidence())
        ),
        Err(PresentedCropError::InvariantViolation(
            "requested_open_generation"
        ))
    );

    let mut wrong_request = painted_crop_evidence();
    wrong_request.painted_request_generation = 8;
    assert_eq!(
        merge_presented_crop_open_events(qualified_events(), crop_event(wrong_request)),
        Err(PresentedCropError::InvariantViolation(
            "painted_request_generation"
        ))
    );

    let mut wrong_render = painted_crop_evidence();
    wrong_render.painted_render_generation = 4;
    assert_eq!(
        merge_presented_crop_open_events(qualified_events(), crop_event(wrong_render)),
        Err(PresentedCropError::InvariantViolation(
            "painted_render_generation"
        ))
    );

    let mut wrong_command = crop_event(painted_crop_evidence());
    wrong_command
        .fields
        .insert("command_id".into(), json!("viewer:other"));
    assert_eq!(
        merge_presented_crop_open_events(qualified_events(), wrong_command),
        Err(PresentedCropError::InvariantViolation("command_id"))
    );

    let mut wrong_completion_command = qualified_events();
    wrong_completion_command[1]
        .fields
        .insert("command_id".into(), json!("small:other"));
    assert_eq!(
        merge_presented_crop_open_events(
            wrong_completion_command,
            crop_event(painted_crop_evidence())
        ),
        Err(PresentedCropError::InvariantViolation(
            "comparison-command-complete.command_id"
        ))
    );
}

#[test]
fn pure_capture_coordinator_maps_authority_and_schedules_exactly_one_frame() {
    let evidence = map_presented_crop_evidence(PresentedCropEvidenceInput {
        comparison_command_id: "small:open-settle".into(),
        fixture_id: "bp-single-page-v1".into(),
        page_index: 0,
        page_size_points: PageSizePoints {
            width: 612.,
            height: 792.,
        },
        painted_outer_page_bounds_window_logical: LogicalBounds {
            x: 180.,
            y: 96.,
            width: 612.,
            height: 792.,
        },
        window_logical_size: LogicalSize {
            width: 1200.,
            height: 900.,
        },
        display_scale_factor: 1.,
        rendered_device_pixel_ratio: 1.,
        painted_request_generation: 7,
        painted_resource_generation: 11,
        painted_render_generation: 3,
        painted_state_sequence: 7,
    })
    .unwrap();
    assert_eq!(evidence.page_id, "bp-single-page-v1:page:001");

    let mut coordinator = CaptureOrchestrationCoordinator::new();
    coordinator.arm(evidence.clone()).unwrap();
    assert_eq!(coordinator.cleanup_reason(), None);
    assert_eq!(
        coordinator.observe_signal(true),
        Ok(PresentedCropSignalDisposition::ScheduleNextFrame)
    );
    assert!(coordinator.take_next_frame_request());
    assert!(!coordinator.take_next_frame_request());
    assert_eq!(coordinator.cleanup_reason(), None);

    let mut drifted = evidence.clone();
    drifted.painted_state_sequence += 1;
    assert_eq!(
        coordinator.confirm_next_frame(&drifted),
        Err(PresentedCropError::PaintedEvidenceDrift)
    );
    assert_eq!(coordinator.cleanup_reason(), None);

    let presented = coordinator.confirm_next_frame(&evidence).unwrap();
    assert_eq!(presented.name, "viewer-native-presented-state");
    assert_eq!(coordinator.cleanup_reason(), None);
    coordinator.authorize_success_cleanup().unwrap();
    assert_eq!(
        coordinator.cleanup_reason(),
        Some(CaptureCleanupReason::QualifiedSuccess)
    );
}

#[test]
fn capture_failure_requires_cleanup_without_success_qualification() {
    let mut coordinator = CaptureOrchestrationCoordinator::new();
    coordinator.arm(painted_crop_evidence()).unwrap();
    coordinator.observe_signal(true).unwrap();
    assert!(coordinator.take_next_frame_request());

    coordinator.begin_failure_cleanup();
    assert_eq!(
        coordinator.cleanup_reason(),
        Some(CaptureCleanupReason::Failure)
    );
    assert!(!coordinator.take_next_frame_request());
    assert_eq!(
        coordinator.complete_resource_cleanup(),
        Ok(CaptureCleanupReason::Failure)
    );
    assert!(!coordinator.success_qualification_allowed());
}

#[test]
fn presented_crop_open_merge_rejects_conflicting_fields() {
    let mut qualification = OpenPdfQualification::new(DocumentId::new(4), 7, "small:open-settle");
    qualification.observe(100., &ready_snapshot(7)).unwrap();
    qualification.observe(350., &ready_snapshot(7)).unwrap();
    let qualified = qualification
        .confirm_presented(351., &ready_snapshot(7))
        .unwrap();
    let mut handshake = PresentedCropHandshake::new();
    let mut crop = handshake.arm(painted_crop_evidence()).unwrap();
    crop.fields.insert("document_opened".into(), json!(false));

    assert_eq!(
        merge_presented_crop_open_events(qualified, crop),
        Err(PresentedCropError::ConflictingOpenField(
            "document_opened".into()
        ))
    );
}
