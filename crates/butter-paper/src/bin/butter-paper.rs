#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use butter_paper::application_close_workspace::{
    ApplicationCloseCancelled, ApplicationCloseCheckpointPublisher, ApplicationCloseShell,
    ApplicationCloseWorkspace, RequestApplicationClose, RequestApplicationQuit,
};
use butter_paper::document_windows::{
    WindowCheckpointPublisher, WindowSessionCoordinator,
};
#[cfg(feature = "development-pdfium-override")]
use butter_paper::application_shell::application_data_directory;
use butter_paper::application_shell::{
    ApplicationShellPreferences, ApplicationShellPreferencesStore, ApplicationUiZoomAction,
    MakeInterfaceBigger, MakeInterfaceSmaller, MinimiseWindow, MoveDocumentToNewWindow, NewWindow,
    OpenReleasePage,
    ResetInterfaceSize, ZoomWindow,
    ReverseScrollZoom, SetAsDefaultPdfApp, ToggleApplicationFullScreen, ToggleApplicationMenuBar,
    ToggleReverseScrollZoom, apply_application_ui_zoom,
    focus_initial_command_context, init_application_shell_actions,
    resolve_application_ui_zoom_level,
};
use butter_paper::document_recovery_store::{
    DocumentRecoveryStore, RecoverySourceKind,
};
use butter_paper::document_tab_bar::TemplateCatalogItem;
use butter_paper::document_workspace::{
    DeferredStartupOpen, DocumentId, DocumentWorkspace, DocumentWorkspaceEvidenceSnapshot,
    DocumentWorkspaceTemplateCommand, PaintedPageEvidence, PdfDocumentSaver, PdfiumWorkerBackend,
    DocumentTabTransferEvent, StartupRecoveryAvailability, StartupRecoveryItem,
    init_document_workspace_actions,
    register_document_workspace_actions_for,
};
use butter_paper::generated_document::GeneratedDocumentStore;
use butter_paper::macos_process_lifecycle::AppRootLifecycleReceipt;
use butter_paper::native_application::{
    ApplicationMenuShellState, NativeApplicationMenuState, NativeDocumentIngress,
    install_native_application_menus_with_shell, install_native_platform_menus,
};
use butter_paper::native_launch::{
    NativeLaunchAction, NativeLaunchConfig, NativeLaunchResolution, NativeLaunchSessionSource,
    NativeLaunchWarning,
};
#[cfg(all(not(feature = "development-pdfium-override"), target_os = "linux"))]
use butter_paper::native_platform_storage::linux_production_storage;
#[cfg(all(not(feature = "development-pdfium-override"), target_os = "windows"))]
use butter_paper::native_platform_storage::windows_production_storage;
#[cfg(all(not(feature = "development-pdfium-override"), target_os = "macos"))]
use butter_paper::native_release_identity::{
    attest_current_release, current_user_home_directory, display_fatal_launch_error,
};
use butter_paper::native_runtime_layout::{
    NativeRuntimeLayout, NativeRuntimeMode, require_explicit_development_authority,
};
#[cfg(all(not(feature = "development-pdfium-override"), target_os = "macos"))]
use butter_paper::native_storage_layout::NativeProductionStorage;
use butter_paper::native_storage_layout::NativeReleaseChannel;
use butter_paper::native_storage_layout::NativeStorageLayout;
use butter_paper::perf_capture_signal::{CaptureSignalError, CaptureSignalGuard};
use butter_paper::perf_protocol::{PerfProtocol, StdoutSink, fields};
use butter_paper::perf_scenario::{
    CaptureCleanupReason, CaptureOrchestrationCoordinator, LogicalBounds, LogicalSize,
    OpenPdfQualification, PageSizePoints, PerfRunConfig, PresentedCropEvidence,
    PresentedCropEvidenceInput, PresentedCropSignalDisposition, QualificationError,
    map_presented_crop_evidence, merge_presented_crop_open_events,
};
use butter_paper::recent_signature_store::{
    PlatformSignatureKeyStore, RECENT_SIGNATURES_FILE_NAME, RecentSignatureStore,
};
use butter_paper::session_manifest::{
    SessionManifestStore, SessionRecoverySnapshot, SessionRestorePlan,
};
use butter_paper::system_theme::follow_window_appearance_with_application_zoom;
use butter_paper::template_manager::{
    PersistentTemplateManager, TemplateManagerView, legacy_blank_request_from_json,
    route_workspace_template_command,
};
#[cfg(not(target_os = "macos"))]
use butter_paper::window_title_bar::{uses_window_title_bar, window_title_bar};
#[cfg(not(target_os = "macos"))]
use gpui::prelude::FluentBuilder as _;
use butter_paper::window_title_bar::{
    APPLICATION_TITLE, format_window_title_for_application, title_bar_window_options,
};
use gpui::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, PromptLevel, Render, StatefulInteractiveElement as _,
    Styled as _,
    Subscription, Task, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{ActiveTheme as _, Root, WindowExt as _, menu::AppMenuBar, v_flex};
use serde_json::json;
use std::{
    cell::Cell,
    ffi::{OsStr, OsString},
    rc::Rc,
    sync::Arc,
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

const DEVELOPMENT_SIGNATURE_KEYCHAIN_SERVICE: &str =
    "com.butterpaper.gpui-migration.recent-signatures";

struct ResolvedStorageContext {
    layout: NativeStorageLayout,
    preferences: ApplicationShellPreferences,
    /// Stable for development storage; the attested channel in production.
    release_channel: NativeReleaseChannel,
    signature_keychain_service: &'static str,
    application_title: &'static str,
}

fn exit_for_launch_failure(context: &str, error: &str) -> ! {
    let message = format!("{context}: {error}");
    eprintln!("{message}");
    #[cfg(all(not(feature = "development-pdfium-override"), target_os = "macos"))]
    display_fatal_launch_error(&message);
    #[cfg(target_os = "windows")]
    display_windows_fatal_launch_error(&message);
    std::process::exit(2);
}

#[cfg(target_os = "windows")]
fn display_windows_fatal_launch_error(detail: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_TASKMODAL, MessageBoxW,
    };

    let text = wide_null_terminated(detail);
    let title = wide_null_terminated("Butter Paper could not start");
    // A GUI-subsystem executable has no visible console for startup errors.
    // Keep this synchronous so the user can read the diagnostic before exit.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_TASKMODAL,
        );
    }
}

#[cfg(target_os = "windows")]
fn wide_null_terminated(value: &str) -> Vec<u16> {
    value
        .replace('\0', "�")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
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
    app_menu_bar_focus: FocusHandle,
    window_handle: AnyWindowHandle,
    menu_bar_visible: bool,
    menu_bar_visibility_supported: bool,
    ui_zoom_level: Rc<Cell<i8>>,
    ui_zoom_base_font_size: Rc<Cell<gpui::Pixels>>,
    template_manager: Option<Entity<TemplateManagerView>>,
    session_store: Option<Arc<SessionManifestStore>>,
    last_observed_recovery_snapshot:
        Option<butter_paper::session_manifest::SessionRecoverySnapshot>,
    pending_recovery_snapshot:
        Option<butter_paper::session_manifest::SessionRecoverySnapshot>,
    recovery_marker_task: Option<Task<()>>,
    _document_workspace_subscription: Subscription,
    _template_command_subscription: Option<Subscription>,
    _app_menu_bar_focus_out_subscription: Subscription,
    _focus_lost_subscription: Subscription,
    _system_theme_subscription: Subscription,
    last_native_menu_state: Option<NativeApplicationMenuState>,
    last_in_window_menu_state: Option<NativeApplicationMenuState>,
    last_native_menu_shell_state: Option<ApplicationMenuShellState>,
    application_title: &'static str,
    window_title: String,
    has_focused_input: bool,
    perf: Option<PerfStoryRuntime>,
}

impl ComponentStory {
    fn new(
        document_workspace: Entity<DocumentWorkspace>,
        app_menu_bar: Entity<AppMenuBar>,
        menu_bar_visibility_supported: bool,
        ui_zoom_level: Rc<Cell<i8>>,
        ui_zoom_base_font_size: Rc<Cell<gpui::Pixels>>,
        template_manager: Option<Entity<TemplateManagerView>>,
        session_store: Option<Arc<SessionManifestStore>>,
        perf: Option<PerfStoryRuntime>,
        application_title: &'static str,
        system_theme_subscription: Subscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let document_workspace_subscription =
            cx.observe_in(&document_workspace, window, |story, workspace, window, cx| {
                // A drag notifies on every pointer move; menus, title and the
                // recovery marker settle on the notify that ends it.
                if workspace.read(cx).pointer_gesture_active() {
                    return;
                }
                story.sync_native_application_menu(cx);
                story.sync_template_operation_state(cx);
                story.sync_window_title(window, cx);
                story.schedule_recovery_marker_checkpoint(cx);
            });
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
        let app_menu_bar_focus = cx.focus_handle();
        let app_menu_bar_focus_out_subscription =
            cx.on_focus_out(&app_menu_bar_focus, window, |_story, _, window, cx| {
                let story = cx.entity().downgrade();
                window.on_next_frame(move |window, cx| {
                    let _ = story.update(cx, |story, cx| {
                        if !story.app_menu_bar_focus.contains_focused(window, cx)
                            && !gpui_component::GlobalState::is_in_deferred_context(cx)
                        {
                            story.sync_native_application_menu(cx);
                        }
                    });
                });
                window.refresh();
            });
        // When the focused element goes away (a closed popover, panel or tab)
        // GPUI leaves nothing focused, and the document's single-key shortcuts
        // would stay dead until the next page press. Return focus to it.
        let focus_lost_subscription = cx.on_focus_lost(window, |story, window, cx| {
            story.document_workspace.read(cx).focus_handle().focus(window, cx);
        });
        let menu_bar_visible =
            !menu_bar_visibility_supported || shared_application(cx).preferences.get().menu_bar_visible();
        // The active document window owns the application menus; refresh them
        // and any template changes made in another window when this one
        // becomes active.
        record_window_bounds(window, cx);
        cx.observe_window_bounds(window, |_, window, cx| record_window_bounds(window, cx))
            .detach();
        cx.observe_window_activation(window, |story, window, cx| {
            if !window.is_window_active() {
                return;
            }
            if cx.has_global::<DocumentWindows>() {
                cx.global_mut::<DocumentWindows>().last_active = Some(window.window_handle());
            }
            story.last_native_menu_state = None;
            story.last_in_window_menu_state = None;
            story.last_native_menu_shell_state = None;
            if let Some(manager) = story.template_manager.clone()
                && !window.has_active_dialog(cx)
            {
                manager.update(cx, |manager, cx| manager.reload_from_shared(cx));
            }
            story.sync_native_application_menu(cx);
        })
        .detach();
        let mut story = Self {
            document_workspace,
            app_menu_bar,
            app_menu_bar_focus,
            window_handle: window.window_handle(),
            menu_bar_visible,
            menu_bar_visibility_supported,
            ui_zoom_level,
            ui_zoom_base_font_size,
            template_manager,
            session_store,
            last_observed_recovery_snapshot: None,
            pending_recovery_snapshot: None,
            recovery_marker_task: None,
            _document_workspace_subscription: document_workspace_subscription,
            _template_command_subscription: template_command_subscription,
            _app_menu_bar_focus_out_subscription: app_menu_bar_focus_out_subscription,
            _focus_lost_subscription: focus_lost_subscription,
            _system_theme_subscription: system_theme_subscription,
            last_native_menu_state: None,
            last_in_window_menu_state: None,
            last_native_menu_shell_state: None,
            application_title,
            window_title: String::new(),
            has_focused_input: false,
            perf,
        };
        story.sync_native_application_menu(cx);
        story.sync_template_catalog(cx);
        story.sync_window_title(window, cx);
        story
    }

    fn schedule_recovery_marker_checkpoint(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.session_store.clone() else {
            return;
        };
        // One marker covers every window, so a crash in any of them is flagged.
        let snapshot = {
            let workspaces = document_window_workspaces(cx);
            let workspaces = if workspaces.is_empty() {
                vec![self.document_workspace.clone()]
            } else {
                workspaces
            };
            if workspaces
                .iter()
                .any(|workspace| workspace.read(cx).active_document_open_batches() > 0)
            {
                return;
            }
            let snapshot = SessionRecoverySnapshot::merged(
                workspaces
                    .iter()
                    .map(|workspace| workspace.read(cx).session_recovery_snapshot(cx)),
            );
            if snapshot.is_empty()
                && workspaces
                    .iter()
                    .any(|workspace| workspace.read(cx).session_recovery_warning().is_some())
            {
                return;
            }
            snapshot
        };
        if self.last_observed_recovery_snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.last_observed_recovery_snapshot = Some(snapshot.clone());
        self.pending_recovery_snapshot = Some(snapshot);
        let executor = cx.background_executor().clone();
        self.recovery_marker_task = Some(cx.spawn(async move |entity, cx| {
            executor.timer(Duration::from_millis(250)).await;
            let Ok(Some(snapshot)) =
                entity.update(cx, |story, _| story.pending_recovery_snapshot.take())
            else {
                return;
            };
            let result = executor
                .spawn(async move { store.replace_recovery_marker(&snapshot) })
                .await;
            if let Err(error) = result {
                eprintln!("unable to publish the dirty-session marker: {error:?}");
            }
        }));
    }

    fn sync_window_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (names, active_index) = {
            let workspace = self.document_workspace.read(cx);
            let active_document_id = workspace.active_document_id();
            let sessions = workspace.sessions();
            (
                sessions
                    .iter()
                    .map(|session| session.read(cx).title().to_owned())
                    .collect::<Vec<_>>(),
                sessions
                    .iter()
                    .position(|session| Some(session.read(cx).id()) == active_document_id),
            )
        };
        let names = names.iter().map(String::as_str).collect::<Vec<_>>();
        let title =
            format_window_title_for_application(&names, active_index, self.application_title);
        if self.window_title == title {
            return;
        }
        window.set_window_title(&title);
        self.window_title = title;
        cx.notify();
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
                    .map(|record| {
                        TemplateCatalogItem::new(record.id(), record.name())
                            .with_preview(record.preview())
                    })
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

    fn update_application_preferences(
        &self,
        cx: &App,
        update: impl FnOnce(&mut ApplicationShellPreferences),
    ) {
        let shared = shared_application(cx);
        let mut preferences = shared.preferences.get();
        update(&mut preferences);
        shared.preferences.set(preferences);
        if let Err(error) = shared.preferences_store.save(preferences) {
            eprintln!("unable to save Butter Paper application preferences: {error}");
        }
    }

    fn toggle_menu_bar(
        &mut self,
        _: &ToggleApplicationMenuBar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.menu_bar_visibility_supported {
            return;
        }
        let visible = !self.menu_bar_visible;
        self.update_application_preferences(cx, |preferences| {
            preferences.set_menu_bar_visible(visible)
        });

        // Let PopupMenu finish its own dismissal and focus restoration before
        // replacing the in-window projection, especially when hiding the bar.
        // The preference applies to every window.
        cx.defer(move |cx| {
            for story in document_window_stories(cx) {
                story.update(cx, |story, cx| {
                    story.menu_bar_visible = visible;
                    story.sync_native_application_menu(cx);
                    cx.notify();
                });
            }
        });
        cx.notify();
    }

    fn toggle_reverse_scroll_zoom(&mut self, cx: &mut Context<Self>) {
        let reverse = !shared_application(cx).preferences.get().reverse_scroll_zoom();
        self.update_application_preferences(cx, |preferences| {
            preferences.set_reverse_scroll_zoom(reverse)
        });
        cx.set_global(ReverseScrollZoom(reverse));
        let story = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = story.update(cx, |story, cx| story.sync_native_application_menu(cx));
        });
        cx.notify();
    }

    fn change_interface_size(
        &mut self,
        action: ApplicationUiZoomAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let level = resolve_application_ui_zoom_level(self.ui_zoom_level.get(), action);
        self.ui_zoom_level.set(level);
        self.update_application_preferences(cx, |preferences| {
            preferences.set_ui_zoom_level(level)
        });
        apply_application_ui_zoom(level, self.ui_zoom_base_font_size.get(), window, cx);
        // The interface size is application-wide.
        let this_window = self.window_handle;
        cx.defer(move |cx| {
            for entry in document_window_entries(cx) {
                if let Some(workspace) = entry.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| workspace.reset_right_rail_pixel_sizes(cx));
                }
                if entry.handle != this_window {
                    let _ = entry.handle.update(cx, |_, window, _| window.refresh());
                }
            }
        });
        cx.notify();
    }

    fn toggle_full_screen(
        &mut self,
        _: &ToggleApplicationFullScreen,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.toggle_fullscreen();
        window.refresh();
    }

    fn open_release_page(&mut self, _: &OpenReleasePage, _: &mut Window, cx: &mut Context<Self>) {
        cx.open_url(butter_paper::application_shell::APPLICATION_RELEASES_URL);
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
            let storage = NativeStorageLayout::performance(
                &self
                    .perf
                    .as_ref()
                    .expect("runtime exists")
                    .config
                    .cache_directory,
            )
            .expect("performance storage was validated before launch");
            let surfaces_released = storage.surfaces_released();
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
        if !owns_application_menus(self.window_handle, cx) {
            return;
        }
        let state = self.native_application_menu_state(cx);
        let shell = self.application_menu_shell_state(cx);
        if self.last_native_menu_state == Some(state)
            && self.last_in_window_menu_state == Some(state)
            && self.last_native_menu_shell_state == Some(shell)
        {
            return;
        }
        self.last_native_menu_state = Some(state);
        self.last_in_window_menu_state = Some(state);
        self.last_native_menu_shell_state = Some(shell);
        install_native_application_menus_with_shell(state, shell, &self.app_menu_bar, cx);
    }

    fn sync_native_platform_menu(&mut self, cx: &mut Context<Self>) {
        if !owns_application_menus(self.window_handle, cx) {
            return;
        }
        let state = self.native_application_menu_state(cx);
        if self.last_native_menu_state == Some(state) {
            return;
        }
        self.last_native_menu_state = Some(state);
        install_native_platform_menus(state, self.application_menu_shell_state(cx), cx);
    }

    fn native_application_menu_state(&self, cx: &Context<Self>) -> NativeApplicationMenuState {
        {
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
                can_move_document_to_new_window: shared_application(cx).multi_window
                    && workspace.session_count() > 1
                    && active_document
                        .is_some_and(|document_id| workspace.can_transfer_document(document_id, cx)),
            }
        }
    }

    fn application_menu_shell_state(&self, cx: &App) -> ApplicationMenuShellState {
        ApplicationMenuShellState {
            menu_bar_visible: self.menu_bar_visible,
            menu_bar_visibility_supported: self.menu_bar_visibility_supported,
            reverse_scroll_zoom: shared_application(cx).preferences.get().reverse_scroll_zoom(),
            updates: updates::menu_state(cx),
        }
    }

    fn observe_root_focus(
        &mut self,
        root: &Entity<Root>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // `open_window` has not installed the returned Root entity on the
        // Window until this callback returns. Querying WindowExt here would
        // therefore ask GPUI Component to read a non-Root window root.
        self.has_focused_input = false;
        cx.observe_in(root, window, |story, _, window, cx| {
            let has_focused_input = window.has_focused_input(cx);
            if story.has_focused_input == has_focused_input {
                return;
            }
            story.has_focused_input = has_focused_input;
            if story.app_menu_bar_focus.contains_focused(window, cx) {
                story.sync_native_platform_menu(cx);
            } else {
                let story = cx.entity().downgrade();
                window.on_next_frame(move |window, cx| {
                    let _ = story.update(cx, |story, cx| {
                        if !story.app_menu_bar_focus.contains_focused(window, cx) {
                            story.sync_native_application_menu(cx);
                        }
                    });
                });
                window.refresh();
            }
        })
        .detach();
        self.sync_native_application_menu(cx);
    }
}

/// Runs `update` on the active document window's story, after the action that
/// triggered it has finished dispatching.
fn defer_to_active_story(
    cx: &mut App,
    update: impl FnOnce(&mut ComponentStory, &mut Window, &mut Context<ComponentStory>) + 'static,
) {
    let Some(entry) = active_document_window(cx) else {
        return;
    };
    cx.defer(move |cx| {
        let Some(story) = entry.story.upgrade() else {
            return;
        };
        let _ = entry.handle.update(cx, |_, window, cx| {
            story.update(cx, |story, cx| update(story, window, cx));
        });
    });
}

/// Registers an application-level command that acts on windows. A menu item
/// or shortcut is dispatched while GPUI is updating the active window, where
/// updating that window again fails, so the command runs once the dispatch
/// has finished.
fn on_window_action<A: gpui::Action>(cx: &mut App, handler: fn(&mut App)) {
    cx.on_action(move |_: &A, cx| cx.defer(handler));
}

/// Application-level shell commands, registered once and routed to the active
/// document window.
fn register_application_shell_actions(cx: &mut App) {
    cx.on_action(|_: &ToggleApplicationMenuBar, cx| {
        defer_to_active_story(cx, |story, window, cx| {
            story.toggle_menu_bar(&ToggleApplicationMenuBar, window, cx)
        });
    });

    cx.on_action(|_: &SetAsDefaultPdfApp, cx| {
        let Some(entry) = active_document_window(cx) else {
            return;
        };
        let task = cx
            .background_executor()
            .spawn(async { butter_paper::default_pdf_app::set_as_default_pdf_app() });
        cx.spawn(async move |cx| {
            let result = task.await;
            let _ = entry.handle.update(cx, |_, window, cx| {
                let (level, message, detail) = match result {
                    Ok(result) => (PromptLevel::Info, result.message, None),
                    Err(error) => (
                        PromptLevel::Warning,
                        "Butter Paper could not be set as the default PDF app".to_owned(),
                        Some(error),
                    ),
                };
                let _ = window.prompt(level, &message, detail.as_deref(), &["OK"], cx);
            });
        })
        .detach();
    });

    cx.on_action(|_: &ToggleReverseScrollZoom, cx| {
        defer_to_active_story(cx, |story, _, cx| story.toggle_reverse_scroll_zoom(cx));
    });
    cx.on_action(|_: &MakeInterfaceBigger, cx| {
        defer_to_active_story(cx, |story, window, cx| {
            story.change_interface_size(ApplicationUiZoomAction::In, window, cx)
        });
    });
    cx.on_action(|_: &MakeInterfaceSmaller, cx| {
        defer_to_active_story(cx, |story, window, cx| {
            story.change_interface_size(ApplicationUiZoomAction::Out, window, cx)
        });
    });
    cx.on_action(|_: &ResetInterfaceSize, cx| {
        defer_to_active_story(cx, |story, window, cx| {
            story.change_interface_size(ApplicationUiZoomAction::Reset, window, cx)
        });
    });
    cx.on_action(|_: &ToggleApplicationFullScreen, cx| {
        defer_to_active_story(cx, |story, window, cx| {
            story.toggle_full_screen(&ToggleApplicationFullScreen, window, cx)
        });
    });
    cx.on_action(|_: &OpenReleasePage, cx| {
        defer_to_active_story(cx, |story, window, cx| {
            story.open_release_page(&OpenReleasePage, window, cx)
        });
    });
    on_window_action::<MinimiseWindow>(cx, |cx| {
        if let Some(entry) = active_document_window(cx) {
            let _ = entry.handle.update(cx, |_, window, _| window.minimize_window());
        }
    });
    on_window_action::<ZoomWindow>(cx, |cx| {
        if let Some(entry) = active_document_window(cx) {
            let _ = entry.handle.update(cx, |_, window, _| window.zoom_window());
        }
    });
    on_window_action::<MoveDocumentToNewWindow>(cx, |cx| {
        #[cfg(feature = "review-driver")]
        review_driver::trace("MoveDocumentToNewWindow handler");
        let Some(entry) = active_document_window(cx) else {
            return;
        };
        let Some(document_id) = entry
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).active_document_id())
        else {
            return;
        };
        let Some(screen) = entry
            .handle
            .update(cx, |_, window, _| {
                let frame = window.bounds();
                frame.origin + gpui::point(px(148.), px(48.))
            })
            .ok()
        else {
            return;
        };
        cx.defer(move |cx| move_document_between_windows(entry.handle, document_id, None, screen, cx));
    });
    on_window_action::<NewWindow>(cx, |cx| {
        if shared_application(cx).multi_window {
            open_document_window(cx, None);
        }
    });
    // Quit closes every window through its own unsaved-changes transaction.
    // Never route this through gpui::Quit.
    on_window_action::<RequestApplicationQuit>(cx, |cx| {
        #[cfg(feature = "review-driver")]
        review_driver::trace("RequestApplicationQuit handler");
        let mut windows = document_window_entries(cx);
        if windows.is_empty() {
            cx.quit();
            return;
        }
        // The front window closes first, so its active document is the
        // session's active document and its window restores in front.
        if let Some(active) = active_document_window(cx) {
            windows.sort_by_key(|entry| entry.handle != active.handle);
        }
        shared_application(cx).coordinator.begin_quit();
        for entry in windows {
            request_window_close(&entry, cx);
        }
    });
    on_window_action::<RequestApplicationClose>(cx, |cx| {
        if let Some(entry) = active_document_window(cx) {
            request_window_close(&entry, cx);
        }
    });
}

fn request_window_close(entry: &DocumentWindowEntry, cx: &mut App) {
    let Some(close) = entry.close.upgrade() else {
        return;
    };
    let _ = entry.handle.update(cx, |_, window, cx| {
        let _ = close.update(cx, |close, cx| close.request_close(cx));
        if close.read(cx).dialog().is_some() && !window.has_active_dialog(cx) {
            ApplicationCloseWorkspace::open_dialog(&close, window, cx);
        }
    });
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
        let root = root.when(uses_window_title_bar(window), |root| {
            root.child(window_title_bar(
                self.window_title.clone(),
                window.viewport_size().width,
                cx.theme().background,
            ))
        });
        #[cfg(target_os = "macos")]
        let root = if self.menu_bar_visible {
            root.child(
                div()
                    .id("app-menu-bar-region")
                    .w_full()
                    .h_8()
                    .pl_1()
                    .flex_shrink_0()
                    .track_focus(&self.app_menu_bar_focus)
                    // A press on the bar opens a menu but never takes focus itself:
                    // the menu then returns focus to the document, which keeps its
                    // single-key shortcuts and receives menu commands.
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| window.prevent_default())
                    .bg(cx.theme().background)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.app_menu_bar.clone()),
            )
        } else {
            root
        };
        #[cfg(not(target_os = "macos"))]
        let root = root.child(
            div()
                .id("app-menu-bar-region")
                .w_full()
                .h_8()
                .pl_1()
                .flex_shrink_0()
                .track_focus(&self.app_menu_bar_focus)
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| window.prevent_default())
                .bg(cx.theme().background)
                .border_b_1()
                .border_color(cx.theme().border)
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

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return true;
    };
    if pid <= 0 {
        return true;
    }
    // Signal 0 probes existence without signalling the owned worker. Permission
    // denial (and unexpected errors) cannot prove termination: fail closed.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(not(unix))]
fn process_is_running(_: u32) -> bool {
    true
}

#[cfg(all(test, unix))]
mod process_cleanup_tests {
    #[test]
    fn owned_child_is_alive_until_terminated_and_reaped() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "read ignored"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let running = super::process_is_running(pid);
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(running, "the blocked owned child must be reported alive");
        assert!(
            !super::process_is_running(pid),
            "a reaped child must not block cleanup"
        );
    }
}

fn native_runtime_mode(
    development: Option<&OsStr>,
    pdfium_library: Option<OsString>,
) -> Result<NativeRuntimeMode, String> {
    #[cfg(not(feature = "development-pdfium-override"))]
    {
        if development.is_some() || pdfium_library.is_some() {
            return Err(
                "external rendering-library inputs are not compiled into this production build"
                    .to_owned(),
            );
        }
        Ok(NativeRuntimeMode::Bundled)
    }
    #[cfg(feature = "development-pdfium-override")]
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

fn native_runtime_mode_from_environment() -> Result<NativeRuntimeMode, String> {
    let development = std::env::var_os("BP_NATIVE_DEVELOPMENT");
    native_runtime_mode(
        development.as_deref(),
        std::env::var_os("BP_PDFIUM_LIBRARY"),
    )
}

#[cfg(test)]
mod native_runtime_mode_tests {
    use super::*;

    #[test]
    fn bundled_runtime_requires_no_external_override() {
        assert!(matches!(
            native_runtime_mode(None, None).unwrap(),
            NativeRuntimeMode::Bundled
        ));
    }

    #[cfg(feature = "development-pdfium-override")]
    #[test]
    fn development_build_requires_explicit_paired_authority() {
        assert!(matches!(
            native_runtime_mode(
                Some(OsStr::new("1")),
                Some(OsString::from("/owned/libpdfium.dylib")),
            )
            .unwrap(),
            NativeRuntimeMode::Development { .. }
        ));
        assert!(native_runtime_mode(None, Some(OsString::from("/tmp/libpdfium.dylib"))).is_err());
        assert!(native_runtime_mode(Some(OsStr::new("1")), None).is_err());
    }

    #[cfg(not(feature = "development-pdfium-override"))]
    #[test]
    fn production_build_rejects_development_override_variables() {
        let error = native_runtime_mode(
            Some(OsStr::new("1")),
            Some(OsString::from("/tmp/libpdfium.dylib")),
        )
        .unwrap_err();
        assert_eq!(
            error,
            "external rendering-library inputs are not compiled into this production build"
        );
        assert!(native_runtime_mode(None, Some(OsString::from("/tmp/libpdfium.dylib"))).is_err());
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

fn apply_launch_action(
    workspace: &mut DocumentWorkspace,
    action: NativeLaunchAction,
    cx: &mut Context<DocumentWorkspace>,
) {
    match action {
        NativeLaunchAction::None => {}
        NativeLaunchAction::OpenExplicit(request) => {
            workspace.open_documents(request, cx);
        }
        NativeLaunchAction::Restore(plan) => restore_session_windows(workspace, plan, cx),
    }
}

/// Restores the first saved window into `workspace` and each other saved
/// window into a new window, keeping the first window in front.
fn restore_session_windows(
    workspace: &mut DocumentWorkspace,
    plan: SessionRestorePlan,
    cx: &mut Context<DocumentWorkspace>,
) {
    if !shared_application(cx).multi_window {
        workspace.restore_session(plan, cx);
        return;
    }
    let mut plans = plan.split_windows().into_iter();
    workspace.restore_session(plans.next().unwrap_or_default(), cx);
    let others = plans.collect::<Vec<_>>();
    if others.is_empty() {
        return;
    }
    let first = cx.entity().downgrade();
    cx.defer(move |cx| {
        for plan in others {
            // Saved windows come back as separate windows where they were
            // (or cascaded when that display is gone), even when macOS prefers
            // tabs for new ones.
            let (bounds, display_id) = plan
                .window_bounds()
                .and_then(|bounds| restored_window_placement(bounds, cx))
                .unwrap_or_else(|| (next_window_bounds(cx), None));
            let saved_origin = plan
                .window_bounds()
                .map(|bounds| gpui::point(px(bounds.x), px(bounds.y)));
            let handle = open_document_window_placed(cx, None, Some(bounds), display_id);
            if let (Some(handle), Some(origin)) = (handle, saved_origin) {
                settle_restored_origin(handle, origin, cx);
            }
            let Some(workspace) = handle
                .and_then(|handle| window_entry(handle, cx))
                .and_then(|entry| entry.workspace.upgrade())
            else {
                continue;
            };
            workspace.update(cx, |workspace, cx| workspace.restore_session(plan, cx));
        }
        if let Some(entry) = document_window_entries(cx)
            .into_iter()
            .find(|entry| entry.workspace == first)
        {
            let _ = entry.handle.update(cx, |_, window, _| window.activate_window());
        }
    });
}

fn deferred_startup_open(action: NativeLaunchAction) -> DeferredStartupOpen {
    match action {
        NativeLaunchAction::None => DeferredStartupOpen::None,
        NativeLaunchAction::OpenExplicit(request) => DeferredStartupOpen::Explicit(request),
        NativeLaunchAction::Restore(plan) => DeferredStartupOpen::Restore(plan),
    }
}

fn apply_deferred_startup_open(
    workspace: &mut DocumentWorkspace,
    action: DeferredStartupOpen,
    cx: &mut Context<DocumentWorkspace>,
) {
    match action {
        DeferredStartupOpen::None => {}
        DeferredStartupOpen::Explicit(request) => {
            workspace.open_documents(request, cx);
        }
        DeferredStartupOpen::Restore(plan) => restore_session_windows(workspace, plan, cx),
    }
}

fn resolve_storage_context(
    perf: Option<&PerfStoryRuntime>,
) -> Result<ResolvedStorageContext, String> {
    if let Some(perf) = perf {
        return NativeStorageLayout::performance(&perf.config.cache_directory)
            .and_then(|layout| {
                let preferences = ApplicationShellPreferencesStore::new(layout.preferences_root())
                    .load()
                    .map_err(|_| "the application preferences are unreadable or invalid")?;
                Ok(ResolvedStorageContext {
                    layout,
                    preferences,
                    release_channel: NativeReleaseChannel::Stable,
                    signature_keychain_service: DEVELOPMENT_SIGNATURE_KEYCHAIN_SERVICE,
                    application_title: APPLICATION_TITLE,
                })
            })
            .map_err(str::to_owned);
    }

    #[cfg(feature = "development-pdfium-override")]
    {
        let run_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "the system clock must follow the Unix epoch".to_owned())?
            .as_nanos();
        return NativeStorageLayout::development(
            application_data_directory(),
            std::env::temp_dir().join(format!(
                "butter-paper-gpui-surfaces-{}-{run_id}",
                std::process::id(),
            )),
        )
        .and_then(|layout| {
            let preferences = ApplicationShellPreferencesStore::new(layout.preferences_root())
                .load()
                .map_err(|_| "the application preferences are unreadable or invalid")?;
            Ok(ResolvedStorageContext {
                layout,
                preferences,
                release_channel: NativeReleaseChannel::Stable,
                signature_keychain_service: DEVELOPMENT_SIGNATURE_KEYCHAIN_SERVICE,
                application_title: APPLICATION_TITLE,
            })
        })
        .map_err(str::to_owned);
    }

    #[cfg(not(feature = "development-pdfium-override"))]
    {
        if std::env::var_os("BP_GPUI_DATA_DIR").is_some() {
            return Err("BP_GPUI_DATA_DIR is not permitted in a production build".to_owned());
        }
        #[cfg(target_os = "macos")]
        let (storage, channel) = {
            let identity = attest_current_release().map_err(|error| error.to_string())?;
            let home = current_user_home_directory().map_err(|error| error.to_string())?;
            let channel = identity.channel();
            (
                NativeProductionStorage::macos(&home, channel).map_err(str::to_owned)?,
                channel,
            )
        };
        #[cfg(target_os = "windows")]
        let (storage, channel) = {
            // The first Windows native package is stable-only. Authenticity is
            // enforced by the Authenticode/package verification release gate,
            // not by accepting a user-controlled runtime channel override.
            let channel = NativeReleaseChannel::Stable;
            (
                windows_production_storage(
                    std::env::var_os("APPDATA").as_deref(),
                    std::env::var_os("LOCALAPPDATA").as_deref(),
                    channel,
                )
                .map_err(str::to_owned)?,
                channel,
            )
        };
        #[cfg(target_os = "linux")]
        let (storage, channel) = {
            // The first Linux native package is stable-only. Package
            // provenance is verified by the release pipeline; runtime storage
            // never accepts a channel from the environment.
            let channel = NativeReleaseChannel::Stable;
            (
                linux_production_storage(
                    std::env::var_os("HOME").as_deref(),
                    std::env::var_os("XDG_DATA_HOME").as_deref(),
                    std::env::var_os("XDG_CACHE_HOME").as_deref(),
                    std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                    channel,
                )
                .map_err(str::to_owned)?,
                channel,
            )
        };
        storage.check_durable_root()?;
        let preferences =
            ApplicationShellPreferencesStore::new(storage.layout().preferences_root())
                .load()
                .map_err(|error| {
                    format!("the native application preferences are invalid: {error}")
                })?;
        Ok(ResolvedStorageContext {
            layout: storage.layout().clone(),
            preferences,
            release_channel: channel,
            signature_keychain_service: channel.signature_keychain_service(),
            application_title: channel.product_name(),
        })
    }
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
    let app_lifecycle = if perf.is_some() {
        AppRootLifecycleReceipt::begin_from_authorized_performance_environment().unwrap_or_else(
            |error| {
                eprintln!("invalid GPUI application lifecycle configuration: {error}");
                std::process::exit(2);
            },
        )
    } else {
        None
    };
    if let Some(app_lifecycle) = app_lifecycle {
        app_lifecycle
            .install_normal_exit_publisher()
            .unwrap_or_else(|error| {
                eprintln!("invalid GPUI application lifecycle configuration: {error}");
                std::process::exit(2);
            });
    }
    let native_runtime_layout = if perf.is_none() {
        let mode = native_runtime_mode_from_environment().unwrap_or_else(|error| {
            exit_for_launch_failure("invalid Butter Paper native runtime mode", &error)
        });
        Some(NativeRuntimeLayout::discover(mode).unwrap_or_else(|error| {
            exit_for_launch_failure(
                "invalid Butter Paper native runtime layout",
                &error.to_string(),
            )
        }))
    } else {
        None
    };
    let storage_context = resolve_storage_context(perf.as_ref()).unwrap_or_else(|error| {
        exit_for_launch_failure("invalid Butter Paper storage or migration", &error)
    });
    let storage_layout = storage_context.layout;
    let startup_preferences = storage_context.preferences;
    let signature_keychain_service = storage_context.signature_keychain_service;
    let application_title = storage_context.application_title;
    let update_settings_root = storage_layout.preferences_root().to_path_buf();
    let release_channel = storage_context.release_channel;
    let session_source = NativeLaunchSessionSource::new(perf.is_some(), &native_launch);
    let native_ingress = NativeDocumentIngress::default();
    let application = gpui_platform::application()
        .with_assets(butter_paper::application_assets::ApplicationAssets);
    application.on_open_urls({
        let native_ingress = native_ingress.clone();
        move |urls| {
            native_ingress.enqueue_file_urls(urls);
        }
    });
    // macOS: clicking the Dock icon with no window open opens one.
    application.on_reopen(|cx| {
        if cx.has_global::<DocumentWindows>()
            && shared_application(cx).multi_window
            && document_window_entries(cx).is_empty()
        {
            open_document_window(cx, None);
        }
    });
    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        init_application_shell_actions(cx);
        init_document_workspace_actions(cx);

        let preferences = startup_preferences;
        cx.set_global(ReverseScrollZoom(preferences.reverse_scroll_zoom()));
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
        let generated_store = GeneratedDocumentStore::new(storage_layout.generated_documents_root())
            .expect("the experiment-owned generated-document store must initialize");
        let session_state_root = storage_layout.session_state_root();
        let (session_store, launch_resolution, recovery_marker) = if session_source.requires_store() {
            let opened = std::fs::create_dir_all(&session_state_root)
                .map_err(|error| error.to_string())
                .and_then(|()| {
                    SessionManifestStore::open(session_state_root.clone())
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
                    let recovery_marker = store
                        .load_recovery_marker()
                        .map_err(|error| format!("{error:?}"));
                    (
                        Some(store),
                        session_source.clone().resolve(loaded),
                        recovery_marker,
                    )
                }
                Err(error) => (
                    None,
                    session_source.clone().resolve(Err(error.clone())),
                    Err(error),
                ),
            }
        } else {
            (None, session_source.clone().resolve(Ok(None)), Ok(None))
        };
        let document_recovery_store = if perf.is_none() {
            Some(
                std::fs::create_dir_all(&session_state_root)
                    .map_err(|error| error.to_string())
                    .and_then(|()| {
                        DocumentRecoveryStore::open(&session_state_root)
                            .map(std::sync::Arc::new)
                            .map_err(|error| error.to_string())
                    }),
            )
        } else {
            None
        };
        if let Some(NativeLaunchWarning::SessionStateUnavailable(message)) =
            launch_resolution.warning.as_ref()
        {
            eprintln!("Butter Paper session state is unavailable: {message}");
        }
        let opener = std::sync::Arc::new(PdfiumWorkerBackend::new(
            worker_executable,
            pdfium_library,
            storage_layout.surface_root().to_owned(),
        ));
        let saver = std::sync::Arc::new(PdfDocumentSaver::new(opener.clone()));
        // Construction performs no credential or filesystem IO on the UI thread.
        // Production binds this service to the authenticated stable/beta channel.
        let recent_signature_store = perf.is_none().then(|| {
            std::sync::Arc::new(RecentSignatureStore::new(
                storage_layout
                    .preferences_root()
                    .join(RECENT_SIGNATURES_FILE_NAME),
                std::sync::Arc::new(PlatformSignatureKeyStore::new(
                    signature_keychain_service,
                    "encryption-key-v1",
                )),
            ))
        });
        let multi_window = perf.is_none();
        cx.set_global(DocumentWindows {
            shared: Rc::new(SharedApplication {
                application_title,
                preferences_store: ApplicationShellPreferencesStore::new(
                    storage_layout.preferences_root(),
                ),
                preferences: Cell::new(preferences),
                ui_zoom_level: Rc::new(Cell::new(preferences.ui_zoom_level())),
                ui_zoom_base_font_size: Rc::new(Cell::new(cx.theme().font_size)),
                opener,
                saver,
                generated_store,
                session_store,
                checkpoint_enabled: launch_resolution.checkpoint_enabled,
                document_recovery_store,
                recent_signature_store,
                template_manager_root: storage_layout.template_library_root(),
                template_authority: std::cell::RefCell::new(None),
                coordinator: Arc::new(WindowSessionCoordinator::default()),
                document_ids: Arc::new(std::sync::atomic::AtomicU64::new(1)),
                multi_window,
            }),
            windows: Vec::new(),
            last_active: None,
        });

        cx.on_window_closed(|cx, window_id| {
            let shared = shared_application(cx);
            let windows = &mut cx.global_mut::<DocumentWindows>().windows;
            windows.retain(|entry| entry.handle.window_id() != window_id);
            let remaining = shared.coordinator.window_closed(window_id.as_u64());
            // The closed window's documents were saved or discarded; refresh
            // the shared dirty-session marker from the windows that remain.
            for story in document_window_stories(cx) {
                story.update(cx, |story, cx| {
                    story.last_observed_recovery_snapshot = None;
                    story.schedule_recovery_marker_checkpoint(cx);
                });
            }
            // macOS apps keep running without windows; elsewhere, and on Quit,
            // the last window ends the process.
            if !remaining
                && (shared.coordinator.is_quitting()
                    || !cfg!(target_os = "macos")
                    || !shared.multi_window)
            {
                cx.quit();
            }
        })
        .detach();

        register_document_workspace_actions_for(
            |may_open_window, cx| {
                if let Some(entry) = active_document_window(cx) {
                    return entry.workspace.upgrade();
                }
                if may_open_window && shared_application(cx).multi_window {
                    return open_document_window(cx, None)
                        .and_then(|handle| window_entry(handle, cx))
                        .and_then(|entry| entry.workspace.upgrade());
                }
                None
            },
            cx,
        );
        register_application_shell_actions(cx);
        updates::init(cx, update_settings_root.clone(), release_channel);
        #[cfg(feature = "review-driver")]
        review_driver::start(cx);

        let native_ingress = native_ingress.clone();
        cx.spawn(async move |cx| {
            cx.update(|cx| {
                open_document_window(
                    cx,
                    Some(StartupWindow {
                        bounds: match &launch_resolution.action {
                            NativeLaunchAction::Restore(plan) => plan.first_window_bounds(),
                            _ => None,
                        },
                        launch_resolution,
                        recovery_marker,
                        perf,
                    }),
                )
            });
            if !multi_window {
                return;
            }
            // Files opened from Finder, the shell or a second launch go to the
            // active window, or a new one when none is open.
            while let Some(request) = native_ingress.next_request().await {
                cx.update(|cx| {
                    let workspace = match active_document_window(cx) {
                        Some(entry) => entry.workspace.upgrade(),
                        None => open_document_window(cx, None)
                            .and_then(|handle| window_entry(handle, cx))
                            .and_then(|entry| entry.workspace.upgrade()),
                    };
                    if let Some(workspace) = workspace {
                        workspace.update(cx, |workspace, cx| {
                            workspace.open_documents(request, cx);
                        });
                    }
                    cx.activate(true);
                });
            }
        })
        .detach();
    });
}

/// State shared by every document window.
struct SharedApplication {
    application_title: &'static str,
    preferences_store: ApplicationShellPreferencesStore,
    preferences: Cell<ApplicationShellPreferences>,
    ui_zoom_level: Rc<Cell<i8>>,
    ui_zoom_base_font_size: Rc<Cell<gpui::Pixels>>,
    opener: Arc<PdfiumWorkerBackend>,
    saver: Arc<PdfDocumentSaver>,
    generated_store: GeneratedDocumentStore,
    session_store: Option<Arc<SessionManifestStore>>,
    checkpoint_enabled: bool,
    document_recovery_store: Option<Result<Arc<DocumentRecoveryStore>, String>>,
    recent_signature_store: Option<Arc<RecentSignatureStore>>,
    template_manager_root: std::path::PathBuf,
    /// One template library authority for every window, created with the first.
    template_authority: std::cell::RefCell<Option<Arc<std::sync::Mutex<PersistentTemplateManager>>>>,
    coordinator: Arc<WindowSessionCoordinator>,
    /// Document ids are unique across windows so a tab can move between them.
    document_ids: Arc<std::sync::atomic::AtomicU64>,
    /// Performance runs keep the original single window.
    multi_window: bool,
}

#[derive(Clone)]
struct DocumentWindowEntry {
    handle: AnyWindowHandle,
    story: gpui::WeakEntity<ComponentStory>,
    workspace: gpui::WeakEntity<DocumentWorkspace>,
    close: gpui::WeakEntity<ApplicationCloseWorkspace>,
}

struct DocumentWindows {
    shared: Rc<SharedApplication>,
    windows: Vec<DocumentWindowEntry>,
    /// The document window most recently made active. Asking the platform
    /// for its window order is a synchronous WindowServer round trip on
    /// macOS, too slow for paths that run on every document change.
    last_active: Option<AnyWindowHandle>,
}

impl gpui::Global for DocumentWindows {}

struct StartupWindow {
    launch_resolution: NativeLaunchResolution,
    recovery_marker: Result<
        Option<butter_paper::session_manifest::SessionRecoverySnapshot>,
        String,
    >,
    perf: Option<PerfStoryRuntime>,
    /// Where the restored session's first window was.
    bounds: Option<butter_paper::session_manifest::SessionWindowBounds>,
}

fn shared_application(cx: &App) -> Rc<SharedApplication> {
    cx.global::<DocumentWindows>().shared.clone()
}

fn document_window_entries(cx: &App) -> Vec<DocumentWindowEntry> {
    cx.try_global::<DocumentWindows>()
        .map(|windows| windows.windows.clone())
        .unwrap_or_default()
}

fn window_entry(handle: AnyWindowHandle, cx: &App) -> Option<DocumentWindowEntry> {
    document_window_entries(cx)
        .into_iter()
        .find(|entry| entry.handle == handle)
}

fn document_window_stories(cx: &App) -> Vec<Entity<ComponentStory>> {
    document_window_entries(cx)
        .iter()
        .filter_map(|entry| entry.story.upgrade())
        .collect()
}

fn document_window_workspaces(cx: &App) -> Vec<Entity<DocumentWorkspace>> {
    document_window_entries(cx)
        .iter()
        .filter_map(|entry| entry.workspace.upgrade())
        .collect()
}

/// The key document window, else the frontmost one, else the newest.
fn active_document_window(cx: &App) -> Option<DocumentWindowEntry> {
    let entries = document_window_entries(cx);
    let find = |handle: AnyWindowHandle| entries.iter().find(|entry| entry.handle == handle).cloned();
    cx.active_window()
        .and_then(find)
        .or_else(|| {
            cx.try_global::<DocumentWindows>()
                .and_then(|windows| windows.last_active)
                .and_then(find)
        })
        .or_else(|| {
            cx.window_stack()
                .unwrap_or_default()
                .into_iter()
                .find_map(find)
        })
        .or_else(|| entries.last().cloned())
}

fn owns_application_menus(handle: AnyWindowHandle, cx: &App) -> bool {
    active_document_window(cx).is_none_or(|entry| entry.handle == handle)
}

/// Keeps the session coordinator's record of where `window` is, so the
/// restart manifest reopens it in the same place.
fn record_window_bounds(window: &Window, cx: &App) {
    let bounds = (!window.is_fullscreen()).then(|| {
        let frame = window.bounds();
        let content = window.viewport_size();
        butter_paper::session_manifest::SessionWindowBounds {
            x: f32::from(frame.origin.x),
            y: f32::from(frame.origin.y),
            width: f32::from(content.width),
            height: f32::from(content.height),
            display: window
                .display(cx)
                .and_then(|display| display.uuid().ok())
                .map(|uuid| uuid.to_string()),
        }
    });
    shared_application(cx)
        .coordinator
        .set_window_bounds(window.window_handle().window_id().as_u64(), bounds);
}

/// Platforms place a new window's frame by slightly different conventions
/// than they report it (Windows reports the client area; X11 window managers
/// place new windows themselves). Once the window is shown, nudge it so the
/// reported origin and content size match the saved ones, and positions do
/// not drift across relaunches.
fn settle_restored_origin(handle: AnyWindowHandle, origin: gpui::Point<gpui::Pixels>, cx: &mut App) {
    let content = handle.update(cx, |_, window, _| window.viewport_size()).ok();
    cx.spawn(async move |cx| {
        // What was last asked for; any constant offset between that and the
        // reported origin (decoration insets) is compensated next time.
        let mut requested = origin;
        for _ in 0..4 {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let settled = cx
                .update(|cx| {
                    handle.update(cx, |_, window, _| {
                        if !window.can_set_origin() {
                            return true;
                        }
                        let mut settled = true;
                        if let Some(content) = content
                            && window.viewport_size() != content
                        {
                            window.resize(content);
                            settled = false;
                        }
                        let actual = window.bounds().origin;
                        if actual != origin {
                            requested = origin - (actual - requested);
                            window.set_origin(requested);
                            settled = false;
                        }
                        settled
                    })
                })
                .unwrap_or(true);
            if settled {
                break;
            }
        }
    })
    .detach();
}

/// Window placement for a restored window, if its display is still present.
fn restored_window_placement(
    bounds: &butter_paper::session_manifest::SessionWindowBounds,
    cx: &App,
) -> Option<(WindowBounds, Option<gpui::DisplayId>)> {
    let display = match &bounds.display {
        Some(saved) => Some(
            cx.displays()
                .into_iter()
                .find(|display| display.uuid().is_ok_and(|uuid| uuid.to_string() == *saved))?,
        ),
        None => None,
    };
    Some((
        WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(bounds.x), px(bounds.y)),
            size: size(px(bounds.width), px(bounds.height)),
        }),
        display.map(|display| display.id()),
    ))
}

/// New windows cascade from the active one, as macOS document apps do.
fn next_window_bounds(cx: &mut App) -> WindowBounds {
    let active = active_document_window(cx).and_then(|entry| {
        entry
            .handle
            .update(cx, |_, window, _| (window.bounds().origin, window.viewport_size()))
            .ok()
    });
    match active {
        // Window sizes are content sizes; the frame adds the title bar.
        Some((origin, content)) => WindowBounds::Windowed(gpui::Bounds {
            origin: origin + gpui::point(px(28.), px(28.)),
            size: content,
        }),
        None => WindowBounds::centered(size(px(1200.), px(800.)), cx),
    }
}

/// Opens a document window. The startup window restores the session and
/// inspects recovery; later windows start empty and share every store.
fn open_document_window(cx: &mut App, startup: Option<StartupWindow>) -> Option<AnyWindowHandle> {
    let placement = startup
        .as_ref()
        .and_then(|startup| startup.bounds.as_ref())
        .and_then(|bounds| restored_window_placement(bounds, cx));
    match placement {
        Some((bounds, display_id)) => {
            let origin = bounds.get_bounds().origin;
            let handle = open_document_window_placed(cx, startup, Some(bounds), display_id);
            if let Some(handle) = handle {
                settle_restored_origin(handle, origin, cx);
            }
            handle
        }
        None => open_document_window_placed(cx, startup, None, None),
    }
}

fn open_document_window_with_bounds(
    cx: &mut App,
    startup: Option<StartupWindow>,
    bounds: Option<WindowBounds>,
) -> Option<AnyWindowHandle> {
    open_document_window_placed(cx, startup, bounds, None)
}

fn open_document_window_placed(
    cx: &mut App,
    startup: Option<StartupWindow>,
    bounds: Option<WindowBounds>,
    display_id: Option<gpui::DisplayId>,
) -> Option<AnyWindowHandle> {
    let shared = shared_application(cx);
    let is_startup = startup.is_some();
    if !is_startup && document_window_entries(cx).is_empty() {
        // The last window's clean close stopped live marker writes.
        if let Some(store) = shared.session_store.as_ref() {
            store.resume_live_writes();
        }
    }
    // A window placed explicitly (a torn-off tab) must stay a separate window
    // even when macOS prefers tabs; it joins the tab group identity only
    // after opening, so Merge All Windows still includes it.
    let separate = bounds.is_some();
    let tabbing_identifier = shared
        .multi_window
        .then(|| DOCUMENT_WINDOW_TABBING_IDENTIFIER.to_owned());
    let window_options = WindowOptions {
        window_bounds: Some(bounds.unwrap_or_else(|| next_window_bounds(cx))),
        tabbing_identifier: if separate { None } else { tabbing_identifier.clone() },
        display_id,
        ..title_bar_window_options()
    };
    let handle = cx
        .open_window(window_options, |window, cx| {
            build_document_window(shared.clone(), startup, window, cx)
        })
        .map_err(|error| eprintln!("Butter Paper could not open a window: {error}"))
        .ok()?;
    if separate && tabbing_identifier.is_some() {
        let _ = handle.update(cx, |_, window, _| {
            window.set_tabbing_identifier(tabbing_identifier)
        });
    }
    Some(handle.into())
}

fn build_document_window(
    shared: Rc<SharedApplication>,
    startup: Option<StartupWindow>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Root> {
    let window_handle = window.window_handle();
    let window_id = window_handle.window_id().as_u64();
    window.set_window_title(shared.application_title);
    let system_theme_subscription = follow_window_appearance_with_application_zoom(
        window,
        cx,
        shared.ui_zoom_level.clone(),
        shared.ui_zoom_base_font_size.clone(),
    );
    let app_menu_bar = AppMenuBar::new(cx);
    let (launch_resolution, recovery_marker, perf) = match startup {
        Some(startup) => (
            Some(startup.launch_resolution),
            Some(startup.recovery_marker),
            startup.perf,
        ),
        None => (None, None, None),
    };
    let document_workspace = cx.new(|cx| {
        let mut workspace = DocumentWorkspace::with_opener_and_generated_store(
            shared.opener.clone(),
            shared.generated_store.clone(),
            cx,
        );
        if let Some(store) = &shared.document_recovery_store {
            match store {
                Ok(store) => workspace.bind_document_recovery_store(store.clone()),
                Err(error) => workspace.bind_document_recovery_store_error(error.clone()),
            }
        }
        if let Some(store) = &shared.recent_signature_store {
            workspace.bind_recent_signature_store(store.clone());
        }
        if shared.multi_window {
            workspace.set_other_window_document_focus(focus_document_in_other_window);
            workspace.share_document_ids(shared.document_ids.clone());
        }
        workspace
    });
    if let Some(recovery_marker) = &recovery_marker {
        document_workspace.update(cx, |workspace, cx| match recovery_marker {
            Ok(Some(snapshot)) => workspace.show_session_recovery_warning(snapshot, cx),
            Ok(None) => {}
            Err(error) => workspace.show_session_recovery_warning_message(
                format!(
                    "Butter Paper could not inspect the previous unsaved-change marker. Verify your documents before continuing. Details: {error}"
                ),
                cx,
            ),
        });
    }
    if let Some(launch_resolution) = &launch_resolution {
        begin_startup_launch(
            &document_workspace,
            launch_resolution,
            &shared.document_recovery_store,
            cx,
        );
    }
    let application_close = cx.new(|_| {
        match shared.session_store.clone() {
            Some(store) if shared.checkpoint_enabled => {
                let checkpoint_publisher: Arc<dyn ApplicationCloseCheckpointPublisher> =
                    Arc::new(WindowCheckpointPublisher::new(
                        window_id,
                        shared.coordinator.clone(),
                        store,
                    ));
                ApplicationCloseWorkspace::with_checkpoint_publisher(
                    document_workspace.clone(),
                    shared.saver.clone(),
                    checkpoint_publisher,
                )
            }
            _ => ApplicationCloseWorkspace::new(document_workspace.clone(), shared.saver.clone()),
        }
    });
    let template_manager = shared.multi_window.then(|| {
        cx.new(|cx| {
            let existing = shared.template_authority.borrow().clone();
            let mut manager = match existing {
                Some(authority) => TemplateManagerView::open_shared(authority, window, cx),
                None => {
                    let legacy_request = std::env::var("BP_LEGACY_BLANK_SETTINGS_JSON")
                        .ok()
                        .and_then(|json| legacy_blank_request_from_json(&json).ok());
                    TemplateManagerView::open_persistent_with_legacy(
                        shared.template_manager_root.clone(),
                        legacy_request,
                        window,
                        cx,
                    )
                }
            }
            .expect("the experiment-owned template library must initialize");
            if shared.template_authority.borrow().is_none() {
                *shared.template_authority.borrow_mut() = manager.shared_persistent();
            }
            manager.bind_document_workspace(
                document_workspace.downgrade(),
                shared.generated_store.clone(),
            );
            manager
        })
    });
    let story = cx.new(|cx| {
        ComponentStory::new(
            document_workspace.clone(),
            app_menu_bar,
            cfg!(target_os = "macos"),
            shared.ui_zoom_level.clone(),
            shared.ui_zoom_base_font_size.clone(),
            template_manager,
            shared.session_store.clone(),
            perf,
            shared.application_title,
            system_theme_subscription,
            window,
            cx,
        )
    });
    shared.coordinator.window_opened(window_id);
    if shared.multi_window {
        cx.subscribe(&document_workspace, move |_, event: &DocumentTabTransferEvent, cx| {
            handle_tab_transfer(window_handle, *event, cx);
        })
        .detach();
    }
    cx.subscribe(&application_close, |_, _: &ApplicationCloseCancelled, cx| {
        shared_application(cx).coordinator.cancel_quit();
    })
    .detach();
    cx.global_mut::<DocumentWindows>().windows.push(DocumentWindowEntry {
        handle: window_handle,
        story: story.downgrade(),
        workspace: document_workspace.downgrade(),
        close: application_close.downgrade(),
    });
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
                let Ok(done) = story_for_monitor.update(cx, |story, cx| story.observe_perf_document(cx))
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
    if story.read(cx).perf.is_none() {
        focus_initial_command_context(&document_workspace.read(cx).focus_handle(), window);
    }
    root
}

const DOCUMENT_WINDOW_TABBING_IDENTIFIER: &str = "butter-paper-documents";

/// Whether `handle` is on screen: not a background member of a macOS native
/// tab group, where every member shares one frame.
fn shown_in_tab_group(handle: AnyWindowHandle, cx: &App) -> bool {
    let Some(controller) = cx.try_global::<gpui::SystemWindowTabController>() else {
        return true;
    };
    let Some(tabs) = controller.tabs(handle.window_id()) else {
        return true;
    };
    tabs.len() <= 1
        || tabs
            .iter()
            .max_by_key(|tab| tab.last_active_at)
            .is_none_or(|tab| tab.id == handle.window_id())
}

fn in_same_tab_group(first: AnyWindowHandle, second: AnyWindowHandle, cx: &App) -> bool {
    cx.try_global::<gpui::SystemWindowTabController>()
        .and_then(|controller| controller.tabs(first.window_id()))
        .is_some_and(|tabs| tabs.iter().any(|tab| tab.id == second.window_id()))
}

/// A window-local point in screen space. Every document window has the same
/// frame chrome, so frame origin plus local point compares across windows.
fn window_screen_point(
    handle: AnyWindowHandle,
    local: gpui::Point<gpui::Pixels>,
    cx: &mut App,
) -> Option<gpui::Point<gpui::Pixels>> {
    handle
        .update(cx, |_, window, _| {
            // Window coordinates start below the title bar.
            let frame = window.bounds();
            let title_bar = frame.size.height - window.viewport_size().height;
            frame.origin + gpui::point(px(0.), title_bar) + local
        })
        .ok()
}

/// The floating copy of a dragged tab: a small borderless window that follows
/// the pointer beyond the source window, as browsers show a dragged tab.
struct TabDragChip {
    text: gpui::SharedString,
}

/// The chip's text: the tab's name, or what a drop there will do.
fn tab_drag_chip_text(label: &str, opens_new_window: bool) -> String {
    if opens_new_window {
        format!("Open {label} in New window")
    } else {
        label.to_owned()
    }
}

/// Wide enough for the text at the interface font size, within limits.
fn tab_drag_chip_size(text: &str) -> gpui::Size<gpui::Pixels> {
    let width = (text.chars().count() as f32 * 7.5 + 32.).clamp(96., 420.);
    gpui::size(px(width), px(32.))
}

impl Render for TabDragChip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_sm()
            .text_color(cx.theme().foreground)
            .whitespace_nowrap()
            .text_ellipsis()
            .child(self.text.clone())
    }
}

struct TabDragPreview {
    handle: gpui::WindowHandle<TabDragChip>,
    chip: Entity<TabDragChip>,
}

impl gpui::Global for TabDragPreview {}

/// Shows or moves the floating tab under the pointer at `screen`.
fn show_tab_drag_chip(
    source: AnyWindowHandle,
    text: String,
    screen: gpui::Point<gpui::Pixels>,
    cx: &mut App,
) {
    // Where windows cannot be positioned (Wayland), keep the in-window copy.
    let movable = source
        .update(cx, |_, window, _| window.can_set_origin())
        .unwrap_or(false);
    if !movable {
        return;
    }
    let origin = screen - gpui::point(px(24.), px(16.));
    let chip_size = tab_drag_chip_size(&text);
    if let Some(preview) = cx.try_global::<TabDragPreview>() {
        let (handle, chip) = (preview.handle, preview.chip.clone());
        let changed = chip.read(cx).text.as_ref() != text;
        chip.update(cx, |chip, cx| {
            chip.text = text.into();
            cx.notify();
        });
        let _ = handle.update(cx, |_, window, _| {
            if changed {
                window.resize(chip_size);
            }
            window.set_origin(origin);
        });
        return;
    }
    let slot = Rc::new(std::cell::RefCell::new(None));
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds {
                origin,
                size: chip_size,
            })),
            titlebar: None,
            focus: false,
            show: true,
            kind: gpui::WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        {
            let slot = slot.clone();
            move |_, cx| {
                let chip = cx.new(|_| TabDragChip { text: text.into() });
                slot.replace(Some(chip.clone()));
                chip
            }
        },
    );
    // A window without a tabbing identifier turns off automatic tabbing for
    // the whole app; restore it so Merge All Windows keeps working.
    let _ = source.update(cx, |_, window, _| {
        window.set_tabbing_identifier(Some(DOCUMENT_WINDOW_TABBING_IDENTIFIER.to_owned()))
    });
    if let (Ok(handle), Some(chip)) = (opened, slot.borrow_mut().take()) {
        cx.set_global(TabDragPreview { handle, chip });
        set_external_tab_drag_preview(true, cx);
    }
}

fn hide_tab_drag_chip(cx: &mut App) {
    set_external_tab_drag_preview(false, cx);
    if cx.has_global::<TabDragPreview>() {
        let preview = cx.remove_global::<TabDragPreview>();
        let _ = preview.handle.update(cx, |_, window, _| window.remove_window());
    }
}

fn set_external_tab_drag_preview(active: bool, cx: &mut App) {
    for workspace in document_window_workspaces(cx) {
        workspace.update(cx, |workspace, cx| workspace.set_external_tab_drag_preview(active, cx));
    }
}

/// The frontmost other document window whose tab strip is under `screen`,
/// and the tab index there.
fn tab_drop_target(
    source: AnyWindowHandle,
    screen: gpui::Point<gpui::Pixels>,
    cx: &mut App,
) -> Option<(DocumentWindowEntry, usize)> {
    // Without global window positions (Wayland) another window's strip cannot
    // be located, so a drop beyond the strip only tears the tab off.
    if !source
        .update(cx, |_, window, _| window.can_set_origin())
        .unwrap_or(false)
    {
        return None;
    }
    let entries = document_window_entries(cx);
    let order = cx.window_stack().unwrap_or_default();
    let mut candidates = entries
        .into_iter()
        .filter(|entry| {
            entry.handle != source
                && !in_same_tab_group(source, entry.handle, cx)
                && shown_in_tab_group(entry.handle, cx)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| {
        order
            .iter()
            .position(|handle| *handle == entry.handle)
            .unwrap_or(usize::MAX)
    });
    for entry in candidates {
        let Some(workspace) = entry.workspace.upgrade() else {
            continue;
        };
        let Ok((frame, content_origin)) = entry.handle.update(cx, |_, window, _| {
            let frame = window.bounds();
            let title_bar = frame.size.height - window.viewport_size().height;
            (frame, frame.origin + gpui::point(px(0.), title_bar))
        }) else {
            continue;
        };
        if !frame.contains(&screen) {
            continue;
        }
        // Window coordinates start below the title bar, as in `window_screen_point`.
        let local = screen - content_origin;
        // The frontmost window under the pointer takes the drop, even when
        // the pointer is not over its tab strip.
        return workspace
            .read(cx)
            .tab_insertion_index(local, cx)
            .map(|index| (entry, index));
    }
    None
}

fn clear_incoming_tab_drops(cx: &mut App) {
    for workspace in document_window_workspaces(cx) {
        workspace.update(cx, |workspace, cx| workspace.set_incoming_tab_drop(None, cx));
    }
}

/// Browser-style tab moves: drop on another window's tab strip to move the
/// tab there; drop elsewhere to open it in a new window (when the source has
/// other tabs). A window left without tabs closes.
fn handle_tab_transfer(source: AnyWindowHandle, event: DocumentTabTransferEvent, cx: &mut App) {
    match event {
        DocumentTabTransferEvent::Dragging {
            document_id,
            position,
            outside_strip,
        } => {
            let screen = window_screen_point(source, position, cx);
            let target = if outside_strip {
                screen.and_then(|screen| tab_drop_target(source, screen, cx))
            } else {
                None
            };
            let source_workspace = window_entry(source, cx).and_then(|entry| entry.workspace.upgrade());
            match (outside_strip, screen, source_workspace) {
                (true, Some(screen), Some(workspace)) => {
                    let (label, tabs) = {
                        let workspace = workspace.read(cx);
                        let label = workspace
                            .document_title(document_id, cx)
                            .map(|title| {
                                butter_paper::document_tab_bar::format_document_tab_label(&title)
                                    .to_owned()
                            })
                            .unwrap_or_default();
                        (label, workspace.session_count())
                    };
                    let text = tab_drag_chip_text(&label, target.is_none() && tabs > 1);
                    show_tab_drag_chip(source, text, screen, cx);
                }
                _ => hide_tab_drag_chip(cx),
            }
            for entry in document_window_entries(cx) {
                let index = target
                    .as_ref()
                    .filter(|(target, _)| target.handle == entry.handle)
                    .map(|(_, index)| *index);
                if let Some(workspace) = entry.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| workspace.set_incoming_tab_drop(index, cx));
                }
            }
        }
        DocumentTabTransferEvent::Ended => {
            hide_tab_drag_chip(cx);
            clear_incoming_tab_drops(cx);
        }
        DocumentTabTransferEvent::Dropped {
            document_id,
            position,
        } => {
            hide_tab_drag_chip(cx);
            clear_incoming_tab_drops(cx);
            let Some(screen) = window_screen_point(source, position, cx) else {
                return;
            };
            let target = tab_drop_target(source, screen, cx);
            cx.defer(move |cx| move_document_between_windows(source, document_id, target, screen, cx));
        }
    }
}

fn move_document_between_windows(
    source: AnyWindowHandle,
    document_id: DocumentId,
    target: Option<(DocumentWindowEntry, usize)>,
    screen: gpui::Point<gpui::Pixels>,
    cx: &mut App,
) {
    let Some(source_entry) = window_entry(source, cx) else {
        return;
    };
    let Some(source_workspace) = source_entry.workspace.upgrade() else {
        return;
    };
    let source_tabs = source_workspace.read(cx).session_count();
    if !source_workspace.read(cx).can_transfer_document(document_id, cx) {
        return;
    }
    let (target_workspace, index) = match target {
        Some((entry, index)) => {
            let Some(workspace) = entry.workspace.upgrade() else {
                return;
            };
            let _ = entry.handle.update(cx, |_, window, _| window.activate_window());
            (workspace, index)
        }
        // Only a window with other tabs can give one up to a new window.
        None if source_tabs > 1 => {
            // Window sizes are content sizes; the frame adds the title bar.
            let Ok(content) = source.update(cx, |_, window, _| window.viewport_size()) else {
                return;
            };
            // Put the new window's tab strip under the pointer.
            let origin = screen - gpui::point(px(120.), px(20.));
            let bounds = WindowBounds::Windowed(gpui::Bounds {
                origin,
                size: content,
            });
            let Some(entry) = open_document_window_with_bounds(cx, None, Some(bounds))
                .and_then(|handle| window_entry(handle, cx))
            else {
                return;
            };
            let Some(workspace) = entry.workspace.upgrade() else {
                return;
            };
            (workspace, 0)
        }
        None => return,
    };
    let detached = source_workspace.update(cx, |workspace, cx| workspace.detach_document(document_id, cx));
    match detached {
        Ok(document) => {
            target_workspace.update(cx, |workspace, cx| {
                workspace.attach_document(document, index, cx);
            });
        }
        Err(error) => {
            eprintln!("Butter Paper could not move the document: {error}");
            return;
        }
    }
    if source_workspace.read(cx).session_count() == 0 {
        request_window_close(&source_entry, cx);
    }
}

/// Brings forward the window and tab already showing `path`, if another
/// window has it open, instead of opening a second copy.
fn focus_document_in_other_window(own: gpui::EntityId, path: &std::path::Path, cx: &mut App) -> bool {
    for entry in document_window_entries(cx) {
        let Some(workspace) = entry.workspace.upgrade() else {
            continue;
        };
        if workspace.entity_id() == own {
            continue;
        }
        let Some(document_id) = workspace.read(cx).document_id_for_path(path, cx) else {
            continue;
        };
        workspace.update(cx, |workspace, cx| workspace.activate_document(document_id, cx));
        let _ = entry.handle.update(cx, |_, window, _| window.activate_window());
        return true;
    }
    false
}

/// The startup window's session restore, deferred behind recovery inspection
/// when a recovery store is available.
fn begin_startup_launch(
    document_workspace: &Entity<DocumentWorkspace>,
    launch_resolution: &NativeLaunchResolution,
    document_recovery_store: &Option<Result<Arc<DocumentRecoveryStore>, String>>,
    cx: &mut App,
) {
    let launch_action = launch_resolution.action.clone();
    match document_recovery_store.as_ref() {
        Some(Ok(store)) => {
            document_workspace.update(cx, |workspace, cx| {
                workspace.defer_startup_open(deferred_startup_open(
                    launch_action.clone(),
                ));
                workspace.begin_startup_recovery_inspection(cx);
            });
            let store = store.clone();
            let workspace = document_workspace.downgrade();
            let task = cx.background_executor().spawn(async move {
                let ids = store.active_document_ids().map_err(|error| error.to_string())?;
                let mut items = Vec::new();
                for id in ids {
                    match store.load(id) {
                        Ok(Some(recovered))
                            if recovered.current_revision == recovered.saved_revision
                                && !recovered.requires_save_as =>
                        {
                            if let Err(error) =
                                store.clear_authority(&recovered.authority)
                            {
                                items.push(StartupRecoveryItem {
                                    id,
                                    authority: Some(recovered.authority),
                                    source_path: Some(recovered.source_path),
                                    current_revision: Some(recovered.current_revision),
                                    saved_revision: Some(recovered.saved_revision),
                                    availability: StartupRecoveryAvailability::Unavailable(
                                        format!(
                                            "the clean checkpoint could not be retired safely: {error}"
                                        ),
                                    ),
                                });
                            }
                        }
                        Ok(Some(recovered)) => items.push(StartupRecoveryItem {
                            id,
                            authority: Some(recovered.authority),
                            source_path: Some(recovered.source_path),
                            current_revision: Some(recovered.current_revision),
                            saved_revision: Some(recovered.saved_revision),
                            availability: match recovered.source_kind {
                                RecoverySourceKind::Opened => {
                                    StartupRecoveryAvailability::OpenedSourceNeedsVerification
                                }
                                RecoverySourceKind::Generated => {
                                    StartupRecoveryAvailability::GeneratedCopyRequired
                                }
                            },
                        }),
                        Ok(None) => {}
                        Err(error) => items.push(StartupRecoveryItem {
                            id,
                            authority: None,
                            source_path: None,
                            current_revision: None,
                            saved_revision: None,
                            availability: StartupRecoveryAvailability::Unavailable(
                                error.to_string(),
                            ),
                        }),
                    }
                }
                Ok::<_, String>(items)
            });
            cx.spawn(async move |cx| {
                match task.await {
                    Ok(items) => {
                        let should_launch = items.is_empty();
                        let _ = workspace.update(cx, |workspace, cx| {
                            workspace.finish_startup_recovery_inspection(items, cx);
                            if should_launch {
                                if let Some(open) =
                                    workspace.take_deferred_startup_open()
                                {
                                    apply_deferred_startup_open(workspace, open, cx);
                                }
                            }
                        });
                    }
                    Err(error) => {
                        let _ = workspace.update(cx, |workspace, cx| {
                            workspace.show_session_recovery_warning_message(
                                format!(
                                    "Butter Paper could not inspect recoverable unsaved changes. Opening documents is paused to protect them. Details: {error}"
                                ),
                                cx,
                            );
                        });
                    }
                }
            })
            .detach();
        }
        _ => {
            document_workspace.update(cx, |workspace, cx| {
                apply_launch_action(workspace, launch_action, cx);
            });
        }
    }
}

/// Development-only review driver. With `BP_REVIEW_SCRIPT`, the app drives
/// its own windows with scripted input and saves frames of its own rendering
/// to `BP_REVIEW_OUT`, for frame-by-frame review. It sends no OS-level input
/// and captures nothing outside this process.
///
/// Script lines (window numbers follow opening order; points are in the
/// source window's coordinates unless noted):
///   wait MS
///   shot NAME                      every window, plus a state log line
///   new-window | quit | close W | move-to-new-window
///   drag-tab W TAB off DX DY NAME  drag tab TAB by (DX, DY) from its centre
///   drag-tab W TAB into W2 INDEX NAME
///                                  drag onto window W2's strip before INDEX
/// Self-update: scheduled and manual checks, background download and
/// preparation, and the handover that installs the update when the app quits.
mod updates {
    use std::{path::PathBuf, time::Duration};

    use butter_paper::{
        application_close_workspace::RequestApplicationQuit,
        application_shell::{
            CheckForUpdates, RestartToUpdate, SetUpdateFrequencyAtStartup,
            SetUpdateFrequencyDaily, SetUpdateFrequencyEverySixHours,
            SetUpdateFrequencyEveryTwelveHours, SetUpdateFrequencyHourly,
            SetUpdateFrequencyMonthly, SetUpdateFrequencyNever, SetUpdateFrequencyWeekly,
        },
        native_application::{UpdateMenuState, UpdateMenuStatus},
        native_storage_layout::NativeReleaseChannel,
        native_update_policy::{
            UpdateFrequency, UpdateSettings, UpdateSettingsStore, format_canonical_utc_timestamp,
        },
        native_updater::{
            AvailableUpdate, Installation, PreparedUpdate, RELEASE_FEED_URL, ReleaseVersion,
            UpdateChannel, UpdateError, UpdateTarget, current_installation, download_update,
            fetch_releases, prepare_update, select_update,
        },
    };
    use gpui::{App, Global};
    use gpui_component::{WindowExt as _, button::Button, notification::Notification};

    enum Status {
        Idle,
        Checking,
        Downloading,
        Ready(PreparedUpdate),
    }

    struct UpdateService {
        channel: UpdateChannel,
        current: ReleaseVersion,
        /// `None` for development builds and copies run outside an install.
        installation: Option<Installation>,
        feed_url: String,
        store: UpdateSettingsStore,
        settings: UpdateSettings,
        status: Status,
        relaunch_after_quit: bool,
    }

    impl Global for UpdateService {}

    pub(super) fn init(cx: &mut App, settings_root: PathBuf, channel: NativeReleaseChannel) {
        let current = ReleaseVersion::parse(env!("CARGO_PKG_VERSION"))
            .expect("the package version is major.minor.patch");
        let store = UpdateSettingsStore::new(settings_root, channel);
        let settings = store.load().unwrap_or_else(|_| UpdateSettings::defaults(channel));
        // Production builds only: development builds never replace themselves.
        let installation = (!cfg!(feature = "development-pdfium-override"))
            .then(|| std::env::current_exe().ok())
            .flatten()
            .and_then(|executable| std::fs::canonicalize(executable).ok())
            .and_then(|executable| current_installation(&executable, current));
        cx.set_global(UpdateService {
            channel: match channel {
                NativeReleaseChannel::Stable => UpdateChannel::Stable,
                NativeReleaseChannel::Beta => UpdateChannel::Beta,
            },
            current,
            installation,
            feed_url: RELEASE_FEED_URL.to_owned(),
            store,
            settings,
            status: Status::Idle,
            relaunch_after_quit: false,
        });

        cx.on_action(|_: &CheckForUpdates, cx| check(cx, true));
        cx.on_action(|_: &RestartToUpdate, cx| {
            cx.global_mut::<UpdateService>().relaunch_after_quit = true;
            cx.dispatch_action(&RequestApplicationQuit);
        });
        set_frequency_on::<SetUpdateFrequencyNever>(cx, UpdateFrequency::Never);
        set_frequency_on::<SetUpdateFrequencyAtStartup>(cx, UpdateFrequency::Startup);
        set_frequency_on::<SetUpdateFrequencyHourly>(cx, UpdateFrequency::Hourly);
        set_frequency_on::<SetUpdateFrequencyEverySixHours>(cx, UpdateFrequency::SixHours);
        set_frequency_on::<SetUpdateFrequencyEveryTwelveHours>(cx, UpdateFrequency::TwelveHours);
        set_frequency_on::<SetUpdateFrequencyDaily>(cx, UpdateFrequency::Daily);
        set_frequency_on::<SetUpdateFrequencyWeekly>(cx, UpdateFrequency::Weekly);
        set_frequency_on::<SetUpdateFrequencyMonthly>(cx, UpdateFrequency::Monthly);

        // A prepared update installs whenever the app quits.
        cx.on_app_quit(|cx| {
            if let Some(service) = cx.try_global::<UpdateService>()
                && let Status::Ready(prepared) = &service.status
                && let Err(error) = prepared.spawn_handover(service.relaunch_after_quit)
            {
                eprintln!("Butter Paper could not start its update: {error}");
            }
            async {}
        })
        .detach();

        if cx.global::<UpdateService>().installation.is_none() {
            return;
        }
        // Scheduled checks: shortly after launch, then hourly re-evaluation.
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(10)).await;
            let mut startup = true;
            loop {
                let due = cx
                    .update(|cx| {
                        let settings = &cx.global::<UpdateService>().settings;
                        let now = format_canonical_utc_timestamp(std::time::SystemTime::now());
                        match settings.frequency() {
                            UpdateFrequency::Startup => startup,
                            _ => settings.is_check_due(&now).unwrap_or(true),
                        }
                    });
                if due {
                    let _ = cx.update(|cx| check(cx, false));
                }
                startup = false;
                cx.background_executor().timer(Duration::from_secs(60 * 60)).await;
            }
        })
        .detach();
    }

    fn set_frequency_on<A: gpui::Action>(cx: &mut App, frequency: UpdateFrequency) {
        cx.on_action(move |_: &A, cx| {
            let service = cx.global_mut::<UpdateService>();
            service.settings.set_frequency(frequency);
            if let Err(error) = service.store.save(&service.settings) {
                eprintln!("Butter Paper could not save its update settings: {error}");
            }
            refresh_menus(cx);
        });
    }

    pub(super) fn menu_state(cx: &App) -> UpdateMenuState {
        let Some(service) = cx.try_global::<UpdateService>() else {
            return UpdateMenuState::default();
        };
        if service.installation.is_none() {
            return UpdateMenuState::default();
        }
        UpdateMenuState {
            status: match &service.status {
                Status::Idle => UpdateMenuStatus::Idle,
                Status::Checking => UpdateMenuStatus::Checking,
                Status::Downloading => UpdateMenuStatus::Downloading,
                Status::Ready(prepared) => UpdateMenuStatus::Ready(prepared.version),
            },
            frequency: Some(service.settings.frequency()),
        }
    }

    fn refresh_menus(cx: &mut App) {
        for story in super::document_window_stories(cx) {
            story.update(cx, |story, cx| story.sync_native_application_menu(cx));
        }
    }

    fn notify(cx: &mut App, notification: Notification) {
        if let Some(entry) = super::active_document_window(cx) {
            let _ = entry
                .handle
                .update(cx, |_, window, cx| window.push_notification(notification, cx));
        }
    }

    fn check(cx: &mut App, manual: bool) {
        let service = cx.global::<UpdateService>();
        if service.installation.is_none() {
            if manual {
                cx.dispatch_action(&butter_paper::application_shell::OpenReleasePage);
            }
            return;
        }
        match &service.status {
            Status::Idle => {}
            Status::Ready(prepared) => {
                if manual {
                    notify_ready(cx, prepared.version);
                }
                return;
            }
            Status::Checking | Status::Downloading => return,
        }
        let (feed, current, channel) = (service.feed_url.clone(), service.current, service.channel);
        cx.global_mut::<UpdateService>().status = Status::Checking;
        refresh_menus(cx);
        let task = cx.background_executor().spawn(async move {
            let target = UpdateTarget::current()
                .ok_or_else(|| UpdateError::new_public("This platform has no update packages."))?;
            let releases = fetch_releases(&feed)?;
            Ok::<_, UpdateError>(select_update(current, channel, &releases, target))
        });
        cx.spawn(async move |cx| {
            let result = task.await;
            let _ = cx.update(|cx| finish_check(cx, manual, result));
        })
        .detach();
    }

    fn finish_check(
        cx: &mut App,
        manual: bool,
        result: Result<Option<AvailableUpdate>, UpdateError>,
    ) {
        let service = cx.global_mut::<UpdateService>();
        if result.is_ok() {
            let now = format_canonical_utc_timestamp(std::time::SystemTime::now());
            if service.settings.record_successful_check(&now).is_ok() {
                let _ = service.store.save(&service.settings);
            }
        }
        match result {
            Ok(Some(update)) => {
                service.status = Status::Downloading;
                download(cx, update, manual);
            }
            Ok(None) => {
                service.status = Status::Idle;
                if manual {
                    let current = cx.global::<UpdateService>().current;
                    notify(
                        cx,
                        Notification::info(format!("Butter Paper {current} is the latest version."))
                            .title("You’re up to date"),
                    );
                }
            }
            Err(error) => {
                service.status = Status::Idle;
                if manual {
                    notify(
                        cx,
                        Notification::error(error.to_string()).title("Couldn’t check for updates"),
                    );
                }
            }
        }
        refresh_menus(cx);
    }

    fn download(cx: &mut App, update: AvailableUpdate, manual: bool) {
        let installation = cx
            .global::<UpdateService>()
            .installation
            .clone()
            .expect("checks run only for installed copies");
        let work = std::env::temp_dir().join(format!(
            "butter-paper-update-{}-{}",
            update.version,
            std::process::id()
        ));
        let task = cx.background_executor().spawn(async move {
            let _ = std::fs::remove_dir_all(&work);
            std::fs::create_dir_all(&work)
                .map_err(|_| UpdateError::new_public("The update could not be saved."))?;
            let result = download_update(&update, &work)
                .and_then(|package| prepare_update(&update, &package, &work, &installation));
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&work);
            }
            result
        });
        cx.spawn(async move |cx| {
            let result = task.await;
            let _ = cx.update(|cx| {
                let service = cx.global_mut::<UpdateService>();
                match result {
                    Ok(prepared) => {
                        let version = prepared.version;
                        service.status = Status::Ready(prepared);
                        notify_ready(cx, version);
                    }
                    Err(error) => {
                        service.status = Status::Idle;
                        if manual {
                            notify(
                                cx,
                                Notification::error(error.to_string())
                                    .title("Couldn’t download the update"),
                            );
                        }
                    }
                }
                refresh_menus(cx);
            });
        })
        .detach();
    }

    fn notify_ready(cx: &mut App, version: ReleaseVersion) {
        notify(
            cx,
            Notification::success(
                "Restart now to finish updating, or it installs when you quit.",
            )
            .title(format!("Butter Paper {version} is ready"))
            .action(|_, _, _| {
                Button::new("restart-to-update")
                    .label("Restart")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(RestartToUpdate), cx))
            }),
        );
    }
}

#[cfg(feature = "review-driver")]
mod review_driver {
    use super::*;
    use gpui::{
        AsyncApp, Bounds, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
        Pixels, PlatformInput, Point, point,
    };
    use std::io::Write as _;
    use std::path::PathBuf;

    static TRACE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    pub fn trace(message: &str) {
        if let Ok(mut trace) = TRACE.lock() {
            trace.push(message.to_owned());
        }
    }

    pub fn start(cx: &mut App) {
        let Ok(script) = std::env::var("BP_REVIEW_SCRIPT") else {
            return;
        };
        let out = PathBuf::from(std::env::var("BP_REVIEW_OUT").expect("BP_REVIEW_OUT"));
        std::fs::create_dir_all(&out).expect("review output directory");
        let script = std::fs::read_to_string(script).expect("review script");
        cx.spawn(async move |cx| {
            let mut log = std::fs::File::create(out.join("review.log")).unwrap();
            // Let the startup window restore and paint.
            pause(cx, 2500).await;
            for line in script.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
                writeln!(log, "> {line}").unwrap();
                if let Ok(mut trace) = TRACE.lock() {
                    for message in trace.drain(..) {
                        writeln!(log, "trace: {message}").unwrap();
                    }
                }
                cx.update(|cx| {
                    if let Some(entry) = active_document_window(cx) {
                        let _ = entry.handle.update(cx, |_, window, cx| {
                            writeln!(
                                log,
                                "focus={:?} quit_available={} move_available={}",
                                window.focused(cx).is_some(),
                                window.is_action_available(&RequestApplicationQuit, cx),
                                window.is_action_available(&MoveDocumentToNewWindow, cx),
                            )
                            .unwrap();
                        });
                    }
                });
                let words = line.split_whitespace().collect::<Vec<_>>();
                let number = |index: usize| words[index].parse::<f32>().unwrap();
                match words[0] {
                    "wait" => pause(cx, number(1) as u64).await,
                    "shot" => shot(cx, &out, words[1], &mut log),
                    "new-window" => {
                        cx.update(|cx| cx.dispatch_action(&NewWindow));
                        pause(cx, 1200).await;
                    }
                    "os-drag" => {
                        // os-drag W TAB DX DY NAME, or os-drag W TAB into W2 INDEX NAME:
                        // real operating-system pointer input through
                        // BP_REVIEW_OS_DRAG_CMD ({x1} {y1} {x2} {y2} {mid}, device
                        // pixels; {mid} is a screenshot taken mid-drag).
                        let Ok(command) = std::env::var("BP_REVIEW_OS_DRAG_CMD") else {
                            writeln!(log, "os-drag: BP_REVIEW_OS_DRAG_CMD is not set").unwrap();
                            continue;
                        };
                        let source = number(1) as usize;
                        let tab = number(2) as usize;
                        let name = words[words.len() - 1];
                        let points = cx.update(|cx| {
                            let entries = document_window_entries(cx);
                            let entry = entries.get(source)?;
                            let (tab_bounds, _) =
                                entry.workspace.upgrade()?.read(cx).session_tab_geometry(tab, cx);
                            let start = window_screen_point(entry.handle, tab_bounds?.center(), cx)?;
                            let end = if words[3] == "into" {
                                let target = entries.get(number(4) as usize)?;
                                let workspace = target.workspace.upgrade()?;
                                let (before, strip) =
                                    workspace.read(cx).session_tab_geometry(number(5) as usize, cx);
                                let local = match before {
                                    Some(bounds) => point(bounds.left() + px(6.), bounds.center().y),
                                    None => point(strip.right() - px(40.), strip.center().y),
                                };
                                window_screen_point(target.handle, local, cx)?
                            } else {
                                start + point(px(number(3)), px(number(4)))
                            };
                            let scale = entry
                                .handle
                                .update(cx, |_, window, _| window.scale_factor())
                                .ok()?;
                            Some((start.scale(scale), end.scale(scale)))
                        });
                        let Some((start, end)) = points else {
                            writeln!(log, "os-drag: window or tab not found").unwrap();
                            continue;
                        };
                        let mid = out.join(format!("{name}-mid-screen.png"));
                        let command = command
                            .replace("{x1}", &format!("{:.0}", start.x.0))
                            .replace("{y1}", &format!("{:.0}", start.y.0))
                            .replace("{x2}", &format!("{:.0}", end.x.0))
                            .replace("{y2}", &format!("{:.0}", end.y.0))
                            .replace("{mid}", &mid.display().to_string());
                        writeln!(log, "os-drag {name}: {start:?} -> {end:?}").unwrap();
                        cx.update(|cx| {
                            if let Some(entry) = document_window_entries(cx).get(source) {
                                let frame = entry.handle.update(cx, |_, window, _| {
                                    (window.bounds(), window.viewport_size())
                                });
                                if let Some(workspace) = entry.workspace.upgrade() {
                                    let workspace = workspace.read(cx);
                                    writeln!(
                                        log,
                                        "  frame={frame:?} viewport={:?} tab={:?}",
                                        workspace.session_tab_viewport(),
                                        workspace.session_tab_geometry(tab, cx).0
                                    )
                                    .unwrap();
                                }
                            }
                        });
                        let mut process = if cfg!(windows) {
                            let mut process = std::process::Command::new("powershell.exe");
                            process.args(["-NoProfile", "-Command", &command]);
                            #[cfg(windows)]
                            {
                                use std::os::windows::process::CommandExt as _;
                                process.creation_flags(0x0800_0000);
                            }
                            process
                        } else {
                            let mut process = std::process::Command::new("sh");
                            process.args(["-c", &command]);
                            process
                        };
                        // Run it alongside: the app must keep handling the real events.
                        match process.spawn() {
                            Ok(mut child) => {
                                for _ in 0..200 {
                                    pause(cx, 100).await;
                                    if child.try_wait().ok().flatten().is_some() {
                                        break;
                                    }
                                }
                                writeln!(log, "os-drag {name} finished: {:?}", child.try_wait()).unwrap();
                            }
                            Err(error) => writeln!(log, "os-drag {name}: {error}").unwrap(),
                        }
                        pause(cx, 1200).await;
                    }
                    "float" => {
                        // float: raise every window above others without
                        // activating the app, so frames draw while the user
                        // keeps working (review benchmarks on macOS only).
                        #[cfg(target_os = "macos")]
                        cx.update(|_| unsafe { float_all_windows() });
                        pause(cx, 300).await;
                    }
                    "drag-bench" => {
                        // drag-bench W X1 Y1 X2 Y2 MOVES: a left drag at display
                        // cadence, logging event-handling time and frame intervals.
                        let window = number(1) as usize;
                        let start = point(px(number(2)), px(number(3)));
                        let end = point(px(number(4)), px(number(5)));
                        let moves = number(6) as usize;
                        // Optional event spacing in ms (default 8, ~a 125 Hz mouse).
                        let spacing = words.get(7).and_then(|word| word.parse::<u64>().ok()).unwrap_or(8);
                        let frames = std::rc::Rc::new(std::cell::RefCell::new(Vec::<std::time::Instant>::new()));
                        let running = std::rc::Rc::new(std::cell::Cell::new(true));
                        fn record(
                            window: &mut gpui::Window,
                            frames: std::rc::Rc<std::cell::RefCell<Vec<std::time::Instant>>>,
                            running: std::rc::Rc<std::cell::Cell<bool>>,
                        ) {
                            window.on_next_frame(move |window, _| {
                                frames.borrow_mut().push(std::time::Instant::now());
                                if running.get() {
                                    record(window, frames, running);
                                }
                            });
                        }
                        dispatch(
                            cx,
                            window,
                            PlatformInput::MouseDown(MouseDownEvent {
                                button: MouseButton::Left,
                                position: start,
                                modifiers: Modifiers::default(),
                                click_count: 1,
                                first_mouse: false,
                            }),
                        );
                        pause(cx, 100).await;
                        cx.update(|cx| {
                            if let Some(entry) = document_window_entries(cx).get(window).cloned() {
                                let frames = frames.clone();
                                let running = running.clone();
                                if std::env::var_os("BP_BENCH_FRAME_CALLBACKS").is_some() {
                                    let _ = entry.handle.update(cx, |_, window, _| record(window, frames, running));
                                }
                            }
                        });
                        let mut handling = Vec::new();
                        for step in 1..=moves {
                            let t = (step % 240) as f32 / 240.;
                            let position = start + (end - start) * t;
                            let began = std::time::Instant::now();
                            dispatch(
                                cx,
                                window,
                                PlatformInput::MouseMove(MouseMoveEvent {
                                    position,
                                    pressed_button: Some(MouseButton::Left),
                                    modifiers: Modifiers::default(),
                                }),
                            );
                            handling.push(began.elapsed().as_secs_f64() * 1000.);
                            pause(cx, spacing).await;
                        }
                        running.set(false);
                        dispatch(
                            cx,
                            window,
                            PlatformInput::MouseUp(MouseUpEvent {
                                button: MouseButton::Left,
                                position: end,
                                modifiers: Modifiers::default(),
                                click_count: 1,
                            }),
                        );
                        let stamps = frames.borrow().clone();
                        let mut intervals = stamps
                            .windows(2)
                            .map(|pair| (pair[1] - pair[0]).as_secs_f64() * 1000.)
                            .collect::<Vec<_>>();
                        let summary = |values: &mut Vec<f64>| {
                            if values.is_empty() {
                                return "none".to_owned();
                            }
                            values.sort_by(f64::total_cmp);
                            let mean = values.iter().sum::<f64>() / values.len() as f64;
                            format!(
                                "n {} mean {mean:.2} p50 {:.2} p95 {:.2} max {:.2}",
                                values.len(),
                                values[values.len() / 2],
                                values[values.len() * 95 / 100],
                                values[values.len() - 1]
                            )
                        };
                        let span = stamps
                            .last()
                            .zip(stamps.first())
                            .map(|(last, first)| (*last - *first).as_secs_f64())
                            .unwrap_or(0.);
                        writeln!(
                            log,
                            "drag-bench fps {:.1}; frame interval ms {}; move handling ms {}",
                            stamps.len().saturating_sub(1) as f64 / span.max(0.001),
                            summary(&mut intervals),
                            summary(&mut handling)
                        )
                        .unwrap();
                        pause(cx, 400).await;
                    }
                    "click" => {
                        // click W X Y: a left click at window coordinates.
                        let window = number(1) as usize;
                        let position = point(px(number(2)), px(number(3)));
                        dispatch(
                            cx,
                            window,
                            PlatformInput::MouseDown(MouseDownEvent {
                                button: MouseButton::Left,
                                position,
                                modifiers: Modifiers::default(),
                                click_count: 1,
                                first_mouse: false,
                            }),
                        );
                        pause(cx, 50).await;
                        dispatch(
                            cx,
                            window,
                            PlatformInput::MouseUp(MouseUpEvent {
                                button: MouseButton::Left,
                                position,
                                modifiers: Modifiers::default(),
                                click_count: 1,
                            }),
                        );
                        pause(cx, 400).await;
                    }
                    "key" => {
                        // key W KEYSTROKE: a key press in window W, through the
                        // same dispatch path as a real shortcut (e.g. ctrl-shift-n).
                        let window = number(1) as usize;
                        match gpui::Keystroke::parse(words[2]) {
                            Ok(keystroke) => dispatch(
                                cx,
                                window,
                                PlatformInput::KeyDown(gpui::KeyDownEvent {
                                    keystroke,
                                    is_held: false,
                                    prefer_character_input: false,
                                }),
                            ),
                            Err(error) => writeln!(log, "bad keystroke: {error}").unwrap(),
                        }
                        if words[2].ends_with("-q") {
                            // A quit shortcut: trace until the process exits.
                            for tick in 0..40 {
                                pause(cx, 250).await;
                                cx.update(|cx| {
                                    writeln!(
                                        log,
                                        "after quit key {tick}: document windows={}",
                                        document_window_entries(cx).len()
                                    )
                                    .unwrap();
                                });
                            }
                        }
                        pause(cx, 800).await;
                    }
                    "place" => {
                        // place W X Y WIDTH HEIGHT: frame origin and content size.
                        let window = number(1) as usize;
                        let origin = point(px(number(2)), px(number(3)));
                        let size = gpui::size(px(number(4)), px(number(5)));
                        cx.update(|cx| {
                            if let Some(entry) = document_window_entries(cx).get(window).cloned() {
                                let _ = entry.handle.update(cx, |_, window, _| {
                                    window.resize(size);
                                    window.set_origin(origin);
                                });
                            }
                        });
                        pause(cx, 800).await;
                    }
                    "move-to-new-window" => {
                        cx.update(|cx| cx.dispatch_action(&MoveDocumentToNewWindow));
                        pause(cx, 1200).await;
                    }
                    "close" => {
                        let window = number(1) as usize;
                        cx.update(|cx| {
                            if let Some(entry) = document_window_entries(cx).get(window).cloned() {
                                request_window_close(&entry, cx);
                            }
                        });
                        pause(cx, 800).await;
                    }
                    "quit" => {
                        writeln!(log, "quitting").unwrap();
                        cx.update(|cx| cx.dispatch_action(&RequestApplicationQuit));
                        // Trace the close transactions until the process exits.
                        for tick in 0..40 {
                            pause(cx, 250).await;
                            cx.update(|cx| {
                                let entries = document_window_entries(cx);
                                let states = entries
                                    .iter()
                                    .filter_map(|entry| entry.close.upgrade())
                                    .map(|close| {
                                        let close = close.read(cx);
                                        format!(
                                            "dialog={:?} quit_intent={} effects={:?}",
                                            close.dialog().is_some(),
                                            close.has_quit_intent(),
                                            close.effects()
                                        )
                                    })
                                    .collect::<Vec<_>>();
                                writeln!(
                                    log,
                                    "after quit {tick}: platform windows={} document windows={} {states:?}",
                                    cx.windows().len(),
                                    entries.len()
                                )
                                .unwrap();
                            });
                        }
                        return;
                    }
                    "drag-tab" => {
                        let source = number(1) as usize;
                        let tab = number(2) as usize;
                        let name = words[words.len() - 1];
                        let Some((start, end)) = cx.update(|cx| {
                            let entries = document_window_entries(cx);
                            let entry = entries.get(source)?;
                            let (tab_bounds, _) = entry
                                .workspace
                                .upgrade()?
                                .read(cx)
                                .session_tab_geometry(tab, cx);
                            let start = tab_bounds?.center();
                            let end = match words[3] {
                                "off" => start + point(px(number(4)), px(number(5))),
                                _ => {
                                    let target = entries.get(number(4) as usize)?;
                                    let index = number(5) as usize;
                                    let workspace = target.workspace.upgrade()?;
                                    let (before, strip) =
                                        workspace.read(cx).session_tab_geometry(index, cx);
                                    let local = match before {
                                        Some(bounds) => point(bounds.left() + px(6.), bounds.center().y),
                                        None => point(strip.right() - px(40.), strip.center().y),
                                    };
                                    let target_frame = frame(target.handle, cx)?;
                                    let source_frame = frame(entry.handle, cx)?;
                                    target_frame.origin + local - source_frame.origin
                                }
                            };
                            Some((start, end))
                        }) else {
                            writeln!(log, "drag-tab: window or tab not found").unwrap();
                            continue;
                        };
                        drag(cx, source, start, end, &out, name, &mut log).await;
                    }
                    other => writeln!(log, "unknown command {other}").unwrap(),
                }
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    unsafe fn float_all_windows() {
        use std::ffi::{c_void, CString};
        #[link(name = "objc")]
        unsafe extern "C" {
            fn objc_getClass(name: *const std::ffi::c_char) -> *mut c_void;
            fn sel_registerName(name: *const std::ffi::c_char) -> *mut c_void;
            fn objc_msgSend();
        }
        let sel = |name: &str| unsafe { sel_registerName(CString::new(name).unwrap().as_ptr()) };
        let send0: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        let send_count: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        let send_index: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        let send_level: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        unsafe {
            let app = send0(objc_getClass(CString::new("NSApplication").unwrap().as_ptr()), sel("sharedApplication"));
            let windows = send0(app, sel("windows"));
            for index in 0..send_count(windows, sel("count")) {
                let window = send_index(windows, sel("objectAtIndex:"), index);
                // canJoinAllSpaces | fullScreenAuxiliary: visible over a full-screen Space.
                send_level(window, sel("setCollectionBehavior:"), 1 | 256);
                send_level(window, sel("setLevel:"), 3);
                send0(window, sel("orderFrontRegardless"));
            }
        }
    }

    fn frame(handle: AnyWindowHandle, cx: &mut App) -> Option<Bounds<Pixels>> {
        handle.update(cx, |_, window, _| window.bounds()).ok()
    }

    async fn pause(cx: &mut AsyncApp, ms: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(ms))
            .await;
    }

    fn dispatch(cx: &mut AsyncApp, window: usize, event: PlatformInput) {
        cx.update(|cx| {
            if let Some(entry) = document_window_entries(cx).get(window).cloned() {
                let _ = entry
                    .handle
                    .update(cx, |_, window, cx| window.dispatch_event(event, cx));
            }
        });
    }

    async fn drag(
        cx: &mut AsyncApp,
        window: usize,
        start: Point<Pixels>,
        end: Point<Pixels>,
        out: &std::path::Path,
        name: &str,
        log: &mut std::fs::File,
    ) {
        writeln!(log, "drag {name}: {start:?} -> {end:?}").unwrap();
        dispatch(
            cx,
            window,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
        );
        pause(cx, 60).await;
        cx.update(|cx| {
            if let Some(workspace) = document_window_entries(cx)
                .get(window)
                .and_then(|entry| entry.workspace.upgrade())
            {
                let workspace = workspace.read(cx);
                writeln!(
                    log,
                    "after mouse-down: drag={:?} tabs={:?}",
                    workspace.session_tab_drag_state(),
                    workspace.session_tab_debug_geometry(cx)
                )
                .unwrap();
            }
        });
        let steps = 16;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let position = start + (end - start) * t;
            dispatch(
                cx,
                window,
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }),
            );
            pause(cx, 70).await;
            cx.update(|cx| {
                let state = document_window_entries(cx)
                    .get(window)
                    .and_then(|entry| entry.workspace.upgrade())
                    .map(|workspace| workspace.read(cx).session_tab_drag_state());
                writeln!(log, "{name} step {step} at {position:?}: drag={state:?}").unwrap();
            });
            shot(cx, out, &format!("{name}-{step:02}"), log);
        }
        dispatch(
            cx,
            window,
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers: Modifiers::default(),
                click_count: 1,
            }),
        );
        for step in 0..8 {
            pause(cx, 150).await;
            shot(cx, out, &format!("{name}-{:02}", steps + 1 + step), log);
        }
    }

    fn shot(cx: &mut AsyncApp, out: &std::path::Path, name: &str, log: &mut std::fs::File) {
        // Platforms without in-process frame capture (Windows, Linux) can name
        // a command that captures this machine's display, with `{path}` for
        // the output file. Review VMs only.
        if let Ok(command) = std::env::var("BP_REVIEW_CAPTURE_CMD") {
            let path = out.join(format!("{name}-screen.png"));
            let command = command.replace("{path}", &path.display().to_string());
            let status = if cfg!(windows) {
                let mut process = std::process::Command::new("powershell.exe");
                process.args(["-NoProfile", "-Command", &command]);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt as _;
                    process.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                }
                process.status()
            } else {
                std::process::Command::new("sh").args(["-c", &command]).status()
            };
            writeln!(log, "{name} screen capture: {status:?}").unwrap();
        }
        cx.update(|cx| {
            if let Some(handle) = cx.try_global::<TabDragPreview>().map(|preview| preview.handle) {
                let path = out.join(format!("{name}-chip.png"));
                if let Ok((bounds, image)) = handle.update(cx, |_, window, _| {
                    (window.bounds(), window.render_to_image())
                }) {
                    let saved = image.map(|image| image.save(&path).is_ok());
                    writeln!(log, "{name} chip frame={bounds:?} saved={saved:?}").unwrap();
                }
            }
            let active = active_document_window(cx).map(|entry| entry.handle);
            for (index, entry) in document_window_entries(cx).into_iter().enumerate() {
                let titles = entry
                    .workspace
                    .upgrade()
                    .map(|workspace| {
                        let workspace = workspace.read(cx);
                        (workspace.session_debug_states(cx), workspace.active_document_id())
                    })
                    .unwrap_or_default();
                let (tool, workspace_focus) = entry
                    .workspace
                    .upgrade()
                    .map(|workspace| {
                        let workspace = workspace.read(cx);
                        (
                            workspace
                                .active_document_id()
                                .and_then(|id| workspace.annotation_tool(id, cx)),
                            Some(workspace.focus_handle()),
                        )
                    })
                    .unwrap_or_default();
                let mut focus = String::new();
                let result = entry.handle.update(cx, |_, window, cx| {
                    focus = format!(
                        "focused={} in_workspace={} context={:?}",
                        window.focused(cx).is_some(),
                        workspace_focus
                            .as_ref()
                            .is_some_and(|handle| handle.contains_focused(window, cx)),
                        window.context_stack().iter().map(|context| format!("{context:?}")).collect::<Vec<_>>()
                    );
                    let image = window.render_to_image();
                    (window.bounds(), image)
                });
                let Ok((bounds, image)) = result else {
                    continue;
                };
                let path = out.join(format!("{name}-w{index}.png"));
                let saved = image.map_err(|error| error.to_string()).and_then(|image| {
                    image.save(&path).map_err(|error| error.to_string())
                });
                writeln!(
                    log,
                    "{name} w{index} frame={:?} active_window={} tabs={:?} active_tab={:?} tool={:?} focus={:?} saved={:?}",
                    bounds,
                    Some(entry.handle) == active,
                    titles.0,
                    titles.1,
                    tool,
                    focus,
                    saved.map(|_| path.display().to_string())
                )
                .unwrap();
            }
        });
    }
}
