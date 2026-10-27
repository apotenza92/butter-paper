//! Arranging a document's pages from Page Thumbnails: adding blank pages,
//! deleting, moving, duplicating, and copying, cutting and pasting pages,
//! each one undoable change. A page takes its markups and edits along.

use super::*;
use crate::annotation_model::ArrangedPage;
use crate::pdf_page_structure::PageSlot;
use std::collections::BTreeSet;

gpui::actions!(
    document_workspace,
    [
        InsertPageBefore,
        InsertPageAfter,
        InsertPageAtEnd,
        DuplicatePages,
        DeletePages,
        CopyPages,
        CutPages,
        PastePages,
        RotatePagesLeft,
        RotatePagesRight
    ]
);

/// The key context of the focused thumbnail list.
pub(super) const PAGE_THUMBNAILS_CONTEXT: &str = "PageThumbnails";

pub(super) fn init_page_actions(cx: &mut App) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-d", DuplicatePages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-d", DuplicatePages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", CopyPages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", CopyPages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", CutPages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", CutPages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", PastePages, Some(PAGE_THUMBNAILS_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", PastePages, Some(PAGE_THUMBNAILS_CONTEXT)),
        KeyBinding::new("backspace", DeletePages, Some(PAGE_THUMBNAILS_CONTEXT)),
        KeyBinding::new("delete", DeletePages, Some(PAGE_THUMBNAILS_CONTEXT)),
    ]);
}

/// The thumbnails selected in a document's Page Thumbnails.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PageSelection {
    pub(super) document_id: DocumentId,
    pub(super) pages: BTreeSet<u32>,
    /// Where a Shift-click range starts.
    pub(super) anchor: u32,
}

/// Pages copied or cut, to paste in the same document.
#[derive(Clone, Debug)]
pub(super) struct PageClipboard {
    document_id: DocumentId,
    pages: Vec<u32>,
    /// The arrangement the page numbers are of: pasting after the pages
    /// are rearranged would paste others.
    arrangement: Vec<PageSlot>,
    cut: bool,
}

/// Thumbnails being dragged to a new place.
#[derive(Clone, Debug)]
pub struct PageDrag {
    pub(super) document_id: DocumentId,
    pub(super) pages: Vec<u32>,
}

/// What follows the pointer while thumbnails are dragged.
pub(super) struct PageDragGhost {
    label: SharedString,
}

impl Render for PageDragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .text_sm()
            .shadow_md()
            .child(self.label.clone())
    }
}

impl PageDragGhost {
    /// A ghost showing `label`.
    pub(super) fn labelled(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl PageDrag {
    pub(super) fn ghost(&self, cx: &mut App) -> Entity<PageDragGhost> {
        let label = match self.pages.as_slice() {
            [page] => format!("Page {}", page + 1),
            pages => format!("{} pages", pages.len()),
        };
        cx.new(|_| PageDragGhost {
            label: label.into(),
        })
    }
}

/// The pages in their order, each shown as it is.
fn pages_as_shown(count: u32) -> Vec<ArrangedPage> {
    (0..count)
        .map(|index| ArrangedPage::Shown { index, copy: false })
        .collect()
}

/// `pages` moved to stand before page `before` of the pages as shown.
fn moved(count: u32, pages: &[u32], before: u32) -> Vec<ArrangedPage> {
    let moving = pages.iter().copied().collect::<BTreeSet<_>>();
    let mut arranged = pages_as_shown(count)
        .into_iter()
        .filter(|page| !matches!(page, ArrangedPage::Shown { index, .. } if moving.contains(index)))
        .collect::<Vec<_>>();
    let at = (0..before.min(count))
        .filter(|page| !moving.contains(page))
        .count();
    for (offset, page) in moving.iter().enumerate() {
        arranged.insert(
            at + offset,
            ArrangedPage::Shown {
                index: *page,
                copy: false,
            },
        );
    }
    arranged
}

/// Copies of `pages` placed before page `before` of the pages as shown.
fn with_copies(count: u32, pages: &[u32], before: u32) -> Vec<ArrangedPage> {
    let mut arranged = pages_as_shown(count);
    let at = before.min(count) as usize;
    for (offset, page) in pages.iter().enumerate() {
        arranged.insert(
            at + offset,
            ArrangedPage::Shown {
                index: *page,
                copy: true,
            },
        );
    }
    arranged
}

impl DocumentWorkspace {
    /// The pages Page Thumbnails acts on: those selected, or the current
    /// page.
    pub fn selected_pages(&self, document_id: DocumentId, cx: &App) -> Vec<u32> {
        let count = self
            .session(document_id, cx)
            .map_or(0, |session| session.read(cx).page_sizes.len() as u32);
        if let Some(selection) = self
            .page_selection
            .as_ref()
            .filter(|selection| selection.document_id == document_id)
        {
            let pages = selection
                .pages
                .iter()
                .copied()
                .filter(|page| *page < count)
                .collect::<Vec<_>>();
            if !pages.is_empty() {
                return pages;
            }
        }
        self.session(document_id, cx)
            .map(|session| vec![session.read(cx).current_page()])
            .unwrap_or_default()
    }

    /// Whether `page_index` is among the selected thumbnails.
    pub(super) fn page_selected(&self, document_id: DocumentId, page_index: u32) -> bool {
        self.page_selection.as_ref().is_some_and(|selection| {
            selection.document_id == document_id && selection.pages.contains(&page_index)
        })
    }

    /// A click on a thumbnail: selects its page and shows it; with Cmd or
    /// Ctrl adds it to or takes it from the selection, and with Shift
    /// selects the pages from the last one clicked.
    pub fn click_thumbnail(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        modifiers: gpui::Modifiers,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .page_selection
            .take()
            .filter(|selection| selection.document_id == document_id);
        let selection = match current {
            Some(mut selection) if modifiers.shift => {
                let (from, to) = if selection.anchor <= page_index {
                    (selection.anchor, page_index)
                } else {
                    (page_index, selection.anchor)
                };
                selection.pages = (from..=to).collect();
                selection
            }
            Some(mut selection) if modifiers.secondary() => {
                if !selection.pages.remove(&page_index) {
                    selection.pages.insert(page_index);
                }
                selection.anchor = page_index;
                selection
            }
            _ => PageSelection {
                document_id,
                pages: BTreeSet::from([page_index]),
                anchor: page_index,
            },
        };
        self.page_selection = Some(selection);
        self.activate_thumbnail_page(document_id, page_index, cx);
        cx.notify();
    }

    /// Selects `page_index` for a menu opened on its thumbnail, unless it
    /// is already among those selected.
    pub(super) fn select_thumbnail_for_menu(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) {
        if !self.page_selected(document_id, page_index) {
            self.page_selection = Some(PageSelection {
                document_id,
                pages: BTreeSet::from([page_index]),
                anchor: page_index,
            });
            cx.notify();
        }
    }

    /// Rearranges `document_id`'s pages as `pages` lists them, as one
    /// undoable change, and selects `select` (pages as now shown).
    pub fn arrange_pages(
        &mut self,
        document_id: DocumentId,
        pages: Vec<ArrangedPage>,
        select: Vec<u32>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("the document is not ready for changes".into());
        }
        let page_count = session.read(cx).page_sizes.len() as u32;
        self.commit_page_text_editing(cx);
        if self
            .content_selection
            .as_ref()
            .is_some_and(|selection| selection.document_id == document_id)
        {
            self.content_selection = None;
            self.content_drag = None;
        }
        self.update_native_session_history_with_result(document_id, cx, |session| {
            session
                .annotations
                .arrange_document_pages(document_id.value(), page_count, &pages)
                .map_err(|error| error.to_string())
        })?;
        self.after_pages_arranged(document_id, cx);
        if let Some(&first) = select.first() {
            self.page_selection = Some(PageSelection {
                document_id,
                pages: select.into_iter().collect(),
                anchor: first,
            });
            self.activate_thumbnail_page(document_id, first, cx);
        }
        cx.notify();
        Ok(())
    }

    /// Forgets what names pages by where they were, and shows the new order.
    pub(super) fn after_pages_arranged(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        self.page_interactions
            .retain(|(owner, _), _| *owner != document_id);
        self.last_painted_page_evidence
            .retain(|(owner, _), _| *owner != document_id);
        self.form_page_transforms
            .retain(|(owner, _), _| *owner != document_id);
        self.forget_page_text_layers(document_id);
        self.page_selection = None;
        self.page_drop_target = None;
        self.sync_document_edits(document_id, cx);
    }

    /// Adds a blank page the size of the page beside it, before page
    /// `before` (the page count: at the end).
    pub fn insert_blank_page(
        &mut self,
        document_id: DocumentId,
        before: u32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (count, size) = {
            let session = self
                .session(document_id, cx)
                .ok_or("document session is closed")?
                .read(cx);
            let count = session.page_sizes.len() as u32;
            let beside = before.min(count.saturating_sub(1));
            (
                count,
                session
                    .page_sizes
                    .get(beside as usize)
                    .copied()
                    .unwrap_or((612., 792.)),
            )
        };
        let before = before.min(count);
        let mut pages = pages_as_shown(count);
        pages.insert(
            before as usize,
            ArrangedPage::Blank {
                width: f64::from(size.0),
                height: f64::from(size.1),
            },
        );
        self.arrange_pages(document_id, pages, vec![before], cx)
    }

    pub fn delete_pages(
        &mut self,
        document_id: DocumentId,
        pages: &[u32],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let count = self.page_count(document_id, cx).unwrap_or(0);
        let deleted = pages.iter().copied().collect::<BTreeSet<_>>();
        let arranged = pages_as_shown(count)
            .into_iter()
            .filter(|page| !matches!(page, ArrangedPage::Shown { index, .. } if deleted.contains(index)))
            .collect::<Vec<_>>();
        if arranged.is_empty() {
            return Err("A document needs at least one page.".into());
        }
        let first = deleted.first().copied().unwrap_or(0);
        let select = first.min(arranged.len() as u32 - 1);
        self.arrange_pages(document_id, arranged, vec![select], cx)
    }

    /// Moves `pages` to stand before page `before` (the page count: the
    /// end).
    pub fn move_pages(
        &mut self,
        document_id: DocumentId,
        pages: &[u32],
        before: u32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let count = self.page_count(document_id, cx).unwrap_or(0);
        let pages = pages
            .iter()
            .copied()
            .filter(|page| *page < count)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if pages.is_empty() {
            return Ok(());
        }
        let at = (0..before.min(count))
            .filter(|page| !pages.contains(page))
            .count() as u32;
        let select = (at..at + pages.len() as u32).collect();
        self.arrange_pages(document_id, moved(count, &pages, before), select, cx)
    }

    /// Adds a copy of each of `pages` after the last of them.
    pub fn duplicate_pages(
        &mut self,
        document_id: DocumentId,
        pages: &[u32],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let count = self.page_count(document_id, cx).unwrap_or(0);
        let pages = pages
            .iter()
            .copied()
            .filter(|page| *page < count)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let Some(&last) = pages.last() else {
            return Ok(());
        };
        let at = last + 1;
        let select = (at..at + pages.len() as u32).collect();
        self.arrange_pages(document_id, with_copies(count, &pages, at), select, cx)
    }

    /// Copies or cuts `pages`, to paste in this document. Cut pages stay
    /// until they are pasted, which moves them.
    pub fn copy_pages(
        &mut self,
        document_id: DocumentId,
        pages: &[u32],
        cut: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let arrangement = session.read(cx).arranged_slots.clone();
        let mut pages = pages.to_vec();
        pages.sort_unstable();
        pages.dedup();
        let count = pages.len();
        self.page_clipboard = Some(PageClipboard {
            document_id,
            pages,
            arrangement,
            cut,
        });
        let noun = if count == 1 { "page" } else { "pages" };
        let verb = if cut { "Cut" } else { "Copied" };
        self.annotation_statuses.insert(
            document_id,
            format!("{verb} {count} {noun}: select a page and paste to place them after it"),
        );
        cx.notify();
    }

    /// Whether copied or cut pages can be pasted into `document_id`.
    pub fn can_paste_pages(&self, document_id: DocumentId, cx: &App) -> bool {
        self.page_clipboard.as_ref().is_some_and(|clipboard| {
            clipboard.document_id == document_id
                && self
                    .session(document_id, cx)
                    .is_some_and(|session| session.read(cx).arranged_slots == clipboard.arrangement)
        })
    }

    /// Pastes copied pages, or moves cut ones, after page `after`.
    pub fn paste_pages(
        &mut self,
        document_id: DocumentId,
        after: u32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(clipboard) = self.page_clipboard.clone() else {
            return Err("Copy or cut pages first.".into());
        };
        if clipboard.document_id != document_id {
            return Err(
                "Pages paste into the document they came from. To bring pages from another document, use Merge.".into(),
            );
        }
        if !self.can_paste_pages(document_id, cx) {
            self.page_clipboard = None;
            return Err(
                "The pages were rearranged since they were copied. Copy them again.".into(),
            );
        }
        let before = after + 1;
        if clipboard.cut {
            self.page_clipboard = None;
            self.move_pages(document_id, &clipboard.pages, before, cx)
        } else {
            let count = self.page_count(document_id, cx).unwrap_or(0);
            let at = before.min(count);
            let select = (at..at + clipboard.pages.len() as u32).collect();
            self.arrange_pages(
                document_id,
                with_copies(count, &clipboard.pages, before),
                select,
                cx,
            )?;
            // The copies paste again until the pages are rearranged.
            let arrangement = self
                .session(document_id, cx)
                .map(|session| session.read(cx).arranged_slots.clone());
            if let Some(clipboard) = self.page_clipboard.as_mut()
                && let Some(arrangement) = arrangement
            {
                clipboard.arrangement = arrangement;
                clipboard.pages = clipboard
                    .pages
                    .iter()
                    .map(|page| {
                        if *page >= at {
                            page + clipboard.pages.len() as u32
                        } else {
                            *page
                        }
                    })
                    .collect();
            }
            Ok(())
        }
    }

    /// Whether the thumbnails have the keyboard focus, so Copy, Cut, Paste
    /// and Delete act on pages.
    pub(super) fn page_thumbnails_focused(&self, window: &Window, cx: &App) -> bool {
        self.thumbnail_focus.contains_focused(window, cx)
    }

    /// Runs a page operation on the active document's selected pages,
    /// showing a refusal as its status.
    fn with_selected_pages(
        &mut self,
        cx: &mut Context<Self>,
        operation: impl FnOnce(
            &mut Self,
            DocumentId,
            Vec<u32>,
            &mut Context<Self>,
        ) -> Result<(), String>,
    ) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let pages = self.selected_pages(document_id, cx);
        if pages.is_empty() {
            return;
        }
        if let Err(error) = operation(self, document_id, pages, cx) {
            self.annotation_statuses.insert(document_id, error);
            cx.notify();
        }
    }

    pub(super) fn insert_page_before_from_action(
        &mut self,
        _: &InsertPageBefore,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.insert_blank_page(document_id, pages[0], cx)
        });
    }

    pub(super) fn insert_page_after_from_action(
        &mut self,
        _: &InsertPageAfter,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.insert_blank_page(document_id, pages[pages.len() - 1] + 1, cx)
        });
    }

    pub(super) fn insert_page_at_end_from_action(
        &mut self,
        _: &InsertPageAtEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, _, cx| {
            let count = workspace.page_count(document_id, cx).unwrap_or(0);
            workspace.insert_blank_page(document_id, count, cx)
        });
    }

    pub(super) fn duplicate_pages_from_action(
        &mut self,
        _: &DuplicatePages,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.duplicate_pages(document_id, &pages, cx)
        });
    }

    pub(super) fn delete_pages_from_action(
        &mut self,
        _: &DeletePages,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.delete_pages(document_id, &pages, cx)
        });
    }

    pub(super) fn copy_pages_from_action(
        &mut self,
        _: &CopyPages,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.copy_pages(document_id, &pages, false, cx);
            Ok(())
        });
    }

    pub(super) fn cut_pages_from_action(
        &mut self,
        _: &CutPages,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.copy_pages(document_id, &pages, true, cx);
            Ok(())
        });
    }

    pub(super) fn paste_pages_from_action(
        &mut self,
        _: &PastePages,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.paste_pages(document_id, pages[pages.len() - 1], cx)
        });
    }

    pub(super) fn rotate_pages_left_from_action(
        &mut self,
        _: &RotatePagesLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.rotate_pages(document_id, pages, PageRotationDirection::Left, cx);
            Ok(())
        });
    }

    pub(super) fn rotate_pages_right_from_action(
        &mut self,
        _: &RotatePagesRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_selected_pages(cx, |workspace, document_id, pages, cx| {
            workspace.rotate_pages(document_id, pages, PageRotationDirection::Right, cx);
            Ok(())
        });
    }

    /// Rotates `pages` one after another: a page turns once the one before
    /// has its new pixels.
    pub fn rotate_pages(
        &mut self,
        document_id: DocumentId,
        pages: Vec<u32>,
        direction: PageRotationDirection,
        cx: &mut Context<Self>,
    ) {
        self.queued_page_rotations
            .extend(pages.into_iter().map(|page| (document_id, page, direction)));
        self.rotate_next_queued_page(cx);
    }

    /// Starts the next queued page rotation when none is under way.
    pub(super) fn rotate_next_queued_page(&mut self, cx: &mut Context<Self>) {
        while let Some((document_id, page, direction)) = self.queued_page_rotations.pop_front() {
            let pending = self
                .session(document_id, cx)
                .map(|session| session.read(cx).pending_rotation_generation.is_some());
            match pending {
                None => continue,
                Some(true) => {
                    self.queued_page_rotations
                        .push_front((document_id, page, direction));
                    return;
                }
                Some(false) => {
                    self.rotate_thumbnail_page_async(document_id, page, direction, cx);
                    return;
                }
            }
        }
    }

    /// Where dragged thumbnails would land as the pointer moves over the
    /// thumbnail of `page_index`, whose bounds are `bounds`: before it in its
    /// top half, after it in its bottom half.
    pub(super) fn update_page_drop_target(
        &mut self,
        event: &gpui::DragMoveEvent<PageDrag>,
        page_index: u32,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx);
        let document_id = drag.document_id;
        if !event.bounds.contains(&event.event.position) {
            return;
        }
        let before = if event.event.position.y < event.bounds.center().y {
            page_index
        } else {
            page_index + 1
        };
        let target = Some((document_id, before));
        if self.page_drop_target != target {
            self.page_drop_target = target;
            cx.notify();
        }
    }

    /// Moves dragged thumbnails to where they were dropped.
    pub(super) fn drop_pages(&mut self, drag: &PageDrag, cx: &mut Context<Self>) {
        let Some((document_id, before)) = self.page_drop_target.take() else {
            return;
        };
        if document_id != drag.document_id {
            return;
        }
        if let Err(error) = self.move_pages(document_id, &drag.pages, before, cx) {
            self.annotation_statuses.insert(document_id, error);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(arranged: &[ArrangedPage]) -> Vec<(u32, bool)> {
        arranged
            .iter()
            .map(|page| match page {
                ArrangedPage::Shown { index, copy } => (*index, *copy),
                ArrangedPage::Blank { .. } => (u32::MAX, false),
            })
            .collect()
    }

    #[test]
    fn moved_pages_land_before_the_page_named() {
        assert_eq!(
            shown(&moved(5, &[0, 1], 4)),
            [(2, false), (3, false), (0, false), (1, false), (4, false)]
        );
        assert_eq!(
            shown(&moved(4, &[3], 0)),
            [(3, false), (0, false), (1, false), (2, false)]
        );
        assert_eq!(
            shown(&moved(3, &[0], 3)),
            [(1, false), (2, false), (0, false)]
        );
    }

    #[test]
    fn copies_land_before_the_page_named() {
        assert_eq!(
            shown(&with_copies(3, &[0, 2], 3)),
            [(0, false), (1, false), (2, false), (0, true), (2, true)]
        );
    }
}
