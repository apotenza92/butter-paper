//! The Edit PDF Content tool on the canvas: hovering, clicking and dragging in
//! existing page text, the inline editor, and keeping the PDF worker's
//! rendering in step with page-text edits.
//!
//! The page itself shows edited text: every change is applied by the worker
//! and re-rendered, and the viewer keeps the previous tiles until the new
//! ones arrive. The workspace only draws the hover outline, the caret and
//! the selection over it.

use super::*;
use crate::page_text_editor::{
    Backspace, CopyText, CutText, DeleteForward, DeleteToLineStart, DeleteWordBackward,
    DeleteWordForward, FinishEditing, InsertNewline, MoveDown, MoveLeft, MoveLineEnd,
    MoveLineStart, MoveRight, MoveUp, MoveWordLeft, MoveWordRight, PAGE_TEXT_EDITOR_CONTEXT,
    PageTextEditState, PageTextInput, PageTextInputEvent, PageTextScreenMap, PasteText, RedoText,
    SelectAllText, SelectDown, SelectLeft, SelectLineEnd, SelectLineStart, SelectRight, SelectUp,
    SelectWordLeft, SelectWordRight, UndoText,
};
use crate::pdf_text_blocks::{PageTextBlock, paragraph_blocks};
use crate::pdf_text_edit::{
    ExtraLine, PageTextEdit, PageTextLayer, PageTextLine, TextLineAnchor, extra_line_base,
    layout_text_edit_fitted,
};
use crate::pdf_text_reflow::Paragraph;
use crate::pdf_worker::DocumentEdits;
use gpui::{AnyElement, ElementInputHandler};
use std::collections::BTreeSet;

/// The hover and press reach of a line beyond its glyph box, in pixels.
const PAGE_TEXT_HIT_TOLERANCE_PX: f64 = 3.;

type Quad = [(f64, f64); 4];

pub(super) enum PageTextLayerState {
    Pending,
    /// The page's text and its paragraphs.
    Ready(Arc<PageTextLayer>, Arc<Vec<PageTextBlock>>),
    Failed(String),
}

/// Per document: the editable text of its pages and the edits (page text
/// and form fields) the PDF worker currently renders.
#[derive(Default)]
pub(super) struct PageTextDocumentState {
    epoch: u64,
    layers: HashMap<u32, PageTextLayerState>,
    /// The edits the worker renders.
    shown: DocumentEdits,
    /// Edits sent to the worker and not yet acknowledged.
    sending: Option<DocumentEdits>,
    /// Edits the worker rejected; not retried until they change.
    rejected: Option<DocumentEdits>,
    /// The last hovered point, tested again when its page's text arrives.
    hover_probe: Option<(PageInteraction, PdfPoint)>,
    /// Each page's paragraph outlines, a quad per line keyed by the
    /// paragraph's first line, for the edits they were drawn with.
    outlines: HashMap<u32, (Vec<PageTextEdit>, Arc<Vec<(TextLineAnchor, Vec<Quad>)>>)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageTextEditing {
    pub document_id: DocumentId,
    pub page_index: u32,
    /// The edited paragraph's first line.
    pub anchor: TextLineAnchor,
    pub epoch: u64,
    /// The press that started a drag selection is still held.
    pub dragging: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PageTextHover {
    pub document_id: DocumentId,
    pub page_index: u32,
    /// The hovered paragraph's first line, or the read-only line.
    pub anchor: TextLineAnchor,
    pub editable: bool,
}

/// What the viewport draws over one page for the Edit PDF Content tool, in page
/// user space.
#[derive(Clone)]
pub(super) struct PageTextOverlay {
    pub page_index: u32,
    /// Every editable paragraph on the page, a quad per line, while Edit
    /// Text is active.
    pub paragraphs: Vec<Quad>,
    /// The hovered paragraph, a quad per line.
    pub hover: Vec<Quad>,
    /// The edited paragraph, a quad per line.
    pub frame: Vec<Quad>,
    /// The frame's colour: a warning when added lines overlap other text.
    pub frame_color: Option<gpui::Hsla>,
    pub selection: Vec<Quad>,
    pub caret: Option<[(f64, f64); 2]>,
    /// Underlines under text an input method is composing.
    pub marked: Vec<[(f64, f64); 2]>,
    /// Return marks after the edited paragraph's hard line breaks.
    pub breaks: Vec<Vec<(f64, f64)>>,
    /// Fainter marks after its lines that wrap into the next.
    pub wraps: Vec<Vec<(f64, f64)>>,
    pub input: Option<(Entity<PageTextInput>, FocusHandle)>,
}

/// A line under a page point: in a paragraph that can be edited, or a
/// read-only line.
struct PageTextHit {
    layer: Arc<PageTextLayer>,
    block: Option<PageTextBlock>,
    line: PageTextLine,
}

/// The text-space span of a line's visible characters as `runs` lay it
/// out, without leading and trailing spaces.
/// The pages whose content differs between two sets of edits: pages whose
/// text edits differ, and the pages of form fields whose values differ.
fn changed_pages(
    before: &DocumentEdits,
    after: &DocumentEdits,
    form: &crate::pdf_forms::FormLayer,
) -> BTreeSet<u32> {
    let on_page = |edits: &[PageTextEdit], page: u32| {
        edits
            .iter()
            .filter(|edit| edit.page_index == page)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut pages = before
        .text
        .iter()
        .chain(&after.text)
        .map(|edit| edit.page_index)
        .filter(|page| on_page(&before.text, *page) != on_page(&after.text, *page))
        .collect::<BTreeSet<_>>();
    let value = |edits: &DocumentEdits, name: &str| {
        edits
            .forms
            .iter()
            .find(|edit| edit.name == name)
            .map(|edit| edit.value.clone())
    };
    for edit in before.forms.iter().chain(&after.forms) {
        if value(before, &edit.name) != value(after, &edit.name)
            && let Some(field) = form.field(&edit.name)
        {
            pages.extend(field.widgets.iter().map(|widget| widget.page_index));
        }
    }
    pages
}

fn visible_span(
    layer: &PageTextLayer,
    line: &PageTextLine,
    edit: Option<&PageTextEdit>,
) -> Option<(f64, f64)> {
    let layout = match edit {
        Some(edit) => layout_text_edit_fitted(layer, line, &edit.runs, edit.layout.fit).ok()?,
        None => {
            layout_text_edit_fitted(layer, line, &line.original_runs(), Default::default()).ok()?
        }
    };
    let mut visible = layout
        .chars
        .iter()
        .filter(|character| !character.character.is_whitespace());
    let first = visible.next()?;
    let last = visible.next_back().unwrap_or(first);
    Some((first.x, last.x + last.advance))
}

/// How far a page point is outside a line's box, in text space; zero when
/// inside.
fn line_distance(
    line: &PageTextLine,
    span: (f64, f64),
    point: (f64, f64),
    tolerance: f64,
) -> Option<f64> {
    let (x, y) = line.text_space_point(point.0, point.1)?;
    let bottom = line.rise + line.descent;
    let top = line.rise + line.ascent;
    let dx = if x < span.0 {
        span.0 - x
    } else if x > span.1 {
        x - span.1
    } else {
        0.
    };
    let dy = if y < bottom {
        bottom - y
    } else if y > top {
        y - top
    } else {
        0.
    };
    (dx <= tolerance && dy <= tolerance).then_some(dx.max(dy))
}

fn line_scale(line: &PageTextLine) -> f64 {
    let m = line.matrix;
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt().max(1e-9)
}

impl DocumentWorkspace {
    fn page_text_state(
        &mut self,
        document_id: DocumentId,
        epoch: u64,
    ) -> &mut PageTextDocumentState {
        let state = self.page_text.entry(document_id).or_default();
        if state.epoch != epoch {
            // A reopened worker renders its own source unedited.
            *state = PageTextDocumentState {
                epoch,
                ..PageTextDocumentState::default()
            };
        }
        state
    }

    /// The editable text of a page, requesting it from the worker when
    /// it has not been read.
    /// With Edit PDF Content active, loads the text of the visible pages, so the
    /// first press on them finds their lines.
    pub(super) fn prefetch_page_text(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let pages = {
            let session = session.read(cx);
            if !session.annotations.tool().edits_page_text() {
                return;
            }
            session
                .viewer
                .plan_snapshot()
                .map(|plan| plan.visible_pages.to_vec())
                .unwrap_or_default()
        };
        // Text far from the view is read again if it comes back, so a long
        // document keeps only the pages around the view.
        let editing = self
            .page_text_editing
            .filter(|editing| editing.document_id == document_id)
            .map(|editing| editing.page_index);
        if !pages.is_empty()
            && let Some(state) = self.page_text.get_mut(&document_id)
        {
            let near = |page: &u32| {
                Some(*page) == editing
                    || pages.iter().any(|visible| {
                        (*visible as i64 - i64::from(*page)).abs() <= KEPT_PAGES_AROUND_VIEW
                    })
            };
            state
                .layers
                .retain(|page, layer| near(page) || matches!(layer, PageTextLayerState::Pending));
            state.outlines.retain(|page, _| near(page));
        }
        // Reading text shares the worker with rendering, so one page is read
        // at a time, always one in view now: pages flown past while
        // scrolling are never queued. Each read's arrival redraws and picks
        // the next.
        let reading = self.page_text.get(&document_id).is_some_and(|state| {
            state
                .layers
                .values()
                .any(|layer| matches!(layer, PageTextLayerState::Pending))
        });
        if reading {
            return;
        }
        let next = pages
            .into_iter()
            .filter_map(|page| u32::try_from(page).ok())
            .find(|page| {
                self.page_text
                    .get(&document_id)
                    .is_none_or(|state| !state.layers.contains_key(page))
            });
        if let Some(page) = next {
            self.page_text_layer(document_id, page, cx);
        }
    }

    /// The outlines of every editable paragraph on a loaded page, kept
    /// until the page's edits change.
    fn page_paragraph_outlines(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Option<Arc<Vec<(TextLineAnchor, Vec<Quad>)>>> {
        // Only text already read: drawing never asks the worker.
        let (layer, blocks) = match self.page_text.get(&document_id)?.layers.get(&page_index)? {
            PageTextLayerState::Ready(layer, blocks) => (layer.clone(), blocks.clone()),
            _ => return None,
        };
        let edits = self
            .page_text_edits_of(document_id, cx)
            .into_iter()
            .filter(|edit| edit.page_index == page_index)
            .collect::<Vec<_>>();
        if let Some((drawn, outlines)) = self
            .page_text
            .get(&document_id)
            .and_then(|state| state.outlines.get(&page_index))
            && *drawn == edits
        {
            return Some(outlines.clone());
        }
        // Text placed outside the page's visible area (printers' notes)
        // shows no outline.
        let view_box = self
            .session(document_id, cx)
            .and_then(|session| session.read(cx).source_page_coordinate_space(page_index))
            .map(|space| space.view_box());
        let visible = |quad: &Quad| {
            view_box.is_none_or(|view| {
                let xs = quad.iter().map(|(x, _)| *x);
                let ys = quad.iter().map(|(_, y)| *y);
                xs.clone().fold(f64::MAX, f64::min) < view.right()
                    && xs.fold(f64::MIN, f64::max) > view.x
                    && ys.clone().fold(f64::MAX, f64::min) < view.top()
                    && ys.fold(f64::MIN, f64::max) > view.y
            })
        };
        let outlines = Arc::new(
            blocks
                .iter()
                .map(|block| {
                    let mut quads =
                        self.paragraph_line_quads(document_id, page_index, &layer, block, cx);
                    quads.retain(visible);
                    (block.lines[0], quads)
                })
                .collect::<Vec<_>>(),
        );
        self.page_text
            .get_mut(&document_id)?
            .outlines
            .insert(page_index, (edits, outlines.clone()));
        Some(outlines)
    }

    /// A paragraph's lines as the page shows them, a quad each, with the
    /// lines earlier edits added below it.
    fn paragraph_line_quads(
        &self,
        document_id: DocumentId,
        page_index: u32,
        layer: &PageTextLayer,
        block: &PageTextBlock,
        cx: &App,
    ) -> Vec<Quad> {
        let mut quads = Vec::new();
        for anchor in &block.lines {
            let Some(line) = layer.line(anchor) else {
                continue;
            };
            let edit = self.page_text_line_edit(document_id, page_index, *anchor, cx);
            if let Some((x0, x1)) = visible_span(layer, line, edit.as_ref()) {
                quads.push(line.quad(x0, x1));
            }
            for extra in edit.iter().flat_map(|edit| &edit.layout.extra_lines) {
                let base = extra_line_base(line, extra.below);
                if let Ok(layout) = layout_text_edit_fitted(layer, &base, &extra.runs, extra.fit)
                    && let (Some(first), Some(last)) = (layout.chars.first(), layout.chars.last())
                {
                    quads.push(base.quad(first.x, last.x + last.advance));
                }
            }
        }
        quads
    }

    fn page_text_layer(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Option<Arc<PageTextLayer>> {
        let session = self.session(document_id, cx)?.clone();
        let (epoch, resource) = {
            let session = session.read(cx);
            (session.resource_epoch, session.resource.clone())
        };
        let state = self.page_text_state(document_id, epoch);
        match state.layers.get(&page_index) {
            Some(PageTextLayerState::Ready(layer, _)) => return Some(layer.clone()),
            Some(PageTextLayerState::Pending | PageTextLayerState::Failed(_)) => return None,
            None => {}
        }
        let resource = resource?;
        state.layers.insert(page_index, PageTextLayerState::Pending);
        let task = cx
            .background_executor()
            .spawn(async move { resource.page_text(page_index) });
        cx.spawn(async move |owner, cx| {
            let result = task.await;
            let _ = owner.update(cx, |workspace, cx| {
                let Some(state) = workspace.page_text.get_mut(&document_id) else {
                    return;
                };
                if state.epoch != epoch {
                    return;
                }
                state.layers.insert(
                    page_index,
                    match result {
                        Ok(layer) => {
                            let blocks = paragraph_blocks(&layer);
                            PageTextLayerState::Ready(Arc::new(layer), Arc::new(blocks))
                        }
                        Err(error) => PageTextLayerState::Failed(error),
                    },
                );
                let probe = state
                    .hover_probe
                    .filter(|(interaction, _)| interaction.page_index == page_index);
                if let Some((interaction, point)) = probe
                    && workspace.annotation_tool(document_id, cx)
                        == Some(AnnotationTool::EditPdfContent)
                {
                    workspace.update_page_text_hover(interaction, point, cx);
                }
                cx.notify();
            });
        })
        .detach();
        None
    }

    fn page_text_edits_of(&self, document_id: DocumentId, cx: &App) -> Vec<PageTextEdit> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_page_text_edits(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// The edits the page should show: the document's, with the paragraph
    /// being edited as it is now.
    fn desired_page_text_edits(&self, document_id: DocumentId, cx: &App) -> Vec<PageTextEdit> {
        let mut edits = self.page_text_edits_of(document_id, cx);
        if let Some(editing) = self.page_text_editing
            && editing.document_id == document_id
            && let Some(state) = self
                .page_text_input
                .as_ref()
                .and_then(|(input, _)| input.read(cx).state.as_ref())
        {
            let anchors = state.paragraph().anchors();
            edits.retain(|edit| {
                edit.page_index != editing.page_index || !anchors.contains(&edit.anchor)
            });
            edits.extend(
                state
                    .edits()
                    .into_iter()
                    .filter(|edit| !edit.is_unchanged()),
            );
            edits.sort_by_key(|edit| (edit.page_index, edit.anchor));
        }
        edits
    }

    /// The form field values the pages should show: the document's.
    fn form_field_edits_of(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<crate::pdf_forms::FormFieldEdit> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_form_field_edits(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// Sends the edits the pages should show (page text and form fields) to
    /// the worker when they differ from what it renders, one update at a
    /// time, then refreshes the view.
    pub(super) fn sync_document_edits(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            self.page_text.remove(&document_id);
            return;
        };
        let (epoch, resource) = {
            let session = session.read(cx);
            (session.resource_epoch, session.resource.clone())
        };
        let desired = DocumentEdits {
            text: self.desired_page_text_edits(document_id, cx),
            forms: self.form_field_edits_of(document_id, cx),
        };
        let form = resource
            .as_ref()
            .map(|resource| resource.form_layer())
            .unwrap_or_default();
        let state = self.page_text_state(document_id, epoch);
        if state.sending.is_some()
            || state.shown == desired
            || state.rejected.as_ref() == Some(&desired)
        {
            return;
        }
        let Some(resource) = resource else {
            return;
        };
        state.sending = Some(desired.clone());
        let sent = desired.clone();
        let task = cx
            .background_executor()
            .spawn(async move { resource.set_document_edits(&sent) });
        cx.spawn(async move |owner, cx| {
            let result = task.await;
            let _ = owner.update(cx, |workspace, cx| {
                let Some(state) = workspace.page_text.get_mut(&document_id) else {
                    return;
                };
                if state.epoch != epoch {
                    return;
                }
                state.sending = None;
                match result {
                    Ok(()) => {
                        let changed_pages = changed_pages(&state.shown, &desired, &form);
                        state.shown = desired;
                        state.rejected = None;
                        if let Some(session) = workspace.session(document_id, cx).cloned() {
                            let rendered = session.update(cx, |session, cx| {
                                session.viewer.refresh_content();
                                cx.notify();
                                changed_pages
                                    .into_iter()
                                    .filter(|page| {
                                        session
                                            .thumbnails
                                            .iter()
                                            .any(|thumbnail| thumbnail.page_index == *page)
                                            || session.thumbnail_requests.contains_key(page)
                                    })
                                    .collect::<Vec<_>>()
                            });
                            // Pages without a thumbnail yet show the edit when
                            // they are first rendered.
                            for page in rendered {
                                workspace.refresh_page_thumbnail(document_id, page, cx);
                            }
                        }
                    }
                    Err(error) => {
                        state.rejected = Some(desired);
                        workspace
                            .annotation_statuses
                            .insert(document_id, format!("Edits cannot be shown: {error}"));
                    }
                }
                // Catch up with changes made while this update ran.
                workspace.sync_document_edits(document_id, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The document's edit of a line, if any.
    fn page_text_line_edit(
        &self,
        document_id: DocumentId,
        page_index: u32,
        anchor: TextLineAnchor,
        cx: &App,
    ) -> Option<PageTextEdit> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .document_page_text_edits(document_id.value())
                .iter()
                .find(|edit| edit.page_index == page_index && edit.anchor == anchor)
                .cloned()
        })
    }

    fn page_text_blocks(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Option<(Arc<PageTextLayer>, Arc<Vec<PageTextBlock>>)> {
        self.page_text_layer(document_id, page_index, cx)?;
        match self.page_text.get(&document_id)?.layers.get(&page_index)? {
            PageTextLayerState::Ready(layer, blocks) => Some((layer.clone(), blocks.clone())),
            _ => None,
        }
    }

    /// The line under a page point, and the paragraph it is in. Lines an
    /// earlier edit added below a paragraph count as its lines.
    fn page_text_hit(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<PageTextHit> {
        let (layer, blocks) =
            self.page_text_blocks(interaction.document_id, interaction.page_index, cx)?;
        let tolerance_points = interaction
            .transform
            .tolerance_points(PAGE_TEXT_HIT_TOLERANCE_PX)
            .unwrap_or(1.);
        // The paragraph being edited is hit across its whole box, so a press
        // at a line's end keeps editing it.
        if let Some(editing) = self.page_text_editing.filter(|editing| {
            editing.document_id == interaction.document_id
                && editing.page_index == interaction.page_index
        }) && let Some(state) = self
            .page_text_input
            .as_ref()
            .and_then(|(input, _)| input.read(cx).state.as_ref())
        {
            let first = &state.paragraph().lines()[0];
            if state.contains_point(point.x, point.y, tolerance_points / line_scale(first))
                && let Some(block) = blocks.iter().find(|block| block.lines[0] == editing.anchor)
            {
                return Some(PageTextHit {
                    layer: layer.clone(),
                    block: Some(block.clone()),
                    line: first.clone(),
                });
            }
        }
        let mut best: Option<(f64, PageTextHit)> = None;
        let mut consider = |line: &PageTextLine,
                            block: Option<&PageTextBlock>,
                            span: (f64, f64)| {
            let tolerance = tolerance_points / line_scale(line);
            let Some(distance) = line_distance(line, span, (point.x, point.y), tolerance) else {
                return;
            };
            if best.as_ref().is_some_and(|(best, _)| *best <= distance) {
                return;
            }
            best = Some((
                distance,
                PageTextHit {
                    layer: layer.clone(),
                    block: block.cloned(),
                    line: line.clone(),
                },
            ));
        };
        for line in &layer.lines {
            let edit = self.page_text_line_edit(
                interaction.document_id,
                interaction.page_index,
                line.anchor,
                cx,
            );
            let block = blocks
                .iter()
                .find(|block| block.lines.contains(&line.anchor));
            if let Some(span) = visible_span(&layer, line, edit.as_ref()) {
                consider(line, block, span);
            }
            for extra in edit.iter().flat_map(|edit| &edit.layout.extra_lines) {
                let base = extra_line_base(line, extra.below);
                if let Some(span) = visible_span(
                    &layer,
                    &base,
                    Some(&PageTextEdit {
                        page_index: interaction.page_index,
                        anchor: extra.anchor,
                        original: Vec::new(),
                        runs: extra.runs.clone(),
                        layout: crate::pdf_text_edit::TextEditLayout {
                            fit: extra.fit,
                            ..Default::default()
                        },
                    }),
                ) {
                    consider(&base, block, span);
                }
            }
        }
        best.map(|(_, hit)| hit)
    }

    /// The anchor that names a hit: its paragraph's first line, or the line.
    fn hit_anchor(hit: &PageTextHit) -> TextLineAnchor {
        hit.block
            .as_ref()
            .map_or(hit.line.anchor, |block| block.lines[0])
    }

    /// Hover feedback for the Edit PDF Content tool. Hover and press share the
    /// same hit test, so the I-beam always means a click edits there.
    pub(super) fn update_page_text_hover(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> bool {
        let hit = self.page_text_hit(interaction, point, cx);
        if let Some(state) = self.page_text.get_mut(&interaction.document_id) {
            state.hover_probe = Some((interaction, point));
        }
        let hover = hit.map(|hit| PageTextHover {
            document_id: interaction.document_id,
            page_index: interaction.page_index,
            anchor: Self::hit_anchor(&hit),
            editable: hit.block.is_some(),
        });
        if self.page_text_hover != hover {
            self.page_text_hover = hover;
            cx.notify();
        }
        true
    }

    pub(super) fn page_text_cursor(&self, document_id: DocumentId) -> Option<CursorStyle> {
        if self
            .page_text_editing
            .is_some_and(|editing| editing.document_id == document_id && editing.dragging)
        {
            return Some(CursorStyle::IBeam);
        }
        let hover = self
            .page_text_hover
            .as_ref()
            .filter(|hover| hover.document_id == document_id)?;
        Some(if hover.editable {
            CursorStyle::IBeam
        } else {
            CursorStyle::OperationNotAllowed
        })
    }

    fn page_text_input_entity(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<PageTextInput> {
        if let Some((input, _)) = &self.page_text_input {
            return input.clone();
        }
        let input = cx.new(PageTextInput::new);
        let subscription = cx.subscribe_in(
            &input,
            window,
            |workspace, _, event: &PageTextInputEvent, window, cx| match event {
                PageTextInputEvent::Changed => {
                    if let Some(editing) = workspace.page_text_editing {
                        workspace.annotation_statuses.remove(&editing.document_id);
                        workspace.sync_document_edits(editing.document_id, cx);
                    }
                    cx.notify();
                }
                PageTextInputEvent::Finish => {
                    workspace.commit_page_text_editing(cx);
                    workspace.workspace_focus.focus(window, cx);
                    cx.notify();
                }
                PageTextInputEvent::Rejected(character) => {
                    if let Some(editing) = workspace.page_text_editing {
                        workspace.annotation_statuses.insert(
                            editing.document_id,
                            format!(
                                "No available font can draw \u{201c}{character}\u{201d} in this text"
                            ),
                        );
                    }
                    cx.notify();
                }
            },
        );
        self.page_text_input = Some((input.clone(), subscription));
        input
    }

    /// A press with the Edit PDF Content tool: place the caret in editable text
    /// (selecting a word or the line on double and triple click), or finish
    /// editing when the press is elsewhere.
    /// Asks before the first text edit of a signed document: saving changed
    /// content invalidates its signatures (Revu refuses; this allows it).
    fn confirm_editing_signed_text(
        owner: WeakEntity<Self>,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.open_alert_dialog(cx, move |alert, _, _| {
            let owner = owner.clone();
            alert
                .close_button(false)
                .title("Edit content in a signed PDF?")
                .description(
                    "This PDF is digitally signed. Saving changes to its text invalidates its signatures.",
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new(DOCUMENT_SIGNED_TEXT_EDIT_CANCEL_ID)
                                .debug_selector(|| DOCUMENT_SIGNED_TEXT_EDIT_CANCEL_ID.into())
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(dialog_action_button(
                            Button::new(DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID)
                                .debug_selector(|| DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID.into())
                                .danger()
                                .label("Edit PDF Content"),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    owner
                        .update(cx, |workspace, cx| {
                            workspace.page_text_signed_confirmed.insert(document_id);
                            workspace.annotation_statuses.insert(
                                document_id,
                                "Click text to edit it. Saving invalidates the signatures.".into(),
                            );
                            cx.notify();
                        })
                        .is_ok()
                })
        });
    }

    pub(super) fn begin_page_text_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        click_count: usize,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let hit = self.page_text_hit(interaction, point, cx);
        let Some(hit) = hit else {
            self.commit_page_text_editing(cx);
            if let Some(PageTextLayerState::Failed(error)) = self
                .page_text
                .get(&interaction.document_id)
                .and_then(|state| state.layers.get(&interaction.page_index))
            {
                self.annotation_statuses.insert(
                    interaction.document_id,
                    format!("Text on this page cannot be edited: {error}"),
                );
            }
            self.workspace_focus.focus(window, cx);
            cx.notify();
            return true;
        };
        let Some(block) = hit.block.clone() else {
            self.commit_page_text_editing(cx);
            let reason = hit
                .line
                .read_only
                .clone()
                .unwrap_or_else(|| "the text has no visible characters".into());
            self.annotation_statuses.insert(
                interaction.document_id,
                format!("This text cannot be edited: {reason}"),
            );
            cx.notify();
            return true;
        };
        let anchor = block.lines[0];
        let continuing = self.page_text_editing.is_some_and(|editing| {
            editing.document_id == interaction.document_id
                && editing.page_index == interaction.page_index
                && editing.anchor == anchor
        });
        let signed = self
            .session(interaction.document_id, cx)
            .and_then(|session| session.read(cx).resource.clone())
            .is_some_and(|resource| resource.digitally_signed());
        if signed
            && !continuing
            && !self
                .page_text_signed_confirmed
                .contains(&interaction.document_id)
        {
            self.commit_page_text_editing(cx);
            Self::confirm_editing_signed_text(
                cx.entity().downgrade(),
                interaction.document_id,
                window,
                cx,
            );
            return true;
        }
        let input = self.page_text_input_entity(window, cx);
        if !continuing {
            self.commit_page_text_editing(cx);
            let Some(paragraph) = Paragraph::new(hit.layer.clone(), &block) else {
                return true;
            };
            // The paragraph's lines as the document shows them now.
            let edits = block
                .lines
                .iter()
                .map(|anchor| {
                    self.page_text_line_edit(
                        interaction.document_id,
                        interaction.page_index,
                        *anchor,
                        cx,
                    )
                })
                .collect::<Vec<_>>();
            let current = paragraph
                .lines()
                .iter()
                .zip(&edits)
                .map(|(line, edit)| {
                    edit.as_ref()
                        .map_or_else(|| line.original_runs(), |edit| edit.runs.clone())
                })
                .collect::<Vec<_>>();
            let added: Vec<ExtraLine> = edits
                .last()
                .and_then(|edit| edit.as_ref())
                .map(|edit| edit.layout.extra_lines.clone())
                .unwrap_or_default();
            // Settings chosen earlier are kept with the first line.
            let format = edits
                .first()
                .and_then(|edit| edit.as_ref())
                .map(|edit| edit.layout.paragraph)
                .unwrap_or_default();
            let paragraph = paragraph.with_format(format);
            // Hard breaks (ending list items) an earlier edit left.
            let breaks = edits
                .iter()
                .map(|edit| edit.as_ref().map(|edit| edit.layout.hard_break))
                .collect::<Vec<_>>();
            let state = match PageTextEditState::new(paragraph, &current, &breaks, &added, 0) {
                Ok(state) => state,
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                    cx.notify();
                    return true;
                }
            };
            let epoch = self
                .session(interaction.document_id, cx)
                .map_or(0, |session| session.read(cx).resource_epoch);
            input.update(cx, |input, cx| {
                input.state = Some(state);
                cx.notify();
            });
            self.page_text_editing = Some(PageTextEditing {
                document_id: interaction.document_id,
                page_index: interaction.page_index,
                anchor,
                epoch,
                dragging: false,
            });
        }
        input.update(cx, |input, cx| {
            if let Some(state) = input.state.as_mut() {
                let index = state.index_at_point(point.x, point.y);
                match click_count {
                    2 => state.select_word_at(
                        index.saturating_sub(usize::from(index == state.len() && index > 0)),
                    ),
                    3 => state.select_line_at(index),
                    count if count >= 4 => state.select_all(),
                    _ => state.move_to(index, modifiers.shift && continuing),
                }
            }
            cx.notify();
        });
        if let Some(editing) = self.page_text_editing.as_mut() {
            editing.dragging = click_count == 1;
        }
        // The press cleared hover; the pointer is over the edited paragraph.
        self.page_text_hover = Some(PageTextHover {
            document_id: interaction.document_id,
            page_index: interaction.page_index,
            anchor,
            editable: true,
        });
        self.annotation_statuses.remove(&interaction.document_id);
        input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
        true
    }

    /// Extends the selection while a press in the edited paragraph is held.
    pub(super) fn update_page_text_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editing) = self.page_text_editing.filter(|editing| editing.dragging) else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(editing.document_id, editing.page_index))
            .copied()
        else {
            return true;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            return true;
        };
        if let Some((input, _)) = &self.page_text_input {
            input.update(cx, |input, cx| {
                if let Some(state) = input.state.as_mut() {
                    let index = state.index_at_point(point.x, point.y);
                    state.move_to(index, true);
                    cx.notify();
                }
            });
        }
        cx.notify();
        true
    }

    pub(super) fn finish_page_text_drag(&mut self, cx: &mut Context<Self>) -> bool {
        match self.page_text_editing.as_mut() {
            Some(editing) if editing.dragging => {
                editing.dragging = false;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub fn edit_pdf_content_property_inspector(
        &self,
    ) -> Option<Entity<crate::edit_pdf_content_property_inspector::EditPdfContentPropertyInspector>>
    {
        self.edit_pdf_content_property_inspector
            .as_ref()
            .map(|(inspector, _)| inspector.clone())
    }

    pub(super) fn ensure_edit_pdf_content_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<crate::edit_pdf_content_property_inspector::EditPdfContentPropertyInspector> {
        use crate::edit_pdf_content_property_inspector::{
            EditPdfContentPropertyEvent, EditPdfContentPropertyInspector,
        };
        if let Some((inspector, _)) = &self.edit_pdf_content_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| EditPdfContentPropertyInspector::new(window, cx));
        let subscription = cx.subscribe_in(
            &inspector,
            window,
            |workspace, _, event: &EditPdfContentPropertyEvent, window, cx| {
                workspace.apply_edit_pdf_content_property_patch(event.patch.clone(), cx);
                // Typing continues in the text, as after a toolbar command.
                if event.return_focus
                    && let Some((input, _)) = &workspace.page_text_input
                {
                    let focus = input.read(cx).focus_handle(cx);
                    window.focus(&focus, cx);
                }
            },
        );
        self.edit_pdf_content_property_inspector = Some((inspector.clone(), subscription));
        inspector
    }

    /// Whether the Edit PDF Content properties show: when the tool's properties
    /// are open, as other tools' are (a double-click on the tool, or the
    /// properties button); editing text never opens them.
    pub(super) fn edit_pdf_content_properties_visible(&self, _cx: &App) -> bool {
        self.right_rail_actions_open
    }

    /// Shows the edited paragraph's text properties in the inspector.
    pub(super) fn sync_edit_pdf_content_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.edit_pdf_content_property_snapshot(cx);
        let inspector = self.ensure_edit_pdf_content_property_inspector(window, cx);
        match snapshot {
            Some(snapshot) => {
                if inspector.read(cx).snapshot() != Some(&snapshot) {
                    inspector.update(cx, |inspector, cx| inspector.sync(snapshot, window, cx));
                }
            }
            None => inspector.update(cx, |inspector, cx| inspector.clear(cx)),
        }
    }

    fn edit_pdf_content_property_snapshot(
        &self,
        cx: &App,
    ) -> Option<crate::edit_pdf_content_property_inspector::EditPdfContentPropertySnapshot> {
        let (input, _) = self.page_text_input.as_ref()?;
        let state = input.read(cx).state.as_ref()?;
        let style = state.current_style()?.clone();
        let paragraph = state.paragraph();
        let layer = paragraph.layer();
        let metrics = layer.font(&style.font)?;
        let (family, bold, italic) = match &style.face {
            Some(face) => (
                Some(crate::pdf_engine::text_face_family(face)),
                face.fallback.bold,
                face.fallback.italic,
            ),
            None => (None, metrics.fallback.bold, metrics.fallback.italic),
        };
        let line_fill = paragraph.lines()[0].fill;
        Some(
            crate::edit_pdf_content_property_inspector::EditPdfContentPropertySnapshot {
                family,
                size: style.font_size,
                fill: style.fill.or(line_fill).unwrap_or([0.; 3]),
                bold,
                italic,
                alignment: paragraph.alignment(),
                line_spacing: (paragraph.line_spacing() * 100.).round() / 100.,
                substitutes: state.substitutes(),
                selection: !state.selection().is_empty(),
            },
        )
    }

    /// Applies a text property to the selection, or the whole paragraph.
    fn apply_edit_pdf_content_property_patch(
        &mut self,
        patch: crate::edit_pdf_content_property_inspector::EditPdfContentPropertyPatch,
        cx: &mut Context<Self>,
    ) {
        use crate::edit_pdf_content_property_inspector::EditPdfContentPropertyPatch;
        use crate::pdf_text_edit::TextRunStyle;
        let Some(input) = self
            .page_text_input
            .as_ref()
            .map(|(input, _)| input.clone())
        else {
            return;
        };
        let Some(layer) = input
            .read(cx)
            .state
            .as_ref()
            .map(|state| state.paragraph().layer().clone())
        else {
            return;
        };
        // The family and style a run is set in now.
        let current = {
            let layer = layer.clone();
            move |style: &TextRunStyle| match &style.face {
                Some(face) => (
                    crate::pdf_engine::text_face_family(face),
                    face.fallback.bold,
                    face.fallback.italic,
                ),
                None => layer.font(&style.font).map_or_else(
                    || ("Arimo".to_owned(), false, false),
                    |metrics| {
                        (
                            crate::pdf_engine::page_font_family(metrics),
                            metrics.fallback.bold,
                            metrics.fallback.italic,
                        )
                    },
                ),
            }
        };
        let restyled = |style: &TextRunStyle, family: String, bold: bool, italic: bool| {
            let mut style = style.clone();
            let own = layer.font(&style.font).map(|metrics| {
                (
                    crate::pdf_engine::page_font_family(metrics),
                    metrics.fallback.bold,
                    metrics.fallback.italic,
                )
            });
            // Back in the page font's own family and style: draw with it.
            style.face = (own != Some((family.clone(), bold, italic)))
                .then(|| crate::pdf_engine::text_face_for(&family, bold, italic));
            style
        };
        match patch {
            EditPdfContentPropertyPatch::Family(family) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| match &family {
                        None => TextRunStyle {
                            face: None,
                            ..style.clone()
                        },
                        Some(family) => {
                            let (_, bold, italic) = current(style);
                            let mut next = restyled(style, family.clone(), bold, italic);
                            // A family chosen by name keeps that family even
                            // when it is the page font's.
                            if next.face.is_none() {
                                next.face =
                                    Some(crate::pdf_engine::text_face_for(family, bold, italic));
                            }
                            next
                        }
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Bold(bold) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| {
                        let (family, _, italic) = current(style);
                        restyled(style, family, bold, italic)
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Italic(italic) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| {
                        let (family, bold, _) = current(style);
                        restyled(style, family, bold, italic)
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Size(size) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| TextRunStyle {
                        font_size: size,
                        ..style.clone()
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Fill(fill) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| TextRunStyle {
                        fill: Some(fill),
                        ..style.clone()
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Alignment(alignment) => input.update(cx, |input, cx| {
                let Some(mut format) = input.state.as_ref().map(|state| state.paragraph().format())
                else {
                    return;
                };
                format.alignment = Some(alignment);
                input.set_format(format, cx);
            }),
            EditPdfContentPropertyPatch::LineSpacing(spacing) => input.update(cx, |input, cx| {
                let Some(mut format) = input.state.as_ref().map(|state| state.paragraph().format())
                else {
                    return;
                };
                format.line_spacing = Some(spacing);
                input.set_format(format, cx);
            }),
        }
        cx.notify();
    }

    /// Records the paragraph being edited as one undoable change and stops
    /// editing. The page already shows the text.
    pub(super) fn commit_page_text_editing(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.page_text_editing.take() else {
            return;
        };
        let state = self
            .page_text_input
            .as_ref()
            .and_then(|(input, _)| input.update(cx, |input, _| input.state.take()));
        let Some(state) = state else {
            return;
        };
        let current_epoch = self
            .session(editing.document_id, cx)
            .map(|session| session.read(cx).resource_epoch);
        if current_epoch != Some(editing.epoch) {
            // The document was reopened under the editor; its lines moved.
            self.annotation_statuses.insert(
                editing.document_id,
                "The edited text was discarded because the document was reloaded".into(),
            );
            self.sync_document_edits(editing.document_id, cx);
            return;
        }
        let edits = state.edits();
        let anchors = state.paragraph().anchors();
        let page_index = state.page_index();
        let result = self.update_annotation_history_with_result(
            editing.document_id,
            cx,
            move |annotations, document_id| {
                annotations.set_document_page_text_edits(document_id, page_index, &anchors, edits)
            },
        );
        if let Err(error) = result {
            self.annotation_statuses.insert(
                editing.document_id,
                format!("Edited text was not kept: {error}"),
            );
        }
        self.sync_document_edits(editing.document_id, cx);
        cx.notify();
    }

    /// Stops editing when the tool, document or page it belongs to goes
    /// away.
    pub(super) fn reconcile_page_text_editing(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.page_text_editing else {
            return;
        };
        let still_editing = self.active_document_id == Some(editing.document_id)
            && self.annotation_tool(editing.document_id, cx)
                == Some(AnnotationTool::EditPdfContent)
            && !self.pan_tool_active;
        if !still_editing {
            self.commit_page_text_editing(cx);
        }
    }

    /// What the viewport draws for the Edit PDF Content tool on each page.
    /// Delivers input-method text to the page-text editor the way the
    /// platform's text input does: `compose` replaces the marked (composing)
    /// text and keeps it marked, otherwise `text` replaces it and commits.
    /// Returns whether an editor took the text.
    pub fn page_text_input_method(
        &mut self,
        text: &str,
        compose: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use gpui::EntityInputHandler as _;
        let Some(input) = self
            .page_text_input
            .as_ref()
            .map(|(input, _)| input.clone())
        else {
            return false;
        };
        if self.page_text_editing.is_none() {
            return false;
        }
        input.update(cx, |input, cx| {
            if compose {
                let end = text.encode_utf16().count();
                input.replace_and_mark_text_in_range(None, text, Some(end..end), window, cx);
            } else {
                input.replace_text_in_range(None, text, window, cx);
            }
        });
        cx.notify();
        true
    }

    /// For review logs: each page's text state and the edits the worker
    /// shows, sends and rejected.
    pub fn page_text_sync_summary(&self, document_id: DocumentId) -> String {
        let Some(state) = self.page_text.get(&document_id) else {
            return "no page text state".into();
        };
        let texts = |edits: &DocumentEdits| {
            edits
                .text
                .iter()
                .map(PageTextEdit::text)
                .chain(
                    edits
                        .forms
                        .iter()
                        .map(|edit| format!("{}={:?}", edit.name, edit.value)),
                )
                .collect::<Vec<_>>()
        };
        let mut layers = state
            .layers
            .iter()
            .map(|(page, layer)| {
                let layer = match layer {
                    PageTextLayerState::Pending => "pending".into(),
                    PageTextLayerState::Ready(_, blocks) => format!("{} blocks", blocks.len()),
                    PageTextLayerState::Failed(error) => format!("failed: {error}"),
                };
                format!("page {page}: {layer}")
            })
            .collect::<Vec<_>>();
        layers.sort();
        format!(
            "layers {layers:?} probe {:?} hover {:?} shown {:?} sending {:?} rejected {:?}",
            state.hover_probe.map(|(_, point)| (point.x, point.y)),
            self.page_text_hover,
            texts(&state.shown),
            state.sending.as_ref().map(texts),
            state.rejected.as_ref().map(texts)
        )
    }

    /// The text of the line being edited, if any.
    pub fn page_text_editing_text(&self, cx: &App) -> Option<String> {
        self.page_text_editing?;
        let (input, _) = self.page_text_input.as_ref()?;
        Some(input.read(cx).state.as_ref()?.text())
    }

    /// The text an input method is composing in the page-text editor.
    pub fn page_text_composing(&self, cx: &App) -> Option<String> {
        let (input, _) = self.page_text_input.as_ref()?;
        let state = input.read(cx).state.as_ref()?;
        let range = state.marked()?;
        Some(
            state
                .text()
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect(),
        )
    }

    pub(super) fn page_text_overlays(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Vec<PageTextOverlay> {
        let mut overlays: Vec<PageTextOverlay> = Vec::new();
        let warning = cx.theme().danger;
        let editing = self
            .page_text_editing
            .filter(|editing| editing.document_id == document_id);
        let editing_layer =
            editing.and_then(|editing| self.page_text_layer(document_id, editing.page_index, cx));
        if let Some(editing) = editing
            && let Some((input, _)) = &self.page_text_input
            && let Some(state) = input.read(cx).state.as_ref()
        {
            let selection = state.selection_quads();
            let caret = selection
                .is_empty()
                .then(|| state.caret_segment())
                .flatten();
            let frame = state.frame_quads();
            // Edited text never moves other text; the frame warns when a
            // line runs into it (added below, or grown into a table's next
            // cell).
            let anchors = state.paragraph().anchors();
            let overlaps = editing_layer.as_ref().is_some_and(|layer| {
                // Text the paragraph already overlapped does not count.
                let original = layer
                    .lines
                    .iter()
                    .filter(|line| anchors.contains(&line.anchor))
                    .map(|line| line.quad(0., line.end_x))
                    .collect::<Vec<_>>();
                let others = layer
                    .lines
                    .iter()
                    .filter(|line| !anchors.contains(&line.anchor) && !line.text.trim().is_empty())
                    .map(|line| line.quad(0., line.end_x))
                    .filter(|other| !original.iter().any(|quad| quads_overlap(quad, other)))
                    .collect::<Vec<_>>();
                state
                    .wrapped()
                    .iter()
                    .zip(&frame)
                    .filter(|(line, _)| !line.unchanged)
                    .any(|(_, quad)| others.iter().any(|other| quads_overlap(quad, other)))
            });
            overlays.push(PageTextOverlay {
                page_index: editing.page_index,
                paragraphs: Vec::new(),
                hover: Vec::new(),
                frame,
                frame_color: overlaps.then_some(warning),
                selection,
                caret,
                marked: state.marked_segments(),
                breaks: state.break_marks(),
                wraps: state.wrap_marks(),
                input: Some((input.clone(), input.read(cx).focus_handle(cx))),
            });
        }
        if let Some(hover) = self
            .page_text_hover
            .clone()
            .filter(|hover| hover.document_id == document_id && hover.editable)
            && self.page_text_editing.is_none_or(|editing| {
                (editing.page_index, editing.anchor) != (hover.page_index, hover.anchor)
            })
            && let Some((layer, blocks)) = self.page_text_blocks(document_id, hover.page_index, cx)
            && let Some(block) = blocks.iter().find(|block| block.lines[0] == hover.anchor)
        {
            let quads = self.paragraph_line_quads(document_id, hover.page_index, &layer, block, cx);
            match overlays
                .iter_mut()
                .find(|overlay| overlay.page_index == hover.page_index)
            {
                Some(overlay) => overlay.hover = quads,
                None => overlays.push(PageTextOverlay {
                    page_index: hover.page_index,
                    paragraphs: Vec::new(),
                    hover: quads,
                    frame: Vec::new(),
                    frame_color: None,
                    selection: Vec::new(),
                    caret: None,
                    marked: Vec::new(),
                    breaks: Vec::new(),
                    wraps: Vec::new(),
                    input: None,
                }),
            }
        }
        // With Edit PDF Content active, every paragraph on the visible pages is
        // outlined faintly, except the one being edited.
        let tool_active =
            self.annotation_tool(document_id, cx) == Some(AnnotationTool::EditPdfContent);
        let visible = self
            .session(document_id, cx)
            .and_then(|session| {
                session
                    .read(cx)
                    .viewer
                    .plan_snapshot()
                    .map(|plan| plan.visible_pages.to_vec())
            })
            .unwrap_or_default();
        for page in visible.into_iter().filter(|_| tool_active) {
            let Ok(page) = u32::try_from(page) else {
                continue;
            };
            let Some(outlines) = self.page_paragraph_outlines(document_id, page, cx) else {
                continue;
            };
            let paragraphs = outlines
                .iter()
                .filter(|(anchor, _)| {
                    editing.is_none_or(|editing| {
                        (editing.page_index, editing.anchor) != (page, *anchor)
                    })
                })
                .flat_map(|(_, quads)| quads.iter().copied())
                .collect::<Vec<_>>();
            match overlays
                .iter_mut()
                .find(|overlay| overlay.page_index == page)
            {
                Some(overlay) => overlay.paragraphs = paragraphs,
                None => overlays.push(PageTextOverlay {
                    page_index: page,
                    paragraphs,
                    hover: Vec::new(),
                    frame: Vec::new(),
                    frame_color: None,
                    selection: Vec::new(),
                    caret: None,
                    marked: Vec::new(),
                    breaks: Vec::new(),
                    wraps: Vec::new(),
                    input: None,
                }),
            }
        }
        overlays
    }
}

/// How many pages either side of the visible ones keep their text.
const KEPT_PAGES_AROUND_VIEW: i64 = 8;

/// Whether two page-space quads' bounding boxes overlap.
fn quads_overlap(a: &Quad, b: &Quad) -> bool {
    let bounds = |quad: &Quad| {
        let xs = quad.iter().map(|(x, _)| *x);
        let ys = quad.iter().map(|(_, y)| *y);
        (
            xs.clone().fold(f64::INFINITY, f64::min),
            ys.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
            ys.fold(f64::NEG_INFINITY, f64::max),
        )
    };
    // The middle of each box: tightly set lines' full boxes touch.
    let core = |(x0, y0, x1, y1): (f64, f64, f64, f64)| {
        let inset = (y1 - y0) * 0.25;
        (x0, y0 + inset, x1, y1 - inset)
    };
    let (a, b) = (core(bounds(a)), core(bounds(b)));
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

macro_rules! forward_actions {
    ($element:expr, $input:expr, [$(($action:ty, $method:ident)),* $(,)?]) => {{
        let mut element = $element;
        $(
            let input = $input.clone();
            element = element.on_action(move |action: &$action, window, cx| {
                input.update(cx, |input, cx| input.$method(action, window, cx));
            });
        )*
        element
    }};
}

/// Draws a page's Edit PDF Content overlay: the hovered line, and the edited
/// line's frame, selection and caret.
pub(super) fn page_text_overlay_element(
    overlay: PageTextOverlay,
    page_size: (f32, f32),
    coordinate_space: PageCoordinateSpace,
    accent: gpui::Hsla,
) -> AnyElement {
    let painted = overlay;
    let layer = gpui::div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let Some((page_bounds, transform)) =
                    contained_page_bounds_for_space(bounds, page_size, coordinate_space)
                else {
                    return;
                };
                let project = |(x, y): (f64, f64)| {
                    let local = transform.point_to_local_pixels(PdfPoint { x, y });
                    point(
                        page_bounds.origin.x + px(local.x as f32),
                        page_bounds.origin.y + px(local.y as f32),
                    )
                };
                let fill_quad = |quad: &Quad, color: gpui::Hsla, window: &mut Window| {
                    let mut builder = PathBuilder::fill();
                    builder.move_to(project(quad[0]));
                    for corner in &quad[1..] {
                        builder.line_to(project(*corner));
                    }
                    builder.close();
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, color);
                    }
                };
                let stroke_quad = |quad: &Quad, color: gpui::Hsla, window: &mut Window| {
                    let mut builder = PathBuilder::stroke(px(1.));
                    builder.move_to(project(quad[0]));
                    for corner in &quad[1..] {
                        builder.line_to(project(*corner));
                    }
                    builder.close();
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, color);
                    }
                };
                for quad in &painted.paragraphs {
                    stroke_quad(quad, accent.opacity(0.35), window);
                }
                for quad in &painted.hover {
                    fill_quad(quad, accent.opacity(0.08), window);
                    stroke_quad(quad, accent.opacity(0.6), window);
                }
                for quad in &painted.frame {
                    stroke_quad(quad, painted.frame_color.unwrap_or(accent), window);
                }
                for quad in &painted.selection {
                    fill_quad(quad, accent.opacity(0.3), window);
                }
                let mark_color = painted.frame_color.unwrap_or(accent);
                let marks = painted
                    .breaks
                    .iter()
                    .map(|stroke| (stroke, mark_color))
                    .chain(
                        painted
                            .wraps
                            .iter()
                            .map(|stroke| (stroke, mark_color.opacity(0.45))),
                    );
                for (stroke, color) in marks {
                    let mut builder = PathBuilder::stroke(px(1.));
                    builder.move_to(project(stroke[0]));
                    for point in &stroke[1..] {
                        builder.line_to(project(*point));
                    }
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, color);
                    }
                }
                for [start, end] in &painted.marked {
                    // Composing text is underlined, as in other text fields.
                    let mut builder = PathBuilder::stroke(px(1.));
                    builder.move_to(project(*start));
                    builder.line_to(project(*end));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, gpui::black());
                    }
                }
                if let Some([bottom, top]) = painted.caret {
                    let mut builder = PathBuilder::stroke(px(1.5));
                    builder.move_to(project(bottom));
                    builder.line_to(project(top));
                    if let Ok(path) = builder.build() {
                        // Paper-relative: the caret is drawn on the page.
                        window.paint_path(path, gpui::black());
                    }
                }
                if let Some((input, _)) = &painted.input {
                    // Page space to window pixels, for the IME.
                    let along = project((1., 0.));
                    let up = project((0., 1.));
                    let base = project((0., 0.));
                    let screen = PageTextScreenMap {
                        matrix: [
                            f64::from(f32::from(along.x - base.x)),
                            f64::from(f32::from(along.y - base.y)),
                            f64::from(f32::from(up.x - base.x)),
                            f64::from(f32::from(up.y - base.y)),
                            f64::from(f32::from(base.x)),
                            f64::from(f32::from(base.y)),
                        ],
                    };
                    input.update(cx, |input, _| input.screen_map = Some(screen));
                }
            },
        )
        .absolute()
        .inset_0(),
    );
    layer.into_any_element()
}

/// Receives the editor's keys and IME input. It sits outside the pages so
/// it stays mounted, and focused, while the pages re-render.
pub(super) fn page_text_input_element(
    input: Entity<PageTextInput>,
    focus: FocusHandle,
) -> AnyElement {
    let handler_input = input.clone();
    let handler_focus = focus.clone();
    let element = gpui::div()
        .id("document-page-text-editor")
        .absolute()
        .left_0()
        .top_0()
        .size(px(1.))
        .key_context(PAGE_TEXT_EDITOR_CONTEXT)
        .track_focus(&focus)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &handler_focus,
                        ElementInputHandler::new(bounds, handler_input.clone()),
                        cx,
                    );
                },
            )
            .size_full(),
        );
    forward_actions!(
        element,
        input,
        [
            (Backspace, backspace),
            (DeleteForward, delete_forward),
            (DeleteWordBackward, delete_word_backward),
            (DeleteWordForward, delete_word_forward),
            (DeleteToLineStart, delete_to_line_start),
            (MoveLeft, move_left),
            (MoveRight, move_right),
            (MoveWordLeft, move_word_left),
            (MoveWordRight, move_word_right),
            (MoveLineStart, move_line_start),
            (MoveLineEnd, move_line_end),
            (MoveUp, move_up),
            (MoveDown, move_down),
            (SelectUp, select_up),
            (SelectDown, select_down),
            (InsertNewline, insert_newline),
            (SelectLeft, select_left),
            (SelectRight, select_right),
            (SelectWordLeft, select_word_left),
            (SelectWordRight, select_word_right),
            (SelectLineStart, select_line_start),
            (SelectLineEnd, select_line_end),
            (SelectAllText, select_all),
            (CopyText, copy),
            (CutText, cut),
            (PasteText, paste),
            (UndoText, undo),
            (RedoText, redo),
            (FinishEditing, finish),
        ]
    )
    .into_any_element()
}
