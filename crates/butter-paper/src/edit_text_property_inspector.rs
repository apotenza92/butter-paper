//! Text properties for the paragraph being edited with the Edit Text tool.
//!
//! The inspector owns only component state and colour preview. The workspace
//! owns the editor, which applies each change to the selection (or, with
//! none, the whole paragraph) as one undoable step.

use crate::pdf_text_blocks::TextBlockAlignment;
use crate::property_controls::{
    PropertyInspectorPanel, PropertyNumericInput, format_property_number, property_color_picker,
    property_pair,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, IndexPath, Selectable as _,
    accordion::Accordion,
    button::{Button, ButtonGroup},
    checkbox::Checkbox,
    color_picker::{ColorPickerEvent, ColorPickerState},
    form::Field,
    h_flex,
    input::{InputEvent, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};

pub const EDIT_TEXT_PROPERTY_INSPECTOR_ID: &str = "edit-text-property-inspector";
pub const EDIT_TEXT_INSPECTOR_FONT_ID: &str = "edit-text-property-font";
pub const EDIT_TEXT_INSPECTOR_SIZE_ID: &str = "edit-text-property-size";
pub const EDIT_TEXT_INSPECTOR_BOLD_ID: &str = "edit-text-property-bold";
pub const EDIT_TEXT_INSPECTOR_ITALIC_ID: &str = "edit-text-property-italic";
pub const EDIT_TEXT_INSPECTOR_COLOR_TRIGGER_ID: &str = "edit-text-property-color-trigger";
pub const EDIT_TEXT_INSPECTOR_APPLY_COLOR_ID: &str = "edit-text-property-apply-color";
pub const EDIT_TEXT_INSPECTOR_ALIGNMENT_ID: &str = "edit-text-property-alignment";
pub const EDIT_TEXT_INSPECTOR_LINE_SPACING_ID: &str = "edit-text-property-line-spacing";
pub const EDIT_TEXT_INSPECTOR_SCOPE_ID: &str = "edit-text-property-scope";
pub const EDIT_TEXT_INSPECTOR_SUBSTITUTES_ID: &str = "edit-text-property-substitutes";
const EDIT_TEXT_INSPECTOR_HEADER_ID: &str = "edit-text-property-inspector-header";
const EDIT_TEXT_INSPECTOR_SCROLL_ID: &str = "edit-text-property-inspector-scroll";

/// The font choice that keeps a run in its own page font.
pub const DOCUMENT_FONT_LABEL: &str = "Document font";
pub const EDIT_TEXT_INSPECTOR_ALIGN_LEFT_ID: &str = "edit-text-property-align-left";
pub const EDIT_TEXT_INSPECTOR_ALIGN_CENTER_ID: &str = "edit-text-property-align-center";
pub const EDIT_TEXT_INSPECTOR_ALIGN_RIGHT_ID: &str = "edit-text-property-align-right";
pub const EDIT_TEXT_INSPECTOR_ALIGN_JUSTIFY_ID: &str = "edit-text-property-align-justify";
const ALIGNMENTS: [(TextBlockAlignment, &str, &str); 4] = [
    (
        TextBlockAlignment::Left,
        "Left",
        EDIT_TEXT_INSPECTOR_ALIGN_LEFT_ID,
    ),
    (
        TextBlockAlignment::Center,
        "Center",
        EDIT_TEXT_INSPECTOR_ALIGN_CENTER_ID,
    ),
    (
        TextBlockAlignment::Right,
        "Right",
        EDIT_TEXT_INSPECTOR_ALIGN_RIGHT_ID,
    ),
    (
        TextBlockAlignment::Justified,
        "Justify",
        EDIT_TEXT_INSPECTOR_ALIGN_JUSTIFY_ID,
    ),
];
pub const MIN_FONT_SIZE: f64 = 1.;
pub const MAX_FONT_SIZE: f64 = 400.;
pub const MIN_LINE_SPACING: f64 = 0.5;
pub const MAX_LINE_SPACING: f64 = 5.;

/// A change from the inspector, and whether typing should return to the
/// text afterwards (not when the change came from leaving a field).
#[derive(Clone, Debug, PartialEq)]
pub struct EditTextPropertyEvent {
    pub patch: EditTextPropertyPatch,
    pub return_focus: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditTextPropertySnapshot {
    /// The family the text is set in; `None` for its document font.
    pub family: Option<String>,
    pub size: f64,
    /// DeviceRGB components.
    pub fill: [f64; 3],
    pub bold: bool,
    pub italic: bool,
    pub alignment: TextBlockAlignment,
    /// Baseline to baseline, as a multiple of the font size.
    pub line_spacing: f64,
    /// Fonts drawing characters the chosen ones lack.
    pub substitutes: Vec<String>,
    /// Changes apply to selected text rather than the whole paragraph.
    pub selection: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditTextPropertyPatch {
    /// `None` returns the text to its document font.
    Family(Option<String>),
    Size(f64),
    Fill([f64; 3]),
    Bold(bool),
    Italic(bool),
    Alignment(TextBlockAlignment),
    LineSpacing(f64),
}

pub struct EditTextPropertyInspector {
    snapshot: Option<EditTextPropertySnapshot>,
    syncing: bool,
    open_sections: [bool; 2],
    families: Vec<SharedString>,
    font: gpui::Entity<SelectState<SearchableVec<SharedString>>>,
    size: gpui::Entity<InputState>,
    line_spacing: gpui::Entity<InputState>,
    color: gpui::Entity<ColorPickerState>,
    _subscriptions: Vec<Subscription>,
}

fn rgb_color(fill: [f64; 3]) -> gpui::Hsla {
    gpui::Rgba {
        r: fill[0] as f32,
        g: fill[1] as f32,
        b: fill[2] as f32,
        a: 1.,
    }
    .into()
}

fn color_rgb(color: gpui::Hsla) -> [f64; 3] {
    let rgb = gpui::Rgba::from(color);
    [rgb.r, rgb.g, rgb.b].map(|component| (f64::from(component) * 255.).round() / 255.)
}

impl EditTextPropertyInspector {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let families: Vec<SharedString> = vec![DOCUMENT_FONT_LABEL.into()];
        let font = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(families.clone()),
                Some(IndexPath::default()),
                window,
                cx,
            )
            .searchable(true)
        });
        let size = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("12")
                .min(MIN_FONT_SIZE)
                .max(MAX_FONT_SIZE)
                .step(1.)
        });
        let line_spacing = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("1.2")
                .min(MIN_LINE_SPACING)
                .max(MAX_LINE_SPACING)
                .step(0.1)
        });
        let color =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb_color([0.; 3])));
        let subscriptions = vec![
            cx.subscribe_in(
                &font,
                window,
                |this, _, event: &SelectEvent<SearchableVec<SharedString>>, _, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    let family = (value.as_ref() != DOCUMENT_FONT_LABEL).then(|| value.to_string());
                    this.emit_patch(EditTextPropertyPatch::Family(family), true, cx);
                },
            ),
            cx.subscribe_in(&size, window, |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                if let Ok(value) = input.read(cx).value().trim().parse::<f64>()
                    && (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&value)
                {
                    let enter = matches!(event, InputEvent::PressEnter { .. });
                    this.emit_patch(EditTextPropertyPatch::Size(value), enter, cx);
                }
            }),
            cx.subscribe_in(
                &line_spacing,
                window,
                |this, input, event: &InputEvent, _, cx| {
                    if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    if let Ok(value) = input.read(cx).value().trim().parse::<f64>()
                        && (MIN_LINE_SPACING..=MAX_LINE_SPACING).contains(&value)
                    {
                        let enter = matches!(event, InputEvent::PressEnter { .. });
                        this.emit_patch(EditTextPropertyPatch::LineSpacing(value), enter, cx);
                    }
                },
            ),
            cx.subscribe(&color, |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(_) = event;
                if !this.syncing {
                    cx.notify();
                }
            }),
        ];
        // Installed families are listed once the fonts are scanned.
        let task = cx.background_spawn(async { crate::pdf_engine::text_face_families() });
        cx.spawn_in(window, async move |this, cx| {
            let families = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_families(families, window, cx)
            });
        })
        .detach();
        Self {
            snapshot: None,
            syncing: false,
            open_sections: [true, true],
            families,
            font,
            size,
            line_spacing,
            color,
            _subscriptions: subscriptions,
        }
    }

    fn set_families(&mut self, families: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.families = std::iter::once(DOCUMENT_FONT_LABEL.to_owned())
            .chain(families)
            .map(SharedString::from)
            .collect();
        let items = SearchableVec::new(self.families.clone());
        self.syncing = true;
        self.font.update(cx, |select, cx| {
            select.set_items(items, window, cx);
        });
        self.sync_font(window, cx);
        self.syncing = false;
        cx.notify();
    }

    fn sync_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label: SharedString = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.family.clone())
            .unwrap_or_else(|| DOCUMENT_FONT_LABEL.to_owned())
            .into();
        if !self.families.contains(&label) {
            // A family the scan has not listed (yet): show it all the same.
            self.families.push(label.clone());
            let items = SearchableVec::new(self.families.clone());
            self.font
                .update(cx, |select, cx| select.set_items(items, window, cx));
        }
        self.font.update(cx, |select, cx| {
            select.set_selected_value(&label, window, cx)
        });
    }

    pub fn snapshot(&self) -> Option<&EditTextPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.take().is_some() {
            cx.notify();
        }
    }

    pub fn sync(
        &mut self,
        snapshot: EditTextPropertySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        let previous = self.snapshot.replace(snapshot.clone());
        if previous.as_ref().map(|previous| &previous.family) != Some(&snapshot.family) {
            self.sync_font(window, cx);
        }
        if previous.as_ref().map(|previous| previous.size) != Some(snapshot.size) {
            self.size.update(cx, |input, cx| {
                input.set_value(format_property_number(snapshot.size), window, cx)
            });
        }
        if previous.as_ref().map(|previous| previous.line_spacing) != Some(snapshot.line_spacing) {
            self.line_spacing.update(cx, |input, cx| {
                input.set_value(
                    format!("{:.2}", snapshot.line_spacing)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_owned(),
                    window,
                    cx,
                )
            });
        }
        if previous.as_ref().map(|previous| previous.fill) != Some(snapshot.fill) {
            self.color.update(cx, |picker, cx| {
                picker.set_value(rgb_color(snapshot.fill), window, cx)
            });
        }
        self.syncing = false;
        cx.notify();
    }

    pub fn font_select(&self) -> gpui::Entity<SelectState<SearchableVec<SharedString>>> {
        self.font.clone()
    }

    pub fn size_input(&self) -> gpui::Entity<InputState> {
        self.size.clone()
    }

    pub fn line_spacing_input(&self) -> gpui::Entity<InputState> {
        self.line_spacing.clone()
    }

    pub fn color_picker(&self) -> gpui::Entity<ColorPickerState> {
        self.color.clone()
    }

    fn apply_preview_color(&mut self, cx: &mut Context<Self>) {
        let Some(color) = self.color.read(cx).value() else {
            return;
        };
        self.emit_patch(EditTextPropertyPatch::Fill(color_rgb(color)), true, cx);
    }

    fn emit_patch(
        &mut self,
        patch: EditTextPropertyPatch,
        return_focus: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let unchanged = match &patch {
            EditTextPropertyPatch::Family(family) => family == &snapshot.family,
            EditTextPropertyPatch::Size(size) => (size - snapshot.size).abs() < 1e-6,
            EditTextPropertyPatch::Fill(fill) => fill == &snapshot.fill,
            EditTextPropertyPatch::Bold(bold) => *bold == snapshot.bold,
            EditTextPropertyPatch::Italic(italic) => *italic == snapshot.italic,
            EditTextPropertyPatch::Alignment(alignment) => *alignment == snapshot.alignment,
            EditTextPropertyPatch::LineSpacing(spacing) => {
                (spacing - snapshot.line_spacing).abs() < 1e-6
            }
        };
        if self.syncing || unchanged {
            return;
        }
        cx.emit(EditTextPropertyEvent {
            patch,
            return_focus,
        });
    }
}

impl EventEmitter<EditTextPropertyEvent> for EditTextPropertyInspector {}

impl Render for EditTextPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(snapshot) = self.snapshot.clone() else {
            return div()
                .id(EDIT_TEXT_PROPERTY_INSPECTOR_ID)
                .hidden()
                .into_any_element();
        };
        let bold = cx.entity().downgrade();
        let italic = cx.entity().downgrade();
        let color = cx.entity().downgrade();
        let alignment = cx.entity().downgrade();
        let sections = cx.entity().downgrade();
        let muted = cx.theme().muted_foreground;
        let scope = if snapshot.selection {
            "Text changes apply to the selected text."
        } else {
            "Text changes apply to the whole paragraph."
        };
        let text = v_flex()
            .gap_3()
            .child(
                Field::new().label("Font").child(
                    div()
                        .id(EDIT_TEXT_INSPECTOR_FONT_ID)
                        .debug_selector(|| EDIT_TEXT_INSPECTOR_FONT_ID.into())
                        .child(Select::new(&self.font).search_placeholder("Search fonts")),
                ),
            )
            .child(property_pair(
                Field::new().label("Font size").child(
                    PropertyNumericInput::new(EDIT_TEXT_INSPECTOR_SIZE_ID, "Font size", &self.size)
                        .suffix("pt"),
                ),
                Field::new().label("Style").child(
                    h_flex()
                        .gap_3()
                        .h_full()
                        .child(
                            Checkbox::new(EDIT_TEXT_INSPECTOR_BOLD_ID)
                                .debug_selector(|| EDIT_TEXT_INSPECTOR_BOLD_ID.into())
                                .label("Bold")
                                .checked(snapshot.bold)
                                .on_click(move |value, _, cx| {
                                    let _ = bold.update(cx, |this, cx| {
                                        this.emit_patch(
                                            EditTextPropertyPatch::Bold(*value),
                                            true,
                                            cx,
                                        )
                                    });
                                }),
                        )
                        .child(
                            Checkbox::new(EDIT_TEXT_INSPECTOR_ITALIC_ID)
                                .debug_selector(|| EDIT_TEXT_INSPECTOR_ITALIC_ID.into())
                                .label("Italic")
                                .checked(snapshot.italic)
                                .on_click(move |value, _, cx| {
                                    let _ = italic.update(cx, |this, cx| {
                                        this.emit_patch(
                                            EditTextPropertyPatch::Italic(*value),
                                            true,
                                            cx,
                                        )
                                    });
                                }),
                        ),
                ),
            ))
            .child(
                Field::new().label("Text colour").child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .id(EDIT_TEXT_INSPECTOR_COLOR_TRIGGER_ID)
                                .debug_selector(|| EDIT_TEXT_INSPECTOR_COLOR_TRIGGER_ID.into())
                                .child(property_color_picker(&self.color, "Colour")),
                        )
                        .child(
                            Button::new(EDIT_TEXT_INSPECTOR_APPLY_COLOR_ID)
                                .debug_selector(|| EDIT_TEXT_INSPECTOR_APPLY_COLOR_ID.into())
                                .label("Apply colour")
                                .on_click(move |_, _, cx| {
                                    let _ =
                                        color.update(cx, |this, cx| this.apply_preview_color(cx));
                                }),
                        ),
                ),
            )
            .child(
                div()
                    .id(EDIT_TEXT_INSPECTOR_SCOPE_ID)
                    .debug_selector(|| EDIT_TEXT_INSPECTOR_SCOPE_ID.into())
                    .text_sm()
                    .text_color(muted)
                    .child(scope),
            )
            .when(!snapshot.substitutes.is_empty(), |text| {
                text.child(
                    div()
                        .id(EDIT_TEXT_INSPECTOR_SUBSTITUTES_ID)
                        .debug_selector(|| EDIT_TEXT_INSPECTOR_SUBSTITUTES_ID.into())
                        .text_sm()
                        .text_color(muted)
                        .child(format!(
                            "Characters the font lacks are drawn with {}.",
                            snapshot.substitutes.join(", ")
                        )),
                )
            });
        let paragraph = v_flex()
            .gap_3()
            .child(
                Field::new().label("Alignment").child(
                    div()
                        .id(EDIT_TEXT_INSPECTOR_ALIGNMENT_ID)
                        .debug_selector(|| EDIT_TEXT_INSPECTOR_ALIGNMENT_ID.into())
                        .child(
                            ButtonGroup::new("edit-text-property-alignment-group").children(
                                ALIGNMENTS.iter().map(|(value, label, id)| {
                                    let alignment = alignment.clone();
                                    let value = *value;
                                    Button::new(*id)
                                        .debug_selector(|| (*id).into())
                                        .label(*label)
                                        .selected(value == snapshot.alignment)
                                        .on_click(move |_, _, cx| {
                                            let _ = alignment.update(cx, |this, cx| {
                                                this.emit_patch(
                                                    EditTextPropertyPatch::Alignment(value),
                                                    true,
                                                    cx,
                                                )
                                            });
                                        })
                                }),
                            ),
                        ),
                ),
            )
            .child(
                Field::new().label("Line spacing").child(
                    PropertyNumericInput::new(
                        EDIT_TEXT_INSPECTOR_LINE_SPACING_ID,
                        "Line spacing",
                        &self.line_spacing,
                    )
                    .suffix("lines"),
                ),
            );
        let open = self.open_sections;
        let accordion = Accordion::new("edit-text-property-sections")
            .bordered(false)
            .multiple(true)
            .item(|item| item.open(open[0]).title("Text").child(text))
            .item(|item| item.open(open[1]).title("Paragraph").child(paragraph))
            .on_toggle_click(move |indexes, _, cx| {
                let _ = sections.update(cx, |this, cx| {
                    this.open_sections = [indexes.contains(&0), indexes.contains(&1)];
                    cx.notify();
                });
            });
        PropertyInspectorPanel::new(
            EDIT_TEXT_PROPERTY_INSPECTOR_ID,
            EDIT_TEXT_INSPECTOR_HEADER_ID,
            EDIT_TEXT_INSPECTOR_SCROLL_ID,
            "Edit Text",
        )
        .child(accordion)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_colours_round_to_whole_8_bit_components() {
        let fill = color_rgb(rgb_color([0.2, 0.4, 1.]));
        assert_eq!(fill, [51. / 255., 102. / 255., 1.]);
    }
}
