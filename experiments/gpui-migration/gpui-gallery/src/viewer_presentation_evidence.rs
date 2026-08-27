//! Truthful native launch and initial-document evidence for the GPUI viewer.

use serde::Serialize;

pub const VIEWER_LAUNCH_COMMAND_ID: &str = "viewer:launch-cold";
pub const VIEWER_OPEN_COMMAND_ID: &str = "viewer:open-each";

#[derive(Clone, Copy, Debug)]
pub struct NativeShellObservation {
    pub shell_render_entered: bool,
    pub native_input_received: bool,
    pub frame_callback_after_submission: bool,
    pub input_latency_samples_before: u64,
    pub input_latency_samples_after: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeShellPresentationProbe {
    input_latency_samples_before: u64,
    post_input_frame_callback_seen: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum NativeShellProbeProgress {
    AwaitingPostPresentSample(NativeShellPresentationProbe),
    Complete(NativeShellObservation),
}

impl NativeShellPresentationProbe {
    pub fn new(input_latency_samples_before: u64) -> Self {
        Self {
            input_latency_samples_before,
            post_input_frame_callback_seen: false,
        }
    }

    pub fn observe_frame_callback(
        mut self,
        shell_render_entered: bool,
        input_latency_samples_after: u64,
    ) -> Result<NativeShellProbeProgress, String> {
        // GPUI CE runs `on_next_frame` callbacks before it draws and presents
        // that frame. The first callback therefore cannot observe the input
        // latency sample produced by the pending presentation. Use it only to
        // request another frame, then sample in the following callback.
        if !self.post_input_frame_callback_seen {
            self.post_input_frame_callback_seen = true;
            return Ok(NativeShellProbeProgress::AwaitingPostPresentSample(self));
        }
        if input_latency_samples_after <= self.input_latency_samples_before {
            return Err(
                "native shell input did not produce a GPUI platform draw submission sample".into(),
            );
        }
        Ok(NativeShellProbeProgress::Complete(NativeShellObservation {
            shell_render_entered,
            native_input_received: true,
            frame_callback_after_submission: true,
            input_latency_samples_before: self.input_latency_samples_before,
            input_latency_samples_after,
        }))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DocumentOpenObservation {
    pub requested_open_generation: u64,
    pub completed_open_generation: Option<u64>,
    pub requested_document_id: u64,
    pub active_document_id: Option<u64>,
    pub preview_document_id: Option<u64>,
    pub preview_generation: Option<u64>,
    pub pending_preview_generation: Option<u64>,
    pub preview_available: bool,
    pub settled_ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ViewerCommandEvidence {
    pub command_id: &'static str,
    pub proven_manifest_milestones: Vec<&'static str>,
    pub gpui_platform_draw_submitted: bool,
    pub physical_scanout_observed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ViewerLaunchOpenEvidence {
    pub launch: ViewerCommandEvidence,
    pub open: ViewerCommandEvidence,
}

pub fn build_viewer_launch_open_evidence(
    shell: NativeShellObservation,
    open: DocumentOpenObservation,
) -> Result<ViewerLaunchOpenEvidence, String> {
    if !shell.shell_render_entered
        || !shell.native_input_received
        || !shell.frame_callback_after_submission
        || shell.input_latency_samples_after <= shell.input_latency_samples_before
    {
        return Err(
            "native shell did not produce a post-input GPUI platform draw submission".into(),
        );
    }
    if open.completed_open_generation != Some(open.requested_open_generation)
        || open.active_document_id != Some(open.requested_document_id)
        || open.preview_document_id != Some(open.requested_document_id)
        || open.preview_generation.is_none()
        || open.pending_preview_generation.is_some()
        || !open.preview_available
    {
        return Err("document open did not reach the current preview generation".into());
    }
    if !open.settled_ms.is_finite() || open.settled_ms < 250.0 {
        return Err("current document preview did not remain settled for 250 ms".into());
    }

    Ok(ViewerLaunchOpenEvidence {
        launch: ViewerCommandEvidence {
            command_id: VIEWER_LAUNCH_COMMAND_ID,
            proven_manifest_milestones: vec![
                "process-started",
                "native-window-presented",
                "interactive-shell",
            ],
            gpui_platform_draw_submitted: true,
            physical_scanout_observed: false,
        },
        open: ViewerCommandEvidence {
            command_id: VIEWER_OPEN_COMMAND_ID,
            proven_manifest_milestones: vec![
                "document-opened",
                "preview-current-generation",
                "settled-current-generation-250ms",
            ],
            gpui_platform_draw_submitted: false,
            physical_scanout_observed: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{NativeShellPresentationProbe, NativeShellProbeProgress};

    #[test]
    fn shell_probe_waits_one_callback_before_accepting_the_presented_input_sample() {
        let probe = NativeShellPresentationProbe::new(2);
        let NativeShellProbeProgress::AwaitingPostPresentSample(probe) = probe
            .observe_frame_callback(true, 2)
            .expect("the pre-presentation callback must not fail early")
        else {
            panic!("the first callback must await the pending presentation");
        };
        let NativeShellProbeProgress::Complete(observation) = probe
            .observe_frame_callback(true, 3)
            .expect("the following callback must accept an advanced sample")
        else {
            panic!("the second callback must finish the probe");
        };
        assert!(observation.shell_render_entered);
        assert!(observation.native_input_received);
        assert!(observation.frame_callback_after_submission);
        assert_eq!(observation.input_latency_samples_before, 2);
        assert_eq!(observation.input_latency_samples_after, 3);
    }

    #[test]
    fn shell_probe_rejects_a_missing_sample_after_the_presentation_opportunity() {
        let probe = NativeShellPresentationProbe::new(2);
        let NativeShellProbeProgress::AwaitingPostPresentSample(probe) = probe
            .observe_frame_callback(true, 2)
            .expect("the first callback is before presentation")
        else {
            panic!("the first callback must await presentation");
        };
        let error = probe
            .observe_frame_callback(true, 2)
            .expect_err("the second callback must fail closed without a new sample");
        assert_eq!(
            error,
            "native shell input did not produce a GPUI platform draw submission sample"
        );
    }
}
