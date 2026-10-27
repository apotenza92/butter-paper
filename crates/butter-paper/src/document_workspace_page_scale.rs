//! Opening the Page Scale dialog for a page, feeding it the page's text,
//! and the calibration line's clicks and pointer moves
//! ([`crate::page_scale_control`]).

use super::*;
use crate::page_scale_control::{CalibrationPreview, PageScaleRequest};
use gpui_component::Sizable as _;

impl DocumentWorkspace {
    /// Opens the Page Scale dialog for `page_index` of the document.
    pub fn open_page_scale_dialog(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.page_scale_request(document_id, page_index, cx) else {
            return;
        };
        self.ensure_page_scale_control(window, cx)
            .update(cx, |control, cx| control.open(request, window, cx));
        self.feed_page_scale_text(window, cx);
    }

    /// What the dialog needs to know about a page.
    fn page_scale_request(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Option<PageScaleRequest> {
        let session = self.session(document_id, cx)?.read(cx);
        let page_size = session.page_size(page_index)?;
        Some(PageScaleRequest {
            document_id,
            page_index,
            page_count: u32::try_from(session.page_count()).unwrap_or(u32::MAX),
            page_size: (f64::from(page_size.0), f64::from(page_size.1)),
            current: session
                .annotations
                .document_page_scale(document_id.value(), page_index)
                .cloned(),
        })
    }

    /// Hands the dialog its page's text once read, to find the scales the
    /// page states.
    pub(super) fn feed_page_scale_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(control) = self.page_scale_control.clone() else {
            return;
        };
        let Some((document_id, page_index)) = control.read(cx).target() else {
            return;
        };
        if !control
            .read(cx)
            .wants_stated_scales(document_id, page_index)
        {
            return;
        }
        let text = match self.page_text_layer(document_id, page_index, cx) {
            // One line per line: a scale is never split across lines.
            Some(layer) => layer
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            // A page whose text cannot be read states no scale.
            None if self
                .page_text
                .get(&document_id)
                .is_some_and(|state| state.failed(page_index)) =>
            {
                String::new()
            }
            None => return,
        };
        control.update(cx, |control, cx| {
            control.set_page_text(document_id, page_index, &text, window, cx)
        });
    }

    /// Where a point of a painted page is in the window: for scripted
    /// review, which clicks in page coordinates.
    pub fn page_point_in_window(
        &self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
    ) -> Option<gpui::Point<Pixels>> {
        let interaction = self.page_interactions.get(&(document_id, page_index))?;
        let local = interaction.transform.point_to_local_pixels(point);
        Some(gpui::point(
            interaction.bounds.origin.x + px(local.x as f32),
            interaction.bounds.origin.y + px(local.y as f32),
        ))
    }

    pub(super) fn page_scale_picking(&self, cx: &App) -> bool {
        self.page_scale_control
            .as_ref()
            .is_some_and(|control| control.read(cx).is_picking())
    }

    pub(super) fn calibration_preview(&self, cx: &App) -> Option<CalibrationPreview> {
        self.page_scale_control
            .as_ref()
            .and_then(|control| control.read(cx).preview())
    }

    /// A calibration point at `point`: snapped to the page's drawing.
    fn calibration_point(
        &self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> PdfPoint {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return point;
        };
        session.update(cx, |session, _| {
            session
                .annotations
                .resolve_calibration_point(document_id.value(), page_index, point)
        })
    }

    /// A click while a calibration line is picked.
    pub fn press_calibration_point(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        constrain: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(control) = self.page_scale_control.clone() else {
            return;
        };
        let point = self.calibration_point(document_id, page_index, point, cx);
        // The first end may be on another page than the dialog's: the line
        // is measured, and the scale set, where it is drawn.
        if control.read(cx).picking_from(document_id, page_index)
            && let Some(request) = self.page_scale_request(document_id, page_index, cx)
        {
            control.update(cx, |control, cx| control.retarget(request, cx));
        }
        let disposition = control.update(cx, |control, cx| {
            control.record_point(document_id, page_index, point, constrain, window, cx)
        });
        match disposition {
            CalibrationPointDisposition::Ignored => {
                self.annotation_statuses.insert(
                    document_id,
                    "Click both ends on the page the scale is for.".into(),
                );
            }
            CalibrationPointDisposition::FirstPoint => {
                self.annotation_statuses.remove(&document_id);
            }
            CalibrationPointDisposition::Completed => {
                self.annotation_statuses.remove(&document_id);
                self.clear_calibration_snap(document_id, cx);
            }
        }
        cx.notify();
    }

    /// The pointer moving while a calibration line is picked.
    pub(super) fn hover_calibration_point(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        constrain: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(control) = self.page_scale_control.clone() else {
            return;
        };
        let point = self.calibration_point(document_id, page_index, point, cx);
        control.update(cx, |control, cx| {
            control.hover_point(document_id, page_index, point, constrain, cx)
        });
        cx.notify();
    }

    pub(super) fn clear_calibration_snap(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                session.annotations.clear_semantic_snap_decision();
                cx.notify();
            });
        }
    }

    /// What to click while a calibration line is picked, floating over the
    /// top of the page so the page does not move.
    pub(super) fn render_calibration_hint(
        &self,
        instruction: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        gpui::div()
            .absolute()
            .top_3()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .px_3()
            .child(
                h_flex()
                    .id(PAGE_SCALE_PICK_HINT_ID)
                    .debug_selector(|| PAGE_SCALE_PICK_HINT_ID.into())
                    // A press on the hint never reaches the page under it.
                    .occlude()
                    .max_w_full()
                    .gap_3()
                    .items_center()
                    .pl_3()
                    .pr_1()
                    .py_1()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover)
                    .text_color(cx.theme().popover_foreground)
                    .shadow_md()
                    .text_sm()
                    .child(gpui::div().flex_none().font_semibold().child("Calibrate"))
                    .child(gpui::div().min_w_0().truncate().child(instruction))
                    .child(
                        Button::new(PAGE_SCALE_PICK_CANCEL_ID)
                            .debug_selector(|| PAGE_SCALE_PICK_CANCEL_ID.into())
                            .label("Back")
                            .small()
                            .ghost()
                            .tooltip("Back to Page Scale (Esc)")
                            .on_click(cx.listener(|workspace, _, window, cx| {
                                workspace.cancel_calibration_pick(window, cx);
                            })),
                    ),
            )
    }

    /// Escape or Back while a line is picked: the dialog returns as it was.
    pub(super) fn cancel_calibration_pick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(control) = self.page_scale_control.clone() else {
            return false;
        };
        let document_id = control
            .read(cx)
            .target()
            .map(|(document_id, _)| document_id);
        if !control.update(cx, |control, cx| control.cancel_pick(window, cx)) {
            return false;
        }
        if let Some(document_id) = document_id {
            self.annotation_statuses.remove(&document_id);
            self.clear_calibration_snap(document_id, cx);
        }
        cx.notify();
        true
    }
}
