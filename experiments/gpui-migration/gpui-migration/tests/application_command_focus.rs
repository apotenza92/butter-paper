use butter_paper_gpui_migration::application_shell::focus_initial_command_context;
use gpui::{
    Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    TestAppContext, Window, div,
};
use gpui_component::input::Undo;

// This checks GPUI dispatch independently of Root's macOS accessibility bridge,
// which requires a real platform window. Real menu/dialog coverage is run on Mac.
struct CommandContexts {
    workspace_focus: FocusHandle,
    input_focus: FocusHandle,
    document_undos: usize,
    input_undos: usize,
}

impl Render for CommandContexts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(
                div()
                    .track_focus(&self.workspace_focus)
                    .on_action(cx.listener(|this, _: &Undo, _, _| this.document_undos += 1)),
            )
            .child(
                div()
                    .track_focus(&self.input_focus)
                    .on_action(cx.listener(|this, _: &Undo, _, _| this.input_undos += 1)),
            )
    }
}

#[gpui::test]
fn initial_command_focus_routes_edits_without_a_tab_click(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let workspace_focus = cx.focus_handle();
        focus_initial_command_context(&workspace_focus, window);
        CommandContexts {
            workspace_focus,
            input_focus: cx.focus_handle(),
            document_undos: 0,
            input_undos: 0,
        }
    });
    let original_focus = cx.update(|window, cx| {
        assert!(
            window.focused(cx).is_none(),
            "wait for the command tree's first frame"
        );
        assert_eq!(window.simulate_next_frame(cx), 1);
        window
            .focused(cx)
            .expect("the first frame establishes command focus")
    });
    cx.dispatch_action(Undo);
    view.read_with(cx, |view, _| {
        assert_eq!(view.document_undos, 1);
        assert_eq!(view.input_undos, 0);
    });

    // Focusing a text field still owns Edit; this is not a global Undo fallback.
    cx.update(|window, cx| {
        let input_focus = view.read(cx).input_focus.clone();
        input_focus.focus(window, cx);
    });
    cx.dispatch_action(Undo);
    view.read_with(cx, |view, _| {
        assert_eq!(view.document_undos, 1);
        assert_eq!(view.input_undos, 1);
    });

    // A popup/dialog can restore the saved target without another pointer click.
    cx.update(|window, cx| original_focus.focus(window, cx));
    cx.dispatch_action(Undo);
    view.read_with(cx, |view, _| {
        assert_eq!(view.document_undos, 2);
        assert_eq!(view.input_undos, 1);
    });
}
