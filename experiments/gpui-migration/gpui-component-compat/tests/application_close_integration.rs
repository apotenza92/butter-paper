use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use butter_paper_gpui_component_compat::{
    application_close::{
        ApplicationCloseAction, ApplicationCloseCompletionKind, ApplicationCloseTransitionStatus,
    },
    application_close_workspace::{
        APPLICATION_CLOSE_RECOVERY_ALERT_ID, APPLICATION_CLOSE_RECOVERY_CANCEL_ID,
        APPLICATION_CLOSE_RECOVERY_ID, APPLICATION_CLOSE_RECOVERY_PRIMARY_ID,
        ApplicationCloseCheckpointPublication, ApplicationCloseCheckpointPublisher,
        ApplicationCloseEffect, ApplicationCloseRecoveryKind, ApplicationCloseShell,
        ApplicationCloseWorkspace, PendingApplicationSaveAs, RequestApplicationClose,
        register_application_close_action,
    },
    document_workspace::{
        ApplyDisposition, DOCUMENT_ANNOTATION_DELETE_ID, DOCUMENT_ANNOTATION_REDO_ID,
        DOCUMENT_ANNOTATION_UNDO_ID, DOCUMENT_ARROW_TOOL_ID, DOCUMENT_ELLIPSE_TOOL_ID,
        DOCUMENT_ERROR_ID, DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID, DOCUMENT_HIGHLIGHT_TOOL_ID,
        DOCUMENT_IMAGE_TOOL_ID, DOCUMENT_INK_PROPERTIES_ID, DOCUMENT_LINE_TOOL_ID,
        DOCUMENT_PEN_TOOL_ID, DOCUMENT_RECTANGLE_TOOL_ID, DOCUMENT_SAVE_AS_ID, DOCUMENT_SAVE_ID,
        DOCUMENT_SELECT_TOOL_ID, DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID,
        DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID,
        DOCUMENT_TEXT_BOX_EDITOR_ID, DOCUMENT_TEXT_BOX_PROPERTIES_ID, DOCUMENT_TEXT_BOX_TOOL_ID,
        DOCUMENT_TOOLBAR_SCROLL_ID, DocumentEditCapabilities, DocumentId,
        DocumentOpenBatchDisposition, DocumentOpenBatchRequest, DocumentOpenBatchStatus,
        DocumentOpenOrigin, DocumentWorkspace, NativeDocumentOpener, NativeDocumentResource,
        NativeDocumentSaveStatus, NativeDocumentSaver, NativeDocumentStatus, OpenDocumentRequest,
        OpenedNativeDocument, PdfDocumentSaver, PdfiumWorkerBackend, RasterSurface,
        RotatePageRight, SaveDocumentRequest, SavedNativeDocument, ThumbnailSurface,
        ViewerFitPreset, ZoomIn,
        document_annotation_layer_id, document_session_close_id, document_session_tab_id,
        document_thumbnail_id, init_document_workspace_actions,
    },
    ink_property_inspector::{
        INK_INSPECTOR_APPLY_COLOR_ID, INK_INSPECTOR_COLOR_ID, INK_INSPECTOR_COLOR_TRIGGER_ID,
        INK_INSPECTOR_LOCKED_ID, INK_INSPECTOR_OPACITY_ID, INK_INSPECTOR_OPACITY_TRACK_ID,
        INK_INSPECTOR_WIDTH_ID,
    },
    native_document_view_state::{RestartView, RestartZoom},
    native_launch::{NativeLaunchAction, NativeLaunchConfig, NativeLaunchSessionSource},
    page_view_control::PageViewMode,
    session_manifest::{SessionManifestStore, SessionSnapshot},
    straight_line_property_inspector::{
        STRAIGHT_LINE_INSPECTOR_APPLY_COLOR_ID, STRAIGHT_LINE_INSPECTOR_COLOR_TRIGGER_ID,
        STRAIGHT_LINE_INSPECTOR_OPACITY_TRACK_ID, STRAIGHT_LINE_INSPECTOR_WIDTH_ID,
    },
    text_box_property_inspector::{
        TEXT_BOX_INSPECTOR_ALIGNMENT_CENTER_ID, TEXT_BOX_INSPECTOR_APPLY_COLOR_ID,
        TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID, TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID,
        TEXT_BOX_INSPECTOR_SIZE_ID,
    },
};
use butter_paper_gpui_gallery::{
    annotation_adapter::{AnnotationTool, ellipse_resize_handle_point_for_rect},
    annotation_model::{
        Annotation, BlendMode, InkTool, LineKind, MarkupId, PageRotation, PdfPoint, PdfRect,
        PenAppearance, RectangleResizeHandle, StraightLineAnnotation, StraightLineAppearance,
        TextAlignment, TextBoxAnnotation, TextBoxStyle,
    },
    generated_document::{GeneratedDocumentRequest, GeneratedDocumentStore},
    pdf_engine::PdfPersistenceSession,
    pdf_file_authority::SaveAsTargetAuthority,
    viewer::TileRequest,
};
use gpui::{
    AnyView, AppContext as _, ClipboardItem, EntityInputHandler as _, Focusable as _, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    point, px,
};
use gpui_component::{Root, WindowExt as _};
use sha2::{Digest as _, Sha256};

#[cfg(target_os = "macos")]
const EDIT_SELECT_ALL: &str = "cmd-a";
#[cfg(not(target_os = "macos"))]
const EDIT_SELECT_ALL: &str = "ctrl-a";
#[cfg(target_os = "macos")]
const EDIT_COPY: &str = "cmd-c";
#[cfg(not(target_os = "macos"))]
const EDIT_COPY: &str = "ctrl-c";
#[cfg(target_os = "macos")]
const EDIT_PASTE: &str = "cmd-v";
#[cfg(not(target_os = "macos"))]
const EDIT_PASTE: &str = "ctrl-v";
#[cfg(target_os = "macos")]
const EDIT_CUT: &str = "cmd-x";
#[cfg(not(target_os = "macos"))]
const EDIT_CUT: &str = "ctrl-x";
#[cfg(target_os = "macos")]
const EDIT_UNDO: &str = "cmd-z";
#[cfg(not(target_os = "macos"))]
const EDIT_UNDO: &str = "ctrl-z";
#[cfg(target_os = "macos")]
const EDIT_REDO: &str = "cmd-shift-z";
#[cfg(not(target_os = "macos"))]
const EDIT_REDO: &str = "ctrl-y";

struct SessionManifestScratch(PathBuf);

impl Drop for SessionManifestScratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

struct NativeShellPidScratchGuard {
    root: PathBuf,
    worker_executable: PathBuf,
    worker_pids: Vec<u32>,
}

impl NativeShellPidScratchGuard {
    fn new(root: PathBuf, worker_executable: PathBuf) -> Self {
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        Self {
            root,
            worker_executable,
            worker_pids: Vec::new(),
        }
    }

    fn track_worker(&mut self, pid: u32) {
        if !self.worker_pids.contains(&pid) {
            self.worker_pids.push(pid);
        }
    }
}

impl Drop for NativeShellPidScratchGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        for pid in self.worker_pids.iter().rev() {
            let proc_root = PathBuf::from(format!("/proc/{pid}"));
            let executable = std::fs::read_link(proc_root.join("exe"));
            if !matches!(executable, Ok(ref path) if path == &self.worker_executable) {
                continue;
            }
            unsafe {
                libc::kill(*pid as i32, libc::SIGTERM);
            }
            for _ in 0..50 {
                if !proc_root.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if proc_root.exists() {
                unsafe {
                    libc::kill(*pid as i32, libc::SIGKILL);
                }
            }
        }
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn application_close_manifest_store(label: &str) -> (SessionManifestScratch, SessionManifestStore) {
    let root = owned_target(format!(
        "application-close-manifest-{label}-{}",
        std::process::id()
    ));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir(&root).unwrap();
    let store = SessionManifestStore::open(root.clone()).unwrap();
    (SessionManifestScratch(root), store)
}

struct DurabilityWarningCheckpointPublisher {
    store: SessionManifestStore,
    attempts: AtomicU32,
}

impl ApplicationCloseCheckpointPublisher for DurabilityWarningCheckpointPublisher {
    fn publish(
        &self,
        snapshot: &SessionSnapshot,
    ) -> Result<ApplicationCloseCheckpointPublication, String> {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        self.store
            .replace(snapshot)
            .map_err(|error| format!("{error:?}"))?;
        let document_count = self.store.load().unwrap().into_parts().0.len();
        Ok(
            ApplicationCloseCheckpointPublication::PublishedWithDurabilityWarning {
                document_count: Some(document_count),
                message: "injected directory sync warning".into(),
            },
        )
    }
}

fn owned_target(name: impl AsRef<Path>) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".prepared")
        .join("owned-targets");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

fn scroll_annotation_target_into_view(cx: &mut gpui::VisualTestContext, target_id: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let scroll = cx
        .debug_bounds(DOCUMENT_TOOLBAR_SCROLL_ID)
        .expect("the annotation toolbar must expose its horizontal scroll owner");
    let target = cx
        .debug_bounds(target_id)
        .unwrap_or_else(|| panic!("{target_id} must render before scrolling"));
    let delta_x = if target.right() > scroll.right() {
        -(f32::from(target.right() - scroll.right()) + 8.)
    } else if target.left() < scroll.left() {
        f32::from(scroll.left() - target.left()) + 8.
    } else {
        0.
    };
    if delta_x != 0. {
        cx.simulate_event(ScrollWheelEvent {
            position: scroll.center(),
            delta: ScrollDelta::Pixels(point(px(delta_x), px(0.))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

fn edit_selected_straight_line_through_real_controls(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
) {
    let inspector = workspace
        .read_with(cx, |workspace, _| workspace.straight_line_property_inspector())
        .expect("the selected Line must retain its property inspector");
    let picker = inspector.read_with(cx, |inspector, _| inspector.color_picker());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let color_trigger = cx
        .debug_bounds(STRAIGHT_LINE_INSPECTOR_COLOR_TRIGGER_ID)
        .expect("the selected Line must expose its real ColorPicker");
    cx.simulate_click(color_trigger.center(), Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let color_input = picker.read_with(cx, |picker, _| picker.hex_input().clone());
    cx.update(|window, cx| color_input.read(cx).focus_handle(cx).focus(window, cx));
    cx.write_to_clipboard(ClipboardItem::new_string("#2563eb".into()));
    cx.simulate_keystrokes(&format!("{EDIT_SELECT_ALL} {EDIT_PASTE} enter"));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let apply_color = cx
        .debug_bounds(STRAIGHT_LINE_INSPECTOR_APPLY_COLOR_ID)
        .expect("the real ColorPicker must expose Apply color");
    cx.simulate_click(apply_color.center(), Modifiers::default());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let width = cx
        .debug_bounds(STRAIGHT_LINE_INSPECTOR_WIDTH_ID)
        .expect("the selected Line must expose its real NumberInput");
    cx.simulate_click(width.center(), Modifiers::default());
    let width_input = inspector.read_with(cx, |inspector, _| inspector.width_input());
    assert!(cx.update(|window, cx| width_input.read(cx).focus_handle(cx).is_focused(window)));
    cx.simulate_keystrokes(&format!("{EDIT_SELECT_ALL} 4 enter"));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let opacity = cx
        .debug_bounds(STRAIGHT_LINE_INSPECTOR_OPACITY_TRACK_ID)
        .expect("the selected Line must expose its real opacity slider");
    let target = point(
        opacity.origin.x + opacity.size.width * 0.5,
        opacity.center().y,
    );
    let before = workspace
        .read_with(cx, |workspace, cx| workspace.annotation_snapshot(document_id, cx))
        .unwrap();
    cx.simulate_mouse_down(target, MouseButton::Left, Modifiers::default());
    let preview = workspace
        .read_with(cx, |workspace, cx| workspace.annotation_snapshot(document_id, cx))
        .unwrap();
    assert_eq!(
        (preview.revision, preview.undo_depth),
        (before.revision, before.undo_depth),
        "opacity Change must remain preview-only until Release"
    );
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
}

fn edit_selected_ink_through_real_controls(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    color: &str,
    width_keystrokes: &str,
    opacity: f32,
) {
    let inspector = workspace
        .read_with(cx, |workspace, _| workspace.ink_property_inspector())
        .expect("the open Ink inspector must retain its component state");
    let picker = inspector.read_with(cx, |inspector, _| inspector.color_picker());
    let before = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    cx.update(|window, cx| picker.read(cx).focus_handle(cx).focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let trigger = cx.debug_bounds(INK_INSPECTOR_COLOR_TRIGGER_ID).unwrap();
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(picker.read_with(cx, |picker, _| picker.is_open()));
    let hex_input = picker.read_with(cx, |picker, _| picker.hex_input().clone());
    cx.update(|window, cx| hex_input.read(cx).focus_handle(cx).focus(window, cx));
    cx.write_to_clipboard(ClipboardItem::new_string(format!("{color}bf")));
    cx.simulate_keystrokes(&format!("{EDIT_SELECT_ALL} {EDIT_PASTE} enter"));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!picker.read_with(cx, |picker, _| picker.is_open()));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        before.revision,
        "ColorPicker Change must only preview"
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let apply = cx.debug_bounds(INK_INSPECTOR_APPLY_COLOR_ID).unwrap();
    cx.simulate_click(apply.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        before.revision + 1
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let width = cx.debug_bounds(INK_INSPECTOR_WIDTH_ID).unwrap();
    cx.simulate_click(width.center(), Modifiers::default());
    cx.simulate_keystrokes(&format!("{EDIT_SELECT_ALL} {width_keystrokes} enter"));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        before.revision + 2
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let track = cx.debug_bounds(INK_INSPECTOR_OPACITY_TRACK_ID).unwrap();
    let target = point(
        track.origin.x + track.size.width * opacity,
        track.center().y,
    );
    cx.simulate_mouse_down(target, MouseButton::Left, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        before.revision + 2,
        "Slider Change must remain preview-only"
    );
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        before.revision + 3
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let lock = cx.debug_bounds(INK_INSPECTOR_LOCKED_ID).unwrap();
    cx.simulate_click(lock.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .revision),
        before.revision + 4
    );
}

fn wait_for_real_application_close_terminal(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    close: &gpui::Entity<ApplicationCloseWorkspace>,
    document_id: DocumentId,
    target_path: &Path,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let sessions_empty =
            workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty());
        let saved_all_quit = close.read_with(cx, |close, _| {
            close.effects().iter().any(|effect| {
                matches!(
                    effect,
                    ApplicationCloseEffect::QuitRequested {
                        kind: ApplicationCloseCompletionKind::SavedAll,
                        ..
                    }
                )
            })
        });
        if sessions_empty && saved_all_quit {
            return;
        }

        let recovery = close.read_with(cx, |close, _| close.recovery().cloned());
        let save_status = workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .map(|session| session.read(cx).save_status().clone())
        });
        if recovery.is_some()
            || matches!(save_status, Some(NativeDocumentSaveStatus::Failed(_)))
            || Instant::now() >= deadline
        {
            let (dialog_busy, effects) = close.read_with(cx, |close, _| {
                (
                    close.dialog().map(|dialog| dialog.busy),
                    close.effects().to_vec(),
                )
            });
            let dirty_revision = workspace.read_with(cx, |workspace, cx| {
                workspace.document_dirty_revision(document_id, cx)
            });
            let target_metadata = std::fs::metadata(target_path)
                .map(|metadata| format!("{} bytes", metadata.len()))
                .unwrap_or_else(|error| format!("unavailable: {error}"));
            panic!(
                "real application close did not reach SavedAll: dialog_busy={dialog_busy:?}, recovery={recovery:?}, effects={effects:?}, save_status={save_status:?}, dirty_revision={dirty_revision:?}, target={}, target_metadata={target_metadata}",
                target_path.display(),
            );
        }
        std::thread::yield_now();
    }
}

fn wait_for_real_document_save_terminal(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    expected_path: &Path,
    operation: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let session_state = workspace.read_with(cx, |workspace, cx| {
            workspace.session(document_id, cx).map(|session| {
                let session = session.read(cx);
                (session.path().to_owned(), session.save_status().clone())
            })
        });
        match &session_state {
            Some((path, NativeDocumentSaveStatus::Idle)) if path == expected_path => return,
            Some((_, NativeDocumentSaveStatus::Failed(_))) => {
                let diagnostic = compact_real_document_save_diagnostic(
                    cx,
                    workspace,
                    document_id,
                    expected_path,
                );
                panic!(
                    "{operation} failed before reaching the expected terminal state: {diagnostic}",
                );
            }
            None => panic!(
                "{operation} lost document {document_id:?} before reaching the expected terminal state"
            ),
            _ => {}
        }
        if Instant::now() >= deadline {
            let diagnostic = compact_real_document_save_diagnostic(
                cx,
                workspace,
                document_id,
                expected_path,
            );
            panic!(
                "{operation} timed out after 10s: {diagnostic}",
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn compact_real_document_save_diagnostic(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    expected_path: &Path,
) -> String {
    let session = workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_id, cx).map(|session| {
            let session = session.read(cx);
            (
                session.path().to_owned(),
                session.save_status().clone(),
                session.worker_pid(),
            )
        })
    });
    let annotation = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(document_id, cx)
    });
    let target_metadata = std::fs::metadata(expected_path).ok();
    let target_exists = target_metadata.is_some();
    let target_bytes = target_metadata.as_ref().map(std::fs::Metadata::len);

    let (path, status, worker_pid) = match session {
        Some((path, NativeDocumentSaveStatus::Idle, worker_pid)) => {
            (path, "Idle".to_owned(), worker_pid)
        }
        Some((path, NativeDocumentSaveStatus::Saving, worker_pid)) => {
            (path, "Saving".to_owned(), worker_pid)
        }
        Some((path, NativeDocumentSaveStatus::Failed(failure), worker_pid)) => (
            path,
            format!(
                "Failed(operation={:?}, message={:?}, generation={})",
                failure.operation, failure.message, failure.generation
            ),
            worker_pid,
        ),
        None => {
            return format!(
                "session=missing, target={}, target_exists={target_exists}, target_bytes={target_bytes:?}",
                expected_path.display(),
            );
        }
    };

    let Some(annotation) = annotation else {
        return format!(
            "path={}, status={status}, annotation=missing, target={}, target_exists={target_exists}, target_bytes={target_bytes:?}, worker_pid={worker_pid:?}",
            path.display(),
            expected_path.display(),
        );
    };

    let rectangle_ids = annotation
        .rectangles
        .iter()
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let ellipse_ids = annotation
        .ellipses
        .iter()
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let line_ids = annotation
        .straight_lines
        .iter()
        .filter(|annotation| annotation.kind == LineKind::Line)
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let arrow_ids = annotation
        .straight_lines
        .iter()
        .filter(|annotation| annotation.kind == LineKind::Arrow)
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let pen_ids = annotation
        .pens
        .iter()
        .filter(|annotation| annotation.tool() == InkTool::Pen)
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let highlight_ids = annotation
        .pens
        .iter()
        .filter(|annotation| annotation.tool() == InkTool::Highlight)
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let text_box_ids = annotation
        .text_boxes
        .iter()
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let image_ids = annotation
        .images
        .iter()
        .map(|annotation| annotation.id.as_str())
        .collect::<Vec<_>>();
    let annotation_order = annotation
        .annotation_order
        .iter()
        .map(MarkupId::as_str)
        .collect::<Vec<_>>();

    format!(
        "path={}, status={status}, revision={}, saved_revision={}, dirty={}, families=[rectangle:{}:{rectangle_ids:?}, ellipse:{}:{ellipse_ids:?}, line:{}:{line_ids:?}, arrow:{}:{arrow_ids:?}, pen:{}:{pen_ids:?}, highlight:{}:{highlight_ids:?}, text_box:{}:{text_box_ids:?}, image:{}:{image_ids:?}], annotation_order={annotation_order:?}, target={}, target_exists={target_exists}, target_bytes={target_bytes:?}, worker_pid={worker_pid:?}",
        path.display(),
        annotation.revision,
        annotation.saved_revision,
        annotation.dirty,
        rectangle_ids.len(),
        ellipse_ids.len(),
        line_ids.len(),
        arrow_ids.len(),
        pen_ids.len(),
        highlight_ids.len(),
        text_box_ids.len(),
        image_ids.len(),
        expected_path.display(),
    )
}

fn wait_for_real_document_ready(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    expected_path: &Path,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let session_state = workspace.read_with(cx, |workspace, cx| {
            workspace.session(document_id, cx).map(|session| {
                let session = session.read(cx);
                (session.path().to_owned(), session.status().clone())
            })
        });
        match &session_state {
            Some((path, NativeDocumentStatus::Ready)) if path == expected_path => return,
            Some((_, NativeDocumentStatus::Failed(message))) => panic!(
                "fresh workspace open failed before Ready: message={message}, session_state={session_state:?}, expected_path={}",
                expected_path.display(),
            ),
            None => panic!("fresh workspace lost document {document_id:?} before Ready"),
            _ => {}
        }
        if Instant::now() >= deadline {
            let evidence = workspace.read_with(cx, |workspace, cx| {
                workspace.evidence_snapshot(document_id, cx)
            });
            panic!(
                "fresh workspace open timed out after 10s: session_state={session_state:?}, evidence={evidence:?}, expected_path={}",
                expected_path.display(),
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Default)]
struct DeterministicBackend {
    next_pid: AtomicU32,
    releases: Mutex<Vec<Arc<AtomicBool>>>,
    fail_saves: AtomicBool,
    save_warning: Mutex<Option<String>>,
    fail_closes: Arc<AtomicBool>,
    fail_close_pid: Arc<AtomicU32>,
    generated_roots: Mutex<Vec<PathBuf>>,
}

impl Drop for DeterministicBackend {
    fn drop(&mut self) {
        if let Ok(roots) = self.generated_roots.get_mut() {
            for root in roots.drain(..) {
                std::fs::remove_dir_all(root).ok();
            }
        }
    }
}

impl DeterministicBackend {
    fn opened(&self, title: &str) -> OpenedNativeDocument {
        self.opened_with_page_count(title, 1)
    }

    fn opened_with_page_count(&self, title: &str, page_count: u32) -> OpenedNativeDocument {
        let released = Arc::new(AtomicBool::new(false));
        self.releases.lock().unwrap().push(released.clone());
        let pid = self.next_pid.fetch_add(1, Ordering::Relaxed) + 4100;
        OpenedNativeDocument::new(
            title,
            vec![(612., 792.); page_count as usize],
            raster(32, 40),
            (0..page_count)
                .map(|page_index| ThumbnailSurface::new(page_index, raster(8, 10)))
                .collect(),
            Arc::new(DeterministicResource {
                pid,
                released,
                fail_close: self.fail_closes.clone(),
                fail_close_pid: self.fail_close_pid.clone(),
            }),
        )
        .unwrap()
    }

    fn all_released(&self) -> bool {
        self.releases
            .lock()
            .unwrap()
            .iter()
            .all(|released| released.load(Ordering::Acquire))
    }
}

impl NativeDocumentOpener for DeterministicBackend {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        Ok(self.opened(
            request
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fixture.pdf"),
        ))
    }
}

impl NativeDocumentSaver for DeterministicBackend {
    fn save(&self, request: &SaveDocumentRequest) -> Result<SavedNativeDocument, String> {
        if self.fail_saves.load(Ordering::Acquire) {
            return Err("deterministic Save As failure".into());
        }
        let saved = SavedNativeDocument::new(
            self.opened(
                request
                    .target_path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("saved.pdf"),
            ),
            request.annotation_revision,
        );
        Ok(match self.save_warning.lock().unwrap().clone() {
            Some(warning) => saved.with_publication_warning(warning),
            None => saved,
        })
    }
}

struct DeterministicResource {
    pid: u32,
    released: Arc<AtomicBool>,
    fail_close: Arc<AtomicBool>,
    fail_close_pid: Arc<AtomicU32>,
}

impl NativeDocumentResource for DeterministicResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(self.pid)
    }

    fn render_page(&self, _: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, width.max(1)))
    }

    fn render_tile(&self, request: TileRequest) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width.max(1) as u32,
            request.crop.height.max(1) as u32,
        ))
    }

    fn close(&self) -> Result<(), String> {
        if self.fail_close.load(Ordering::Acquire)
            || self.fail_close_pid.load(Ordering::Acquire) == self.pid
        {
            return Err("injected worker close failure".into());
        }
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

fn dirty_two_documents(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<DocumentWorkspace>,
    gpui::Entity<ApplicationCloseWorkspace>,
    Arc<DeterministicBackend>,
    [DocumentId; 2],
) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let first = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("fixtures/first.pdf"), cx)
    });
    let second = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("fixtures/second.pdf"), cx)
    });
    cx.run_until_parked();
    for (document_id, suffix) in [(first, "first"), (second, "second")] {
        workspace
            .update(cx, |workspace, cx| {
                workspace.create_rectangle(
                    document_id,
                    0,
                    MarkupId::new(format!("application-close:{suffix}")).unwrap(),
                    PdfPoint::new(72., 96.).unwrap(),
                    PdfPoint::new(216., 192.).unwrap(),
                    cx,
                )
            })
            .unwrap();
    }
    let close = cx.new({
        let workspace = workspace.clone();
        let saver = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    (workspace, close, backend, [first, second])
}

fn dirty_two_generated_documents(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<DocumentWorkspace>,
    gpui::Entity<ApplicationCloseWorkspace>,
    Arc<DeterministicBackend>,
    [DocumentId; 2],
) {
    static NEXT_ROOT: AtomicU32 = AtomicU32::new(1);

    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".prepared")
        .join(format!(
            "application-close-generated-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::remove_dir_all(&root).ok();
    let store = GeneratedDocumentStore::new(root.clone()).unwrap();
    backend.generated_roots.lock().unwrap().push(root);
    let create =
        |title: &str, workspace: &gpui::Entity<DocumentWorkspace>, cx: &mut TestAppContext| {
            workspace
                .update(cx, |workspace, cx| {
                    workspace.create_generated_document(
                        store.clone(),
                        GeneratedDocumentRequest {
                            title: title.into(),
                            width_mm: 420.,
                            height_mm: 297.,
                            pattern: None,
                        },
                        cx,
                    )
                })
                .unwrap()
        };
    let first = create("first", &workspace, cx);
    let second = create("second", &workspace, cx);
    cx.run_until_parked();
    let close = cx.new({
        let workspace = workspace.clone();
        let saver = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    (workspace, close, backend, [first, second])
}

fn pending_save_as(
    close: &gpui::Entity<ApplicationCloseWorkspace>,
    cx: &TestAppContext,
) -> PendingApplicationSaveAs {
    close
        .read_with(cx, |close, _| close.pending_save_as().cloned())
        .expect("Save All must request the next Save As target")
}

#[gpui::test]
fn application_close_commits_live_text_box_draft_or_surfaces_blocked_draft_without_transaction(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, document_ids) = dirty_two_documents(cx);
    let action_close = cx.new({
        let workspace = workspace.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });
    let document_id = document_ids[1];
    let id = MarkupId::new("application-close:text-box-draft").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_text_box(
                document_id,
                TextBoxAnnotation::new(
                    id.clone(),
                    0,
                    PdfRect::new(300., 300., 100., 40.).unwrap(),
                    "baseline",
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                )
                .unwrap(),
                cx,
            )
        })
        .unwrap();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.activate_document(document_id, cx)
    }));
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &id, cx)
    }));
    let close_for_window = action_close.clone();
    let content = AnyView::from(workspace.clone());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                close_for_window,
                content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let body = point(
        origin.x + px(340. * scale),
        origin.y + px((792. - 320.) * scale),
    );
    let double_click = |cx: &mut gpui::VisualTestContext| {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: body,
            modifiers: Modifiers::default(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: body,
            modifiers: Modifiers::default(),
            click_count: 2,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };

    double_click(cx);
    let input = workspace
        .read_with(cx, |workspace, _| workspace.pending_text_box_input())
        .expect("double-click must retain the application-close draft");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(Some(0..8), "committed once", window, cx)
        })
    });
    let before = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        close
            .update(cx, |close, cx| close.request_close(cx))
            .unwrap(),
        ApplicationCloseTransitionStatus::Applied,
    );
    let committed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(committed.text_boxes[0].content(), "committed once");
    assert_eq!(committed.revision, before.revision + 1);
    assert_eq!(committed.undo_depth, before.undo_depth + 1);
    assert!(
        workspace
            .read_with(cx, |workspace, _| workspace.pending_text_box_input())
            .is_none()
    );
    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::Cancel, cx)
    });

    double_click(cx);
    let blocked_input = workspace
        .read_with(cx, |workspace, _| workspace.pending_text_box_input())
        .expect("the editor must reopen for blocked application close");
    cx.update(|window, cx| {
        blocked_input.update(cx, |input, cx| {
            input.replace_text_in_range(
                Some(0.."committed once".len()),
                "blocked draft",
                window,
                cx,
            )
        })
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(document_id, true, cx)
        })
        .unwrap();
    let locked = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    cx.dispatch_action(RequestApplicationClose);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID)
            .is_some(),
        "the actual close action must render the blocked draft error",
    );
    let accessibility_tree = cx.update(|window, _| window.debug_a11y_tree_json());
    if let Some(accessibility_tree) = accessibility_tree {
        assert!(accessibility_tree.contains("Couldn’t finish editing"));
        assert!(accessibility_tree.contains("Text Box is locked"));
    }
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("blocked draft".to_owned()),
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        }),
        Some(locked),
    );
    assert!(
        workspace
            .read_with(cx, |workspace, _| workspace
                .text_box_commit_error()
                .map(str::to_owned))
            .is_some_and(|error| error.contains("locked"))
    );
    assert!(action_close.read_with(cx, |close, _| close.close_snapshot().is_none()));
    assert!(action_close.read_with(cx, |close, _| close.dialog().is_none()));
    assert!(!action_close.read_with(cx, |close, _| close.has_quit_intent()));
    assert!(cx.update(|window, cx| { blocked_input.read(cx).focus_handle(cx).is_focused(window) }));
}

#[gpui::test]
fn session_checkpoint_publishes_ordered_active_workspace_before_release_and_quit(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let first = owned_target("checkpoint-first.pdf");
    let second = owned_target("checkpoint-second.pdf");
    workspace.update(cx, |workspace, cx| {
        workspace.open_path(first.clone(), cx);
        workspace.open_path(second.clone(), cx);
    });
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.set_view_configuration(
            DocumentId::new(1),
            PageViewMode::Continuous,
            125.,
            cx,
        ));
        assert!(workspace.set_viewport_scroll(DocumentId::new(1), 12., 24., cx));
        assert!(workspace.set_page_view_mode(DocumentId::new(2), PageViewMode::SinglePage, cx,));
        assert!(workspace.set_fit_preset(DocumentId::new(2), ViewerFitPreset::Page, cx));
        assert!(workspace.set_viewport_scroll(DocumentId::new(2), 36., 48., cx));
    });
    let expected_snapshot = SessionSnapshot::new(vec![first.clone(), second.clone()], Some(1))
        .with_restart_views(vec![
            RestartView::new(
                0,
                PageViewMode::Continuous,
                RestartZoom::Manual(125.),
                12.,
                24.,
            ),
            RestartView::new(0, PageViewMode::SinglePage, RestartZoom::FitPage, 36., 48.),
        ]);
    let (scratch, store) = application_close_manifest_store("ordered");
    let inspector = SessionManifestStore::open(scratch.0.clone()).unwrap();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::with_session_manifest_store(workspace, backend, store)
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    let (checkpoint_documents, checkpoint_active) = inspector.load().unwrap().into_documents();
    assert_eq!(
        checkpoint_documents
            .iter()
            .map(|document| (document.path().to_owned(), document.view()))
            .collect::<Vec<_>>(),
        vec![
            (
                first.clone(),
                RestartView::new(
                    0,
                    PageViewMode::Continuous,
                    RestartZoom::Manual(125.),
                    12.,
                    24.,
                ),
            ),
            (
                second.clone(),
                RestartView::new(0, PageViewMode::SinglePage, RestartZoom::FitPage, 36., 48.,),
            ),
        ]
    );
    assert_eq!(checkpoint_active, Some(1));
    let effects = close.read_with(cx, |close, _| close.effects().to_vec());
    let checkpoint_index = effects
        .iter()
        .position(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SessionCheckpointPublished {
                    document_count: 2,
                    ..
                }
            )
        })
        .unwrap();
    let release_index = effects
        .iter()
        .position(|effect| matches!(effect, ApplicationCloseEffect::ReleaseRequested { .. }))
        .unwrap();
    let quit_index = effects
        .iter()
        .position(ApplicationCloseEffect::is_quit_requested)
        .unwrap();
    assert!(checkpoint_index < release_index && release_index < quit_index);
    assert!(backend.all_released());
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));

    let restore_backend = Arc::new(DeterministicBackend::default());
    let restored = cx.new({
        let restore_backend = restore_backend.clone();
        move |cx| DocumentWorkspace::with_opener(restore_backend, cx)
    });
    restored.update(cx, |workspace, cx| {
        workspace.restore_session(inspector.load().unwrap(), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        restored.read_with(cx, |workspace, cx| workspace.session_snapshot(cx)),
        expected_snapshot,
        "a clean close checkpoint must restore into a fresh workspace",
    );
    for document_id in restored.read_with(cx, |workspace, cx| workspace.session_order(cx)) {
        assert!(restored.update(cx, |workspace, cx| {
            workspace.close_document(document_id, cx)
        }));
    }
    assert!(restore_backend.all_released());
}

#[gpui::test]
fn failed_session_checkpoint_preserves_manifest_resources_and_discard_retry_completion(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let current = owned_target("checkpoint-current.pdf");
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(current.clone(), cx));
    cx.run_until_parked();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("application-close:checkpoint-failure").unwrap(),
                PdfPoint::new(72., 72.).unwrap(),
                PdfPoint::new(144., 144.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let prior = owned_target("checkpoint-prior.pdf");
    let (scratch, seed_store) = application_close_manifest_store("failure");
    seed_store
        .replace(&SessionSnapshot::new(vec![prior.clone()], Some(0)))
        .unwrap();
    let manifest = scratch.0.join("session-manifest.json");
    let preserved_manifest = scratch.0.join("preserved-session-manifest.json");
    let prior_bytes = std::fs::read(&manifest).unwrap();
    std::fs::rename(&manifest, &preserved_manifest).unwrap();
    std::fs::create_dir(&manifest).unwrap();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| {
            ApplicationCloseWorkspace::with_session_manifest_store(workspace, backend, seed_store)
        }
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::DiscardAll, cx)
    });
    assert_eq!(
        close.read_with(cx, |close, _| close
            .recovery()
            .map(|recovery| recovery.kind)),
        Some(ApplicationCloseRecoveryKind::SessionCheckpointFailed)
    );
    assert_eq!(std::fs::read(&preserved_manifest).unwrap(), prior_bytes);
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_id, cx).is_some()
    }));
    assert!(!backend.all_released());
    assert!(!close.read_with(cx, |close, _| close.has_quit_intent()));

    std::fs::remove_dir(&manifest).unwrap();
    std::fs::rename(&preserved_manifest, &manifest).unwrap();
    let inspector = SessionManifestStore::open(scratch.0.clone()).unwrap();
    assert_eq!(
        inspector.load().unwrap().into_parts(),
        (vec![prior], Some(0))
    );
    assert_eq!(
        close
            .update(cx, |close, cx| close.resume_recovery(cx))
            .unwrap(),
        ApplicationCloseTransitionStatus::Applied
    );
    assert_eq!(
        inspector.load().unwrap().into_parts(),
        (vec![current], Some(0))
    );
    assert!(backend.all_released());
    assert!(close.read_with(cx, |close, _| close.effects().iter().any(
        |effect| matches!(
            effect,
            ApplicationCloseEffect::QuitRequested {
                kind: ApplicationCloseCompletionKind::DiscardedAll,
                ..
            }
        )
    )));
}

#[gpui::test]
fn session_checkpoint_durability_warning_is_reported_once_and_continues_close(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let current = owned_target("checkpoint-warning.pdf");
    workspace.update(cx, |workspace, cx| workspace.open_path(current.clone(), cx));
    cx.run_until_parked();
    let (scratch, _) = application_close_manifest_store("warning");
    let inspector = SessionManifestStore::open(scratch.0.clone()).unwrap();
    let publisher = Arc::new(DurabilityWarningCheckpointPublisher {
        store: SessionManifestStore::open(scratch.0.clone()).unwrap(),
        attempts: AtomicU32::new(0),
    });
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        let publisher = publisher.clone();
        move |_| ApplicationCloseWorkspace::with_checkpoint_publisher(workspace, backend, publisher)
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    assert_eq!(
        inspector.load().unwrap().into_parts(),
        (vec![current], Some(0))
    );
    assert_eq!(publisher.attempts.load(Ordering::Acquire), 1);
    assert_eq!(
        close.read_with(cx, |close, _| close
            .effects()
            .iter()
            .filter(|effect| matches!(
                effect,
                ApplicationCloseEffect::SessionCheckpointDurabilityWarning { .. }
            ))
            .count()),
        1
    );
    assert!(backend.all_released());
    assert!(close.read_with(cx, |close, _| close.has_quit_intent()));
}

#[gpui::test]
fn empty_workspace_replaces_stale_session_manifest_before_quit(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let stale = owned_target("checkpoint-stale.pdf");
    let (scratch, store) = application_close_manifest_store("empty");
    store
        .replace(&SessionSnapshot::new(vec![stale], Some(0)))
        .unwrap();
    let inspector = SessionManifestStore::open(scratch.0.clone()).unwrap();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::with_session_manifest_store(workspace, backend, store)
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    assert_eq!(inspector.load().unwrap().into_parts(), (Vec::new(), None));
    let effects = close.read_with(cx, |close, _| close.effects().to_vec());
    assert!(matches!(
        effects.first(),
        Some(ApplicationCloseEffect::SessionCheckpointPublished {
            document_count: 0,
            ..
        })
    ));
    assert!(
        effects
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    );
}

#[gpui::test]
fn generated_save_as_checkpoints_only_the_final_durable_target(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let generated_root = owned_target(format!("checkpoint-generated-root-{}", std::process::id()));
    std::fs::remove_dir_all(&generated_root).ok();
    let generated_store = GeneratedDocumentStore::new(generated_root.clone()).unwrap();
    backend.generated_roots.lock().unwrap().push(generated_root);
    let document_id = workspace
        .update(cx, |workspace, cx| {
            workspace.create_generated_document(
                generated_store,
                GeneratedDocumentRequest::a3_landscape_blank(),
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    let temporary_path = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .path()
            .to_owned()
    });
    let final_target = owned_target("checkpoint-generated-final.pdf");
    let (scratch, store) = application_close_manifest_store("generated-save-as");
    let inspector = SessionManifestStore::open(scratch.0.clone()).unwrap();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::with_session_manifest_store(workspace, backend, store)
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let prompt = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(prompt.token, Some(final_target.clone()), cx)
    });
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );
    assert_ne!(temporary_path, final_target);
    assert_eq!(
        inspector.load().unwrap().into_parts(),
        (vec![final_target], Some(0))
    );
    assert!(backend.all_released());
    assert!(close.read_with(cx, |close, _| close.effects().iter().any(
        |effect| matches!(
            effect,
            ApplicationCloseEffect::QuitRequested {
                kind: ApplicationCloseCompletionKind::SavedAll,
                ..
            }
        )
    )));
}

#[gpui::test]
fn native_close_snapshot_uses_stable_ids_and_cancel_preserves_live_documents(
    cx: &mut TestAppContext,
) {
    let (workspace, close, _, document_ids) = dirty_two_documents(cx);

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    let snapshot = close
        .read_with(cx, |close, _| close.close_snapshot().cloned())
        .unwrap();
    assert_eq!(
        snapshot
            .documents
            .iter()
            .map(|document| document.id.as_str())
            .collect::<Vec<_>>(),
        ["document-1", "document-2"]
    );
    assert!(
        snapshot
            .documents
            .iter()
            .all(|document| { !document.requires_save_as && document.dirty_revision == Some(1) })
    );
    assert_eq!(snapshot.active_document_id.as_deref(), Some("document-2"));

    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::Cancel, cx)
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        2
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(document_ids[1])
    );
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::CloseCancelled { .. })
    ));
}

#[gpui::test]
fn duplicate_application_close_request_cannot_replace_the_frozen_transaction_snapshot(
    cx: &mut TestAppContext,
) {
    let (workspace, close, _, document_ids) = dirty_two_documents(cx);
    assert_eq!(
        close
            .update(cx, |close, cx| close.request_close(cx))
            .unwrap(),
        ApplicationCloseTransitionStatus::Applied,
    );
    let frozen = close
        .read_with(cx, |close, _| close.close_snapshot().cloned())
        .unwrap();

    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_ids[0],
                0,
                MarkupId::new("application-close:after-frozen-snapshot").unwrap(),
                PdfPoint::new(300., 300.).unwrap(),
                PdfPoint::new(360., 360.).unwrap(),
                cx,
            )
        })
        .unwrap();
    assert_eq!(
        close
            .update(cx, |close, cx| close.request_close(cx))
            .unwrap(),
        ApplicationCloseTransitionStatus::IgnoredPending,
    );
    assert_eq!(
        close.read_with(cx, |close, _| close.close_snapshot().cloned()),
        Some(frozen),
    );
}

#[gpui::test]
fn opened_dirty_documents_save_in_place_during_tab_and_application_close(cx: &mut TestAppContext) {
    let (workspace, close, backend, document_ids) = dirty_two_documents(cx);
    let first_path = PathBuf::from("fixtures/first.pdf");
    let second_path = PathBuf::from("fixtures/second.pdf");

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_ids[0], cx)
        }),
        butter_paper_gpui_component_compat::document_workspace::CloseRequestDisposition::ConfirmationRequired
    );
    let tab_close_request = workspace
        .update(cx, |workspace, cx| workspace.begin_dirty_close_save(cx))
        .expect("an opened dirty tab must save to its existing source before close");
    assert!(tab_close_request.is_in_place());
    assert_eq!(tab_close_request.target_path(), first_path.as_path());
    let saved = backend.save(&tab_close_request).unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.apply_save_result(&tab_close_request, Ok(saved), cx)
        }),
        ApplyDisposition::Applied
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_ids[0], cx).is_none()
            && workspace.session(document_ids[1], cx).is_some()
    }));

    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_ids[1],
                0,
                MarkupId::new("application-close:second-again").unwrap(),
                PdfPoint::new(240., 240.).unwrap(),
                PdfPoint::new(300., 300.).unwrap(),
                cx,
            )
        })
        .unwrap();
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    assert!(close.read_with(cx, |close, _| close.pending_save_as().is_none()));
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .pending_save()
                .map(|pending| pending.target_path.clone())
        }),
        Some(second_path)
    );
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );
    assert!(close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(|effect| matches!(effect, ApplicationCloseEffect::SaveApplied { document_id, .. } if document_id == "document-2"))
            && close
                .effects()
                .iter()
                .any(ApplicationCloseEffect::is_quit_requested)
    }));
}

#[gpui::test]
fn save_all_user_action_dispatches_every_opened_document_without_a_sync_driver(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, _) = dirty_two_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();

    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    assert!(
        close.read_with(cx, |close, _| close.pending_save().is_none()),
        "the user-facing action must move PDF writing off the GPUI thread",
    );

    cx.run_until_parked();
    assert!(backend.all_released());
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .effects()
                .iter()
                .filter_map(|effect| match effect {
                    ApplicationCloseEffect::SaveApplied { document_id, .. } => {
                        Some(document_id.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        }),
        ["document-1".to_owned(), "document-2".to_owned()],
    );
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::QuitRequested {
            kind: ApplicationCloseCompletionKind::SavedAll,
            ..
        })
    ));
}

#[gpui::test]
fn published_durability_warning_interrupts_tab_and_application_close_without_losing_the_save(
    cx: &mut TestAppContext,
) {
    let warning =
        "saved PDF was published, but its directory durability sync failed: injected".to_owned();
    let (workspace, _, backend, document_ids) = dirty_two_documents(cx);
    *backend.save_warning.lock().unwrap() = Some(warning.clone());
    workspace.update(cx, |workspace, cx| {
        workspace.request_close_document(document_ids[0], cx)
    });
    let request = workspace
        .update(cx, |workspace, cx| workspace.begin_dirty_close_save(cx))
        .unwrap();
    let saved = backend.save(&request).unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.apply_save_result(&request, Ok(saved), cx)
    });
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_ids[0], cx).is_some()
            && workspace.pending_close_document_id() == Some(document_ids[0])
            && workspace.close_after_save_document_id().is_none()
            && workspace
                .document_dirty_revision(document_ids[0], cx)
                .is_none()
            && workspace.active_document_id() == Some(document_ids[1])
    }));
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.activate_document(document_ids[0], cx)
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status()),
        Some(warning.clone())
    );

    let (application_workspace, close, application_backend, application_ids) =
        dirty_two_documents(cx);
    *application_backend.save_warning.lock().unwrap() = Some(warning.clone());
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );
    assert!(close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveWarningReported {
                    document_id,
                    message,
                    ..
                } if document_id == "document-1" && message == &warning
            )
        }) && !close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
    assert!(application_workspace.read_with(cx, |workspace, cx| {
        workspace.session(application_ids[0], cx).is_some()
            && workspace
                .document_dirty_revision(application_ids[0], cx)
                .is_none()
            && workspace.session(application_ids[1], cx).is_some()
            && workspace
                .document_dirty_revision(application_ids[1], cx)
                .is_some()
    }));
    assert_eq!(
        close.read_with(cx, |close, _| close
            .recovery()
            .map(|recovery| recovery.kind)),
        Some(ApplicationCloseRecoveryKind::PublishedWithWarning)
    );
}

#[gpui::test]
fn mixed_opened_and_generated_save_all_is_serial_and_cancel_preserves_earlier_save(
    cx: &mut TestAppContext,
) {
    static NEXT_ROOT: AtomicU32 = AtomicU32::new(10_000);

    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let opened = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("fixtures/opened-first.pdf"), cx)
    });
    cx.run_until_parked();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                opened,
                0,
                MarkupId::new("application-close:mixed-opened").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".prepared")
        .join(format!(
            "application-close-mixed-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::remove_dir_all(&root).ok();
    let store = GeneratedDocumentStore::new(root.clone()).unwrap();
    backend.generated_roots.lock().unwrap().push(root);
    let generated = workspace
        .update(cx, |workspace, cx| {
            workspace.create_generated_document(
                store,
                GeneratedDocumentRequest {
                    title: "generated-second".into(),
                    width_mm: 420.,
                    height_mm: 297.,
                    pattern: None,
                },
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    let close = cx.new({
        let workspace = workspace.clone();
        let backend = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, backend)
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .pending_save()
                .map(|pending| pending.target_path.clone())
        }),
        Some(PathBuf::from("fixtures/opened-first.pdf"))
    );
    assert!(close.read_with(cx, |close, _| close.pending_save_as().is_none()));
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );
    let generated_prompt = pending_save_as(&close, cx);
    assert_eq!(generated_prompt.document_id, generated.to_string());
    close.update(cx, |close, cx| {
        close.resolve_save_as(generated_prompt.token, None, cx)
    });
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(opened, cx).is_some()
            && workspace.document_dirty_revision(opened, cx).is_none()
            && workspace.session(generated, cx).is_some()
            && workspace.document_dirty_revision(generated, cx).is_some()
    }));
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    assert_eq!(
        pending_save_as(&close, cx).document_id,
        generated.to_string()
    );
}

#[gpui::test]
fn generated_save_all_user_action_prompts_and_continues_until_the_next_picker_is_cancelled(
    cx: &mut TestAppContext,
) {
    let (workspace, close, _, document_ids) = dirty_two_generated_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });

    assert!(cx.did_prompt_for_new_path());
    assert_eq!(pending_save_as(&close, cx).suggested_name, "Untitled.pdf",);
    cx.simulate_new_path_selection(|directory| {
        assert_eq!(directory, Path::new(""));
        Some(owned_target("generated-first.pdf"))
    });
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|directory| {
        assert_eq!(directory, Path::new(""));
        None
    });
    cx.run_until_parked();

    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_ids[0], cx).is_some()
            && workspace
                .document_dirty_revision(document_ids[0], cx)
                .is_none()
            && workspace.session(document_ids[1], cx).is_some()
            && workspace
                .document_dirty_revision(document_ids[1], cx)
                .is_some()
    }));
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .effects()
                .iter()
                .filter_map(|effect| match effect {
                    ApplicationCloseEffect::SaveApplied {
                        document_id,
                        target_path,
                        ..
                    } => Some((document_id.clone(), target_path.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>()
        }),
        [("document-1".to_owned(), owned_target("generated-first.pdf"),)],
    );
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::CloseCancelled { .. })
    ));
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
}

#[gpui::test]
fn generated_application_close_rejects_a_non_pdf_picker_result_without_saving_or_duplicate_prompt(
    cx: &mut TestAppContext,
) {
    let (workspace, close, _, document_ids) = dirty_two_generated_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(owned_target("generated-not-a-pdf.txt")));
    cx.run_until_parked();

    assert!(!cx.did_prompt_for_new_path());
    assert!(workspace.read_with(cx, |workspace, cx| {
        document_ids.iter().all(|document_id| {
            workspace.session(*document_id, cx).is_some()
                && workspace
                    .document_dirty_revision(*document_id, cx)
                    .is_some()
        })
    }));
    assert!(close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveFailureReported {
                    document_id,
                    message,
                    ..
                } if document_id == "document-1"
                    && message == "Save As requires a .pdf file name."
            )
        }) && !close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveStarted { .. }
                    | ApplicationCloseEffect::SaveApplied { .. }
                    | ApplicationCloseEffect::QuitRequested { .. }
            )
        })
    }));
    assert!(close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(|effect| matches!(effect, ApplicationCloseEffect::CloseCancelled { .. }))
    }));
    assert_eq!(
        close.read_with(cx, |close, _| close
            .recovery()
            .map(|recovery| recovery.kind)),
        Some(ApplicationCloseRecoveryKind::TargetRejected)
    );
}

#[gpui::test]
fn stale_native_save_as_picker_cannot_resolve_a_newer_application_close_transaction(
    cx: &mut TestAppContext,
) {
    let (_, close, _, _) = dirty_two_generated_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    let stale = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(stale.token.clone(), None, cx)
    });

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    let current = pending_save_as(&close, cx);
    assert_ne!(current.token.transaction_id, stale.token.transaction_id);

    cx.simulate_new_path_selection(|_| Some(owned_target("stale-generated.pdf")));
    cx.run_until_parked();
    assert_eq!(pending_save_as(&close, cx), current);
    assert!(cx.did_prompt_for_new_path());
    assert!(!close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveStarted { .. }
                    | ApplicationCloseEffect::SaveApplied { .. }
                    | ApplicationCloseEffect::QuitRequested { .. }
            )
        })
    }));

    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_new_path());
}

#[gpui::test]
fn generated_document_mutation_while_save_as_is_open_prevents_application_close_save(
    cx: &mut TestAppContext,
) {
    let (workspace, close, _, document_ids) = dirty_two_generated_documents(cx);
    let snapshot_revision = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_ids[0], cx)
        })
        .expect("the generated document must be dirty before application close");
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_ids[0],
                0,
                MarkupId::new("application-close:mutated-during-picker").unwrap(),
                PdfPoint::new(360., 360.).unwrap(),
                PdfPoint::new(420., 420.).unwrap(),
                cx,
            )
        })
        .unwrap();
    cx.simulate_new_path_selection(|_| Some(owned_target("mutated-generated.pdf")));
    cx.run_until_parked();

    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .document_dirty_revision(document_ids[0], cx)
            .is_some_and(|revision| revision > snapshot_revision)
            && workspace.session(document_ids[0], cx).is_some()
    }));
    assert!(close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveFailureReported {
                    document_id,
                    message,
                    ..
                } if document_id == "document-1"
                    && message == "document changed after the application-close snapshot"
            )
        }) && !close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveStarted { .. }
                    | ApplicationCloseEffect::SaveApplied { .. }
                    | ApplicationCloseEffect::QuitRequested { .. }
            )
        })
    }));
}

#[gpui::test]
fn save_all_is_serial_and_save_as_cancel_or_duplicate_request_never_requests_quit(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, document_ids) = dirty_two_generated_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let cancelled = pending_save_as(&close, cx);
    let mut stale_cancel = cancelled.token.clone();
    stale_cancel.request_sequence += 100;
    let cancellation_effects_before = close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .filter(|effect| matches!(effect, ApplicationCloseEffect::SaveAsCancelled { .. }))
            .count()
    });
    close.update(cx, |close, cx| {
        close.resolve_save_as(stale_cancel, None, cx)
    });
    assert_eq!(pending_save_as(&close, cx), cancelled);
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .effects()
                .iter()
                .filter(|effect| matches!(effect, ApplicationCloseEffect::SaveAsCancelled { .. }))
                .count()
        }),
        cancellation_effects_before,
        "a stale Save As result must not emit a cancellation effect",
    );
    close.update(cx, |close, cx| {
        close.resolve_save_as(cancelled.token, None, cx)
    });
    assert!(close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveAsCancelled { document_id, .. }
                    if document_id == "document-1"
            )
        }) && !close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));

    backend.fail_saves.store(true, Ordering::Release);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let failing = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(failing.token, Some(owned_target("failing.pdf")), cx)
    });
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );
    assert!(close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveFailureReported {
                    document_id,
                    message,
                    ..
                } if document_id == "document-1" && message == "deterministic Save As failure"
            )
        }) && !close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
    backend.fail_saves.store(false, Ordering::Release);

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let first = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(first.token, Some(owned_target("first.pdf")), cx)
    });
    assert_eq!(
        close.read_with(cx, |close, _| {
            close.pending_save().map(|pending| pending.document_id)
        }),
        Some(document_ids[0])
    );

    assert_eq!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.begin_save_as(document_ids[0], owned_target("newer-first.pdf"), cx)
            })
            .unwrap_err(),
        "document save is already in progress"
    );
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );
    assert!(close.read_with(cx, |close, _| {
        close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::SaveApplied { document_id, .. }
                    if document_id == "document-1"
            )
        }) && !close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
    let second = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(second.token, None, cx)
    });
}

#[gpui::test]
fn application_close_save_failure_retains_typed_visible_recovery(cx: &mut TestAppContext) {
    let (workspace, close, backend, document_ids) = dirty_two_generated_documents(cx);
    backend.fail_saves.store(true, Ordering::Release);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let pending = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(
            pending.token,
            Some(owned_target("visible-recovery.pdf")),
            cx,
        )
    });
    assert_eq!(
        close.update(cx, |close, cx| close.drive_pending_save(cx)),
        Some(ApplyDisposition::Applied)
    );

    let recovery = close
        .read_with(cx, |close, _| close.recovery().cloned())
        .expect("a failed Save All must remain visibly recoverable");
    assert_eq!(recovery.kind, ApplicationCloseRecoveryKind::SaveFailed);
    assert_eq!(recovery.document_id, "document-1");
    assert_eq!(recovery.document_name, "Untitled.pdf");
    assert_eq!(recovery.title, "Couldn’t save “Untitled.pdf”");
    assert_eq!(recovery.primary_label, "Try again");
    assert!(recovery.message.contains("deterministic Save As failure"));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_ids[0], cx).is_some()
    }));
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
}

#[gpui::test]
fn application_close_shell_renders_recovery_and_retry_resumes_without_republishing(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, document_ids) = dirty_two_generated_documents(cx);
    backend.fail_saves.store(true, Ordering::Release);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let pending = pending_save_as(&close, cx);
    close.update(cx, |close, cx| {
        close.resolve_save_as(
            pending.token,
            Some(owned_target("rendered-recovery.pdf")),
            cx,
        )
    });
    close.update(cx, |close, cx| close.drive_pending_save(cx));

    let rendered_close = close.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|_| ApplicationCloseShell::new(rendered_close.clone()));
        Root::new(shell, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(APPLICATION_CLOSE_RECOVERY_ID).is_some());
    assert!(
        cx.debug_bounds(APPLICATION_CLOSE_RECOVERY_ALERT_ID)
            .is_some()
    );
    assert!(
        cx.debug_bounds(APPLICATION_CLOSE_RECOVERY_CANCEL_ID)
            .is_some()
    );
    let retry = cx
        .debug_bounds(APPLICATION_CLOSE_RECOVERY_PRIMARY_ID)
        .expect("the visible failure must expose its semantic recovery command");

    backend.fail_saves.store(false, Ordering::Release);
    cx.simulate_click(retry.center(), Modifiers::default());
    cx.run_until_parked();
    assert!(close.read_with(cx, |close, _| close.recovery().is_none()));
    assert!(close.read_with(cx, |close, _| close.pending_save_as().is_some()));
    assert!(cx.did_prompt_for_new_path());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_ids[0], cx).is_some()
    }));
    assert_eq!(
        close.read_with(cx, |close, _| close
            .effects()
            .iter()
            .filter(|effect| matches!(effect, ApplicationCloseEffect::SaveApplied { .. }))
            .count()),
        0,
        "retrying before publication must not fabricate or repeat a completed save"
    );
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
}

#[cfg(unix)]
#[gpui::test]
fn save_as_selection_preserves_a_non_utf8_native_path(cx: &mut TestAppContext) {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

    let (_, close, _, _) = dirty_two_generated_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    let pending = pending_save_as(&close, cx);
    let target = owned_target(PathBuf::from(OsString::from_vec(
        b"non-utf8-\xff.pdf".to_vec(),
    )));
    close.update(cx, |close, cx| {
        close.resolve_save_as(pending.token, Some(target.clone()), cx)
    });
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .pending_save()
                .map(|pending| pending.target_path.clone())
        }),
        Some(target),
    );
}

#[gpui::test]
fn save_all_and_discard_all_preserve_tab_order_and_acknowledge_real_workspace_release(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, _) = dirty_two_documents(cx);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::SaveAll, cx)
    });
    for source in [
        PathBuf::from("fixtures/first.pdf"),
        PathBuf::from("fixtures/second.pdf"),
    ] {
        assert!(close.read_with(cx, |close, _| close.pending_save_as().is_none()));
        assert_eq!(
            close.read_with(cx, |close, _| {
                close
                    .pending_save()
                    .map(|pending| pending.target_path.clone())
            }),
            Some(source)
        );
        assert_eq!(
            close.update(cx, |close, cx| close.drive_pending_save(cx)),
            Some(ApplyDisposition::Applied)
        );
    }
    let saved_order = close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                ApplicationCloseEffect::SaveApplied { document_id, .. } => {
                    Some(document_id.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(saved_order, ["document-1", "document-2"]);
    assert!(backend.all_released());
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::QuitRequested {
            kind: ApplicationCloseCompletionKind::SavedAll,
            ..
        })
    ));

    let (discard_workspace, discard_close, discard_backend, _) = dirty_two_documents(cx);
    discard_close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    discard_close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::DiscardAll, cx)
    });
    let released_order = discard_close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                ApplicationCloseEffect::ReleaseAcknowledged { document_id, .. } => {
                    Some(document_id.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(released_order, ["document-1", "document-2"]);
    assert!(discard_backend.all_released());
    assert!(discard_workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(matches!(
        discard_close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::QuitRequested {
            kind: ApplicationCloseCompletionKind::DiscardedAll,
            ..
        })
    ));
}

#[gpui::test]
fn worker_close_failure_is_not_acknowledged_or_promoted_to_quit(cx: &mut TestAppContext) {
    let (workspace, close, backend, document_ids) = dirty_two_documents(cx);
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_ids[0], cx)
        }),
        butter_paper_gpui_component_compat::document_workspace::CloseRequestDisposition::ConfirmationRequired
    );
    backend.fail_closes.store(true, Ordering::Release);
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        butter_paper_gpui_component_compat::document_workspace::DirtyCloseResolution::ReleaseFailed
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.pending_close_document_id()),
        Some(document_ids[0])
    );
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::DiscardAll, cx)
    });
    assert!(close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(|effect| matches!(effect, ApplicationCloseEffect::ReleaseFailed { .. }))
    }));
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(|effect| matches!(effect, ApplicationCloseEffect::ReleaseAcknowledged { .. }))
            || close
                .effects()
                .iter()
                .any(ApplicationCloseEffect::is_quit_requested)
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        2,
        "a failed worker close must preserve every live document session"
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        document_ids
            .iter()
            .all(|document_id| workspace.session(*document_id, cx).is_some())
    }));
    assert!(!backend.all_released());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.pending_close_document_id()),
        Some(document_ids[0]),
        "fallible release must not clear the pending dirty-close identity"
    );
    assert!(close.read_with(cx, |close, _| close.effects().iter().any(|effect| {
        matches!(
            effect,
            ApplicationCloseEffect::CloseCancelled {
                reason: butter_paper_gpui_component_compat::application_close::ApplicationCloseInterruptionReason::ReleaseFailed,
                ..
            }
        )
    })));
    assert!(
        close.read_with(cx, |close, _| close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::ReleaseFailureReported { .. }
            )
        }))
    );
    assert_eq!(
        close.read_with(cx, |close, _| close
            .recovery()
            .map(|recovery| recovery.kind)),
        Some(ApplicationCloseRecoveryKind::ReleaseFailed)
    );

    backend.fail_closes.store(false, Ordering::Release);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::DiscardAll, cx)
    });
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(backend.all_released());
    assert!(close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
}

#[gpui::test]
fn clean_close_failure_is_reported_and_preserves_the_session(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let backend = Arc::new(DeterministicBackend::default());
    backend.fail_closes.store(true, Ordering::Release);
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(PathBuf::from("fixtures/clean.pdf"), cx)
    });
    cx.run_until_parked();

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        butter_paper_gpui_component_compat::document_workspace::CloseRequestDisposition::ReleaseFailed
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_id, cx).is_some()
            && workspace
                .last_file_error()
                .is_some_and(|message| message.contains("injected worker close failure"))
    }));
    let rendered_workspace = workspace.clone();
    let (_, cx) =
        cx.add_window_view(move |window, cx| Root::new(rendered_workspace.clone(), window, cx));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds(DOCUMENT_ERROR_ID).is_some(),
        "an active document must render its close failure"
    );
}

#[gpui::test]
fn later_serial_release_failure_reports_already_closed_document_identities(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, document_ids) = dirty_two_documents(cx);
    backend.fail_close_pid.store(4101, Ordering::Release);
    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::DiscardAll, cx)
    });

    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_ids[0], cx).is_none()
            && workspace.session(document_ids[1], cx).is_some()
    }));
    assert!(
        close.read_with(cx, |close, _| close.effects().iter().any(|effect| {
            matches!(
                effect,
                ApplicationCloseEffect::ReleaseFailureReported {
                    document_id,
                    completed_release_document_ids,
                    ..
                } if document_id == "document-2"
                    && completed_release_document_ids == &["document-1"]
            )
        }))
    );
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
}

#[gpui::test]
fn close_after_save_failure_preserves_retry_identities_and_live_session(cx: &mut TestAppContext) {
    let (workspace, _, backend, document_ids) = dirty_two_documents(cx);
    let document_id = document_ids[0];
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        butter_paper_gpui_component_compat::document_workspace::CloseRequestDisposition::ConfirmationRequired
    );
    let request = workspace
        .update(cx, |workspace, cx| workspace.begin_dirty_close_save(cx))
        .unwrap();
    backend.fail_closes.store(true, Ordering::Release);
    let saved = backend.save(&request).unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.apply_save_result(&request, Ok(saved), cx)
        }),
        ApplyDisposition::Applied
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_id, cx).is_some()
            && workspace.pending_close_document_id() == Some(document_id)
            && workspace.close_after_save_document_id() == Some(document_id)
    }));
}

#[gpui::test]
fn native_window_close_is_blocked_until_the_application_transaction_completes(
    cx: &mut TestAppContext,
) {
    let (workspace, close, backend, document_ids) = dirty_two_documents(cx);
    let close_for_window = close.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell =
            cx.new(|cx| ApplicationCloseShell::new_for_native_window(close_for_window, window, cx));
        Root::new(shell, window, cx)
    });

    assert!(
        !cx.simulate_close(),
        "the platform close request must be intercepted while dirty documents await a decision"
    );
    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        2
    );
    assert!(!backend.all_released());
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));

    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::Cancel, cx)
    });
    assert!(workspace.read_with(cx, |workspace, cx| {
        document_ids
            .iter()
            .all(|document_id| workspace.session(*document_id, cx).is_some())
    }));
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_mixed_document_application_close_saves_generated_target_and_releases_resources(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().unwrap();
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let public_sources = [
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf"),
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf"),
    ];
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(public_sources.iter().all(|source| source.is_file()));

    let owned_root = manifest_dir
        .join(".prepared/application-close-real")
        .join(std::process::id().to_string());
    std::fs::remove_dir_all(&owned_root).ok();
    let source_root = owned_root.join("sources");
    let surface_root = owned_root.join("surfaces");
    std::fs::create_dir_all(&source_root).unwrap();
    let session_root = owned_root.join("session-state");
    std::fs::create_dir_all(&session_root).unwrap();
    let session_store = Arc::new(SessionManifestStore::open(session_root.clone()).unwrap());
    let public_bytes = public_sources
        .each_ref()
        .map(|source| std::fs::read(source).unwrap());
    let sources = [
        source_root.join("first.pdf"),
        source_root.join("second.pdf"),
    ];
    for (public, owned) in public_sources.iter().zip(&sources) {
        std::fs::copy(public, owned).unwrap();
    }
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker.clone(),
        library.clone(),
        surface_root.clone(),
    ));
    let saver = Arc::new(PdfDocumentSaver::new(backend.clone()));

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let close_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let close_slot = close_slot.clone();
        let backend = backend.clone();
        let session_store = session_store.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            let close = cx.new(|_| {
                let publisher: Arc<dyn ApplicationCloseCheckpointPublisher> = session_store;
                ApplicationCloseWorkspace::with_checkpoint_publisher(
                    workspace.clone(),
                    saver,
                    publisher,
                )
            });
            workspace_slot.replace(Some(workspace));
            close_slot.replace(Some(close.clone()));
            let shell = cx.new(|_| ApplicationCloseShell::new(close));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let close = close_slot.borrow_mut().take().unwrap();
    let document_ids = sources
        .clone()
        .map(|source| workspace.update(cx, |workspace, cx| workspace.open_path(source, cx)));
    cx.run_until_parked();
    for (document_id, suffix) in document_ids.into_iter().zip(["real-1", "real-2"]) {
        workspace
            .update(cx, |workspace, cx| {
                workspace.create_rectangle(
                    document_id,
                    0,
                    MarkupId::new(format!("application-close:{suffix}")).unwrap(),
                    PdfPoint::new(72., 96.).unwrap(),
                    PdfPoint::new(216., 192.).unwrap(),
                    cx,
                )
            })
            .unwrap();
    }
    let generated_store_root = owned_root.join("generated-sources");
    let generated_store = GeneratedDocumentStore::new(generated_store_root.clone()).unwrap();
    let generated_document = workspace
        .update(cx, |workspace, cx| {
            workspace.create_generated_document(
                generated_store.clone(),
                GeneratedDocumentRequest {
                    title: "Untitled".into(),
                    width_mm: 420.,
                    height_mm: 297.,
                    pattern: None,
                },
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    let generated_source = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(generated_document, cx)
            .unwrap()
            .read(cx)
            .path()
            .to_owned()
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                generated_document,
                0,
                MarkupId::new("application-close:real-generated").unwrap(),
                PdfPoint::new(144., 144.).unwrap(),
                PdfPoint::new(288., 240.).unwrap(),
                cx,
            )
        })
        .unwrap();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.move_document_session_by_keyboard(generated_document, -1, cx)
            && workspace.activate_document(document_ids[1], cx)
    }));
    let generated_target = source_root.join("generated.pdf");

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    close.update(cx, |close, cx| {
        close.choose(ApplicationCloseAction::Cancel, cx)
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        3
    );
    assert!(!close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
    assert!(close.read_with(cx, |close, _| close.dialog().is_none()));

    close
        .update(cx, |close, cx| close.request_close(cx))
        .unwrap();
    cx.update(|window, cx| {
        assert!(ApplicationCloseWorkspace::open_dialog(&close, window, cx));
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let mut observed_worker_pids = workspace.read_with(cx, |workspace, cx| {
        workspace
            .sessions()
            .iter()
            .filter_map(|session| session.read(cx).worker_pid())
            .collect::<Vec<_>>()
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(close.read_with(cx, |close, _| {
        close.pending_save_as().is_some() && close.pending_save().is_none()
    }));
    assert!(cx.did_prompt_for_new_path());
    assert_eq!(pending_save_as(&close, cx).suggested_name, "Untitled.pdf");
    cx.simulate_new_path_selection(|directory| {
        assert_eq!(directory, Path::new(""));
        Some(generated_target.clone())
    });
    cx.run_until_parked();
    observed_worker_pids.extend(close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                ApplicationCloseEffect::ReleaseAcknowledged { worker_pid, .. } => *worker_pid,
                _ => None,
            })
            .collect::<Vec<_>>()
    }));
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .effects()
                .iter()
                .filter_map(|effect| match effect {
                    ApplicationCloseEffect::SaveStarted { target_path, .. } => {
                        Some(target_path.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        }),
        [
            sources[0].clone(),
            generated_target.clone(),
            sources[1].clone(),
        ],
    );
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::QuitRequested {
            kind: ApplicationCloseCompletionKind::SavedAll,
            ..
        })
    ));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    let expected_session_paths = vec![
        sources[0].clone(),
        generated_target.clone(),
        sources[1].clone(),
    ];
    assert_eq!(
        session_store.load().unwrap().into_parts(),
        (expected_session_paths.clone(), Some(2))
    );
    let checkpoint_and_release = close.read_with(cx, |close, _| {
        let checkpoint = close
            .effects()
            .iter()
            .position(|effect| {
                matches!(
                    effect,
                    ApplicationCloseEffect::SessionCheckpointPublished {
                        document_count: 3,
                        ..
                    }
                )
            })
            .unwrap();
        let release = close
            .effects()
            .iter()
            .position(|effect| matches!(effect, ApplicationCloseEffect::ReleaseRequested { .. }))
            .unwrap();
        (checkpoint, release)
    });
    assert!(checkpoint_and_release.0 < checkpoint_and_release.1);
    observed_worker_pids.sort_unstable();
    observed_worker_pids.dedup();
    let closed_worker_pids = observed_worker_pids.clone();
    for pid in &closed_worker_pids {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "Save All release acknowledgements must follow mapped-surface release"
    );
    assert_eq!(
        close.read_with(cx, |close, _| {
            close
                .effects()
                .iter()
                .filter_map(|effect| match effect {
                    ApplicationCloseEffect::ReleaseAcknowledged { document_id, .. } => {
                        Some(document_id.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        }),
        [
            "document-1".to_owned(),
            "document-3".to_owned(),
            "document-2".to_owned(),
        ]
    );
    for ((public, original), owned) in public_sources
        .iter()
        .zip(public_bytes.iter())
        .zip(sources.iter())
    {
        assert_eq!(std::fs::read(public).unwrap(), *original);
        assert_ne!(std::fs::read(owned).unwrap(), *original);
        assert!(
            std::process::Command::new("qpdf")
                .arg("--check")
                .arg(owned)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("pdfinfo")
                .arg(owned)
                .status()
                .unwrap()
                .success()
        );
        let reopened = PdfPersistenceSession::open(owned).unwrap();
        assert!(
            reopened
                .rectangles()
                .iter()
                .any(|rectangle| { rectangle.id.as_str().starts_with("application-close:real-") })
        );
    }
    assert!(generated_target.is_file());
    assert!(!generated_source.exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&generated_target)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&generated_target)
            .status()
            .unwrap()
            .success()
    );
    let reopened_generated = PdfPersistenceSession::open(&generated_target).unwrap();
    assert!(
        reopened_generated
            .rectangles()
            .iter()
            .any(|rectangle| { rectangle.id.as_str() == "application-close:real-generated" })
    );
    generated_store.remove_if_empty().unwrap();
    assert!(!generated_store_root.exists());
    assert!(
        std::fs::read_dir(&source_root)
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains("butter-paper")),
        "successful in-place Save All must not leave a staging file"
    );

    let fresh_surface_root = owned_root.join("fresh-surfaces");
    let fresh_backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        fresh_surface_root.clone(),
    ));
    let fresh_saver = Arc::new(PdfDocumentSaver::new(fresh_backend.clone()));
    let mut fresh_cx = TestAppContext::single();
    fresh_cx.update(gpui_component::init);
    let fresh_workspace = fresh_cx.new({
        let fresh_backend = fresh_backend.clone();
        move |cx| DocumentWorkspace::with_opener(fresh_backend, cx)
    });
    let consumed = fresh_workspace.update(&mut fresh_cx, |workspace, cx| {
        workspace.begin_open(owned_root.join("process-local-id-placeholder.pdf"), cx)
    });
    assert!(fresh_workspace.update(&mut fresh_cx, |workspace, cx| {
        workspace.close_document(consumed.document_id, cx)
    }));
    let source = NativeLaunchSessionSource::new(false, &NativeLaunchConfig::default());
    assert!(source.requires_manifest_load());
    let resolution = source.resolve(Ok(Some(session_store.load().unwrap())));
    let NativeLaunchAction::Restore(plan) = resolution.action else {
        panic!("a valid non-empty manifest must resolve to startup restore");
    };
    fresh_workspace.update(&mut fresh_cx, |workspace, cx| {
        workspace.restore_session(plan, cx)
    });
    fresh_cx.run_until_parked();
    let restored = fresh_workspace.read_with(&fresh_cx, |workspace, cx| {
        workspace
            .sessions()
            .iter()
            .map(|session| {
                let session = session.read(cx);
                (
                    session.id(),
                    session.path().to_owned(),
                    session.worker_pid(),
                )
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        restored
            .iter()
            .map(|(_, path, _)| path.clone())
            .collect::<Vec<_>>(),
        expected_session_paths
    );
    assert_eq!(
        restored
            .iter()
            .map(|(id, _, _)| id.value())
            .collect::<Vec<_>>(),
        [2, 3, 4],
        "restored paths must receive new process-local document identities"
    );
    assert_eq!(
        fresh_workspace.read_with(&fresh_cx, |workspace, _| workspace.active_document_id()),
        Some(DocumentId::new(4))
    );
    let fresh_worker_pids = restored
        .iter()
        .map(|(_, _, pid)| pid.expect("each restored PDF has a fresh worker"))
        .collect::<Vec<_>>();
    assert!(fresh_worker_pids.iter().all(|pid| {
        !closed_worker_pids.contains(pid) && Path::new(&format!("/proc/{pid}")).exists()
    }));
    for document_id in [restored[0].0, restored[2].0] {
        let evidence = fresh_workspace
            .read_with(&fresh_cx, |workspace, cx| {
                workspace.evidence_snapshot(document_id, cx)
            })
            .unwrap();
        assert!(evidence.ready && evidence.current_raster_has_spatial_variation);
    }
    assert!(!fresh_workspace.read_with(&fresh_cx, |workspace, cx| {
        workspace.document_requires_save_as(restored[1].0, cx)
    }));
    assert_eq!(
        fresh_workspace.read_with(&fresh_cx, |workspace, cx| workspace.session_snapshot(cx)),
        SessionSnapshot::new(expected_session_paths, Some(2))
    );

    let fresh_close = fresh_cx.new({
        let fresh_workspace = fresh_workspace.clone();
        let session_store = session_store.clone();
        move |_| {
            let publisher: Arc<dyn ApplicationCloseCheckpointPublisher> = session_store;
            ApplicationCloseWorkspace::with_checkpoint_publisher(
                fresh_workspace,
                fresh_saver,
                publisher,
            )
        }
    });
    fresh_close
        .update(&mut fresh_cx, |close, cx| close.request_close(cx))
        .unwrap();
    assert!(fresh_close.read_with(&fresh_cx, |close, _| close.has_quit_intent()));
    assert!(fresh_workspace.read_with(&fresh_cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(
        fresh_worker_pids
            .iter()
            .all(|pid| !Path::new(&format!("/proc/{pid}")).exists())
    );
    assert!(
        !fresh_surface_root.exists()
            || std::fs::read_dir(&fresh_surface_root)
                .unwrap()
                .next()
                .is_none(),
        "fresh clean-close must release every restored mapped surface"
    );
    std::fs::remove_dir_all(owned_root).unwrap();
}
#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_native_shell_rectangle_edit_save_close_and_fresh_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().unwrap();
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let public_source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(public_source.is_file());
    let public_source_bytes = std::fs::read(&public_source).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&public_source_bytes)),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
    );
    let source_persistence = PdfPersistenceSession::open(&public_source).unwrap();
    let source_page_count = source_persistence.page_count();
    let source_untouched_annotations = source_persistence.untouched_annotations().to_vec();

    let owned_root = manifest_dir
        .join(".prepared/native-shell-rectangle-real")
        .join(std::process::id().to_string());
    std::fs::remove_dir_all(&owned_root).ok();
    std::fs::create_dir_all(&owned_root).unwrap();
    let owned_source = owned_root.join("edited.pdf");
    let sibling_source = owned_root.join("sibling.pdf");
    let failed_source = owned_root.join("missing.pdf");
    std::fs::copy(&public_source, &owned_source).unwrap();
    std::fs::copy(&public_source, &sibling_source).unwrap();
    let surface_root = owned_root.join("surfaces");
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));
    let saver = Arc::new(PdfDocumentSaver::new(backend.clone()));
    let base_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_084),
            generation: 1,
            path: owned_source.clone(),
        })
        .unwrap();
    let original_base_digest =
        Sha256::digest(base_proof.render_page(0, 320).unwrap().pixels_bgra()).to_vec();
    let base_worker_pid = base_proof.worker_pid().unwrap();
    base_proof.close().unwrap();
    assert!(!Path::new(&format!("/proc/{base_worker_pid}")).exists());

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let close = cx.new({
        let workspace = workspace.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    cx.update(|cx| register_application_close_action(&close, cx));
    let close_for_window = close.clone();
    let content = AnyView::from(workspace.clone());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                close_for_window,
                content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });

    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.open_documents(
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::Drop,
                [
                    owned_source.clone(),
                    sibling_source.clone(),
                    failed_source.clone(),
                ],
            ),
            cx,
        )),
        DocumentOpenBatchDisposition::Started {
            batch_id: 1,
            candidate_count: 3,
        }
    );
    cx.run_until_parked();
    let (document_id, sibling_id) = workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.sessions().len(), 2);
        assert_eq!(
            workspace.document_open_status(),
            &DocumentOpenBatchStatus::Completed {
                batch_id: 1,
                opened: vec![DocumentId::new(1), DocumentId::new(2)],
                focused_existing: None,
                failed_count: 1,
                status_message: "Loaded 2 documents".into(),
            }
        );
        let failure = workspace
            .last_document_open_failure()
            .expect("the missing sibling must produce one deterministic failure");
        assert_eq!(failure.path, failed_source);
        (DocumentId::new(1), DocumentId::new(2))
    });
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    let (original_worker_pid, sibling_worker_pid) = workspace.read_with(cx, |workspace, cx| {
        for id in [document_id, sibling_id] {
            let evidence = workspace
                .evidence_snapshot(id, cx)
                .expect("each public fixture copy must expose real page evidence");
            assert!(evidence.ready);
            assert_eq!(evidence.page_count, 100);
            assert!(evidence.current_raster_has_spatial_variation);
            let session = workspace.session(id, cx).unwrap().read(cx);
            assert!(
                session
                    .thumbnail_base_raster(0)
                    .is_some_and(|thumbnail| thumbnail.has_spatial_variation()),
                "each retained document must expose a nonblank real thumbnail",
            );
        }
        (
            workspace
                .session(document_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
                .unwrap(),
            workspace
                .session(sibling_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
                .unwrap(),
        )
    });
    assert!(Path::new(&format!("/proc/{original_worker_pid}")).exists());
    assert!(Path::new(&format!("/proc/{sibling_worker_pid}")).exists());

    let stale_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                document_id,
                PageViewMode::Continuous,
                100.,
                1.,
                640.,
                480.,
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    cx.dispatch_action(ZoomIn);
    assert!(
        workspace
            .update(cx, |workspace, cx| workspace
                .render_planned_tiles_for_evidence(document_id, &stale_plan, cx))
            .is_err(),
        "a zoom revision must reject stale real-PDF tile work",
    );
    let first_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(document_id, cx)
        })
        .unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let sibling_tab_id = Box::leak(document_session_tab_id(sibling_id).into_boxed_str());
    let sibling_tab = cx
        .debug_bounds(sibling_tab_id)
        .expect("the real sibling tab must render");
    cx.simulate_click(sibling_tab.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(sibling_id),
    );
    let sibling_thumbnail_id = Box::leak(document_thumbnail_id(sibling_id, 1).into_boxed_str());
    let sibling_thumbnail = cx
        .debug_bounds(sibling_thumbnail_id)
        .expect("the real page-2 sibling thumbnail must render");
    cx.simulate_click(sibling_thumbnail.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(sibling_id, cx)
            .unwrap()
            .read(cx)
            .current_page()),
        1,
        "the real thumbnail click must navigate to page 2 before the keybinding runs",
    );
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("end");
    cx.run_until_parked();
    cx.dispatch_action(ZoomIn);
    cx.dispatch_action(ZoomIn);
    cx.dispatch_action(RotatePageRight);
    cx.run_until_parked();
    let sibling_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(sibling_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(sibling_id, cx)
            .unwrap()
            .read(cx)
            .current_page()),
        99,
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(sibling_id, cx)
            .unwrap()
            .read(cx)
            .page_rotation(99)),
        Some(PageRotation::Degrees90),
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .page_rotation(0)),
        Some(PageRotation::Degrees0),
        "rotating the sibling must not change the first document's page rotation",
    );
    assert_ne!(sibling_view.zoom_percent(), first_view.zoom_percent());
    cx.simulate_keystrokes(EDIT_UNDO);
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(sibling_id, cx))
            .unwrap()
            .dirty,
        "undoing the sibling rotation must leave the sibling clean for tab close",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first_tab_id = Box::leak(document_session_tab_id(document_id).into_boxed_str());
    let first_tab = cx
        .debug_bounds(first_tab_id)
        .expect("the real first-document tab must render");
    cx.simulate_click(first_tab.center(), Modifiers::default());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .document_view_state(document_id, cx))
            .unwrap(),
        first_view,
        "switching documents must preserve the first document's independent view state",
    );

    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the real native annotation canvas must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };

    scroll_annotation_target_into_view(cx, DOCUMENT_RECTANGLE_TOOL_ID);
    let rectangle_tool = cx
        .debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID)
        .expect("the real Rectangle control must render");
    cx.simulate_click(rectangle_tool.center(), Modifiers::default());
    let create_start = to_view(100., 100.);
    let create_end = to_view(200., 200.);
    cx.simulate_mouse_down(create_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(create_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(create_end, MouseButton::Left, Modifiers::default());

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the Rectangle gesture must update retained state");
    assert_eq!(created.rectangles.len(), 1);
    let rectangle_id = created.rectangles[0].id.clone();
    assert_eq!(rectangle_id.as_str(), "workspace:rectangle:1");
    assert_eq!(created.selected_id.as_ref(), Some(&rectangle_id));
    let expected_appearance = created.rectangles[0].appearance.clone();

    scroll_annotation_target_into_view(cx, DOCUMENT_SELECT_TOOL_ID);
    let select_tool = cx
        .debug_bounds(DOCUMENT_SELECT_TOOL_ID)
        .expect("the real Select control must render");
    cx.simulate_click(select_tool.center(), Modifiers::default());
    let move_start = to_view(150., 150.);
    let move_end = to_view(168., 138.);
    cx.simulate_mouse_down(move_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(move_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(move_end, MouseButton::Left, Modifiers::default());
    let resize_start = to_view(218., 138.);
    let resize_end = to_view(248., 138.);
    cx.simulate_mouse_down(resize_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(resize_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(resize_end, MouseButton::Left, Modifiers::default());

    let expected_rect = PdfRect::new(118., 88., 130., 100.).unwrap();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real pointer edits must remain in retained state");
    assert_eq!(edited.rectangles[0].id, rectangle_id);
    assert_eq!(edited.rectangles[0].appearance, expected_appearance);
    for (actual, expected) in [
        (edited.rectangles[0].rect.x, expected_rect.x),
        (edited.rectangles[0].rect.y, expected_rect.y),
        (edited.rectangles[0].rect.width, expected_rect.width),
        (edited.rectangles[0].rect.height, expected_rect.height),
    ] {
        assert!(
            (actual - expected).abs() < 0.001,
            "native Rectangle gesture drifted: expected {expected}, got {actual}"
        );
    }
    assert!(edited.dirty);

    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_UNDO_ID);
    let undo = cx
        .debug_bounds(DOCUMENT_ANNOTATION_UNDO_ID)
        .expect("the real Undo command must render");
    cx.simulate_click(undo.center(), Modifiers::default());
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_ne!(undone.rectangles[0].rect, expected_rect);
    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_REDO_ID);
    let redo = cx
        .debug_bounds(DOCUMENT_ANNOTATION_REDO_ID)
        .expect("the real Redo command must render");
    cx.simulate_click(redo.center(), Modifiers::default());
    let redone_rect = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .rectangles[0]
        .rect;
    for (actual, expected) in [
        (redone_rect.x, expected_rect.x),
        (redone_rect.y, expected_rect.y),
        (redone_rect.width, expected_rect.width),
        (redone_rect.height, expected_rect.height),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let sibling_close_id = Box::leak(document_session_close_id(sibling_id).into_boxed_str());
    let sibling_close = cx
        .debug_bounds(sibling_close_id)
        .expect("the real clean-sibling tab close must render");
    cx.simulate_click(sibling_close.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1,
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(document_id),
    );
    assert!(!Path::new(&format!("/proc/{sibling_worker_pid}")).exists());
    let edited_after_sibling_close = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(edited_after_sibling_close.dirty);
    assert_eq!(edited_after_sibling_close.rectangles[0].id, rectangle_id);

    let mut fresh_app = cx.cx.clone();
    cx.dispatch_action(RequestApplicationClose);
    cx.run_until_parked();
    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(close.read_with(cx, |close, _| {
        close
            .effects()
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }));
    assert!(!Path::new(&format!("/proc/{original_worker_pid}")).exists());
    assert!(!Path::new(&format!("/proc/{sibling_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );

    let reopened = PdfPersistenceSession::open(&owned_source).unwrap();
    let saved_rectangle = reopened
        .rectangles()
        .iter()
        .find(|rectangle| rectangle.id == rectangle_id)
        .expect("the stable Rectangle ID must survive the first Save and reopen");
    for (actual, expected) in [
        (saved_rectangle.rect.x, expected_rect.x),
        (saved_rectangle.rect.y, expected_rect.y),
        (saved_rectangle.rect.width, expected_rect.width),
        (saved_rectangle.rect.height, expected_rect.height),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    assert_eq!(saved_rectangle.appearance, expected_appearance);
    assert_eq!(reopened.page_count(), source_page_count);
    assert_eq!(
        reopened.untouched_annotations(),
        source_untouched_annotations.as_slice()
    );

    let fresh_saver = Arc::new(PdfDocumentSaver::new(backend.clone()));
    let (fresh_workspace, fresh_close) = fresh_app.update(|cx| {
        gpui_component::init(cx);
        let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
        let close = cx.new({
            let workspace = workspace.clone();
            move |_| ApplicationCloseWorkspace::new(workspace, fresh_saver)
        });
        register_application_close_action(&close, cx);
        (workspace, close)
    });
    let fresh_content = AnyView::from(fresh_workspace.clone());
    let fresh_close_for_window = fresh_close.clone();
    let (_, fresh_cx) = fresh_app.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                fresh_close_for_window,
                fresh_content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });
    let reopened_id = fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    fresh_cx.run_until_parked();
    fresh_cx.update(|window, _| window.activate_window());
    fresh_cx.update(|window, cx| window.draw(cx).clear(cx));
    let reopened_worker_pid = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            let session = workspace.session(reopened_id, cx).unwrap();
            let snapshot = workspace.annotation_snapshot(reopened_id, cx).unwrap();
            assert!(!snapshot.dirty);
            let rectangle = snapshot
                .rectangles
                .iter()
                .find(|rectangle| rectangle.id == rectangle_id)
                .expect("the fresh workspace must import the stable Rectangle ID");
            for (actual, expected) in [
                (rectangle.rect.x, expected_rect.x),
                (rectangle.rect.y, expected_rect.y),
                (rectangle.rect.width, expected_rect.width),
                (rectangle.rect.height, expected_rect.height),
            ] {
                assert!((actual - expected).abs() < 0.001);
            }
            assert_eq!(rectangle.appearance, expected_appearance);
            session.read(cx).worker_pid()
        })
        .unwrap();

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_085),
            generation: 1,
            path: owned_source.clone(),
        })
        .unwrap();
    let annotated = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .unwrap();
    let saved_base = pixel_proof.render_page(0, 320).unwrap();
    assert_eq!(
        Sha256::digest(saved_base.pixels_bgra()).to_vec(),
        original_base_digest,
        "the untouched base-page channel must survive Save and fresh reopen exactly",
    );
    assert_ne!(
        Sha256::digest(annotated.pixels_bgra()),
        Sha256::digest(saved_base.pixels_bgra()),
        "the annotation-aware channel must remain distinct from untouched base content",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!Path::new(&format!("/proc/{pixel_worker_pid}")).exists());

    let reopened_layer_id =
        Box::leak(document_annotation_layer_id(reopened_id, 0).into_boxed_str());
    let reopened_layer = fresh_cx
        .debug_bounds(reopened_layer_id)
        .expect("the reopened native annotation canvas must render");
    let reopened_scale = (f32::from(reopened_layer.size.width) / 612.)
        .min(f32::from(reopened_layer.size.height) / 792.);
    let reopened_origin = point(
        reopened_layer.origin.x
            + px((f32::from(reopened_layer.size.width) - 612. * reopened_scale) / 2.),
        reopened_layer.origin.y
            + px((f32::from(reopened_layer.size.height) - 792. * reopened_scale) / 2.),
    );
    let reopened_to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            reopened_origin.x + px(pdf_x as f32 * reopened_scale),
            reopened_origin.y + px((792. - pdf_y as f32) * reopened_scale),
        )
    };
    scroll_annotation_target_into_view(fresh_cx, DOCUMENT_SELECT_TOOL_ID);
    let reopened_select = fresh_cx
        .debug_bounds(DOCUMENT_SELECT_TOOL_ID)
        .expect("the fresh shell must render the Select control");
    fresh_cx.simulate_click(reopened_select.center(), Modifiers::default());
    fresh_cx.simulate_click(reopened_to_view(183., 138.), Modifiers::default());
    assert_eq!(
        fresh_workspace
            .read_with(fresh_cx, |workspace, cx| workspace
                .annotation_snapshot(reopened_id, cx))
            .unwrap()
            .selected_id
            .as_ref(),
        Some(&rectangle_id)
    );
    scroll_annotation_target_into_view(fresh_cx, DOCUMENT_ANNOTATION_DELETE_ID);
    let delete = fresh_cx
        .debug_bounds(DOCUMENT_ANNOTATION_DELETE_ID)
        .expect("the fresh shell must render the Delete control");
    fresh_cx.simulate_click(delete.center(), Modifiers::default());
    let deleted = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_id, cx)
        })
        .unwrap();
    assert!(
        deleted
            .rectangles
            .iter()
            .all(|rectangle| rectangle.id != rectangle_id)
    );
    assert!(deleted.dirty);

    let mut second_fresh_app = fresh_cx.cx.clone();
    fresh_cx.dispatch_action(RequestApplicationClose);
    fresh_cx.run_until_parked();
    assert!(fresh_close.read_with(fresh_cx, |close, _| close.dialog().is_some()));
    assert!(fresh_cx.update(|window, cx| window.has_active_dialog(cx)));
    fresh_cx.simulate_keystrokes("enter");
    fresh_cx.run_until_parked();
    assert!(fresh_workspace.read_with(fresh_cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(!Path::new(&format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());

    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    let deleted_reopen = PdfPersistenceSession::open(&owned_source).unwrap();
    assert_eq!(deleted_reopen.page_count(), source_page_count);
    assert_eq!(
        deleted_reopen.untouched_annotations(),
        source_untouched_annotations.as_slice()
    );
    assert!(
        deleted_reopen
            .rectangles()
            .iter()
            .all(|rectangle| rectangle.id != rectangle_id)
    );
    assert!(!deleted_reopen.has_canonical_raw_annotation_name(&rectangle_id));

    let second_workspace = second_fresh_app.update(|cx| {
        gpui_component::init(cx);
        cx.new(|cx| DocumentWorkspace::with_opener(backend, cx))
    });
    let second_content = AnyView::from(second_workspace.clone());
    let (_, second_cx) =
        second_fresh_app.add_window_view(move |window, cx| Root::new(second_content, window, cx));
    let second_reopened_id = second_workspace.update(second_cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    second_cx.run_until_parked();
    let second_worker_pid = second_workspace
        .read_with(second_cx, |workspace, cx| {
            let snapshot = workspace
                .annotation_snapshot(second_reopened_id, cx)
                .unwrap();
            assert!(!snapshot.dirty);
            assert!(
                snapshot
                    .rectangles
                    .iter()
                    .all(|rectangle| rectangle.id != rectangle_id)
            );
            let evidence = workspace.evidence_snapshot(second_reopened_id, cx).unwrap();
            assert_eq!(evidence.page_count as usize, source_page_count);
            assert!(evidence.current_raster_has_spatial_variation);
            workspace
                .session(second_reopened_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
        })
        .unwrap();
    assert!(second_workspace.update(second_cx, |workspace, cx| {
        workspace.close_document(second_reopened_id, cx)
    }));
    assert!(!Path::new(&format!("/proc/{second_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    std::fs::remove_dir_all(owned_root).unwrap();
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_native_shell_pen_highlight_create_undo_redo_save_close_and_fresh_reopen(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().unwrap();
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let public_source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(public_source.is_file());
    let public_bytes = std::fs::read(&public_source).unwrap();
    let source_persistence = PdfPersistenceSession::open(&public_source).unwrap();
    let source_page_count = source_persistence.page_count();
    let source_untouched_annotations = source_persistence.untouched_annotations().to_vec();

    let owned_root = manifest_dir
        .join(".prepared/native-shell-pen-highlight-real")
        .join(std::process::id().to_string());
    std::fs::remove_dir_all(&owned_root).ok();
    std::fs::create_dir_all(&owned_root).unwrap();
    let owned_source = owned_root.join("edited.pdf");
    std::fs::copy(&public_source, &owned_source).unwrap();
    let surface_root = owned_root.join("surfaces");
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));
    let saver = Arc::new(PdfDocumentSaver::new(backend.clone()));

    cx.update(gpui_component::init);
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let close = cx.new({
        let workspace = workspace.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    cx.update(|cx| register_application_close_action(&close, cx));
    let close_for_window = close.clone();
    let content = AnyView::from(workspace.clone());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                close_for_window,
                content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });

    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            let evidence = workspace.evidence_snapshot(document_id, cx).unwrap();
            assert!(evidence.ready);
            assert_eq!(evidence.page_count, source_page_count);
            assert!(evidence.current_raster_has_spatial_variation);
            evidence.worker_pid
        })
        .unwrap();

    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the real native annotation canvas must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };

    scroll_annotation_target_into_view(cx, DOCUMENT_PEN_TOOL_ID);
    let pen_tool = cx
        .debug_bounds(DOCUMENT_PEN_TOOL_ID)
        .expect("the actual Pen control must render");
    cx.simulate_click(pen_tool.center(), Modifiers::default());
    let pen_points = [(72., 96.), (120., 144.), (180., 160.)];
    cx.simulate_mouse_down(
        to_view(pen_points[0].0, pen_points[0].1),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(pen_points[1].0, pen_points[1].1),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(pen_points[2].0, pen_points[2].1),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        to_view(pen_points[2].0, pen_points[2].1),
        MouseButton::Left,
        Modifiers::default(),
    );
    let pen_created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(pen_created.pens.len(), 1);
    let pen_id = pen_created.pens[0].id.clone();
    assert_eq!(pen_id.as_str(), "workspace:pen:1");
    assert_eq!(pen_created.pens[0].tool(), InkTool::Pen);
    assert_eq!(pen_created.pens[0].blend_mode(), BlendMode::Normal);
    assert!(pen_created.pens[0].smooth_curves);
    assert_eq!(pen_created.pens[0].appearance.color(), "#ff0000");
    assert_eq!(pen_created.pens[0].appearance.width_pt(), 1.);
    assert_eq!(pen_created.pens[0].appearance.opacity(), 1.);

    scroll_annotation_target_into_view(cx, DOCUMENT_INK_PROPERTIES_ID);
    let pen_properties = cx
        .debug_bounds(DOCUMENT_INK_PROPERTIES_ID)
        .expect("the exact selected Pen must expose the real Properties trigger");
    cx.simulate_click(pen_properties.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for id in [
        INK_INSPECTOR_LOCKED_ID,
        INK_INSPECTOR_COLOR_ID,
        INK_INSPECTOR_WIDTH_ID,
        INK_INSPECTOR_OPACITY_ID,
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "the real Pen inspector must render {id}"
        );
    }
    edit_selected_ink_through_real_controls(cx, &workspace, document_id, "#123456", "2 . 5", 0.6);
    let edited_pen = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        edited_pen.pens[0].appearance,
        PenAppearance::new("#123456", 2.5, 0.6).unwrap()
    );
    assert!(edited_pen.pens[0].locked);
    assert_eq!((edited_pen.revision, edited_pen.undo_depth), (5, 5));
    scroll_annotation_target_into_view(cx, DOCUMENT_INK_PROPERTIES_ID);
    let close_pen_properties = cx
        .debug_bounds(DOCUMENT_INK_PROPERTIES_ID)
        .expect("the Pen Properties trigger must remain available while its panel is open");
    cx.simulate_click(close_pen_properties.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, DOCUMENT_HIGHLIGHT_TOOL_ID);
    let highlight_tool = cx
        .debug_bounds(DOCUMENT_HIGHLIGHT_TOOL_ID)
        .expect("the actual Highlight control must render");
    cx.simulate_click(highlight_tool.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Highlight),
        "opening Highlight settings must select Highlight",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID)
            .is_some(),
        "the Highlight settings must open from its GPUI Component trigger",
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID)
            .is_none(),
        "Escape must dismiss the Highlight settings",
    );
    let highlight_points = [(72., 240.), (144., 252.), (240., 244.)];
    cx.simulate_mouse_down(
        to_view(highlight_points[0].0, highlight_points[0].1),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(highlight_points[1].0, highlight_points[1].1),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(highlight_points[2].0, highlight_points[2].1),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        to_view(highlight_points[2].0, highlight_points[2].1),
        MouseButton::Left,
        Modifiers::default(),
    );
    let highlighted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(highlighted.pens.len(), 2);
    let highlight_id = highlighted.pens[1].id.clone();
    assert_eq!(highlight_id.as_str(), "workspace:highlight:2");
    assert_eq!(highlighted.pens[1].tool(), InkTool::Highlight);
    assert_eq!(highlighted.pens[1].blend_mode(), BlendMode::Multiply);
    assert!(!highlighted.pens[1].smooth_curves);
    assert_eq!(highlighted.pens[1].appearance.color(), "#ffff00");
    assert_eq!(highlighted.pens[1].appearance.width_pt(), 12.);
    assert_eq!(highlighted.pens[1].appearance.opacity(), 1.);
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .highlight_composite_evidence(document_id, cx))
            .unwrap()
            .current_page_pixels
            > 0
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let highlight_properties = cx
        .debug_bounds(DOCUMENT_INK_PROPERTIES_ID)
        .expect("the exact selected Highlight must expose the controlled Properties trigger");
    cx.simulate_click(highlight_properties.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for id in [
        INK_INSPECTOR_LOCKED_ID,
        INK_INSPECTOR_COLOR_ID,
        INK_INSPECTOR_WIDTH_ID,
        INK_INSPECTOR_OPACITY_ID,
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "the real Highlight inspector must render {id}"
        );
    }
    edit_selected_ink_through_real_controls(cx, &workspace, document_id, "#00ff00", "1 8", 0.5);
    let edited_highlight = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        edited_highlight.pens[1].appearance,
        PenAppearance::new("#00ff00", 18., 0.5).unwrap()
    );
    assert!(edited_highlight.pens[1].locked);
    assert_eq!(
        (edited_highlight.revision, edited_highlight.undo_depth),
        (10, 10)
    );

    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_UNDO_ID);
    let undo = cx
        .debug_bounds(DOCUMENT_ANNOTATION_UNDO_ID)
        .expect("the actual Undo control must render");
    cx.simulate_click(undo.center(), Modifiers::default());
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(undone.pens.len(), 2);
    assert_eq!(undone.pens[0].id, pen_id);
    assert!(
        !undone.pens[1].locked,
        "Undo must revert exactly the lock edit"
    );
    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_REDO_ID);
    let redo = cx
        .debug_bounds(DOCUMENT_ANNOTATION_REDO_ID)
        .expect("the actual Redo control must render");
    cx.simulate_click(redo.center(), Modifiers::default());
    let redone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(redone.pens.len(), 2);
    assert_eq!(redone.pens[0].id, pen_id);
    assert_eq!(redone.pens[1].id, highlight_id);
    assert!(
        redone.pens[1].locked,
        "Redo must restore exactly the lock edit"
    );
    assert!(redone.dirty);

    let mut fresh_app = cx.cx.clone();
    cx.dispatch_action(RequestApplicationClose);
    cx.run_until_parked();
    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(!Path::new(&format!("/proc/{original_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    assert_eq!(std::fs::read(&public_source).unwrap(), public_bytes);

    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    let persisted = PdfPersistenceSession::open(&owned_source).unwrap();
    assert_eq!(persisted.page_count(), source_page_count);
    assert_eq!(
        persisted.untouched_annotations(),
        source_untouched_annotations.as_slice()
    );
    assert_eq!(
        persisted.pens().len(),
        2,
        "the saved PDF must contain exactly the created Pen and Highlight"
    );
    let persisted_pen = persisted
        .pens()
        .iter()
        .find(|pen| pen.id == pen_id)
        .expect("the Pen stable identity must persist");
    let persisted_highlight = persisted
        .pens()
        .iter()
        .find(|pen| pen.id == highlight_id)
        .expect("the Highlight stable identity must persist");
    assert_eq!(persisted_pen.tool(), InkTool::Pen);
    assert_eq!(persisted_pen.blend_mode(), BlendMode::Normal);
    assert!(persisted_pen.smooth_curves);
    assert_eq!(persisted_pen.appearance.color(), "#123456");
    assert_eq!(persisted_pen.appearance.width_pt(), 2.5);
    assert_eq!(persisted_pen.appearance.opacity(), 0.6);
    assert!(persisted_pen.locked);
    assert_eq!(persisted_highlight.tool(), InkTool::Highlight);
    assert_eq!(persisted_highlight.blend_mode(), BlendMode::Multiply);
    assert!(!persisted_highlight.smooth_curves);
    assert_eq!(persisted_highlight.appearance.color(), "#00ff00");
    assert_eq!(persisted_highlight.appearance.width_pt(), 18.);
    assert_eq!(persisted_highlight.appearance.opacity(), 0.5);
    assert!(persisted_highlight.locked);
    for (actual, expected) in persisted_pen.points().iter().zip(pen_points) {
        assert!((actual.x - expected.0).abs() < 0.001);
        assert!((actual.y - expected.1).abs() < 0.001);
    }
    for (actual, expected) in persisted_highlight.points().iter().zip(highlight_points) {
        assert!((actual.x - expected.0).abs() < 0.001);
        assert!((actual.y - expected.1).abs() < 0.001);
    }

    let rendered = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(91),
            generation: 1,
            path: owned_source.clone(),
        })
        .expect("the saved PDF must reopen through the real PDFium worker");
    let annotated_pixels = rendered
        .render_page_with_pdf_annotations(0, 320)
        .expect("PDFium must rasterize persisted Pen and Highlight appearances");
    let base_pixels = rendered
        .render_page(0, 320)
        .expect("PDFium must retain the annotation-free page content");
    assert!(base_pixels.has_spatial_variation());
    assert_ne!(annotated_pixels.pixels_bgra(), base_pixels.pixels_bgra());
    let render_worker_pid = rendered.worker_pid().unwrap();
    rendered.close().unwrap();
    assert!(!Path::new(&format!("/proc/{render_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());

    let fresh_workspace = fresh_app.update(|cx| {
        gpui_component::init(cx);
        cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx))
    });
    let fresh_content = AnyView::from(fresh_workspace.clone());
    let (_, fresh_cx) =
        fresh_app.add_window_view(move |window, cx| Root::new(fresh_content, window, cx));
    let reopened_id = fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    fresh_cx.run_until_parked();
    let reopened_worker_pid = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            let snapshot = workspace.annotation_snapshot(reopened_id, cx).unwrap();
            assert!(!snapshot.dirty);
            assert_eq!(
                snapshot.pens.len(),
                2,
                "the fresh workspace must reopen exactly the saved Pen and Highlight"
            );
            let pen = snapshot
                .pens
                .iter()
                .find(|pen| pen.id == pen_id)
                .expect("the fresh workspace must import the Pen stable identity");
            let highlight = snapshot
                .pens
                .iter()
                .find(|pen| pen.id == highlight_id)
                .expect("the fresh workspace must import the Highlight stable identity");
            assert_eq!(pen.tool(), InkTool::Pen);
            assert_eq!(pen.blend_mode(), BlendMode::Normal);
            assert_eq!(
                pen.appearance,
                PenAppearance::new("#123456", 2.5, 0.6).unwrap()
            );
            assert!(pen.locked);
            assert_eq!(highlight.tool(), InkTool::Highlight);
            assert_eq!(highlight.blend_mode(), BlendMode::Multiply);
            assert_eq!(
                highlight.appearance,
                PenAppearance::new("#00ff00", 18., 0.5).unwrap()
            );
            assert!(highlight.locked);
            assert!(
                workspace
                    .highlight_composite_evidence(reopened_id, cx)
                    .unwrap()
                    .current_page_pixels
                    > 0
            );
            workspace
                .session(reopened_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
        })
        .unwrap();
    assert!(fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.close_document(reopened_id, cx)
    }));
    assert!(!Path::new(&format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    std::fs::remove_dir_all(owned_root).unwrap();
}

fn assert_real_text_box_alignment_rasters(
    backend: &Arc<PdfiumWorkerBackend>,
    public_source: &Path,
    owned_root: &Path,
    surface_root: &Path,
) {
    let proof_rect = PdfRect::new(300., 500., 160., 70.).unwrap();
    let proof_content = "iiii\nWWWW";
    let mut proof_bounds = Vec::new();
    let mut proof_base_pixels: Option<Vec<u8>> = None;
    for (index, alignment) in [
        TextAlignment::Left,
        TextAlignment::Center,
        TextAlignment::Right,
    ]
    .into_iter()
    .enumerate()
    {
        let proof_path = owned_root.join(format!("alignment-{index}.pdf"));
        let proof_id = MarkupId::new("real:text-box-alignment-proof").unwrap();
        let mut proof_session = PdfPersistenceSession::open(public_source).unwrap();
        proof_session
            .add_text_box(
                TextBoxAnnotation::new(
                    proof_id.clone(),
                    0,
                    proof_rect,
                    proof_content,
                    TextBoxStyle::new("Helvetica", 16., "#111827", 1.)
                        .unwrap()
                        .with_weight_and_alignment(400, alignment)
                        .unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let proof_authority =
            SaveAsTargetAuthority::bind(proof_path.clone(), proof_session.source_path()).unwrap();
        proof_session
            .prepare_save_authorized(&proof_authority)
            .unwrap()
            .publish()
            .unwrap();
        let proof_reopen = PdfPersistenceSession::open(&proof_path).unwrap();
        assert_eq!(proof_reopen.text_boxes().len(), 1);
        assert_eq!(proof_reopen.text_boxes()[0].id, proof_id);
        assert_eq!(proof_reopen.text_boxes()[0].content(), proof_content);
        assert_eq!(proof_reopen.text_boxes()[0].style().alignment(), alignment);

        let proof_render = backend
            .open(&OpenDocumentRequest {
                document_id: DocumentId::new(9_300 + index as u64),
                generation: 1,
                path: proof_path,
            })
            .unwrap();
        let annotated = proof_render
            .render_page_with_pdf_annotations(0, 640)
            .unwrap();
        let base = proof_render.render_page(0, 640).unwrap();
        if let Some(expected) = &proof_base_pixels {
            assert_eq!(base.pixels_bgra(), expected.as_slice());
        } else {
            proof_base_pixels = Some(base.pixels_bgra().to_vec());
        }
        let pixels_per_pdf_x = annotated.width() as f64 / 612.;
        let pixels_per_pdf_y = annotated.height() as f64 / 792.;
        let x_start = (proof_rect.x * pixels_per_pdf_x).floor().max(0.) as u32;
        let x_end = ((proof_rect.x + proof_rect.width) * pixels_per_pdf_x)
            .ceil()
            .min(annotated.width() as f64) as u32;
        let line_height = 16. * 1.15;
        let first_baseline = proof_rect.height - 2. - 16.;
        let mut line_bounds = Vec::new();
        for line_index in 0..2 {
            let baseline = proof_rect.y + first_baseline - line_index as f64 * line_height;
            let pdf_low = baseline - 2.;
            let pdf_high = baseline + 14.;
            let y_start = ((792. - pdf_high) * pixels_per_pdf_y).floor().max(0.) as u32;
            let y_end = ((792. - pdf_low) * pixels_per_pdf_y)
                .ceil()
                .min(annotated.height() as f64) as u32;
            let mut min_x = u32::MAX;
            let mut max_x = 0;
            for y in y_start..y_end {
                for x in x_start..x_end {
                    let offset = ((y * annotated.width() + x) * 4) as usize;
                    if annotated.pixels_bgra()[offset..offset + 4]
                        != base.pixels_bgra()[offset..offset + 4]
                    {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                    }
                }
            }
            assert_ne!(min_x, u32::MAX, "alignment {alignment:?} line {line_index}");
            line_bounds.push((min_x, max_x));
        }
        proof_bounds.push(line_bounds);
        let worker_pid = proof_render.worker_pid().unwrap();
        proof_render.close().unwrap();
        assert!(!Path::new(&format!("/proc/{worker_pid}")).exists());
    }
    let left = &proof_bounds[0];
    let center = &proof_bounds[1];
    let right = &proof_bounds[2];
    assert!(left[0].0.abs_diff(left[1].0) <= 2, "left edges: {left:?}");
    assert!(
        (center[0].0 + center[0].1).abs_diff(center[1].0 + center[1].1) <= 3,
        "centers: {center:?}"
    );
    assert!(
        right[0].1.abs_diff(right[1].1) <= 2,
        "right edges: {right:?}"
    );
    assert!(left[0].0 < center[0].0 && center[0].0 < right[0].0);
    assert!(!surface_root.exists() || std::fs::read_dir(surface_root).unwrap().next().is_none());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_native_shell_text_box_create_type_escape_save_close_and_fresh_reopen(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().unwrap();
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let public_source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(public_source.is_file());
    let public_bytes = std::fs::read(&public_source).unwrap();
    let source_persistence = PdfPersistenceSession::open(&public_source).unwrap();
    let source_page_count = source_persistence.page_count();
    let source_untouched_annotations = source_persistence.untouched_annotations().to_vec();

    let owned_root = manifest_dir
        .join(".prepared/native-shell-text-box-real")
        .join(std::process::id().to_string());
    std::fs::remove_dir_all(&owned_root).ok();
    std::fs::create_dir_all(&owned_root).unwrap();
    let owned_source = owned_root.join("edited.pdf");
    std::fs::copy(&public_source, &owned_source).unwrap();
    let surface_root = owned_root.join("surfaces");
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));
    let original_base = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_200),
            generation: 1,
            path: owned_source.clone(),
        })
        .expect("the annotation-free source must open for the base-raster proof");
    let original_base_digest = Sha256::digest(
        original_base
            .render_page(0, 320)
            .expect("the annotation-free source page must rasterize")
            .pixels_bgra(),
    )
    .to_vec();
    original_base.close().unwrap();
    let saver = Arc::new(PdfDocumentSaver::new(backend.clone()));

    cx.update(gpui_component::init);
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let close = cx.new({
        let workspace = workspace.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    cx.update(|cx| register_application_close_action(&close, cx));
    let close_for_window = close.clone();
    let content = AnyView::from(workspace.clone());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                close_for_window,
                content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });

    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            let evidence = workspace.evidence_snapshot(document_id, cx).unwrap();
            assert!(evidence.ready);
            assert_eq!(evidence.page_count, source_page_count);
            assert!(evidence.current_raster_has_spatial_variation);
            evidence.worker_pid
        })
        .unwrap();

    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the real native annotation canvas must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let placement_pdf = PdfPoint::new(72., 96.).unwrap();
    let placement = point(
        origin.x + px(placement_pdf.x as f32 * render_scale),
        origin.y + px((792. - placement_pdf.y as f32) * render_scale),
    );

    scroll_annotation_target_into_view(cx, DOCUMENT_TEXT_BOX_TOOL_ID);
    let text_box_tool = cx
        .debug_bounds(DOCUMENT_TEXT_BOX_TOOL_ID)
        .expect("the actual Text Box control must render");
    cx.simulate_click(text_box_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::TextBox),
    );
    cx.simulate_mouse_down(placement, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("canvas placement must open the real multiline Text Box editor");
    let return_focus = workspace.read_with(cx, |workspace, _| workspace.text_box_return_focus());
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));
    let pending = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(pending.text_boxes.is_empty());
    assert_eq!((pending.revision, pending.undo_depth), (0, 0));

    let expected_content = "native text\nlevel 2";
    cx.simulate_keystrokes("n a t i v e space t e x t enter l e v e l space 2");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some(expected_content.to_owned()),
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_none());
    assert!(!cx.update(|window, _| editor_focus.is_focused(window)));
    assert!(cx.update(|window, _| return_focus.is_focused(window)));
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.text_box_commit_error().is_none()
    }));
    let committed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(committed.text_boxes.len(), 1);
    assert_eq!((committed.revision, committed.undo_depth), (1, 1));
    assert_eq!(committed.redo_depth, 0);
    assert!(committed.dirty);
    let created_text_box = committed.text_boxes[0].clone();
    assert_eq!(created_text_box.id.as_str(), "workspace:text:1");
    assert_eq!(created_text_box.page_index, 0);
    assert_eq!(created_text_box.content(), expected_content);
    assert!((created_text_box.layout_rect.x - 66.5).abs() < 0.001);
    assert!((created_text_box.layout_rect.y - 73.2).abs() < 0.001);
    assert!((created_text_box.layout_rect.width - 89.2).abs() < 0.001);
    assert!((created_text_box.layout_rect.height - 31.8).abs() < 0.001);
    assert_eq!(created_text_box.style().font_family(), "Helvetica");
    assert_eq!(created_text_box.style().font_size_pt(), 12.);
    assert_eq!(created_text_box.style().color(), "#ff0000");
    assert_eq!(created_text_box.style().opacity(), 1.);
    assert_eq!(created_text_box.style().weight(), 400);

    let to_view = |pdf: PdfPoint| {
        point(
            origin.x + px(pdf.x as f32 * render_scale),
            origin.y + px((792. - pdf.y as f32) * render_scale),
        )
    };
    let move_start = PdfPoint::new(
        created_text_box.layout_rect.x + created_text_box.layout_rect.width * 0.5,
        created_text_box.layout_rect.y + created_text_box.layout_rect.height * 0.5,
    )
    .unwrap();
    let move_end = PdfPoint::new(move_start.x + 12., move_start.y + 8.).unwrap();
    cx.simulate_mouse_down(to_view(move_start), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(move_end),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(move_end), MouseButton::Left, Modifiers::default());
    let moved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(
        (moved.text_boxes[0].layout_rect.x - (created_text_box.layout_rect.x + 12.)).abs() < 0.001
    );
    assert!(
        (moved.text_boxes[0].layout_rect.y - (created_text_box.layout_rect.y + 8.)).abs() < 0.001
    );
    assert_eq!((moved.revision, moved.undo_depth), (2, 2));

    let moved_rect = moved.text_boxes[0].layout_rect;
    let east = PdfPoint::new(
        moved_rect.x + moved_rect.width,
        moved_rect.y + moved_rect.height * 0.5,
    )
    .unwrap();
    let east_end = PdfPoint::new(east.x + 24., east.y).unwrap();
    cx.simulate_mouse_down(to_view(east), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(east_end),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(east_end), MouseButton::Left, Modifiers::default());
    let resized = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!((resized.text_boxes[0].layout_rect.width - (moved_rect.width + 24.)).abs() < 0.001);
    assert_eq!((resized.revision, resized.undo_depth), (3, 3));

    let edit_point = PdfPoint::new(
        resized.text_boxes[0].layout_rect.x + 20.,
        resized.text_boxes[0].layout_rect.y + resized.text_boxes[0].layout_rect.height * 0.5,
    )
    .unwrap();
    let double_click = |cx: &mut gpui::VisualTestContext| {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: to_view(edit_point),
            modifiers: Modifiers::default(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: to_view(edit_point),
            modifiers: Modifiers::default(),
            click_count: 2,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    double_click(cx);
    let cancel_input = workspace
        .read_with(cx, |workspace, _| workspace.pending_text_box_input())
        .expect("double-click must reopen the existing Text Box editor");
    cx.update(|window, cx| {
        cancel_input.update(cx, |input, cx| {
            input.replace_text_in_range(Some(0..expected_content.len()), "cancelled", window, cx)
        })
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(document_id, cx))
            .unwrap(),
        resized,
        "Escape must not mutate or add history for an existing Text Box",
    );

    double_click(cx);
    let edit_input = workspace
        .read_with(cx, |workspace, _| workspace.pending_text_box_input())
        .expect("the existing Text Box editor must reopen after cancellation");
    let edited_content = "世界\nlevel 2";
    cx.update(|window, cx| {
        edit_input.update(cx, |input, cx| {
            input.replace_text_in_range(Some(0..expected_content.len()), edited_content, window, cx)
        })
    });
    let canvas_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| canvas_focus.focus(window, cx));
    cx.run_until_parked();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.text_boxes[0].id, created_text_box.id);
    assert_eq!(edited.text_boxes[0].content(), edited_content);
    assert_eq!(edited.text_boxes[0].style(), created_text_box.style());
    assert_eq!((edited.revision, edited.undo_depth), (4, 4));
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(document_id, cx))
            .unwrap()
            .text_boxes[0]
            .content(),
        expected_content,
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(document_id, cx)
        })
        .unwrap();
    let pre_appearance_text_box = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .text_boxes[0]
        .clone();
    assert_eq!(pre_appearance_text_box.content(), edited_content);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, DOCUMENT_TEXT_BOX_PROPERTIES_ID);
    let properties = cx
        .debug_bounds(DOCUMENT_TEXT_BOX_PROPERTIES_ID)
        .expect("the selected real Text Box must expose Properties");
    cx.simulate_click(properties.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let inspector = workspace
        .read_with(cx, |workspace, _| workspace.text_box_property_inspector())
        .unwrap();
    let picker = inspector.read_with(cx, |inspector, _| inspector.color_picker());
    let color_trigger = cx
        .debug_bounds(TEXT_BOX_INSPECTOR_COLOR_TRIGGER_ID)
        .unwrap();
    cx.simulate_click(color_trigger.center(), Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let hex_input = picker.read_with(cx, |picker, _| picker.hex_input().clone());
    cx.update(|window, cx| hex_input.read(cx).focus_handle(cx).focus(window, cx));
    cx.write_to_clipboard(ClipboardItem::new_string("#2563eb66".into()));
    cx.simulate_keystrokes(&format!("{EDIT_SELECT_ALL} {EDIT_PASTE} enter"));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let apply_color = cx.debug_bounds(TEXT_BOX_INSPECTOR_APPLY_COLOR_ID).unwrap();
    cx.simulate_click(apply_color.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let size = cx.debug_bounds(TEXT_BOX_INSPECTOR_SIZE_ID).unwrap();
    cx.simulate_click(size.center(), Modifiers::default());
    cx.simulate_keystrokes(&format!("{EDIT_SELECT_ALL} 1 8 enter"));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let opacity = cx
        .debug_bounds(TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID)
        .unwrap();
    let opacity_point = point(
        opacity.origin.x + opacity.size.width * 0.6,
        opacity.center().y,
    );
    cx.simulate_mouse_down(opacity_point, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(opacity_point, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let center = cx
        .debug_bounds(TEXT_BOX_INSPECTOR_ALIGNMENT_CENTER_ID)
        .unwrap();
    cx.simulate_click(center.center(), Modifiers::default());
    let text_box = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .text_boxes[0]
        .clone();
    assert_eq!(text_box.style().font_size_pt(), 18.);
    assert_eq!(text_box.style().color(), "#2563eb");
    assert_eq!(text_box.style().opacity(), 0.6);
    assert_eq!(text_box.style().alignment(), TextAlignment::Center);
    assert_eq!(text_box.layout_rect, pre_appearance_text_box.layout_rect);
    assert_eq!(text_box.content(), pre_appearance_text_box.content());

    let mut fresh_app = cx.cx.clone();
    cx.dispatch_action(RequestApplicationClose);
    cx.run_until_parked();
    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    close.update(cx, |close, cx| {
        close.choose_and_dispatch(ApplicationCloseAction::SaveAll, cx)
    });
    wait_for_real_application_close_terminal(cx, &workspace, &close, document_id, &owned_source);
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(close.read_with(cx, |close, _| close.recovery().is_none()));
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::QuitRequested {
            kind: ApplicationCloseCompletionKind::SavedAll,
            ..
        })
    ));
    assert!(!Path::new(&format!("/proc/{original_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    assert_eq!(std::fs::read(&public_source).unwrap(), public_bytes);

    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );
    let persisted = PdfPersistenceSession::open(&owned_source).unwrap();
    assert_eq!(persisted.page_count(), source_page_count);
    assert_eq!(
        persisted.untouched_annotations(),
        source_untouched_annotations.as_slice()
    );
    let persisted_text_box = persisted
        .text_boxes()
        .iter()
        .find(|candidate| candidate.id == text_box.id)
        .expect("the Text Box stable identity must persist");
    assert_eq!(persisted.text_boxes().len(), 1);
    assert_eq!(persisted_text_box.page_index, text_box.page_index);
    assert_eq!(persisted_text_box.content(), text_box.content());
    assert!((persisted_text_box.layout_rect.x - text_box.layout_rect.x).abs() < 0.001);
    assert!((persisted_text_box.layout_rect.y - text_box.layout_rect.y).abs() < 0.001);
    assert!((persisted_text_box.layout_rect.width - text_box.layout_rect.width).abs() < 0.001);
    assert!((persisted_text_box.layout_rect.height - text_box.layout_rect.height).abs() < 0.001);
    assert_eq!(persisted_text_box.style(), text_box.style());

    let rendered = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(92),
            generation: 1,
            path: owned_source.clone(),
        })
        .expect("the saved PDF must reopen through the real PDFium worker");
    let annotated_pixels = rendered
        .render_page_with_pdf_annotations(0, 320)
        .expect("PDFium must rasterize the persisted Text Box appearance");
    let base_pixels = rendered
        .render_page(0, 320)
        .expect("PDFium must retain the annotation-free page content");
    assert!(base_pixels.has_spatial_variation());
    assert_eq!(
        Sha256::digest(base_pixels.pixels_bgra()).to_vec(),
        original_base_digest,
        "Save must preserve the exact annotation-free base-page raster",
    );
    assert_ne!(annotated_pixels.pixels_bgra(), base_pixels.pixels_bgra());
    let render_worker_pid = rendered.worker_pid().unwrap();
    rendered.close().unwrap();
    assert!(!Path::new(&format!("/proc/{render_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    assert_real_text_box_alignment_rasters(&backend, &public_source, &owned_root, &surface_root);

    let fresh_workspace = fresh_app.update(|cx| {
        gpui_component::init(cx);
        cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx))
    });
    let fresh_content = AnyView::from(fresh_workspace.clone());
    let (_, fresh_cx) =
        fresh_app.add_window_view(move |window, cx| Root::new(fresh_content, window, cx));
    let reopened_id = fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    fresh_cx.run_until_parked();
    let reopened_worker_pid = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            let snapshot = workspace.annotation_snapshot(reopened_id, cx).unwrap();
            assert!(!snapshot.dirty);
            let reopened = snapshot
                .text_boxes
                .iter()
                .find(|candidate| candidate.id == text_box.id)
                .expect("the fresh workspace must import the Text Box stable identity");
            assert_eq!(snapshot.text_boxes.len(), 1);
            assert_eq!(reopened.page_index, text_box.page_index);
            assert_eq!(reopened.content(), text_box.content());
            assert!((reopened.layout_rect.x - text_box.layout_rect.x).abs() < 0.001);
            assert!((reopened.layout_rect.y - text_box.layout_rect.y).abs() < 0.001);
            assert!((reopened.layout_rect.width - text_box.layout_rect.width).abs() < 0.001);
            assert!((reopened.layout_rect.height - text_box.layout_rect.height).abs() < 0.001);
            assert_eq!(reopened.style(), text_box.style());
            workspace
                .session(reopened_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
        })
        .unwrap();
    let reopened_base = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_201),
            generation: 1,
            path: owned_source.clone(),
        })
        .expect("the freshly reopened PDF must support the base-raster proof");
    assert_eq!(
        Sha256::digest(reopened_base.render_page(0, 320).unwrap().pixels_bgra()).to_vec(),
        original_base_digest,
        "fresh reopen must preserve the exact annotation-free base-page raster",
    );
    reopened_base.close().unwrap();
    assert!(fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.close_document(reopened_id, cx)
    }));
    assert!(!Path::new(&format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    std::fs::remove_dir_all(owned_root).unwrap();
}

#[gpui::test]
fn application_shell_edit_shortcuts_use_production_workspace_action_bootstrap_and_focused_context(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let backend = Arc::new(DeterministicBackend::default());
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let close = cx.new({
        let workspace = workspace.clone();
        let saver = backend.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    let close_for_window = close.clone();
    let content = AnyView::from(workspace.clone());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                close_for_window,
                content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });

    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("clipboard-bootstrap.pdf"), cx)
    });
    let annotation = |id: &str, page_index, y| {
        StraightLineAnnotation::new(
            MarkupId::new(id).unwrap(),
            page_index,
            PdfPoint::new(100., y).unwrap(),
            PdfPoint::new(200., y).unwrap(),
            LineKind::Arrow,
            StraightLineAppearance::default_for(LineKind::Arrow),
        )
        .unwrap()
    };
    let source = annotation("clipboard:arrow:source", 0, 200.);
    let mut locked = annotation("clipboard:arrow:locked", 0, 300.);
    locked.locked = true;
    let other_page = annotation("clipboard:arrow:other-page", 1, 400.);
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &request,
            Ok(backend
                .opened_with_page_count("clipboard-bootstrap.pdf", 2)
                .with_annotations(vec![
                    Annotation::StraightLine(source.clone()),
                    Annotation::StraightLine(locked.clone()),
                    Annotation::StraightLine(other_page.clone()),
                ],)),
            cx,
        )),
        ApplyDisposition::Applied,
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let initial = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (initial.revision, initial.undo_depth, initial.redo_depth),
        (0, 0, 0)
    );
    assert!(!initial.dirty);
    assert_eq!(
        initial.annotation_order,
        vec![source.id.clone(), locked.id.clone(), other_page.id.clone()]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.document_edit_capabilities(cx)),
        DocumentEditCapabilities {
            can_select_all: true,
            ..Default::default()
        }
    );
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));

    cx.simulate_keystrokes(EDIT_SELECT_ALL);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.selected_annotation_ids(request.document_id, cx)
        }),
        vec![source.id.clone(), locked.id.clone()]
    );
    let selected_capabilities =
        workspace.read_with(cx, |workspace, cx| workspace.document_edit_capabilities(cx));
    assert!(selected_capabilities.can_cut);
    assert!(selected_capabilities.can_copy);
    assert!(selected_capabilities.can_delete);
    assert!(!selected_capabilities.can_paste);

    cx.simulate_keystrokes(EDIT_COPY);
    let copied = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (copied.revision, copied.undo_depth, copied.redo_depth),
        (0, 0, 0)
    );
    assert_eq!(
        copied.straight_lines,
        vec![source.clone(), locked.clone(), other_page.clone()]
    );
    assert_eq!(copied.selected_id.as_ref(), Some(&source.id));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.document_edit_capabilities(cx).can_paste
    }));

    cx.simulate_keystrokes(EDIT_PASTE);
    let pasted_source_id = MarkupId::new("workspace:paste:arrow:1").unwrap();
    let pasted_locked_id = MarkupId::new("workspace:paste:arrow:2").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.selected_annotation_ids(request.document_id, cx)
        }),
        vec![pasted_source_id.clone(), pasted_locked_id.clone()]
    );
    let pasted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (pasted.revision, pasted.undo_depth, pasted.redo_depth),
        (1, 1, 0)
    );
    assert!(pasted.dirty);
    assert_eq!(
        pasted.annotation_order,
        vec![
            source.id.clone(),
            locked.id.clone(),
            other_page.id.clone(),
            pasted_source_id.clone(),
            pasted_locked_id.clone(),
        ]
    );
    assert_eq!(pasted.straight_lines[0], source);
    let duplicate = pasted
        .straight_lines
        .iter()
        .find(|annotation| annotation.id == pasted_source_id)
        .unwrap();
    assert_eq!(duplicate.page_index, 0);
    assert_eq!(duplicate.start, PdfPoint::new(112., 188.).unwrap());
    assert_eq!(duplicate.end, PdfPoint::new(212., 188.).unwrap());
    assert_eq!(duplicate.kind, LineKind::Arrow);
    assert_eq!(duplicate.appearance, pasted.straight_lines[0].appearance);
    assert_eq!(duplicate.locked, pasted.straight_lines[0].locked);
    assert!(
        pasted
            .straight_lines
            .iter()
            .find(|annotation| annotation.id == pasted_locked_id)
            .unwrap()
            .locked
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.document_edit_capabilities(cx).can_undo
    }));

    cx.simulate_keystrokes(EDIT_UNDO);
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (undone.revision, undone.undo_depth, undone.redo_depth),
        (0, 0, 1)
    );
    assert!(!undone.dirty);
    assert_eq!(
        undone.annotation_order,
        vec![source.id.clone(), locked.id.clone(), other_page.id.clone()]
    );
    assert_eq!(undone.selected_id, None);
    let undone_capabilities =
        workspace.read_with(cx, |workspace, cx| workspace.document_edit_capabilities(cx));
    assert!(undone_capabilities.can_redo);
    assert!(undone_capabilities.can_paste);
    assert!(!undone_capabilities.can_copy);

    cx.simulate_keystrokes(EDIT_REDO);
    let redone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (redone.revision, redone.undo_depth, redone.redo_depth),
        (1, 1, 0)
    );
    assert!(redone.dirty);
    assert_eq!(
        redone.annotation_order,
        vec![
            source.id.clone(),
            locked.id.clone(),
            other_page.id.clone(),
            pasted_source_id,
            pasted_locked_id,
        ]
    );
    assert_eq!(redone.selected_id, None);
    assert_eq!(
        redone
            .straight_lines
            .iter()
            .find(|annotation| annotation.id == duplicate.id)
            .unwrap(),
        duplicate
    );

    cx.simulate_keystrokes(EDIT_UNDO);
    cx.simulate_keystrokes(EDIT_SELECT_ALL);
    cx.simulate_keystrokes(EDIT_CUT);
    let cut = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((cut.undo_depth, cut.redo_depth), (1, 0));
    assert_eq!(
        cut.annotation_order,
        vec![locked.id.clone(), other_page.id.clone()]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.selected_annotation_ids(request.document_id, cx)
        }),
        vec![locked.id.clone()]
    );
    let cut_capabilities =
        workspace.read_with(cx, |workspace, cx| workspace.document_edit_capabilities(cx));
    assert!(cut_capabilities.can_cut);
    assert!(cut_capabilities.can_copy);
    assert!(cut_capabilities.can_paste);
    assert!(!cut_capabilities.can_delete);

    cx.simulate_keystrokes("delete");
    let after_locked_delete = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(after_locked_delete, cut);

    scroll_annotation_target_into_view(cx, DOCUMENT_TEXT_BOX_TOOL_ID);
    let text_box_tool = cx
        .debug_bounds(DOCUMENT_TEXT_BOX_TOOL_ID)
        .expect("the actual Text Box control must render");
    cx.simulate_click(text_box_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the deterministic annotation canvas must render");
    cx.simulate_mouse_down(layer.center(), MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("Text Box placement must focus the real gpui-component Textarea");
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));

    cx.simulate_keystrokes("a l p h a");
    cx.simulate_keystrokes(EDIT_SELECT_ALL);
    cx.simulate_keystrokes(EDIT_CUT);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some(String::new())
    );
    cx.simulate_keystrokes(EDIT_PASTE);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("alpha".into())
    );
    cx.simulate_keystrokes(EDIT_UNDO);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some(String::new())
    );
    cx.simulate_keystrokes(EDIT_REDO);
    cx.simulate_keystrokes(EDIT_SELECT_ALL);
    cx.simulate_keystrokes("delete");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some(String::new())
    );
    let after_text_actions = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(after_text_actions, cut);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.selected_annotation_ids(request.document_id, cx)
        }),
        vec![locked.id.clone()]
    );

    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| focus.focus(window, cx));
    cx.simulate_keystrokes(EDIT_PASTE);
    let clipboard_paste = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    let clipboard_source_id = MarkupId::new("workspace:paste:arrow:4").unwrap();
    let clipboard_locked_id = MarkupId::new("workspace:paste:arrow:5").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.selected_annotation_ids(request.document_id, cx)
        }),
        vec![clipboard_source_id.clone(), clipboard_locked_id.clone()]
    );
    assert_eq!(
        (clipboard_paste.undo_depth, clipboard_paste.redo_depth),
        (2, 0)
    );
    assert!(clipboard_paste.annotation_order.contains(&locked.id));
    assert!(clipboard_paste.annotation_order.contains(&other_page.id));
    assert!(
        clipboard_paste
            .annotation_order
            .contains(&clipboard_source_id)
    );
    assert!(
        clipboard_paste
            .annotation_order
            .contains(&clipboard_locked_id)
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_native_shell_all_eight_families_edit_history_save_save_as_close_and_two_reopens(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().unwrap();
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let public_source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let public_image =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-image-checker-v1.png");
    assert!(worker.is_file() && library.is_file());
    let public_source_bytes = std::fs::read(&public_source).unwrap();
    let public_image_bytes = std::fs::read(&public_image).unwrap();
    let public_source_sha256 = format!("{:x}", Sha256::digest(&public_source_bytes));
    let public_image_sha256 = format!("{:x}", Sha256::digest(&public_image_bytes));
    assert_eq!(
        public_source_sha256,
        "4a0a94cdbcc08e7ee06504914e5b84d218f2aeb01035b42d62f2275e38d02cbd"
    );
    assert_eq!(
        public_image_sha256,
        "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda"
    );
    let source_persistence = PdfPersistenceSession::open(&public_source).unwrap();
    let source_page_count = source_persistence.page_count();
    let source_untouched = source_persistence.untouched_annotations().to_vec();
    assert!(source_untouched.iter().any(|item| item.name == "unknown-1"));

    let owned_root = manifest_dir
        .join(".prepared/native-shell-core-editor-real")
        .join(std::process::id().to_string());
    let mut scratch_guard =
        NativeShellPidScratchGuard::new(owned_root.clone(), worker.canonicalize().unwrap());
    let owned_source = owned_root.join("edited.pdf");
    let saved_as = owned_root.join("saved-as.pdf");
    let owned_image = owned_root.join("checker.png");
    assert!(!saved_as.exists());
    std::fs::copy(&public_source, &owned_source).unwrap();
    std::fs::copy(&public_image, &owned_image).unwrap();
    let surface_root = owned_root.join("surfaces");
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));
    let saver = Arc::new(PdfDocumentSaver::new(backend.clone()));

    let base_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_086),
            generation: 1,
            path: owned_source.clone(),
        })
        .unwrap();
    let original_base_digest =
        Sha256::digest(base_proof.render_page(0, 320).unwrap().pixels_bgra()).to_vec();
    let original_annotated_digest = Sha256::digest(
        base_proof
            .render_page_with_pdf_annotations(0, 320)
            .unwrap()
            .pixels_bgra(),
    )
    .to_vec();
    assert_ne!(original_annotated_digest, original_base_digest);
    let base_worker_pid = base_proof.worker_pid().unwrap();
    scratch_guard.track_worker(base_worker_pid);
    base_proof.close().unwrap();
    assert!(!Path::new(&format!("/proc/{base_worker_pid}")).exists());

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace = cx.new({
        let backend = backend.clone();
        move |cx| DocumentWorkspace::with_opener(backend, cx)
    });
    let close = cx.new({
        let workspace = workspace.clone();
        move |_| ApplicationCloseWorkspace::new(workspace, saver)
    });
    cx.update(|cx| register_application_close_action(&close, cx));
    let close_for_window = close.clone();
    let content = AnyView::from(workspace.clone());
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let shell = cx.new(|cx| {
            ApplicationCloseShell::new_for_native_window_with_content(
                close_for_window,
                content,
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });
    let document_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            let evidence = workspace.evidence_snapshot(document_id, cx).unwrap();
            assert!(evidence.ready && evidence.current_raster_has_spatial_variation);
            assert_eq!(evidence.page_count as usize, source_page_count);
            evidence.worker_pid
        })
        .unwrap();
    scratch_guard.track_worker(original_worker_pid);
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the product canvas must render");
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view = |x: f64, y: f64| {
        point(
            origin.x + px(x as f32 * scale),
            origin.y + px((792. - y as f32) * scale),
        )
    };

    scroll_annotation_target_into_view(cx, DOCUMENT_RECTANGLE_TOOL_ID);
    let rectangle_tool_point = cx
        .debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID)
        .unwrap()
        .center();
    cx.simulate_click(rectangle_tool_point, Modifiers::default());
    cx.simulate_mouse_down(to_view(390., 580.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(500., 660.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(500., 660.), MouseButton::Left, Modifiers::default());
    let rectangle_id = MarkupId::new("workspace:rectangle:1").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_ELLIPSE_TOOL_ID);
    let ellipse_tool_point = cx.debug_bounds(DOCUMENT_ELLIPSE_TOOL_ID).unwrap().center();
    cx.simulate_click(ellipse_tool_point, Modifiers::default());
    cx.simulate_mouse_down(to_view(72., 580.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(150., 650.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(150., 650.), MouseButton::Left, Modifiers::default());
    let ellipse_id = MarkupId::new("workspace:ellipse:2").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_LINE_TOOL_ID);
    let line_tool_point = cx.debug_bounds(DOCUMENT_LINE_TOOL_ID).unwrap().center();
    cx.simulate_click(line_tool_point, Modifiers::default());
    cx.simulate_mouse_down(to_view(180., 520.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(310., 500.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(310., 500.), MouseButton::Left, Modifiers::default());
    let line_id = MarkupId::new("workspace:line:3").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_ARROW_TOOL_ID);
    let arrow_tool_point = cx.debug_bounds(DOCUMENT_ARROW_TOOL_ID).unwrap().center();
    cx.simulate_click(arrow_tool_point, Modifiers::default());
    cx.simulate_mouse_down(to_view(180., 410.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(300., 430.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(300., 430.), MouseButton::Left, Modifiers::default());
    let arrow_id = MarkupId::new("workspace:arrow:4").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_PEN_TOOL_ID);
    let pen_tool_point = cx.debug_bounds(DOCUMENT_PEN_TOOL_ID).unwrap().center();
    cx.simulate_click(pen_tool_point, Modifiers::default());
    cx.simulate_mouse_down(to_view(180., 700.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(250., 720.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(330., 690.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(330., 690.), MouseButton::Left, Modifiers::default());
    let pen_id = MarkupId::new("workspace:pen:5").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_HIGHLIGHT_TOOL_ID);
    let highlight_tool_point = cx
        .debug_bounds(DOCUMENT_HIGHLIGHT_TOOL_ID)
        .unwrap()
        .center();
    cx.simulate_click(highlight_tool_point, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID)
            .is_some()
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID)
            .is_none()
    );
    cx.simulate_mouse_down(to_view(180., 600.), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(250., 608.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(330., 596.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(330., 596.), MouseButton::Left, Modifiers::default());
    let highlight_id = MarkupId::new("workspace:highlight:6").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_TEXT_BOX_TOOL_ID);
    let text_tool_point = cx.debug_bounds(DOCUMENT_TEXT_BOX_TOOL_ID).unwrap().center();
    cx.simulate_click(text_tool_point, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_down(to_view(350., 520.), MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let text_editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("Text Box placement must focus the real editor");
    let text_return_focus =
        workspace.read_with(cx, |workspace, _| workspace.text_box_return_focus());
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    assert!(cx.update(|window, _| text_editor_focus.is_focused(window)));
    cx.simulate_keystrokes("n a t i v e space t e x t enter l e v e l space 2");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("native text\nlevel 2".to_owned()),
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_none());
    assert!(!cx.update(|window, _| text_editor_focus.is_focused(window)));
    assert!(cx.update(|window, _| text_return_focus.is_focused(window)));
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.text_box_commit_error().is_none()
    }));
    let text_id = MarkupId::new("workspace:text:7").unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_IMAGE_TOOL_ID);
    let image_tool_point = cx.debug_bounds(DOCUMENT_IMAGE_TOOL_ID).unwrap().center();
    cx.simulate_click(image_tool_point, Modifiers::default());
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response({
        let owned_image = owned_image.clone();
        move |options| {
            assert!(options.files && !options.directories && !options.multiple);
            Some(vec![owned_image])
        }
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_click(to_view(450., 180.), Modifiers::default());
    let image_id = MarkupId::new("workspace:image:8").unwrap();
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let mut expected_order = source_persistence.annotation_order().to_vec();
    expected_order.extend([
        rectangle_id.clone(),
        ellipse_id.clone(),
        line_id.clone(),
        arrow_id.clone(),
        pen_id.clone(),
        highlight_id.clone(),
        text_id.clone(),
        image_id.clone(),
    ]);
    assert_eq!(created.annotation_order, expected_order);
    let rectangle_before = created
        .rectangles
        .iter()
        .find(|item| item.id == rectangle_id)
        .unwrap()
        .clone();
    let ellipse_before = created
        .ellipses
        .iter()
        .find(|item| item.id == ellipse_id)
        .unwrap()
        .clone();
    assert!(
        created
            .straight_lines
            .iter()
            .any(|item| item.id == line_id && item.kind == LineKind::Line)
    );
    assert!(
        created
            .straight_lines
            .iter()
            .any(|item| item.id == arrow_id && item.kind == LineKind::Arrow)
    );
    let pen_before = created.pens.iter().find(|item| item.id == pen_id).unwrap();
    assert_eq!(pen_before.tool(), InkTool::Pen);
    assert_eq!(pen_before.blend_mode(), BlendMode::Normal);
    assert!(pen_before.smooth_curves);
    assert_eq!(pen_before.appearance.color(), "#ff0000");
    assert_eq!(pen_before.appearance.width_pt(), 1.);
    let highlight_before = created
        .pens
        .iter()
        .find(|item| item.id == highlight_id)
        .unwrap();
    assert_eq!(highlight_before.tool(), InkTool::Highlight);
    assert_eq!(highlight_before.blend_mode(), BlendMode::Multiply);
    assert!(!highlight_before.smooth_curves);
    assert_eq!(highlight_before.appearance.color(), "#ffff00");
    assert_eq!(highlight_before.appearance.width_pt(), 12.);
    let text_before = created
        .text_boxes
        .iter()
        .find(|item| item.id == text_id)
        .unwrap();
    assert_eq!(text_before.content(), "native text\nlevel 2");
    assert_eq!(text_before.style().font_family(), "Helvetica");
    assert_eq!(text_before.style().font_size_pt(), 12.);
    assert_eq!(text_before.style().color(), "#ff0000");
    let image_before = created
        .images
        .iter()
        .find(|item| item.id == image_id)
        .unwrap()
        .clone();
    let image_asset_id = image_before.asset().id().as_str().to_owned();
    let original_image_weak = workspace
        .read_with(cx, |workspace, cx| {
            workspace.image_render_asset_weak(document_id, &image_asset_id, cx)
        })
        .unwrap();

    scroll_annotation_target_into_view(cx, DOCUMENT_SELECT_TOOL_ID);
    let select_tool_point = cx.debug_bounds(DOCUMENT_SELECT_TOOL_ID).unwrap().center();
    cx.simulate_click(select_tool_point, Modifiers::default());
    let rectangle_center = PdfPoint::new(
        rectangle_before.rect.x + rectangle_before.rect.width / 2.,
        rectangle_before.rect.y + rectangle_before.rect.height / 2.,
    )
    .unwrap();
    cx.simulate_mouse_down(
        to_view(rectangle_center.x, rectangle_center.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(rectangle_center.x + 12., rectangle_center.y - 12.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        to_view(rectangle_center.x + 12., rectangle_center.y - 12.),
        MouseButton::Left,
        Modifiers::default(),
    );
    let moved_rectangle = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .rectangles
        .into_iter()
        .find(|item| item.id == rectangle_id)
        .unwrap();
    assert!((moved_rectangle.rect.x - rectangle_before.rect.x - 12.).abs() < 0.001);
    assert!((moved_rectangle.rect.y - rectangle_before.rect.y + 12.).abs() < 0.001);
    assert_eq!(moved_rectangle.rect.width, rectangle_before.rect.width);
    assert_eq!(moved_rectangle.rect.height, rectangle_before.rect.height);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rectangle_east = ellipse_resize_handle_point_for_rect(
        moved_rectangle.rect,
        moved_rectangle.rotation_degrees,
        RectangleResizeHandle::East,
    );
    cx.simulate_mouse_down(
        to_view(rectangle_east.x, rectangle_east.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(rectangle_east.x + 24., rectangle_east.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    let rectangle_preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    });
    let rectangle_preview = rectangle_preview
        .rectangles
        .iter()
        .find(|item| item.id == rectangle_id)
        .unwrap();
    assert!(rectangle_preview.preview);
    assert!((rectangle_preview.rect.width - moved_rectangle.rect.width - 24.).abs() < 0.001);
    cx.simulate_mouse_up(
        to_view(rectangle_east.x + 24., rectangle_east.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    let resized_rectangle = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .rectangles
        .into_iter()
        .find(|item| item.id == rectangle_id)
        .unwrap();
    assert!((resized_rectangle.rect.width - moved_rectangle.rect.width - 24.).abs() < 0.001);

    let initial_east = ellipse_resize_handle_point_for_rect(
        ellipse_before.rect,
        ellipse_before.rotation_degrees,
        RectangleResizeHandle::East,
    );
    cx.simulate_mouse_down(
        to_view(initial_east.x, initial_east.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(initial_east.x + 18., initial_east.y - 12.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        to_view(initial_east.x + 18., initial_east.y - 12.),
        MouseButton::Left,
        Modifiers::default(),
    );
    let moved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(moved.selected_id.as_ref(), Some(&ellipse_id));
    let moved_ellipse = moved
        .ellipses
        .iter()
        .find(|item| item.id == ellipse_id)
        .unwrap()
        .clone();
    let expected_moved = PdfRect::new(
        ellipse_before.rect.x + 18.,
        ellipse_before.rect.y - 12.,
        ellipse_before.rect.width,
        ellipse_before.rect.height,
    )
    .unwrap();
    for (actual, expected) in [
        (moved_ellipse.rect.x, expected_moved.x),
        (moved_ellipse.rect.y, expected_moved.y),
        (moved_ellipse.rect.width, expected_moved.width),
        (moved_ellipse.rect.height, expected_moved.height),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let moved_east = ellipse_resize_handle_point_for_rect(
        moved_ellipse.rect,
        moved_ellipse.rotation_degrees,
        RectangleResizeHandle::East,
    );
    cx.simulate_mouse_down(
        to_view(moved_east.x, moved_east.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(moved_east.x + 30., moved_east.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    let preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    });
    let preview = preview
        .ellipses
        .iter()
        .find(|item| item.id == ellipse_id)
        .unwrap();
    assert!(preview.preview && preview.rect.width > moved_ellipse.rect.width);
    cx.simulate_mouse_up(
        to_view(moved_east.x + 30., moved_east.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    let resized_ellipse = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .ellipses
        .into_iter()
        .find(|item| item.id == ellipse_id)
        .unwrap();
    assert!(resized_ellipse.rect.width > moved_ellipse.rect.width);

    cx.simulate_click(to_view(245., 510.), Modifiers::default());
    let line_before = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .straight_lines
        .into_iter()
        .find(|item| item.id == line_id)
        .unwrap();
    cx.simulate_mouse_down(
        to_view(line_before.end.x, line_before.end.y),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(line_before.end.x + 24., line_before.end.y + 12.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        to_view(line_before.end.x + 24., line_before.end.y + 12.),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert_ne!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(document_id, cx))
            .unwrap()
            .straight_lines
            .iter()
            .find(|item| item.id == line_id)
            .unwrap()
            .end,
        line_before.end
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID);
    let properties_point = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID)
        .unwrap()
        .center();
    cx.simulate_click(properties_point, Modifiers::default());
    edit_selected_straight_line_through_real_controls(cx, &workspace, document_id);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save_as_point = cx.debug_bounds(DOCUMENT_SAVE_AS_ID).unwrap().center();
    cx.simulate_click(save_as_point, Modifiers::default());
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection({
        let owned_root = owned_root.clone();
        let saved_as = saved_as.clone();
        move |directory| {
            assert_eq!(directory, owned_root.as_path());
            Some(saved_as)
        }
    });
    wait_for_real_document_save_terminal(
        cx,
        &workspace,
        document_id,
        &saved_as,
        "Save As",
    );
    let (save_as_worker_pid, save_as_snapshot) = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert_eq!(session.path(), saved_as.as_path());
        assert_eq!(session.save_status(), &NativeDocumentSaveStatus::Idle);
        (
            session.worker_pid().unwrap(),
            workspace.annotation_snapshot(document_id, cx).unwrap(),
        )
    });
    assert!(!save_as_snapshot.dirty);
    assert_eq!(save_as_snapshot.saved_revision, save_as_snapshot.revision);
    assert_ne!(save_as_worker_pid, original_worker_pid);
    scratch_guard.track_worker(save_as_worker_pid);
    assert!(!Path::new(&format!("/proc/{original_worker_pid}")).exists());
    assert!(saved_as.is_file());
    assert_eq!(std::fs::read(&owned_source).unwrap(), public_source_bytes);

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &image_id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_image_rect(
                document_id,
                PdfRect::new(
                    image_before.rect.x + 12.,
                    image_before.rect.y - 8.,
                    image_before.rect.width + 24.,
                    image_before.rect.height + 16.,
                )
                .unwrap(),
                cx,
            )
        })
        .unwrap();
    let image_edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(image_edited.dirty);
    let save_point = cx.debug_bounds(DOCUMENT_SAVE_ID).unwrap().center();
    cx.simulate_click(save_point, Modifiers::default());
    wait_for_real_document_save_terminal(cx, &workspace, document_id, &saved_as, "Save");
    let (save_worker_pid, saved_snapshot) = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert_eq!(session.path(), saved_as.as_path());
        assert_eq!(session.save_status(), &NativeDocumentSaveStatus::Idle);
        (
            session.worker_pid().unwrap(),
            workspace.annotation_snapshot(document_id, cx).unwrap(),
        )
    });
    assert!(!saved_snapshot.dirty);
    assert_eq!(saved_snapshot.saved_revision, saved_snapshot.revision);
    assert_eq!(
        saved_snapshot.pens.len(),
        2,
        "the saved workspace state must contain exactly one Pen and one Highlight"
    );
    assert_ne!(save_worker_pid, save_as_worker_pid);
    scratch_guard.track_worker(save_worker_pid);
    assert!(!Path::new(&format!("/proc/{save_as_worker_pid}")).exists());

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &arrow_id, cx)
    }));
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));
    let before_paste = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let original_arrow = before_paste
        .straight_lines
        .iter()
        .find(|item| item.id == arrow_id)
        .unwrap()
        .clone();
    cx.simulate_keystrokes(EDIT_COPY);
    cx.simulate_keystrokes(EDIT_PASTE);
    let pasted_id = MarkupId::new("workspace:paste:arrow:9").unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(document_id, cx)),
        vec![pasted_id.clone()]
    );
    let after_paste = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let duplicate = after_paste
        .straight_lines
        .iter()
        .find(|item| item.id == pasted_id)
        .unwrap();
    assert!((duplicate.start.x - original_arrow.start.x - 12.).abs() < 0.001);
    assert!((duplicate.start.y - original_arrow.start.y + 12.).abs() < 0.001);
    assert_eq!(duplicate.kind, original_arrow.kind);
    assert_eq!(duplicate.appearance, original_arrow.appearance);
    assert_eq!(after_paste.revision, before_paste.revision + 1);
    assert_eq!(after_paste.undo_depth, before_paste.undo_depth + 1);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_DELETE_ID);
    let delete_point = cx
        .debug_bounds(DOCUMENT_ANNOTATION_DELETE_ID)
        .unwrap()
        .center();
    cx.simulate_click(delete_point, Modifiers::default());
    let deleted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!deleted.annotation_order.contains(&pasted_id));
    assert_eq!(deleted.revision, after_paste.revision + 1);
    assert_eq!(deleted.undo_depth, after_paste.undo_depth + 1);
    assert_eq!(deleted.redo_depth, 0);
    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_UNDO_ID);
    let undo_point = cx
        .debug_bounds(DOCUMENT_ANNOTATION_UNDO_ID)
        .unwrap()
        .center();
    cx.simulate_click(undo_point, Modifiers::default());
    let undone_delete = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(undone_delete.annotation_order.contains(&pasted_id));
    assert_eq!(undone_delete.selected_id, None);
    assert_eq!(undone_delete.revision, after_paste.revision);
    assert_eq!(undone_delete.undo_depth, after_paste.undo_depth);
    assert_eq!(undone_delete.redo_depth, 1);
    scroll_annotation_target_into_view(cx, DOCUMENT_ANNOTATION_REDO_ID);
    let redo_point = cx
        .debug_bounds(DOCUMENT_ANNOTATION_REDO_ID)
        .unwrap()
        .center();
    cx.simulate_click(redo_point, Modifiers::default());
    let expected = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!expected.annotation_order.contains(&pasted_id));
    assert_eq!(expected.selected_id, None);
    assert!(expected.dirty && expected.redo_depth == 0);
    assert_eq!(expected.revision, deleted.revision);
    assert_eq!(expected.undo_depth, deleted.undo_depth);
    assert_eq!(expected.annotation_order, expected_order);
    assert!(
        expected
            .rectangles
            .iter()
            .any(|item| item.id == rectangle_id)
    );
    assert!(expected.ellipses.iter().any(|item| item.id == ellipse_id));
    assert!(expected.images.iter().any(|item| item.id == image_id));
    assert!(
        expected
            .straight_lines
            .iter()
            .any(|item| item.id == arrow_id && item.kind == LineKind::Arrow)
    );
    let expected_rectangle = expected
        .rectangles
        .iter()
        .find(|item| item.id == rectangle_id)
        .unwrap()
        .clone();
    let expected_ellipse = expected
        .ellipses
        .iter()
        .find(|item| item.id == ellipse_id)
        .unwrap()
        .clone();
    let expected_line = expected
        .straight_lines
        .iter()
        .find(|item| item.id == line_id)
        .unwrap()
        .clone();
    assert_eq!(expected_line.appearance.stroke_color(), "#2563eb");
    assert_eq!(expected_line.appearance.stroke_width_pt(), 4.);
    assert_eq!(expected_line.appearance.opacity(), 0.5);
    let expected_arrow = expected
        .straight_lines
        .iter()
        .find(|item| item.id == arrow_id)
        .unwrap()
        .clone();
    let expected_pen = expected
        .pens
        .iter()
        .find(|item| item.id == pen_id)
        .unwrap()
        .clone();
    assert_eq!(expected_pen.tool(), InkTool::Pen);
    assert_eq!(expected_pen.blend_mode(), BlendMode::Normal);
    assert!(expected_pen.smooth_curves);
    assert_eq!(expected_pen.appearance.color(), "#ff0000");
    assert_eq!(expected_pen.appearance.width_pt(), 1.);
    let expected_highlight = expected
        .pens
        .iter()
        .find(|item| item.id == highlight_id)
        .unwrap()
        .clone();
    assert_eq!(expected_highlight.tool(), InkTool::Highlight);
    assert_eq!(expected_highlight.blend_mode(), BlendMode::Multiply);
    assert!(!expected_highlight.smooth_curves);
    assert_eq!(expected_highlight.appearance.color(), "#ffff00");
    assert_eq!(expected_highlight.appearance.width_pt(), 12.);
    let expected_text = expected
        .text_boxes
        .iter()
        .find(|item| item.id == text_id)
        .unwrap()
        .clone();
    assert_eq!(expected_text.content(), "native text\nlevel 2");
    assert_eq!(expected_text.style().font_family(), "Helvetica");
    assert_eq!(expected_text.style().font_size_pt(), 12.);
    assert_eq!(expected_text.style().color(), "#ff0000");
    let expected_image = expected
        .images
        .iter()
        .find(|item| item.id == image_id)
        .unwrap()
        .clone();

    let mut fresh_app = cx.cx.clone();
    cx.dispatch_action(RequestApplicationClose);
    cx.run_until_parked();
    assert!(close.read_with(cx, |close, _| close.dialog().is_some()));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_keystrokes("enter");
    wait_for_real_application_close_terminal(cx, &workspace, &close, document_id, &saved_as);
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert!(close.read_with(cx, |close, _| close.recovery().is_none()));
    assert!(matches!(
        close.read_with(cx, |close, _| close.effects().last().cloned()),
        Some(ApplicationCloseEffect::QuitRequested {
            kind: ApplicationCloseCompletionKind::SavedAll,
            ..
        })
    ));
    assert!(!Path::new(&format!("/proc/{save_worker_pid}")).exists());
    assert!(original_image_weak.upgrade().is_none());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());

    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_as)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&saved_as)
            .status()
            .unwrap()
            .success()
    );

    let typed = PdfPersistenceSession::open(&saved_as).unwrap();
    assert_eq!(typed.page_count(), source_page_count);
    assert_eq!(typed.untouched_annotations(), source_untouched.as_slice());
    assert!(
        typed
            .untouched_annotations()
            .iter()
            .any(|item| item.name == "unknown-1")
    );
    assert_eq!(typed.annotation_order(), expected_order);
    assert!(
        typed
            .rectangles()
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_rectangle))
    );
    assert!(
        typed
            .ellipses()
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_ellipse))
    );
    assert!(
        typed
            .straight_lines()
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_line))
    );
    assert!(
        typed
            .straight_lines()
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_arrow))
    );
    assert!(
        typed
            .straight_lines()
            .iter()
            .all(|item| item.id != pasted_id)
    );
    assert_eq!(typed.pens().len(), 2);
    assert!(typed.pens().iter().any(|item| item == &expected_pen));
    assert!(typed.pens().iter().any(|item| item == &expected_highlight));
    assert!(
        typed
            .text_boxes()
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_text))
    );
    let typed_image = typed
        .images()
        .iter()
        .find(|item| item.id == image_id)
        .unwrap();
    assert_eq!(typed_image.id, expected_image.id);
    assert_eq!(typed_image.page_index, expected_image.page_index);
    assert!(
        typed_image.rect.same_pdf_geometry_as(expected_image.rect),
        "typed reopen image rect mismatch: actual=({}, {}, {}, {}), expected=({}, {}, {}, {})",
        typed_image.rect.x,
        typed_image.rect.y,
        typed_image.rect.width,
        typed_image.rect.height,
        expected_image.rect.x,
        expected_image.rect.y,
        expected_image.rect.width,
        expected_image.rect.height,
    );
    assert!(
        typed_image.asset() == expected_image.asset(),
        "typed reopen image asset mismatch: actual={} {}x{}, expected={} {}x{}",
        typed_image.asset().id().as_str(),
        typed_image.asset().width_px(),
        typed_image.asset().height_px(),
        expected_image.asset().id().as_str(),
        expected_image.asset().width_px(),
        expected_image.asset().height_px(),
    );
    assert_eq!(typed_image.aspect_locked, expected_image.aspect_locked);
    assert_eq!(typed_image.locked, expected_image.locked);

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_087),
            generation: 1,
            path: saved_as.clone(),
        })
        .unwrap();
    let annotated = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .unwrap();
    let saved_base = pixel_proof.render_page(0, 320).unwrap();
    assert_eq!(
        Sha256::digest(saved_base.pixels_bgra()).to_vec(),
        original_base_digest
    );
    assert_ne!(
        Sha256::digest(annotated.pixels_bgra()),
        Sha256::digest(saved_base.pixels_bgra())
    );
    assert_ne!(
        Sha256::digest(annotated.pixels_bgra()).to_vec(),
        original_annotated_digest,
        "the final annotation-enabled page-zero digest must show an aggregate change from the original annotated fixture"
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    scratch_guard.track_worker(pixel_worker_pid);
    pixel_proof.close().unwrap();
    assert!(!Path::new(&format!("/proc/{pixel_worker_pid}")).exists());

    let fresh_workspace = fresh_app.update(|cx| {
        gpui_component::init(cx);
        cx.new(|cx| DocumentWorkspace::with_opener(backend, cx))
    });
    let fresh_content = AnyView::from(fresh_workspace.clone());
    let (_, fresh_cx) =
        fresh_app.add_window_view(move |window, cx| Root::new(fresh_content, window, cx));
    let reopened_document = fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.open_path(saved_as.clone(), cx)
    });
    wait_for_real_document_ready(fresh_cx, &fresh_workspace, reopened_document, &saved_as);
    let reopened_evidence = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            workspace.evidence_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert!(reopened_evidence.ready && reopened_evidence.current_raster_has_spatial_variation);
    assert_eq!(reopened_evidence.page_count as usize, source_page_count);
    let reopened = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert_eq!((reopened.undo_depth, reopened.redo_depth), (0, 0));
    assert!(!reopened.dirty);
    assert_eq!(reopened.selected_id, None);
    assert_eq!(reopened.annotation_order, expected_order);
    assert!(
        reopened
            .rectangles
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_rectangle))
    );
    assert!(
        reopened
            .ellipses
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_ellipse))
    );
    assert!(
        reopened
            .straight_lines
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_line))
    );
    assert!(
        reopened
            .straight_lines
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_arrow))
    );
    assert!(
        reopened
            .straight_lines
            .iter()
            .all(|item| item.id != pasted_id)
    );
    assert_eq!(reopened.pens.len(), 2);
    assert!(reopened.pens.iter().any(|item| item == &expected_pen));
    assert!(reopened.pens.iter().any(|item| item == &expected_highlight));
    assert!(
        reopened
            .text_boxes
            .iter()
            .any(|item| item.same_persisted_state_as(&expected_text))
    );
    let reopened_image = reopened
        .images
        .iter()
        .find(|item| item.id == image_id)
        .unwrap();
    assert_eq!(reopened_image.id, expected_image.id);
    assert_eq!(reopened_image.page_index, expected_image.page_index);
    assert!(
        reopened_image
            .rect
            .same_pdf_geometry_as(expected_image.rect),
        "fresh reopen image rect mismatch: actual=({}, {}, {}, {}), expected=({}, {}, {}, {})",
        reopened_image.rect.x,
        reopened_image.rect.y,
        reopened_image.rect.width,
        reopened_image.rect.height,
        expected_image.rect.x,
        expected_image.rect.y,
        expected_image.rect.width,
        expected_image.rect.height,
    );
    assert!(
        reopened_image.asset() == expected_image.asset(),
        "fresh reopen image asset mismatch: actual={} {}x{}, expected={} {}x{}",
        reopened_image.asset().id().as_str(),
        reopened_image.asset().width_px(),
        reopened_image.asset().height_px(),
        expected_image.asset().id().as_str(),
        expected_image.asset().width_px(),
        expected_image.asset().height_px(),
    );
    assert_eq!(reopened_image.aspect_locked, expected_image.aspect_locked);
    assert_eq!(reopened_image.locked, expected_image.locked);
    let reopened_image_weak = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            workspace.image_render_asset_weak(reopened_document, &image_asset_id, cx)
        })
        .unwrap();
    let reopened_worker_pid = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    scratch_guard.track_worker(reopened_worker_pid);
    assert!(fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.close_document(reopened_document, cx)
    }));
    assert!(!Path::new(&format!("/proc/{reopened_worker_pid}")).exists());
    assert!(reopened_image_weak.upgrade().is_none());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    assert_eq!(std::fs::read(&public_source).unwrap(), public_source_bytes);
    assert_eq!(std::fs::read(&public_image).unwrap(), public_image_bytes);
    assert_eq!(std::fs::read(&owned_source).unwrap(), public_source_bytes);
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&public_source).unwrap())),
        public_source_sha256
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&public_image).unwrap())),
        public_image_sha256
    );
    drop(scratch_guard);
    assert!(!owned_root.exists());
}
