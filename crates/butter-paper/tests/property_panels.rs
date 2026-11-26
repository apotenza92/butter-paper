//! The properties panels' shared behaviour: a picked colour applies at once
//! (a slider drag once the picker closes), and a markup's Text field reports
//! one edit when it is committed.

use std::{cell::RefCell, rc::Rc};

use butter_paper::{
    annotation_model::{
        DimensionAppearance, LineKind, MarkupId, StraightLineAppearance, TextBoxStyle,
    },
    dimension_property_inspector::{DimensionPropertyInspector, DimensionPropertySnapshot},
    document_workspace::DocumentId,
    property_controls::PropertyTextEditEvent,
    text_box_property_inspector::{
        TEXT_BOX_INSPECTOR_TEXT_ID, TextBoxPropertyEvent, TextBoxPropertyInspector,
        TextBoxPropertyPatch, TextBoxPropertySnapshot,
    },
};
use gpui::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext, rgb};
use gpui_component::Root;

fn text_box_snapshot() -> TextBoxPropertySnapshot {
    TextBoxPropertySnapshot {
        document_id: DocumentId::new(1),
        annotation_id: MarkupId::generate(),
        expected_revision: 3,
        style: TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        locked: false,
        mutation_disabled: false,
    }
}

fn open_text_box_inspector(
    cx: &mut TestAppContext,
) -> (&mut VisualTestContext, Entity<TextBoxPropertyInspector>) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let inspector = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let inspector = inspector.clone();
        move |window, cx| {
            let panel = cx.new(|cx| {
                let mut panel = TextBoxPropertyInspector::new(window, cx);
                panel.set_embedded();
                panel.sync(text_box_snapshot(), window, cx);
                panel
            });
            inspector.replace(Some(panel.clone()));
            Root::new(panel, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let inspector = inspector.borrow_mut().take().unwrap();
    (cx, inspector)
}

#[gpui::test]
fn a_picked_colour_applies_at_once_and_a_slider_drag_once_the_picker_closes(
    cx: &mut TestAppContext,
) {
    let (cx, inspector) = open_text_box_inspector(cx);
    let events = Rc::new(RefCell::new(Vec::<TextBoxPropertyEvent>::new()));
    let _subscription = cx.update(|_, cx| {
        let events = events.clone();
        cx.subscribe(&inspector, move |_, event: &TextBoxPropertyEvent, _| {
            events.borrow_mut().push(event.clone())
        })
    });
    let picker = inspector.read_with(cx, |inspector, _| inspector.color_picker());
    let colour = |events: &Rc<RefCell<Vec<TextBoxPropertyEvent>>>| {
        events
            .borrow()
            .iter()
            .map(|event| match &event.patch {
                TextBoxPropertyPatch::Style(style) => style.color().to_owned(),
                patch => panic!("unexpected {patch:?}"),
            })
            .collect::<Vec<_>>()
    };

    // A palette pick: one change, no Apply step.
    cx.update(|window, cx| {
        picker.update(cx, |picker, cx| {
            picker.select_color(rgb(0x0000ff).into(), window, cx)
        })
    });
    assert_eq!(colour(&events), ["#0000ff"]);

    // A slider drag inside the open picker waits for it to close.
    cx.update(|_, cx| picker.update(cx, |picker, cx| picker.set_open(true, cx)));
    for value in [0x00ff00, 0x00cc00, 0x009900] {
        cx.update(|window, cx| {
            picker.update(cx, |picker, cx| {
                picker.update_color(rgb(value).into(), window, cx)
            })
        });
    }
    assert_eq!(colour(&events), ["#0000ff"], "a drag is not yet a change");
    cx.update(|_, cx| picker.update(cx, |picker, cx| picker.set_open(false, cx)));
    assert_eq!(colour(&events), ["#0000ff", "#009900"]);

    // Closing again with nothing new applies nothing.
    cx.update(|_, cx| picker.update(cx, |picker, cx| picker.set_open(true, cx)));
    cx.update(|_, cx| picker.update(cx, |picker, cx| picker.set_open(false, cx)));
    assert_eq!(colour(&events), ["#0000ff", "#009900"]);
}

#[gpui::test]
fn the_text_field_shows_the_text_and_reports_one_edit_when_committed(cx: &mut TestAppContext) {
    let (cx, inspector) = open_text_box_inspector(cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(TEXT_BOX_INSPECTOR_TEXT_ID).is_none(),
        "no Text field until the workspace gives the text"
    );
    let edits = Rc::new(RefCell::new(Vec::<PropertyTextEditEvent>::new()));
    let _subscription = cx.update(|_, cx| {
        let edits = edits.clone();
        cx.subscribe(&inspector, move |_, event: &PropertyTextEditEvent, _| {
            edits.borrow_mut().push(event.clone())
        })
    });
    cx.update(|window, cx| {
        inspector.update(cx, |inspector, cx| {
            inspector.sync_content("Site note", window, cx)
        })
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(TEXT_BOX_INSPECTOR_TEXT_ID).is_some());
    let input = inspector.read_with(cx, |inspector, _| inspector.content_input());
    assert_eq!(input.read_with(cx, |input, _| input.value()), "Site note");
    assert!(edits.borrow().is_empty(), "showing the text is not an edit");

    cx.update(|window, _| window.activate_window());
    let elsewhere = cx.update(|_, cx| cx.focus_handle());
    cx.update(|window, cx| input.read(cx).focus_handle(cx).focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("end enter r e v i s e d");
    assert!(edits.borrow().is_empty(), "Enter starts a new line");
    assert_eq!(
        input.read_with(cx, |input, _| input.value()),
        "Site note\nrevised"
    );
    // Leaving the field commits it.
    cx.update(|window, cx| elsewhere.focus(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let committed = edits.borrow().clone();
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].content, "Site note\nrevised");
    assert_eq!(committed[0].expected_revision, 3);

    // Leaving it again unchanged is not another edit.
    cx.update(|window, cx| input.read(cx).focus_handle(cx).focus(window, cx));
    cx.update(|window, cx| elsewhere.focus(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(edits.borrow().len(), 1);

    // Cmd/Ctrl+Enter commits it without leaving a new line behind.
    cx.update(|window, cx| input.read(cx).focus_handle(cx).focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("end space n o w secondary-enter");
    cx.run_until_parked();
    let committed = edits.borrow().clone();
    assert_eq!(committed.len(), 2);
    assert_eq!(committed[1].content, "Site note\nrevised now");
    assert_eq!(
        input.read_with(cx, |input, _| input.value()),
        "Site note\nrevised now"
    );
}

#[gpui::test]
fn a_dimension_text_field_commits_on_enter_without_a_trailing_line(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let inspector = Rc::new(RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let inspector = inspector.clone();
        move |window, cx| {
            let panel = cx.new(|cx| {
                let mut panel = DimensionPropertyInspector::new(window, cx);
                panel.set_embedded();
                panel.sync(
                    DimensionPropertySnapshot {
                        document_id: DocumentId::new(2),
                        annotation_id: MarkupId::generate(),
                        expected_revision: 5,
                        offset_pt: 24.,
                        show_offset: true,
                        text_only: false,
                        appearance: DimensionAppearance::new(
                            StraightLineAppearance::default_for(LineKind::Line),
                            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                        )
                        .unwrap(),
                        caption_moved: false,
                        measuring: false,
                        locked: false,
                        mutation_disabled: false,
                    },
                    window,
                    cx,
                );
                panel.sync_content("1200 mm", window, cx);
                panel
            });
            inspector.replace(Some(panel.clone()));
            Root::new(panel, window, cx)
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let inspector: Entity<DimensionPropertyInspector> = inspector.borrow_mut().take().unwrap();
    let edits = Rc::new(RefCell::new(Vec::<PropertyTextEditEvent>::new()));
    let _subscription = cx.update(|_, cx| {
        let edits = edits.clone();
        cx.subscribe(&inspector, move |_, event: &PropertyTextEditEvent, _| {
            edits.borrow_mut().push(event.clone())
        })
    });
    let input = inspector.read_with(cx, |inspector, _| inspector.content_input());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| input.set_value("1250 mm", window, cx));
        input.read(cx).focus_handle(cx).focus(window, cx);
    });
    cx.simulate_keystrokes("enter");
    let committed = edits.borrow().clone();
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].content, "1250 mm");
}

struct NumberPairView {
    x: Entity<gpui_component::input::InputState>,
    y: Entity<gpui_component::input::InputState>,
}

impl gpui::Render for NumberPairView {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use butter_paper::property_controls::{PropertyNumericInput, property_number_pair_row};
        use gpui::{ParentElement as _, Styled as _};
        gpui::div().size_full().child(property_number_pair_row(
            "Position (pt)",
            PropertyNumericInput::new("pair-x", "X", &self.x).prefix("X"),
            PropertyNumericInput::new("pair-y", "Y", &self.y).prefix("Y"),
        ))
    }
}

#[gpui::test]
fn a_narrow_number_pair_moves_below_its_label_and_stays_side_by_side(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
    });
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| NumberPairView {
            x: cx.new(|cx| {
                gpui_component::input::InputState::new(window, cx).default_value("1191.06")
            }),
            y: cx.new(|cx| {
                gpui_component::input::InputState::new(window, cx).default_value("791.5")
            }),
        });
        Root::new(view, window, cx)
    });
    // 268px is the default 300px panel's content: there the pair moves below
    // its label rather than clip a four-digit coordinate.
    for (width, below_label) in [(400., false), (268., true), (220., true)] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(300.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let x = cx.debug_bounds("pair-x").unwrap();
        let y = cx.debug_bounds("pair-y").unwrap();
        assert_eq!(x.top(), y.top(), "at {width}px X and Y share a row");
        assert_eq!(
            x.left() < gpui::px(40.),
            below_label,
            "at {width}px: the pair sits beside its label only when it fits: {x:?}"
        );
        assert!(
            x.size.width >= gpui::px(100.),
            "each number keeps room for its value: {x:?}"
        );
    }
}

// ---- Optional sections -------------------------------------------------

mod optional_section {
    use std::{cell::RefCell, rc::Rc};

    use butter_paper::property_controls::{
        PropertyColor, PropertyNumericInput, PropertyOptionalSection, PropertyOptionalSectionIds,
        PropertySliderInput, PropertyTextField, property_color_and_size, property_row,
        property_slider_row,
    };
    use gpui::{
        AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
        KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString, Styled as _,
        TestAppContext, VisualTestContext, Window, div,
    };
    use gpui_component::{
        Disableable as _, Root,
        button::Button,
        color_picker::ColorPickerState,
        input::{InputState, TextareaState},
        select::{Select, SelectState},
        slider::SliderState,
        switch::Switch,
    };

    const SECTION: PropertyOptionalSectionIds = PropertyOptionalSectionIds {
        root: "test-section",
        checkbox: "test-section-checkbox",
        chevron: "test-section-chevron",
        body: "test-section-body",
    };

    /// Every id one of each property control family draws in the body.
    const FIELD_IDS: [&str; 8] = [
        "field-colour",
        "field-number",
        "field-slider",
        "field-select",
        "field-switch",
        "field-button",
        "field-text",
        "field-child",
    ];

    struct SectionView {
        checked: bool,
        open: bool,
        /// A setting in the body that `child` depends on.
        parent_on: bool,
        log: Rc<RefCell<Vec<String>>>,
        colour: Entity<ColorPickerState>,
        number: Entity<InputState>,
        child: Entity<InputState>,
        opacity: Entity<SliderState>,
        opacity_input: Entity<InputState>,
        select: Entity<SelectState<Vec<SharedString>>>,
        text: Entity<TextareaState>,
    }

    impl Render for SectionView {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let view = cx.entity().downgrade();
            let on_checked = view.clone();
            let on_open = view.clone();
            let on_parent = view.clone();
            let log_button = self.log.clone();
            let (colour, number, child, opacity, opacity_input, select, text, parent_on) = (
                self.colour.clone(),
                self.number.clone(),
                self.child.clone(),
                self.opacity.clone(),
                self.opacity_input.clone(),
                self.select.clone(),
                self.text.clone(),
                self.parent_on,
            );
            div().w_full().p_3().child(
                PropertyOptionalSection::new(SECTION, "Measure", self.checked, self.open)
                    .on_checked_change(move |checked, _, cx| {
                        let _ = on_checked.update(cx, |view, cx| {
                            view.log.borrow_mut().push(format!("checked:{checked}"));
                            view.checked = checked;
                            if checked {
                                view.open = true;
                            }
                            cx.notify();
                        });
                    })
                    .on_open_change(move |open, _, cx| {
                        let _ = on_open.update(cx, |view, cx| {
                            view.log.borrow_mut().push(format!("open:{open}"));
                            view.open = open;
                            cx.notify();
                        });
                    })
                    .body(move |disabled| {
                        gpui::div()
                            .w_full()
                            .child(property_row(
                                "Line",
                                property_color_and_size(
                                    PropertyColor::new("field-colour", "Line colour", &colour)
                                        .disabled(disabled),
                                    PropertyNumericInput::new("field-number", "Width", &number)
                                        .disabled(disabled),
                                ),
                            ))
                            .child(property_slider_row(
                                "Opacity",
                                PropertySliderInput::new("Opacity", &opacity, &opacity_input)
                                    .slider_id("field-slider")
                                    .disabled(disabled),
                            ))
                            .child(property_row(
                                "Unit",
                                div()
                                    .id("field-select")
                                    .debug_selector(|| "field-select".into())
                                    .child(Select::new(&select).disabled(disabled)),
                            ))
                            .child(property_row(
                                "Switch",
                                div()
                                    .id("field-switch")
                                    .debug_selector(|| "field-switch".into())
                                    .child(
                                        Switch::new("test-switch")
                                            .checked(parent_on)
                                            .disabled(disabled)
                                            .on_click(move |on, _, cx| {
                                                let _ = on_parent.update(cx, |view, cx| {
                                                    view.parent_on = *on;
                                                    view.log.borrow_mut().push("switch".into());
                                                    cx.notify();
                                                });
                                            }),
                                    ),
                            ))
                            .child(property_row(
                                "Action",
                                div()
                                    .id("field-button")
                                    .debug_selector(|| "field-button".into())
                                    .child(
                                        Button::new("test-button")
                                            .label("Change")
                                            .disabled(disabled)
                                            .on_click(move |_, _, _| {
                                                log_button.borrow_mut().push("button".into())
                                            }),
                                    ),
                            ))
                            .child(property_row(
                                "Text",
                                PropertyTextField::new("field-text", &text).disabled(disabled),
                            ))
                            .child(
                                property_row(
                                    "Child",
                                    PropertyNumericInput::new("field-child", "Child", &child)
                                        .disabled(disabled || !parent_on),
                                )
                                .dependent(),
                            )
                            .into_any_element()
                    }),
            )
        }
    }

    fn open_view(
        cx: &mut TestAppContext,
        checked: bool,
    ) -> (&mut VisualTestContext, Entity<SectionView>) {
        cx.update(|cx| {
            gpui_component::init(cx);
            // Behaviour tests check end states; motion has its own tests.
            cx.set_reduce_motion(true);
        });
        let slot = Rc::new(RefCell::new(None));
        let (_, cx) = cx.add_window_view({
            let slot = slot.clone();
            move |window, cx| {
                let view = cx.new(|cx| SectionView {
                    checked,
                    open: true,
                    parent_on: true,
                    log: Rc::default(),
                    colour: cx.new(|cx| ColorPickerState::new(window, cx)),
                    number: cx.new(|cx| InputState::new(window, cx).default_value("1")),
                    child: cx.new(|cx| InputState::new(window, cx).default_value("2")),
                    opacity: cx.new(|_| {
                        SliderState::new()
                            .min(0.)
                            .max(100.)
                            .step(1.)
                            .default_value(50.)
                    }),
                    opacity_input: cx.new(|cx| InputState::new(window, cx).default_value("50")),
                    select: cx.new(|cx| {
                        SelectState::new(
                            vec![SharedString::from("Inches"), SharedString::from("Feet")],
                            None,
                            window,
                            cx,
                        )
                    }),
                    text: cx.new(|cx| TextareaState::new(window, cx)),
                });
                slot.replace(Some(view.clone()));
                Root::new(view, window, cx)
            }
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let view = slot.borrow_mut().take().unwrap();
        (cx, view)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn shown(cx: &mut VisualTestContext) -> Vec<&'static str> {
        [SECTION.checkbox, SECTION.chevron, SECTION.body]
            .into_iter()
            .chain(FIELD_IDS)
            .filter(|id| cx.debug_bounds(id).is_some())
            .collect()
    }

    fn log(view: &Entity<SectionView>, cx: &mut VisualTestContext) -> Vec<String> {
        view.read_with(cx, |view, _| view.log.borrow().clone())
    }

    fn tap(cx: &mut VisualTestContext, key: &str) {
        let keystroke = Keystroke::parse(key).unwrap();
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
    }

    #[gpui::test]
    fn an_unchecked_section_keeps_every_row_disabled_and_inert(cx: &mut TestAppContext) {
        let (cx, view) = open_view(cx, false);
        let unchecked = shown(cx);
        assert_eq!(unchecked.len(), 3 + FIELD_IDS.len(), "{unchecked:?}");

        // Every control family: pressing it does nothing.
        for id in [
            "field-switch",
            "field-button",
            "field-slider",
            "field-select",
        ] {
            let bounds = cx.debug_bounds(id).unwrap();
            cx.simulate_click(bounds.center(), Modifiers::default());
        }
        let number = view.read_with(cx, |view, _| view.number.clone());
        let child = view.read_with(cx, |view, _| view.child.clone());
        let opacity = view.read_with(cx, |view, _| view.opacity.clone());
        for (id, input) in [("field-number", &number), ("field-child", &child)] {
            let bounds = cx.debug_bounds(id).unwrap();
            cx.simulate_click(bounds.center(), Modifiers::default());
            cx.simulate_keystrokes("9");
            let _ = input;
        }
        draw(cx);
        assert!(log(&view, cx).is_empty(), "{:?}", log(&view, cx));
        assert_eq!(
            number.read_with(cx, |input, _| input.value().to_string()),
            "1"
        );
        assert_eq!(
            child.read_with(cx, |input, _| input.value().to_string()),
            "2"
        );
        assert_eq!(
            opacity.read_with(cx, |slider, _| slider.value().start()),
            50.
        );
        assert!(view.read_with(cx, |view, cx| !view.colour.read(cx).is_open()));
    }

    #[gpui::test]
    fn checking_a_section_enables_the_same_rows_and_opens_it(cx: &mut TestAppContext) {
        let (cx, view) = open_view(cx, false);
        let unchecked = shown(cx);

        // Collapsed by its chevron while unchecked, then checked: the
        // checkbox opens the section again.
        let chevron = cx.debug_bounds(SECTION.chevron).unwrap();
        cx.simulate_click(chevron.center(), Modifiers::default());
        draw(cx);
        assert_eq!(
            shown(cx),
            [SECTION.checkbox, SECTION.chevron],
            "collapsed: the heading stays, the fields go"
        );
        let checkbox = cx.debug_bounds(SECTION.checkbox).unwrap();
        cx.simulate_click(checkbox.center(), Modifiers::default());
        draw(cx);
        assert_eq!(log(&view, cx), ["open:false", "checked:true"]);
        assert!(view.read_with(cx, |view, _| view.checked && view.open));
        let checked = shown(cx);
        assert_eq!(checked, unchecked, "checking adds and removes no row");

        // The same rows now answer.
        for id in ["field-switch", "field-button"] {
            let bounds = cx.debug_bounds(id).unwrap();
            cx.simulate_click(bounds.center(), Modifiers::default());
        }
        let number = view.read_with(cx, |view, _| view.number.clone());
        let bounds = cx.debug_bounds("field-number").unwrap();
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.simulate_keystrokes("9");
        draw(cx);
        let log = log(&view, cx);
        assert!(log.contains(&"switch".to_owned()) && log.contains(&"button".to_owned()));
        assert_ne!(
            number.read_with(cx, |input, _| input.value().to_string()),
            "1"
        );
    }

    #[gpui::test]
    fn the_chevron_collapses_without_unchecking_and_back(cx: &mut TestAppContext) {
        let (cx, view) = open_view(cx, true);
        let open = shown(cx);
        let chevron = cx.debug_bounds(SECTION.chevron).unwrap();
        cx.simulate_click(chevron.center(), Modifiers::default());
        draw(cx);
        assert!(view.read_with(cx, |view, _| view.checked && !view.open));
        assert_eq!(shown(cx), [SECTION.checkbox, SECTION.chevron]);
        let chevron = cx.debug_bounds(SECTION.chevron).unwrap();
        cx.simulate_click(chevron.center(), Modifiers::default());
        draw(cx);
        assert_eq!(shown(cx), open);
        assert_eq!(log(&view, cx), ["open:false", "open:true"]);
    }

    #[gpui::test]
    fn a_dependent_row_is_indented_and_disabled_while_its_parent_is_off(cx: &mut TestAppContext) {
        let (cx, view) = open_view(cx, true);
        let text = cx.debug_bounds("field-text").unwrap();
        let child = cx.debug_bounds("field-child").unwrap();
        assert!(
            child.top() > text.top() && child.left() > text.left(),
            "the dependent row sits under its parent, indented: {child:?} {text:?}"
        );
        // Off, the dependent input stays and ignores typing.
        let switch = cx.debug_bounds("field-switch").unwrap();
        cx.simulate_click(switch.center(), Modifiers::default());
        draw(cx);
        assert!(!view.read_with(cx, |view, _| view.parent_on));
        assert!(cx.debug_bounds("field-child").is_some());
        let child_input = view.read_with(cx, |view, _| view.child.clone());
        let bounds = cx.debug_bounds("field-child").unwrap();
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.simulate_keystrokes("7");
        assert_eq!(
            child_input.read_with(cx, |input, _| input.value().to_string()),
            "2"
        );
    }

    #[gpui::test]
    fn the_checkbox_and_the_chevron_are_separate_tab_stops_that_space_and_enter_press(
        cx: &mut TestAppContext,
    ) {
        let (cx, view) = open_view(cx, false);
        // Tab, as the window's own focus order.
        cx.update(|window, cx| window.focus_next(cx));
        let focused = cx.update(|window, cx| window.focused(cx).is_some());
        assert!(focused, "Tab reaches the checkbox first");
        tap(cx, "space");
        draw(cx);
        assert_eq!(log(&view, cx), ["checked:true"], "Space on the checkbox");
        cx.update(|window, cx| window.focus_next(cx));
        tap(cx, "enter");
        draw(cx);
        assert_eq!(
            log(&view, cx),
            ["checked:true", "open:false"],
            "Enter on the chevron collapses, leaving the checkbox as it was"
        );
        assert!(view.read_with(cx, |view, _| view.checked && !view.open));
        tap(cx, "space");
        draw(cx);
        assert_eq!(
            log(&view, cx),
            ["checked:true", "open:false", "open:true"],
            "Space on the chevron expands again"
        );
    }
}
