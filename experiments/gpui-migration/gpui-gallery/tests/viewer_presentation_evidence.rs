use butter_paper_gpui_gallery::viewer_presentation_evidence::{
    DocumentOpenObservation, NativeShellObservation, VIEWER_LAUNCH_COMMAND_ID,
    VIEWER_OPEN_COMMAND_ID, build_viewer_launch_open_evidence,
};

fn shell() -> NativeShellObservation {
    NativeShellObservation {
        shell_render_entered: true,
        native_input_received: true,
        frame_callback_after_submission: true,
        input_latency_samples_before: 2,
        input_latency_samples_after: 3,
    }
}

fn open() -> DocumentOpenObservation {
    DocumentOpenObservation {
        requested_open_generation: 4,
        completed_open_generation: Some(4),
        requested_document_id: 7,
        active_document_id: Some(7),
        preview_document_id: Some(7),
        preview_generation: Some(5),
        pending_preview_generation: None,
        preview_available: true,
        settled_ms: 250.0,
    }
}

#[test]
fn proves_platform_draw_interactive_shell_and_current_settled_preview_without_claiming_scanout() {
    let report = build_viewer_launch_open_evidence(shell(), open()).unwrap();

    assert_eq!(report.launch.command_id, VIEWER_LAUNCH_COMMAND_ID);
    assert_eq!(
        report.launch.proven_manifest_milestones,
        [
            "process-started",
            "native-window-presented",
            "interactive-shell"
        ]
    );
    assert!(report.launch.gpui_platform_draw_submitted);
    assert!(!report.launch.physical_scanout_observed);
    assert_eq!(report.open.command_id, VIEWER_OPEN_COMMAND_ID);
    assert_eq!(
        report.open.proven_manifest_milestones,
        [
            "document-opened",
            "preview-current-generation",
            "settled-current-generation-250ms"
        ]
    );
}

#[test]
fn rejects_a_first_frame_callback_without_a_post_input_platform_draw_sample() {
    let mut observation = shell();
    observation.input_latency_samples_after = observation.input_latency_samples_before;
    assert!(
        build_viewer_launch_open_evidence(observation, open())
            .unwrap_err()
            .contains("platform draw")
    );
}

#[test]
fn rejects_stale_pending_or_under_settled_document_preview_state() {
    let mut observation = open();
    observation.pending_preview_generation = Some(6);
    assert!(
        build_viewer_launch_open_evidence(shell(), observation)
            .unwrap_err()
            .contains("current preview")
    );

    let mut observation = open();
    observation.settled_ms = 249.999;
    assert!(
        build_viewer_launch_open_evidence(shell(), observation)
            .unwrap_err()
            .contains("250 ms")
    );
}
