use butter_paper_gpui_component_compat::application_close_workspace::{
    ApplicationCloseCheckpointPublisher, ApplicationCloseShell, ApplicationCloseWorkspace,
    register_application_close_action,
};
use butter_paper_gpui_component_compat::document_tab_bar::TemplateCatalogItem;
use butter_paper_gpui_component_compat::document_workspace::{
    DocumentId, DocumentWorkspace, DocumentWorkspaceEvidenceSnapshot,
    DocumentWorkspaceTemplateCommand, PaintedPageEvidence, PdfDocumentSaver, PdfiumWorkerBackend,
    init_document_workspace_actions, register_document_workspace_global_actions,
};
use butter_paper_gpui_component_compat::native_application::{
    NativeApplicationMenuState, NativeDocumentIngress, install_native_application_menus,
};
use butter_paper_gpui_component_compat::native_launch::{
    NativeLaunchAction, NativeLaunchConfig, NativeLaunchSessionSource, NativeLaunchWarning,
};
use butter_paper_gpui_component_compat::native_runtime_layout::{
    NativeRuntimeLayout, NativeRuntimeMode, require_explicit_development_authority,
};
use butter_paper_gpui_component_compat::perf_capture_signal::{
    CaptureSignalError, CaptureSignalGuard,
};
use butter_paper_gpui_component_compat::perf_protocol::{PerfProtocol, StdoutSink, fields};
use butter_paper_gpui_component_compat::perf_scenario::{
    CaptureCleanupReason, CaptureOrchestrationCoordinator, LogicalBounds, LogicalSize,
    OpenPdfQualification, PageSizePoints, PerfRunConfig, PresentedCropEvidence,
    PresentedCropEvidenceInput, PresentedCropSignalDisposition, QualificationError,
    map_presented_crop_evidence, merge_presented_crop_open_events,
};
use butter_paper_gpui_component_compat::session_manifest::SessionManifestStore;
use butter_paper_gpui_component_compat::system_theme::follow_window_appearance;
use butter_paper_gpui_component_compat::template_manager::{
    TemplateManagerView, legacy_blank_request_from_json, route_workspace_template_command,
};
use butter_paper_gpui_gallery::generated_document::GeneratedDocumentStore;
use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{ActiveTheme as _, Root, WindowExt as _, menu::AppMenuBar, v_flex};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct PerfCleanupProbe {
    worker_pid: Option<u32>,
    deadline_ms: f64,
    reason: CaptureCleanupReason,
}

struct PerfStoryRuntime {
    config: PerfRunConfig,
    origin: Instant,
    protocol: PerfProtocol<StdoutSink>,
    document_id: Option<DocumentId>,
    worker_pid: Option<u32>,
    qualification: Option<OpenPdfQualification>,
    first_render_entered: bool,
    first_frame_observed: bool,
    launch_completed: bool,
    #[cfg(feature = "benchmark-evidence")]
    launch_input_samples_before: Option<u64>,
    presentation_pending: bool,
    presentation_callback_scheduled: bool,
    capture_signal: Option<CaptureSignalGuard>,
    capture: CaptureOrchestrationCoordinator,
    cleanup_probe: Option<PerfCleanupProbe>,
    failed: bool,
}

impl PerfStoryRuntime {
    fn new(config: PerfRunConfig) -> Result<Self, CaptureSignalError> {
        let capture_signal = if config.requires_presented_crop_signal() {
            Some(CaptureSignalGuard::install()?)
        } else {
            None
        };
        let origin = Instant::now();
        let mut protocol =
            PerfProtocol::new(config.scenario.clone(), std::process::id(), StdoutSink);
        protocol
            .emit_at("process-start", 0., Default::default())
            .unwrap();
        protocol
            .emit_at("process-main-enter", 0., Default::default())
            .unwrap();
        protocol
            .emit_at("open-window-requested", 0., Default::default())
            .unwrap();
        Ok(Self {
            config,
            origin,
            protocol,
            document_id: None,
            worker_pid: None,
            qualification: None,
            first_render_entered: false,
            first_frame_observed: false,
            launch_completed: false,
            #[cfg(feature = "benchmark-evidence")]
            launch_input_samples_before: None,
            presentation_pending: false,
            presentation_callback_scheduled: false,
            capture_signal,
            capture: CaptureOrchestrationCoordinator::new(),
            cleanup_probe: None,
            failed: false,
        })
    }

    fn elapsed_ms(&self) -> f64 {
        self.origin.elapsed().as_secs_f64() * 1_000.
    }

    fn emit(&mut self, event: &str, details: serde_json::Map<String, serde_json::Value>) {
        self.protocol
            .emit_at(event, self.elapsed_ms(), details)
            .expect("performance events must preserve protocol-owned fields and time order");
    }

    fn fail(&mut self, error: impl Into<String>) {
        if self.failed {
            return;
        }
        self.failed = true;
        self.emit("scenario-failed", fields([("error", json!(error.into()))]));
    }
}

struct ComponentStory {
    document_workspace: Entity<DocumentWorkspace>,
    app_menu_bar: Entity<AppMenuBar>,
    template_manager: Option<Entity<TemplateManagerView>>,
    _template_command_subscription: Option<Subscription>,
    _system_theme_subscription: Subscription,
    last_native_menu_state: Option<NativeApplicationMenuState>,
    has_focused_input: bool,
    perf: Option<PerfStoryRuntime>,
}

impl ComponentStory {
    fn new(
        document_workspace: Entity<DocumentWorkspace>,
        app_menu_bar: Entity<AppMenuBar>,
        template_manager: Option<Entity<TemplateManagerView>>,
        perf: Option<PerfStoryRuntime>,
        system_theme_subscription: Subscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&document_workspace, |story, _, cx| {
            story.sync_native_application_menu(cx);
            story.sync_template_operation_state(cx);
        })
        .detach();
        if template_manager.is_some() {
            document_workspace.update(cx, |workspace, _| {
                workspace.use_external_template_authority(true);
            });
        }
        let template_command_subscription = template_manager.as_ref().map(|_| {
            cx.subscribe_in(
                &document_workspace,
                window,
                |story, _, event: &DocumentWorkspaceTemplateCommand, window, cx| {
                    let Some(manager) = story.template_manager.as_ref() else {
                        return;
                    };
                    route_workspace_template_command(manager, event, window, cx);
                },
            )
        });
        if let Some(manager) = template_manager.as_ref() {
            cx.observe(manager, |story, _, cx| story.sync_template_catalog(cx))
                .detach();
        }
        let mut story = Self {
            document_workspace,
            app_menu_bar,
            template_manager,
            _template_command_subscription: template_command_subscription,
            _system_theme_subscription: system_theme_subscription,
            last_native_menu_state: None,
            has_focused_input: false,
            perf,
        };
        story.sync_native_application_menu(cx);
        story.sync_template_catalog(cx);
        story
    }

    fn sync_template_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(manager) = self.template_manager.as_ref() else {
            return;
        };
        let (templates, last_used_id, storage_busy) = {
            let manager = manager.read(cx);
            let model = manager.model();
            (
                model
                    .records()
                    .iter()
                    .map(|record| TemplateCatalogItem::new(record.id(), record.name()))
                    .collect(),
                model.last_used_id().to_owned(),
                manager.is_storage_busy(),
            )
        };
        self.document_workspace.update(cx, |workspace, cx| {
            workspace.apply_template_catalog(templates, last_used_id, cx);
            workspace.set_template_operation_state(storage_busy, cx);
        });
    }

    fn sync_template_operation_state(&mut self, cx: &mut Context<Self>) {
        let storage_busy = self
            .template_manager
            .as_ref()
            .is_some_and(|manager| manager.read(cx).is_storage_busy());
        self.document_workspace.update(cx, |workspace, cx| {
            workspace.set_template_operation_state(storage_busy, cx);
        });
    }

    fn begin_perf_open(&mut self, cx: &mut Context<Self>) {
        let Some(perf) = self.perf.as_ref() else {
            return;
        };
        if perf.document_id.is_some() || perf.failed {
            return;
        }
        let path = perf.config.pdfs[0].clone();
        let document_id = self
            .document_workspace
            .update(cx, |workspace, cx| workspace.open_path(path, cx));
        self.perf.as_mut().expect("runtime exists").document_id = Some(document_id);
        let Some(snapshot) = self
            .document_workspace
            .read(cx)
            .evidence_snapshot(document_id, cx)
        else {
            self.fail_perf_and_begin_cleanup(
                "the opened document did not create a stable session",
                cx,
            );
            return;
        };
        let perf = self.perf.as_mut().expect("runtime exists");
        perf.worker_pid = snapshot.worker_pid;
        perf.emit(
            "pdf-open-requested",
            fields([
                ("document_id", json!(document_id.value())),
                ("generation", json!(snapshot.request_generation)),
            ]),
        );
        perf.qualification = Some(OpenPdfQualification::new(
            document_id,
            snapshot.request_generation,
            perf.config.command_id.clone(),
        ));
    }

    fn restore_perf_capture_signal(&mut self) -> Option<String> {
        let signal = self
            .perf
            .as_mut()
            .and_then(|perf| perf.capture_signal.take());
        signal.and_then(|signal| {
            signal
                .restore()
                .err()
                .map(|error| format!("capture signal restoration failed: {error}"))
        })
    }

    fn fail_perf_and_begin_cleanup(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        let error = error.into();
        let Some(perf) = self.perf.as_mut() else {
            return;
        };
        perf.fail(error);
        if perf.cleanup_probe.is_some() {
            return;
        }
        perf.capture.begin_failure_cleanup();
        let document_id = perf.document_id;
        let worker_pid = perf.worker_pid;
        let now_ms = perf.elapsed_ms();
        if let Some(restoration_error) = self.restore_perf_capture_signal() {
            self.perf.as_mut().expect("runtime exists").emit(
                "capture-signal-restoration-failed",
                fields([("error", json!(restoration_error))]),
            );
        }
        if let Some(document_id) = document_id {
            let _ = self.document_workspace.update(cx, |workspace, cx| {
                workspace.close_document(document_id, cx)
            });
        }
        let perf = self.perf.as_mut().expect("runtime exists");
        perf.emit(
            "failure-resource-cleanup-requested",
            fields([
                ("document_id", json!(document_id.map(DocumentId::value))),
                ("worker_pid", json!(worker_pid)),
            ]),
        );
        perf.cleanup_probe = Some(PerfCleanupProbe {
            worker_pid,
            deadline_ms: now_ms + 5_000.,
            reason: CaptureCleanupReason::Failure,
        });
    }

    #[cfg(feature = "benchmark-evidence")]
    fn complete_perf_launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(perf) = self.perf.as_mut() else {
            return;
        };
        if perf.launch_completed || perf.failed {
            return;
        }
        let Some(input_latency_samples_before) = perf.launch_input_samples_before else {
            perf.fail("native launch input arrived before the GPUI latency baseline");
            return;
        };
        let snapshot = window.input_latency_snapshot();
        let input_latency_samples_after = snapshot.latency_histogram.len();
        if input_latency_samples_after <= input_latency_samples_before {
            perf.fail("the native input did not produce a GPUI input-to-draw sample");
            return;
        }
        perf.launch_completed = true;
        perf.emit(
            "viewer-native-launch-evidence",
            fields([
                ("command_id", json!("viewer:launch-cold")),
                ("native_input_observed", json!(true)),
                ("input_api", json!("XTEST-pointer")),
                ("input_latency_samples_before", json!(input_latency_samples_before)),
                ("input_latency_samples_after", json!(input_latency_samples_after)),
                (
                    "input_to_application_draw_ack_p50_ns",
                    json!(snapshot.latency_histogram.value_at_quantile(0.5)),
                ),
                (
                    "input_to_application_draw_ack_p95_ns",
                    json!(snapshot.latency_histogram.value_at_quantile(0.95)),
                ),
                (
                    "receipt_scope",
                    json!("gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout"),
                ),
                ("gpui_platform_draw_submitted", json!(true)),
                ("interactive_shell", json!(true)),
                ("physical_scanout_observed", json!(false)),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        perf.emit(
            "comparison-command-complete",
            fields([("command_id", json!("small:launch-cold"))]),
        );
        self.begin_perf_open(cx);
        window.refresh();
        cx.notify();
    }

    fn complete_semantic_launch_diagnostic(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(perf) = self.perf.as_mut() else {
            return;
        };
        if perf.launch_completed || perf.failed {
            return;
        }
        perf.launch_completed = true;
        perf.emit(
            "viewer-semantic-launch-diagnostic",
            fields([
                ("command_id", json!("small:launch-cold")),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        perf.emit(
            "comparison-command-complete",
            fields([("command_id", json!("small:launch-cold"))]),
        );
        self.begin_perf_open(cx);
        window.refresh();
        cx.notify();
    }

    fn observe_perf_activation(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(perf) = self.perf.as_ref() else {
            return;
        };
        if perf.config.input_lane != "native-x11-xtest" || perf.launch_completed {
            return;
        }
        #[cfg(not(feature = "benchmark-evidence"))]
        {
            self.perf
                .as_mut()
                .expect("performance runtime exists")
                .fail("native X11 evidence requires the benchmark-evidence feature");
            cx.notify();
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        {
            let story = cx.entity().downgrade();
            _window.on_next_frame(move |window, _cx| {
                let story_after_present = story.clone();
                window.on_next_frame(move |window, cx| {
                    let _ = story_after_present.update(cx, |story, cx| {
                        story.complete_perf_launch(window, cx);
                    });
                });
                window.refresh();
            });
            _window.refresh();
            cx.notify();
        }
    }

    fn observe_perf_document(&mut self, cx: &mut Context<Self>) -> bool {
        if self.perf.is_none() {
            return true;
        }
        let cleanup_probe = self
            .perf
            .as_ref()
            .expect("runtime presence was checked")
            .cleanup_probe;
        if let Some(cleanup) = cleanup_probe {
            let worker_exited = cleanup
                .worker_pid
                .is_none_or(|pid| !process_is_running(pid));
            let surface_root = self
                .perf
                .as_ref()
                .expect("runtime exists")
                .config
                .cache_directory
                .join("document-workspace");
            let surfaces_released = directory_is_absent_or_empty(&surface_root);
            if worker_exited && surfaces_released {
                let perf = self.perf.as_mut().expect("runtime exists");
                let reason = perf
                    .capture
                    .complete_resource_cleanup()
                    .expect("a cleanup probe must have an authorized reason");
                debug_assert_eq!(reason, cleanup.reason);
                if reason == CaptureCleanupReason::QualifiedSuccess {
                    let events = perf
                        .qualification
                        .as_mut()
                        .expect("successful cleanup follows qualification")
                        .confirm_cleanup(true, true)
                        .expect("verified cleanup must complete qualification");
                    for event in events {
                        perf.emit(event.name, event.fields);
                    }
                } else {
                    perf.emit(
                        "failure-resource-cleanup-complete",
                        fields([
                            ("worker_exited", json!(true)),
                            ("mapped_surfaces_released", json!(true)),
                        ]),
                    );
                }
                cx.defer(|cx| cx.quit());
                return true;
            }
            if self.perf.as_ref().expect("runtime exists").elapsed_ms() >= cleanup.deadline_ms {
                let error = format!(
                    "resource cleanup timed out: worker_exited={worker_exited} surfaces_released={surfaces_released}"
                );
                let perf = self.perf.as_mut().expect("runtime exists");
                if perf.failed {
                    perf.emit(
                        "failure-resource-cleanup-timeout",
                        fields([("error", json!(error))]),
                    );
                } else {
                    perf.fail(error);
                }
                cx.defer(|cx| cx.quit());
                return true;
            }
            return false;
        }
        if self.perf.as_ref().expect("runtime exists").failed {
            cx.defer(|cx| cx.quit());
            return true;
        }
        let capture_signal_pending = self
            .perf
            .as_ref()
            .expect("runtime exists")
            .capture_signal
            .as_ref()
            .is_some_and(CaptureSignalGuard::consume);
        if capture_signal_pending {
            let disposition = self
                .perf
                .as_mut()
                .expect("runtime exists")
                .capture
                .observe_signal(true);
            match disposition {
                Ok(PresentedCropSignalDisposition::ScheduleNextFrame) => {
                    cx.notify();
                }
                Ok(PresentedCropSignalDisposition::NoSignal) => {
                    unreachable!("a consumed SIGUSR1 must request the post-capture frame")
                }
                Err(error) => {
                    let error = format!("capture signal protocol failed: {error:?}");
                    self.fail_perf_and_begin_cleanup(error, cx);
                    return false;
                }
            }
        }
        let (now_ms, document_id) = {
            let perf = self.perf.as_ref().expect("runtime exists");
            (perf.elapsed_ms(), perf.document_id)
        };
        let Some(document_id) = document_id else {
            return false;
        };
        if self
            .perf
            .as_ref()
            .expect("runtime exists")
            .qualification
            .is_none()
        {
            return false;
        }
        let Some(snapshot) = self
            .document_workspace
            .read(cx)
            .evidence_snapshot(document_id, cx)
        else {
            self.fail_perf_and_begin_cleanup("the performance document session disappeared", cx);
            return false;
        };
        {
            let perf = self.perf.as_mut().expect("runtime exists");
            perf.worker_pid = snapshot.worker_pid.or(perf.worker_pid);
        }
        if let Some(error) = snapshot.failure.clone() {
            self.fail_perf_and_begin_cleanup(error, cx);
            return false;
        }
        if !snapshot.ready {
            return false;
        }
        let events = match self
            .perf
            .as_mut()
            .expect("runtime exists")
            .qualification
            .as_mut()
            .expect("qualification presence was checked")
            .observe(now_ms, &snapshot)
        {
            Ok(events) => events,
            Err(QualificationError::DocumentNotReady) => return false,
            Err(error) => {
                let error = format!("open qualification failed: {error:?}");
                self.fail_perf_and_begin_cleanup(error, cx);
                return false;
            }
        };
        let settled = events
            .iter()
            .any(|event| event.name == "viewer-generation-settled");
        for event in events {
            self.perf
                .as_mut()
                .expect("runtime exists")
                .emit(event.name, event.fields);
        }
        if settled {
            self.perf
                .as_mut()
                .expect("runtime exists")
                .presentation_pending = true;
            cx.notify();
        }
        false
    }

    fn presented_crop_evidence(
        &self,
        snapshot: &DocumentWorkspaceEvidenceSnapshot,
        window: &Window,
        cx: &App,
    ) -> Result<PresentedCropEvidence, String> {
        let painted = self
            .document_workspace
            .read(cx)
            .painted_page_evidence(snapshot.document_id, snapshot.current_page, cx)
            .ok_or_else(|| "the settled page has no current native prepaint evidence".to_owned())?;
        if painted.document_id != snapshot.document_id
            || painted.page_index != snapshot.current_page
            || painted.request_generation != snapshot.request_generation
            || painted.viewer_generation != snapshot.viewer_generation
            || snapshot.rendered_device_pixel_ratio.is_none_or(|ratio| {
                !ratio.is_finite() || (ratio - painted.rendered_dpr).abs() >= 0.001
            })
        {
            return Err("native prepaint evidence drifted from the settled document".to_owned());
        }
        self.presented_crop_evidence_from_paint(painted, window)
    }

    fn presented_crop_evidence_from_paint(
        &self,
        painted: PaintedPageEvidence,
        window: &Window,
    ) -> Result<PresentedCropEvidence, String> {
        let perf = self
            .perf
            .as_ref()
            .ok_or_else(|| "the performance runtime disappeared".to_owned())?;
        let fixture_id = perf
            .config
            .fixture_ids
            .first()
            .cloned()
            .ok_or_else(|| "the performance fixture identity disappeared".to_owned())?;
        let bounds = painted.contained_bounds;
        let viewport = window.viewport_size();
        map_presented_crop_evidence(PresentedCropEvidenceInput {
            comparison_command_id: perf.config.command_id.clone(),
            fixture_id,
            page_index: painted.page_index,
            page_size_points: PageSizePoints {
                width: f64::from(painted.source_pdf_page_size_points.0),
                height: f64::from(painted.source_pdf_page_size_points.1),
            },
            painted_outer_page_bounds_window_logical: LogicalBounds {
                x: f64::from(f32::from(bounds.origin.x)),
                y: f64::from(f32::from(bounds.origin.y)),
                width: f64::from(f32::from(bounds.size.width)),
                height: f64::from(f32::from(bounds.size.height)),
            },
            window_logical_size: LogicalSize {
                width: f64::from(f32::from(viewport.width)),
                height: f64::from(f32::from(viewport.height)),
            },
            display_scale_factor: f64::from(window.scale_factor()),
            rendered_device_pixel_ratio: f64::from(painted.rendered_dpr),
            painted_request_generation: painted.request_generation,
            painted_resource_generation: painted.resource_generation,
            painted_render_generation: painted.viewer_generation,
            painted_state_sequence: painted.painted_state_sequence,
        })
        .map_err(|error| format!("invalid presented crop evidence: {error:?}"))
    }

    fn confirm_perf_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.perf.as_ref().and_then(|perf| perf.document_id) else {
            return;
        };
        let Some(snapshot) = self
            .document_workspace
            .read(cx)
            .evidence_snapshot(document_id, cx)
        else {
            self.fail_perf_and_begin_cleanup(
                "document disappeared before the post-paint callback",
                cx,
            );
            return;
        };
        let wait_for_capture = self
            .perf
            .as_ref()
            .is_some_and(|perf| perf.capture_signal.is_some());
        let crop_evidence = if wait_for_capture {
            match self.presented_crop_evidence(&snapshot, window, cx) {
                Ok(evidence) => Some(evidence),
                Err(error) => {
                    self.fail_perf_and_begin_cleanup(error, cx);
                    return;
                }
            }
        } else {
            None
        };
        let now_ms = self.perf.as_ref().expect("runtime exists").elapsed_ms();
        let result = {
            let perf = self.perf.as_mut().expect("runtime exists");
            let qualified = perf
                .qualification
                .as_mut()
                .expect("qualification exists")
                .confirm_presented(now_ms, &snapshot);
            match (qualified, crop_evidence) {
                (Ok(qualified), Some(evidence)) => perf
                    .capture
                    .arm(evidence)
                    .map_err(|error| format!("capture arm failed: {error:?}"))
                    .and_then(|crop| {
                        merge_presented_crop_open_events(qualified, crop)
                            .map_err(|error| format!("capture open merge failed: {error:?}"))
                    }),
                (Ok(qualified), None) => Ok(qualified),
                (Err(error), _) => Err(format!("presentation qualification failed: {error:?}")),
            }
        };
        let events = match result {
            Ok(events) => events,
            Err(error) => {
                self.fail_perf_and_begin_cleanup(error, cx);
                return;
            }
        };
        let perf = self.perf.as_mut().expect("runtime exists");
        perf.presentation_pending = false;
        perf.presentation_callback_scheduled = false;
        for event in events {
            perf.emit(event.name, event.fields);
        }
        let worker_pid = snapshot
            .worker_pid
            .expect("qualified snapshot owns a worker");
        if wait_for_capture {
            return;
        }
        if let Err(error) = self
            .perf
            .as_mut()
            .expect("runtime exists")
            .capture
            .authorize_uncaptured_success_cleanup()
        {
            self.fail_perf_and_begin_cleanup(
                format!("uncaptured cleanup authorization failed: {error:?}"),
                cx,
            );
            return;
        }
        self.begin_perf_cleanup(document_id, worker_pid, now_ms, cx);
    }

    fn confirm_perf_post_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.perf.as_ref().and_then(|perf| perf.document_id) else {
            return;
        };
        let Some(snapshot) = self
            .document_workspace
            .read(cx)
            .evidence_snapshot(document_id, cx)
        else {
            self.fail_perf_and_begin_cleanup(
                "document disappeared before post-capture confirmation",
                cx,
            );
            return;
        };
        let evidence = match self.presented_crop_evidence(&snapshot, window, cx) {
            Ok(evidence) => evidence,
            Err(error) => {
                self.fail_perf_and_begin_cleanup(error, cx);
                return;
            }
        };
        let now_ms = self.perf.as_ref().expect("runtime exists").elapsed_ms();
        let confirmation = {
            let perf = self.perf.as_mut().expect("runtime exists");
            perf.capture
                .confirm_next_frame(&evidence)
                .map_err(|error| format!("post-capture presentation failed: {error:?}"))
                .and_then(|event| {
                    perf.capture
                        .authorize_success_cleanup()
                        .map_err(|error| {
                            format!("post-capture cleanup authorization failed: {error:?}")
                        })
                        .map(|()| event)
                })
        };
        let event = match confirmation {
            Ok(event) => event,
            Err(error) => {
                self.fail_perf_and_begin_cleanup(error, cx);
                return;
            }
        };
        self.perf
            .as_mut()
            .expect("runtime exists")
            .emit(event.name, event.fields);
        let Some(worker_pid) = snapshot.worker_pid else {
            self.fail_perf_and_begin_cleanup(
                "the PDF worker disappeared before post-capture cleanup",
                cx,
            );
            return;
        };
        self.begin_perf_cleanup(document_id, worker_pid, now_ms, cx);
    }

    fn begin_perf_cleanup(
        &mut self,
        document_id: DocumentId,
        worker_pid: u32,
        now_ms: f64,
        cx: &mut Context<Self>,
    ) {
        if let Some(restoration_error) = self.restore_perf_capture_signal() {
            self.fail_perf_and_begin_cleanup(restoration_error, cx);
            return;
        }
        if !self.document_workspace.update(cx, |workspace, cx| {
            workspace.close_document(document_id, cx)
        }) {
            self.fail_perf_and_begin_cleanup(
                "the qualified document could not be closed for cleanup proof",
                cx,
            );
            return;
        }
        let perf = self.perf.as_mut().expect("runtime exists");
        perf.emit(
            "resource-cleanup-requested",
            fields([
                ("document_id", json!(document_id.value())),
                ("worker_pid", json!(worker_pid)),
            ]),
        );
        perf.cleanup_probe = Some(PerfCleanupProbe {
            worker_pid: Some(worker_pid),
            deadline_ms: now_ms + 5_000.,
            reason: CaptureCleanupReason::QualifiedSuccess,
        });
    }

    fn observe_first_perf_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(perf) = self.perf.as_mut() else {
            return;
        };
        if perf.first_frame_observed {
            return;
        }
        perf.first_frame_observed = true;
        #[cfg(feature = "benchmark-evidence")]
        {
            perf.launch_input_samples_before =
                Some(window.input_latency_snapshot().latency_histogram.len());
        }
        let gpu = window.gpu_specs();
        perf.emit(
            "gpu-adapter-selected",
            fields([
                ("available", json!(gpu.is_some())),
                (
                    "is_software_emulated",
                    gpu.as_ref().map_or(serde_json::Value::Null, |gpu| {
                        json!(gpu.is_software_emulated)
                    }),
                ),
                (
                    "device_name",
                    gpu.as_ref()
                        .map_or(serde_json::Value::Null, |gpu| json!(gpu.device_name)),
                ),
                (
                    "driver_name",
                    gpu.as_ref()
                        .map_or(serde_json::Value::Null, |gpu| json!(gpu.driver_name)),
                ),
            ]),
        );
        perf.emit("first-frame-callback-fired", Default::default());
        perf.emit("first-frame", Default::default());
        perf.emit("shell-ready", Default::default());
        let viewport = window.viewport_size();
        perf.emit(
            "native-viewer-shell-ready",
            fields([
                ("command_id", json!("viewer:launch-cold")),
                (
                    "control",
                    json!({
                        "window_logical_size": {
                            "width": f32::from(viewport.width),
                            "height": f32::from(viewport.height),
                        },
                        "point": {
                            "x": 16.,
                            "y": 16.,
                        }
                    }),
                ),
            ]),
        );
        if perf.config.input_lane == "semantic-diagnostic" {
            self.complete_semantic_launch_diagnostic(window, cx);
        }
    }

    fn sync_native_application_menu(&mut self, cx: &mut Context<Self>) {
        let state = {
            let workspace = self.document_workspace.read(cx);
            let active_document = workspace.active_document_id();
            let edit = workspace.document_edit_capabilities(cx);
            let commands = workspace.document_command_state(cx);
            NativeApplicationMenuState {
                has_active_document: active_document.is_some(),
                save_busy: commands.save_busy,
                has_focused_input: self.has_focused_input,
                can_undo: edit.can_undo,
                can_redo: edit.can_redo,
                can_cut: edit.can_cut,
                can_copy: edit.can_copy,
                can_paste: edit.can_paste,
                can_select_all: edit.can_select_all,
                can_delete: edit.can_delete,
                can_close_document: commands.can_close_document,
                document_ready: commands.document_ready,
                can_previous_page: commands.can_previous_page,
                can_next_page: commands.can_next_page,
                rotation_busy: commands.rotation_busy,
                can_zoom_out: commands.can_zoom_out,
                can_zoom_in: commands.can_zoom_in,
                actual_size_checked: commands.actual_size_checked,
                fit_width_checked: commands.fit_width_checked,
                fit_page_checked: commands.fit_page_checked,
                continuous_view_checked: commands.continuous_view_checked,
                single_page_view_checked: commands.single_page_view_checked,
            }
        };
        if self.last_native_menu_state == Some(state) {
            return;
        }
        self.last_native_menu_state = Some(state);
        install_native_application_menus(state, &self.app_menu_bar, cx);
    }

    fn observe_root_focus(
        &mut self,
        root: &Entity<Root>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.has_focused_input = window.has_focused_input(cx);
        cx.observe_in(root, window, |story, _, window, cx| {
            let has_focused_input = window.has_focused_input(cx);
            if story.has_focused_input == has_focused_input {
                return;
            }
            story.has_focused_input = has_focused_input;
            story.sync_native_application_menu(cx);
        })
        .detach();
        self.sync_native_application_menu(cx);
    }
}

impl Render for ComponentStory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let first_perf_render = self.perf.as_mut().is_some_and(|perf| {
            if perf.first_render_entered {
                false
            } else {
                perf.first_render_entered = true;
                perf.emit("first-render-enter", Default::default());
                true
            }
        });
        if first_perf_render {
            let story = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = story.update(cx, |story, cx| {
                    story.observe_first_perf_frame(window, cx);
                });
            });
            window.refresh();
        }
        let schedule_presentation = self.perf.as_mut().is_some_and(|perf| {
            if perf.presentation_pending && !perf.presentation_callback_scheduled {
                perf.presentation_callback_scheduled = true;
                true
            } else {
                false
            }
        });
        if schedule_presentation {
            let story = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = story.update(cx, |story, cx| story.confirm_perf_presentation(window, cx));
            });
            window.refresh();
        }
        let schedule_post_capture = self
            .perf
            .as_mut()
            .is_some_and(|perf| perf.capture.take_next_frame_request());
        if schedule_post_capture {
            let story = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = story.update(cx, |story, cx| story.confirm_perf_post_capture(window, cx));
            });
            window.refresh();
        }
        let root = v_flex()
            .id("compat-perf-native-target")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground);
        #[cfg(not(target_os = "macos"))]
        let root = root.child(
            div()
                .w_full()
                .h(px(28.))
                .flex_shrink_0()
                .child(self.app_menu_bar.clone()),
        );
        root.child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .child(self.document_workspace.clone()),
        )
        .on_click(cx.listener(Self::observe_perf_activation))
    }
}

#[cfg(target_os = "linux")]
fn process_is_running(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_is_running(_: u32) -> bool {
    true
}

fn directory_is_absent_or_empty(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn native_runtime_mode_from_environment() -> Result<NativeRuntimeMode, String> {
    let development = std::env::var_os("BP_NATIVE_DEVELOPMENT");
    let pdfium_library = std::env::var_os("BP_PDFIUM_LIBRARY");
    match development.as_deref() {
        None if pdfium_library.is_none() => Ok(NativeRuntimeMode::Bundled),
        None => Err("BP_PDFIUM_LIBRARY is allowed only when BP_NATIVE_DEVELOPMENT=1".to_owned()),
        Some(value) if value == "1" => {
            let pdfium_library = pdfium_library
                .ok_or_else(|| "BP_NATIVE_DEVELOPMENT=1 requires BP_PDFIUM_LIBRARY".to_owned())?;
            Ok(NativeRuntimeMode::Development {
                pdfium_library: pdfium_library.into(),
            })
        }
        Some(_) => Err("BP_NATIVE_DEVELOPMENT must be exactly 1 when set".to_owned()),
    }
}

fn authorized_perf_run_config_from_process() -> Result<Option<PerfRunConfig>, String> {
    if std::env::var_os("BP_GPUI_PERF_SCENARIO").is_none() {
        return Ok(None);
    }
    require_explicit_development_authority(std::env::var_os("BP_NATIVE_DEVELOPMENT").as_deref())
        .map_err(|error| error.to_string())?;
    PerfRunConfig::from_process().map_err(|error| format!("{error:?}"))
}

fn main() {
    let perf = match authorized_perf_run_config_from_process() {
        Ok(Some(config)) => match PerfStoryRuntime::new(config) {
            Ok(perf) => Some(perf),
            Err(error) => {
                eprintln!("failed to install GPUI capture signal: {error}");
                std::process::exit(2);
            }
        },
        Ok(None) => None,
        Err(error) => {
            eprintln!("invalid GPUI performance configuration: {error}");
            std::process::exit(2);
        }
    };
    let native_launch = if perf.is_none() {
        match NativeLaunchConfig::parse(std::env::args_os().skip(1)) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("invalid Butter Paper launch: {error}");
                std::process::exit(2);
            }
        }
    } else {
        NativeLaunchConfig::default()
    };
    let native_runtime_layout = if perf.is_none() {
        let mode = native_runtime_mode_from_environment().unwrap_or_else(|error| {
            eprintln!("invalid Butter Paper native runtime mode: {error}");
            std::process::exit(2);
        });
        Some(NativeRuntimeLayout::discover(mode).unwrap_or_else(|error| {
            eprintln!("invalid Butter Paper native runtime layout: {error}");
            std::process::exit(2);
        }))
    } else {
        None
    };
    let session_source = NativeLaunchSessionSource::new(perf.is_some(), &native_launch);
    let native_ingress = NativeDocumentIngress::default();
    let application = gpui_platform::application();
    application.on_open_urls({
        let native_ingress = native_ingress.clone();
        move |urls| {
            native_ingress.enqueue_file_urls(urls);
        }
    });
    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1200.), px(800.)), cx)),
            ..Default::default()
        };

        let native_ingress = native_ingress.clone();
        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                window.set_window_title(if perf.is_some() {
                    "Butter Paper GPUI comparison"
                } else {
                    "Butter Paper"
                });
                let system_theme_subscription = follow_window_appearance(window, cx);
                let app_menu_bar = AppMenuBar::new(cx);
                let worker_executable = perf.as_ref().map_or_else(
                    || {
                        native_runtime_layout
                            .as_ref()
                            .expect("normal launch resolved the native runtime layout")
                            .worker_executable()
                            .to_owned()
                    },
                    |perf| perf.config.worker_executable.clone(),
                );
                let pdfium_library = perf.as_ref().map_or_else(
                    || {
                        native_runtime_layout
                            .as_ref()
                            .expect("normal launch resolved the native runtime layout")
                            .pdfium_library()
                            .to_owned()
                    },
                    |perf| perf.config.pdfium_library.clone(),
                );
                let surface_root = perf.as_ref().map_or_else(
                    || std::env::temp_dir().join("butter-paper-document-workspace"),
                    |perf| perf.config.cache_directory.join("document-workspace"),
                );
                let generated_store =
                    GeneratedDocumentStore::new(surface_root.join("generated-documents"))
                        .expect("the experiment-owned generated-document store must initialize");
                let template_manager_root = surface_root.join("template-library");
                let session_state_root = surface_root.join("session-state");
                let (session_store, launch_resolution) = if session_source.requires_store() {
                    let opened = std::fs::create_dir_all(&session_state_root)
                        .map_err(|error| error.to_string())
                        .and_then(|()| {
                            SessionManifestStore::open(session_state_root)
                                .map_err(|error| format!("{error:?}"))
                        });
                    match opened {
                        Ok(store) => {
                            let store = std::sync::Arc::new(store);
                            let loaded = if session_source.requires_manifest_load() {
                                store.load().map(Some).map_err(|error| format!("{error:?}"))
                            } else {
                                Ok(None)
                            };
                            (Some(store), session_source.clone().resolve(loaded))
                        }
                        Err(error) => (None, session_source.clone().resolve(Err(error))),
                    }
                } else {
                    (None, session_source.clone().resolve(Ok(None)))
                };
                if let Some(NativeLaunchWarning::SessionStateUnavailable(message)) =
                    launch_resolution.warning.as_ref()
                {
                    eprintln!("Butter Paper session state is unavailable: {message}");
                }
                let opener = std::sync::Arc::new(PdfiumWorkerBackend::new(
                    worker_executable,
                    pdfium_library,
                    surface_root,
                ));
                let saver = std::sync::Arc::new(PdfDocumentSaver::new(opener.clone()));
                let document_workspace = cx.new(|cx| {
                    DocumentWorkspace::with_opener_and_generated_store(
                        opener,
                        generated_store.clone(),
                        cx,
                    )
                });
                match launch_resolution.action.clone() {
                    NativeLaunchAction::None => {}
                    NativeLaunchAction::OpenExplicit(request) => {
                        document_workspace.update(cx, |workspace, cx| {
                            workspace.open_documents(request, cx);
                        });
                    }
                    NativeLaunchAction::Restore(plan) => {
                        document_workspace.update(cx, |workspace, cx| {
                            workspace.restore_session(plan, cx);
                        });
                    }
                }
                let application_close = cx.new(|_| {
                    if launch_resolution.checkpoint_enabled {
                        let checkpoint_publisher: std::sync::Arc<
                            dyn ApplicationCloseCheckpointPublisher,
                        > = session_store
                            .clone()
                            .expect("enabled checkpointing has an open session store");
                        ApplicationCloseWorkspace::with_checkpoint_publisher(
                            document_workspace.clone(),
                            saver,
                            checkpoint_publisher,
                        )
                    } else {
                        ApplicationCloseWorkspace::new(document_workspace.clone(), saver)
                    }
                });
                let template_manager = perf.is_none().then(|| {
                    cx.new(|cx| {
                        let legacy_request = std::env::var("BP_LEGACY_BLANK_SETTINGS_JSON")
                            .ok()
                            .and_then(|json| legacy_blank_request_from_json(&json).ok());
                        let mut manager = TemplateManagerView::open_persistent_with_legacy(
                            template_manager_root,
                            legacy_request,
                            window,
                            cx,
                        )
                        .expect("the experiment-owned template library must initialize");
                        manager.bind_document_workspace(
                            document_workspace.downgrade(),
                            generated_store.clone(),
                        );
                        manager
                    })
                });
                if perf.is_none() {
                    let ingress = native_ingress.clone();
                    let workspace = document_workspace.downgrade();
                    cx.spawn(async move |cx| {
                        while let Some(request) = ingress.next_request().await {
                            if workspace
                                .update(cx, |workspace, cx| {
                                    workspace.open_documents(request, cx);
                                })
                                .is_err()
                            {
                                break;
                            }
                            cx.update(|cx| cx.activate(true));
                        }
                    })
                    .detach();
                }
                let story = cx.new(|cx| {
                    ComponentStory::new(
                        document_workspace.clone(),
                        app_menu_bar,
                        template_manager,
                        perf,
                        system_theme_subscription,
                        window,
                        cx,
                    )
                });
                register_document_workspace_global_actions(&document_workspace, cx);
                register_application_close_action(&application_close, cx);
                if story.read(cx).perf.is_some() {
                    story.update(cx, |story, _| {
                        if let Some(perf) = story.perf.as_mut() {
                            perf.emit("window-created", Default::default());
                        }
                    });
                    let executor = cx.background_executor().clone();
                    let story_for_monitor = story.downgrade();
                    cx.spawn(async move |cx| {
                        loop {
                            executor.timer(Duration::from_millis(25)).await;
                            let Ok(done) = story_for_monitor
                                .update(cx, |story, cx| story.observe_perf_document(cx))
                            else {
                                break;
                            };
                            if done {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                let story_view = gpui::AnyView::from(story.clone());
                let shell = cx.new(|cx| {
                    ApplicationCloseShell::new_for_native_window_with_content(
                        application_close,
                        story_view,
                        window,
                        cx,
                    )
                });
                let root = cx.new(|cx| Root::new(shell, window, cx));
                story.update(cx, |story, cx| {
                    story.observe_root_focus(&root, window, cx);
                });
                root
            })
            .expect("failed to open GPUI Component proof window");
        })
        .detach();
    });
}
