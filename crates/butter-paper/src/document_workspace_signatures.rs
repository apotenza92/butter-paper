//! A signed document's digital signatures: checked in the background when
//! it opens and after each save ([`crate::pdf_signatures`]), summarised by
//! the toolbar's signatures button and listed in the Signatures dialog.

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use super::*;
use crate::pdf_signatures::SignatureStatus;
use crate::pdf_signatures::{
    Changes, Integrity, Revocation, SignatureReport, Trust, Verdict, document_verdict,
    signatures_checked, system_trusted_roots,
};
use crate::signature_trust::TrustStore;
use gpui::div;
use gpui_component::Icon;

pub const DOCUMENT_SIGNATURES_DIALOG_ID: &str = "document-signatures-dialog";
pub const DOCUMENT_SIGNATURES_LIST_ID: &str = "document-signatures-list";

/// How a line about a signature reads: a plain fact, a doubt about who
/// signed, or a broken signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LineTone {
    Note,
    Warning,
    Problem,
}

/// The signatures of a signed document, `None` while they are checked.
pub(super) struct SignatureCheck {
    generation: u64,
    pub(super) reports: Option<Vec<SignatureReport>>,
}

/// Where signatures are checked against and whether Adobe's trust lists
/// were refreshed this run.
#[derive(Clone, Default)]
pub(super) struct SignatureTrust {
    store: Option<Arc<TrustStore>>,
    refreshed: Arc<AtomicBool>,
}

impl DocumentWorkspace {
    /// Checks signatures against this computer's roots and Adobe's trust
    /// lists kept in `store` (refreshed once a run, when a signed document
    /// first opens).
    pub fn set_signature_trust_store(&mut self, store: Arc<TrustStore>) {
        self.signature_trust.store = Some(store);
    }

    /// The checked signatures of `document_id`: `None` for an unsigned
    /// document or while they are checked.
    pub fn document_signatures(&self, document_id: DocumentId) -> Option<&[SignatureReport]> {
        self.signature_checks
            .get(&document_id)
            .and_then(|check| check.reports.as_deref())
    }

    pub(super) fn signature_status(&self, document_id: DocumentId) -> Option<SignatureStatus> {
        let check = self.signature_checks.get(&document_id)?;
        match &check.reports {
            None => Some(SignatureStatus::Checking),
            Some(reports) => document_verdict(reports).map(SignatureStatus::Checked),
        }
    }

    /// Checks `document_id`'s signatures again from its file, when the PDF
    /// is signed.
    pub(super) fn check_document_signatures(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (signed, path) = {
            let session = session.read(cx);
            (
                session
                    .resource
                    .as_ref()
                    .is_some_and(|resource| resource.digitally_signed()),
                session.path.clone(),
            )
        };
        // A document signed before this save keeps its (now broken)
        // signatures in view.
        if !signed && !self.signature_checks.contains_key(&document_id) {
            return;
        }
        let generation = self
            .signature_checks
            .get(&document_id)
            .map_or(1, |check| check.generation + 1);
        self.signature_checks.insert(
            document_id,
            SignatureCheck {
                generation,
                reports: None,
            },
        );
        self.sync_active_viewer_toolbar(cx);
        let trust = self.signature_trust.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            // First what the PDF itself holds, at once; then, in the app,
            // the certificates' issuers are asked about revocation.
            let online = trust.store.is_some();
            let check = {
                let path = path.clone();
                let store = trust.store.clone();
                move |ask: bool| {
                    let pdf = std::fs::read(&path).ok()?;
                    let roots = match &store {
                        Some(store) => store.roots(),
                        None => system_trusted_roots().to_vec(),
                    };
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_secs() as i64);
                    let fetch: &dyn Fn(&str, Option<&[u8]>) -> Option<Vec<u8>> =
                        &crate::signature_trust::fetch_revocation;
                    Some(signatures_checked(&pdf, &roots, now, ask.then_some(fetch)))
                }
            };
            let offline = check.clone();
            let reports = background.spawn(async move { offline(false) }).await;
            let unanswered = reports.as_ref().is_some_and(|reports| {
                reports
                    .iter()
                    .any(|report| report.certificate.revocation == Revocation::NotChecked)
            });
            let applied = entity
                .update(cx, |workspace, cx| {
                    workspace.apply_signature_check(document_id, generation, reports, cx)
                })
                .unwrap_or(false);
            if !applied {
                return;
            }
            if online && unanswered {
                let reports = background.spawn(async move { check(true) }).await;
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.apply_signature_check(document_id, generation, reports, cx)
                });
            }
            // Adobe's lists hold the document-signing roots system stores
            // leave out: fetch them once a run, then check every signed
            // document again when they changed.
            let Some(store) = trust.store else {
                return;
            };
            if trust.refreshed.swap(true, AtomicOrdering::AcqRel) {
                return;
            }
            let replaced = background
                .spawn(async move { store.refresh() })
                .await
                .unwrap_or_else(|error| {
                    eprintln!("Adobe's trust lists could not be refreshed: {error}");
                    false
                });
            if replaced {
                let _ = entity.update(cx, |workspace, cx| {
                    let documents: Vec<DocumentId> =
                        workspace.signature_checks.keys().copied().collect();
                    for document_id in documents {
                        workspace.check_document_signatures(document_id, cx);
                    }
                });
            }
        })
        .detach();
    }

    fn apply_signature_check(
        &mut self,
        document_id: DocumentId,
        generation: u64,
        reports: Option<Vec<SignatureReport>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(check) = self.signature_checks.get_mut(&document_id) else {
            return false;
        };
        if check.generation != generation {
            return false;
        }
        // An unreadable file shows no signatures rather than a false alarm.
        check.reports = Some(reports.unwrap_or_default());
        self.sync_active_viewer_toolbar(cx);
        cx.notify();
        true
    }

    pub(super) fn forget_document_signatures(&mut self, document_id: DocumentId) {
        self.signature_checks.remove(&document_id);
        if self.signatures_dialog_requested == Some(document_id) {
            self.signatures_dialog_requested = None;
        }
    }

    pub(super) fn request_signatures_dialog(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        if self.document_signatures(document_id).is_some() {
            self.signatures_dialog_requested = Some(document_id);
            cx.notify();
        }
    }

    /// Lists the active document's signatures: who signed, when, whether
    /// the PDF changed since, and whether the signer's certificate is
    /// trusted. The top-right X or Escape closes it.
    pub(super) fn open_signatures_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.signatures_dialog_requested.take() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(reports) = self.document_signatures(document_id).map(<[_]>::to_vec) else {
            return;
        };
        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let theme = cx.theme();
            let rows = reports.iter().enumerate().map(|(index, report)| {
                signature_row(document_id, index, report, workspace.clone(), theme)
            });
            let summary = match document_verdict(&reports) {
                Some(Verdict::Valid) => "Every signature is valid.",
                Some(Verdict::Unverified) => {
                    "The signed content is unchanged, but a signer's identity can't be verified."
                }
                Some(Verdict::Invalid) | None => {
                    "This PDF changed after it was signed, so a signature is invalid."
                }
            };
            dialog
                .title(if reports.len() == 1 {
                    "Signature"
                } else {
                    "Signatures"
                })
                .w(px(520.))
                .max_w_full()
                .close_button(true)
                .child(
                    v_flex()
                        .id(DOCUMENT_SIGNATURES_DIALOG_ID)
                        .debug_selector(|| DOCUMENT_SIGNATURES_DIALOG_ID.into())
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(summary),
                        )
                        .child(
                            v_flex()
                                .id(DOCUMENT_SIGNATURES_LIST_ID)
                                .debug_selector(|| DOCUMENT_SIGNATURES_LIST_ID.into())
                                // The dialog's body scrolls a long list, within
                                // the window: no second scroller inside it.
                                .w_full()
                                .rounded(theme.radius)
                                .border_1()
                                .border_color(theme.border)
                                .children(rows),
                        )
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            "Certificates are checked against this computer's trusted \
                             authorities and Adobe's trust lists, and their issuers are asked \
                             whether they were revoked.",
                        )),
                )
        });
    }
}

fn signature_row(
    document_id: DocumentId,
    index: usize,
    report: &SignatureReport,
    workspace: WeakEntity<DocumentWorkspace>,
    theme: &gpui_component::Theme,
) -> impl IntoElement {
    let verdict = report.verdict();
    let (icon, color) = match verdict {
        Verdict::Valid => (
            crate::application_assets::SIGNATURE_VALID_ICON,
            theme.success,
        ),
        Verdict::Unverified => (
            crate::application_assets::SIGNATURE_UNVERIFIED_ICON,
            theme.warning,
        ),
        Verdict::Invalid => (
            crate::application_assets::SIGNATURE_INVALID_ICON,
            theme.danger,
        ),
    };
    let signer = if report.signer.is_empty() {
        "Unknown signer".to_owned()
    } else {
        report.signer.clone()
    };
    let heading = if report.document_timestamp {
        format!("Document timestamp by {signer}")
    } else {
        signer
    };
    // The organisation, unless it only repeats the signer's name.
    let organization = report
        .organization
        .clone()
        .filter(|organization| !organization.eq_ignore_ascii_case(&report.signer));
    let lines = signature_lines(report);
    let page = report.page;
    v_flex()
        .w_full()
        .gap_1()
        .p_3()
        .when(index > 0, |row| row.border_t_1().border_color(theme.border))
        .child(
            // The page button sits in the heading so it stays in view, and
            // focus never lands on a control scrolled out of sight.
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::default().path(icon).text_color(color).small())
                .child(div().flex_shrink_0().font_semibold().child(heading))
                .when_some(organization, |row, organization| {
                    row.child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(organization),
                    )
                })
                .child(div().flex_1())
                .when_some(page, |row, page| {
                    row.child(
                        Button::new(("document-signature-page", index))
                            .outline()
                            .small()
                            .label(format!("Show on page {}", page + 1))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                let _ = workspace.update(cx, |workspace, cx| {
                                    workspace.activate_thumbnail_page(document_id, page, cx);
                                });
                            }),
                    )
                }),
        )
        .children(lines.into_iter().map(|(tone, line)| {
            div()
                .text_sm()
                .text_color(match tone {
                    LineTone::Note => theme.muted_foreground,
                    // The amber shield marks a doubt; amber text is too
                    // faint to read on the dialog.
                    LineTone::Warning => theme.foreground,
                    LineTone::Problem => theme.danger,
                })
                .child(line)
        }))
}

/// What to say about one signature, each line flagged when it is a
/// problem.
pub(super) fn signature_lines(report: &SignatureReport) -> Vec<(LineTone, String)> {
    let mut lines = Vec::new();
    match (&report.integrity, &report.changes) {
        (Integrity::Altered, _) => lines.push((
            LineTone::Problem,
            "The signed content was altered, or the signature is damaged.".to_owned(),
        )),
        (Integrity::Unsupported(reason), _) => lines.push((
            LineTone::Warning,
            if reason.is_empty() {
                "This signature can't be checked.".to_owned()
            } else {
                format!("This signature can't be checked: {reason}.")
            },
        )),
        (Integrity::Intact, Changes::Content) => lines.push((
            LineTone::Problem,
            "The pages were changed after this signature.".to_owned(),
        )),
        (Integrity::Intact, Changes::None)
        | (
            Integrity::Intact,
            Changes::Permitted {
                signatures: false,
                other: false,
            },
        ) => lines.push((
            LineTone::Note,
            "The PDF hasn't changed since this signature.".to_owned(),
        )),
        (
            Integrity::Intact,
            Changes::Permitted {
                signatures: true,
                other: false,
            },
        ) => lines.push((
            LineTone::Note,
            "Only further signatures were added after this one.".to_owned(),
        )),
        (Integrity::Intact, Changes::Permitted { other: true, .. }) => lines.push((
            LineTone::Note,
            "Notes, markups or form entries were added after this signature; its content is \
             unchanged."
                .to_owned(),
        )),
    }
    if !matches!(report.integrity, Integrity::Altered) && !report.certificate.chain.is_empty() {
        let issuer = report.certificate.chain.get(1).cloned();
        match (&report.certificate.trust, report.certificate.in_date) {
            (Trust::Trusted, true) => lines.push((
                LineTone::Note,
                match issuer {
                    Some(issuer) => format!("Certificate issued by {issuer}, a trusted authority."),
                    None => "The signer's certificate is trusted.".to_owned(),
                },
            )),
            (Trust::Trusted, false) => lines.push((
                LineTone::Warning,
                if report
                    .timestamp
                    .as_ref()
                    .is_some_and(|timestamp| timestamp.valid && timestamp.trusted)
                {
                    "The signer's certificate wasn't valid when it signed.".to_owned()
                } else {
                    "The signer's certificate isn't valid now, and no timestamp proves it was \
                     when it signed."
                        .to_owned()
                },
            )),
            (Trust::NotForSigning, _) => lines.push((
                LineTone::Warning,
                "The signer's certificate is trusted, but not for signing documents.".to_owned(),
            )),
            (Trust::Untrusted, _) => lines.push((
                LineTone::Warning,
                match issuer {
                    Some(issuer) => format!(
                        "The signer's identity can't be verified: {issuer} isn't a trusted \
                         authority."
                    ),
                    None => "The signer's identity can't be verified: the certificate is \
                             self-signed."
                        .to_owned(),
                },
            )),
        }
    }
    if !matches!(report.integrity, Integrity::Altered) && !report.certificate.chain.is_empty() {
        match report.certificate.revocation {
            Revocation::Good => lines.push((
                LineTone::Note,
                "Its certificate was not revoked when it signed.".to_owned(),
            )),
            Revocation::Revoked { at } => lines.push((
                LineTone::Problem,
                format!(
                    "Its certificate was revoked on {}, before it signed.",
                    local_time(at)
                ),
            )),
            Revocation::NotChecked => lines.push((
                LineTone::Note,
                "Whether its certificate was revoked could not be checked.".to_owned(),
            )),
        }
    }
    match &report.timestamp {
        Some(timestamp) if timestamp.valid && !timestamp.trusted => lines.push((
            LineTone::Warning,
            format!(
                "Timestamped {} by {}, which isn't a trusted timestamp authority, so when it \
                 was signed is unproven.",
                local_time(timestamp.time),
                if timestamp.authority.is_empty() {
                    "an unknown authority"
                } else {
                    &timestamp.authority
                }
            ),
        )),
        Some(timestamp) if timestamp.valid => lines.push((
            LineTone::Note,
            format!(
                "Signed {}, timestamped by {}.",
                local_time(timestamp.time),
                if timestamp.authority.is_empty() {
                    "a timestamp authority"
                } else {
                    &timestamp.authority
                }
            ),
        )),
        Some(_) => lines.push((
            LineTone::Warning,
            "Its timestamp doesn't check out, so when it was signed is unproven.".to_owned(),
        )),
        None => {
            if let Some(time) = report.claimed_time {
                lines.push((
                    LineTone::Note,
                    format!("Signed {}, by the signer's clock.", local_time(time)),
                ));
            }
        }
    }
    let details: Vec<String> = [
        report
            .reason
            .as_ref()
            .map(|reason| format!("Reason: {reason}")),
        report
            .location
            .as_ref()
            .map(|location| format!("Location: {location}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !details.is_empty() {
        lines.push((LineTone::Note, details.join(" · ")));
    }
    lines
}

/// `seconds` since 1970 in local time, as "31 Oct 2026 at 9:30 pm".
pub(super) fn local_time(seconds: i64) -> String {
    let local = seconds + crate::pdf_engine::local_utc_offset_seconds(seconds);
    let days = local.div_euclid(86_400);
    let time_of_day = local.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let hour = time_of_day / 3_600;
    let minute = time_of_day / 60 % 60;
    let (hour12, half) = match hour {
        0 => (12, "am"),
        1..=11 => (hour, "am"),
        12 => (12, "pm"),
        _ => (hour - 12, "pm"),
    };
    format!(
        "{day} {} {year} at {hour12}:{minute:02} {half}",
        MONTHS[(month - 1) as usize]
    )
}
