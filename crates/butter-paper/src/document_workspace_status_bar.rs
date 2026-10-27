//! Full-width contextual guidance, outside the document's scroll and sidebars.
use super::*;

pub const DOCUMENT_STATUS_SCALE_ID: &str = "document-status-scale";

impl DocumentWorkspace {
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
                AnnotationTool::Cloud => {
                    "Drag a rectangle or click polygon corners · Enter to finish"
                }
                AnnotationTool::CloudPlus => {
                    "Drag a rectangle or click polygon corners · Enter to finish · Then type text"
                }
                AnnotationTool::Length | AnnotationTool::Dimension => {
                    "Click two points · Esc to cancel"
                }
                AnnotationTool::Arc => "Click start, end and bulge · Esc to cancel",
                AnnotationTool::TextBox => "Click to place text · Click away to finish",
                AnnotationTool::Callout => "Click twice or drag the leader · Then type text",
                AnnotationTool::Image => "Click to place image · Esc to cancel",
                AnnotationTool::Rectangle
                | AnnotationTool::Ellipse
                | AnnotationTool::Line
                | AnnotationTool::Arrow
                | AnnotationTool::Redact
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
                    | AnnotationTool::Snapshot,
                ) => Some(
                    "Drag between two points and release, or click the first point, move the pointer, then click the second point.",
                ),
                Some(AnnotationTool::Cloud | AnnotationTool::CloudPlus) => Some(
                    "Drag and release to draw a rectangular cloud. For a polygon, click each corner, then press Enter, double-click, or click the starting point to finish. Escape finishes a valid polygon or cancels an incomplete one. Cloud+ then opens its text editor.",
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
            .child(
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
