//! A signed document's signatures in the workspace: checked from its file
//! when it opens, summarised by the toolbar's signatures button, and listed
//! in the Signatures dialog the button opens.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DOCUMENT_SIGN_CHOOSE_ID, DOCUMENT_SIGN_CONFIRM_ID, DOCUMENT_SIGN_DIALOG_ID,
    DOCUMENT_SIGN_INVISIBLE_ID, DOCUMENT_SIGN_PASSWORD_ID, DOCUMENT_SIGN_TIMESTAMP_ID,
    DOCUMENT_SIGNATURES_DIALOG_ID, DocumentId, DocumentWorkspace, NativeDocumentOpener,
    OpenDocumentRequest, init_document_workspace_actions,
};
use butter_paper::pdf_signatures::{Verdict, document_verdict};
use butter_paper::viewer_toolbar_strip::VIEWER_SIGNATURES_ID;
use gpui::{AppContext as _, Modifiers, TestAppContext};
use gpui_component::Root;

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

/// A document the worker would report as signed (or not).
struct SignedResource {
    signed: bool,
    released: AtomicBool,
}

impl NativeDocumentResource for SignedResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4300)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_thumbnail(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn digitally_signed(&self) -> bool {
        self.signed
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

struct DialogShell(gpui::Entity<DocumentWorkspace>);

impl gpui::Render for DialogShell {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement as _, Styled as _};
        let dialogs = Root::render_dialog_layer(window, cx);
        gpui::div()
            .size_full()
            .child(self.0.clone())
            .children(dialogs)
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/signatures")
        .join(name)
}

/// Opens `path` as a document the worker reports as `signed`, and lets the
/// signature check finish.
fn open(
    cx: &mut TestAppContext,
    path: PathBuf,
    signed: bool,
) -> (
    gpui::Entity<DocumentWorkspace>,
    DocumentId,
    &mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            let shell = cx.new(|_| DialogShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let request = workspace.update(cx, |workspace, cx| workspace.begin_open(path, cx));
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "signed.pdf",
            vec![(595., 842.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(SignedResource {
                signed,
                released: AtomicBool::new(false),
            }),
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (workspace, document_id, cx)
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not painted"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn a_pdf_changed_after_signing_shows_an_invalid_signature_and_lists_it(cx: &mut TestAppContext) {
    let (workspace, document_id, cx) = open(cx, fixture("signed-then-changed.pdf"), true);
    let verdict = workspace.read_with(cx, |workspace, _| {
        document_verdict(workspace.document_signatures(document_id).unwrap())
    });
    assert_eq!(verdict, Some(Verdict::Invalid));

    click(cx, VIEWER_SIGNATURES_ID);
    assert!(
        cx.debug_bounds(DOCUMENT_SIGNATURES_DIALOG_ID).is_some(),
        "the button opens the Signatures dialog"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_SIGNATURES_DIALOG_ID).is_none(),
        "Escape closes it"
    );
}

#[gpui::test]
fn an_intact_signature_from_an_untrusted_certificate_is_unverified(cx: &mut TestAppContext) {
    // The test root is not one of this computer's roots.
    let (workspace, document_id, cx) = open(cx, fixture("signed-rsa.pdf"), true);
    let verdict = workspace.read_with(cx, |workspace, _| {
        document_verdict(workspace.document_signatures(document_id).unwrap())
    });
    assert_eq!(verdict, Some(Verdict::Unverified));
    assert!(cx.debug_bounds(VIEWER_SIGNATURES_ID).is_some());
}

#[gpui::test]
fn an_unsigned_pdf_has_no_signatures_button(cx: &mut TestAppContext) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/word-table.pdf");
    let (workspace, document_id, cx) = open(cx, path, false);
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.document_signatures(document_id).is_none()
    }));
    assert!(cx.debug_bounds(VIEWER_SIGNATURES_ID).is_none());
}

/// Opens files as documents the worker reports as signed when they hold a
/// signature.
struct SignedOpener;

impl NativeDocumentOpener for SignedOpener {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        let bytes = std::fs::read(&request.path).map_err(|error| error.to_string())?;
        let signed = bytes.windows(10).any(|window| window == b"/ByteRange");
        OpenedNativeDocument::new(
            "report.pdf",
            vec![(595., 842.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(SignedResource {
                signed,
                released: AtomicBool::new(false),
            }),
        )
    }
}

/// A workspace opening real files through [`SignedOpener`], with the
/// system identities `identities` gives, showing a scratch copy of the
/// untagged report in a 595 x 842 page view.
fn signing_workspace<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    identities: fn() -> Vec<butter_paper::system_certificates::SystemIdentity>,
) -> (
    gpui::Entity<DocumentWorkspace>,
    PathBuf,
    PathBuf,
    &'a mut gpui::VisualTestContext,
) {
    use butter_paper::page_view_control::PageViewMode;
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("target/test-scratch/{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let pdf = scratch.join("report.pdf");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/word-table.pdf"),
        &pdf,
    )
    .unwrap();
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| {
                let mut workspace = DocumentWorkspace::with_opener(Arc::new(SignedOpener), cx);
                workspace.set_system_identity_source(identities);
                workspace
            });
            workspace_slot.replace(Some(workspace.clone()));
            let shell = cx.new(|_| DialogShell(workspace));
            Root::new(shell, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id = workspace.update(cx, |workspace, cx| workspace.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 800., 900., 1., cx)
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (workspace, scratch, pdf, cx)
}

/// Draws a box from page point `from` to `to` (PDF points on the 595 x 842
/// page), click then click as the Snapshot tool takes it.
fn draw_box(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    from: (f32, f32),
    to: (f32, f32),
) {
    use gpui::{point, px};
    let document_id =
        workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let layer_id = Box::leak(
        butter_paper::document_workspace::document_annotation_layer_id(document_id, 0)
            .into_boxed_str(),
    );
    let layer = cx.debug_bounds(layer_id).expect("the page is painted");
    let scale = (f32::from(layer.size.width) / 595.).min(f32::from(layer.size.height) / 842.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 595. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 842. * scale) / 2.),
    );
    let project =
        |(x, y): (f32, f32)| point(origin.x + px(x * scale), origin.y + px((842. - y) * scale));
    cx.simulate_click(project(from), Modifiers::default());
    cx.simulate_mouse_move(project(to), None, Modifiers::default());
    cx.simulate_click(project(to), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // The dialog that follows opens on the next frame and animates in.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn sign_with_certificate_draws_a_box_then_saves_a_signed_copy(cx: &mut TestAppContext) {
    use butter_paper::annotation_adapter::AnnotationTool;
    use butter_paper::pdf_signatures::{Changes, Integrity, signatures};
    let (workspace, scratch, pdf, cx) = signing_workspace(cx, "sign-dialog", Vec::new);
    let document_id =
        workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    let original = std::fs::read(&pdf).unwrap();
    let p12 = fixture("test-signer-rsa.p12");

    // As in Acrobat and Revu: first the box where the signature goes.
    workspace.update(cx, |workspace, cx| {
        workspace.sign_active_document_with_certificate(cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Snapshot)
    );
    assert!(cx.debug_bounds(DOCUMENT_SIGN_DIALOG_ID).is_none());
    draw_box(cx, &workspace, (300., 60.), (540., 120.));
    assert!(
        cx.debug_bounds(DOCUMENT_SIGN_DIALOG_ID).is_some(),
        "then the certificate"
    );
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(snapshot.snapshots.is_empty(), "the box leaves no markup");
    // No timestamp server is set, so none is offered.
    assert!(cx.debug_bounds(DOCUMENT_SIGN_TIMESTAMP_ID).is_none());

    click(cx, DOCUMENT_SIGN_CHOOSE_ID);
    cx.simulate_path_prompt_response(move |_| Some(vec![p12]));
    cx.run_until_parked();
    // A wrong password keeps the dialog open.
    click(cx, DOCUMENT_SIGN_PASSWORD_ID);
    cx.simulate_input("wrong");
    click(cx, DOCUMENT_SIGN_CONFIRM_ID);
    assert!(
        cx.debug_bounds(DOCUMENT_SIGN_DIALOG_ID).is_some(),
        "the dialog stays to say the password is wrong"
    );
    click(cx, DOCUMENT_SIGN_PASSWORD_ID);
    cx.simulate_keystrokes("cmd-a backspace ctrl-a backspace");
    cx.simulate_input("butter-paper-test");
    click(cx, DOCUMENT_SIGN_CONFIRM_ID);
    cx.run_until_parked();

    // Then Save As, suggesting a copy beside it.
    assert!(cx.did_prompt_for_new_path());
    let copy = scratch.join("report (signed).pdf");
    let expected_directory = scratch.clone();
    let chosen = copy.clone();
    cx.simulate_new_path_selection(move |directory| {
        assert_eq!(
            std::fs::canonicalize(directory).unwrap(),
            std::fs::canonicalize(&expected_directory).unwrap()
        );
        Some(chosen)
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SIGN_DIALOG_ID).is_none());

    assert_eq!(
        std::fs::read(&pdf).unwrap(),
        original,
        "the original is unchanged"
    );
    let found = signatures(&std::fs::read(&copy).unwrap(), &[]);
    assert_eq!(found.len(), 1, "the copy is signed");
    assert_eq!(found[0].signer, "Test Signer RSA");
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].changes, Changes::None);
    assert_eq!(found[0].page, Some(0), "visible in the box");
    // The signed copy opens, showing its signature.
    assert!(cx.debug_bounds(VIEWER_SIGNATURES_ID).is_some());
    let _ = std::fs::remove_dir_all(scratch);
}

/// The RSA test identity as if the system held it.
fn test_system_identities() -> Vec<butter_paper::system_certificates::SystemIdentity> {
    use butter_paper::system_certificates::{SystemIdentity, SystemKey};
    struct TestKey(rsa::RsaPrivateKey);
    impl SystemKey for TestKey {
        fn sign_sha256(&self, digest: &[u8]) -> Result<Vec<u8>, String> {
            self.0
                .sign(rsa::Pkcs1v15Sign::new::<signature_sha2::Sha256>(), digest)
                .map_err(|error| error.to_string())
        }
    }
    let store = p12_keystore::KeyStore::from_pkcs12(
        &std::fs::read(fixture("test-signer-rsa.p12")).unwrap(),
        "butter-paper-test",
        p12_keystore::Pkcs12ImportPolicy::Relaxed,
    )
    .unwrap();
    let (_, chain) = store.private_key_chain().unwrap();
    let key =
        <rsa::RsaPrivateKey as rsa::pkcs8::DecodePrivateKey>::from_pkcs8_der(chain.key().as_der())
            .unwrap();
    vec![SystemIdentity {
        name: "Test Signer RSA".into(),
        issuer: "Butter Paper Test Root".into(),
        expires: i64::MAX,
        chain: chain
            .certs()
            .iter()
            .map(|certificate| certificate.as_der().to_vec())
            .collect(),
        key: Box::new(TestKey(key)),
    }]
}

#[gpui::test]
fn a_system_certificate_signs_without_a_password(cx: &mut TestAppContext) {
    use butter_paper::pdf_signatures::{Integrity, signatures};
    let (workspace, scratch, pdf, cx) =
        signing_workspace(cx, "sign-system", test_system_identities);
    // A timestamp server set in Settings is offered, as a choice.
    cx.update(|_, cx| {
        butter_paper::signature_settings::SignatureSettings::set_timestamp_server(
            Some("https://timestamp.invalid".into()),
            cx,
        )
    });
    workspace.update(cx, |workspace, cx| {
        workspace.sign_active_document_with_certificate(cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    draw_box(cx, &workspace, (300., 60.), (540., 120.));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_SIGN_PASSWORD_ID).is_none(),
        "a system certificate needs no password"
    );
    // No network in tests: sign without the timestamp.
    click(cx, DOCUMENT_SIGN_TIMESTAMP_ID);
    // And without a mark on the page, as Acrobat's invisible signature.
    click(cx, DOCUMENT_SIGN_INVISIBLE_ID);
    click(cx, DOCUMENT_SIGN_CONFIRM_ID);
    cx.run_until_parked();
    // Signing over the PDF itself keeps it in place.
    let same = pdf.clone();
    cx.simulate_new_path_selection(move |_| Some(same));
    cx.run_until_parked();
    let found = signatures(&std::fs::read(&pdf).unwrap(), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].signer, "Test Signer RSA");
    assert_eq!(found[0].integrity, Integrity::Intact);
    assert_eq!(found[0].timestamp, None);
    assert_eq!(found[0].page, None, "invisible");
    let _ = std::fs::remove_dir_all(scratch);
}

#[gpui::test]
fn the_signing_dialog_keeps_its_buttons_on_screen_in_a_short_window(cx: &mut TestAppContext) {
    let (workspace, scratch, _pdf, cx) =
        signing_workspace(cx, "sign-short", test_system_identities);
    workspace.update(cx, |workspace, cx| {
        workspace.sign_active_document_with_certificate(cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    draw_box(cx, &workspace, (300., 60.), (540., 120.));
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(320.)));
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let viewport = cx.update(|window, _| window.viewport_size());
    let confirm = cx
        .debug_bounds(DOCUMENT_SIGN_CONFIRM_ID)
        .expect("Sign is painted");
    assert!(
        confirm.bottom() <= viewport.height && confirm.top() >= gpui::px(0.),
        "Sign stays in the {viewport:?} window: {confirm:?}"
    );
    // The body scrolls instead: what does not fit is clipped, not lost.
    let content = cx.debug_bounds(DOCUMENT_SIGN_DIALOG_ID).unwrap();
    // Below the title bar the app draws outside macOS, which stays usable.
    if !cfg!(target_os = "macos") {
        assert!(
            content.top() > gpui_component::TITLE_BAR_HEIGHT,
            "below the title bar: {content:?}"
        );
    }
    assert!(content.size.height > viewport.height - gpui::px(32.) - confirm.size.height);
    let _ = std::fs::remove_dir_all(scratch);
}

#[gpui::test]
fn a_signature_waiting_on_a_save_that_fails_is_dropped_and_said(cx: &mut TestAppContext) {
    use butter_paper::annotation_model::{MarkupId, PdfPoint};
    let (workspace, scratch, pdf, cx) =
        signing_workspace(cx, "sign-unsaved", test_system_identities);
    let original = std::fs::read(&pdf).unwrap();
    let document_id =
        workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    // An unsaved change; this test workspace cannot save, so the save
    // before signing fails.
    workspace.update(cx, |workspace, cx| {
        workspace
            .create_rectangle(
                document_id,
                0,
                MarkupId::new("rectangle:unsaved").unwrap(),
                PdfPoint::new(40., 40.).unwrap(),
                PdfPoint::new(80., 80.).unwrap(),
                cx,
            )
            .unwrap()
    });
    workspace.update(cx, |workspace, cx| {
        workspace.sign_active_document_with_certificate(cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    draw_box(cx, &workspace, (300., 60.), (540., 120.));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    click(cx, DOCUMENT_SIGN_CONFIRM_ID);
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status()),
        Some("Not signed: the PDF wasn't saved.".to_owned())
    );
    assert!(
        !cx.did_prompt_for_new_path(),
        "no Save As for a signed copy"
    );
    assert_eq!(std::fs::read(&pdf).unwrap(), original, "nothing was signed");
    let _ = std::fs::remove_dir_all(scratch);
}
