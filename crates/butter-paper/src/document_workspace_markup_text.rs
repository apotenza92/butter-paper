//! Editing a selected markup's text where it sits. The I-beam the pointer
//! shows over that text and the press that puts the caret there come from
//! one hit test, so what the cursor offers is what the press does.
use super::*;

/// How far outside a selected Dimension's caption a press still moves the
/// caption: its text edits, its edge moves it.
const CAPTION_MOVE_MARGIN_PX: f64 = 6.;

/// The text of the one selected markup under the pointer.
#[derive(Clone)]
pub(super) struct MarkupTextHit {
    pub(super) id: MarkupId,
    target: PendingTextEditorTarget,
    /// The text's box on the page as drawn: upper left, upper right, lower
    /// right and lower left of its first line's reading direction.
    corners: [PdfPoint; 4],
    /// Where the letters are, the I-beam's reach: the rest of the box (its
    /// padding, the room a large Text Box leaves) still moves the markup.
    region: [PdfPoint; 4],
    content: String,
    style: TextBoxStyle,
}

/// A press in a markup's text, held: dragging selects from where it began,
/// mapped onto the text as the press found it.
#[derive(Clone)]
pub(super) struct MarkupTextDrag {
    document_id: DocumentId,
    page_index: u32,
    anchor: usize,
    layout: MarkupTextHit,
}

impl DocumentWorkspace {
    /// The selected markup's text under `point`, when a press there edits
    /// it: the Select tool, one unlocked markup with text selected (a Text
    /// Box, Callout, Cloud+ or Dimension), and no resize, rotate or end
    /// handle under the pointer, which keep their own drags.
    pub(super) fn markup_text_hit(
        &self,
        document_id: DocumentId,
        page_index: u32,
        point: PdfPoint,
        transform: PageTransform,
        cx: &App,
    ) -> Option<MarkupTextHit> {
        if self.pending_text_box_editor.is_some() || self.editing_content(document_id, cx) {
            return None;
        }
        let session = self.session(document_id, cx)?.read(cx);
        let annotations = &session.annotations;
        if annotations.tool() != AnnotationTool::Select {
            return None;
        }
        let document = document_id.value();
        let tolerance = transform.tolerance_points(4.).ok()?;
        if matches!(
            annotations
                .select_hover_hit(document, page_index, point, tolerance)
                .ok()
                .flatten(),
            Some(
                HitTarget::ResizeHandle { .. }
                    | HitTarget::RotationHandle(_)
                    | HitTarget::LineEndpoint { .. }
            )
        ) {
            return None;
        }
        // Every handle keeps its own drag. A Dimension's offset handle sits
        // past the end of its caption, so the caption itself is all text.
        if annotations
            .hover_transform_handle(document, page_index, point, tolerance)
            .ok()
            .flatten()
            .is_some()
        {
            return None;
        }
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let boxed = |id: &MarkupId,
                     target: PendingTextEditorTarget,
                     rect: PdfRect,
                     rotation: f64,
                     content: &str,
                     style: &TextBoxStyle,
                     fitted: bool| MarkupTextHit {
            id: id.clone(),
            target,
            corners: rotated_rect_corners(rect, rotation),
            region: glyph_region(rect, rotation, style, content, fitted, &text_system),
            content: content.to_owned(),
            style: style.clone(),
        };
        let hit = if let Some(text_box) = annotations.exact_selected_text_box(document) {
            if text_box.page_index != page_index || text_box.locked {
                return None;
            }
            boxed(
                &text_box.id,
                PendingTextEditorTarget::ExistingTextBox {
                    id: text_box.id.clone(),
                },
                text_box.layout_rect,
                text_box.rotation_degrees(),
                text_box.content(),
                text_box.style(),
                false,
            )
        } else if let Some(callout) = annotations.exact_selected_callout(document) {
            if callout.page_index != page_index || callout.locked {
                return None;
            }
            boxed(
                &callout.id,
                PendingTextEditorTarget::ExistingCallout {
                    id: callout.id.clone(),
                },
                callout.text_box,
                0.,
                callout.content(),
                callout.appearance.text(),
                true,
            )
        } else if let Some(cloud_plus) = annotations.exact_selected_cloud_plus(document) {
            if cloud_plus.page_index != page_index || cloud_plus.locked {
                return None;
            }
            boxed(
                &cloud_plus.id,
                PendingTextEditorTarget::ExistingCloudPlus {
                    id: cloud_plus.id.clone(),
                },
                cloud_plus.text_box,
                0.,
                cloud_plus.content(),
                cloud_plus.appearance.text(),
                true,
            )
        } else {
            let dimension = annotations.exact_selected_dimension(document)?;
            if dimension.page_index != page_index || dimension.locked {
                return None;
            }
            let caption = self
                .annotation_caption_selection_paths_for_session(
                    document_id,
                    page_index,
                    session,
                    cx,
                )
                .remove(&dimension.id)?;
            // A Dimension's caption box already hugs its text.
            let corners: [PdfPoint; 4] = caption.try_into().ok()?;
            MarkupTextHit {
                id: dimension.id.clone(),
                target: PendingTextEditorTarget::ExistingDimension {
                    id: dimension.id.clone(),
                },
                corners,
                region: corners,
                content: dimension.content().to_owned(),
                style: dimension.appearance.text().clone(),
            }
        };
        point_in_quad(point, &hit.region).then_some(hit)
    }

    /// The caption paths a press and hover test against: a selected
    /// Dimension's caption reaches a little past its text, so its edge
    /// still moves the caption while a press in the text edits it.
    pub(super) fn caption_press_paths(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> crate::annotation_model::AnnotationSelectionSupplement {
        let mut paths = self.annotation_caption_selection_paths(document_id, page_index, cx);
        let Some(margin) = self
            .page_interactions
            .get(&(document_id, page_index))
            .and_then(|interaction| {
                interaction
                    .transform
                    .tolerance_points(CAPTION_MOVE_MARGIN_PX)
                    .ok()
            })
        else {
            return paths;
        };
        let selected = self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .exact_selected_dimension(document_id.value())
                .map(|dimension| dimension.id.clone())
        });
        if let Some(id) = selected
            && let Some(corners) = paths.get_mut(&id)
            && let Ok(quad) = <[PdfPoint; 4]>::try_from(corners.as_slice())
        {
            *corners = expanded_quad(quad, margin).to_vec();
        }
        paths
    }

    /// Recomputes which selected markup's text the pointer is over.
    pub(super) fn update_markup_text_hover(
        &mut self,
        interaction: &PageInteraction,
        point: Option<PdfPoint>,
        cx: &App,
    ) -> bool {
        let next = point.and_then(|point| {
            self.markup_text_hit(
                interaction.document_id,
                interaction.page_index,
                point,
                interaction.transform,
                cx,
            )
            .map(|hit| (interaction.document_id, interaction.page_index, hit.id))
        });
        if self.markup_text_hover == next {
            return false;
        }
        self.markup_text_hover = next;
        true
    }

    /// A press in the selected markup's text opens its editor on the page
    /// with the caret where it landed.
    pub(super) fn begin_markup_text_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(hit) = self.markup_text_hit(
            interaction.document_id,
            interaction.page_index,
            point,
            interaction.transform,
            cx,
        ) else {
            return false;
        };
        let offset = text_offset_at(&hit, point, window.text_system());
        if !self.begin_pending_composite_text_editor(
            interaction.document_id,
            interaction.page_index,
            hit.target.clone(),
            window,
            cx,
        ) {
            return false;
        }
        if let Some(editor) = self.pending_text_box_editor.as_ref() {
            editor
                .input
                .update(cx, |input, cx| input.set_selected_range(offset..offset, cx));
        }
        self.markup_text_hover = None;
        self.markup_text_drag = Some(MarkupTextDrag {
            document_id: interaction.document_id,
            page_index: interaction.page_index,
            anchor: offset,
            layout: hit,
        });
        cx.notify();
        true
    }

    /// A double-click in a markup's text opens its editor on the page with
    /// the word under the pointer selected. The first click selected the
    /// markup.
    pub(super) fn begin_markup_text_word_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(hit) = self.markup_text_hit(
            interaction.document_id,
            interaction.page_index,
            point,
            interaction.transform,
            cx,
        ) else {
            return false;
        };
        let offset = text_offset_at(&hit, point, window.text_system());
        let word = word_range(&hit.content, offset);
        if !self.begin_pending_composite_text_editor(
            interaction.document_id,
            interaction.page_index,
            hit.target.clone(),
            window,
            cx,
        ) {
            return false;
        }
        if let Some(editor) = self.pending_text_box_editor.as_ref() {
            editor
                .input
                .update(cx, |input, cx| input.set_selected_range(word, cx));
        }
        self.markup_text_hover = None;
        cx.notify();
        true
    }

    /// Extends the text selection while the press that placed the caret is
    /// held.
    pub(super) fn update_markup_text_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.markup_text_drag.as_ref() else {
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
        let Some(editor) = self.pending_text_box_editor.as_ref() else {
            self.markup_text_drag = None;
            return false;
        };
        let layout = MarkupTextHit {
            content: editor.input.read(cx).value().to_string(),
            ..drag.layout.clone()
        };
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let focus = text_offset_at(&layout, point, &text_system);
        let range = drag.anchor.min(focus)..drag.anchor.max(focus);
        editor
            .input
            .update(cx, |input, cx| input.set_selected_range(range, cx));
        cx.notify();
        true
    }

    pub(super) fn finish_markup_text_drag(&mut self) -> bool {
        self.markup_text_drag.take().is_some()
    }
}

impl DocumentWorkspace {
    /// Where the open text editor sits on its page, upright, and the text
    /// style it types in: over a Text Box's box, a Callout's or Cloud+'s
    /// caption box, or centred on a Dimension's caption.
    pub(super) fn pending_text_editor_layout(
        &self,
        text_system: &gpui::WindowTextSystem,
        cx: &App,
    ) -> Option<(Option<MarkupId>, PdfRect, TextBoxStyle)> {
        let editor = self.pending_text_box_editor.as_ref()?;
        let content = editor.input.read(cx).value();
        let content = content.as_ref();
        let annotations = &self.session(editor.document_id, cx)?.read(cx).annotations;
        let document = editor.document_id.value();
        Some(match &editor.target {
            PendingTextEditorTarget::NewTextBox { rect, style, .. } => (None, *rect, style.clone()),
            PendingTextEditorTarget::ExistingTextBox { id } => {
                let text_box = self.inline_existing_text_box(editor.document_id, id, cx)?;
                (
                    Some(id.clone()),
                    text_box.layout_rect,
                    text_box.style().clone(),
                )
            }
            PendingTextEditorTarget::Callout { id }
            | PendingTextEditorTarget::ExistingCallout { id } => {
                let callout = annotations.callout(document, id)?;
                let style = callout.appearance.text();
                let rect = caption_editor_rect(
                    crate::annotation_adapter::callout_fitted_text_box(callout, content),
                    style,
                    content,
                );
                (Some(id.clone()), rect, style.clone())
            }
            PendingTextEditorTarget::NewCloudPlus { id }
            | PendingTextEditorTarget::ExistingCloudPlus { id } => {
                let cloud_plus = annotations.cloud_plus(document, id)?;
                let style = cloud_plus.appearance.text();
                let rect = caption_editor_rect(
                    crate::annotation_adapter::cloud_plus_fitted_text_box(cloud_plus, content),
                    style,
                    content,
                );
                (Some(id.clone()), rect, style.clone())
            }
            PendingTextEditorTarget::NewDimension { id }
            | PendingTextEditorTarget::ExistingDimension { id } => {
                let dimension = annotations
                    .snapshot(document)?
                    .dimensions
                    .into_iter()
                    .find(|dimension| &dimension.id == id)?;
                let style = dimension.appearance.text().clone();
                let rect = dimension_caption_editor_rect(&dimension, &style, content, text_system);
                (Some(id.clone()), rect, style)
            }
        })
    }

    /// The open editor's box in window coordinates: a press inside it is
    /// the editor's own, to move the caret, not a click away.
    pub(super) fn pending_text_editor_window_bounds(&self, cx: &App) -> Option<Bounds<Pixels>> {
        let editor = self.pending_text_box_editor.as_ref()?;
        let interaction = match &editor.target {
            PendingTextEditorTarget::NewTextBox { interaction, .. } => *interaction,
            _ => *self
                .page_interactions
                .get(&(editor.document_id, editor.page_index))?,
        };
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let (_, rect, _) = self.pending_text_editor_layout(&text_system, cx)?;
        let local = interaction.transform.rect_to_local_pixels(rect);
        Some(Bounds::new(
            point(
                interaction.bounds.origin.x + px(local.x as f32),
                interaction.bounds.origin.y + px(local.y as f32),
            ),
            size(
                px((local.width as f32).max(24.)),
                px((local.height as f32).max(18.)),
            ),
        ))
    }
}

impl DocumentWorkspace {
    /// Applies a properties Text field's edit to the markup it was shown
    /// for, if the document has not changed since; otherwise the status bar
    /// says why and the field shows the markup's text again.
    pub(super) fn apply_property_text_edit(
        &mut self,
        event: &crate::property_controls::PropertyTextEditEvent,
        cx: &mut Context<Self>,
    ) {
        let current = self.session(event.document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .snapshot(event.document_id.value())
                .map(|snapshot| snapshot.revision)
        });
        let result = if current != Some(event.expected_revision) {
            Err("The markup changed while its text was being edited".to_owned())
        } else {
            self.set_markup_content(
                event.document_id,
                &event.annotation_id,
                event.content.clone(),
                cx,
            )
        };
        if let Err(error) = result {
            self.annotation_statuses.insert(event.document_id, error);
            cx.notify();
        }
    }

    /// Sets a markup's text from its properties' Text field, as one undoable
    /// change: a Text Box's, Callout's, Cloud+'s or Dimension's. Locked
    /// markups and unchanged text are left alone; while its text is being
    /// typed on the page the page's editor owns it.
    pub fn set_markup_content(
        &mut self,
        document_id: DocumentId,
        id: &MarkupId,
        content: String,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self
            .pending_text_box_editor
            .as_ref()
            .is_some_and(|editor| editor.document_id == document_id)
        {
            return Ok(false);
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Ok(false);
        };
        let (kind, page_index) = {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || session.save_status == NativeDocumentSaveStatus::Saving
                || session.pending_rotation_generation.is_some()
            {
                return Ok(false);
            }
            let Some(snapshot) = session.annotations.snapshot(document_id.value()) else {
                return Ok(false);
            };
            let found = snapshot
                .text_boxes
                .iter()
                .find(|markup| &markup.id == id)
                .map(|markup| {
                    (
                        AnnotationKind::TextBox,
                        markup.page_index,
                        markup.locked,
                        markup.content() == content,
                    )
                })
                .or_else(|| {
                    snapshot
                        .dimensions
                        .iter()
                        .find(|markup| &markup.id == id)
                        .map(|markup| {
                            (
                                AnnotationKind::Dimension,
                                markup.page_index,
                                markup.locked,
                                markup.content() == content,
                            )
                        })
                })
                .or_else(|| {
                    snapshot
                        .callouts
                        .iter()
                        .find(|markup| &markup.id == id)
                        .map(|markup| {
                            (
                                AnnotationKind::Callout,
                                markup.page_index,
                                markup.locked,
                                markup.content() == content,
                            )
                        })
                })
                .or_else(|| {
                    snapshot
                        .cloud_pluses
                        .iter()
                        .find(|markup| &markup.id == id)
                        .map(|markup| {
                            (
                                AnnotationKind::CloudPlus,
                                markup.page_index,
                                markup.locked,
                                markup.content() == content,
                            )
                        })
                });
            let Some((kind, page_index, locked, unchanged)) = found else {
                return Ok(false);
            };
            if locked || unchanged {
                return Ok(false);
            }
            (kind, page_index)
        };
        if kind == AnnotationKind::Dimension && !content.is_ascii() {
            return Err("Dimension captions currently support ASCII text only".into());
        }
        let caption_supplement =
            self.annotation_caption_selection_paths(document_id, page_index, cx);
        let id = id.clone();
        self.update_annotation_history(
            document_id,
            cx,
            move |annotations, document_id| match kind {
                AnnotationKind::TextBox => {
                    if !annotations.select_id(document_id, &id) {
                        return Err(AnnotationError::NoSelection);
                    }
                    annotations.replace_selected_text(document_id, content)
                }
                AnnotationKind::Dimension => {
                    if !annotations.select_id(document_id, &id) {
                        return Err(AnnotationError::NoSelection);
                    }
                    annotations.replace_selected_dimension_content(document_id, content)
                }
                AnnotationKind::Callout => {
                    annotations.replace_callout_text(document_id, &id, content)
                }
                _ => annotations.replace_cloud_plus_text_with_routing_supplement(
                    document_id,
                    &id,
                    content,
                    &caption_supplement,
                ),
            },
        )?;
        cx.notify();
        Ok(true)
    }
}

/// An upright editor centred where a Dimension draws its caption, wide
/// enough for its text and the next character.
fn dimension_caption_editor_rect(
    dimension: &crate::annotation_model::DimensionAnnotation,
    style: &TextBoxStyle,
    content: &str,
    text_system: &gpui::WindowTextSystem,
) -> PdfRect {
    let center = dimension.caption_center();
    let anchor = PdfPoint {
        x: center.x + dimension.caption_offset.x,
        y: center.y + dimension.caption_offset.y,
    };
    let font_size = style.font_size_pt();
    let mut caption_font = font(display_annotation_font_family(style.font_family()).to_owned());
    caption_font.weight = gpui::FontWeight(style.weight() as f32);
    let lines: Vec<&str> = content.split('\n').collect();
    let widest = lines
        .iter()
        .map(|line| {
            let run = TextRun {
                len: line.len(),
                font: caption_font.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f64::from(f32::from(
                text_system
                    .shape_line(
                        (*line).to_owned().into(),
                        px(font_size as f32),
                        &[run],
                        None,
                    )
                    .width(),
            ))
        })
        .fold(0., f64::max);
    let width = (widest + style.inset_pt() * 2. + font_size).max(font_size * 4.);
    let height = style.line_height_pt() * lines.len() as f64 + style.inset_pt() * 2.;
    PdfRect {
        x: anchor.x - width / 2.,
        y: anchor.y - height / 2.,
        width,
        height,
    }
}

/// A rectangle's corners turned about its centre, as the markup draws it.
fn rotated_rect_corners(rect: PdfRect, rotation_degrees: f64) -> [PdfPoint; 4] {
    let center_x = rect.x + rect.width * 0.5;
    let center_y = rect.y + rect.height * 0.5;
    let (sin, cos) = (-rotation_degrees).to_radians().sin_cos();
    [
        (rect.x, rect.y + rect.height),
        (rect.x + rect.width, rect.y + rect.height),
        (rect.x + rect.width, rect.y),
        (rect.x, rect.y),
    ]
    .map(|(x, y)| {
        let (dx, dy) = (x - center_x, y - center_y);
        PdfPoint {
            x: center_x + dx * cos - dy * sin,
            y: center_y + dx * sin + dy * cos,
        }
    })
}

/// The extent of a box's letters, turned with it: its lines from one inset
/// in from the top (centred top to bottom in a caption box `fitted` to its
/// text), each as wide as it is drawn and aligned as the style says. Empty
/// text keeps one em where the caret goes.
fn glyph_region(
    rect: PdfRect,
    rotation_degrees: f64,
    style: &TextBoxStyle,
    content: &str,
    fitted: bool,
    text_system: &gpui::WindowTextSystem,
) -> [PdfPoint; 4] {
    let inset = style.inset_pt();
    let font_size = style.font_size_pt();
    let room = (rect.width - inset * 2.).max(0.);
    let mut caption_font = font(display_annotation_font_family(style.font_family()).to_owned());
    caption_font.weight = gpui::FontWeight(style.weight() as f32);
    let lines: Vec<&str> = content.split('\n').collect();
    let (mut left, mut right) = (f64::INFINITY, f64::NEG_INFINITY);
    for line in &lines {
        let run = TextRun {
            len: line.len(),
            font: caption_font.clone(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = f64::from(f32::from(
            text_system
                .shape_line(
                    (*line).to_owned().into(),
                    px(font_size as f32),
                    &[run],
                    None,
                )
                .width(),
        ))
        .max(font_size);
        let start = match style.alignment() {
            TextAlignment::Left => 0.,
            TextAlignment::Center => (room - width) * 0.5,
            TextAlignment::Right => room - width,
        };
        left = left.min(start);
        right = right.max(start + width);
    }
    let block = style.line_height_pt() * lines.len() as f64;
    let top = if fitted {
        rect.y + (rect.height + block) * 0.5
    } else {
        rect.y + rect.height - inset
    };
    let letters = PdfRect {
        x: rect.x + inset + left.max(-inset),
        y: (top - block).max(rect.y),
        width: (right.min(room + inset) - left.max(-inset)).max(0.),
        height: block.min(rect.height),
    };
    let center_x = rect.x + rect.width * 0.5;
    let center_y = rect.y + rect.height * 0.5;
    let (sin, cos) = (-rotation_degrees).to_radians().sin_cos();
    rotated_rect_corners(letters, 0.).map(|corner| {
        let (dx, dy) = (corner.x - center_x, corner.y - center_y);
        PdfPoint {
            x: center_x + dx * cos - dy * sin,
            y: center_y + dx * sin + dy * cos,
        }
    })
}

/// The run of word characters, spaces or other characters around the byte
/// `offset`, as a double-click in a text field selects it.
fn word_range(content: &str, offset: usize) -> std::ops::Range<usize> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Other,
    }
    let class = |character: char| {
        if character.is_alphanumeric() || character == '_' {
            Class::Word
        } else if character.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };
    let mut offset = offset.min(content.len());
    while !content.is_char_boundary(offset) {
        offset -= 1;
    }
    // The character under the pointer, or the last one at the end of a line.
    let Some((start, character)) = content[offset..]
        .chars()
        .next()
        .filter(|character| *character != '\n')
        .map(|character| (offset, character))
        .or_else(|| content[..offset].char_indices().next_back())
        .filter(|(_, character)| *character != '\n')
    else {
        return offset..offset;
    };
    let kind = class(character);
    let begin = content[..start]
        .char_indices()
        .rev()
        .take_while(|(_, character)| *character != '\n' && class(*character) == kind)
        .last()
        .map_or(start, |(index, _)| index);
    let end = content[start..]
        .char_indices()
        .take_while(|(_, character)| *character != '\n' && class(*character) == kind)
        .last()
        .map_or(start, |(index, character)| {
            start + index + character.len_utf8()
        });
    begin..end
}

/// Whether `point` lies inside a convex quadrilateral, either winding.
fn point_in_quad(point: PdfPoint, corners: &[PdfPoint; 4]) -> bool {
    let mut sign = 0.;
    for index in 0..4 {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        if cross.abs() < f64::EPSILON {
            continue;
        }
        if sign == 0. {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.
}

/// A quadrilateral with each side pushed `margin` outward.
fn expanded_quad(corners: [PdfPoint; 4], margin: f64) -> [PdfPoint; 4] {
    let unit = |from: PdfPoint, to: PdfPoint| {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let length = dx.hypot(dy);
        if length <= f64::EPSILON {
            (0., 0.)
        } else {
            (dx / length, dy / length)
        }
    };
    std::array::from_fn(|index| {
        let corner = corners[index];
        let previous = corners[(index + 3) % 4];
        let next = corners[(index + 1) % 4];
        let away_previous = unit(previous, corner);
        let away_next = unit(next, corner);
        PdfPoint {
            x: corner.x + (away_previous.0 + away_next.0) * margin,
            y: corner.y + (away_previous.1 + away_next.1) * margin,
        }
    })
}

/// The byte offset in the text nearest `point`, laid out as the markup
/// draws it: lines from the top of its box, one inset in, aligned.
fn text_offset_at(
    hit: &MarkupTextHit,
    point: PdfPoint,
    text_system: &gpui::WindowTextSystem,
) -> usize {
    let [top_left, top_right, _, bottom_left] = hit.corners;
    let across = (top_right.x - top_left.x, top_right.y - top_left.y);
    let down = (bottom_left.x - top_left.x, bottom_left.y - top_left.y);
    let width = across.0.hypot(across.1);
    let height = down.0.hypot(down.1);
    if width <= f64::EPSILON || height <= f64::EPSILON {
        return 0;
    }
    let relative = (point.x - top_left.x, point.y - top_left.y);
    let x = (relative.0 * across.0 + relative.1 * across.1) / width;
    let y = (relative.0 * down.0 + relative.1 * down.1) / height;
    let style = &hit.style;
    let inset = style.inset_pt();
    let line_height = style.line_height_pt().max(f64::EPSILON);
    let lines: Vec<&str> = hit.content.split('\n').collect();
    let line_index = (((y - inset).max(0.) / line_height).floor() as usize).min(lines.len() - 1);
    let line_start: usize = lines[..line_index].iter().map(|line| line.len() + 1).sum();
    let line = lines[line_index];
    let mut caption_font = font(display_annotation_font_family(style.font_family()).to_owned());
    caption_font.weight = gpui::FontWeight(style.weight() as f32);
    let run = TextRun {
        len: line.len(),
        font: caption_font,
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = text_system.shape_line(
        line.to_owned().into(),
        px(style.font_size_pt() as f32),
        &[run],
        None,
    );
    let line_width = f64::from(f32::from(shaped.width()));
    let room = (width - inset * 2.).max(0.);
    let start = match style.alignment() {
        TextAlignment::Left => 0.,
        TextAlignment::Center => (room - line_width) * 0.5,
        TextAlignment::Right => room - line_width,
    };
    let within = (x - inset - start).max(0.);
    line_start
        + shaped
            .closest_index_for_x(px(within as f32))
            .min(line.len())
}

#[cfg(test)]
mod tests {
    use super::{expanded_quad, point_in_quad, rotated_rect_corners, word_range};
    use crate::annotation_model::{PdfPoint, PdfRect};

    #[test]
    fn a_double_click_selects_the_word_space_run_or_symbols_under_it() {
        let text = "Cloud+ steel\nbeam";
        assert_eq!(word_range(text, 2), 0..5);
        assert_eq!(word_range(text, 5), 5..6);
        assert_eq!(word_range(text, 6), 6..7);
        assert_eq!(word_range(text, 9), 7..12);
        // At the end of a line, the word before the break.
        assert_eq!(word_range(text, 12), 7..12);
        assert_eq!(word_range(text, 14), 13..17);
        assert_eq!(word_range(text, 17), 13..17);
        assert_eq!(word_range("", 0), 0..0);
        assert_eq!(word_range("façade", 3), 0.."façade".len());
    }

    #[test]
    fn quads_contain_their_inside_and_grow_outward() {
        let rect = PdfRect {
            x: 10.,
            y: 20.,
            width: 40.,
            height: 10.,
        };
        let corners = rotated_rect_corners(rect, 0.);
        assert!(point_in_quad(PdfPoint { x: 30., y: 25. }, &corners));
        assert!(!point_in_quad(PdfPoint { x: 52., y: 25. }, &corners));
        let grown = expanded_quad(corners, 3.);
        assert!(point_in_quad(PdfPoint { x: 52., y: 25. }, &grown));
        assert!(!point_in_quad(PdfPoint { x: 54., y: 25. }, &grown));
        // A quarter turn stands the box on end about its centre.
        let turned = rotated_rect_corners(rect, 90.);
        assert!(point_in_quad(PdfPoint { x: 30., y: 40. }, &turned));
        assert!(!point_in_quad(PdfPoint { x: 45., y: 25. }, &turned));
    }
}
