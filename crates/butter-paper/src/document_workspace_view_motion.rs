//! Zoom, Fit Width and Fit Page move the view to its new zoom over a short
//! motion instead of in one frame.
//!
//! Each frame sets an in-between zoom and the page point at the view centre,
//! through the same anchored-zoom path that pinch zoom uses. That path keeps
//! the pages drawn from the tiles they have while new ones render. When the
//! motion ends, the command itself runs, so the final view is exactly the one
//! the command gives without motion. With reduce motion set, the command runs
//! at once.

use super::*;
use crate::zoom_anchor::ZoomAnchor;

/// The view's motion: the stock Sidebar's length, with the dropdowns'
/// ease-out curve, so the view settles into place.
pub(super) const VIEW_MOTION_DURATION: Duration = Duration::from_millis(200);

/// The command a view motion carries out when it ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ViewMotionEnd {
    Zoom {
        mode: PageViewMode,
        zoom_percent: f32,
    },
    Fit(ViewerFitPreset),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ViewMotion {
    document_id: DocumentId,
    page: usize,
    from_zoom: f32,
    to_zoom: f32,
    from_fraction: (f32, f32),
    to_fraction: (f32, f32),
    started: Instant,
    end: ViewMotionEnd,
}

impl ViewMotion {
    /// The zoom and the page point at the view centre, `progress` of the way.
    fn sample(&self, progress: f32) -> (f32, (f32, f32)) {
        let eased = gpui_component::animation::ease_out_cubic(progress.clamp(0., 1.));
        // Zoom moves by equal ratios, so zooming in and out feel alike.
        let zoom = self.from_zoom * (self.to_zoom / self.from_zoom).powf(eased);
        let fraction = (
            self.from_fraction.0 + (self.to_fraction.0 - self.from_fraction.0) * eased,
            self.from_fraction.1 + (self.to_fraction.1 - self.from_fraction.1) * eased,
        );
        (zoom, fraction)
    }
}

impl DocumentWorkspace {
    /// Starts moving the view of `document_id` toward `end`. Returns false
    /// when the command must run at once instead: reduce motion is set, the
    /// view has no layout yet, the page mode changes, or nothing would move.
    pub(super) fn begin_view_motion(
        &mut self,
        document_id: DocumentId,
        end: ViewMotionEnd,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.finishing_view_motion || cx.reduce_motion() {
            return false;
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let motion = {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return false;
            }
            let Some(plan) = session.viewer.plan_snapshot() else {
                return false;
            };
            let Some((width, height)) = session.view_state.viewport_size() else {
                return false;
            };
            let from_zoom = session.view_state.zoom_percent();
            let scroll = session.viewer.logical_scroll();
            let centre = (scroll.0 + width / 2., scroll.1 + height / 2.);
            let fraction_on = |page: usize| {
                let rect = plan
                    .page_layouts
                    .iter()
                    .find(|layout| layout.page == page)?
                    .logical_rect;
                (rect.width > 0. && rect.height > 0.).then(|| {
                    (
                        (centre.0 - rect.x) / rect.width,
                        (centre.1 - rect.y) / rect.height,
                    )
                })
            };
            let (page, from_fraction, to_fraction, to_zoom) = match end {
                ViewMotionEnd::Zoom { mode, zoom_percent } => {
                    if mode != session.view_state.mode() {
                        return false;
                    }
                    // A zoom command keeps the page point at the view centre.
                    let Some(anchor) = session.viewport_centre_anchor() else {
                        return false;
                    };
                    (
                        anchor.page,
                        anchor.page_fraction,
                        anchor.page_fraction,
                        crate::native_document_view_state::clamp_zoom_percent(zoom_percent),
                    )
                }
                ViewMotionEnd::Fit(preset) => {
                    let page = session.current_page as usize;
                    let Some(page_size) = session.page_sizes.get(page).copied() else {
                        return false;
                    };
                    let Some(from_fraction) = fraction_on(page) else {
                        return false;
                    };
                    let mut fitted = session.view_state.clone();
                    fitted.set_fit_preset(preset);
                    fitted.update_viewport(width, height, page_size);
                    // A fit centres the page across, and Fit Page also down;
                    // Fit Width keeps the height at the view centre.
                    let to_fraction = (
                        0.5,
                        if preset == ViewerFitPreset::Page {
                            0.5
                        } else {
                            from_fraction.1
                        },
                    );
                    (page, from_fraction, to_fraction, fitted.zoom_percent())
                }
            };
            let still = (to_zoom - from_zoom).abs() < 0.01
                && (to_fraction.0 - from_fraction.0).abs() < 0.0001
                && (to_fraction.1 - from_fraction.1).abs() < 0.0001;
            if still || from_zoom <= 0. || to_zoom <= 0. {
                return false;
            }
            ViewMotion {
                document_id,
                page,
                from_zoom,
                to_zoom,
                from_fraction,
                to_fraction,
                started: cx.background_executor().now(),
                end,
            }
        };
        self.view_motion = Some(motion);
        cx.notify();
        true
    }

    /// Stops a view motion where it is, for input that moves the view itself.
    pub(super) fn cancel_view_motion(&mut self) {
        self.view_motion = None;
    }

    /// Moves the view one frame on. Called while rendering; asks for the next
    /// frame until the motion ends, then carries out its command.
    pub(super) fn step_view_motion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(motion) = self.view_motion else {
            return;
        };
        let now = cx.background_executor().now();
        let progress = now.saturating_duration_since(motion.started).as_secs_f32()
            / VIEW_MOTION_DURATION.as_secs_f32();
        let active = self.active_document_id == Some(motion.document_id);
        if progress >= 1. || !active || cx.reduce_motion() {
            self.view_motion = None;
            self.finishing_view_motion = true;
            match motion.end {
                ViewMotionEnd::Zoom { mode, zoom_percent } => {
                    self.set_view_configuration(motion.document_id, mode, zoom_percent, cx);
                }
                ViewMotionEnd::Fit(preset) => {
                    self.set_fit_preset(motion.document_id, preset, cx);
                }
            }
            self.finishing_view_motion = false;
            return;
        }
        let Some(session) = self.session(motion.document_id, cx).cloned() else {
            self.view_motion = None;
            return;
        };
        let (zoom, fraction) = motion.sample(progress);
        let Some((width, height)) = session.read(cx).view_state.viewport_size() else {
            self.view_motion = None;
            return;
        };
        session.update(cx, |session, cx| {
            session.view_state.set_manual_zoom(zoom);
            session.pending_zoom_anchor = Some(ZoomAnchor {
                page: motion.page,
                page_fraction: fraction,
                viewport_point: (width / 2., height / 2.),
            });
            session
                .viewer
                .configure(session.view_state.mode(), session.view_state.zoom_percent());
            cx.notify();
        });
        let _ = self.refresh_viewport_async(
            motion.document_id,
            width,
            height,
            window.scale_factor(),
            cx,
        );
        window.request_animation_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_motion_moves_zoom_by_ratio_and_the_centre_point_in_a_line() {
        let motion = ViewMotion {
            document_id: DocumentId::new(1),
            page: 0,
            from_zoom: 50.,
            to_zoom: 200.,
            from_fraction: (0.2, 0.4),
            to_fraction: (0.5, 0.4),
            started: Instant::now(),
            end: ViewMotionEnd::Fit(ViewerFitPreset::Width),
        };
        assert_eq!(motion.sample(0.), (50., (0.2, 0.4)));
        let (zoom, (x, y)) = motion.sample(1.);
        assert!((zoom - 200.).abs() < 0.001 && (x - 0.5).abs() < 0.0001 && y == 0.4);
        // Half-way along the eased curve, the zoom is the geometric mean of
        // the ends at the same point as the centre is along its line.
        let eased = gpui_component::animation::ease_out_cubic(0.5);
        let (zoom, (x, _)) = motion.sample(0.5);
        assert!((zoom - 50. * 4f32.powf(eased)).abs() < 0.01);
        assert!((x - (0.2 + 0.3 * eased)).abs() < 0.0001);
    }
}
