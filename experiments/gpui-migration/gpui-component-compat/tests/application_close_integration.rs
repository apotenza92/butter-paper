use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use butter_paper_gpui_component_compat::{
    application_close::{
        ApplicationCloseAction, ApplicationCloseCompletionKind, ApplicationCloseTransitionStatus,
    },
    application_close_workspace::{
        APPLICATION_CLOSE_RECOVERY_ALERT_ID, APPLICATION_CLOSE_RECOVERY_CANCEL_ID,
        APPLICATION_CLOSE_RECOVERY_ID, APPLICATION_CLOSE_RECOVERY_PRIMARY_ID,
        ApplicationCloseEffect, ApplicationCloseRecoveryKind, ApplicationCloseShell,
        ApplicationCloseWorkspace, PendingApplicationSaveAs, RequestApplicationClose,
        register_application_close_action,
    },
    document_workspace::{
        ApplyDisposition, DOCUMENT_ERROR_ID, DocumentId, DocumentWorkspace, NativeDocumentOpener,
        NativeDocumentResource, NativeDocumentSaver, OpenDocumentRequest, OpenedNativeDocument,
        PdfDocumentSaver, PdfiumWorkerBackend, RasterSurface, SaveDocumentRequest,
        SavedNativeDocument, ThumbnailSurface,
    },
};
use butter_paper_gpui_gallery::{
    annotation_model::{MarkupId, PdfPoint},
    generated_document::{GeneratedDocumentRequest, GeneratedDocumentStore},
    pdf_engine::PdfPersistenceSession,
    viewer::TileRequest,
};
use gpui::{AnyView, AppContext as _, Modifiers, TestAppContext};
use gpui_component::{Root, WindowExt as _};

fn owned_target(name: impl AsRef<Path>) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".prepared")
        .join("owned-targets");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(name)
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
        let released = Arc::new(AtomicBool::new(false));
        self.releases.lock().unwrap().push(released.clone());
        let pid = self.next_pid.fetch_add(1, Ordering::Relaxed) + 4100;
        OpenedNativeDocument::new(
            title,
            vec![(612., 792.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
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
        worker,
        library,
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
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            let close = cx.new(|_| ApplicationCloseWorkspace::new(workspace.clone(), saver));
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
            sources[1].clone(),
            generated_target.clone(),
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
    observed_worker_pids.sort_unstable();
    observed_worker_pids.dedup();
    for pid in observed_worker_pids {
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
            "document-2".to_owned(),
            "document-3".to_owned(),
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

    let owned_root = manifest_dir
        .join(".prepared/native-shell-rectangle-real")
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
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
        })
        .unwrap();
    let rectangle_id = MarkupId::new("native-shell:rectangle").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                rectangle_id.clone(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });

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
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&owned_source)
            .status()
            .unwrap()
            .success()
    );

    let reopened = PdfPersistenceSession::open(&owned_source).unwrap();
    assert!(
        reopened
            .rectangles()
            .iter()
            .any(|rectangle| rectangle.id == rectangle_id)
    );
    let fresh_workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, fresh_cx) = fresh_app.add_window_view({
        let fresh_workspace_slot = fresh_workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            fresh_workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let fresh_workspace = fresh_workspace_slot
        .borrow_mut()
        .take()
        .expect("the fresh native window must retain its document workspace");
    let reopened_id = fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.open_path(owned_source.clone(), cx)
    });
    fresh_cx.run_until_parked();
    let reopened_worker_pid = fresh_workspace
        .read_with(fresh_cx, |workspace, cx| {
            let session = workspace.session(reopened_id, cx).unwrap();
            assert!(
                workspace
                    .annotation_snapshot(reopened_id, cx)
                    .unwrap()
                    .rectangles
                    .iter()
                    .any(|rectangle| rectangle.id == rectangle_id)
            );
            session.read(cx).worker_pid()
        })
        .unwrap();
    assert!(fresh_workspace.update(fresh_cx, |workspace, cx| {
        workspace.close_document(reopened_id, cx)
    }));
    assert!(!Path::new(&format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    std::fs::remove_dir_all(owned_root).unwrap();
}
