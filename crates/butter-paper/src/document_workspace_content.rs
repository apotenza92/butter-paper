//! Edit PDF mode: a per-document switch under which the Select tool edits
//! and moves the page's own content (text, drawings and images) where no
//! markup or form field takes a press.

use gpui::{App, Context, CursorStyle, Modifiers, Pixels, Window};

use super::{DocumentId, DocumentWorkspace, NativeDocumentStatus, PageInteraction};
use crate::annotation_adapter::AnnotationTool;
use crate::annotation_model::{MarkupId, PdfPoint, RectangleResizeHandle};
use crate::pdf_content_moves::{
    ContentChange, ContentKind, ContentMove, MovedContent, PageObject, Placement,
};
use crate::pdf_text_selection::Quad;

/// Page content selected in Edit PDF mode: one or more groups (a
/// paragraph's lines, a drawing or an image), moved together.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentSelection {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub groups: Vec<Vec<MovedContent>>,
    /// Markups selected with it, which move with it.
    pub markups: Vec<MarkupId>,
}

/// A selection being dragged: from where the press was, in page space,
/// and how the drag changes it so far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentDrag {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub start: (f64, f64),
    pub change: ContentChange,
    /// Moving the selection, or resizing it from a handle of its bounds
    /// as shown when the drag began.
    pub mode: DragMode,
    /// The pointer has moved far enough to be a drag, not a click.
    pub moved: bool,
    /// Arrow keys move it, not the pointer: it ends once they are let go.
    pub keyboard: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragMode {
    Move,
    Resize {
        handle: RectangleResizeHandle,
        bounds: [f64; 4],
    },
}

/// The picture of content being dragged, cut from the page as shown when
/// the drag began, each piece painted where the drag puts it while the
/// worker shows the page without it. Once dropped, it stays where it was
/// dropped until the page shows the content there.
#[derive(Clone)]
pub(super) struct ContentSprite {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub pieces: Vec<SpritePiece>,
    /// Where the pieces were dropped: their shown bounds.
    pub dropped: Option<Vec<[f64; 4]>>,
    /// The edits that show the drop went to the worker, and came back.
    pub sent: bool,
    pub replied: bool,
}

/// A part of a dragged selection: where it was shown when the drag began,
/// in page space, which group (and item, for drawings and images) it is,
/// and its picture, a pixel wider each side.
#[derive(Clone)]
pub(super) struct SpritePiece {
    pub from: [f64; 4],
    pub group: usize,
    pub item: Option<usize>,
    pub image: std::sync::Arc<gpui::RenderImage>,
}

/// A sprite as painted: each piece's bounds now, in page space, and its
/// picture.
#[derive(Clone)]
pub(super) struct SpritePaint {
    pub page_index: u32,
    pub pieces: Vec<([f64; 4], std::sync::Arc<gpui::RenderImage>)>,
}

impl DocumentWorkspace {
    /// Whether the document is in Edit PDF mode.
    pub fn content_edit_mode(&self, document_id: DocumentId) -> bool {
        self.content_edit_mode.contains(&document_id)
    }

    /// How many pieces of content (paragraphs, drawings, images) are
    /// selected on the document, in Edit PDF mode.
    pub fn selected_content_count(&self, document_id: DocumentId) -> usize {
        self.content_selection
            .as_ref()
            .filter(|selection| selection.document_id == document_id)
            .map_or(0, |selection| selection.groups.len())
    }

    /// Whether a press on the document's pages edits its content: Edit PDF
    /// mode with the Select tool.
    pub(super) fn editing_content(&self, document_id: DocumentId, cx: &App) -> bool {
        self.content_edit_mode(document_id)
            && self.annotation_tool(document_id, cx) == Some(AnnotationTool::Select)
            && !self.pan_tool_active
    }

    /// Turns Edit PDF mode on or off for the document. Turning it on arms
    /// the Select tool; turning it off finishes the edit in progress.
    pub fn set_content_edit_mode(
        &mut self,
        document_id: DocumentId,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        if on == self.content_edit_mode(document_id) {
            return;
        }
        if on {
            self.content_edit_mode.insert(document_id);
            if self.annotation_tool(document_id, cx) != Some(AnnotationTool::Select)
                || self.pan_tool_active
            {
                let _ = self.set_annotation_tool(document_id, AnnotationTool::Select, cx);
                self.pan_tool_active = false;
            }
            self.prefetch_page_text(document_id, cx);
        } else {
            self.content_edit_mode.remove(&document_id);
        }
        self.reconcile_page_text_editing(cx);
        self.sync_active_viewer_toolbar(cx);
        cx.notify();
    }

    pub fn toggle_content_edit_mode(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let on = !self.content_edit_mode(document_id);
        self.set_content_edit_mode(document_id, on, cx);
    }
}

impl DocumentWorkspace {
    /// The content moves the document has.
    pub(super) fn page_content_moves_of(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<ContentMove> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_page_content_moves(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// The content moves the pages should show: the document's, with the
    /// selection being dragged taken off the page while its picture is
    /// painted where the drag puts it (moved, when it has no picture).
    pub(super) fn desired_page_content_moves(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<ContentMove> {
        let mut moves = self.page_content_moves_of(document_id, cx);
        let Some((groups, _, change)) = self
            .content_drag
            .and_then(|drag| self.dragged_content(document_id, drag.page_index))
        else {
            return moves;
        };
        let page_index = self.content_drag.map_or(0, |drag| drag.page_index);
        let pictured = self.content_sprite.as_ref().is_some_and(|sprite| {
            sprite.document_id == document_id
                && sprite.page_index == page_index
                && sprite.dropped.is_none()
        });
        for item in groups.iter().flatten() {
            moves.retain(|existing| {
                !(existing.page_index == page_index && existing.is(item.kind, item.rect, item.copy))
            });
            // The page shows it gone while its picture follows the drag;
            // without a picture, moved.
            let placement = if pictured {
                item.from
            } else {
                crate::pdf_content_moves::changed_placement(item, change)
            };
            moves.push(ContentMove {
                page_index,
                kind: item.kind,
                rect: item.rect,
                offset: placement.offset,
                scale: placement.scale,
                removed: pictured,
                copy: item.copy,
            });
        }
        moves
    }

    /// [`Self::desired_page_content_moves`] on one page.
    pub(super) fn shown_page_content_moves(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Vec<ContentMove> {
        self.desired_page_content_moves(document_id, cx)
            .into_iter()
            .filter(|item| item.page_index == page_index)
            .collect()
    }

    /// The selection being dragged on the page, and how far.
    pub(super) fn dragged_content(
        &self,
        document_id: DocumentId,
        page_index: u32,
    ) -> Option<(Vec<Vec<MovedContent>>, Vec<MarkupId>, ContentChange)> {
        let drag = self.content_drag.filter(|drag| {
            drag.document_id == document_id && drag.page_index == page_index && drag.moved
        })?;
        let selection = self.content_selection.as_ref().filter(|selection| {
            selection.document_id == document_id && selection.page_index == page_index
        })?;
        Some((
            selection.groups.clone(),
            selection.markups.clone(),
            drag.change,
        ))
    }
}

/// The drawing, image or copy under the pointer: the content a press
/// selects, and its bounds as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentHover {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub group: Vec<MovedContent>,
    pub bounds: [f64; 4],
}

/// A resize handle's size on screen, in pixels.
pub(super) const HANDLE_SIZE_PIXELS: f32 = 8.;
/// How far, in pixels, a frame and its handles stand off the content.
pub(super) const FRAME_PADDING_PIXELS: f32 = 3.;

/// How far, in pixels, a press must move to drag content rather than click.
const DRAG_THRESHOLD_PIXELS: f64 = 3.;
/// How far outside a paragraph, in pixels, a press moves it rather than
/// editing its text.
const MOVE_BAND_PIXELS: f64 = 10.;
/// How far an arrow key moves the selection, in points, and with Shift.
const NUDGE: f64 = 1.;
const NUDGE_FAR: f64 = 10.;
/// How long after the last arrow key a keyboard move ends, if its key's
/// release is not seen.
const NUDGE_SETTLE: std::time::Duration = std::time::Duration::from_millis(800);
/// How far each paste lands from the copied content, in points, right and
/// down.
const PASTE_STEP: f64 = 10.;

/// Content copied in Edit PDF mode, to paste as copies on its page.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentClipboard {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub groups: Vec<Vec<MovedContent>>,
    /// How often it was pasted, each a step further away.
    pub pastes: u32,
}

impl DocumentWorkspace {
    /// Whether Edit PDF mode can be turned on for the document: it is
    /// open and ready.
    pub fn content_edit_available(&self, document_id: DocumentId, cx: &App) -> bool {
        self.session(document_id, cx)
            .is_some_and(|session| matches!(session.read(cx).status, NativeDocumentStatus::Ready))
    }

    /// Whether the document has content selected or being edited, which
    /// the Edit PDF properties show.
    pub(super) fn content_in_hand(&self, document_id: DocumentId) -> bool {
        self.page_text_editing
            .is_some_and(|editing| editing.document_id == document_id)
            || self
                .content_selection
                .as_ref()
                .is_some_and(|selection| selection.document_id == document_id)
    }

    /// The copy shown under `point`: copies are drawn over the page's own
    /// content, so they are found first.
    fn content_copy_at(
        &self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> Option<ContentHover> {
        let (document_id, page_index) = (interaction.document_id, interaction.page_index);
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        let mut groups: std::collections::BTreeMap<u32, Vec<MovedContent>> = Default::default();
        for item in moves.iter().filter(|item| !item.removed) {
            let Some(copy) = item.copy else {
                continue;
            };
            groups.entry(copy).or_default().push(MovedContent {
                kind: item.kind,
                rect: item.rect,
                from: item.placement(),
                copy: Some(copy),
            });
        }
        let tolerance = interaction.transform.tolerance_points(4.).unwrap_or(2.);
        let area = |rect: [f64; 4]| (rect[2] - rect[0]) * (rect[3] - rect[1]);
        groups
            .into_values()
            .filter_map(|group| {
                let bounds = group.iter().map(|item| item.shown()).reduce(union)?;
                crate::pdf_content_moves::contains(
                    outset(bounds, tolerance),
                    [point.x, point.y, point.x, point.y],
                    0.,
                )
                .then_some((group, bounds))
            })
            .min_by(|(_, a), (_, b)| area(*a).total_cmp(&area(*b)))
            .map(|(group, bounds)| ContentHover {
                document_id,
                page_index,
                group,
                bounds,
            })
    }

    /// The drawing or image shown under `point`, outside the page's tables.
    fn content_object_at(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<ContentHover> {
        let (document_id, page_index) = (interaction.document_id, interaction.page_index);
        let layer = self.page_text_layer(document_id, page_index, cx)?;
        if layer.objects.is_empty() {
            return None;
        }
        let (_, _, tables) = self.loaded_page_text(document_id, page_index)?;
        let excluded = tables
            .iter()
            .map(|table| {
                [
                    table.columns[0],
                    *table.rows.last().unwrap_or(&table.rows[0]),
                    *table.columns.last().unwrap_or(&table.columns[0]),
                    table.rows[0],
                ]
            })
            .collect::<Vec<_>>();
        let shown_tables = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown_tables.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        // Each object where it is shown.
        let shown = layer
            .objects
            .iter()
            .filter_map(|object| {
                let placement = crate::pdf_content_moves::placement_of(
                    &refs,
                    &moves,
                    object.kind,
                    object.rect,
                )?;
                Some((object.clone(), placement))
            })
            .collect::<Vec<_>>();
        let shown_objects = shown
            .iter()
            .map(|(object, placement)| PageObject {
                rect: placement.shown(object.rect),
                ..object.clone()
            })
            .collect::<Vec<_>>();
        // Tables are excluded where their base grid is: their parts are
        // edited through the table.
        let tolerance = interaction.transform.tolerance_points(4.).unwrap_or(2.);
        let picked = crate::pdf_content_moves::object_at(
            &shown_objects,
            layer.page_box,
            &excluded,
            (point.x, point.y),
            tolerance,
        )?;
        let index = shown_objects.iter().position(|object| *object == picked)?;
        let (object, placement) = shown[index].clone();
        let group = self.object_content(document_id, page_index, &object, placement, cx);
        Some(ContentHover {
            document_id,
            page_index,
            group,
            bounds: picked.rect,
        })
    }

    /// A drawing or image as content to move, with the text lines its clip
    /// clips.
    fn object_content(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        object: &PageObject,
        placement: Placement,
        cx: &mut Context<Self>,
    ) -> Vec<MovedContent> {
        let shown_tables = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown_tables.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        std::iter::once(MovedContent {
            kind: object.kind,
            rect: object.rect,
            from: placement,
            copy: None,
        })
        .chain(object.texts.iter().filter_map(|&rect| {
            let offset = crate::pdf_content_moves::line_offset(&refs, &moves, rect)?;
            Some(MovedContent {
                kind: ContentKind::Text,
                rect,
                from: Placement::moved(offset),
                copy: None,
            })
        }))
        .collect()
    }

    /// The drawing whose clip holds a line of `block`, and how far it is
    /// shown from where the page draws it.
    fn drawing_holding(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        block: &crate::pdf_text_blocks::PageTextBlock,
        cx: &mut Context<Self>,
    ) -> Option<(PageObject, Placement)> {
        let layer = self.page_text_layer(document_id, page_index, cx)?;
        let lines = block
            .lines
            .iter()
            .filter_map(|anchor| layer.line(anchor))
            .filter_map(crate::pdf_tables::line_core_box)
            .collect::<Vec<_>>();
        let object = layer
            .objects
            .iter()
            .find(|object| {
                object.texts.iter().any(|text| {
                    lines
                        .iter()
                        .any(|line| line.iter().zip(text).all(|(a, b)| (a - b).abs() < 0.01))
                })
            })?
            .clone();
        let shown_tables = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown_tables.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        let placement =
            crate::pdf_content_moves::placement_of(&refs, &moves, object.kind, object.rect)?;
        Some((object, placement))
    }

    /// The paragraph (outside tables) whose edge band holds `point`: a
    /// press there moves the paragraph, or the drawing whose clip holds
    /// it, as a markup's edge moves it. Inside the paragraph a press edits
    /// its text.
    fn paragraph_band_at(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<ContentHover> {
        let (document_id, page_index) = (interaction.document_id, interaction.page_index);
        let band = interaction
            .transform
            .tolerance_points(MOVE_BAND_PIXELS)
            .ok()?;
        let (_, blocks) = self.page_text_blocks(document_id, page_index, cx)?;
        let area = |rect: [f64; 4]| (rect[2] - rect[0]) * (rect[3] - rect[1]);
        let mut best: Option<ContentHover> = None;
        for block in blocks.iter().filter(|block| block.cell.is_none()) {
            let group = match self.drawing_holding(document_id, page_index, block, cx) {
                Some((object, placement)) => {
                    self.object_content(document_id, page_index, &object, placement, cx)
                }
                None => self.paragraph_content(document_id, page_index, block, cx),
            };
            let Some(bounds) = self.group_frame(
                document_id,
                page_index,
                &group,
                ContentChange::Move(0., 0.),
                cx,
            ) else {
                continue;
            };
            if !crate::pdf_content_moves::contains(
                outset(bounds, band),
                [point.x, point.y, point.x, point.y],
                0.,
            ) || best
                .as_ref()
                .is_some_and(|best| area(best.bounds) <= area(bounds))
            {
                continue;
            }
            best = Some(ContentHover {
                document_id,
                page_index,
                group,
                bounds,
            });
        }
        best
    }

    /// The band round a paragraph that a grown table now covers, holding
    /// `point` where the table's cell would take it: the band still moves
    /// the paragraph out of the way, its text inside staying the cell's.
    fn covered_paragraph_band(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<ContentHover> {
        let in_cell = self
            .page_text_and_table_hit(interaction, point, cx)
            .is_some_and(|hit| hit.cell.is_some());
        if !in_cell {
            return None;
        }
        self.paragraph_band_at(interaction, point, cx)
            .filter(|band| {
                !crate::pdf_content_moves::contains(
                    band.bounds,
                    [point.x, point.y, point.x, point.y],
                    0.,
                )
            })
    }

    /// A paragraph's lines as content to move.
    fn paragraph_content(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        block: &crate::pdf_text_blocks::PageTextBlock,
        cx: &mut Context<Self>,
    ) -> Vec<MovedContent> {
        let Some(layer) = self.page_text_layer(document_id, page_index, cx) else {
            return Vec::new();
        };
        let shown_tables = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown_tables.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        block
            .lines
            .iter()
            .filter_map(|anchor| layer.line(anchor))
            .filter_map(|line| {
                let rect = crate::pdf_tables::line_core_box(line)?;
                let offset = crate::pdf_content_moves::line_offset(&refs, &moves, rect)?;
                Some(MovedContent {
                    kind: ContentKind::Text,
                    rect,
                    from: Placement::moved(offset),
                    copy: None,
                })
            })
            .collect()
    }

    /// A press in Edit PDF mode. Markups and form fields take it first;
    /// then a press in text edits it (placing the caret, or selecting the
    /// word on a double click), and a press on a drawing, an image or the
    /// band round a paragraph selects it to drag. A press on empty page
    /// ends any edit or selection; with none, it returns false and the
    /// Select tool takes it.
    pub(super) fn begin_content_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        click_count: usize,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let document_id = interaction.document_id;
        self.settle_content_nudge(cx);
        // The selection's resize handles are on top of everything.
        if let Some((handle, bounds)) = self.content_handle_at(&interaction, point) {
            self.content_drag = Some(ContentDrag {
                document_id,
                page_index: interaction.page_index,
                start: (point.x, point.y),
                change: ContentChange::Move(0., 0.),
                mode: DragMode::Resize { handle, bounds },
                moved: false,
                keyboard: false,
            });
            return true;
        }
        if self.markup_under_point(&interaction, point, cx)
            || self.form_widget_hit(&interaction, point, cx).is_some()
        {
            self.commit_page_text_editing(cx);
            // A press on a selected markup's body while content is selected
            // too drags them all.
            let page_index = interaction.page_index;
            let content_selected = self.content_selection.as_ref().is_some_and(|selection| {
                selection.document_id == document_id && selection.page_index == page_index
            });
            if content_selected && self.press_on_selected_markup_body(&interaction, point, cx) {
                let markups = self.selected_markups_on(document_id, page_index, cx);
                if let Some(selection) = self.content_selection.as_mut() {
                    selection.markups = markups;
                }
                self.begin_content_drag(document_id, page_index, point);
                return true;
            }
            // Shift adds a markup to the selection and keeps the content.
            if !modifiers.shift {
                self.clear_content_selection(cx);
            }
            return false;
        }
        // A table's inner column border, which a drag resizes.
        if self.begin_column_drag(interaction, point, cx) {
            return true;
        }
        // Copies are drawn over the page's own content.
        if let Some(hover) = self.content_copy_at(interaction, point, cx) {
            self.commit_page_text_editing(cx);
            self.select_content(interaction, point, hover.group, modifiers.shift, cx);
            self.workspace_focus.focus(window, cx);
            return true;
        }
        if let Some(hover) = self.covered_paragraph_band(interaction, point, cx) {
            self.commit_page_text_editing(cx);
            self.select_content(interaction, point, hover.group, modifiers.shift, cx);
            self.workspace_focus.focus(window, cx);
            return true;
        }
        if self
            .page_text_and_table_hit(interaction, point, cx)
            .is_some()
        {
            self.clear_content_selection(cx);
            self.clear_markup_selection(document_id, cx);
            return self.begin_page_text_press(
                interaction,
                point,
                click_count,
                modifiers,
                window,
                cx,
            );
        }
        if let Some(hover) = self.content_object_at(interaction, point, cx) {
            self.commit_page_text_editing(cx);
            self.select_content(interaction, point, hover.group, modifiers.shift, cx);
            self.workspace_focus.focus(window, cx);
            return true;
        }
        if let Some(hover) = self.paragraph_band_at(interaction, point, cx) {
            self.commit_page_text_editing(cx);
            self.select_content(interaction, point, hover.group, modifiers.shift, cx);
            self.workspace_focus.focus(window, cx);
            return true;
        }
        // Empty page: a press ends the edit or selection and does no more,
        // as a press off a selected markup does. With nothing in hand,
        // Select takes it, and its selection box selects content too.
        let in_hand = self.content_in_hand(document_id);
        self.commit_page_text_editing(cx);
        self.clear_content_selection(cx);
        if in_hand {
            self.clear_markup_selection(document_id, cx);
            cx.notify();
            return true;
        }
        self.content_marquee = Some((document_id, interaction.page_index, point));
        false
    }

    /// Selects `group` (adding it with Shift) and arms a drag from `point`.
    /// A press on content already selected keeps the selection, to drag it.
    fn select_content(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        group: Vec<MovedContent>,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if group.is_empty()
            || self.hold_restricted_content(interaction.document_id, interaction.page_index, cx)
        {
            return;
        }
        let document_id = interaction.document_id;
        let page_index = interaction.page_index;
        let same_page = self.content_selection.clone().filter(|selection| {
            selection.document_id == document_id && selection.page_index == page_index
        });
        let already = same_page.as_ref().is_some_and(|selection| {
            selection.groups.iter().any(|existing| {
                existing
                    .iter()
                    .any(|item| group.iter().any(|new| new.same(item)))
            })
        });
        // A press on content already selected, or with Shift, keeps the
        // selected markups to move with it; otherwise they are deselected.
        if !(already || extend) {
            self.clear_markup_selection(document_id, cx);
        }
        let mut groups = match same_page {
            Some(selection) if already || extend => selection.groups,
            _ => Vec::new(),
        };
        if !already {
            groups.push(group);
        }
        self.content_selection = Some(ContentSelection {
            document_id,
            page_index,
            groups,
            markups: self.selected_markups_on(document_id, page_index, cx),
        });
        self.begin_content_drag(document_id, page_index, point);
        self.annotation_statuses.remove(&document_id);
        cx.notify();
    }

    /// Whether a press at `point` is on the body of a selected markup (not
    /// one of its handles).
    fn press_on_selected_markup_body(
        &self,
        interaction: &PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> bool {
        let Some(session) = self.session(interaction.document_id, cx) else {
            return false;
        };
        let Ok(tolerance) = interaction.transform.tolerance_points(4.) else {
            return false;
        };
        let annotations = &session.read(cx).annotations;
        let document = interaction.document_id.value();
        if annotations
            .hover_transform_handle(document, interaction.page_index, point, tolerance)
            .ok()
            .flatten()
            .is_some()
        {
            return false;
        }
        annotations
            .hover_markup_id(document, interaction.page_index, point, tolerance)
            .ok()
            .flatten()
            .is_some_and(|id| annotations.selected_ids(document).contains(&id))
    }

    /// The markups selected on the document; those on the content's page
    /// move with it.
    fn selected_markups_on(
        &self,
        document_id: DocumentId,
        _page_index: u32,
        cx: &App,
    ) -> Vec<MarkupId> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .selected_ids(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    fn begin_content_drag(&mut self, document_id: DocumentId, page_index: u32, point: PdfPoint) {
        self.content_drag = Some(ContentDrag {
            document_id,
            page_index,
            start: (point.x, point.y),
            change: ContentChange::Move(0., 0.),
            mode: DragMode::Move,
            moved: false,
            keyboard: false,
        });
    }

    /// Selects the paragraphs (outside tables), drawings, images and copies
    /// the selection marquee on the page selects, by the rule it selects
    /// markups with. Returns false while the marquee is not yet drawn: a
    /// click arms a box the next click finishes.
    pub(super) fn select_content_in_marquee(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((layer, blocks)) = self.page_text_blocks(document_id, page_index, cx) else {
            return true;
        };
        let mut candidates: Vec<Vec<MovedContent>> = Vec::new();
        for block in blocks.iter().filter(|block| block.cell.is_none()) {
            // Text inside a drawing's clip comes with the drawing.
            if self
                .drawing_holding(document_id, page_index, block, cx)
                .is_some()
            {
                continue;
            }
            let group = self.paragraph_content(document_id, page_index, block, cx);
            if !group.is_empty() {
                candidates.push(group);
            }
        }
        let tables = self
            .loaded_page_text(document_id, page_index)
            .map(|(_, _, tables)| tables)
            .unwrap_or_default();
        let shown_tables = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown_tables.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        let page_area = layer
            .page_box
            .map(|rect| (rect[2] - rect[0]) * (rect[3] - rect[1]));
        for object in layer.objects.iter() {
            let in_table = tables.iter().any(|table| {
                crate::pdf_content_moves::contains(
                    [
                        table.columns[0],
                        *table.rows.last().unwrap_or(&table.rows[0]),
                        *table.columns.last().unwrap_or(&table.columns[0]),
                        table.rows[0],
                    ],
                    object.rect,
                    1.,
                )
            });
            let size = (object.rect[2] - object.rect[0]) * (object.rect[3] - object.rect[1]);
            if in_table || page_area.is_some_and(|page| size >= 0.8 * page) {
                continue;
            }
            let Some(placement) =
                crate::pdf_content_moves::placement_of(&refs, &moves, object.kind, object.rect)
            else {
                continue;
            };
            let object = object.clone();
            candidates.push(self.object_content(document_id, page_index, &object, placement, cx));
        }
        // Copies, grouped by their number.
        let mut copies: std::collections::BTreeMap<u32, Vec<MovedContent>> = Default::default();
        for item in moves.iter().filter(|item| !item.removed) {
            if let Some(copy) = item.copy {
                copies.entry(copy).or_default().push(MovedContent {
                    kind: item.kind,
                    rect: item.rect,
                    from: item.placement(),
                    copy: Some(copy),
                });
            }
        }
        candidates.extend(copies.into_values());
        let rects = candidates
            .iter()
            .filter_map(|group| group.iter().map(|item| item.shown()).reduce(union))
            .collect::<Vec<_>>();
        let Some(selected) = self.session(document_id, cx).and_then(|session| {
            session.read(cx).annotations.marquee_selects_rects(
                document_id.value(),
                page_index,
                &rects,
            )
        }) else {
            return false;
        };
        let groups = candidates
            .into_iter()
            .zip(selected)
            .filter_map(|(group, selected)| selected.then_some(group))
            .collect::<Vec<_>>();
        if !groups.is_empty() && !self.hold_restricted_content(document_id, page_index, cx) {
            self.content_selection = Some(ContentSelection {
                document_id,
                page_index,
                groups,
                markups: Vec::new(),
            });
            cx.notify();
        }
        true
    }

    /// Selects the paragraph whose first line is `anchor`, in Edit PDF
    /// mode.
    pub(super) fn select_paragraph(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        anchor: crate::pdf_text_edit::TextLineAnchor,
        cx: &mut Context<Self>,
    ) {
        if !self.editing_content(document_id, cx) {
            return;
        }
        let Some((_, blocks)) = self.page_text_blocks(document_id, page_index, cx) else {
            return;
        };
        let Some(block) = blocks
            .iter()
            .find(|block| block.lines[0] == anchor)
            .cloned()
        else {
            return;
        };
        let group = self.paragraph_content(document_id, page_index, &block, cx);
        if group.is_empty() || self.hold_restricted_content(document_id, page_index, cx) {
            return;
        }
        self.content_selection = Some(ContentSelection {
            document_id,
            page_index,
            groups: vec![group],
            markups: Vec::new(),
        });
        cx.notify();
    }

    fn clear_markup_selection(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                session.annotations.clear_selection(document_id.value());
                cx.notify();
            });
        }
    }

    /// Ends the content selection (and a drag of it, without moving it).
    pub(super) fn clear_content_selection(&mut self, cx: &mut Context<Self>) {
        // A move by arrow keys is kept.
        self.settle_content_nudge(cx);
        let dragged = self.content_drag.take().filter(|drag| drag.moved);
        if let Some(drag) = dragged {
            self.drop_content_sprite(drag.document_id, ContentChange::Move(0., 0.), cx);
        }
        if self.content_selection.take().is_some() {
            cx.notify();
        }
        if let Some(drag) = dragged {
            self.sync_document_edits(drag.document_id, cx);
        }
    }

    /// Follows the pointer while selected content is pressed: past a few
    /// pixels it drags, and the page shows it moved.
    pub(super) fn update_content_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.content_drag.filter(|drag| !drag.keyboard) else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(drag.document_id, drag.page_index))
            .copied()
        else {
            return true;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            return true;
        };
        let raw = (point.x - drag.start.0, point.y - drag.start.1);
        let threshold = interaction
            .transform
            .tolerance_points(DRAG_THRESHOLD_PIXELS)
            .unwrap_or(1.);
        if !drag.moved && raw.0.hypot(raw.1) < threshold {
            return true;
        }
        drag.change = match drag.mode {
            DragMode::Move => {
                let (dx, dy) = self.snapped_content_delta(drag, point, cx).unwrap_or(raw);
                ContentChange::Move(dx, dy)
            }
            DragMode::Resize { handle, bounds } => {
                let point = self.snapped_handle_point(drag, handle, bounds, point, cx);
                resize_change(handle, bounds, (point.x, point.y), modifiers.shift)
            }
        };
        if !drag.moved {
            self.take_content_sprite(drag.document_id, drag.page_index, cx);
        }
        drag.moved = true;
        self.content_drag = Some(drag);
        self.content_hover = None;
        self.sync_document_edits(drag.document_id, cx);
        cx.notify();
        true
    }

    /// Ends a press on selected content: a drag moves it, as one undoable
    /// change with the markups on it.
    pub(super) fn finish_content_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.content_drag.take() else {
            return false;
        };
        if let Some(session) = self.session(drag.document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                session.annotations.clear_content_move_snap();
                cx.notify();
            });
        }
        if drag.moved {
            self.drop_content_sprite(drag.document_id, drag.change, cx);
            self.change_selected_content(drag.change, cx);
        }
        true
    }

    /// Takes the picture of the selection about to be dragged, from the
    /// page's tiles as shown.
    fn take_content_sprite(&mut self, document_id: DocumentId, page_index: u32, cx: &App) {
        self.content_sprite = None;
        let Some(selection) = self.content_selection.as_ref().filter(|selection| {
            selection.document_id == document_id && selection.page_index == page_index
        }) else {
            return;
        };
        // Each piece: its bounds, place, and whether it is ink on paper
        // (text and drawings, which take the paper away) or an image.
        let mut parts = Vec::new();
        for (group_index, group) in selection.groups.iter().enumerate() {
            if group.iter().all(|item| item.kind == ContentKind::Text) {
                if let Some(from) = self.group_frame(
                    document_id,
                    page_index,
                    group,
                    ContentChange::Move(0., 0.),
                    cx,
                ) {
                    parts.push((from, group_index, None, true));
                }
                continue;
            }
            for (item_index, item) in group.iter().enumerate() {
                let ink = item.kind != ContentKind::Image;
                parts.push((item.shown(), group_index, Some(item_index), ink));
            }
        }
        let Some(interaction) = self
            .page_interactions
            .get(&(document_id, page_index))
            .copied()
        else {
            return;
        };
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let session = session.read(cx);
        let page = page_index as usize;

        let tile_pad = crate::document_resource::TILE_IMAGE_PADDING as i64;
        let tiles = session.viewer.visible_tiles(page);
        // The page's tiles, or the image of the whole page drawn under them.
        let (scale, sources) = match tiles.first() {
            Some((first, _)) => (
                first.device_scale_millis as f64 / 1_000.,
                tiles
                    .iter()
                    .map(|(request, image)| (request.crop, image.clone(), tile_pad))
                    .collect::<Vec<_>>(),
            ),
            None => {
                let Some(image) = session
                    .current_image
                    .clone()
                    .filter(|_| session.current_image_page == Some(page_index))
                else {
                    return;
                };
                // It is drawn over the page as last painted.
                let size = interaction.bounds.size;
                let crop = crate::viewer::PixelRect {
                    x: 0,
                    y: 0,
                    width: f32::from(size.width).round().max(1.) as usize,
                    height: f32::from(size.height).round().max(1.) as usize,
                };
                (1., vec![(crop, image, 0)])
            }
        };
        let pieces = parts
            .into_iter()
            .filter_map(|(from, group, item, ink)| {
                let [left, bottom, right, top] = from;
                let a = interaction
                    .transform
                    .point_to_local_pixels(PdfPoint { x: left, y: bottom });
                let b = interaction
                    .transform
                    .point_to_local_pixels(PdfPoint { x: right, y: top });
                let logical = [
                    a.x.min(b.x) - 1.,
                    a.y.min(b.y) - 1.,
                    a.x.max(b.x) + 1.,
                    a.y.max(b.y) + 1.,
                ];
                Some(SpritePiece {
                    from,
                    group,
                    item,
                    image: piece_image(&sources, scale, logical, ink)?,
                })
            })
            .collect::<Vec<_>>();
        if pieces.is_empty() {
            return;
        }
        self.content_sprite = Some(ContentSprite {
            document_id,
            page_index,
            pieces,
            dropped: None,
            sent: false,
            replied: false,
        });
    }

    /// Where each piece of the dragged selection is shown after `change`.
    fn sprite_targets(&self, sprite: &ContentSprite, change: ContentChange) -> Vec<[f64; 4]> {
        let Some(selection) = self.content_selection.as_ref() else {
            return Vec::new();
        };
        sprite
            .pieces
            .iter()
            .filter_map(|piece| {
                let group = selection.groups.get(piece.group)?;
                Some(match piece.item {
                    Some(item) => {
                        let item = group.get(item)?;
                        crate::pdf_content_moves::changed_placement(item, change).shown(item.rect)
                    }
                    // Text only moves, with its first line.
                    None => {
                        let first = group.first()?;
                        let to = crate::pdf_content_moves::changed_placement(first, change).offset;
                        let (dx, dy) = (to[0] - first.from.offset[0], to[1] - first.from.offset[1]);
                        let from = piece.from;
                        [from[0] + dx, from[1] + dy, from[2] + dx, from[3] + dy]
                    }
                })
            })
            .collect()
    }

    /// Leaves the dragged picture where `change` puts it, until the page
    /// shows the content there.
    fn drop_content_sprite(
        &mut self,
        document_id: DocumentId,
        change: ContentChange,
        cx: &mut Context<Self>,
    ) {
        let Some(sprite) = self
            .content_sprite
            .take()
            .filter(|sprite| sprite.document_id == document_id)
        else {
            return;
        };
        let targets = self.sprite_targets(&sprite, change);
        self.content_sprite = Some(ContentSprite {
            dropped: Some(targets),
            ..sprite
        });
        cx.notify();
    }

    /// How many pieces the picture of dragged (or just dropped) content has.
    pub fn content_drag_picture_pieces(&self, document_id: DocumentId) -> Option<usize> {
        self.content_sprite
            .as_ref()
            .filter(|sprite| sprite.document_id == document_id)
            .map(|sprite| sprite.pieces.len())
    }

    /// Notes that the edits sent to the worker show a dropped picture's
    /// content.
    pub(super) fn note_content_sprite_sent(&mut self, document_id: DocumentId) {
        if let Some(sprite) = self.content_sprite.as_mut()
            && sprite.document_id == document_id
            && sprite.dropped.is_some()
        {
            sprite.sent = true;
        }
    }

    /// Notes that the worker shows the edits a dropped picture waited for.
    pub(super) fn note_content_sprite_replied(&mut self, document_id: DocumentId) {
        if let Some(sprite) = self.content_sprite.as_mut()
            && sprite.document_id == document_id
            && sprite.sent
        {
            sprite.replied = true;
        }
    }

    /// The dragged picture to paint, if any: a dropped one goes once the
    /// page's tiles show the content where it was dropped.
    pub(super) fn content_sprite_paint(
        &mut self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<SpritePaint> {
        let sprite = self
            .content_sprite
            .as_ref()
            .filter(|sprite| sprite.document_id == document_id)?;
        if sprite.replied {
            let fresh = self.session(document_id, cx).is_none_or(|session| {
                session
                    .read(cx)
                    .viewer
                    .shows_fresh_tiles(sprite.page_index as usize)
            });
            if fresh {
                self.content_sprite = None;
                return None;
            }
        }
        let targets = match &sprite.dropped {
            Some(targets) => targets.clone(),
            None => {
                let drag = self.content_drag.filter(|drag| {
                    drag.document_id == document_id
                        && drag.page_index == sprite.page_index
                        && drag.moved
                })?;
                self.sprite_targets(sprite, drag.change)
            }
        };
        Some(SpritePaint {
            page_index: sprite.page_index,
            pieces: targets
                .into_iter()
                .zip(&sprite.pieces)
                .map(|(to, piece)| (to, piece.image.clone()))
                .collect(),
        })
    }

    /// The selection's resize handle at `point`, and the selection's shown
    /// bounds.
    fn content_handle_at(
        &self,
        interaction: &PageInteraction,
        point: PdfPoint,
    ) -> Option<(RectangleResizeHandle, [f64; 4])> {
        let selection = self.content_selection.as_ref().filter(|selection| {
            selection.document_id == interaction.document_id
                && selection.page_index == interaction.page_index
        })?;
        let bounds = resizable_bounds(selection, ContentChange::Move(0., 0.))?;
        let reach = interaction
            .transform
            .tolerance_points(f64::from(HANDLE_SIZE_PIXELS) / 2. + 2.)
            .ok()?;
        // The handles are drawn on the frame, off the content.
        let padding = interaction
            .transform
            .tolerance_points(f64::from(FRAME_PADDING_PIXELS))
            .ok()?;
        handle_points(outset(bounds, padding))
            .iter()
            .position(|(x, y)| (point.x - x).abs() <= reach && (point.y - y).abs() <= reach)
            .map(|index| (RectangleResizeHandle::ALL[index], bounds))
    }

    /// The pointer at `point` dragging `handle`, snapped as a markup's
    /// handle snaps.
    fn snapped_handle_point(
        &mut self,
        drag: ContentDrag,
        handle: RectangleResizeHandle,
        bounds: [f64; 4],
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> PdfPoint {
        let index = RectangleResizeHandle::ALL
            .iter()
            .position(|candidate| *candidate == handle)
            .unwrap_or(0);
        let (x, y) = handle_points(bounds)[index];
        let start = PdfPoint {
            x: drag.start.0,
            y: drag.start.1,
        };
        let anchor = PdfPoint { x, y };
        let Some(session) = self.session(drag.document_id, cx).cloned() else {
            return point;
        };
        let Ok(rect) = crate::annotation_model::PdfRect::new(
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        ) else {
            return point;
        };
        let markups = self
            .content_selection
            .as_ref()
            .map(|selection| selection.markups.clone())
            .unwrap_or_default();
        let snapped = session.update(cx, |session, cx| {
            let snapped = session.annotations.resolve_content_move(
                drag.document_id.value(),
                drag.page_index,
                start,
                point,
                std::slice::from_ref(&anchor),
                rect,
                &[outset(bounds, 1.)],
                &markups,
                &[],
            );
            cx.notify();
            snapped
        });
        // The handle goes where the snapped pointer takes it.
        PdfPoint {
            x: x + snapped.x - start.x,
            y: y + snapped.y - start.y,
        }
    }

    /// How far the selection moves with the pointer at `point`, snapped as
    /// a markup move snaps: its corners, edge midpoints and centre to the
    /// page's markups, content and grids, with the same guides.
    fn snapped_content_delta(
        &mut self,
        drag: ContentDrag,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<(f64, f64)> {
        let selection = self.content_selection.clone()?;
        // What is dragged snaps by its boxes as framed when the drag began
        // (a paragraph's, not its lines').
        let framed = self
            .content_sprite
            .as_ref()
            .filter(|sprite| sprite.document_id == drag.document_id && sprite.dropped.is_none())
            .map(|sprite| {
                sprite
                    .pieces
                    .iter()
                    .map(|piece| piece.from)
                    .collect::<Vec<_>>()
            });
        let rects = framed.unwrap_or_else(|| {
            selection
                .groups
                .iter()
                .filter_map(|group| {
                    group.iter().map(|item| item.shown()).reduce(|a, b| {
                        [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[2].max(b[2]),
                            a[3].max(b[3]),
                        ]
                    })
                })
                .collect::<Vec<_>>()
        });
        let bounds = rects.iter().copied().reduce(|a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })?;
        let anchors = rects
            .iter()
            .flat_map(|rect| {
                let (cx_, cy) = ((rect[0] + rect[2]) / 2., (rect[1] + rect[3]) / 2.);
                [
                    (rect[0], rect[1]),
                    (rect[2], rect[1]),
                    (rect[2], rect[3]),
                    (rect[0], rect[3]),
                    (cx_, rect[1]),
                    (cx_, rect[3]),
                    (rect[0], cy),
                    (rect[2], cy),
                    (cx_, cy),
                ]
            })
            .take(128)
            .map(|(x, y)| PdfPoint { x, y })
            .collect::<Vec<_>>();
        let excluded = rects
            .iter()
            .map(|rect| outset(*rect, 1.))
            .collect::<Vec<_>>();
        // Other boxes on the page its box lines up with.
        let align_to = self
            .content_snap_frames(drag.document_id, drag.page_index, cx)
            .into_iter()
            .filter_map(|frame| match frame {
                crate::pdf_content_geometry::PdfContentPrimitive::Frame { rect } => {
                    Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
                }
                _ => None,
            })
            .filter(|frame| !excluded.iter().any(|own| contains(*own, *frame)))
            .collect::<Vec<_>>();
        let session = self.session(drag.document_id, cx)?.clone();
        let start = PdfPoint {
            x: drag.start.0,
            y: drag.start.1,
        };
        let snapped = session.update(cx, |session, cx| {
            let snapped = session.annotations.resolve_content_move(
                drag.document_id.value(),
                drag.page_index,
                start,
                point,
                &anchors,
                crate::annotation_model::PdfRect::new(
                    bounds[0],
                    bounds[1],
                    bounds[2] - bounds[0],
                    bounds[3] - bounds[1],
                )
                .ok()?,
                &excluded,
                &selection.markups,
                &align_to,
            );
            cx.notify();
            Some(snapped)
        })?;
        Some((snapped.x - start.x, snapped.y - start.y))
    }

    /// Changes the selected content (moves, resizes or removes it), as
    /// one undoable change; it stays selected unless removed.
    fn change_selected_content(&mut self, change: ContentChange, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.content_selection.clone() else {
            return false;
        };
        let groups = selection.groups.clone();
        let markups = selection.markups.clone();
        let page_index = selection.page_index;
        let result = self.update_annotation_history_with_result(
            selection.document_id,
            cx,
            move |annotations, document_id| {
                annotations.change_document_page_content(
                    document_id,
                    page_index,
                    &groups,
                    &markups,
                    change,
                )
            },
        );
        match result {
            Ok(_) if change == ContentChange::Remove => {
                self.content_selection = None;
                self.content_hover = None;
                self.annotation_statuses.remove(&selection.document_id);
            }
            Ok(_) => {
                // A hover frame drawn before the change is where it was.
                self.content_hover = None;
                if let Some(selection) = self.content_selection.as_mut() {
                    for item in selection.groups.iter_mut().flatten() {
                        item.from = crate::pdf_content_moves::changed_placement(item, change);
                    }
                }
                self.annotation_statuses.remove(&selection.document_id);
            }
            Err(error) => {
                self.annotation_statuses.insert(
                    selection.document_id,
                    format!("The content was not changed: {error}"),
                );
            }
        }
        self.sync_document_edits(selection.document_id, cx);
        self.refresh_content_snapping(selection.document_id, page_index, cx);
        cx.notify();
        true
    }

    /// Reads the page's snapping targets again once content on it moved.
    fn refresh_content_snapping(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, _| {
                session.page_content_geometry_requests.remove(&page_index);
                session
                    .annotations
                    .clear_semantic_snap_page_content_page(document_id.value(), page_index);
            });
        }
        self.ensure_visible_page_content_snap_geometry(document_id, false, cx);
    }

    /// The boxes Edit PDF frames a page's content with, where it is shown:
    /// each paragraph's and each drawing's and image's (not a page's
    /// background, nor a table's line work), snapped to as content is.
    pub(super) fn content_snap_frames(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Vec<crate::pdf_content_geometry::PdfContentPrimitive> {
        let Some((layer, blocks, tables)) = self.loaded_page_text(document_id, page_index) else {
            return Vec::new();
        };
        let mut frames = blocks
            .iter()
            .filter_map(|block| {
                quads_bounds(&self.paragraph_line_quads(document_id, page_index, &layer, block, cx))
            })
            .collect::<Vec<_>>();
        let shown_tables = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown_tables.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        let page_area = layer
            .page_box
            .map(|page| (page[2] - page[0]) * (page[3] - page[1]));
        let in_table = |rect: [f64; 4]| {
            tables.iter().any(|table| {
                let (left, right) = (table.columns[0], table.columns[table.columns.len() - 1]);
                let (bottom, top) = (table.rows[table.rows.len() - 1], table.rows[0]);
                rect[0] >= left - 1.
                    && rect[2] <= right + 1.
                    && rect[1] >= bottom - 1.
                    && rect[3] <= top + 1.
            })
        };
        frames.extend(layer.objects.iter().filter_map(|object| {
            let area = (object.rect[2] - object.rect[0]) * (object.rect[3] - object.rect[1]);
            if page_area
                .is_some_and(|page| area >= crate::pdf_content_moves::MAX_OBJECT_SHARE * page)
                || in_table(object.rect)
            {
                return None;
            }
            let placement =
                crate::pdf_content_moves::placement_of(&refs, &moves, object.kind, object.rect)?;
            Some(placement.shown(object.rect))
        }));
        frames
            .into_iter()
            .filter(|frame| frame.iter().all(|value| value.is_finite()))
            .map(|[left, bottom, right, top]| {
                crate::pdf_content_geometry::PdfContentPrimitive::Frame {
                    rect: crate::pdf_content_geometry::PdfRect {
                        x: left,
                        y: bottom,
                        width: right - left,
                        height: top - bottom,
                    },
                }
            })
            .collect()
    }

    /// Reads a page's content again for snapping once its text is read, so
    /// the boxes round its paragraphs and objects are snapped to. Not while
    /// content is dragged: what moves would become its own target.
    pub(super) fn refresh_content_snapping_for_text(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) {
        let requested = self.session(document_id, cx).is_some_and(|session| {
            session
                .read(cx)
                .page_content_geometry_requests
                .contains_key(&page_index)
        });
        if requested && self.content_drag.is_none() {
            self.refresh_content_snapping(document_id, page_index, cx);
        }
    }

    /// Moves the selected content one step with an arrow key (ten with
    /// Shift). Held keys move it as a drag does, its picture following each
    /// step, and it is one undoable change once they are let go. Returns
    /// whether there was a selection to move.
    pub(super) fn nudge_content_selection(
        &mut self,
        direction: (f64, f64),
        far: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = self.content_selection.as_ref() else {
            return false;
        };
        let (document_id, page_index) = (selection.document_id, selection.page_index);
        if self.active_document_id != Some(document_id) || !self.editing_content(document_id, cx) {
            return false;
        }
        let step = if far { NUDGE_FAR } else { NUDGE };
        let (dx, dy) = (direction.0 * step, direction.1 * step);
        match self.content_drag {
            Some(mut drag) if drag.keyboard => {
                if let ContentChange::Move(x, y) = drag.change {
                    drag.change = ContentChange::Move(x + dx, y + dy);
                }
                self.content_drag = Some(drag);
            }
            Some(_) => return false,
            None => {
                self.content_drag = Some(ContentDrag {
                    document_id,
                    page_index,
                    start: (0., 0.),
                    change: ContentChange::Move(dx, dy),
                    mode: DragMode::Move,
                    moved: true,
                    keyboard: true,
                });
                self.take_content_sprite(document_id, page_index, cx);
            }
        }
        self.content_hover = None;
        self.sync_document_edits(document_id, cx);
        cx.notify();
        self.content_nudges = self.content_nudges.wrapping_add(1);
        let nudges = self.content_nudges;
        cx.spawn(async move |workspace, cx| {
            cx.background_executor().timer(NUDGE_SETTLE).await;
            let _ = workspace.update(cx, |workspace, cx| {
                if workspace.content_nudges == nudges {
                    workspace.settle_content_nudge(cx);
                }
            });
        })
        .detach();
        true
    }

    /// Ends a move by arrow keys, as one undoable change. Returns whether
    /// one was under way.
    pub(super) fn settle_content_nudge(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.content_drag.is_some_and(|drag| drag.keyboard) {
            return false;
        }
        self.finish_content_drag(cx)
    }

    /// Copies the selected content for pasting (Cut also deletes it).
    /// Returns whether there was content selected.
    pub(super) fn copy_content_selection(&mut self, cut: bool, cx: &mut Context<Self>) -> bool {
        self.settle_content_nudge(cx);
        let Some(selection) = self.content_selection.clone() else {
            return false;
        };
        if self.active_document_id != Some(selection.document_id)
            || !self.editing_content(selection.document_id, cx)
            || self.content_drag.is_some()
        {
            return false;
        }
        self.content_clipboard = Some(ContentClipboard {
            document_id: selection.document_id,
            page_index: selection.page_index,
            groups: selection.groups.clone(),
            pastes: 0,
        });
        if cut {
            self.change_selected_content(ContentChange::Remove, cx);
        }
        true
    }

    /// Pastes copied content as new copies on its page, a little away from
    /// the last, and selects them. Returns whether there was content to
    /// paste in Edit PDF mode.
    pub(super) fn paste_content(&mut self, cx: &mut Context<Self>) -> bool {
        self.settle_content_nudge(cx);
        let Some(document_id) = self.active_document_id else {
            return false;
        };
        if !self.editing_content(document_id, cx) {
            return false;
        }
        let Some(clipboard) = self
            .content_clipboard
            .as_mut()
            .filter(|clipboard| clipboard.document_id == document_id)
        else {
            return false;
        };
        let page_index = clipboard.page_index;
        let current_page = self
            .session(document_id, cx)
            .map(|session| session.read(cx).current_page());
        if current_page.is_some_and(|current| current != page_index) {
            self.annotation_statuses.insert(
                document_id,
                format!(
                    "Page content pastes on the page it was copied from (page {}).",
                    page_index + 1
                ),
            );
            cx.notify();
            return true;
        }
        let Some(clipboard) = self.content_clipboard.as_mut() else {
            return false;
        };
        clipboard.pastes += 1;
        let step = PASTE_STEP * f64::from(clipboard.pastes);
        let groups = clipboard.groups.clone();
        let result = self.update_annotation_history_with_result(
            document_id,
            cx,
            move |annotations, document_id| {
                annotations.paste_document_page_content(
                    document_id,
                    page_index,
                    &groups,
                    (step, -step),
                )
            },
        );
        match result {
            Ok(pasted) => {
                self.clear_markup_selection(document_id, cx);
                self.content_selection = Some(ContentSelection {
                    document_id,
                    page_index,
                    groups: pasted,
                    markups: Vec::new(),
                });
                self.annotation_statuses.remove(&document_id);
            }
            Err(error) => {
                self.annotation_statuses
                    .insert(document_id, format!("The content was not pasted: {error}"));
            }
        }
        self.sync_document_edits(document_id, cx);
        self.refresh_content_snapping(document_id, page_index, cx);
        cx.notify();
        true
    }

    /// Deletes the selected content, and the markups selected with it.
    /// Returns whether there was content selected.
    pub(super) fn delete_content_selection(&mut self, cx: &mut Context<Self>) -> bool {
        self.settle_content_nudge(cx);
        let Some(selection) = self.content_selection.as_ref() else {
            return false;
        };
        if self.active_document_id != Some(selection.document_id)
            || !self.editing_content(selection.document_id, cx)
            || self.content_drag.is_some()
        {
            return false;
        }
        self.change_selected_content(ContentChange::Remove, cx)
    }

    /// Starts editing the selected paragraph, with the caret at its end
    /// (Enter). Returns whether one was selected.
    pub(super) fn edit_selected_paragraph(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.settle_content_nudge(cx);
        let Some(selection) = self.content_selection.clone() else {
            return false;
        };
        if self.active_document_id != Some(selection.document_id)
            || !self.editing_content(selection.document_id, cx)
        {
            return false;
        }
        let [group] = selection.groups.as_slice() else {
            return false;
        };
        let Some(last) = group.iter().rfind(|item| item.kind == ContentKind::Text) else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(selection.document_id, selection.page_index))
            .copied()
        else {
            return false;
        };
        let shown = last.shown();
        let point = PdfPoint {
            x: shown[2],
            y: (shown[1] + shown[3]) / 2.,
        };
        self.clear_content_selection(cx);
        self.begin_page_text_press(interaction, point, 1, Modifiers::default(), window, cx)
    }

    /// Hover in Edit PDF mode: false over a markup or form field (the
    /// Select tool's hover applies), else the content under the pointer.
    pub(super) fn update_content_hover(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.content_handle_at(&interaction, point).is_none()
            && (self.markup_under_point(&interaction, point, cx)
                || self.form_widget_hit(&interaction, point, cx).is_some())
        {
            let changed = self.page_text_hover.take().is_some()
                | self.content_hover.take().is_some()
                | self.content_handle_hover.take().is_some();
            if changed {
                cx.notify();
            }
            return false;
        }
        // The Select tool's own hover ends; the content's is set afresh.
        let previous = (self.page_text_hover.clone(), self.content_hover.clone());
        self.clear_hover_candidate(cx);
        (self.page_text_hover, self.content_hover) = previous;
        let handle = self
            .content_handle_at(&interaction, point)
            .map(|(handle, _)| (interaction.document_id, handle));
        if self.content_handle_hover != handle {
            self.content_handle_hover = handle;
            cx.notify();
        }
        if handle.is_some() {
            self.page_text_hover = None;
            self.content_hover = None;
            return true;
        }
        let edge = self
            .table_column_edge_at(interaction, point, cx)
            .map(|_| interaction.document_id);
        if self.column_edge_hover != edge {
            self.column_edge_hover = edge;
            cx.notify();
        }
        if edge.is_some() {
            self.page_text_hover = None;
            self.content_hover = None;
            return true;
        }
        // Copies, and the band of a paragraph a table covers, come before
        // the text under them, as a press takes them.
        let on_top = self
            .content_copy_at(interaction, point, cx)
            .or_else(|| self.covered_paragraph_band(interaction, point, cx));
        if on_top.is_some() {
            self.page_text_hover = None;
        } else {
            self.update_page_text_hover(interaction, point, cx);
        }
        let object = if on_top.is_some() {
            on_top
        } else if self.page_text_hover.is_some() {
            None
        } else {
            self.content_object_at(interaction, point, cx)
                .or_else(|| self.paragraph_band_at(interaction, point, cx))
        };
        if self.content_hover != object {
            self.content_hover = object;
            cx.notify();
        }
        true
    }

    /// The cursor while selected content is dragged.
    pub(super) fn content_drag_cursor(&self, document_id: DocumentId) -> Option<CursorStyle> {
        if self.column_drag_active(document_id) {
            return Some(CursorStyle::ResizeColumn);
        }
        self.content_drag
            .filter(|drag| drag.document_id == document_id && drag.moved && !drag.keyboard)
            .map(|drag| match drag.mode {
                DragMode::Move => crate::canvas_cursors::move_cursor(true),
                DragMode::Resize { handle, .. } => super::rectangle_resize_cursor_style(handle, 0.),
            })
            .or_else(|| {
                self.page_text_editing
                    .filter(|editing| editing.document_id == document_id && editing.dragging)
                    .map(|_| CursorStyle::IBeam)
            })
    }

    /// The cursor over page content: a text cursor over text a press
    /// edits, a move cursor where a press selects content to drag, and
    /// not-allowed over text that cannot be edited.
    pub(super) fn content_cursor(&self, document_id: DocumentId) -> Option<CursorStyle> {
        if self.column_edge_hover == Some(document_id) {
            return Some(CursorStyle::ResizeColumn);
        }
        if let Some((hover_document, handle)) = self.content_handle_hover
            && hover_document == document_id
        {
            return Some(super::rectangle_resize_cursor_style(handle, 0.));
        }
        if self
            .content_hover
            .as_ref()
            .is_some_and(|hover| hover.document_id == document_id)
        {
            return Some(crate::canvas_cursors::move_cursor(false));
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

    /// Where `group` is framed once `change` changes it: a paragraph by
    /// its lines' full boxes, as the text editor frames it, and other
    /// content by its bounds.
    fn group_frame(
        &self,
        document_id: DocumentId,
        page_index: u32,
        group: &[MovedContent],
        change: ContentChange,
        cx: &App,
    ) -> Option<[f64; 4]> {
        let first = group.first()?;
        if group.iter().all(|item| item.kind == ContentKind::Text)
            && let Some(frame) = self.paragraph_frame(document_id, page_index, first.rect, cx)
        {
            // Text is only ever moved.
            let [dx, dy] = crate::pdf_content_moves::changed_placement(first, change).offset;
            return Some([frame[0] + dx, frame[1] + dy, frame[2] + dx, frame[3] + dy]);
        }
        group
            .iter()
            .map(|item| crate::pdf_content_moves::changed_placement(item, change).shown(item.rect))
            .reduce(union)
    }

    /// The frame of the paragraph holding the line whose core box is
    /// `line_rect`, where the page draws it.
    fn paragraph_frame(
        &self,
        document_id: DocumentId,
        page_index: u32,
        line_rect: [f64; 4],
        cx: &App,
    ) -> Option<[f64; 4]> {
        let (layer, blocks, _) = self.loaded_page_text(document_id, page_index)?;
        let line = layer.lines.iter().find(|line| {
            crate::pdf_tables::line_core_box(line).is_some_and(|core| {
                core.iter()
                    .zip(line_rect)
                    .all(|(a, b)| (a - b).abs() < 0.01)
            })
        })?;
        let block = blocks
            .iter()
            .find(|block| block.lines.contains(&line.anchor))?;
        quads_bounds(&self.paragraph_line_quads(document_id, page_index, &layer, block, cx))
    }

    /// Each page's selected content outlines, and the outline of the
    /// drawing or image under the pointer, as shown.
    pub(super) fn content_overlays(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<(u32, Vec<Quad>, Vec<Quad>, Vec<(f64, f64)>)> {
        if !self.editing_content(document_id, cx) {
            return Vec::new();
        }
        let mut overlays = Vec::new();
        if let Some(selection) = self
            .content_selection
            .as_ref()
            .filter(|selection| selection.document_id == document_id)
        {
            let change = self
                .content_drag
                .filter(|drag| drag.moved)
                .map_or(ContentChange::Move(0., 0.), |drag| drag.change);
            let rects = selection
                .groups
                .iter()
                .filter_map(|group| {
                    self.group_frame(document_id, selection.page_index, group, change, cx)
                })
                .collect::<Vec<_>>();
            let handles = resizable_bounds(selection, change)
                .map(|bounds| handle_points(bounds).to_vec())
                .unwrap_or_default();
            let quads = rects.into_iter().map(rect_quad).collect::<Vec<_>>();
            overlays.push((selection.page_index, quads, Vec::new(), handles));
        }
        if let Some(hover) = self
            .content_hover
            .as_ref()
            .filter(|hover| hover.document_id == document_id)
        {
            let quad = rect_quad(hover.bounds);
            match overlays
                .iter_mut()
                .find(|(page, _, _, _)| *page == hover.page_index)
            {
                Some((_, _, hovered, _)) => hovered.push(quad),
                None => overlays.push((hover.page_index, Vec::new(), vec![quad], Vec::new())),
            }
        }
        overlays
    }
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The shown bounds of a selection that resizes from handles: one drawing
/// or image (with any text its clip holds), changed by `change`.
fn resizable_bounds(selection: &ContentSelection, change: ContentChange) -> Option<[f64; 4]> {
    let [group] = selection.groups.as_slice() else {
        return None;
    };
    let first = group.first()?;
    if first.kind == ContentKind::Text {
        return None;
    }
    Some(crate::pdf_content_moves::changed_placement(first, change).shown(first.rect))
}

/// The resize handles of `bounds`, in [`RectangleResizeHandle::ALL`]'s
/// order.
fn handle_points(bounds: [f64; 4]) -> [(f64, f64); 8] {
    let [left, bottom, right, top] = bounds;
    let (mid_x, mid_y) = ((left + right) / 2., (bottom + top) / 2.);
    [
        (left, top),
        (mid_x, top),
        (right, top),
        (right, mid_y),
        (right, bottom),
        (mid_x, bottom),
        (left, bottom),
        (left, mid_y),
    ]
}

/// The resize that drags `handle` of `bounds` to `point`, keeping the
/// opposite handle still; Shift (or a corner) keeps the proportions when
/// `uniform`. Content keeps at least 2 points' size and never flips.
fn resize_change(
    handle: RectangleResizeHandle,
    bounds: [f64; 4],
    point: (f64, f64),
    uniform: bool,
) -> ContentChange {
    let index = RectangleResizeHandle::ALL
        .iter()
        .position(|candidate| *candidate == handle)
        .unwrap_or(0);
    let handles = handle_points(bounds);
    let (from, origin) = (handles[index], handles[(index + 4) % 8]);
    let along = |to: f64, from: f64, origin: f64, size: f64| {
        let span = from - origin;
        if span.abs() < 1e-9 {
            return 1.;
        }
        ((to - origin) / span).max(MIN_RESIZED / size.max(1e-9))
    };
    let width = bounds[2] - bounds[0];
    let height = bounds[3] - bounds[1];
    let mut factor = (
        along(point.0, from.0, origin.0, width),
        along(point.1, from.1, origin.1, height),
    );
    let corner = index % 2 == 0;
    if uniform && corner {
        let uniform = factor.0.max(factor.1);
        factor = (uniform, uniform);
    }
    ContentChange::Resize { origin, factor }
}

/// The least size, in points, a resize leaves content.
const MIN_RESIZED: f64 = 2.;

/// The upright bounds of `quads`.
pub(super) fn quads_bounds(quads: &[Quad]) -> Option<[f64; 4]> {
    quads
        .iter()
        .flatten()
        .map(|&(x, y)| [x, y, x, y])
        .reduce(union)
}

fn outset(rect: [f64; 4], by: f64) -> [f64; 4] {
    [rect[0] - by, rect[1] - by, rect[2] + by, rect[3] + by]
}

fn rect_quad(rect: [f64; 4]) -> Quad {
    [
        (rect[0], rect[1]),
        (rect[2], rect[1]),
        (rect[2], rect[3]),
        (rect[0], rect[3]),
    ]
}

/// Paints a dragged picture on its page: each piece's picture where the
/// piece is now.
pub(super) fn content_sprite_element(
    sprite: SpritePaint,
    page_size: (f32, f32),
    coordinate_space: crate::page_geometry::PageCoordinateSpace,
) -> gpui::AnyElement {
    use gpui::{Bounds, IntoElement as _, ParentElement as _, Styled as _, point, px};
    gpui::div()
        .absolute()
        .inset_0()
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let Some((page_bounds, transform)) =
                        super::contained_page_bounds_for_space(bounds, page_size, coordinate_space)
                    else {
                        return;
                    };
                    for ([left, bottom, right, top], image) in &sprite.pieces {
                        let a = transform.point_to_local_pixels(PdfPoint {
                            x: *left,
                            y: *bottom,
                        });
                        let b = transform.point_to_local_pixels(PdfPoint { x: *right, y: *top });
                        // The picture is a pixel wider each side.
                        let to = Bounds::from_corners(
                            point(
                                page_bounds.origin.x + px(a.x.min(b.x) as f32 - 1.),
                                page_bounds.origin.y + px(a.y.min(b.y) as f32 - 1.),
                            ),
                            point(
                                page_bounds.origin.x + px(a.x.max(b.x) as f32 + 1.),
                                page_bounds.origin.y + px(a.y.max(b.y) as f32 + 1.),
                            ),
                        );
                        let _ =
                            window.paint_image(to, to, Default::default(), image.clone(), 0, false);
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}

/// The picture of the page within `logical` (left, top, right, bottom, in
/// logical pixels from the page's top left) from its tiles. Ink on paper
/// (`ink`) takes the paper away: white turns clear and lighter colours
/// show through, so the picture shows over what it passes.
fn piece_image(
    sources: &[(
        crate::viewer::PixelRect,
        std::sync::Arc<gpui::RenderImage>,
        i64,
    )],
    scale: f64,
    logical: [f64; 4],
    ink: bool,
) -> Option<std::sync::Arc<gpui::RenderImage>> {
    // Image pixels per source pixel: a tile's image may be finer or coarser
    // than the tile (detail and preview renders).
    let fineness = |(crop, image, pad): &(
        crate::viewer::PixelRect,
        std::sync::Arc<gpui::RenderImage>,
        i64,
    )| {
        let width = i64::from(image.size(0).width.0) - 2 * pad;
        width as f64 / crop.width.max(1) as f64
    };
    // The picture is as fine as the finest source.
    let fine = sources.iter().map(fineness).fold(1., f64::max).min(4.);
    let to_pixels = scale * fine;
    let x0 = (logical[0] * to_pixels).floor().max(0.) as i64;
    let y0 = (logical[1] * to_pixels).floor().max(0.) as i64;
    let x1 = (logical[2] * to_pixels).ceil() as i64;
    let y1 = (logical[3] * to_pixels).ceil() as i64;
    let (width, height) = (x1 - x0, y1 - y0);
    // A selection larger than a screenful is left to the worker.
    if width <= 0 || height <= 0 || width * height > 16_000_000 {
        return None;
    }
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    for source in sources {
        let (crop, image, pad) = source;
        let (crop, pad) = (*crop, *pad);
        // The tile in picture pixels.
        let (cx0, cy0) = (crop.x as f64 * fine, crop.y as f64 * fine);
        let (cx1, cy1) = (
            cx0 + crop.width as f64 * fine,
            cy0 + crop.height as f64 * fine,
        );
        let ix0 = x0.max(cx0.ceil() as i64);
        let iy0 = y0.max(cy0.ceil() as i64);
        let ix1 = x1.min(cx1.ceil() as i64);
        let iy1 = y1.min(cy1.ceil() as i64);
        if ix0 >= ix1 || iy0 >= iy1 {
            continue;
        }
        let Some(bytes) = image.as_bytes(0) else {
            continue;
        };
        let size = image.size(0);
        let (image_width, image_height) = (i64::from(size.width.0), i64::from(size.height.0));
        // Image pixels per picture pixel.
        let step = fineness(source) / fine;
        for y in iy0..iy1 {
            let sy = (pad + ((y as f64 - cy0) * step) as i64).clamp(0, image_height - 1);
            for x in ix0..ix1 {
                let sx = (pad + ((x as f64 - cx0) * step) as i64).clamp(0, image_width - 1);
                let source = ((sy * image_width + sx) * 4) as usize;
                let target = (((y - y0) * width + (x - x0)) * 4) as usize;
                if let Some(pixel) = bytes.get(source..source + 4) {
                    pixels[target..target + 4].copy_from_slice(pixel);
                }
            }
        }
    }
    if ink {
        for pixel in pixels.chunks_exact_mut(4) {
            // BGRA: the least of the colour channels says how much paper
            // shows; what is left of each channel is the ink.
            let alpha = 255 - pixel[0].min(pixel[1]).min(pixel[2]);
            if alpha == 0 {
                pixel.copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for channel in &mut pixel[..3] {
                let ink = 255 - u32::from(*channel);
                *channel = (255 - (ink * 255 / u32::from(alpha)).min(255)) as u8;
            }
            pixel[3] = alpha;
        }
    }
    let buffer = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
        width as u32,
        height as u32,
        pixels,
    )?;
    Some(std::sync::Arc::new(gpui::RenderImage::new(
        smallvec::smallvec![image::Frame::new(buffer)],
    )))
}

/// Whether `outer` holds all of `inner` (left, bottom, right, top).
fn contains(outer: [f64; 4], inner: [f64; 4]) -> bool {
    inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_resizes_from_the_opposite_handle() {
        let bounds = [100., 100., 200., 150.];
        // The north-east corner dragged out: twice as wide, as tall again.
        let change = resize_change(
            RectangleResizeHandle::NorthEast,
            bounds,
            (300., 200.),
            false,
        );
        assert_eq!(
            change,
            ContentChange::Resize {
                origin: (100., 100.),
                factor: (2., 2.)
            }
        );
        // The west edge only widens; it never flips past the east edge.
        let change = resize_change(RectangleResizeHandle::West, bounds, (50., 0.), false);
        assert_eq!(
            change,
            ContentChange::Resize {
                origin: (200., 125.),
                factor: (1.5, 1.)
            }
        );
        let ContentChange::Resize { factor, .. } =
            resize_change(RectangleResizeHandle::West, bounds, (400., 0.), false)
        else {
            unreachable!()
        };
        assert!((factor.0 * 100. - MIN_RESIZED).abs() < 1e-9);
        // Shift keeps a corner's proportions.
        let change = resize_change(RectangleResizeHandle::SouthEast, bounds, (300., 90.), true);
        assert_eq!(
            change,
            ContentChange::Resize {
                origin: (100., 150.),
                factor: (2., 2.)
            }
        );
    }

    /// A tile at `fine` image pixels per tile pixel, 4 x 2 tile pixels,
    /// whose pixel (x, y) is grey `values[y][x]`, with its border.
    fn tile(
        values: [[u8; 4]; 2],
        fine: usize,
    ) -> (
        crate::viewer::PixelRect,
        std::sync::Arc<gpui::RenderImage>,
        i64,
    ) {
        let pad = crate::document_resource::TILE_IMAGE_PADDING as usize;
        let (width, height) = (4 * fine + 2 * pad, 2 * fine + 2 * pad);
        let mut pixels = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let tx = (x.saturating_sub(pad) / fine).min(3);
                let ty = (y.saturating_sub(pad) / fine).min(1);
                let value = values[ty][tx];
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let buffer = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
            width as u32,
            height as u32,
            pixels,
        )
        .unwrap();
        let crop = crate::viewer::PixelRect {
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        (
            crop,
            std::sync::Arc::new(gpui::RenderImage::new(smallvec::smallvec![
                image::Frame::new(buffer)
            ])),
            pad as i64,
        )
    }

    fn picture(image: &gpui::RenderImage) -> (i32, Vec<[u8; 4]>) {
        let bytes = image.as_bytes(0).unwrap();
        (
            image.size(0).width.0,
            bytes
                .chunks_exact(4)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                .collect(),
        )
    }

    #[test]
    fn a_dragged_picture_is_cut_from_the_tiles_at_their_resolution() {
        let values = [[10, 20, 30, 40], [50, 60, 70, 80]];
        // Tile pixels 1 to 3 across, both rows; a tile drawn twice as fine
        // gives a picture twice as fine.
        for fine in [1, 2] {
            let image = piece_image(&[tile(values, fine)], 1., [1., 0., 3., 2.], false).unwrap();
            let (width, pixels) = picture(&image);
            assert_eq!(width, 2 * fine as i32);
            let at = |x: usize, y: usize| pixels[y * 2 * fine + x][0];
            assert_eq!(
                [at(0, 0), at(2 * fine - 1, 0), at(0, 2 * fine - 1)],
                [20, 30, 60]
            );
        }
    }

    #[test]
    fn dragged_ink_leaves_the_paper_behind() {
        let image = piece_image(
            &[tile([[255, 0, 128, 255]; 2], 1)],
            1.,
            [0., 0., 4., 1.],
            true,
        )
        .unwrap();
        let (_, pixels) = picture(&image);
        // White is clear; black stays black; grey is black, half clear.
        assert_eq!(pixels[0][3], 0);
        assert_eq!(pixels[1], [0, 0, 0, 255]);
        assert_eq!(pixels[2], [0, 0, 0, 127]);
    }
}
