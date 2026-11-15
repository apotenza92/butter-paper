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
    cx.update(gpui_component::init);
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
    cx.update(gpui_component::init);
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
                        measure: Some(false),
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
    cx.update(gpui_component::init);
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
