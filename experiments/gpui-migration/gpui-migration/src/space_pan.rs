//! Hold-Space temporary pan and double-tap permanent toggle (Phase 4, Gap 2).
//!
//! Reference contract (`App.tsx:1339-1406`, `utils/toolShortcuts.ts`):
//! Space keydown on a non-interactive target (key repeat ignored) stashes the
//! current tool and activates pan; keyup restores the stash. Two keydowns
//! within 300ms toggle pan/select permanently and clear the stash. An explicit
//! tool change clears the stash.
//!
//! Parity decisions recorded here:
//! - Focus loss / window deactivation: REPLICATED, not fixed. The reference
//!   has no blur handler, so held-pan survives deactivation there too. The
//!   stuck state self-heals: the next Space keyup still restores the stash,
//!   and any explicit tool change clears it first, so no stale tool can be
//!   restored over a newer choice.
//! - Modifier guard: the reference `mod` early-return runs before Space
//!   handling, so platform/ctrl+Space never pans. Shift/alt+Space pans in the
//!   reference and pans here too.
//! - Middle-button pan is NOT part of this slice; the canvas still accepts
//!   left-button pan only. Cursor policy (grab affordance) stays with Gap 10.
use std::time::{Duration, Instant};

use crate::annotation_adapter::AnnotationTool;

/// Two Space keydowns within this window toggle pan/select permanently.
pub(super) const SPACE_PAN_DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);

/// Outcome of one non-repeat Space keydown. The caller owns tool changes; the
/// stash is recorded only after the tool funnel runs, because the funnel
/// clears it as an explicit change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpacePanDown {
    Ignored,
    BeginHold,
    TogglePermanent,
}

/// Hold-Space state: the stashed tool plus the last release time used for the
/// 300ms double-tap window. Time is caller-supplied so tests stay
/// deterministic without sleeping.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SpacePanHold {
    stash: Option<AnnotationTool>,
    last_release: Option<Instant>,
}

impl SpacePanHold {
    pub(super) fn key_down(
        &mut self,
        now: Instant,
        pan_active: bool,
        repeat: bool,
    ) -> SpacePanDown {
        if repeat {
            return SpacePanDown::Ignored;
        }
        if self.last_release.is_some_and(|release| {
            now.saturating_duration_since(release) <= SPACE_PAN_DOUBLE_TAP_WINDOW
        }) {
            self.last_release = None;
            self.stash = None;
            return SpacePanDown::TogglePermanent;
        }
        if !pan_active && self.stash.is_none() {
            return SpacePanDown::BeginHold;
        }
        SpacePanDown::Ignored
    }

    /// Records every release for double-tap detection and returns the stash,
    /// if any, for the caller to restore. Mirrors the reference keyup, which
    /// stamps the tap time before checking the stash.
    pub(super) fn key_up(&mut self, now: Instant) -> Option<AnnotationTool> {
        self.last_release = Some(now);
        self.stash.take()
    }

    pub(super) fn set_stash(&mut self, tool: AnnotationTool) {
        self.stash = Some(tool);
    }

    pub(super) fn clear_stash(&mut self) {
        self.stash = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hold() -> (SpacePanHold, Instant) {
        (SpacePanHold::default(), Instant::now())
    }

    #[test]
    fn hold_stashes_and_keyup_restores() {
        let (mut hold, now) = hold();
        assert_eq!(hold.key_down(now, false, false), SpacePanDown::BeginHold);
        hold.set_stash(AnnotationTool::Line);
        assert_eq!(hold.stash, Some(AnnotationTool::Line));
        assert_eq!(hold.key_up(now), Some(AnnotationTool::Line));
        assert_eq!(hold.stash, None);
    }

    #[test]
    fn key_repeat_never_starts_or_toggles() {
        let (mut hold, now) = hold();
        assert_eq!(hold.key_down(now, false, true), SpacePanDown::Ignored);
        assert_eq!(hold.stash, None);
        hold.set_stash(AnnotationTool::Rectangle);
        assert_eq!(hold.key_down(now, false, true), SpacePanDown::Ignored);
        assert_eq!(hold.stash, Some(AnnotationTool::Rectangle));
    }

    #[test]
    fn second_tap_within_300ms_toggles_and_clears() {
        let (mut hold, first_down) = hold();
        assert_eq!(
            hold.key_down(first_down, false, false),
            SpacePanDown::BeginHold
        );
        hold.set_stash(AnnotationTool::Line);
        let first_up = first_down + Duration::from_millis(60);
        assert_eq!(hold.key_up(first_up), Some(AnnotationTool::Line));
        let second_down = first_up + Duration::from_millis(299);
        assert_eq!(
            hold.key_down(second_down, false, false),
            SpacePanDown::TogglePermanent
        );
        assert_eq!(hold.stash, None);
        // The toggle consumes the tap stamp, so the next press holds again.
        let third_down = second_down + Duration::from_millis(10);
        assert_eq!(
            hold.key_down(third_down, false, false),
            SpacePanDown::BeginHold
        );
    }

    #[test]
    fn tap_after_the_window_holds_instead_of_toggling() {
        let (mut hold, first_down) = hold();
        assert_eq!(
            hold.key_down(first_down, false, false),
            SpacePanDown::BeginHold
        );
        let first_up = first_down + Duration::from_millis(60);
        assert_eq!(hold.key_up(first_up), None);
        let second_down = first_up + SPACE_PAN_DOUBLE_TAP_WINDOW + Duration::from_millis(1);
        assert_eq!(
            hold.key_down(second_down, false, false),
            SpacePanDown::BeginHold
        );
    }

    #[test]
    fn hold_is_ignored_while_pan_is_already_active() {
        let (mut hold, now) = hold();
        assert_eq!(hold.key_down(now, true, false), SpacePanDown::Ignored);
        assert_eq!(hold.stash, None);
    }

    #[test]
    fn second_down_keeps_the_original_stash() {
        let (mut hold, now) = hold();
        assert_eq!(hold.key_down(now, false, false), SpacePanDown::BeginHold);
        hold.set_stash(AnnotationTool::Arrow);
        // A duplicate keydown without an intervening keyup must not overwrite
        // the stash, matching the reference `stash === null` guard.
        assert_eq!(hold.key_down(now, false, false), SpacePanDown::Ignored);
        assert_eq!(hold.stash, Some(AnnotationTool::Arrow));
    }

    #[test]
    fn explicit_tool_change_clears_the_stash() {
        let (mut hold, now) = hold();
        assert_eq!(hold.key_down(now, false, false), SpacePanDown::BeginHold);
        hold.set_stash(AnnotationTool::Pen);
        hold.clear_stash();
        assert_eq!(hold.key_up(now), None);
    }

    #[test]
    fn double_tap_clears_a_stale_stash() {
        let (mut hold, now) = hold();
        hold.set_stash(AnnotationTool::Highlight);
        hold.key_up(now);
        assert_eq!(
            hold.key_down(now + Duration::from_millis(100), false, false),
            SpacePanDown::TogglePermanent
        );
        assert_eq!(hold.stash, None);
    }
}
