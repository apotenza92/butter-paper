//! Full-width contextual guidance, outside the document's scroll and sidebars.
use super::*;

pub const DOCUMENT_STATUS_SCALE_ID: &str = "document-status-scale";
pub const DOCUMENT_STATUS_MESSAGE_ID: &str = "document-status-message";

/// How long a status message replaces the status bar's guidance.
const STATUS_MESSAGE_SHOWN: std::time::Duration = std::time::Duration::from_secs(6);

impl DocumentWorkspace {
    /// Notes a new status message for the active document (why text cannot
    /// be edited, a conversion's progress or failure) so the status bar
    /// shows it, then clears it once it has been shown long enough, so the
    /// same message shows again the next time it is set.
    pub(super) fn refresh_status_message(&mut self, cx: &mut Context<Self>) {
        let current = self.active_document_id.and_then(|document_id| {
            Some((
                document_id,
                self.annotation_statuses.get(&document_id)?.clone(),
            ))
        });
        let Some((document_id, text)) = current else {
            self.status_message = None;
            return;
        };
        if self
            .status_message
            .as_ref()
            .is_some_and(|(shown, shown_text, _)| *shown == document_id && *shown_text == text)
        {
            return;
        }
        self.status_message = Some((document_id, text.clone(), std::time::Instant::now()));
        cx.spawn(async move |workspace, cx| {
            cx.background_executor().timer(STATUS_MESSAGE_SHOWN).await;
            let _ = workspace.update(cx, |workspace, cx| {
                if workspace.annotation_statuses.get(&document_id) == Some(&text) {
                    workspace.annotation_statuses.remove(&document_id);
                }
                if workspace
                    .status_message
                    .as_ref()
                    .is_some_and(|(_, shown_text, _)| *shown_text == text)
                {
                    workspace.status_message = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_status_bar(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tool = self
            .active_document_id
            .and_then(|id| self.session(id, cx))
            .map(|session| session.read(cx).annotations.tool());
        let editing_content = self
            .active_document_id
            .is_some_and(|id| self.editing_content(id, cx));
        let editing_text = self
            .page_text_editing
            .is_some_and(|editing| Some(editing.document_id) == self.active_document_id);
        let context = if self.page_scale_picking(cx) {
            "Calibrate · Click both ends of a length you know · Shift keeps it straight · Esc to go back"
                .to_owned()
        } else if self.pending_text_box_editor.is_some() {
            "Text Box · Click away to finish".to_owned()
        } else if editing_content && editing_text {
            "Edit PDF · Type to edit · Esc or click away to finish".to_owned()
        } else if editing_content {
            "Edit PDF · Click text to edit it · Drag its edge, a drawing or an image to move it"
                .to_owned()
        } else if self.pan_tool_active {
            "Pan · Drag to move the page".to_owned()
        } else if let Some(tool) = tool {
            let help = match tool {
                AnnotationTool::Select => {
                    "Click to select · Drag to lasso · Click empty space to start a box"
                }
                AnnotationTool::Polyline
                | AnnotationTool::Polygon
                | AnnotationTool::Polylength
                | AnnotationTool::Area => {
                    "Click points · Enter or double-click to finish · Esc to cancel"
                }
                AnnotationTool::CloudPlus => "Click or drag to draw · Then type text",
                AnnotationTool::Length | AnnotationTool::Dimension => {
                    "Click or drag between two points · Esc to cancel"
                }
                AnnotationTool::Arc => "Click start, end and bulge · Esc to cancel",
                AnnotationTool::TextBox => "Click or drag to draw a box · Then type text",
                AnnotationTool::Callout => "Click twice or drag the leader · Then type text",
                AnnotationTool::Image => "Click or drag to place image · Esc to cancel",
                AnnotationTool::Rectangle
                | AnnotationTool::Ellipse
                | AnnotationTool::Line
                | AnnotationTool::Arrow
                | AnnotationTool::Redact
                | AnnotationTool::Cloud
                | AnnotationTool::Snapshot => "Click or drag to draw · Esc to cancel",
                AnnotationTool::Pen | AnnotationTool::Highlight => {
                    "Drag to draw · Release to finish · Esc to cancel"
                }
            };
            format!("{} · {help}", tool.label())
        } else {
            "Open a PDF or create a document to start".to_owned()
        };
        let wide = window.viewport_size().width >= window.rem_size() * 65.;
        let navigation = if wide {
            "Hold Space + drag: pan · Two fingers: scroll · Pinch to zoom · Hold Ctrl while scrolling to zoom"
        } else {
            "Space + drag: pan · Pinch: zoom"
        };
        let details = if self.pan_tool_active || self.pending_text_box_editor.is_some() {
            None
        } else {
            match tool {
                Some(
                    AnnotationTool::Rectangle
                    | AnnotationTool::Ellipse
                    | AnnotationTool::Line
                    | AnnotationTool::Arrow
                    | AnnotationTool::Redact
                    | AnnotationTool::Cloud
                    | AnnotationTool::Length
                    | AnnotationTool::Snapshot,
                ) => Some(
                    "Drag between two points and release, or click the first point, move the pointer, then click the second point.",
                ),
                Some(AnnotationTool::CloudPlus) => Some(
                    "Drag and release to draw a rectangular cloud, or click one corner and then the other. Then type its text. For a cloud of any shape, turn on Cloud outline in the Polygon tool's properties.",
                ),
                Some(AnnotationTool::Polygon) => Some(
                    "Click each corner, then press Enter, double-click, or click the starting point to finish. With Cloud outline on in its properties, the polygon is drawn as a cloud.",
                ),
                Some(AnnotationTool::Dimension) => Some(
                    "Drag between two points and release, or click the first point and then the second. Then type the dimension text.",
                ),
                Some(AnnotationTool::TextBox) => Some(
                    "Drag the box for the text and release, or click one corner and then the other. Then type; the box grows to fit the text.",
                ),
                Some(AnnotationTool::Image) => Some(
                    "Drag the box for the image and release, or click one corner and then the other. The image fills the box, keeping its proportions.",
                ),
                Some(AnnotationTool::Callout) => Some(
                    "Drag from the arrow tip to the text position, or click those two points separately. Then type the callout text.",
                ),
                Some(AnnotationTool::Arc) => Some(
                    "Click the start and end of the arc, move the pointer to adjust the bulge, then click to finish.",
                ),
                _ => None,
            }
        };
        let context_tooltip = details.map_or_else(
            || context.clone(),
            |details| format!("{context}. {details}"),
        );
        // The current page's scale, shown once it has one or while measuring.
        let scale = self.active_document_id.and_then(|document_id| {
            let session = self.session(document_id, cx)?.read(cx);
            let page_index = session.current_page;
            let scale = session
                .annotations
                .document_page_scale(document_id.value(), page_index)
                .map(crate::page_scale_catalog::scale_label);
            let measuring = matches!(
                tool,
                Some(AnnotationTool::Length | AnnotationTool::Polylength | AnnotationTool::Area)
            );
            (scale.is_some() || measuring).then(|| {
                (
                    document_id,
                    page_index,
                    scale.unwrap_or_else(|| "No scale".to_owned()),
                )
            })
        });
        let navigation_tooltip = "Hold Space and drag to pan. Two-finger trackpad scrolling always scrolls. Pinch or Ctrl + trackpad scroll zooms. Mouse-wheel behaviour follows the view dropdown; Ctrl + wheel does the opposite.";
        let accessible_help = format!("{context_tooltip}. {navigation_tooltip}");
        let message = self
            .status_message
            .as_ref()
            .filter(|(document_id, _, shown)| {
                Some(*document_id) == self.active_document_id
                    && shown.elapsed() < STATUS_MESSAGE_SHOWN
            })
            .map(|(_, text, _)| text.clone());
        let showing_message = message.is_some();
        let accessible_help = match &message {
            Some(message) => format!("{message}. {accessible_help}"),
            None => accessible_help,
        };
        h_flex()
            .id("document-status-bar")
            .debug_selector(|| "document-status-bar".into())
            .role(Role::Status)
            .aria_label(accessible_help)
            .w_full()
            .flex_none()
            .min_w_0()
            .px_3()
            .py_1()
            .gap_4()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .when_some(message, |bar, message| {
                bar.child(
                    gpui::div()
                        .id(DOCUMENT_STATUS_MESSAGE_ID)
                        .debug_selector(|| DOCUMENT_STATUS_MESSAGE_ID.into())
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(cx.theme().foreground)
                        .tooltip({
                            let message = message.clone();
                            move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(message.clone())
                                    .build(window, cx)
                            }
                        })
                        .child(message),
                )
            })
            .when(!showing_message, |bar| {
                bar.child(
                    gpui::div()
                        .id("document-context-help")
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::element({
                                let text = context_tooltip.clone();
                                move |window, _| {
                                    gpui::div()
                                        .w((window.viewport_size().width - window.rem_size() * 4.)
                                            .min(window.rem_size() * 24.))
                                        .whitespace_normal()
                                        .child(text.clone())
                                }
                            })
                            .build(window, cx)
                        })
                        .child(context),
                )
            })
            .when_some(scale, |bar, (document_id, page_index, label)| {
                bar.child(
                    Button::new(DOCUMENT_STATUS_SCALE_ID)
                        .debug_selector(|| DOCUMENT_STATUS_SCALE_ID.into())
                        .flex_none()
                        .xsmall()
                        .ghost()
                        .icon(gpui_component::Icon::default().path("icons/rail/scan-line.svg"))
                        .label(label)
                        .tooltip("Page scale. Click to change it.")
                        .on_click(cx.listener(move |workspace, _, window, cx| {
                            workspace.open_page_scale_dialog(document_id, page_index, window, cx);
                        })),
                )
            })
            .child(
                gpui::div()
                    .id("document-navigation-help")
                    .flex_none()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::element(move |window, _| {
                            gpui::div()
                                .w((window.viewport_size().width - window.rem_size() * 4.)
                                    .min(window.rem_size() * 24.))
                                .whitespace_normal()
                                .child(navigation_tooltip)
                        })
                        .build(window, cx)
                    })
                    .child(navigation),
            )
    }
}
