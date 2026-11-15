//! Text properties for the paragraph being edited with the Edit PDF Content tool.
//!
//! The inspector owns only component state and colour preview. The workspace
//! owns the editor, which applies each change to the selection (or, with
//! none, the whole paragraph) as one undoable step.

use crate::pdf_text_blocks::TextBlockAlignment;
use crate::property_controls::{
    PropertyColor, PropertyGroup, PropertyInspectorPanel, PropertyNumericInput,
    format_property_number, property_body, property_color_and_size, property_number, property_row,
    subscribe_property_color,
};
use gpui::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme as _, IndexPath, Selectable as _,
    button::{Button, ButtonGroup},
    checkbox::Checkbox,
    color_picker::ColorPickerState,
    h_flex,
    input::{InputEvent, InputState},
    select::{SearchableVec, Select, SelectEvent, SelectState},
};

pub const EDIT_PDF_CONTENT_PROPERTY_INSPECTOR_ID: &str = "edit-pdf-content-property-inspector";
pub const EDIT_PDF_CONTENT_INSPECTOR_FONT_ID: &str = "edit-pdf-content-property-font";
pub const EDIT_PDF_CONTENT_INSPECTOR_SIZE_ID: &str = "edit-pdf-content-property-size";
pub const EDIT_PDF_CONTENT_INSPECTOR_BOLD_ID: &str = "edit-pdf-content-property-bold";
pub const EDIT_PDF_CONTENT_INSPECTOR_ITALIC_ID: &str = "edit-pdf-content-property-italic";
pub const EDIT_PDF_CONTENT_INSPECTOR_COLOR_TRIGGER_ID: &str =
    "edit-pdf-content-property-color-trigger";
pub const EDIT_PDF_CONTENT_INSPECTOR_ALIGNMENT_ID: &str = "edit-pdf-content-property-alignment";
pub const EDIT_PDF_CONTENT_INSPECTOR_LINE_SPACING_ID: &str =
    "edit-pdf-content-property-line-spacing";
pub const EDIT_PDF_CONTENT_INSPECTOR_SCOPE_ID: &str = "edit-pdf-content-property-scope";
pub const EDIT_PDF_CONTENT_INSPECTOR_SUBSTITUTES_ID: &str = "edit-pdf-content-property-substitutes";
const EDIT_PDF_CONTENT_INSPECTOR_HEADER_ID: &str = "edit-pdf-content-property-inspector-header";
const EDIT_PDF_CONTENT_INSPECTOR_SCROLL_ID: &str = "edit-pdf-content-property-inspector-scroll";

/// The font choice that keeps a run in its own page font.
pub const DOCUMENT_FONT_LABEL: &str = "Document font";
pub const EDIT_PDF_CONTENT_INSPECTOR_ALIGN_LEFT_ID: &str = "edit-pdf-content-property-align-left";
pub const EDIT_PDF_CONTENT_INSPECTOR_ALIGN_CENTER_ID: &str =
    "edit-pdf-content-property-align-center";
pub const EDIT_PDF_CONTENT_INSPECTOR_ALIGN_RIGHT_ID: &str = "edit-pdf-content-property-align-right";
pub const EDIT_PDF_CONTENT_INSPECTOR_ALIGN_JUSTIFY_ID: &str =
    "edit-pdf-content-property-align-justify";
const ALIGNMENTS: [(TextBlockAlignment, &str, &str); 4] = [
    (
        TextBlockAlignment::Left,
        "Left",
        EDIT_PDF_CONTENT_INSPECTOR_ALIGN_LEFT_ID,
    ),
    (
        TextBlockAlignment::Center,
        "Centre",
        EDIT_PDF_CONTENT_INSPECTOR_ALIGN_CENTER_ID,
    ),
    (
        TextBlockAlignment::Right,
        "Right",
        EDIT_PDF_CONTENT_INSPECTOR_ALIGN_RIGHT_ID,
    ),
    (
        TextBlockAlignment::Justified,
        "Justify",
        EDIT_PDF_CONTENT_INSPECTOR_ALIGN_JUSTIFY_ID,
    ),
];
pub const MIN_FONT_SIZE: f64 = 1.;
pub const MAX_FONT_SIZE: f64 = 400.;
pub const MIN_LINE_SPACING: f64 = 0.5;
pub const MAX_LINE_SPACING: f64 = 5.;

/// A change from the inspector, and whether typing should return to the
/// text afterwards (not when the change came from leaving a field).
#[derive(Clone, Debug, PartialEq)]
pub struct EditPdfContentPropertyEvent {
    pub patch: EditPdfContentPropertyPatch,
    pub return_focus: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditPdfContentPropertySnapshot {
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
pub enum EditPdfContentPropertyPatch {
    /// `None` returns the text to its document font.
    Family(Option<String>),
    Size(f64),
    Fill([f64; 3]),
    Bold(bool),
    Italic(bool),
    Alignment(TextBlockAlignment),
    LineSpacing(f64),
}

pub struct EditPdfContentPropertyInspector {
    snapshot: Option<EditPdfContentPropertySnapshot>,
    syncing: bool,
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

impl EditPdfContentPropertyInspector {
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
        let mut subscriptions = vec![
            cx.subscribe_in(
                &font,
                window,
                |this, _, event: &SelectEvent<SearchableVec<SharedString>>, _, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    let family = (value.as_ref() != DOCUMENT_FONT_LABEL).then(|| value.to_string());
                    this.emit_patch(EditPdfContentPropertyPatch::Family(family), true, cx);
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
                    this.emit_patch(EditPdfContentPropertyPatch::Size(value), enter, cx);
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
                        this.emit_patch(EditPdfContentPropertyPatch::LineSpacing(value), enter, cx);
                    }
                },
            ),
        ];
        subscriptions.extend(subscribe_property_color(
            &color,
            cx,
            |this: &mut Self, cx| this.apply_preview_color(cx),
        ));
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

    pub fn snapshot(&self) -> Option<&EditPdfContentPropertySnapshot> {
        self.snapshot.as_ref()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.snapshot.take().is_some() {
            cx.notify();
        }
    }

    pub fn sync(
        &mut self,
        snapshot: EditPdfContentPropertySnapshot,
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
        self.emit_patch(
            EditPdfContentPropertyPatch::Fill(color_rgb(color)),
            true,
            cx,
        );
    }

    fn emit_patch(
        &mut self,
        patch: EditPdfContentPropertyPatch,
        return_focus: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let unchanged = match &patch {
            EditPdfContentPropertyPatch::Family(family) => family == &snapshot.family,
            EditPdfContentPropertyPatch::Size(size) => (size - snapshot.size).abs() < 1e-6,
            EditPdfContentPropertyPatch::Fill(fill) => fill == &snapshot.fill,
            EditPdfContentPropertyPatch::Bold(bold) => *bold == snapshot.bold,
            EditPdfContentPropertyPatch::Italic(italic) => *italic == snapshot.italic,
            EditPdfContentPropertyPatch::Alignment(alignment) => *alignment == snapshot.alignment,
            EditPdfContentPropertyPatch::LineSpacing(spacing) => {
                (spacing - snapshot.line_spacing).abs() < 1e-6
            }
        };
        if self.syncing || unchanged {
            return;
        }
        cx.emit(EditPdfContentPropertyEvent {
            patch,
            return_focus,
        });
    }
}

impl EventEmitter<EditPdfContentPropertyEvent> for EditPdfContentPropertyInspector {}

impl EditPdfContentPropertyInspector {
    /// The text and paragraph sections for the edited paragraph.
    fn text_sections(
        &self,
        snapshot: EditPdfContentPropertySnapshot,
        cx: &mut Context<Self>,
    ) -> (gpui::AnyElement, gpui::AnyElement) {
        let bold = cx.entity().downgrade();
        let italic = cx.entity().downgrade();
        let alignment = cx.entity().downgrade();
        let muted = cx.theme().muted_foreground;
        let scope = if snapshot.selection {
            "Changes apply to the selected text."
        } else {
            "Changes apply to the whole paragraph."
        };
        let mut text = PropertyGroup::titled("Text")
            .row(property_row(
                "Typeface",
                div()
                    .id(EDIT_PDF_CONTENT_INSPECTOR_FONT_ID)
                    .debug_selector(|| EDIT_PDF_CONTENT_INSPECTOR_FONT_ID.into())
                    .child(Select::new(&self.font).search_placeholder("Search fonts")),
            ))
            .row(property_row(
                "Font",
                property_color_and_size(
                    PropertyColor::new(
                        EDIT_PDF_CONTENT_INSPECTOR_COLOR_TRIGGER_ID,
                        "Text colour",
                        &self.color,
                    ),
                    PropertyNumericInput::new(
                        EDIT_PDF_CONTENT_INSPECTOR_SIZE_ID,
                        "Font size",
                        &self.size,
                    )
                    .suffix("pt"),
                ),
            ))
            .row(property_row(
                "Style",
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new(EDIT_PDF_CONTENT_INSPECTOR_BOLD_ID)
                            .debug_selector(|| EDIT_PDF_CONTENT_INSPECTOR_BOLD_ID.into())
                            .label("Bold")
                            .checked(snapshot.bold)
                            .on_click(move |value, _, cx| {
                                let _ = bold.update(cx, |this, cx| {
                                    this.emit_patch(
                                        EditPdfContentPropertyPatch::Bold(*value),
                                        true,
                                        cx,
                                    )
                                });
                            }),
                    )
                    .child(
                        Checkbox::new(EDIT_PDF_CONTENT_INSPECTOR_ITALIC_ID)
                            .debug_selector(|| EDIT_PDF_CONTENT_INSPECTOR_ITALIC_ID.into())
                            .label("Italic")
                            .checked(snapshot.italic)
                            .on_click(move |value, _, cx| {
                                let _ = italic.update(cx, |this, cx| {
                                    this.emit_patch(
                                        EditPdfContentPropertyPatch::Italic(*value),
                                        true,
                                        cx,
                                    )
                                });
                            }),
                    ),
            ))
            .row(
                div()
                    .id(EDIT_PDF_CONTENT_INSPECTOR_SCOPE_ID)
                    .debug_selector(|| EDIT_PDF_CONTENT_INSPECTOR_SCOPE_ID.into())
                    .text_xs()
                    .text_color(muted)
                    .child(scope),
            );
        if !snapshot.substitutes.is_empty() {
            text = text.row(
                div()
                    .id(EDIT_PDF_CONTENT_INSPECTOR_SUBSTITUTES_ID)
                    .debug_selector(|| EDIT_PDF_CONTENT_INSPECTOR_SUBSTITUTES_ID.into())
                    .text_xs()
                    .text_color(muted)
                    .child(format!(
                        "Characters the font lacks are drawn with {}.",
                        snapshot.substitutes.join(", ")
                    )),
            );
        }
        // Four alignments need the panel's full width.
        let paragraph = PropertyGroup::titled("Paragraph")
            .row(
                div()
                    .id(EDIT_PDF_CONTENT_INSPECTOR_ALIGNMENT_ID)
                    .debug_selector(|| EDIT_PDF_CONTENT_INSPECTOR_ALIGNMENT_ID.into())
                    .child(
                        ButtonGroup::new("edit-pdf-content-property-alignment-group")
                            .outline()
                            .w_full()
                            .children(ALIGNMENTS.iter().map(|(value, label, id)| {
                                let alignment = alignment.clone();
                                let value = *value;
                                Button::new(*id)
                                    .debug_selector(|| (*id).into())
                                    .flex_1()
                                    .label(*label)
                                    .selected(value == snapshot.alignment)
                                    .on_click(move |_, _, cx| {
                                        let _ = alignment.update(cx, |this, cx| {
                                            this.emit_patch(
                                                EditPdfContentPropertyPatch::Alignment(value),
                                                true,
                                                cx,
                                            )
                                        });
                                    })
                            })),
                    ),
            )
            .row(property_row(
                "Line spacing",
                property_number(
                    PropertyNumericInput::new(
                        EDIT_PDF_CONTENT_INSPECTOR_LINE_SPACING_ID,
                        "Line spacing",
                        &self.line_spacing,
                    )
                    .suffix("×"),
                ),
            ));
        (text.into_any_element(), paragraph.into_any_element())
    }
}

impl Render for EditPdfContentPropertyInspector {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = PropertyInspectorPanel::new(
            EDIT_PDF_CONTENT_PROPERTY_INSPECTOR_ID,
            EDIT_PDF_CONTENT_INSPECTOR_HEADER_ID,
            EDIT_PDF_CONTENT_INSPECTOR_SCROLL_ID,
            "Edit PDF",
        );
        let Some(snapshot) = self.snapshot.clone() else {
            return panel
                .child(
                    div()
                        .p_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Double-click text on the page to edit it and see its properties."),
                )
                .into_any_element();
        };
        let (text, paragraph) = self.text_sections(snapshot, cx);
        panel
            .child(property_body().child(text).child(paragraph))
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
