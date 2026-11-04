//! Sign with Certificate, as Acrobat and Revu do it: a box drawn where the
//! signature goes (or a selected handwritten signature to put it in place
//! of), then a certificate from the system or a `.p12`/`.pfx` file, then
//! Save As for the signed copy ([`crate::pdf_signing`]). A timestamp is
//! added only when Settings names a timestamp server.

use super::*;
use gpui::div;
use gpui_component::{
    checkbox::Checkbox,
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
};

use crate::annotation_model::DecodedRgbaAsset;
use crate::dialog_buttons::dialog_close_button;
use crate::pdf_signing::{SignOptions, SignatureAppearance, SigningIdentity, sign_pdf};
use crate::system_certificates::SystemIdentity;

pub const DOCUMENT_SIGN_DIALOG_ID: &str = "document-sign-dialog";
pub const DOCUMENT_SIGN_CHOOSE_ID: &str = "document-sign-choose";
pub const DOCUMENT_SIGN_IDENTITIES_ID: &str = "document-sign-identities";
pub const DOCUMENT_SIGN_PASSWORD_ID: &str = "document-sign-password";
pub const DOCUMENT_SIGN_REASON_ID: &str = "document-sign-reason";
pub const DOCUMENT_SIGN_PLACE_ID: &str = "document-sign-place";
pub const DOCUMENT_SIGN_APPEARANCE_ID: &str = "document-sign-appearance";
pub const DOCUMENT_SIGN_INVISIBLE_ID: &str = "document-sign-invisible";
pub const DOCUMENT_SIGN_TIMESTAMP_ID: &str = "document-sign-timestamp";
pub const DOCUMENT_SIGN_TIMESTAMP_SETTINGS_ID: &str = "document-sign-timestamp-settings";
pub const DOCUMENT_SIGN_CANCEL_ID: &str = "document-sign-cancel";
pub const DOCUMENT_SIGN_CONFIRM_ID: &str = "document-sign-confirm";

/// A handwritten signature placed on the page, to show the digital
/// signature in its place.
#[derive(Clone)]
pub(super) struct PlacedSignature {
    id: MarkupId,
    page: u32,
    rect: [f64; 4],
    image: DecodedRgbaAsset,
}

/// Where a digital signature goes.
#[derive(Clone)]
pub(super) enum Placement {
    /// In a box drawn on a page: zero-based page, and left, bottom, right,
    /// top in PDF user space.
    Box { page: u32, rect: [f64; 4] },
    /// In place of the selected handwritten signature, which it draws.
    Selected(PlacedSignature),
}

/// How a signature in a drawn box looks, as Acrobat's appearance choices.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SignatureLook {
    /// The signer's name large beside "Digitally signed by", the name and
    /// the date (Acrobat's standard text).
    #[default]
    NameAndDetails,
    /// Only the details.
    Details,
    /// No mark on the page; the signature shows in the Signatures list.
    Invisible,
}

impl SignatureLook {
    const ALL: [Self; 3] = [Self::NameAndDetails, Self::Details, Self::Invisible];

    fn label(self) -> &'static str {
        match self {
            Self::NameAndDetails => "Name and details",
            Self::Details => "Details only",
            Self::Invisible => "Invisible",
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::NameAndDetails => "document-sign-name-and-details",
            Self::Details => "document-sign-details",
            Self::Invisible => DOCUMENT_SIGN_INVISIBLE_ID,
        }
    }
}

/// A signature to add once its document is saved.
pub(super) struct PendingSignature {
    document_id: DocumentId,
    identity: Arc<SigningIdentity>,
    reason: Option<String>,
    placement: Placement,
    look: SignatureLook,
    timestamp_server: Option<String>,
}

/// The Sign with Certificate dialog's state.
pub(super) struct SignDialog {
    document_id: DocumentId,
    workspace: WeakEntity<DocumentWorkspace>,
    certificate: Option<PathBuf>,
    /// The system's signing identities, once looked up.
    identities: Option<Vec<SystemIdentity>>,
    /// The chosen system identity; `None` signs with a certificate file.
    chosen_identity: Option<usize>,
    /// The user picked an option, so a late lookup does not change it.
    chose: bool,
    password: Entity<InputState>,
    reason: Entity<InputState>,
    placement: Placement,
    look: SignatureLook,
    /// Whether to add a timestamp, when Settings names a server.
    timestamp: bool,
    error: Option<String>,
    _settings: Subscription,
}

impl SignDialog {
    fn choose_certificate(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a certificate file".into()),
            allowed_extensions: Some(vec!["p12".into(), "pfx".into()]),
        });
        cx.spawn(async move |dialog, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = dialog.update(cx, |dialog, cx| {
                dialog.certificate = Some(path.clone());
                dialog.error = None;
                let _ = dialog.workspace.update(cx, |workspace, _| {
                    workspace.last_certificate_path = Some(path);
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn identities_found(&mut self, identities: Vec<SystemIdentity>, cx: &mut Context<Self>) {
        if !self.chose {
            let now = unix_now();
            self.chosen_identity = identities
                .iter()
                .position(|identity| identity.expires > now);
        }
        self.identities = Some(identities);
        cx.notify();
    }

    /// Whether Sign can go ahead.
    fn ready(&self) -> bool {
        self.chosen_identity.is_some() || self.certificate.is_some()
    }

    /// Opens the chosen identity (a certificate file with its password);
    /// the dialog shows why it could not be.
    fn confirm(&mut self, cx: &mut Context<Self>) -> bool {
        let chosen = self
            .chosen_identity
            .zip(self.identities.as_mut())
            .filter(|(index, identities)| *index < identities.len())
            .map(|(index, identities)| identities.remove(index));
        let identity = if let Some(chosen) = chosen {
            self.chosen_identity = None;
            Ok(SigningIdentity::from_system(chosen))
        } else {
            let Some(path) = self.certificate.clone() else {
                self.error = Some("Choose a certificate first.".into());
                cx.notify();
                return false;
            };
            let password = self.password.read(cx).value().to_string();
            std::fs::read(&path)
                .map_err(|error| format!("The certificate file could not be read: {error}."))
                .and_then(|bytes| {
                    SigningIdentity::from_pkcs12(&bytes, &password)
                        .map_err(|error| error.to_string())
                })
        };
        let identity = match identity {
            Ok(identity) => identity,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return false;
            }
        };
        let reason = self.reason.read(cx).value().trim().to_owned();
        let pending = PendingSignature {
            document_id: self.document_id,
            identity: Arc::new(identity),
            reason: (!reason.is_empty()).then_some(reason),
            placement: self.placement.clone(),
            look: self.look,
            timestamp_server: crate::signature_settings::SignatureSettings::timestamp_server(cx)
                .filter(|_| self.timestamp),
        };
        let _ = self.workspace.update(cx, |workspace, cx| {
            workspace.begin_certificate_signing(pending, cx);
        });
        true
    }
}

impl Render for SignDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let certificate = self
            .certificate
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());
        let label = |text: &'static str| div().text_sm().font_medium().child(text);
        let now = unix_now();
        let file_chosen = self.chosen_identity.is_none();
        let identities: Vec<(String, bool)> = self
            .identities
            .iter()
            .flatten()
            .map(|identity| {
                let expired = identity.expires <= now;
                let mut text = format!("{}, issued by {}", identity.name, identity.issuer);
                if expired {
                    text.push_str(" (expired)");
                }
                (text, expired)
            })
            .collect();
        let identity_count = identities.len();
        let choices = (identity_count > 0).then(|| {
            let mut group = RadioGroup::vertical(DOCUMENT_SIGN_IDENTITIES_ID)
                .selected_index(Some(self.chosen_identity.unwrap_or(identity_count)))
                .on_click(cx.listener(move |dialog, index: &usize, _, cx| {
                    dialog.chose = true;
                    dialog.chosen_identity = (*index < identity_count).then_some(*index);
                    dialog.error = None;
                    cx.notify();
                }));
            for (index, (text, expired)) in identities.into_iter().enumerate() {
                group = group.child(
                    Radio::new(("document-sign-identity", index))
                        .label(text)
                        .disabled(expired),
                );
            }
            group.child(
                Radio::new("document-sign-identity-file")
                    .label("A certificate file (.p12 or .pfx)"),
            )
        });
        let looking = self.identities.is_none();
        v_flex()
            .id(DOCUMENT_SIGN_DIALOG_ID)
            .debug_selector(|| DOCUMENT_SIGN_DIALOG_ID.into())
            .gap_3()
            // Room for the last row's descenders, which hang below a
            // control label's box (Segoe UI's most), inside the dialog's
            // clipped body.
            .pb_1()
            .child(div().text_sm().text_color(theme.muted_foreground).child(
                "A digital signature shows who signed and reveals any later change. It is \
                         added to the saved PDF.",
            ))
            .child(
                v_flex()
                    .gap_1()
                    .child(label("Certificate"))
                    .when(looking, |section| {
                        section.child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Looking for your certificates…"),
                        )
                    })
                    .children(choices)
                    .when(file_chosen, |section| {
                        section.child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_sm()
                                        .when(certificate.is_none(), |text| {
                                            text.text_color(theme.muted_foreground)
                                        })
                                        .child(certificate.unwrap_or_else(|| {
                                            "No .p12 or .pfx file chosen".to_owned()
                                        })),
                                )
                                .child(
                                    Button::new(DOCUMENT_SIGN_CHOOSE_ID)
                                        .debug_selector(|| DOCUMENT_SIGN_CHOOSE_ID.into())
                                        .outline()
                                        .small()
                                        .label("Choose…")
                                        .on_click(cx.listener(|dialog, _, _, cx| {
                                            dialog.choose_certificate(cx);
                                        })),
                                ),
                        )
                    }),
            )
            .when(file_chosen, |content| {
                content.child(
                    v_flex().gap_1().child(label("Password")).child(
                        div()
                            .id(DOCUMENT_SIGN_PASSWORD_ID)
                            .debug_selector(|| DOCUMENT_SIGN_PASSWORD_ID.into())
                            .child(Input::new(&self.password).mask_toggle()),
                    ),
                )
            })
            .child(
                v_flex().gap_1().child(label("Reason")).child(
                    div()
                        .id(DOCUMENT_SIGN_REASON_ID)
                        .debug_selector(|| DOCUMENT_SIGN_REASON_ID.into())
                        .child(Input::new(&self.reason)),
                ),
            )
            .child(match self.placement {
                Placement::Selected(_) => div()
                    .id(DOCUMENT_SIGN_PLACE_ID)
                    .debug_selector(|| DOCUMENT_SIGN_PLACE_ID.into())
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("It goes in place of the selected signature, which stays in this PDF.")
                    .into_any_element(),
                Placement::Box { .. } => {
                    let mut looks = RadioGroup::horizontal(DOCUMENT_SIGN_APPEARANCE_ID)
                        .selected_index(
                            SignatureLook::ALL
                                .iter()
                                .position(|look| *look == self.look),
                        )
                        .on_click(cx.listener(|dialog, index: &usize, _, cx| {
                            if let Some(look) = SignatureLook::ALL.get(*index) {
                                dialog.look = *look;
                                cx.notify();
                            }
                        }));
                    for (index, look) in SignatureLook::ALL.into_iter().enumerate() {
                        looks = looks.child(
                            Radio::new(("document-sign-look", index))
                                .debug_selector(move || look.selector().into())
                                .label(look.label()),
                        );
                    }
                    v_flex()
                        .gap_1()
                        .child(label("Appearance"))
                        .child(
                            div()
                                .debug_selector(|| DOCUMENT_SIGN_APPEARANCE_ID.into())
                                .child(looks),
                        )
                        .into_any_element()
                }
            })
            .child(
                v_flex().gap_1().child(label("Timestamp")).child(
                    match crate::signature_settings::SignatureSettings::timestamp_server(cx)
                        .as_deref()
                    {
                        Some(server) => Checkbox::new(DOCUMENT_SIGN_TIMESTAMP_ID)
                            .debug_selector(|| DOCUMENT_SIGN_TIMESTAMP_ID.into())
                            .label(format!(
                                "Add a timestamp from {}",
                                crate::signature_settings::timestamp_server_host(server)
                            ))
                            .checked(self.timestamp)
                            .on_click(cx.listener(|dialog, checked: &bool, _, cx| {
                                dialog.timestamp = *checked;
                                cx.notify();
                            }))
                            .into_any_element(),
                        None => h_flex()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("No timestamp server is set."),
                            )
                            .child(
                                Button::new(DOCUMENT_SIGN_TIMESTAMP_SETTINGS_ID)
                                    .debug_selector(|| DOCUMENT_SIGN_TIMESTAMP_SETTINGS_ID.into())
                                    .ghost()
                                    .small()
                                    .label("Set one in Settings…")
                                    .on_click(|_, window, cx| {
                                        crate::application_settings::open_signature_settings(
                                            window, cx,
                                        );
                                    }),
                            )
                            .into_any_element(),
                    },
                ),
            )
            .when_some(self.error.clone(), |content, error| {
                content.child(div().text_sm().text_color(theme.danger).child(error))
            })
    }
}

impl DocumentWorkspace {
    /// Where Sign with Certificate finds the system's signing identities
    /// (the system's own store unless a test replaces it).
    pub fn set_system_identity_source(&mut self, source: fn() -> Vec<SystemIdentity>) {
        self.system_identity_source = source;
    }

    /// File ▸ Sign with Certificate…: with a handwritten signature selected,
    /// signs in its place; otherwise asks for a box where the signature
    /// goes, then for a certificate.
    pub fn sign_active_document_with_certificate(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        if self.signing_documents.contains(&document_id) {
            return;
        }
        match self.selected_placed_signature(document_id, cx) {
            Some(selected) => {
                self.sign_dialog_requested = Some((document_id, Placement::Selected(selected)));
                cx.notify();
            }
            None => self.begin_signature_box(
                document_id,
                super::signature_capture::BoxPurpose::PlaceDigitalSignature,
                cx,
            ),
        }
    }

    /// The box for a digital signature is drawn: asks for the certificate.
    pub(super) fn open_sign_dialog_for_box(
        &mut self,
        document_id: DocumentId,
        page: u32,
        rect: [f64; 4],
        cx: &mut Context<Self>,
    ) {
        self.sign_dialog_requested = Some((document_id, Placement::Box { page, rect }));
        cx.notify();
    }

    /// The selected annotation when it is an image, such as a placed
    /// handwritten signature.
    fn selected_placed_signature(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<PlacedSignature> {
        let snapshot = self.annotation_snapshot(document_id, cx)?;
        let selected = snapshot.selected_id.as_ref()?;
        let image = snapshot.images.iter().find(|image| &image.id == selected)?;
        let rect = image.rect;
        Some(PlacedSignature {
            id: image.id.clone(),
            page: image.page_index,
            rect: [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height],
            image: image.asset().clone(),
        })
    }

    pub(super) fn open_sign_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((document_id, placement)) = self.sign_dialog_requested.take() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let workspace = cx.entity().downgrade();
        let certificate = self.last_certificate_path.clone();
        let dialog = cx.new(|cx| SignDialog {
            document_id,
            workspace,
            certificate,
            identities: None,
            chosen_identity: None,
            chose: false,
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("The certificate file's password")
            }),
            reason: cx.new(|cx| InputState::new(window, cx).placeholder("Optional")),
            placement,
            look: SignatureLook::default(),
            // On when a server is set, as it is then wanted.
            timestamp: true,
            error: None,
            // A server set meanwhile in Settings shows at once.
            _settings: cx.observe_global::<crate::signature_settings::SignatureSettings>(
                |_, cx| cx.notify(),
            ),
        });
        // The system's identities, looked up off the UI thread: a locked
        // keychain may take a while.
        let lookup = dialog.downgrade();
        let background = cx.background_executor().clone();
        let source = self.system_identity_source;
        cx.spawn(async move |_, cx| {
            let identities = background.spawn(async move { source() }).await;
            let _ = lookup.update(cx, |dialog, cx| dialog.identities_found(identities, cx));
        })
        .detach();
        window.open_dialog(cx, move |surface, _, cx| {
            let confirm = dialog.clone();
            let ready = dialog.read(cx).ready();
            surface
                .title("Sign with a certificate")
                .w(px(460.))
                .max_w_full()
                .close_button(false)
                .child(dialog.clone())
                .footer(
                    DialogFooter::new()
                        .child(dialog_close_button(
                            Button::new(DOCUMENT_SIGN_CANCEL_ID)
                                .debug_selector(|| DOCUMENT_SIGN_CANCEL_ID.into())
                                .outline()
                                .label("Cancel"),
                        ))
                        .child(dialog_action_button(
                            Button::new(DOCUMENT_SIGN_CONFIRM_ID)
                                .debug_selector(|| DOCUMENT_SIGN_CONFIRM_ID.into())
                                .primary()
                                .disabled(!ready)
                                .label("Sign"),
                        )),
                )
                .on_ok(move |_, _, cx| confirm.update(cx, |dialog, cx| dialog.confirm(cx)))
        });
    }

    /// Signs once the document is saved, as Revu asks. A selected
    /// handwritten signature stays in this PDF; the signed copy draws the
    /// digital signature in its place instead.
    pub(super) fn begin_certificate_signing(
        &mut self,
        pending: PendingSignature,
        cx: &mut Context<Self>,
    ) {
        let document_id = pending.document_id;
        let dirty = self
            .session(document_id, cx)
            .is_some_and(|session| session.read(cx).is_dirty());
        if dirty {
            self.sign_after_save = Some(pending);
            self.annotation_statuses
                .insert(document_id, "Saving the PDF before signing it…".into());
            // This document's own save, whichever tab is active.
            if self.document_requires_save_as(document_id, cx) {
                self.prompt_to_save_as(document_id, cx);
            } else {
                let _ = self.save_path(document_id, cx);
            }
            cx.notify();
            return;
        }
        self.sign_saved_document_now(pending, cx);
    }

    /// After a save of `document_id`: signs it when that save was for
    /// signing and left nothing unsaved.
    pub(super) fn sign_after_document_saved(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        if self
            .sign_after_save
            .as_ref()
            .is_none_or(|pending| pending.document_id != document_id)
        {
            return;
        }
        let Some(pending) = self.sign_after_save.take() else {
            return;
        };
        let dirty = self
            .session(document_id, cx)
            .is_some_and(|session| session.read(cx).is_dirty());
        if dirty {
            self.annotation_statuses.insert(
                document_id,
                "Not signed: the PDF changed while it was saved. Sign it again.".into(),
            );
            cx.notify();
        } else {
            self.sign_saved_document_now(pending, cx);
        }
    }

    /// Forgets a signature waiting on a save that did not happen (cancelled
    /// or failed): nothing is signed, and the status bar says so.
    pub(super) fn cancel_sign_after_save(&mut self, document_id: DocumentId) {
        if self
            .sign_after_save
            .as_ref()
            .is_some_and(|pending| pending.document_id == document_id)
        {
            self.sign_after_save = None;
            self.annotation_statuses
                .insert(document_id, "Not signed: the PDF wasn't saved.".into());
        }
    }

    /// Asks where to save the signed copy, as Acrobat and Revu do, then
    /// signs: a new file opens in a new tab; signing over the PDF itself
    /// closes it while the signature is appended and reopens it in place.
    fn sign_saved_document_now(&mut self, pending: PendingSignature, cx: &mut Context<Self>) {
        let document_id = pending.document_id;
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let source = session.read(cx).path.clone();
        // Without Windows' `\\?\` prefix, which file dialogs read poorly.
        let directory = source
            .parent()
            .map(|directory| dunce::simplified(directory).to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".to_owned());
        // Review builds answer Save As from `BP_REVIEW_SAVE_AS`, as a
        // scripted review cannot reach the system's dialog.
        #[cfg(feature = "review-driver")]
        let scripted = std::env::var_os("BP_REVIEW_SAVE_AS").map(PathBuf::from);
        #[cfg(not(feature = "review-driver"))]
        let scripted: Option<PathBuf> = None;
        let picker = scripted
            .is_none()
            .then(|| cx.prompt_for_new_path(&directory, Some(&format!("{stem} (signed).pdf"))));
        self.signing_documents.insert(document_id);
        cx.spawn(async move |entity, cx| {
            let chosen = match (scripted, picker) {
                (Some(path), _) => Some(path),
                (None, Some(picker)) => match picker.await {
                    Ok(Ok(Some(path))) => Some(path),
                    _ => None,
                },
                (None, None) => None,
            };
            let Some(destination) = chosen else {
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.signing_documents.remove(&document_id);
                    workspace
                        .annotation_statuses
                        .insert(document_id, "Not signed.".into());
                    cx.notify();
                });
                return;
            };
            let _ = entity.update(cx, |workspace, cx| {
                workspace.sign_into(pending, source, destination, cx);
            });
        })
        .detach();
    }

    fn sign_into(
        &mut self,
        pending: PendingSignature,
        source: PathBuf,
        destination: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let document_id = pending.document_id;
        let same_file = normalized_document_path(&source) == normalized_document_path(&destination);
        let index = self
            .sessions
            .iter()
            .position(|session| session.read(cx).id == document_id);
        // Signing over the PDF itself: nothing may hold the file.
        if same_file && !self.close_document(document_id, cx) {
            self.signing_documents.remove(&document_id);
            return;
        }
        let options = SignOptions {
            reason: pending.reason,
            location: None,
            appearance: match pending.placement {
                Placement::Box { .. } if pending.look == SignatureLook::Invisible => None,
                Placement::Box { page, rect } => Some(SignatureAppearance {
                    page,
                    rect,
                    details_only: pending.look == SignatureLook::Details,
                    ..SignatureAppearance::default()
                }),
                Placement::Selected(placed) => Some(SignatureAppearance {
                    page: placed.page,
                    rect: placed.rect,
                    image: Some(placed.image),
                    replaces: Some(placed.id.as_str().to_owned()),
                    ..SignatureAppearance::default()
                }),
            },
            // With a timestamp, also the revocation data that keeps the
            // signature checkable after its certificates expire.
            long_term: pending.timestamp_server.is_some(),
            timestamp_server: pending.timestamp_server,
        };
        let identity = pending.identity;
        let signer = identity.signer_name();
        let background = cx.background_executor().clone();
        let written = destination.clone();
        cx.spawn(async move |entity, cx| {
            let result = background
                .spawn(async move { sign_file(&source, &written, &identity, &options) })
                .await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.signing_documents.remove(&document_id);
                // The signed copy opens; when signing failed, only the PDF
                // that was closed to sign it in place opens again.
                let opened =
                    (result.is_ok() || same_file).then(|| workspace.open_path(destination, cx));
                if same_file
                    && let Some(index) = index
                    && let Some(opened) = opened
                    && let Some(position) = workspace
                        .sessions
                        .iter()
                        .position(|session| session.read(cx).id == opened)
                {
                    let session = workspace.sessions.remove(position);
                    workspace
                        .sessions
                        .insert(index.min(workspace.sessions.len()), session);
                }
                match result {
                    Ok(()) => {
                        if let Some(opened) = opened {
                            workspace.annotation_statuses.insert(
                                opened,
                                if signer.is_empty() {
                                    "Signed.".into()
                                } else {
                                    format!("Signed by {signer}.")
                                },
                            );
                        }
                    }
                    Err(error) => workspace.signing_failed = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Says why signing failed; the PDF is unchanged.
    pub(super) fn open_signing_failed_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(error) = self.signing_failed.take() else {
            return;
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .close_button(true)
                .title("Could not sign the PDF")
                .description(error.clone())
        });
    }
}

/// Signs the PDF at `source` into `destination` (which may be the same
/// file), replacing it only once the signed copy is complete.
fn sign_file(
    source: &Path,
    destination: &Path,
    identity: &SigningIdentity,
    options: &SignOptions,
) -> Result<(), String> {
    let pdf =
        std::fs::read(source).map_err(|error| format!("The PDF could not be read: {error}."))?;
    let signed = sign_pdf(&pdf, identity, options).map_err(|error| error.to_string())?;
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let partial = destination.with_file_name(format!(".{name}.signing"));
    let written = match std::fs::write(&partial, &signed) {
        // A file chosen in a macOS Save panel may be written while its
        // folder (Downloads, Documents, the Desktop) refuses a new file
        // beside it: write the signed copy in place instead.
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            let _ = std::fs::remove_file(&partial);
            return std::fs::write(destination, &signed).map_err(|error| {
                if error.kind() != std::io::ErrorKind::PermissionDenied {
                    let _ = std::fs::remove_file(destination);
                }
                format!("The signed PDF could not be saved: {error}.")
            });
        }
        written => written,
    };
    written
        .and_then(|()| std::fs::rename(&partial, destination))
        .map_err(|error| {
            let _ = std::fs::remove_file(&partial);
            format!("The signed PDF could not be saved: {error}.")
        })
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}
