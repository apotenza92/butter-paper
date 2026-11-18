//! Rotating the page in view turns its picture a quarter turn into place.
//!
//! The page's layout takes its new orientation at once, as before; only the
//! picture moves. The tiles the page keeps across a rotation are already
//! turned to the new orientation (see `invalidate_raster`), so the turn draws
//! them rotated back by what is left of the quarter turn. GPUI cannot rotate
//! the markup layer's paths and text, so the page's markups are hidden while
//! the picture turns and fade in once it lands. With reduce motion set, the
//! page shows its new orientation at once.

use super::*;

/// The quarter turn: the stock Sidebar's length, easing in and out.
pub(super) const PAGE_TURN_DURATION: Duration = Duration::from_millis(200);
/// The markups' return once the page has landed.
pub(super) const MARKUP_RETURN_DURATION: Duration = Duration::from_millis(150);
/// A new selection's outline and handles fade in over this.
pub(super) const SELECTION_FADE_DURATION: Duration = Duration::from_millis(100);
/// Snap guides fade in and out over this.
pub(super) const SNAP_GUIDE_FADE_DURATION: Duration = Duration::from_millis(80);
/// Moved thumbnails and tabs slide to their new places over this, with the
/// stock Sidebar's curve.
pub(super) const REORDER_SLIDE_DURATION: Duration = Duration::from_millis(200);

/// How much of a reorder slide is still to go, from `started`: 1 at the
/// start, 0 when done.
pub(super) fn slide_remaining(started: Instant, now: Instant) -> f32 {
    let progress =
        now.saturating_duration_since(started).as_secs_f32() / REORDER_SLIDE_DURATION.as_secs_f32();
    if progress >= 1. {
        0.
    } else {
        1. - gpui_component::animation::ease_in_out_cubic(progress)
    }
}

/// The Snapshot tool's copied box flashes white and fades over this.
pub(super) const SNAPSHOT_FLASH_DURATION: Duration = Duration::from_millis(250);

/// The snap indicator, tracking guides and relationship guides of a frame.
pub(super) type SnapGuides = (
    Option<SemanticSnapDecision>,
    Option<crate::semantic_snapping::ObjectSnapTrackingResult>,
    Vec<RelationshipSnapGuide>,
);

/// Snap guides fading in since a time, or the last ones shown fading out.
pub(super) enum SnapFade {
    In {
        document_id: DocumentId,
        since: Instant,
        last: SnapGuides,
    },
    Out {
        document_id: DocumentId,
        since: Instant,
        last: SnapGuides,
    },
}

#[derive(Clone)]
pub(super) struct PageTurn {
    document_id: DocumentId,
    page: usize,
    clockwise: bool,
    /// Set on the first frame with the page's turned tiles in hand.
    started: Option<Instant>,
    tiles: Vec<(crate::viewer::Rect, Arc<RenderImage>)>,
    /// Where the page was laid out before it turned.
    old_rect: Option<crate::viewer::Rect>,
    /// The old centre's offset from the new one, found on the first frame.
    start_offset: (f32, f32),
}

/// What the viewport draws for the turning page this frame.
#[derive(Clone)]
pub struct PageTurnPaint {
    pub page: usize,
    /// Clockwise radians the page's picture is drawn turned by.
    pub angle: f32,
    /// How far the turn's centre is from the page's centre: the turn starts
    /// where the page was and ends in its new place.
    pub offset: (f32, f32),
    pub tiles: Vec<(crate::viewer::Rect, Arc<RenderImage>)>,
}

impl DocumentWorkspace {
    /// Turns the picture of `page` into its new orientation over a short
    /// motion, unless reduce motion is set.
    pub(super) fn begin_page_turn(
        &mut self,
        document_id: DocumentId,
        page: usize,
        clockwise: bool,
        old_rect: Option<crate::viewer::Rect>,
        cx: &App,
    ) {
        if cx.reduce_motion() {
            self.page_turn = None;
            return;
        }
        self.page_turn = Some(PageTurn {
            document_id,
            page,
            clockwise,
            started: None,
            tiles: Vec::new(),
            old_rect,
            start_offset: (0., 0.),
        });
        self.markups_return = None;
    }

    /// The turning page's picture for this frame, given that page's kept
    /// tiles. Starts the turn on the first frame that has them; a page with
    /// none to turn shows its new orientation at once.
    pub(super) fn page_turn_paint(
        &mut self,
        document_id: DocumentId,
        kept_tiles: impl FnOnce(
            usize,
        ) -> (
            Vec<(crate::viewer::Rect, Arc<RenderImage>)>,
            Option<crate::viewer::Rect>,
        ),
        now: Instant,
        window: &mut Window,
    ) -> Option<PageTurnPaint> {
        let turn = self.page_turn.as_mut()?;
        if turn.document_id != document_id {
            return None;
        }
        if turn.started.is_none() {
            let (tiles, new_rect) = kept_tiles(turn.page);
            if tiles.is_empty() {
                self.page_turn = None;
                return None;
            }
            turn.tiles = tiles;
            if let (Some(old), Some(new)) = (turn.old_rect, new_rect) {
                turn.start_offset = (
                    (old.x + old.width / 2.) - (new.x + new.width / 2.),
                    (old.y + old.height / 2.) - (new.y + new.height / 2.),
                );
            }
            turn.started = Some(now);
        }
        let started = turn.started?;
        let progress =
            now.saturating_duration_since(started).as_secs_f32() / PAGE_TURN_DURATION.as_secs_f32();
        if progress >= 1. {
            let page = turn.page;
            self.page_turn = None;
            self.markups_return = Some((document_id, page, now));
            window.request_animation_frame();
            return None;
        }
        window.request_animation_frame();
        let remaining = 1. - gpui_component::animation::ease_in_out_cubic(progress);
        let quarter = std::f32::consts::FRAC_PI_2;
        Some(PageTurnPaint {
            page: turn.page,
            angle: if turn.clockwise { -quarter } else { quarter } * remaining,
            offset: (
                turn.start_offset.0 * remaining,
                turn.start_offset.1 * remaining,
            ),
            tiles: turn.tiles.clone(),
        })
    }

    /// Fades in the outline and handles of a new selection. They respond to
    /// the pointer from the first frame; only their paint fades.
    pub(super) fn step_selection_fade(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            interaction_chrome::set_selection_fade(1.);
            return;
        };
        let selected = self.selected_annotation_ids(document_id, cx);
        let now = cx.background_executor().now();
        let changed = self
            .selection_shown
            .as_ref()
            .is_none_or(|(shown_document, ids, _)| {
                *shown_document != document_id || *ids != selected
            });
        if changed {
            self.selection_shown = Some((document_id, selected.clone(), now));
        }
        let started = self.selection_shown.as_ref().map_or(now, |(_, _, at)| *at);
        let progress = now.saturating_duration_since(started).as_secs_f32()
            / SELECTION_FADE_DURATION.as_secs_f32();
        let fade = if cx.reduce_motion() || selected.is_empty() || progress >= 1. {
            1.
        } else {
            window.request_animation_frame();
            gpui_component::animation::ease_out_cubic(progress)
        };
        interaction_chrome::set_selection_fade(fade);
    }

    /// The snap guides to draw this frame and how far they have faded in
    /// (`snap_guide_fade`). Guides fade in when a snap starts, and the last
    /// ones fade out after it ends. They are exact while shown in full.
    pub(super) fn fade_snap_guides(
        &mut self,
        document_id: DocumentId,
        guides: SnapGuides,
        now: Instant,
        window: &mut Window,
        cx: &App,
    ) -> SnapGuides {
        let shown = guides.0.is_some() || guides.1.is_some() || !guides.2.is_empty();
        let progress = |since: Instant| {
            now.saturating_duration_since(since).as_secs_f32()
                / SNAP_GUIDE_FADE_DURATION.as_secs_f32()
        };
        if cx.reduce_motion() {
            self.snap_fade = None;
            self.snap_guide_fade = 1.;
            return guides;
        }
        if shown {
            let since = match self.snap_fade.take() {
                Some(SnapFade::In {
                    document_id: shown_in,
                    since,
                    ..
                }) if shown_in == document_id => since,
                _ => now,
            };
            let fade = progress(since);
            self.snap_guide_fade = if fade >= 1. {
                1.
            } else {
                window.request_animation_frame();
                gpui_component::animation::ease_out_cubic(fade)
            };
            self.snap_fade = Some(SnapFade::In {
                document_id,
                since,
                last: guides.clone(),
            });
            return guides;
        }
        let (since, last) = match self.snap_fade.take() {
            Some(SnapFade::In {
                document_id: shown_in,
                last,
                ..
            }) if shown_in == document_id => (now, last),
            Some(SnapFade::Out {
                document_id: shown_in,
                since,
                last,
            }) if shown_in == document_id => (since, last),
            _ => {
                self.snap_guide_fade = 1.;
                return guides;
            }
        };
        let fade = progress(since);
        if fade >= 1. {
            self.snap_guide_fade = 1.;
            return guides;
        }
        window.request_animation_frame();
        self.snap_guide_fade = 1. - gpui_component::animation::ease_out_cubic(fade);
        self.snap_fade = Some(SnapFade::Out {
            document_id,
            since,
            last: last.clone(),
        });
        last
    }

    /// After the tabs were reordered from `old_order`, slides each tab from
    /// where it was to its new place. `dropped` is a tab dropped by the
    /// pointer, which takes its new place at once, where it was let go.
    pub(super) fn begin_tab_slide(
        &mut self,
        old_order: &[DocumentId],
        dropped: Option<DocumentId>,
        cx: &App,
    ) {
        if cx.reduce_motion() {
            self.tab_slide = None;
            return;
        }
        let measured = |id: &DocumentId| {
            self.session_tab_bounds
                .get(id)
                .map(|bounds| bounds.get())
                .filter(|bounds| bounds.size.width > px(0.))
        };
        let Some(old_bounds) = old_order.iter().map(measured).collect::<Option<Vec<_>>>() else {
            return;
        };
        let (Some(first), Some(second)) = (old_bounds.first(), old_bounds.get(1)) else {
            return;
        };
        let gap = f32::from(second.left() - first.right());
        let old_left = old_order
            .iter()
            .zip(&old_bounds)
            .map(|(id, bounds)| {
                (
                    *id,
                    (f32::from(bounds.left()), f32::from(bounds.size.width)),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut left = f32::from(first.left());
        let mut offsets = HashMap::new();
        for id in self.session_order(cx) {
            let Some(&(was, width)) = old_left.get(&id) else {
                return;
            };
            if Some(id) != dropped && (was - left).abs() > 0.5 {
                offsets.insert(id, was - left);
            }
            left += width + gap;
        }
        self.tab_slide = (!offsets.is_empty()).then(|| (offsets, cx.background_executor().now()));
    }

    /// How far `document_id`'s tab is drawn from its place this frame.
    pub(super) fn tab_slide_offset(&self, document_id: DocumentId, now: Instant) -> Option<f32> {
        let (offsets, started) = self.tab_slide.as_ref()?;
        let remaining = slide_remaining(*started, now);
        (remaining > 0.)
            .then(|| offsets.get(&document_id).map(|offset| offset * remaining))
            .flatten()
    }

    /// Flashes the box the Snapshot tool copied, as a camera does.
    pub(super) fn begin_snapshot_flash(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        rect: PdfRect,
        cx: &App,
    ) {
        if !cx.reduce_motion() {
            self.snapshot_flash = Some((
                document_id,
                page_index,
                rect,
                cx.background_executor().now(),
            ));
        }
    }

    /// The copied box's flash this frame: its page, box and strength.
    pub(super) fn step_snapshot_flash(
        &mut self,
        window: &mut Window,
        cx: &App,
    ) -> Option<(DocumentId, u32, PdfRect, f32)> {
        let (document_id, page_index, rect, started) = self.snapshot_flash?;
        let progress = cx
            .background_executor()
            .now()
            .saturating_duration_since(started)
            .as_secs_f32()
            / SNAPSHOT_FLASH_DURATION.as_secs_f32();
        if progress >= 1. || cx.reduce_motion() {
            self.snapshot_flash = None;
            return None;
        }
        window.request_animation_frame();
        Some((
            document_id,
            page_index,
            rect,
            1. - gpui_component::animation::ease_out_cubic(progress),
        ))
    }

    /// How opaque `page`'s markups are drawn: hidden while the page turns,
    /// then fading in after it lands.
    pub(super) fn page_markup_opacity(
        &mut self,
        document_id: DocumentId,
        now: Instant,
        window: &mut Window,
    ) -> Option<(usize, f32)> {
        if let Some(turn) = self
            .page_turn
            .as_ref()
            .filter(|turn| turn.document_id == document_id)
        {
            return Some((turn.page, 0.));
        }
        let (returning_document, page, landed) = self.markups_return?;
        if returning_document != document_id {
            return None;
        }
        let progress = now.saturating_duration_since(landed).as_secs_f32()
            / MARKUP_RETURN_DURATION.as_secs_f32();
        if progress >= 1. {
            self.markups_return = None;
            return None;
        }
        window.request_animation_frame();
        Some((page, gpui_component::animation::ease_out_cubic(progress)))
    }
}

thread_local! {
    /// One white pixel, stretched and turned to draw the turning page's
    /// paper: GPUI turns images but not filled rectangles.
    static PAPER: Arc<RenderImage> = {
        let buffer = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
            1,
            1,
            vec![255, 255, 255, 255],
        )
        .expect("one pixel fits one pixel's bytes");
        Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(buffer)]))
    };
}

/// Draws the turning page: its paper and kept tiles, turned by `paint.angle`
/// about the page's centre. `page_size` is the page's laid-out size.
pub(super) fn page_turn_element(paint: PageTurnPaint) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            // Turned about the page's centre, which moves from where the
            // page was to where it is now.
            let scale = window.scale_factor();
            let centre = bounds.center();
            let pivot = point(
                gpui::ScaledPixels((f32::from(centre.x) + paint.offset.0) * scale),
                gpui::ScaledPixels((f32::from(centre.y) + paint.offset.1) * scale),
            );
            let back = point(
                gpui::ScaledPixels(-f32::from(centre.x) * scale),
                gpui::ScaledPixels(-f32::from(centre.y) * scale),
            );
            let turn = gpui::TransformationMatrix::unit()
                .translate(pivot)
                .rotate(gpui::Radians(paint.angle))
                .translate(back);
            let paper = PAPER.with(Arc::clone);
            let _ = window.paint_image_transformed(
                bounds,
                bounds,
                Default::default(),
                paper,
                0,
                false,
                turn,
            );
            for (rect, image) in &paint.tiles {
                let tile = Bounds::new(
                    point(bounds.origin.x + px(rect.x), bounds.origin.y + px(rect.y)),
                    size(px(rect.width), px(rect.height)),
                );
                // The image's border of repeated edge pixels lies outside
                // the tile and is clipped away, as the viewport draws it.
                let pixels = image.size(0);
                let pad = crate::document_resource::TILE_IMAGE_PADDING as f32;
                let border = |extent: f32, pixels: i32| {
                    let inner = pixels as f32 - 2. * pad;
                    if inner > 0. { extent * pad / inner } else { 0. }
                };
                let border_x = px(border(rect.width, pixels.width.0));
                let border_y = px(border(rect.height, pixels.height.0));
                let image_bounds = Bounds::new(
                    point(tile.origin.x - border_x, tile.origin.y - border_y),
                    size(
                        tile.size.width + border_x * 2.,
                        tile.size.height + border_y * 2.,
                    ),
                );
                let _ = window.paint_image_transformed(
                    tile,
                    image_bounds,
                    Default::default(),
                    image.clone(),
                    0,
                    false,
                    turn,
                );
            }
        },
    )
    .absolute()
    .inset_0()
}
