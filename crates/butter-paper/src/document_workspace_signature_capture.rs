//! Boxes drawn on a page for signatures, with the Snapshot tool: round a
//! handwritten signature to take it as one (From the page), or where a
//! digital signature goes (Sign with Certificate, as Acrobat and Revu ask).
//! Neither leaves a snapshot markup. Paste takes a signature from a picture
//! on the clipboard instead. Pictures are cleaned up as an imported image
//! is: the paper made transparent and cropped to the ink.

use super::*;
use crate::image_asset_decode::{sanitize_signature_bytes, sanitize_signature_rgba_pixels};

/// What the next Snapshot box is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BoxPurpose {
    /// Take the handwritten signature inside it.
    TakeSignature,
    /// Place a digital signature in it.
    PlaceDigitalSignature,
}

/// A box being drawn for a signature on a document.
pub(super) struct BoxCapture {
    pub(super) document_id: DocumentId,
    pub(super) purpose: BoxPurpose,
    /// The snapshots the document already had.
    known: HashSet<MarkupId>,
}

impl DocumentWorkspace {
    /// Arms the Snapshot tool so its next box serves `purpose`; the status
    /// bar says what the box is for until it is drawn or cancelled.
    pub(super) fn begin_signature_box(
        &mut self,
        document_id: DocumentId,
        purpose: BoxPurpose,
        cx: &mut Context<Self>,
    ) {
        let known: HashSet<MarkupId> = self
            .annotation_snapshot(document_id, cx)
            .map(|snapshot| {
                snapshot
                    .snapshots
                    .iter()
                    .map(|snapshot| snapshot.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        if self
            .set_annotation_tool(document_id, AnnotationTool::Snapshot, cx)
            .is_err()
        {
            return;
        }
        self.signature_box = Some(BoxCapture {
            document_id,
            purpose,
            known,
        });
        cx.notify();
    }

    /// From the page: the panel steps aside for a box round a signature.
    pub(super) fn begin_signature_capture(
        &mut self,
        document_id: DocumentId,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_signature_popover(document_id, window, cx);
        self.begin_signature_box(document_id, BoxPurpose::TakeSignature, cx);
    }

    /// The status bar's guidance while a box is drawn for a signature.
    pub(super) fn signature_box_guidance(&self) -> Option<&'static str> {
        let capture = self
            .signature_box
            .as_ref()
            .filter(|capture| Some(capture.document_id) == self.active_document_id)?;
        Some(match capture.purpose {
            BoxPurpose::TakeSignature => {
                "Signature from the page · Click or drag a box around a signature · Esc to cancel"
            }
            BoxPurpose::PlaceDigitalSignature => {
                "Sign with Certificate · Click or drag a box where the signature goes · Esc to cancel"
            }
        })
    }

    /// The prompt over the page while a box is drawn for a signature: what
    /// to draw, and an X that stops. The status bar says it too, but a
    /// command chosen from a menu needs an answer where the user is looking.
    pub(super) fn render_signature_box_prompt(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let capture = self
            .signature_box
            .as_ref()
            .filter(|capture| Some(capture.document_id) == self.active_document_id)?;
        let document_id = capture.document_id;
        let text = match capture.purpose {
            BoxPurpose::TakeSignature => "Drag a box around the signature on the page",
            BoxPurpose::PlaceDigitalSignature => "Drag a box where the signature goes",
        };
        let bounds = self.signature_box_prompt_bounds.clone();
        let theme = cx.theme();
        Some(
            gpui::div()
                .absolute()
                .top_3()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    h_flex()
                        .id(DOCUMENT_SIGNATURE_BOX_PROMPT_ID)
                        .debug_selector(|| DOCUMENT_SIGNATURE_BOX_PROMPT_ID.into())
                        .relative()
                        .gap_2()
                        .pl_3()
                        .pr_1()
                        .py_1()
                        .max_w_full()
                        .rounded(theme.radius_lg)
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .text_color(theme.popover_foreground)
                        .shadow_md()
                        .text_sm()
                        .cursor(CursorStyle::Arrow)
                        .child(
                            gpui::canvas(move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {})
                                .absolute()
                                .inset_0(),
                        )
                        .child(gpui::div().min_w_0().truncate().child(text))
                        .child(
                            crate::accessible_button::close_icon_button(
                                DOCUMENT_SIGNATURE_BOX_CANCEL_ID,
                                "Stop drawing the box",
                            )
                            .on_click(cx.listener(
                                move |workspace, _: &ClickEvent, _, cx| {
                                    workspace.cancel_signature_box(document_id, cx);
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Whether a press at `position` is on that prompt.
    pub(super) fn signature_box_prompt_owns(&self, position: gpui::Point<Pixels>) -> bool {
        self.signature_box
            .as_ref()
            .is_some_and(|capture| Some(capture.document_id) == self.active_document_id)
            && self
                .signature_box_prompt_bounds
                .get()
                .is_some_and(|bounds| bounds.contains(&position))
    }

    /// Stops drawing a box for a signature, as Esc does.
    pub(super) fn cancel_signature_box(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        self.signature_box = None;
        self.signature_box_prompt_bounds.set(None);
        let _ = self.set_annotation_tool(document_id, AnnotationTool::Select, cx);
        cx.notify();
    }

    /// Takes the box once the Snapshot tool has drawn it; stops waiting
    /// when another tool is chosen.
    pub(super) fn poll_signature_capture(&mut self, cx: &mut Context<Self>) {
        let Some(capture) = self.signature_box.as_ref() else {
            return;
        };
        let document_id = capture.document_id;
        let purpose = capture.purpose;
        let drawn = self
            .annotation_snapshot(document_id, cx)
            .and_then(|snapshot| {
                snapshot
                    .snapshots
                    .iter()
                    .find(|snapshot| !capture.known.contains(&snapshot.id))
                    .map(|snapshot| (snapshot.page_index, snapshot.rect, snapshot.asset().clone()))
            });
        let Some((page, rect, asset)) = drawn else {
            if self.annotation_tool(document_id, cx) != Some(AnnotationTool::Snapshot) {
                self.signature_box = None;
            }
            return;
        };
        self.signature_box = None;
        // The box was for a signature, not a snapshot markup.
        let _ = self.undo_annotations(document_id, cx);
        let _ = self.set_annotation_tool(document_id, AnnotationTool::Select, cx);
        match purpose {
            BoxPurpose::TakeSignature => {
                let result = sanitize_signature_rgba_pixels(
                    asset.width_px(),
                    asset.height_px(),
                    asset.rgba().to_vec(),
                )
                .map_err(|error| match error {
                    crate::image_asset_decode::SignatureImageError::NoInk => {
                        "That box has no signature in it. Choose From the page again and draw \
                         the box round the signature."
                            .to_owned()
                    }
                    crate::image_asset_decode::SignatureImageError::ExcessiveCoverage => {
                        "That box holds more than a signature. Choose From the page again and \
                         draw the box closer round it."
                            .to_owned()
                    }
                    error => error.to_string(),
                });
                self.show_captured_signature(result, cx);
            }
            BoxPurpose::PlaceDigitalSignature => {
                self.open_sign_dialog_for_box(
                    document_id,
                    page,
                    [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height],
                    cx,
                );
            }
        }
    }

    /// Paste: the picture on the clipboard, such as a screenshot.
    pub(super) fn paste_signature(&mut self, cx: &mut Context<Self>) {
        let image = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                gpui::ClipboardEntry::Image(image) => Some(image.bytes.clone()),
                _ => None,
            })
        });
        let result = match image {
            Some(bytes) => sanitize_signature_bytes(&bytes).map_err(|error| error.to_string()),
            None => Err(
                "Copy a picture of your signature first, such as a screenshot of it.".to_owned(),
            ),
        };
        self.drawn_signature.clear();
        self.show_captured_signature(result, cx);
    }

    /// Shows a captured signature in the Signature panel, ready to place.
    fn show_captured_signature(
        &mut self,
        result: Result<SanitizedSignatureFile, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.signature_popover_open {
            self.signature_popover_open = true;
            self.load_recent_signatures(cx);
        }
        self.signature_input_mode = SignatureInputMode::Image;
        self.signature_prepare_state = match result {
            Err(error) => SignaturePrepareState::Error(error),
            Ok(sanitized) => {
                let asset = sanitized.into_asset();
                match crate::document_session::decoded_asset_render_image(&asset, 1.) {
                    Some(image) => {
                        SignaturePrepareState::Preview(SignaturePreview { asset, image })
                    }
                    None => SignaturePrepareState::Error("Unable to process this image.".into()),
                }
            }
        };
        cx.notify();
    }
}
