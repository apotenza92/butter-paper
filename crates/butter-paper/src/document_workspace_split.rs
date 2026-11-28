//! Split View: one document in two panes side by side. Both panes show the
//! same session (its pages, markups, history and saves); each has its own
//! view. The session's own view fields hold the active pane's view, so
//! every command acts on it; `NativeDocumentSession::split` holds the
//! other pane's, and making that pane active swaps the two.

use super::viewport::{ViewerPageRender, ViewportInputs};
use super::*;
use crate::document_session::{PaneSide, split_left_width};
use crate::document_viewer::ViewerPlanSnapshot;
use gpui::AnyElement;

pub const SPLIT_CANVAS_ID: &str = "document-split-canvas";
pub const SPLIT_PANE_LEFT_ID: &str = "document-split-pane-left";
pub const SPLIT_PANE_RIGHT_ID: &str = "document-split-pane-right";
pub const SPLIT_DIVIDER_ID: &str = "document-split-divider";
pub const SPLIT_OTHER_VIEWPORT_ID: &str = "document-viewport-other";
/// The ring round the active pane.
pub const SPLIT_ACTIVE_RING_ID: &str = "document-split-active-ring";
/// The divider's width, all of it a hit target; its line is drawn in the
/// middle.
pub(super) const SPLIT_DIVIDER_WIDTH: f32 = 8.;
/// How far an arrow key moves the divider.
const DIVIDER_KEY_STEP: f32 = 16.;
const SPLIT_DIVIDER_CONTEXT: &str = "SplitDivider";

gpui::actions!(
    split_view,
    [
        SplitDividerLeft,
        SplitDividerRight,
        SplitDividerHome,
        SplitDividerEnd,
        SplitDividerReset
    ]
);

/// The divider's keys, while it has the focus: arrows move it, Home and
/// End to either least width, Enter back to the middle.
pub(super) fn init_split_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", SplitDividerLeft, Some(SPLIT_DIVIDER_CONTEXT)),
        KeyBinding::new("right", SplitDividerRight, Some(SPLIT_DIVIDER_CONTEXT)),
        KeyBinding::new("home", SplitDividerHome, Some(SPLIT_DIVIDER_CONTEXT)),
        KeyBinding::new("end", SplitDividerEnd, Some(SPLIT_DIVIDER_CONTEXT)),
        KeyBinding::new("enter", SplitDividerReset, Some(SPLIT_DIVIDER_CONTEXT)),
    ]);
}

impl DocumentWorkspace {
    /// Split View on or off for `document_id`. On, the other pane starts as
    /// a copy of the view; off, the active pane's view stays.
    pub fn toggle_split_view(&mut self, document_id: DocumentId, cx: &mut Context<Self>) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return false;
        }
        let on = session.update(cx, |session, cx| {
            // The active pane changes width: it keeps its current page.
            session.pending_keep_page = session
                .viewport_size
                .map(|(width, _)| (session.current_page, width));
            let on = if session.split.is_some() {
                session.close_split();
                false
            } else {
                session.open_split();
                true
            };
            cx.notify();
            on
        });
        if !on {
            self.split_drag = None;
            self.split_other_refresh_scheduled = None;
            self.split_other_quality_task = None;
        }
        self.sync_active_viewer_toolbar(cx);
        cx.notify();
        true
    }

    /// Whether the divider between the panes has the keyboard focus.
    pub fn split_divider_focused(&self, window: &Window) -> bool {
        self.split_divider_focus.is_focused(window)
    }

    /// Which pane is active and the left pane's share of the canvas, while
    /// `document_id` shows in Split View.
    pub fn split_view_state(&self, document_id: DocumentId, cx: &App) -> Option<(PaneSide, f32)> {
        let session = self.session(document_id, cx)?.read(cx);
        session
            .split
            .as_ref()
            .map(|split| (split.active, split.ratio))
    }

    /// The other pane's view settings, page and scroll.
    pub fn other_pane_view(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<(NativeDocumentViewState, u32, (f32, f32))> {
        let session = self.session(document_id, cx)?.read(cx);
        let other = &session.split.as_ref()?.other;
        Some((
            other.view_state.clone(),
            other.current_page,
            other.viewer.logical_scroll(),
        ))
    }

    /// The pages the other pane laid out in view at its last layout.
    pub fn other_pane_visible_pages(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<Vec<usize>> {
        let session = self.session(document_id, cx)?.read(cx);
        let plan = session.split.as_ref()?.other.viewer.plan_snapshot()?;
        Some(plan.visible_pages.clone())
    }

    /// Makes the other pane of `document_id` the active one. A text box,
    /// page text or form field being edited is committed first, as a
    /// click elsewhere commits it; if that fails, the pane stays inactive.
    pub fn activate_other_pane(
        &mut self,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        if session.read(cx).split.is_none() {
            return false;
        }
        if self
            .commit_pending_text_editor_before_application_close(cx)
            .is_err()
        {
            return false;
        }
        self.cancel_view_motion();
        self.pan_drag = None;
        self.hovered_annotation = None;
        self.hot_annotation_handle = None;
        self.select_hover_hit = None;
        self.pending_image_hover = None;
        // Edit PDF's hover shows in both panes: the pointer finds it again.
        self.page_text_hover = None;
        self.content_hover = None;
        session.update(cx, |session, cx| {
            session.swap_panes();
            cx.notify();
        });
        // What was painted belongs to the pane that was active; the next
        // frame records the newly active pane's.
        self.viewport_bounds.remove(&document_id);
        self.viewport_hitboxes.remove(&document_id);
        self.viewport_painted_layouts.remove(&document_id);
        self.viewport_painted_scroll.remove(&document_id);
        self.viewport_refresh_scheduled = None;
        self.split_other_refresh_scheduled = None;
        self.workspace_focus.focus(window, cx);
        self.sync_active_viewer_toolbar(cx);
        cx.notify();
        true
    }

    /// Makes the pane on `side` active; nothing happens if it already is.
    pub fn activate_split_pane(
        &mut self,
        document_id: DocumentId,
        side: PaneSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.split_view_state(document_id, cx) {
            Some((active, _)) if active != side => {
                self.activate_other_pane(document_id, window, cx)
            }
            _ => false,
        }
    }

    /// Moves the divider: the left pane takes `ratio` of the canvas, within
    /// each pane's least width.
    pub fn set_split_ratio(&mut self, document_id: DocumentId, ratio: f32, cx: &mut Context<Self>) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let ratio = match self.split_canvas_bounds.get() {
            Some(bounds) => {
                let width = f32::from(bounds.size.width);
                let room = (width - SPLIT_DIVIDER_WIDTH).max(1.);
                split_left_width(ratio, width, SPLIT_DIVIDER_WIDTH) / room
            }
            None => ratio.clamp(0., 1.),
        };
        session.update(cx, |session, cx| {
            if let Some(split) = &mut session.split
                && (split.ratio - ratio).abs() > f32::EPSILON
            {
                split.ratio = ratio;
                cx.notify();
            }
        });
        cx.notify();
    }

    fn move_split_divider(&mut self, step: Option<f32>, to: Option<f32>, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        let Some((_, ratio)) = self.split_view_state(document_id, cx) else {
            return;
        };
        let width = self
            .split_canvas_bounds
            .get()
            .map_or(1_000., |bounds| f32::from(bounds.size.width));
        let room = (width - SPLIT_DIVIDER_WIDTH).max(1.);
        let ratio = match (step, to) {
            (_, Some(to)) => to,
            (Some(step), None) => ratio + step / room,
            (None, None) => ratio,
        };
        self.set_split_ratio(document_id, ratio, cx);
    }

    /// The other pane was painted `width` by `height`: it is laid out
    /// again on the next frame when its size or scroll changed.
    fn observe_other_pane(
        &mut self,
        document_id: DocumentId,
        width: f32,
        height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let device_scale = window.scale_factor();
        let needs_plan = session.update(cx, |session, cx| {
            let ready = matches!(session.status, NativeDocumentStatus::Ready);
            let NativeDocumentSession {
                split, page_sizes, ..
            } = session;
            let Some(split) = split else {
                return false;
            };
            let other = &mut split.other;
            other.viewer.observe_viewport_size(width, height);
            other.viewer.reconcile_scroll();
            let next = Some((width, height));
            if other.viewport_size != next {
                let resized = other.viewport_size.is_some();
                other.viewport_size = next;
                let preset = match other.view_state.zoom_preset() {
                    ViewerZoomPreset::Manual => None,
                    ViewerZoomPreset::FitWidth => Some(ViewerFitPreset::Width),
                    ViewerZoomPreset::FitPage => Some(ViewerFitPreset::Page),
                };
                if resized && let Some(preset) = preset {
                    other.pending_page_frame = Some((other.current_page, preset));
                }
                if let Some(size) = pane_fit_size(page_sizes, &other.view_state, other.current_page)
                {
                    other.view_state.update_viewport(width, height, size);
                }
                other
                    .viewer
                    .configure(other.view_state.mode(), other.view_state.zoom_percent());
                cx.notify();
            }
            ready
                && other
                    .viewer
                    .needs_plan(width, height, device_scale, other.current_page as usize)
        });
        if !needs_plan || self.split_other_refresh_scheduled == Some(document_id) {
            return;
        }
        self.split_other_refresh_scheduled = Some(document_id);
        let workspace = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = workspace.update(cx, |workspace, cx| {
                if workspace.split_other_refresh_scheduled != Some(document_id) {
                    return;
                }
                workspace.split_other_refresh_scheduled = None;
                let _ = workspace.refresh_other_pane(
                    document_id,
                    width,
                    height,
                    window.scale_factor(),
                    cx,
                );
            });
        });
        // Asked for, so the pane is laid out at its new size at once, not
        // at the next input.
        window.request_animation_frame();
    }

    /// Lays the other pane out at its scroll, resolving any navigation it
    /// waits for, and asks for its tiles.
    pub fn refresh_other_pane(
        &mut self,
        document_id: DocumentId,
        viewport_width: f32,
        viewport_height: f32,
        device_scale: f32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let now = cx.background_executor().now();
        session.update(cx, |session, cx| {
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".to_owned());
            }
            let rotations = session.page_rotation_quarter_turns();
            let page_count = session.page_sizes.len();
            if page_count == 0 {
                return Err("viewer document geometry is unavailable".into());
            }
            let NativeDocumentSession {
                split, page_sizes, ..
            } = session;
            let Some(split) = split.as_mut() else {
                return Err("the document is not in Split View".into());
            };
            let other = &mut split.other;
            // Pages may have gone since the pane last laid out.
            let last = (page_count - 1) as u32;
            other.current_page = other.current_page.min(last);
            other.requested_page = other.requested_page.min(last);
            let view = &other.view_state;
            other.viewer.configure(view.mode(), view.zoom_percent());
            other.viewer.configure_cover_alone(view.cover_alone());
            other.viewer.configure_cad(view.cad_view_active().then(|| {
                (
                    match view.cad_organisation() {
                        CadViewOrganisation::Columns => PlannerCadOrganisation::Columns,
                        CadViewOrganisation::Rows => PlannerCadOrganisation::Rows,
                    },
                    view.pages_per_lane(),
                )
            }));
            let (mut scroll_x, mut scroll_y) = other.viewer.logical_scroll();
            let viewport = (viewport_width, viewport_height);
            let plan_at = |other: &mut crate::document_session::PaneView,
                           page: u32,
                           scroll: (f32, f32)|
             -> Result<ViewerPlanSnapshot, String> {
                other.view_state.set_scroll(scroll.0, scroll.1);
                other.viewer.set_scroll(scroll.0, scroll.1);
                other.viewer.observe_motion(scroll.0, scroll.1, now);
                other.viewer.plan_at(
                    document_id.value(),
                    page_sizes,
                    &rotations,
                    page as usize,
                    viewport.0,
                    viewport.1,
                    scroll.0,
                    scroll.1,
                    device_scale,
                    now,
                )
            };
            let mut plan = plan_at(other, other.current_page, (scroll_x, scroll_y))?;
            // The anchor was taken at the centre of the view the pane was
            // copied from: it holds at this pane's centre.
            if let Some(mut anchor) = other.pending_zoom_anchor.take()
                && let Some((anchored_x, anchored_y)) = crate::zoom_anchor::anchored_scroll(
                    {
                        anchor.viewport_point = (viewport_width / 2., viewport_height / 2.);
                        anchor
                    },
                    &plan.page_layouts,
                    (plan.total_width, plan.total_height),
                    viewport,
                    other.viewer.pan_margin(),
                )
            {
                scroll_x = anchored_x;
                scroll_y = anchored_y;
                plan = plan_at(other, other.current_page, (scroll_x, scroll_y))?;
            }
            let mut navigated = false;
            // A fit framing the view keeps the current page, as navigation
            // does: the layout moved the view, not the reader.
            let mut framed = false;
            if let Some(target) = other.pending_navigation_scroll.take()
                && let Some(layout) = plan
                    .page_layouts
                    .iter()
                    .find(|layout| layout.page == target as usize)
            {
                let rect = layout.logical_rect;
                scroll_y = rect.y.max(0.);
                // A page off the view across comes into it.
                if rect.x < scroll_x || rect.width > viewport_width {
                    scroll_x = rect.x;
                } else if rect.x + rect.width > scroll_x + viewport_width {
                    scroll_x = rect.x + rect.width - viewport_width;
                }
                other.current_page = target;
                other.requested_page = target;
                navigated = true;
                plan = plan_at(other, target, (scroll_x, scroll_y))?;
            }
            if let Some((page, preset)) = other.pending_page_frame.take()
                && let Some(rect) = crate::document_viewer::frame_rect(
                    &plan.page_layouts,
                    page as usize,
                    other.view_state.mode(),
                    plan.total_width,
                )
            {
                scroll_x = rect.x + rect.width / 2. - viewport_width / 2.;
                if preset == ViewerFitPreset::Page {
                    scroll_y = rect.y + rect.height / 2. - viewport_height / 2.;
                }
                // Fit Page in Two Pages keeps the row's paper edges in view,
                // the first and last rows too.
                let edge = if preset == ViewerFitPreset::Page
                    && other.view_state.mode() == PageViewMode::TwoPages
                {
                    crate::native_document_view_state::FIT_PAGE_EDGE_ROOM
                } else {
                    0.
                };
                scroll_y = if plan.total_height > viewport_height {
                    scroll_y.clamp(-edge, plan.total_height - viewport_height + edge)
                } else {
                    (plan.total_height - viewport_height) / 2.
                };
                framed = true;
                plan = plan_at(other, page, (scroll_x, scroll_y))?;
            }
            let cover = other.view_state.cover_alone();
            let same_row = |page: usize, current: u32| {
                other.view_state.mode() == PageViewMode::TwoPages
                    && crate::viewer::two_page_slot(page, cover).0
                        == crate::viewer::two_page_slot(current as usize, cover).0
            };
            // The page gone to stays current until the pane scrolls.
            if navigated || framed {
                other.picked_page_scroll = Some(other.viewer.logical_scroll());
            } else if other.picked_page_scroll != Some(other.viewer.logical_scroll()) {
                other.picked_page_scroll = None;
            }
            if other.view_state.mode().scrolls()
                && other.picked_page_scroll.is_none()
                && let Some(visible) = plan.current_page
                && !(same_row(visible, other.current_page)
                    && plan.visible_pages.contains(&(other.current_page as usize)))
            {
                other.current_page = visible as u32;
                other.requested_page = visible as u32;
            }
            cx.notify();
            Ok(())
        })?;
        self.dispatch_other_pane_jobs(document_id, cx);
        self.arm_other_pane_quality_timer(document_id, cx);
        cx.notify();
        Ok(())
    }

    /// Sends the other pane's waiting tiles to the worker.
    pub(super) fn dispatch_other_pane_jobs(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        if session.read(cx).split.is_none() || !session.read(cx).can_render_viewer_jobs(document_id)
        {
            return;
        }
        let now = cx.background_executor().now();
        let jobs = session.update(cx, |session, _| {
            session.split.as_mut().map_or_else(Vec::new, |split| {
                split.other.viewer.release_due_promotions(now);
                split.other.viewer.claim_jobs()
            })
        });
        self.spawn_viewer_jobs(document_id, jobs, cx);
    }

    /// Promotes the other pane's tiles (a draft to full, full to detail)
    /// when they are due.
    fn arm_other_pane_quality_timer(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let deadline = self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .split
                .as_ref()
                .and_then(|split| split.other.viewer.next_promotion_deadline())
        });
        let Some(deadline) = deadline else {
            self.split_other_quality_task = None;
            return;
        };
        let executor = cx.background_executor().clone();
        let delay = deadline.saturating_duration_since(executor.now());
        self.split_other_quality_task = Some(cx.spawn(async move |entity, cx| {
            executor.timer(delay).await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.dispatch_other_pane_jobs(document_id, cx);
                workspace.arm_other_pane_quality_timer(document_id, cx);
                cx.notify();
            });
        }));
    }

    /// The other pane's pages as the viewport draws them: tiles and
    /// committed markups, no selection or editing chrome.
    fn other_pane_pages(
        &self,
        document_id: DocumentId,
        session: &NativeDocumentSession,
    ) -> Vec<ViewerPageRender> {
        let Some(split) = &session.split else {
            return Vec::new();
        };
        let viewer = &split.other.viewer;
        let Some(plan) = viewer.plan_snapshot() else {
            return Vec::new();
        };
        plan.page_layouts
            .iter()
            .filter(|layout| plan.visible_pages.contains(&layout.page))
            .filter_map(|layout| {
                let page = layout.page as u32;
                let tiles = viewer.visible_tiles(layout.page);
                let expected_tiles = plan
                    .tiles
                    .iter()
                    .filter(|request| request.page == layout.page)
                    .count();
                Some((
                    *layout,
                    *session.page_sizes.get(layout.page)?,
                    session.annotation_page_geometry(page)?,
                    session.annotation_page_coordinate_space(page)?,
                    session
                        .annotations
                        .thumbnail_scene(document_id.value(), page),
                    !viewer.page_shows_stale_highlights(layout.page)
                        && expected_tiles > 0
                        && tiles.len() == expected_tiles,
                    tiles,
                    None,
                    viewer.page_quality(layout.page),
                    None,
                ))
            })
            .collect()
    }

    /// What the other pane draws of Edit PDF, which is the document's mode
    /// and so shows in both panes: the frames round its pages' paragraphs,
    /// the hovered and selected content, the paragraph being edited (its
    /// frame, selection and caret, but not its input, which only the active
    /// pane holds) and the picture of content being dragged.
    pub(super) fn other_pane_edit_overlays(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> (
        Vec<page_text::PageTextOverlay>,
        Option<super::content::SpritePaint>,
    ) {
        let pages = self.other_pane_visible_pages(document_id, cx);
        let Some(pages) = pages else {
            return (Vec::new(), None);
        };
        let editing_text = self
            .page_text_editing
            .is_some_and(|editing| editing.document_id == document_id);
        let mut overlays = if self.editing_content(document_id, cx) || editing_text {
            self.page_text_overlays_on(document_id, Some(pages), cx)
        } else {
            Vec::new()
        };
        for overlay in &mut overlays {
            overlay.input = None;
        }
        (overlays, self.content_sprite_paint(document_id, cx))
    }

    /// The other pane: its viewport, drawn from its own viewer, and its
    /// scroll bars.
    fn render_other_pane(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (page_text_overlays, content_sprite) = self.other_pane_edit_overlays(document_id, cx);
        let session = self.session(document_id, cx)?.read(cx);
        let other = &session.split.as_ref()?.other;
        let viewer_plan = other.viewer.plan_snapshot().cloned();
        let viewer_scroll = other.viewer.scroll_handle();
        let viewer_pan_margin = other.viewer.pan_margin();
        let viewer_zoom_fallback = viewer_plan
            .iter()
            .flat_map(|plan| plan.visible_pages.iter().copied())
            .map(|page| (page, other.viewer.page_fallback_tiles(page)))
            .filter(|(_, tiles)| !tiles.is_empty())
            .collect::<HashMap<_, _>>();
        let scrollbar = other.viewer.scrollbar_handle();
        let current_page = other.current_page;
        let viewer_pages = self.other_pane_pages(document_id, session);
        let image_assets = Arc::new(session.image_assets.clone());
        let observer_scroll = viewer_scroll.clone();
        let hitbox_scroll = viewer_scroll.clone();
        let control = cx.entity().downgrade();
        let viewport_observer = canvas(
            move |bounds, window, _| {
                let offset = hitbox_scroll.offset();
                window.insert_hitbox(
                    Bounds {
                        origin: bounds.origin - offset,
                        size: bounds.size,
                    },
                    gpui::HitboxBehavior::Normal,
                )
            },
            move |bounds, _, window, cx| {
                let _ = observer_scroll.offset();
                let _ = control.update(cx, |workspace, cx| {
                    workspace.observe_other_pane(
                        document_id,
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                        window,
                        cx,
                    );
                });
            },
        )
        .absolute()
        .inset_0();
        let viewport = self.render_document_viewport(
            ViewportInputs {
                active_selection_marquee: None,
                construction_grid_spacing_mm: None,
                current_image: None,
                current_page,
                document_id,
                document_opening_batch_count: 0,
                focused_annotation_id: None,
                hot_annotation_handle: None,
                hovered_annotation: None,
                image_assets,
                object_snap_tracking_result: None,
                pending_image_preview: None,
                pending_text_box_presentation: None,
                page_text_overlays,
                content_sprite,
                form_overlays: Vec::new(),
                relationship_snap_guides: Vec::new(),
                angle_snap_guide: None,
                selection_color: interaction_chrome::selection_colour(),
                semantic_snap_decision: None,
                calibration_preview: None,
                page_outline: None,
                viewer_pages,
                viewer_zoom_fallback,
                page_turn: None,
                page_markup_opacity: None,
                viewer_pan_margin,
                viewer_plan,
                viewer_scroll,
                viewport_cursor_style: CursorStyle::Arrow,
                viewport_observer,
                other_pane: true,
            },
            cx,
        );
        Some(
            gpui::div()
                .relative()
                .size_full()
                .child(viewport)
                .child(
                    gpui::div().absolute().inset_0().child(
                        gpui_component::scroll::Scrollbar::new(&scrollbar)
                            .id("document-viewport-other-scrollbars")
                            .axis(gpui_component::scroll::ScrollbarAxis::Both),
                    ),
                )
                .into_any_element(),
        )
    }

    /// The canvas body: `active` alone, or in Split View the two panes and
    /// the divider between them, the active pane ringed.
    pub(super) fn render_split_canvas(
        &mut self,
        document_id: DocumentId,
        active: AnyElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((active_side, ratio)) = self.split_view_state(document_id, cx) else {
            return active;
        };
        let Some(other) = self.render_other_pane(document_id, cx) else {
            return active;
        };
        let theme = cx.theme();
        let ring = theme.ring;
        let line = theme.border;
        let divider_focused = self.split_divider_focus.is_focused(window);
        let dragging = self.split_drag == Some(document_id);
        let left_width = self.split_canvas_bounds.get().map(|bounds| {
            split_left_width(ratio, f32::from(bounds.size.width), SPLIT_DIVIDER_WIDTH)
        });
        let ring_element = gpui::div()
            .id(SPLIT_ACTIVE_RING_ID)
            .debug_selector(|| SPLIT_ACTIVE_RING_ID.into())
            .absolute()
            .inset_0()
            .border_2()
            .border_color(ring.opacity(0.7));
        let active = gpui::div()
            .relative()
            .size_full()
            .child(active)
            .child(ring_element)
            .into_any_element();
        let (left, right) = match active_side {
            PaneSide::Left => (active, other),
            PaneSide::Right => (other, active),
        };
        let other_side = active_side.other();
        let pane = |id: &'static str, side: PaneSide, content: AnyElement| {
            gpui::div()
                .id(id)
                .debug_selector(move || id.into())
                .relative()
                .h_full()
                .min_w_0()
                .overflow_hidden()
                .when(side == PaneSide::Left, |pane| match left_width {
                    Some(width) => pane.flex_none().w(px(width)),
                    None => pane.flex_none().w(gpui::relative(ratio)),
                })
                .when(side == PaneSide::Right, |pane| pane.flex_1())
                .when(side == other_side, |pane| {
                    // A press in the other pane makes it active and does
                    // nothing else.
                    pane.capture_any_mouse_down(cx.listener(
                        move |workspace, _: &MouseDownEvent, window, cx| {
                            if crate::overlay_state::press_owned_by_overlay(window, cx) {
                                return;
                            }
                            workspace.activate_other_pane(document_id, window, cx);
                            window.prevent_default();
                            cx.stop_propagation();
                        },
                    ))
                })
                .child(content)
        };
        let bounds_cell = self.split_canvas_bounds.clone();
        let divider = gpui::div()
            .id(SPLIT_DIVIDER_ID)
            .debug_selector(|| SPLIT_DIVIDER_ID.into())
            .key_context(SPLIT_DIVIDER_CONTEXT)
            .track_focus(&self.split_divider_focus)
            .tab_index(0)
            .role(gpui::Role::Splitter)
            .aria_label("Split View divider")
            .flex_none()
            .w(px(SPLIT_DIVIDER_WIDTH))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .bg(theme.secondary)
            // The focus ring, round the whole divider, as Tab brings it.
            .when(divider_focused, |divider| {
                divider.border_2().border_color(ring).rounded(theme.radius)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |workspace, event: &MouseDownEvent, window, cx| {
                    if event.click_count == 2 {
                        workspace.set_split_ratio(document_id, 0.5, cx);
                        return;
                    }
                    workspace.split_drag = Some(document_id);
                    workspace.split_divider_focus.focus(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_action(cx.listener(|workspace, _: &SplitDividerLeft, _, cx| {
                workspace.move_split_divider(Some(-DIVIDER_KEY_STEP), None, cx);
            }))
            .on_action(cx.listener(|workspace, _: &SplitDividerRight, _, cx| {
                workspace.move_split_divider(Some(DIVIDER_KEY_STEP), None, cx);
            }))
            .on_action(cx.listener(|workspace, _: &SplitDividerHome, _, cx| {
                workspace.move_split_divider(None, Some(0.), cx);
            }))
            .on_action(cx.listener(|workspace, _: &SplitDividerEnd, _, cx| {
                workspace.move_split_divider(None, Some(1.), cx);
            }))
            .on_action(cx.listener(|workspace, _: &SplitDividerReset, _, cx| {
                workspace.move_split_divider(None, Some(0.5), cx);
            }))
            .child(gpui::div().h_full().map(|line_element| {
                if divider_focused || dragging {
                    line_element.w(px(2.)).bg(ring)
                } else {
                    line_element.w(px(1.)).bg(line)
                }
            }));
        h_flex()
            .id(SPLIT_CANVAS_ID)
            .debug_selector(|| SPLIT_CANVAS_ID.into())
            .size_full()
            .on_prepaint(move |bounds, _, _| bounds_cell.set(Some(bounds)))
            .child(pane(SPLIT_PANE_LEFT_ID, PaneSide::Left, left))
            .child(divider)
            .child(pane(SPLIT_PANE_RIGHT_ID, PaneSide::Right, right))
            // Always listening, so a drag follows from its first move.
            .child(self.split_drag_listeners(document_id, cx))
            .into_any_element()
    }
}

impl DocumentWorkspace {
    /// While the divider is dragged, the pointer moves it wherever it is
    /// in the window, and the release anywhere ends the drag.
    fn split_drag_listeners(
        &self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> gpui::Canvas<()> {
        let move_control = cx.entity().downgrade();
        let up_control = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = move_control.update(cx, |workspace, cx| {
                        if workspace.split_drag != Some(document_id) {
                            return;
                        }
                        if event.pressed_button != Some(MouseButton::Left) {
                            workspace.split_drag = None;
                            cx.notify();
                            return;
                        }
                        let Some(bounds) = workspace.split_canvas_bounds.get() else {
                            return;
                        };
                        let room = f32::from(bounds.size.width) - SPLIT_DIVIDER_WIDTH;
                        if room <= 0. {
                            return;
                        }
                        let x = f32::from(event.position.x - bounds.origin.x)
                            - SPLIT_DIVIDER_WIDTH / 2.;
                        workspace.set_split_ratio(document_id, x / room, cx);
                        cx.stop_propagation();
                    });
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = up_control.update(cx, |workspace, cx| {
                        if workspace.split_drag.take().is_some() {
                            cx.notify();
                        }
                    });
                });
            },
        )
        .absolute()
        .size_0()
    }
}

/// The size a fit fits in a pane showing `view`: the page, or in Two Pages
/// view the row holding it.
pub(super) fn pane_fit_size(
    page_sizes: &[(f32, f32)],
    view: &NativeDocumentViewState,
    page: u32,
) -> Option<(f32, f32)> {
    if view.mode() == PageViewMode::TwoPages {
        crate::viewer::two_page_row_size(
            page_sizes,
            page as usize,
            view.cover_alone(),
            crate::document_viewer::PAGE_GAP,
        )
    } else {
        page_sizes.get(page as usize).copied()
    }
}
