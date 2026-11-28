//! The Edit PDF format bar: a second bar below the viewer toolbar while Edit
//! PDF is on, with the text controls of a word processor's formatting bar.
//!
//! The bar holds its own control states in an
//! [`EditPdfContentPropertyInspector`] that it never draws as a panel. So
//! each control emits the same [`EditPdfContentPropertyEvent`] as the
//! Properties panel control, and the workspace applies it with the same
//! patch path (one undo step, to the selection or the whole paragraph). The
//! workspace syncs the bar and the panel from the one edited paragraph, so
//! they always agree.
//!
//! The bar shows every control in every state. With no text being edited
//! the controls are disabled; no control appears or goes away.

use std::{cell::Cell, rc::Rc};

use gpui::{
    App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyBinding, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, actions, div,
    prelude::FluentBuilder as _, rems,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, Icon, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    select::{SearchableVec, Select, SelectEvent},
    tooltip::Tooltip,
};

use crate::accessible_button::accessible_icon_button;
use crate::application_assets::{
    FORMAT_ALIGN_CENTRE_ICON, FORMAT_ALIGN_JUSTIFY_ICON, FORMAT_ALIGN_LEFT_ICON,
    FORMAT_ALIGN_RIGHT_ICON, FORMAT_BOLD_ICON, FORMAT_ITALIC_ICON,
};
use crate::edit_pdf_content_property_inspector::{
    EditPdfContentPropertyInspector, EditPdfContentPropertyPatch, EditPdfContentPropertySnapshot,
};
use crate::pdf_text_blocks::TextBlockAlignment;
use crate::property_controls::{PropertyColor, PropertyStepInput, property_color_picker};

pub const EDIT_PDF_FORMAT_BAR_ID: &str = "edit-pdf-format-bar";
pub const EDIT_PDF_FORMAT_BAR_CONTENT_ID: &str = "edit-pdf-format-bar-content";
pub const EDIT_PDF_FORMAT_FONT_ID: &str = "edit-pdf-format-font";
pub const EDIT_PDF_FORMAT_SIZE_ID: &str = "edit-pdf-format-size";
const EDIT_PDF_FORMAT_SIZE_SLOT_ID: &str = "edit-pdf-format-size-slot";
pub const EDIT_PDF_FORMAT_STYLE_GROUP_ID: &str = "edit-pdf-format-style";
pub const EDIT_PDF_FORMAT_BOLD_ID: &str = "edit-pdf-format-bold";
pub const EDIT_PDF_FORMAT_ITALIC_ID: &str = "edit-pdf-format-italic";
pub const EDIT_PDF_FORMAT_COLOR_ID: &str = "edit-pdf-format-color";
pub const EDIT_PDF_FORMAT_ALIGNMENT_GROUP_ID: &str = "edit-pdf-format-alignment";
pub const EDIT_PDF_FORMAT_ALIGN_LEFT_ID: &str = "edit-pdf-format-align-left";
pub const EDIT_PDF_FORMAT_ALIGN_CENTER_ID: &str = "edit-pdf-format-align-center";
pub const EDIT_PDF_FORMAT_ALIGN_RIGHT_ID: &str = "edit-pdf-format-align-right";
pub const EDIT_PDF_FORMAT_ALIGN_JUSTIFY_ID: &str = "edit-pdf-format-align-justify";
/// The narrow forms' one Alignment menu button, in place of the four.
pub const EDIT_PDF_FORMAT_ALIGNMENT_MENU_ID: &str = "edit-pdf-format-alignment-menu";
/// The dividers between the bar's groups.
const DIVIDER_GROUP: &str = "edit-pdf-format-groups";
const KEY_CONTEXT: &str = "EditPdfFormatBar";

actions!(edit_pdf_format_bar, [ReturnToEditedText]);

/// Escape anywhere in the bar returns the keyboard to the edited text. A
/// select or a colour palette that is open takes Escape first and closes.
pub fn init_edit_pdf_format_bar_actions(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        ReturnToEditedText,
        Some(KEY_CONTEXT),
    )]);
}

/// Each alignment: its value, its tooltip, its menu item, its button's id
/// and its icon.
const ALIGNMENTS: [(TextBlockAlignment, &str, &str, &str, &str); 4] = [
    (
        TextBlockAlignment::Left,
        "Align Left",
        "Left",
        EDIT_PDF_FORMAT_ALIGN_LEFT_ID,
        FORMAT_ALIGN_LEFT_ICON,
    ),
    (
        TextBlockAlignment::Center,
        "Align Centre",
        "Centre",
        EDIT_PDF_FORMAT_ALIGN_CENTER_ID,
        FORMAT_ALIGN_CENTRE_ICON,
    ),
    (
        TextBlockAlignment::Right,
        "Align Right",
        "Right",
        EDIT_PDF_FORMAT_ALIGN_RIGHT_ID,
        FORMAT_ALIGN_RIGHT_ICON,
    ),
    (
        TextBlockAlignment::Justified,
        "Justify",
        "Justify",
        EDIT_PDF_FORMAT_ALIGN_JUSTIFY_ID,
        FORMAT_ALIGN_JUSTIFY_ICON,
    ),
];

/// What a bar of a given width has room for. The whole bar changes form at
/// once, as the viewer toolbar does, and every control stays reachable:
/// - Full: every control, the colour with its hex value.
/// - AlignmentMenu: the four alignment buttons merge into one menu button.
/// - Narrow: the colour shows its swatch alone, the typeface and size are
///   narrower and the gaps are halved.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum FormatBarFit {
    Full,
    AlignmentMenu,
    Narrow,
}

/// The parts of the size stepper, which each have their own tooltip.
#[derive(Clone, Copy)]
enum SizePart {
    Decrease,
    Field,
    Increase,
}

/// Each part's hover region id, its tooltip and its part.
const SIZE_TOOLTIPS: [(&str, &str, SizePart); 3] = [
    (
        "edit-pdf-format-size-decrease-tip",
        "Decrease font size",
        SizePart::Decrease,
    ),
    (
        "edit-pdf-format-size-field-tip",
        "Font size (pt)",
        SizePart::Field,
    ),
    (
        "edit-pdf-format-size-increase-tip",
        "Increase font size",
        SizePart::Increase,
    ),
];

/// One stock − or + button: `min_w_8` at the default size.
const SIZE_BUTTON_REMS: f32 = 2.;

/// The rooms of the bar's controls, in rems.
const TYPEFACE_REMS: f32 = 12.;
const NARROW_TYPEFACE_REMS: f32 = 5.25;
/// The stock − and + take 4 rems and the field's padding 1.5: the rest
/// shows the number.
const SIZE_REMS: f32 = 7.5;
const NARROW_SIZE_REMS: f32 = 6.75;
const STYLE_REMS: f32 = 4.;
const COLOR_REMS: f32 = 6.5;
const SWATCH_REMS: f32 = 1.75;
const ALIGNMENT_REMS: f32 = 8.;
/// The compact menu button: its icon, its caret and its padding.
const ALIGNMENT_MENU_REMS: f32 = 3.375;

impl FormatBarFit {
    /// Every form, from the widest to the narrowest.
    pub const ALL: [Self; 3] = [Self::Full, Self::AlignmentMenu, Self::Narrow];

    /// The width the form needs, in rems: its controls, the dividers and
    /// gaps between its five groups, and its edge insets.
    pub const fn rems(self) -> f32 {
        // Five groups: four gaps with a divider in each, and two insets.
        let (gap, typeface, size, color, alignment) = match self {
            Self::Full => (0.5, TYPEFACE_REMS, SIZE_REMS, COLOR_REMS, ALIGNMENT_REMS),
            Self::AlignmentMenu => (
                0.5,
                TYPEFACE_REMS,
                SIZE_REMS,
                COLOR_REMS,
                ALIGNMENT_MENU_REMS,
            ),
            Self::Narrow => (
                0.25,
                NARROW_TYPEFACE_REMS,
                NARROW_SIZE_REMS,
                SWATCH_REMS,
                ALIGNMENT_MENU_REMS,
            ),
        };
        // A divider is 1px; a sixteenth of a rem at the default size.
        typeface + size + STYLE_REMS + color + alignment + 8. * gap + 4. * 0.0625 + 2. * gap
    }

    fn narrow(self) -> bool {
        self == Self::Narrow
    }
}

/// The form a bar `width` wide shows.
pub fn format_bar_fit(width: Pixels, rem: Pixels) -> FormatBarFit {
    FormatBarFit::ALL
        .into_iter()
        .find(|fit| width >= rem * fit.rems())
        .unwrap_or(FormatBarFit::Narrow)
}

pub struct EditPdfFormatBar {
    controls: Entity<EditPdfContentPropertyInspector>,
    /// The edited text's input, which a click on a control and Escape
    /// return the keyboard to.
    edit_focus: Option<FocusHandle>,
    focus_handle: FocusHandle,
    width: Rc<Cell<Pixels>>,
    _subscriptions: Vec<Subscription>,
}

impl EditPdfFormatBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let controls = cx.new(|cx| EditPdfContentPropertyInspector::new(window, cx));
        let font = controls.read(cx).font_select();
        let subscriptions = vec![
            cx.observe(&controls, |_, _, cx| cx.notify()),
            // A pick returns the keyboard to the text, even of the family
            // already shown (which changes nothing). Deferred: the list
            // focuses its own field again after it reports the pick.
            cx.subscribe_in(
                &font,
                window,
                |bar, _, event: &SelectEvent<SearchableVec<SharedString>>, window, cx| {
                    if let SelectEvent::Confirm(Some(_)) = event
                        && let Some(focus) = bar.edit_focus.clone()
                    {
                        window.defer(cx, move |window, cx| window.focus(&focus, cx));
                    }
                },
            ),
        ];
        Self {
            controls,
            edit_focus: None,
            focus_handle: cx.focus_handle(),
            width: Rc::new(Cell::new(Pixels::MAX)),
            _subscriptions: subscriptions,
        }
    }

    /// The control states, which emit the Properties panel's events.
    pub fn controls(&self) -> Entity<EditPdfContentPropertyInspector> {
        self.controls.clone()
    }

    /// What the bar shows: the edited text's properties, or `None` with no
    /// text being edited (the controls are then disabled).
    pub fn snapshot(&self, cx: &App) -> Option<EditPdfContentPropertySnapshot> {
        self.controls.read(cx).snapshot().cloned()
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// Shows `snapshot`, the edited text's properties, with `edit_focus`
    /// its input; `None` disables the controls.
    pub fn sync(
        &mut self,
        snapshot: Option<EditPdfContentPropertySnapshot>,
        edit_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_focus = edit_focus;
        let current = self.controls.read(cx).snapshot().cloned();
        if current == snapshot {
            return;
        }
        self.controls.update(cx, |controls, cx| match snapshot {
            Some(snapshot) => controls.sync(snapshot, window, cx),
            None => controls.clear(window, cx),
        });
        cx.notify();
    }

    /// Applies `patch` as the Properties panel control does, then returns
    /// the keyboard to the edited text, so typing goes on.
    fn apply(&mut self, patch: EditPdfContentPropertyPatch, window: &mut Window, cx: &mut App) {
        self.controls
            .update(cx, |controls, cx| controls.request_patch(patch, cx));
        self.return_to_text(window, cx);
    }

    fn return_to_text(&self, window: &mut Window, cx: &mut App) {
        if let Some(focus) = &self.edit_focus {
            window.focus(focus, cx);
        }
    }

    fn on_return_to_text(
        &mut self,
        _: &ReturnToEditedText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit_focus.is_none() {
            cx.propagate();
            return;
        }
        // A number typed but not committed goes back to the shown value,
        // so leaving the field commits nothing.
        self.controls
            .update(cx, |controls, cx| controls.restore_fields(window, cx));
        self.return_to_text(window, cx);
    }

    fn style_button(
        &self,
        id: &'static str,
        name: &'static str,
        key: char,
        icon: &'static str,
        on: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let modifier = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let shortcut = format!("{modifier}+{key}");
        let button = Button::new(id)
            .debug_selector(move || id.into())
            .accessibility_id(id)
            .icon(Icon::default().path(icon))
            .selected(on)
            .toggled(on)
            .disabled(disabled)
            .map(
                |button| match crate::shortcut_label::shortcut_kbd(&shortcut) {
                    Some(key_binding) => button.tooltip_with_key_binding(name, key_binding),
                    None => button.tooltip(name),
                },
            )
            .on_click(cx.listener(move |bar, _: &ClickEvent, window, cx| {
                let patch = match key {
                    'B' => EditPdfContentPropertyPatch::Bold(!on),
                    _ => EditPdfContentPropertyPatch::Italic(!on),
                };
                bar.apply(patch, window, cx);
            }));
        accessible_icon_button(button, name)
    }

    fn render_alignment(
        &self,
        fit: FormatBarFit,
        current: Option<TextBlockAlignment>,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if fit >= FormatBarFit::AlignmentMenu {
            let (_, _, name, _, icon) = ALIGNMENTS
                .iter()
                .find(|(value, ..)| Some(*value) == current)
                .copied()
                .unwrap_or(ALIGNMENTS[0]);
            let label: SharedString = match current {
                Some(_) => format!("Alignment: {name}").into(),
                None => "Alignment".into(),
            };
            let bar = cx.entity().downgrade();
            let button = accessible_icon_button(
                Button::new(EDIT_PDF_FORMAT_ALIGNMENT_MENU_ID)
                    .debug_selector(|| EDIT_PDF_FORMAT_ALIGNMENT_MENU_ID.into())
                    .accessibility_id(EDIT_PDF_FORMAT_ALIGNMENT_MENU_ID)
                    .outline()
                    // Compact: the narrowest form fits the narrowest viewer
                    // column only with the tighter padding.
                    .compact()
                    // A child, not `icon`: an icon alone makes a square
                    // button with no room for the caret.
                    .child(Icon::default().path(icon))
                    .dropdown_caret(true)
                    .tooltip(label.clone())
                    .disabled(disabled),
                label,
            )
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.check_side(gpui_component::Side::Right);
                for (value, _, name, _, _) in ALIGNMENTS {
                    let bar = bar.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name)
                            .checked(current == Some(value))
                            .on_click(move |_, window, cx| {
                                let _ = bar.update(cx, |bar, cx| {
                                    bar.apply(
                                        EditPdfContentPropertyPatch::Alignment(value),
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    );
                }
                menu
            });
            // In a slot of its counted room, as the other controls are.
            return div()
                .flex_none()
                .w(rems(ALIGNMENT_MENU_REMS))
                .child(button)
                .into_any_element();
        }
        ButtonGroup::new(EDIT_PDF_FORMAT_ALIGNMENT_GROUP_ID)
            .outline()
            // Disabled first: the group passes it on to the children added
            // after it.
            .disabled(disabled)
            .children(ALIGNMENTS.iter().map(|(value, name, _, id, icon)| {
                let value = *value;
                let id = *id;
                accessible_icon_button(
                    Button::new(id)
                        .debug_selector(move || id.into())
                        .accessibility_id(id)
                        .icon(Icon::default().path(*icon))
                        .tooltip(*name)
                        .disabled(disabled)
                        .selected(current == Some(value))
                        .toggled(current == Some(value))
                        .on_click(cx.listener(move |bar, _: &ClickEvent, window, cx| {
                            bar.apply(EditPdfContentPropertyPatch::Alignment(value), window, cx);
                        })),
                    *name,
                )
            }))
            .into_any_element()
    }
}

impl Render for EditPdfFormatBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controls = self.controls.read(cx);
        let snapshot = controls.snapshot().cloned();
        let font = controls.font_select();
        let size = controls.size_input();
        let color = controls.color_picker();
        let disabled = snapshot.is_none();
        let fit = format_bar_fit(self.width.get(), window.rem_size());
        let narrow = fit.narrow();

        let typeface = div()
            .id(EDIT_PDF_FORMAT_FONT_ID)
            .debug_selector(|| EDIT_PDF_FORMAT_FONT_ID.into())
            .flex_none()
            .w(rems(if narrow {
                NARROW_TYPEFACE_REMS
            } else {
                TYPEFACE_REMS
            }))
            .child(
                Select::new(&font)
                    .placeholder("Typeface")
                    .search_placeholder("Search fonts")
                    .disabled(disabled),
            );
        // No "pt" after the number, as in a word processor: the bar has no
        // room for it beside − and +. The tooltip names the unit. The stock
        // stepper takes no tooltip on its buttons, so a hover region over
        // each part names it: it shows the tooltip and takes no click.
        let size_tip = |id: &'static str, text: &'static str| {
            div()
                .id(id)
                .absolute()
                .top_0()
                .bottom_0()
                .tooltip(move |window, cx| Tooltip::new(text).build(window, cx))
        };
        let font_size = div()
            .id(EDIT_PDF_FORMAT_SIZE_SLOT_ID)
            .relative()
            .flex_none()
            .w(rems(if narrow { NARROW_SIZE_REMS } else { SIZE_REMS }))
            .child(PropertyStepInput::new(EDIT_PDF_FORMAT_SIZE_ID, &size).disabled(disabled))
            .children(SIZE_TOOLTIPS.map(|(id, text, part)| {
                let tip = size_tip(id, text);
                match part {
                    SizePart::Decrease => tip.left_0().w(rems(SIZE_BUTTON_REMS)),
                    SizePart::Field => tip
                        .left(rems(SIZE_BUTTON_REMS))
                        .right(rems(SIZE_BUTTON_REMS)),
                    SizePart::Increase => tip.right_0().w(rems(SIZE_BUTTON_REMS)),
                }
            }));
        let (bold, italic) = snapshot
            .as_ref()
            .map_or((false, false), |snapshot| (snapshot.bold, snapshot.italic));
        let style = ButtonGroup::new(EDIT_PDF_FORMAT_STYLE_GROUP_ID)
            .outline()
            .disabled(disabled)
            .child(self.style_button(
                EDIT_PDF_FORMAT_BOLD_ID,
                "Bold",
                'B',
                FORMAT_BOLD_ICON,
                bold,
                disabled,
                cx,
            ))
            .child(self.style_button(
                EDIT_PDF_FORMAT_ITALIC_ID,
                "Italic",
                'I',
                FORMAT_ITALIC_ICON,
                italic,
                disabled,
                cx,
            ));
        let text_colour = if narrow {
            // The swatch alone, which opens the palette as the full form's
            // swatch does.
            let swatch = if disabled {
                let value = color.read(cx).value();
                div()
                    .size_5()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().input)
                    .when_some(value, |swatch, value| swatch.bg(value))
                    .opacity(0.5)
                    .into_any_element()
            } else {
                // Ringed as the disabled swatch is, so a colour as dark or
                // as light as the bar still shows its edge.
                div()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().input)
                    .child(property_color_picker(&color).small())
                    .into_any_element()
            };
            h_flex()
                .id(EDIT_PDF_FORMAT_COLOR_ID)
                .debug_selector(|| EDIT_PDF_FORMAT_COLOR_ID.into())
                .flex_none()
                .w(rems(SWATCH_REMS))
                .justify_center()
                .child(swatch)
                .into_any_element()
        } else {
            div()
                .flex_none()
                .w(rems(COLOR_REMS))
                .child(
                    PropertyColor::new(EDIT_PDF_FORMAT_COLOR_ID, "Text colour", &color)
                        .disabled(disabled),
                )
                .into_any_element()
        };
        let alignment = self.render_alignment(
            fit,
            snapshot.as_ref().map(|snapshot| snapshot.alignment),
            disabled,
            cx,
        );

        let width = self.width.clone();
        let bar = cx.entity().downgrade();
        h_flex()
            .on_prepaint(move |bounds, window, cx| {
                // The form follows the width laid out.
                let rem = window.rem_size();
                let previous = format_bar_fit(width.replace(bounds.size.width), rem);
                if previous != format_bar_fit(bounds.size.width, rem) {
                    let bar = bar.clone();
                    window.defer(cx, move |_, cx| {
                        bar.update(cx, |_, cx| cx.notify()).ok();
                    });
                }
            })
            .id(EDIT_PDF_FORMAT_BAR_ID)
            .debug_selector(|| EDIT_PDF_FORMAT_BAR_ID.into())
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(Self::on_return_to_text))
            .track_focus(&self.focus_handle)
            .tab_group()
            .w_full()
            .min_w_0()
            .h(crate::layout_metrics::scaled(crate::layout_metrics::BAR))
            .flex_shrink_0()
            .bg(cx.theme().background)
            .relative()
            .child(crate::layout_metrics::inside_line(
                crate::layout_metrics::Edge::Bottom,
                cx.theme().border,
            ))
            .child(
                // Some form always fits the narrowest viewer column; the bar
                // clips only while a side panel slides in or out.
                h_flex()
                    .id(EDIT_PDF_FORMAT_BAR_CONTENT_ID)
                    .debug_selector(|| EDIT_PDF_FORMAT_BAR_CONTENT_ID.into())
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .map(|content| {
                        if narrow {
                            content.gap_1().px_1()
                        } else {
                            content.gap_2().px_2()
                        }
                    })
                    .children(crate::toolbar_group::divided(
                        DIVIDER_GROUP,
                        vec![
                            typeface.into_any_element(),
                            font_size.into_any_element(),
                            style.into_any_element(),
                            text_colour,
                            alignment,
                        ],
                        cx,
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_go_from_the_widest_to_the_narrowest() {
        let rem = gpui::px(16.);
        let widths = FormatBarFit::ALL.map(FormatBarFit::rems);
        assert!(
            widths.windows(2).all(|pair| pair[0] > pair[1]),
            "{widths:?}"
        );
        assert_eq!(
            format_bar_fit(rem * FormatBarFit::Full.rems(), rem),
            FormatBarFit::Full
        );
        assert_eq!(
            format_bar_fit(rem * FormatBarFit::Full.rems() - gpui::px(1.), rem),
            FormatBarFit::AlignmentMenu
        );
        assert_eq!(format_bar_fit(gpui::px(10.), rem), FormatBarFit::Narrow);
    }

    #[test]
    fn the_narrowest_form_fits_the_narrowest_viewer_toolbar() {
        assert!(
            FormatBarFit::Narrow.rems()
                <= crate::viewer_toolbar_strip::VIEWER_TOOLBAR_NARROWEST_REMS,
            "{} > {}",
            FormatBarFit::Narrow.rems(),
            crate::viewer_toolbar_strip::VIEWER_TOOLBAR_NARROWEST_REMS
        );
    }
}
