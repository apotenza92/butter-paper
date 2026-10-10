use gpui::{IntoElement, ParentElement, Styled, div};
use gpui_component::{
    button::Button,
    dialog::{DialogAction, DialogClose},
};

/// Wraps a stock dialog footer button that dismisses the dialog. The pinned
/// `DialogClose` fills its parent, which in a `DialogFooter` row stretches
/// every button to an equal share; the wrapper keeps it at its label width,
/// right-aligned like the application's other dialog footers.
pub(crate) fn dialog_close_button(button: Button) -> impl IntoElement {
    div().flex_none().child(DialogClose::new().child(button))
}

/// Wraps a stock dialog footer button that confirms the dialog (its `on_ok`),
/// sized to its label for the same reason as [`dialog_close_button`].
pub(crate) fn dialog_action_button(button: Button) -> impl IntoElement {
    div().flex_none().child(DialogAction::new().child(button))
}
