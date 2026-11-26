//! The mouse wheel and the trackpad on a properties panel's numbers and
//! sliders: a scroll over a number or a slider steps its value, one step for
//! a notch, with the markup shown as it changes. A burst of steps is one
//! undoable change, committed when the wheel rests.
//!
//! A scroll that starts over the panel's empty space or labels belongs to the
//! panel until it rests, so a field that passes under the pointer does not
//! take it. A scroll that starts over a field belongs to the field, and the
//! panel does not move.

use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration, time::Instant};

use gpui::{
    App, Div, ElementId, Entity, EntityId, Global, InteractiveElement as _, ScrollDelta,
    ScrollWheelEvent, Stateful, StatefulInteractiveElement as _, Window,
};
use gpui_component::{
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
};

use crate::mouse_wheel::notch_lines;
use crate::property_controls::format_property_number;

/// A scroll gesture ends after this long without a wheel event. The
/// steps of one gesture are one undo step.
pub const WHEEL_IDLE: Duration = Duration::from_millis(300);

/// The pixels of a trackpad's scroll that make one step.
pub const TRACKPAD_STEP_PX: f32 = 24.;

/// Steps for one wheel notch with Shift held: the coarse modifier on every
/// platform.
pub const COARSE_STEPS: i32 = 10;

/// What a number field steps by, and where it stops.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NumberSpec {
    step: f64,
    min: f64,
    max: f64,
}

/// The wheel's steps for the number fields that have `register_number_wheel`.
#[derive(Default)]
struct NumberSpecs(HashMap<EntityId, NumberSpec>);

impl Global for NumberSpecs {}

/// Lets the wheel step the number field `input` by `step` between `min` and
/// `max`. The field's owner reads each step as a typed value: a `Change`
/// while the wheel turns and a `PressEnter` when it rests.
pub fn register_number_wheel(
    input: &Entity<InputState>,
    step: f64,
    min: f64,
    max: f64,
    cx: &mut App,
) {
    cx.default_global::<NumberSpecs>()
        .0
        .insert(input.entity_id(), NumberSpec { step, min, max });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Owner {
    /// The panel, or no field: the scroll belongs to the panel's scrolling.
    #[default]
    Panel,
    /// The field or slider with this entity.
    Control(EntityId),
}

/// Who the scroll gesture in progress belongs to.
#[derive(Default)]
struct Gesture {
    owner: Owner,
    last: Option<Instant>,
}

impl Global for Gesture {}

fn active(gesture: &Gesture, now: Instant) -> bool {
    gesture
        .last
        .is_some_and(|last| now.saturating_duration_since(last) < WHEEL_IDLE)
}

/// A wheel event over a panel's own space, where no field took it: the
/// gesture belongs to the panel until it rests.
pub fn wheel_over_panel(cx: &mut App) {
    let now = cx.background_executor().now();
    let gesture = cx.default_global::<Gesture>();
    gesture.owner = Owner::Panel;
    gesture.last = Some(now);
}

/// Whether the control `key` takes a wheel event: a gesture that began over
/// it, or one that begins now.
fn takes_wheel(key: EntityId, cx: &mut App) -> bool {
    let now = cx.background_executor().now();
    let gesture = cx.default_global::<Gesture>();
    let takes = !active(gesture, now) || gesture.owner == Owner::Control(key);
    if takes {
        gesture.owner = Owner::Control(key);
        gesture.last = Some(now);
    }
    takes
}

/// The steps of a wheel event: a notch, or the trackpad's pixels, as a
/// fraction. Up is positive.
pub fn wheel_steps(delta: &ScrollDelta) -> f32 {
    let (x, y, lines) = match delta {
        ScrollDelta::Lines(delta) => (delta.x, delta.y, true),
        ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y), false),
    };
    // Shift turns a wheel sideways on some platforms.
    let main = if y != 0. { y } else { x };
    if !main.is_finite() {
        return 0.;
    }
    if lines {
        main / notch_lines()
    } else {
        main / TRACKPAD_STEP_PX
    }
}

/// The wheel's rest: how much of a step has built up, and the burst of steps
/// that the owner has not been told to commit.
#[derive(Default)]
struct Burst {
    remainder: f32,
    pending: bool,
    generation: u64,
    last: Option<Instant>,
}

/// Handles one wheel event over a control. `apply` changes the value by the
/// given whole steps and shows it, true when it changed; `commit` makes the
/// burst final.
fn handle_wheel(
    event: &ScrollWheelEvent,
    key: EntityId,
    burst: &Entity<Burst>,
    window: &mut Window,
    cx: &mut App,
    apply: &Rc<dyn Fn(i32, &mut Window, &mut App) -> bool>,
    commit: &Rc<dyn Fn(&mut App)>,
) {
    if !takes_wheel(key, cx) {
        return;
    }
    cx.stop_propagation();
    let now = cx.background_executor().now();
    let coarse = event.modifiers.shift;
    let units = wheel_steps(&event.delta);
    let whole = burst.update(cx, |burst, _| {
        if burst
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= WHEEL_IDLE)
            || burst.remainder * units < 0.
        {
            // A new gesture, or a turn the other way, starts afresh.
            burst.remainder = 0.;
        }
        burst.last = Some(now);
        burst.remainder += units;
        let whole = burst.remainder.trunc();
        burst.remainder -= whole;
        whole as i32
    });
    let steps = if coarse { whole * COARSE_STEPS } else { whole };
    if steps == 0 || !apply(steps, window, cx) {
        return;
    }
    let generation = burst.update(cx, |burst, _| {
        burst.pending = true;
        burst.generation += 1;
        burst.generation
    });
    let burst = burst.clone();
    let commit = commit.clone();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(WHEEL_IDLE).await;
        cx.update(|cx| {
            if burst.read(cx).generation == generation {
                end_burst(&burst, &commit, cx);
            }
        });
    })
    .detach();
}

/// Commits the burst of steps, if there is one.
fn end_burst(burst: &Entity<Burst>, commit: &Rc<dyn Fn(&mut App)>, cx: &mut App) {
    let pending = burst.update(cx, |burst, _| std::mem::take(&mut burst.pending));
    if pending {
        commit(cx);
    }
}

fn burst_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<Burst> {
    window.use_keyed_state(
        ElementId::NamedChild(Arc::new(id.clone()), "wheel-burst".into()),
        cx,
        |_, _| Burst::default(),
    )
}

/// Gives the number field `input`, drawn as `element`, the wheel, if its
/// steps were registered with [`register_number_wheel`].
pub fn number_wheel(
    element: Stateful<Div>,
    id: &ElementId,
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let Some(spec) = cx
        .try_global::<NumberSpecs>()
        .and_then(|specs| specs.0.get(&input.entity_id()).copied())
    else {
        return element;
    };
    let burst = burst_state(id, window, cx);
    let key = input.entity_id();
    let apply: Rc<dyn Fn(i32, &mut Window, &mut App) -> bool> = {
        let input = input.clone();
        Rc::new(move |steps, window, cx| {
            let current = input
                .read(cx)
                .value()
                .trim()
                .parse::<f64>()
                .unwrap_or(spec.min);
            let target = ((current + f64::from(steps) * spec.step).clamp(spec.min, spec.max) * 1e6)
                .round()
                / 1e6;
            if target == current {
                return false;
            }
            input.update(cx, |input, cx| {
                input.set_value(format_property_number(target), window, cx);
                cx.emit(InputEvent::Change);
            });
            true
        })
    };
    let commit: Rc<dyn Fn(&mut App)> = {
        let input = input.clone();
        Rc::new(move |cx| {
            input.update(cx, |_, cx| {
                cx.emit(InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                })
            })
        })
    };
    wheel_listeners(element, key, burst, apply, commit)
}

/// Gives the slider `slider`, with the row `element` that holds it, the
/// wheel: a step is the slider's own step.
pub fn slider_wheel(
    element: Stateful<Div>,
    id: &ElementId,
    slider: &Entity<SliderState>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let burst = burst_state(id, window, cx);
    let key = slider.entity_id();
    let apply: Rc<dyn Fn(i32, &mut Window, &mut App) -> bool> = {
        let slider = slider.clone();
        Rc::new(move |steps, window, cx| {
            let (current, step, min, max) = {
                let slider = slider.read(cx);
                (
                    slider.value().end(),
                    slider.step_value(),
                    slider.min_value(),
                    slider.max_value(),
                )
            };
            let target = ((current + steps as f32 * step).clamp(min, max) * 1e4).round() / 1e4;
            if target == current {
                return false;
            }
            slider.update(cx, |slider, cx| {
                slider.set_value(target, window, cx);
                cx.emit(SliderEvent::Change(slider.value()));
            });
            true
        })
    };
    let commit: Rc<dyn Fn(&mut App)> = {
        let slider = slider.clone();
        Rc::new(move |cx| {
            slider.update(cx, |slider, cx| {
                cx.emit(SliderEvent::Release(slider.value()))
            })
        })
    };
    wheel_listeners(element, key, burst, apply, commit)
}

fn wheel_listeners(
    element: Stateful<Div>,
    key: EntityId,
    burst: Entity<Burst>,
    apply: Rc<dyn Fn(i32, &mut Window, &mut App) -> bool>,
    commit: Rc<dyn Fn(&mut App)>,
) -> Stateful<Div> {
    let wheel_burst = burst.clone();
    let wheel_commit = commit.clone();
    element
        .on_scroll_wheel(move |event, window, cx| {
            handle_wheel(event, key, &wheel_burst, window, cx, &apply, &wheel_commit)
        })
        // The pointer leaving ends the burst at once.
        .on_hover(move |hovered, _, cx| {
            if !*hovered {
                end_burst(&burst, &commit, cx);
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn a_notch_is_one_step_and_up_is_positive() {
        let notch = notch_lines();
        assert_eq!(wheel_steps(&ScrollDelta::Lines(point(0., notch))), 1.);
        assert_eq!(wheel_steps(&ScrollDelta::Lines(point(0., -notch))), -1.);
    }

    #[test]
    fn a_trackpad_gives_a_step_for_each_run_of_pixels() {
        let steps = wheel_steps(&ScrollDelta::Pixels(point(px(0.), px(TRACKPAD_STEP_PX))));
        assert_eq!(steps, 1.);
        let half = wheel_steps(&ScrollDelta::Pixels(point(
            px(0.),
            px(TRACKPAD_STEP_PX / 2.),
        )));
        assert_eq!(half, 0.5);
    }

    #[test]
    fn a_sideways_wheel_steps_as_well() {
        let notch = notch_lines();
        assert_eq!(wheel_steps(&ScrollDelta::Lines(point(notch, 0.))), 1.);
    }
}
