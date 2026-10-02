use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use butter_paper::continuous_view_control::{
    CONTINUOUS_PRIMARY_ID, CONTINUOUS_SPLIT_ID, ContinuousViewControl, WheelBehavior,
};
use butter_paper::system_theme::apply_window_appearance;
use gpui::{
    AppContext as _, Bounds, Context, Entity, EntityInputHandler as _, HighlightStyle,
    InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render,
    StrikethroughStyle, Styled as _, TestAppContext, UnderlineStyle, Window, WindowAppearance, div,
    point, px, radians, size,
};
use gpui_component::{
    Root, Selectable as _, Theme, ThemeMode,
    button::{Button, ButtonGroup},
    input::{EditorState, TextDecoration, Textarea, TextareaRotation, TextareaState},
};

#[gpui::test]
fn window_appearance_changes_project_complete_component_and_base_themes(cx: &mut TestAppContext) {
    let pinned_theme_source =
        include_str!("../.prepared/gpui-component-c27f5d5c/crates/ui/src/theme/mod.rs");
    for projection in [
        "cx.set_global(base_theme);",
        "tokens: self.semantic_tokens(),",
        ".with_mode(self.scrollbar_mode)",
        "handle: self.border,",
        "active_handle: self.drag_border,",
    ] {
        assert!(
            pinned_theme_source.contains(projection),
            "the pinned GPUI Component Base projection must retain {projection:?}"
        );
    }
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| CompatibilityView {
            clicks: Rc::new(Cell::new(0)),
        });
        Root::new(view, window, cx)
    });

    let mut projections = Vec::new();
    for appearance in [
        WindowAppearance::Light,
        WindowAppearance::VibrantLight,
        WindowAppearance::Dark,
        WindowAppearance::VibrantDark,
    ] {
        projections.push(cx.update(|window, cx| {
            apply_window_appearance(appearance, window, cx);
            let theme = Theme::global(cx);
            (
                theme.mode,
                theme.semantic_tokens(),
                theme.scrollbar_mode,
                theme.border,
                theme.drag_border,
            )
        }));
    }

    assert_eq!(projections[0].0, ThemeMode::Light);
    assert_eq!(projections[1].0, ThemeMode::Light);
    assert_eq!(projections[2].0, ThemeMode::Dark);
    assert_eq!(projections[3].0, ThemeMode::Dark);
    assert_eq!(projections[0].1, projections[1].1);
    assert_eq!(projections[2].1, projections[3].1);
    assert_ne!(projections[0].1, projections[2].1);

    let roundtrip = cx.update(|window, cx| {
        apply_window_appearance(WindowAppearance::Light, window, cx);
        let theme = Theme::global(cx);
        (
            theme.mode,
            theme.semantic_tokens(),
            theme.scrollbar_mode,
            theme.border,
            theme.drag_border,
        )
    });
    assert_eq!(roundtrip, projections[0]);
}

struct CompatibilityView {
    clicks: Rc<Cell<usize>>,
}

struct ButtonGroupCompatibilityView {
    selections: Rc<RefCell<Vec<Vec<usize>>>>,
}

struct RotatedTextareaCompatibilityView {
    input: Entity<TextareaState>,
    rotation: TextareaRotation,
}

struct RotatedDecoratedEditorCompatibilityView {
    input: Entity<EditorState>,
    rotation: TextareaRotation,
}

impl Render for CompatibilityView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        Button::new("phase-0-button")
            .debug_selector(|| "phase-0-button".into())
            .label("Compatibility")
            .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
    }
}

impl Render for ButtonGroupCompatibilityView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let selections = self.selections.clone();
        ButtonGroup::new("phase-0-button-group")
            .child(
                Button::new("phase-0-button-group-single")
                    .debug_selector(|| "phase-0-button-group-single".into())
                    .label("Single page"),
            )
            .child(
                Button::new("phase-0-button-group-continuous")
                    .debug_selector(|| "phase-0-button-group-continuous".into())
                    .label("Continuous")
                    .selected(true),
            )
            .on_click(move |selected, _, _| selections.borrow_mut().push(selected.clone()))
    }
}

impl Render for RotatedTextareaCompatibilityView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let frame = self.rotation.frame_size();
        div()
            .id("rotated-textarea-platform-input")
            .debug_selector(|| "rotated-textarea-platform-input".into())
            .w(frame.width)
            .h(frame.height)
            .child(
                Textarea::new(&self.input)
                    .appearance(false)
                    .bordered(false)
                    .rotation(self.rotation)
                    .size_full(),
            )
    }
}

impl Render for RotatedDecoratedEditorCompatibilityView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let frame = self.rotation.frame_size();
        div()
            .id("rotated-decorated-editor")
            .debug_selector(|| "rotated-decorated-editor".into())
            .w(frame.width)
            .h(frame.height)
            .child(self.input.clone())
    }
}

#[gpui::test]
fn component_stack_initializes_renders_and_activates(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let clicks = Rc::new(Cell::new(0));
    let (_, cx) = cx.add_window_view({
        let clicks = clicks.clone();
        move |window, cx| {
            let view = cx.new(|_| CompatibilityView { clicks });
            Root::new(view, window, cx)
        }
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let button = cx
        .debug_bounds("phase-0-button")
        .expect("the real GPUI Component button must participate in layout");
    assert!(button.size.width > px(0.));
    assert!(button.size.height > px(0.));

    cx.simulate_click(button.center(), Modifiers::default());
    assert_eq!(clicks.get(), 1);
}

#[gpui::test]
fn button_group_renders_and_reports_single_selection(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let selections = Rc::new(RefCell::new(Vec::new()));
    let (_, cx) = cx.add_window_view({
        let selections = selections.clone();
        move |window, cx| {
            let view = cx.new(|_| ButtonGroupCompatibilityView { selections });
            Root::new(view, window, cx)
        }
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let single_page = cx
        .debug_bounds("phase-0-button-group-single")
        .expect("the first grouped button must participate in layout");
    let continuous = cx
        .debug_bounds("phase-0-button-group-continuous")
        .expect("the second grouped button must participate in layout");
    assert!(single_page.size.width > px(0.));
    assert!(continuous.size.width > px(0.));

    cx.simulate_click(single_page.center(), Modifiers::default());
    assert_eq!(selections.borrow().as_slice(), &[vec![0]]);
}

#[gpui::test]
fn rotated_textarea_platform_range_and_point_geometry_round_trip(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let input_slot = Rc::new(RefCell::new(None::<Entity<TextareaState>>));
    let rotation = TextareaRotation::new(
        radians(std::f32::consts::FRAC_PI_6),
        size(px(160.), px(60.)),
    );
    let (_, cx) = cx.add_window_view({
        let input_slot = input_slot.clone();
        move |window, cx| {
            let input = cx.new(|cx| TextareaState::new(window, cx).default_value("abc"));
            input_slot.replace(Some(input.clone()));
            let view = cx.new(|_| RotatedTextareaCompatibilityView { input, rotation });
            Root::new(view, window, cx)
        }
    });
    let input = input_slot
        .borrow_mut()
        .take()
        .expect("the rotated Textarea state must be retained");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let frame = cx
        .debug_bounds("rotated-textarea-platform-input")
        .expect("the rotated Textarea frame must participate in layout");
    let content = Bounds::new(
        point(
            frame.center().x - rotation.content_size().width / 2.,
            frame.center().y - rotation.content_size().height / 2.,
        ),
        rotation.content_size(),
    );
    let (public_bounds, platform_bounds, mapped_index) = cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            let public_bounds = input.range_to_bounds(&(1..1));
            let platform_bounds = input.bounds_for_range(1..1, content, window, cx);
            let mapped_index = platform_bounds
                .and_then(|bounds| input.character_index_for_point(bounds.center(), window, cx));
            (public_bounds, platform_bounds, mapped_index)
        })
    });
    let public_bounds = public_bounds.expect("the public rotated caret range must be laid out");
    let platform_bounds =
        platform_bounds.expect("the platform rotated caret range must be laid out");
    assert_eq!(platform_bounds, public_bounds);
    assert!(platform_bounds.size.width > px(1.));
    assert!(platform_bounds.size.height > platform_bounds.size.width);
    assert_eq!(mapped_index, Some(1));
}

#[gpui::test]
fn rotated_editor_strikethrough_paints_through_app_graph(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let rotation = TextareaRotation::new(
        radians(std::f32::consts::FRAC_PI_6),
        size(px(120.), px(80.)),
    );
    let value = "wrapped strikethrough decoration crosses visual rows";
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let input = cx.new(|cx| {
            EditorState::new(window, cx)
                .default_value(value)
                .soft_wrap(true)
        });
        input.update(cx, |input, cx| {
            input.set_textarea_rotation(Some(rotation));
            input.create_decorations_collection(
                vec![TextDecoration::new(
                    0..value.len(),
                    HighlightStyle {
                        strikethrough: Some(StrikethroughStyle {
                            thickness: px(2.),
                            color: None,
                        }),
                        ..Default::default()
                    },
                )],
                cx,
            );
        });
        let view = cx.new(|_| RotatedDecoratedEditorCompatibilityView { input, rotation });
        Root::new(view, window, cx)
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let frame = cx
        .debug_bounds("rotated-decorated-editor")
        .expect("the rotated decorated editor must participate in layout");
    let expected = rotation.frame_size();
    assert!((frame.size.width - expected.width).abs() <= px(0.5));
    assert!((frame.size.height - expected.height).abs() <= px(0.5));
}

#[gpui::test]
fn rotated_editor_wavy_underline_paints_through_app_graph(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let rotation = TextareaRotation::new(
        radians(std::f32::consts::FRAC_PI_6),
        size(px(120.), px(80.)),
    );
    let value = "wrapped wavy underline decoration crosses visual rows";
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let input = cx.new(|cx| {
            EditorState::new(window, cx)
                .default_value(value)
                .soft_wrap(true)
        });
        input.update(cx, |input, cx| {
            input.set_textarea_rotation(Some(rotation));
            let split = "wrapped wavy".len();
            let underline = UnderlineStyle {
                thickness: px(1.4),
                color: Some(gpui::red()),
                wavy: true,
            };
            input.create_decorations_collection(
                vec![
                    TextDecoration::new(
                        0..split,
                        HighlightStyle {
                            color: Some(gpui::black()),
                            underline: Some(underline),
                            ..Default::default()
                        },
                    ),
                    TextDecoration::new(
                        split..value.len(),
                        HighlightStyle {
                            color: Some(gpui::blue()),
                            underline: Some(underline),
                            ..Default::default()
                        },
                    ),
                ],
                cx,
            );
        });
        let view = cx.new(|_| RotatedDecoratedEditorCompatibilityView { input, rotation });
        Root::new(view, window, cx)
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let frame = cx
        .debug_bounds("rotated-decorated-editor")
        .expect("the rotated decorated editor must participate in layout");
    let expected = rotation.frame_size();
    assert!((frame.size.width - expected.width).abs() <= px(0.5));
    assert!((frame.size.height - expected.height).abs() <= px(0.5));
}

#[gpui::test]
fn continuous_split_control_traces_primary_menu_and_selection(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let control_slot = Rc::new(RefCell::new(None::<Entity<ContinuousViewControl>>));
    let (_, cx) = cx.add_window_view({
        let control_slot = control_slot.clone();
        move |window, cx| {
            let control = cx.new(|_| ContinuousViewControl::new());
            control_slot.replace(Some(control.clone()));
            Root::new(control, window, cx)
        }
    });
    let control = control_slot
        .borrow()
        .clone()
        .expect("the Continuous-view control entity must be retained");

    assert_eq!(CONTINUOUS_SPLIT_ID, "continuous-view-split");
    assert_eq!(CONTINUOUS_PRIMARY_ID, "continuous-view-primary");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let split = cx
        .debug_bounds(CONTINUOUS_SPLIT_ID)
        .expect("the split control must participate in layout under its stable ID");
    let primary = cx
        .debug_bounds(CONTINUOUS_PRIMARY_ID)
        .expect("the primary action must participate in layout under its stable ID");
    assert!(split.size.width > primary.size.width);

    cx.simulate_click(primary.center(), Modifiers::default());
    assert_eq!(
        control.read_with(cx, |control, _| control.primary_activations()),
        1
    );
    assert_eq!(
        control.read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Scroll
    );

    let focus_before_menu = cx.update(|window, cx| window.focused(cx));
    cx.simulate_click(
        point(
            primary.origin.x + primary.size.width + px(10.),
            primary.center().y,
        ),
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let menu_focus = cx.update(|window, cx| window.focused(cx));
    assert!(menu_focus.is_some(), "the opened menu must take focus");
    assert_ne!(
        menu_focus, focus_before_menu,
        "the caret must move focus into the real menu"
    );

    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        control.read_with(cx, |control, _| control.wheel_behavior()),
        WheelBehavior::Zoom
    );
    assert_eq!(
        control.read_with(cx, |control, _| control.primary_activations()),
        1
    );
    assert!(cx.debug_bounds(CONTINUOUS_SPLIT_ID).is_some());
    assert!(cx.debug_bounds(CONTINUOUS_PRIMARY_ID).is_some());
}
