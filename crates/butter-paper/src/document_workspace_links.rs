//! Following a PDF's links with the Select tool.
//!
//! Markups and form fields sit above links. A link shows the pointing hand;
//! a click on it (a press and a release without dragging) opens a web or
//! mail address with the system, or shows the page it links to. Other
//! addresses are not opened.

use super::*;
use crate::pdf_links::{LinkLayer, LinkTarget, openable_uri};

/// How far outside a link's rectangle the pointer still hovers it.
const LINK_HIT_TOLERANCE_PX: f64 = 1.;
/// How far a press may move and still click a link.
const LINK_CLICK_SLOP_PX: f64 = 4.;

/// A press on a link, followed if released there.
#[derive(Clone, Debug)]
pub(super) struct LinkPress {
    document_id: DocumentId,
    target: LinkTarget,
    position: gpui::Point<Pixels>,
}

impl DocumentWorkspace {
    fn link_layer(&self, document_id: DocumentId, cx: &App) -> Option<Arc<LinkLayer>> {
        let layer = self
            .session(document_id, cx)?
            .read(cx)
            .resource
            .as_ref()?
            .link_layer();
        (!layer.links.is_empty()).then_some(layer)
    }

    /// The link under a page point. A page shows the links of the file page
    /// it shows; a new blank page has none.
    pub(super) fn link_hit(
        &self,
        interaction: &PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> Option<LinkTarget> {
        let layer = self.link_layer(interaction.document_id, cx)?;
        let session = self.session(interaction.document_id, cx)?.read(cx);
        let file_page = match session.arranged_slots.get(interaction.page_index as usize) {
            Some(slot) => slot.file_page()?,
            None => interaction.page_index,
        };
        let tolerance = interaction
            .transform
            .tolerance_points(LINK_HIT_TOLERANCE_PX)
            .unwrap_or(0.5);
        layer
            .link_at(file_page, point.x, point.y, tolerance)
            .map(|link| link.target.clone())
    }

    /// Hover feedback for links under the Select tool, where no markup or
    /// form field is. Hover and press share one hit test.
    pub(super) fn update_link_hover(
        &mut self,
        interaction: &PageInteraction,
        point: Option<PdfPoint>,
        cx: &mut Context<Self>,
    ) -> bool {
        let hover = point
            .and_then(|point| self.link_hit(interaction, point, cx))
            .map(|_| interaction.document_id);
        if self.link_hover != hover {
            self.link_hover = hover;
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(super) fn link_cursor(&self, document_id: DocumentId) -> Option<CursorStyle> {
        (self.link_hover == Some(document_id)).then_some(CursorStyle::PointingHand)
    }

    /// A Select-tool press on a link: held until the release follows it.
    /// Returns false, leaving the press to text selection, when no link is
    /// there or a markup is above it.
    pub(super) fn begin_link_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.markup_under_point(&interaction, point, cx) {
            return false;
        }
        let Some(target) = self.link_hit(&interaction, point, cx) else {
            return false;
        };
        // Pressing a link clears a markup selection and a text selection, as
        // pressing the page does.
        if let Some(session) = self.session(interaction.document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                session
                    .annotations
                    .clear_selection(interaction.document_id.value());
                cx.notify();
            });
        }
        self.clear_text_selection(cx);
        self.link_press = Some(LinkPress {
            document_id: interaction.document_id,
            target,
            position,
        });
        true
    }

    /// Ends a press on a link: a release near where it began follows it.
    /// Returns whether a link press was in progress.
    pub(super) fn finish_link_press(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(press) = self.link_press.take() else {
            return false;
        };
        if (position - press.position).magnitude() <= LINK_CLICK_SLOP_PX
            && self.active_document_id == Some(press.document_id)
        {
            self.follow_link(press.document_id, press.target, cx);
        }
        true
    }

    fn follow_link(&mut self, document_id: DocumentId, target: LinkTarget, cx: &mut Context<Self>) {
        match target {
            LinkTarget::Uri(uri) => {
                if openable_uri(&uri) {
                    self.annotation_statuses.remove(&document_id);
                    cx.open_url(uri.trim());
                } else {
                    self.annotation_statuses.insert(
                        document_id,
                        "Butter Paper only opens web and email links.".into(),
                    );
                }
            }
            LinkTarget::Page(file_page) => {
                // The page as arranged now: the file page itself, else a
                // copy of it. A deleted page cannot be shown.
                let page = self.session(document_id, cx).and_then(|session| {
                    let slots = &session.read(cx).arranged_slots;
                    if slots.is_empty() {
                        return Some(file_page);
                    }
                    slots
                        .iter()
                        .position(|slot| {
                            *slot
                                == crate::pdf_page_structure::PageSlot::Source { index: file_page }
                        })
                        .or_else(|| {
                            slots
                                .iter()
                                .position(|slot| slot.file_page() == Some(file_page))
                        })
                        .map(|page| page as u32)
                });
                match page {
                    Some(page) => {
                        self.activate_thumbnail_page(document_id, page, cx);
                    }
                    None => {
                        self.annotation_statuses.insert(
                            document_id,
                            "The page this link goes to has been deleted.".into(),
                        );
                    }
                }
            }
        }
        cx.notify();
    }
}
