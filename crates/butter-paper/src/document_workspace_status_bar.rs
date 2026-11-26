//! Full-width contextual guidance, outside the document's scroll and sidebars.
use super::*;

pub const DOCUMENT_STATUS_SCALE_ID: &str = "document-status-scale";
pub const DOCUMENT_STATUS_MESSAGE_ID: &str = "document-status-message";
/// The area toasts are laid out in: the canvas.
pub const DOCUMENT_TOAST_LAYER_ID: &str = "document-toast-layer";

/// The status bar while a calibration line is picked.
pub const CALIBRATE_STATUS: &str = "Calibrate · Esc to go back to Page Scale";
/// The status bar with page text highlighted: a click away only clears it.
pub const CLICK_OUT_TEXT_STATUS: &str =
    "Text selected · Click away to deselect · Shift-click to extend";
/// The status bar with markups selected: a click away only deselects them.
pub const CLICK_OUT_MARKUP_STATUS: &str =
    "Click another markup to select it · Click away to deselect · Shift-click to add or remove";

/// The status bar while text is typed in a field outside the page or in a
/// form field: a click on the page only finishes the typing.
pub const CLICK_OUT_TYPING_STATUS: &str = "Click away to finish typing";

/// How long a status message replaces the status bar's guidance.
const STATUS_MESSAGE_SHOWN: std::time::Duration = std::time::Duration::from_secs(6);

/// The modifier that draws a selection box from anywhere.
const BOX_HINT: &str = if cfg!(target_os = "macos") {
    "Command-drag: box anywhere"
} else {
    "Ctrl+drag: box anywhere"
};

impl DocumentWorkspace {
    /// Sets a document's status message, as `severity` says it matters: the
    /// status bar shows every message, and only a notice or an error is
    /// also a toast.
    pub(super) fn set_status(
        &mut self,
        document_id: DocumentId,
        text: String,
        severity: StatusSeverity,
    ) {
        self.annotation_statuses.insert(document_id, text);
        self.annotation_status_severities
            .insert(document_id, severity);
    }

    /// The severity of the active document's status message.
    pub fn annotation_status_severity(&self) -> Option<StatusSeverity> {
        let document_id = self.active_document_id?;
        self.annotation_statuses.get(&document_id)?;
        Some(
            self.annotation_status_severities
                .get(&document_id)
                .copied()
                .unwrap_or_default(),
        )
    }

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
            .is_some_and(|(shown, shown_text, _, _)| *shown == document_id && *shown_text == text)
        {
            return;
        }
        let severity = self
            .annotation_status_severities
            .get(&document_id)
            .copied()
            .unwrap_or_default();
        self.status_message = Some((
            document_id,
            text.clone(),
            std::time::Instant::now(),
            severity,
        ));
        cx.spawn(async move |workspace, cx| {
            cx.background_executor().timer(STATUS_MESSAGE_SHOWN).await;
            let _ = workspace.update(cx, |workspace, cx| {
                if workspace.annotation_statuses.get(&document_id) == Some(&text) {
                    workspace.annotation_statuses.remove(&document_id);
                }
                if workspace
                    .status_message
                    .as_ref()
                    .is_some_and(|(_, shown_text, _, _)| *shown_text == text)
                {
                    workspace.status_message = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The status bar's line while the Redact tool is chosen or a
    /// redaction mark is selected: what a mark does not do, in place of the
    /// tool help, so nothing above the page moves.
    pub(super) fn redaction_status_context(
        &self,
        tool: Option<AnnotationTool>,
        cx: &Context<Self>,
    ) -> Option<&'static str> {
        self.redact_pending_status_text(cx)?;
        Some(if tool == Some(AnnotationTool::Redact) {
            "Redact · Click or drag to mark · Marks cover content but don’t remove it from the PDF"
        } else {
            "Redaction mark · It covers content but doesn’t remove it from the PDF"
        })
    }

    /// The status line for a mostly-picture page on dark paper, from the
    /// worker's report for the page image on screen.
    pub fn pictures_keep_colours_note(&self, cx: &gpui::App) -> Option<&'static str> {
        let session = self.session(self.active_document_id?, cx)?.read(cx);
        crate::paper_appearance::pictures_keep_colours_note(
            self.paper_appearance,
            session.current_base_paper,
            session.current_base_raster.as_ref()?.picture_coverage(),
        )
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
            // The banner over the page says what to click and what Shift
            // does; the bar names the mode and its key alone.
            CALIBRATE_STATUS.to_owned()
        } else if self.pending_text_box_editor.is_some() {
            "Text Box · Click away to finish".to_owned()
        } else if let Some(status) = self.typing_click_out_status() {
            status.to_owned()
        } else if let Some(guidance) = self.signature_box_guidance() {
            guidance.to_owned()
        } else if let Some(redaction) = self.redaction_status_context(tool, cx) {
            redaction.to_owned()
        } else if editing_content && editing_text {
            "Edit PDF · Type to edit · Esc or click away to finish".to_owned()
        } else if editing_content && self.selected_block(cx).is_some() {
            "Edit PDF · Block · Right-click › Explode block to select its parts · Drag to move it"
                .to_owned()
        } else if editing_content {
            format!(
                "Edit PDF · Click text to edit it · Drag its edge, a drawing or an image to move it · {BOX_HINT}"
            )
        } else if self.pan_tool_active {
            "Pan · Drag to move the page".to_owned()
        } else if let Some(status) = self
            .active_document_id
            .and_then(|document_id| self.click_out_status(document_id, cx))
        {
            status.to_owned()
        } else if let Some(tool) = tool {
            let help = match tool {
                AnnotationTool::Select if cfg!(target_os = "macos") => {
                    "Click to select · Drag to lasso · Click empty space to start a box · Command-drag: box anywhere"
                }
                AnnotationTool::Select => {
                    "Click to select · Drag to lasso · Click empty space to start a box · Ctrl+drag: box anywhere"
                }
                AnnotationTool::Polyline
                | AnnotationTool::Polygon
                | AnnotationTool::Polylength
                | AnnotationTool::Area
                    if cfg!(target_os = "macos") =>
                {
                    "Click points · Return or double-click to finish · Esc to cancel"
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
        // A page that is mostly pictures (a scan) keeps its real colours on
        // dark pages: say so first, where the bar's truncation keeps it.
        let context = match self.pictures_keep_colours_note(cx) {
            Some(note) => format!("{note} · {context}"),
            None => context,
        };
        let wide = window.viewport_size().width >= window.rem_size() * 65.;
        let navigation = if !wide {
            "Space + drag: pan · Pinch: zoom"
        } else if cfg!(target_os = "macos") {
            "Hold Space + drag: pan · Two fingers: scroll · Pinch to zoom · Hold Control while scrolling to zoom"
        } else {
            "Hold Space + drag: pan · Two fingers: scroll · Pinch to zoom · Hold Ctrl while scrolling to zoom"
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
                Some(AnnotationTool::Polygon) if cfg!(target_os = "macos") => Some(
                    "Click each corner, then press Return, double-click, or click the starting point to finish. With Cloud outline on in its properties, the polygon is drawn as a cloud.",
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
        let context_tooltip = match self.redact_pending_status_text(cx) {
            Some(redaction) => format!("{context_tooltip} {redaction}"),
            None => context_tooltip,
        };
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
        let navigation_tooltip = if cfg!(target_os = "macos") {
            "Hold Space and drag to pan. Two-finger trackpad scrolling always scrolls. Pinch or Control + trackpad scroll zooms. Mouse-wheel behaviour follows the view dropdown; Control + wheel does the opposite."
        } else {
            "Hold Space and drag to pan. Two-finger trackpad scrolling always scrolls. Pinch or Ctrl + trackpad scroll zooms. Mouse-wheel behaviour follows the view dropdown; Ctrl + wheel does the opposite."
        };
        let accessible_help = format!("{context_tooltip}. {navigation_tooltip}");
        // Timed messages show as toasts (`push_status_toast`); the bar keeps
        // its guidance, the page's scale and the navigation help.
        let context_help = gpui::div()
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
            .child(context);
        // 8 + its 20px controls + 8, with the grid's unit at the ends and
        // between items; the line is drawn inside.
        let bar = gpui_component::status_bar::StatusBar::new()
            .w_full()
            .min_w_0()
            .h(crate::layout_metrics::scaled(
                crate::layout_metrics::STATUS_BAR,
            ))
            .py_0()
            .px(crate::layout_metrics::scaled(crate::layout_metrics::SPACE))
            .gap(crate::layout_metrics::scaled(crate::layout_metrics::SPACE))
            .border_t_0()
            .child(context_help)
            .when_some(scale, |bar, (document_id, page_index, label)| {
                bar.right(
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
            .right(
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
            );
        gpui::div()
            .id("document-status-bar")
            .debug_selector(|| "document-status-bar".into())
            .role(Role::Status)
            .aria_label(accessible_help)
            .w_full()
            .flex_none()
            .min_w_0()
            .relative()
            .child(bar)
            .child(crate::layout_metrics::inside_line(
                crate::layout_metrics::Edge::Top,
                cx.theme().border,
            ))
    }

    /// Shows a new status message as a stock toast, if it is a notice or an
    /// error: a routine message stays in the status bar. A newer message
    /// replaces the one before it.
    pub(super) fn push_status_toast(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((document_id, text, shown, severity)) = self.status_message.clone() else {
            return;
        };
        if Some(document_id) != self.active_document_id
            || self.status_toast_shown == Some(shown)
            || shown.elapsed() >= STATUS_MESSAGE_SHOWN
        {
            return;
        }
        // Toasts live in the window's stock Root; a window without one
        // (a bare workspace in a test) has nowhere to show them.
        if window.root::<gpui_component::Root>().flatten().is_none() {
            return;
        }
        self.status_toast_shown = Some(shown);
        if severity == StatusSeverity::Info {
            return;
        }
        struct StatusToast;
        let toast = if severity == StatusSeverity::Error {
            gpui_component::notification::Notification::error(text)
        } else {
            gpui_component::notification::Notification::new().message(text)
        };
        window.push_notification(toast.id::<StatusToast>().placement(TOAST_PLACEMENT), cx);
    }
}

/// Toasts show at the bottom right of the layer [`toast_layer`] lays over
/// the canvas.
const TOAST_PLACEMENT: gpui::Anchor = gpui::Anchor::BottomRight;

/// The window's stock toasts, laid over the area it is placed in: the
/// canvas, under the view toolbar, between the side panels and above the
/// status bar, so a toast never covers the rails or the status bar. The
/// stock margins keep it 16px from that area's edges.
pub(super) fn toast_layer(window: &mut Window, cx: &mut App) -> Option<gpui::AnyElement> {
    let layer = gpui_component::Root::render_notification_layer(window, cx)?;
    Some(
        gpui::div()
            .absolute()
            .inset_0()
            .debug_selector(|| DOCUMENT_TOAST_LAYER_ID.into())
            .child(layer)
            .into_any_element(),
    )
}
