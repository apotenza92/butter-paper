//! Interactive form (AcroForm) fields: reading their values and widgets, and
//! writing filled values with new appearances.
//!
//! Butter Paper fills the fields a PDF already has. It does not create
//! fields, run form JavaScript, sign signature fields or fill XFA forms.

use crate::pdf_engine::{
    PdfPersistenceError, field_text_content, field_text_vertical_metrics, field_text_width_em,
};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const FIELD_READ_ONLY: i64 = 1;
const FIELD_REQUIRED: i64 = 1 << 1;
const TEXT_MULTILINE: i64 = 1 << 12;
const TEXT_PASSWORD: i64 = 1 << 13;
const TEXT_FILE_SELECT: i64 = 1 << 20;
const TEXT_COMB: i64 = 1 << 24;
const BUTTON_NO_TOGGLE_TO_OFF: i64 = 1 << 14;
const BUTTON_RADIO: i64 = 1 << 15;
const BUTTON_PUSH: i64 = 1 << 16;
const CHOICE_COMBO: i64 = 1 << 17;
const CHOICE_EDIT: i64 = 1 << 18;
const CHOICE_MULTI_SELECT: i64 = 1 << 21;
const ANNOTATION_HIDDEN: i64 = 1 << 1;
const ANNOTATION_NO_VIEW: i64 = 1 << 5;

/// The size automatically sized multiline text and list boxes start from.
const AUTO_FONT_SIZE: f64 = 12.;
const MIN_AUTO_FONT_SIZE: f64 = 4.;
/// Baseline to baseline, in multiples of the font size.
const LINE_SPACING: f64 = 1.15;
/// The highlight behind a list box's selected options (Acrobat's).
const LIST_SELECTION_FILL: &str = "0.6 0.756863 0.854902 rg";

/// The form a document's pages carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormLayer {
    pub fields: Vec<FormField>,
    /// Widgets in tab order, as `(field, widget)` indices.
    pub tab_order: Vec<(usize, usize)>,
    pub xfa: XfaForm,
}

/// How a form uses XFA, the XML form architecture Butter Paper does not
/// fill.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum XfaForm {
    #[default]
    None,
    /// XFA alongside AcroForm fields. Filling the fields drops the XFA,
    /// whose values would no longer match them.
    Hybrid,
    /// Only XFA describes the form: it has no fields to fill.
    Dynamic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormField {
    /// The fully qualified name, which identifies the field.
    pub name: String,
    /// The alternate name shown as the field's description (`/TU`).
    pub label: Option<String>,
    pub kind: FormFieldKind,
    pub read_only: bool,
    pub required: bool,
    pub value: FormValue,
    pub widgets: Vec<FormWidget>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormFieldKind {
    Text {
        multiline: bool,
        password: bool,
        comb: bool,
        max_len: Option<usize>,
    },
    CheckBox,
    Radio {
        /// Whether clicking the chosen button turns it off.
        toggles_off: bool,
    },
    PushButton,
    Combo {
        options: Vec<ChoiceOption>,
        editable: bool,
    },
    List {
        options: Vec<ChoiceOption>,
        multi_select: bool,
    },
    Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceOption {
    /// The value the field takes when it is chosen.
    pub export: String,
    pub display: String,
}

/// A field's value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormValue {
    Text(String),
    /// A check box or radio button's chosen on state; `None` when off.
    Button(Option<String>),
    /// A choice field's chosen values (one for a combo box).
    Choices(Vec<String>),
}

/// One place a field shows on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct FormWidget {
    pub page_index: u32,
    /// `[left, bottom, right, top]` in page user space.
    pub rect: [f64; 4],
    /// A check box or radio button's on state.
    pub on_state: Option<String>,
    /// Its position in its page's annotations.
    pub slot: usize,
    /// The text size its default appearance sets; zero fits the text.
    pub font_size: f64,
    /// Text alignment: 0 left, 1 centred, 2 right.
    pub quadding: i64,
}

/// A field's value changed from the value the document has.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FormFieldEdit {
    pub name: String,
    /// The value in the base document, which undoing a saved edit restores.
    pub original: FormValue,
    pub value: FormValue,
}

impl FormFieldEdit {
    pub fn is_unchanged(&self) -> bool {
        self.original == self.value
    }
}

impl FormField {
    /// Whether the user can change the field's value.
    pub fn fillable(&self) -> bool {
        !self.read_only
            && !matches!(
                self.kind,
                FormFieldKind::PushButton | FormFieldKind::Signature
            )
    }

    /// The text a value shows in the field: a choice's display text.
    pub fn display_text(&self, value: &FormValue) -> String {
        match (value, &self.kind) {
            (FormValue::Text(text), _) => text.clone(),
            (
                FormValue::Choices(values),
                FormFieldKind::Combo { options, .. } | FormFieldKind::List { options, .. },
            ) => values
                .iter()
                .map(|value| {
                    options
                        .iter()
                        .find(|option| &option.export == value)
                        .map_or_else(|| value.clone(), |option| option.display.clone())
                })
                .collect::<Vec<_>>()
                .join(", "),
            (FormValue::Choices(values), _) => values.join(", "),
            (FormValue::Button(state), _) => state.clone().unwrap_or_default(),
        }
    }

    /// The value after a click on one of its check box or radio widgets.
    pub fn toggled(&self, value: &FormValue, widget: usize) -> Option<FormValue> {
        let on = self.widgets.get(widget)?.on_state.clone()?;
        let FormValue::Button(current) = value else {
            return None;
        };
        let chosen = current.as_deref() == Some(on.as_str());
        Some(FormValue::Button(match &self.kind {
            FormFieldKind::CheckBox => (!chosen).then_some(on),
            FormFieldKind::Radio { toggles_off } => {
                if chosen && *toggles_off {
                    None
                } else {
                    Some(on)
                }
            }
            _ => return None,
        }))
    }
}

impl FormLayer {
    pub fn field(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|field| field.name == name)
    }
}

/// A field's value with the edits applied.
pub fn current_value(field: &FormField, edits: &[FormFieldEdit]) -> FormValue {
    edits
        .iter()
        .find(|edit| edit.name == field.name)
        .map_or_else(|| field.value.clone(), |edit| edit.value.clone())
}

/// Whether the bundled fonts can draw every character of a field's text.
pub fn text_is_drawable(text: &str) -> bool {
    text.split('\n')
        .all(|line| field_text_width_em(line, "Helvetica", false, false).is_some())
}

/// Rewrites a history state's edits once `saved` edits are in the file, so
/// they apply to the saved file as their new base.
pub fn rebase_form_edits(state: &[FormFieldEdit], saved: &[FormFieldEdit]) -> Vec<FormFieldEdit> {
    let mut rebased = state
        .iter()
        .filter(|edit| !saved.iter().any(|done| done.name == edit.name))
        .cloned()
        .collect::<Vec<_>>();
    for done in saved {
        let desired = state
            .iter()
            .find(|edit| edit.name == done.name)
            .map_or(&done.original, |edit| &edit.value);
        let edit = FormFieldEdit {
            name: done.name.clone(),
            original: done.value.clone(),
            value: desired.clone(),
        };
        if !edit.is_unchanged() {
            rebased.push(edit);
        }
    }
    rebased.sort_by(|a, b| a.name.cmp(&b.name));
    rebased
}

/// Reads the document's form fields. A document without a form, or with
/// one that cannot be read, has no fields.
pub fn extract_form_layer(document: &Document) -> FormLayer {
    let Some(form) = acro_form(document) else {
        return FormLayer::default();
    };
    let placements = widget_placements(document);
    let mut fields = Vec::new();
    for raw in collect_fields(document) {
        if let Some(field) = form_field(document, &raw, &placements) {
            fields.push(field);
        }
    }
    let xfa = if !form.has(b"XFA") {
        XfaForm::None
    } else if fields.is_empty() {
        XfaForm::Dynamic
    } else {
        XfaForm::Hybrid
    };
    let tab_order = tab_order(document, &fields);
    FormLayer {
        fields,
        tab_order,
        xfa,
    }
}

/// Writes the edited values into the document's fields and gives each of
/// their widgets a new appearance.
pub fn apply_form_edits(
    document: &mut Document,
    edits: &[FormFieldEdit],
) -> Result<(), PdfPersistenceError> {
    let edits = edits
        .iter()
        .filter(|edit| !edit.is_unchanged())
        .collect::<Vec<_>>();
    if edits.is_empty() {
        return Ok(());
    }
    let fonts = form_fonts(document);
    let raw_fields = collect_fields(document);
    for edit in edits {
        let matching = raw_fields
            .iter()
            .filter(|raw| raw.name == edit.name)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "form field {:?} is not in the document",
                edit.name
            )));
        }
        for raw in matching {
            write_field_value(document, raw, &edit.value, &fonts)?;
        }
    }
    // XFA keeps its own copy of the values, which no longer match.
    if let Some(form_id) = acro_form_id(document) {
        match form_id {
            FormLocation::Indirect(id) => {
                document.get_object_mut(id)?.as_dict_mut()?.remove(b"XFA");
            }
            FormLocation::Catalog => {
                let root = document.trailer.get(b"Root")?.as_reference()?;
                document
                    .get_object_mut(root)?
                    .as_dict_mut()?
                    .get_mut(b"AcroForm")?
                    .as_dict_mut()?
                    .remove(b"XFA");
            }
        }
    }
    Ok(())
}

/// Whether a field's widgets lack the appearances that show its value, so
/// a viewer must draw them (an unfilled form's AP-less fields, or a form
/// that asks viewers to regenerate them).
pub fn needs_appearances(document: &Document) -> bool {
    let Some(form) = acro_form(document) else {
        return false;
    };
    if form
        .get(b"NeedAppearances")
        .ok()
        .and_then(|value| resolve(document, value).as_bool().ok())
        == Some(true)
    {
        return true;
    }
    collect_fields(document).iter().any(|raw| {
        raw.widgets.iter().any(|widget| {
            document
                .get_dictionary(*widget)
                .is_ok_and(|widget| !widget.has(b"AP"))
        })
    })
}

/// Gives every text and choice field, and every check box or radio button
/// without one, an appearance of its value. For display only: a viewer
/// asked to regenerate appearances draws them.
pub fn regenerate_appearances(document: &mut Document) -> Result<(), PdfPersistenceError> {
    let regenerate_all = acro_form(document).is_some_and(|form| {
        form.get(b"NeedAppearances")
            .ok()
            .and_then(|value| resolve(document, value).as_bool().ok())
            == Some(true)
    });
    let fonts = form_fonts(document);
    for raw in collect_fields(document) {
        let missing = raw.widgets.iter().any(|widget| {
            document
                .get_dictionary(*widget)
                .is_ok_and(|widget| !widget.has(b"AP"))
        });
        if !(missing || regenerate_all) {
            continue;
        }
        let field_type = raw.inherited.field_type.as_deref();
        if field_type == Some(b"Btn".as_slice()) && !missing {
            continue;
        }
        let value = raw_value(document, &raw);
        // A field that cannot be drawn keeps its appearance.
        let _ = write_field_appearances(document, &raw, &value, &fonts);
    }
    Ok(())
}

// Reading the field tree.

#[derive(Clone, Debug, Default)]
struct Inherited {
    field_type: Option<Vec<u8>>,
    flags: i64,
    value: Option<Object>,
    default_appearance: Option<Vec<u8>>,
    quadding: i64,
    max_len: Option<i64>,
    options: Option<Object>,
}

impl Inherited {
    fn absorb(&mut self, document: &Document, node: &Dictionary) {
        let get = |key: &[u8]| node.get(key).ok().map(|value| resolve(document, value));
        if let Some(Object::Name(name)) = get(b"FT") {
            self.field_type = Some(name.clone());
        }
        if let Some(flags) = get(b"Ff").and_then(|value| value.as_i64().ok()) {
            self.flags = flags;
        }
        if let Some(value) = get(b"V") {
            self.value = Some(value.clone());
        }
        if let Some(appearance) = get(b"DA").and_then(|value| value.as_str().ok()) {
            self.default_appearance = Some(appearance.to_vec());
        }
        if let Some(quadding) = get(b"Q").and_then(|value| value.as_i64().ok()) {
            self.quadding = quadding;
        }
        if let Some(max_len) = get(b"MaxLen").and_then(|value| value.as_i64().ok()) {
            self.max_len = Some(max_len);
        }
        if let Some(options) = get(b"Opt") {
            self.options = Some(options.clone());
        }
    }
}

/// A terminal field: the node that holds its name and value, and its
/// widgets.
#[derive(Clone, Debug)]
struct RawField {
    name: String,
    node: ObjectId,
    widgets: Vec<ObjectId>,
    inherited: Inherited,
    label: Option<String>,
}

enum FormLocation {
    Indirect(ObjectId),
    Catalog,
}

fn catalog(document: &Document) -> Option<&Dictionary> {
    let root = document.trailer.get(b"Root").ok()?;
    resolve(document, root).as_dict().ok()
}

fn acro_form(document: &Document) -> Option<&Dictionary> {
    resolve(document, catalog(document)?.get(b"AcroForm").ok()?)
        .as_dict()
        .ok()
}

fn acro_form_id(document: &Document) -> Option<FormLocation> {
    match catalog(document)?.get(b"AcroForm").ok()? {
        Object::Reference(id) => Some(FormLocation::Indirect(*id)),
        Object::Dictionary(_) => Some(FormLocation::Catalog),
        _ => None,
    }
}

fn resolve<'a>(document: &'a Document, mut object: &'a Object) -> &'a Object {
    for _ in 0..16 {
        match object {
            Object::Reference(id) => match document.get_object(*id) {
                Ok(target) => object = target,
                Err(_) => return &Object::Null,
            },
            _ => return object,
        }
    }
    &Object::Null
}

fn text_of(document: &Document, object: &Object) -> Option<String> {
    match resolve(document, object) {
        object @ Object::String(..) => {
            Some(lopdf::decode_text_string(object).unwrap_or_else(|_| {
                String::from_utf8_lossy(object.as_str().unwrap_or_default()).into_owned()
            }))
        }
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        Object::Stream(stream) => {
            let content = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
            Some(
                lopdf::decode_text_string(&Object::string_literal(content.clone()))
                    .unwrap_or_else(|_| String::from_utf8_lossy(&content).into_owned()),
            )
        }
        _ => None,
    }
}

fn number(document: &Document, object: &Object) -> Option<f64> {
    match resolve(document, object) {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn reference_list(document: &Document, object: Option<&Object>) -> Vec<ObjectId> {
    object
        .and_then(|object| resolve(document, object).as_array().ok())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_reference().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn is_widget(dictionary: &Dictionary) -> bool {
    dictionary
        .get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        == Some(b"Widget".as_slice())
}

fn collect_fields(document: &Document) -> Vec<RawField> {
    let Some(form) = acro_form(document) else {
        return Vec::new();
    };
    let mut root = Inherited::default();
    root.absorb(document, form);
    // Only the form-wide defaults are inherited from the form dictionary.
    root.field_type = None;
    root.flags = 0;
    root.value = None;
    root.max_len = None;
    root.options = None;
    let mut fields = Vec::new();
    let mut seen = BTreeSet::new();
    for id in reference_list(document, form.get(b"Fields").ok()) {
        walk_field(document, id, "", &root, &mut seen, &mut fields, 0);
    }
    fields
}

fn walk_field(
    document: &Document,
    id: ObjectId,
    parent_name: &str,
    inherited: &Inherited,
    seen: &mut BTreeSet<ObjectId>,
    fields: &mut Vec<RawField>,
    depth: usize,
) {
    if depth > 32 || !seen.insert(id) {
        return;
    }
    let Ok(node) = document.get_dictionary(id) else {
        return;
    };
    let name = match node.get(b"T").ok().and_then(|t| text_of(document, t)) {
        Some(partial) if parent_name.is_empty() => partial,
        Some(partial) => format!("{parent_name}.{partial}"),
        None => parent_name.to_owned(),
    };
    let mut inherited = inherited.clone();
    inherited.absorb(document, node);
    let label = node.get(b"TU").ok().and_then(|tu| text_of(document, tu));
    let kids = reference_list(document, node.get(b"Kids").ok());
    if kids.is_empty() {
        fields.push(RawField {
            name,
            node: id,
            widgets: if is_widget(node) {
                vec![id]
            } else {
                Vec::new()
            },
            inherited,
            label,
        });
        return;
    }
    let mut widgets = Vec::new();
    for kid in kids {
        let Ok(dictionary) = document.get_dictionary(kid) else {
            continue;
        };
        if is_widget(dictionary) && !dictionary.has(b"T") && !dictionary.has(b"Kids") {
            if seen.insert(kid) {
                widgets.push(kid);
            }
        } else {
            walk_field(document, kid, &name, &inherited, seen, fields, depth + 1);
        }
    }
    if !widgets.is_empty() {
        fields.push(RawField {
            name,
            node: id,
            widgets,
            inherited,
            label,
        });
    }
}

/// Each widget's page and slot in the page's `/Annots`.
fn widget_placements(document: &Document) -> HashMap<ObjectId, (u32, usize)> {
    let mut placements = HashMap::new();
    for (number, page_id) in document.get_pages() {
        let Ok(page) = document.get_dictionary(page_id) else {
            continue;
        };
        let annotations = page
            .get(b"Annots")
            .ok()
            .and_then(|value| resolve(document, value).as_array().ok());
        for (slot, annotation) in annotations.into_iter().flatten().enumerate() {
            if let Ok(id) = annotation.as_reference() {
                placements
                    .entry(id)
                    .or_insert((number.saturating_sub(1), slot));
            }
        }
    }
    placements
}

fn form_field(
    document: &Document,
    raw: &RawField,
    placements: &HashMap<ObjectId, (u32, usize)>,
) -> Option<FormField> {
    let flags = raw.inherited.flags;
    let kind = match raw.inherited.field_type.as_deref()? {
        b"Tx" => FormFieldKind::Text {
            multiline: flags & TEXT_MULTILINE != 0,
            password: flags & TEXT_PASSWORD != 0,
            comb: flags & TEXT_COMB != 0
                && flags & (TEXT_MULTILINE | TEXT_PASSWORD | TEXT_FILE_SELECT) == 0
                && raw.inherited.max_len.is_some_and(|max_len| max_len > 0),
            max_len: raw
                .inherited
                .max_len
                .and_then(|max_len| usize::try_from(max_len).ok())
                .filter(|max_len| *max_len > 0),
        },
        b"Btn" if flags & BUTTON_PUSH != 0 => FormFieldKind::PushButton,
        b"Btn" if flags & BUTTON_RADIO != 0 => FormFieldKind::Radio {
            toggles_off: flags & BUTTON_NO_TOGGLE_TO_OFF == 0,
        },
        b"Btn" => FormFieldKind::CheckBox,
        b"Ch" => {
            let options = choice_options(document, raw.inherited.options.as_ref());
            if flags & CHOICE_COMBO != 0 {
                FormFieldKind::Combo {
                    options,
                    editable: flags & CHOICE_EDIT != 0,
                }
            } else {
                FormFieldKind::List {
                    options,
                    multi_select: flags & CHOICE_MULTI_SELECT != 0,
                }
            }
        }
        b"Sig" => FormFieldKind::Signature,
        _ => return None,
    };
    let mut widgets = Vec::new();
    for id in &raw.widgets {
        let Some((page_index, slot)) = placements.get(id) else {
            continue;
        };
        let Ok(widget) = document.get_dictionary(*id) else {
            continue;
        };
        let annotation_flags = widget
            .get(b"F")
            .ok()
            .and_then(|value| resolve(document, value).as_i64().ok())
            .unwrap_or(0);
        if annotation_flags & (ANNOTATION_HIDDEN | ANNOTATION_NO_VIEW) != 0 {
            continue;
        }
        let Some(rect) = widget_rect(document, widget) else {
            continue;
        };
        let on_state = matches!(kind, FormFieldKind::CheckBox | FormFieldKind::Radio { .. })
            .then(|| widget_on_state(document, widget));
        let appearance = widget
            .get(b"DA")
            .ok()
            .and_then(|da| resolve(document, da).as_str().ok())
            .map(<[u8]>::to_vec)
            .or_else(|| raw.inherited.default_appearance.clone())
            .unwrap_or_default();
        let quadding = widget
            .get(b"Q")
            .ok()
            .and_then(|q| resolve(document, q).as_i64().ok())
            .unwrap_or(raw.inherited.quadding);
        widgets.push(FormWidget {
            page_index: *page_index,
            rect,
            on_state,
            slot: *slot,
            font_size: DefaultAppearance::parse(&appearance).size.max(0.),
            quadding,
        });
    }
    if widgets.is_empty() {
        return None;
    }
    Some(FormField {
        name: raw.name.clone(),
        label: raw.label.clone(),
        read_only: flags & FIELD_READ_ONLY != 0
            || (matches!(kind, FormFieldKind::Text { .. }) && flags & TEXT_FILE_SELECT != 0),
        required: flags & FIELD_REQUIRED != 0,
        value: raw_value(document, raw),
        kind,
        widgets,
    })
}

fn raw_value(document: &Document, raw: &RawField) -> FormValue {
    let value = raw.inherited.value.as_ref();
    match raw.inherited.field_type.as_deref() {
        Some(b"Btn") => {
            let chosen = value
                .and_then(|value| text_of(document, value))
                .or_else(|| {
                    // No value: the state a widget shows.
                    raw.widgets.iter().find_map(|widget| {
                        let widget = document.get_dictionary(*widget).ok()?;
                        text_of(document, widget.get(b"AS").ok()?)
                    })
                })
                .filter(|state| !state.is_empty() && state != "Off");
            FormValue::Button(chosen)
        }
        Some(b"Ch") => FormValue::Choices(match value.map(|value| resolve(document, value)) {
            Some(Object::Array(items)) => items
                .iter()
                .filter_map(|item| text_of(document, item))
                .collect(),
            Some(value) => text_of(document, value)
                .filter(|value| !value.is_empty())
                .into_iter()
                .collect(),
            None => Vec::new(),
        }),
        _ => FormValue::Text(
            value
                .and_then(|value| text_of(document, value))
                .unwrap_or_default()
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
        ),
    }
}

fn choice_options(document: &Document, options: Option<&Object>) -> Vec<ChoiceOption> {
    let Some(Object::Array(items)) = options.map(|options| resolve(document, options)) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match resolve(document, item) {
            Object::Array(pair) if pair.len() >= 2 => Some(ChoiceOption {
                export: text_of(document, &pair[0])?,
                display: text_of(document, &pair[1])?,
            }),
            item => {
                let text = text_of(document, item)?;
                Some(ChoiceOption {
                    export: text.clone(),
                    display: text,
                })
            }
        })
        .collect()
}

fn widget_rect(document: &Document, widget: &Dictionary) -> Option<[f64; 4]> {
    let rect = resolve(document, widget.get(b"Rect").ok()?)
        .as_array()
        .ok()?;
    if rect.len() != 4 {
        return None;
    }
    let values = rect
        .iter()
        .map(|value| number(document, value))
        .collect::<Option<Vec<_>>>()?;
    let rect = [
        values[0].min(values[2]),
        values[1].min(values[3]),
        values[0].max(values[2]),
        values[1].max(values[3]),
    ];
    (rect.iter().all(|value| value.is_finite()) && rect[2] > rect[0] && rect[3] > rect[1])
        .then_some(rect)
}

fn widget_on_state(document: &Document, widget: &Dictionary) -> String {
    let appearance_states = |key: &[u8]| -> Option<String> {
        let appearances = resolve(document, widget.get(b"AP").ok()?).as_dict().ok()?;
        let states = resolve(document, appearances.get(key).ok()?)
            .as_dict()
            .ok()?;
        states
            .iter()
            .map(|(state, _)| String::from_utf8_lossy(state).into_owned())
            .find(|state| state != "Off")
    };
    appearance_states(b"N")
        .or_else(|| appearance_states(b"D"))
        .or_else(|| {
            widget
                .get(b"AS")
                .ok()
                .and_then(|state| text_of(document, state))
                .filter(|state| !state.is_empty() && state != "Off")
        })
        .unwrap_or_else(|| "Yes".into())
}

/// Widgets in the order Tab visits them: page by page, each page in its
/// `/Tabs` order (rows, columns, or its annotation order).
fn tab_order(document: &Document, fields: &[FormField]) -> Vec<(usize, usize)> {
    let mut order = Vec::new();
    let pages = document.get_pages();
    let mut by_page: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    for (field_index, field) in fields.iter().enumerate() {
        for (widget_index, widget) in field.widgets.iter().enumerate() {
            by_page
                .entry(widget.page_index)
                .or_default()
                .push((field_index, widget_index));
        }
    }
    for (page_index, mut widgets) in by_page {
        let tabs = pages
            .get(&(page_index + 1))
            .and_then(|page| document.get_dictionary(*page).ok())
            .and_then(|page| page.get(b"Tabs").ok())
            .and_then(|tabs| resolve(document, tabs).as_name().ok())
            .map(<[u8]>::to_vec);
        let rect = |(field, widget): &(usize, usize)| fields[*field].widgets[*widget].rect;
        match tabs.as_deref() {
            Some(b"R") => widgets.sort_by(|a, b| {
                (-rect(a)[3])
                    .total_cmp(&-rect(b)[3])
                    .then(rect(a)[0].total_cmp(&rect(b)[0]))
            }),
            Some(b"C") => widgets.sort_by(|a, b| {
                rect(a)[0]
                    .total_cmp(&rect(b)[0])
                    .then((-rect(a)[3]).total_cmp(&-rect(b)[3]))
            }),
            _ => widgets.sort_by_key(|(field, widget)| fields[*field].widgets[*widget].slot),
        }
        order.extend(widgets);
    }
    order
}

// Writing values.

/// A PDF text string: literal for plain ASCII, else UTF-16 (which also
/// keeps line breaks, which literal strings may not carry intact).
fn text_string(text: &str) -> Object {
    if text
        .chars()
        .all(|character| character.is_ascii() && !character.is_ascii_control())
    {
        Object::string_literal(text)
    } else {
        let mut bytes = Vec::with_capacity(2 + text.encode_utf16().count() * 2);
        lopdf::encode_utf16_be(text, &mut bytes);
        Object::String(bytes, lopdf::StringFormat::Hexadecimal)
    }
}

fn write_field_value(
    document: &mut Document,
    raw: &RawField,
    value: &FormValue,
    fonts: &BTreeMap<Vec<u8>, String>,
) -> Result<(), PdfPersistenceError> {
    let stored = match value {
        FormValue::Text(text) => text_string(text),
        FormValue::Button(state) => Object::Name(state.as_deref().unwrap_or("Off").into()),
        FormValue::Choices(values) => {
            if raw.inherited.flags & CHOICE_MULTI_SELECT != 0 && values.len() != 1 {
                Object::Array(values.iter().map(|value| text_string(value)).collect())
            } else {
                values
                    .first()
                    .map_or_else(|| text_string(""), |value| text_string(value))
            }
        }
    };
    let options = choice_options(document, raw.inherited.options.as_ref());
    let node = document.get_object_mut(raw.node)?.as_dict_mut()?;
    node.set("V", stored);
    if let FormValue::Choices(values) = value {
        // List boxes keep the chosen options' indices beside their values.
        let indices = values
            .iter()
            .filter_map(|value| options.iter().position(|option| &option.export == value))
            .collect::<BTreeSet<_>>();
        if indices.is_empty() || raw.inherited.flags & CHOICE_COMBO != 0 {
            node.remove(b"I");
        } else {
            node.set(
                "I",
                Object::Array(
                    indices
                        .into_iter()
                        .map(|index| Object::Integer(index as i64))
                        .collect(),
                ),
            );
        }
    }
    write_field_appearances(document, raw, value, fonts)
}

fn write_field_appearances(
    document: &mut Document,
    raw: &RawField,
    value: &FormValue,
    fonts: &BTreeMap<Vec<u8>, String>,
) -> Result<(), PdfPersistenceError> {
    let flags = raw.inherited.flags;
    for widget_id in &raw.widgets {
        let Ok(widget) = document.get_dictionary(*widget_id).cloned() else {
            continue;
        };
        let Some(rect) = widget_rect(document, &widget) else {
            continue;
        };
        let look = WidgetLook::read(document, &widget, raw, fonts);
        match value {
            FormValue::Button(state) => {
                let on = widget_on_state(document, &widget);
                let shown = if state.as_deref() == Some(on.as_str()) {
                    on.clone()
                } else {
                    "Off".into()
                };
                let has_states = widget
                    .get(b"AP")
                    .ok()
                    .and_then(|ap| resolve(document, ap).as_dict().ok())
                    .and_then(|ap| ap.get(b"N").ok())
                    .and_then(|normal| resolve(document, normal).as_dict().ok())
                    .is_some_and(|states| states.has(on.as_bytes()));
                if !has_states {
                    let radio = flags & BUTTON_RADIO != 0;
                    let appearances = button_appearances(document, &look, rect, &on, radio)?;
                    document
                        .get_object_mut(*widget_id)?
                        .as_dict_mut()?
                        .set("AP", appearances);
                }
                document
                    .get_object_mut(*widget_id)?
                    .as_dict_mut()?
                    .set("AS", Object::Name(shown.into_bytes()));
            }
            FormValue::Text(_) | FormValue::Choices(_) => {
                let content = if flags & CHOICE_COMBO == 0
                    && raw.inherited.field_type.as_deref() == Some(b"Ch")
                {
                    let FormValue::Choices(chosen) = value else {
                        unreachable!()
                    };
                    let options = choice_options(document, raw.inherited.options.as_ref());
                    let top_index = widget
                        .get(b"TI")
                        .ok()
                        .and_then(|ti| resolve(document, ti).as_i64().ok())
                        .or_else(|| {
                            document
                                .get_dictionary(raw.node)
                                .ok()?
                                .get(b"TI")
                                .ok()?
                                .as_i64()
                                .ok()
                        })
                        .unwrap_or(0)
                        .max(0) as usize;
                    list_box_content(document, &look, rect, &options, chosen, top_index)?
                } else {
                    let text = match value {
                        FormValue::Text(text) => text.clone(),
                        FormValue::Choices(chosen) => {
                            let options = choice_options(document, raw.inherited.options.as_ref());
                            chosen
                                .first()
                                .map(|chosen| {
                                    options
                                        .iter()
                                        .find(|option| &option.export == chosen)
                                        .map_or_else(
                                            || chosen.clone(),
                                            |option| option.display.clone(),
                                        )
                                })
                                .unwrap_or_default()
                        }
                        FormValue::Button(_) => unreachable!(),
                    };
                    let text_layout = if raw.inherited.field_type.as_deref() == Some(b"Tx") {
                        TextLayout {
                            multiline: flags & TEXT_MULTILINE != 0,
                            password: flags & TEXT_PASSWORD != 0,
                            comb: (flags & TEXT_COMB != 0
                                && flags & (TEXT_MULTILINE | TEXT_PASSWORD | TEXT_FILE_SELECT)
                                    == 0)
                                .then_some(raw.inherited.max_len)
                                .flatten()
                                .and_then(|max_len| usize::try_from(max_len).ok())
                                .filter(|max_len| *max_len > 0),
                        }
                    } else {
                        TextLayout::default()
                    };
                    text_field_content(document, &look, rect, &text, text_layout)?
                };
                let stream = document.add_object(content);
                document
                    .get_object_mut(*widget_id)?
                    .as_dict_mut()?
                    .set("AP", dictionary! { "N" => stream });
            }
        }
    }
    Ok(())
}

/// The fonts of the form's default resources (`/DR`): resource name to base
/// font name.
fn form_fonts(document: &Document) -> BTreeMap<Vec<u8>, String> {
    acro_form(document).map_or_else(BTreeMap::new, |form| {
        resource_fonts(document, form.get(b"DR").ok())
    })
}

/// The fonts of a resource dictionary: resource name to base font name.
fn resource_fonts(document: &Document, resources: Option<&Object>) -> BTreeMap<Vec<u8>, String> {
    let Some(fonts) = resources
        .and_then(|resources| resolve(document, resources).as_dict().ok())
        .and_then(|resources| resources.get(b"Font").ok())
        .and_then(|fonts| resolve(document, fonts).as_dict().ok())
    else {
        return BTreeMap::new();
    };
    fonts
        .iter()
        .filter_map(|(name, font)| {
            let font = resolve(document, font).as_dict().ok()?;
            let base = font.get(b"BaseFont").ok()?.as_name().ok()?;
            Some((name.clone(), String::from_utf8_lossy(base).into_owned()))
        })
        .collect()
}

/// How a widget draws: its default appearance, colours, border and
/// rotation.
#[derive(Clone, Debug)]
struct WidgetLook {
    family: &'static str,
    bold: bool,
    italic: bool,
    /// Zero sizes the text to fit.
    font_size: f64,
    text_color: String,
    background: Option<String>,
    border: Option<(String, f64, BorderStyle)>,
    rotation: i64,
    quadding: i64,
    /// A check box's mark (`/MK /CA`), in ZapfDingbats.
    caption: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum BorderStyle {
    Solid,
    Dashed(Vec<f64>),
    Underline,
}

impl WidgetLook {
    fn read(
        document: &Document,
        widget: &Dictionary,
        raw: &RawField,
        fonts: &BTreeMap<Vec<u8>, String>,
    ) -> Self {
        let appearance = widget
            .get(b"DA")
            .ok()
            .and_then(|da| resolve(document, da).as_str().ok())
            .map(<[u8]>::to_vec)
            .or_else(|| raw.inherited.default_appearance.clone())
            .unwrap_or_default();
        let parsed = DefaultAppearance::parse(&appearance);
        // The widget's own resources (some producers keep them there), then
        // the form's.
        let own_fonts = resource_fonts(document, widget.get(b"DR").ok());
        let base = parsed
            .font
            .as_ref()
            .and_then(|name| {
                own_fonts
                    .get(name.as_slice())
                    .or_else(|| fonts.get(name.as_slice()))
                    .cloned()
            })
            .or_else(|| {
                parsed
                    .font
                    .as_ref()
                    .map(|name| String::from_utf8_lossy(name).into_owned())
            })
            .unwrap_or_default();
        let (family, bold, italic) = font_family(&base);
        let characteristics = widget
            .get(b"MK")
            .ok()
            .and_then(|mk| resolve(document, mk).as_dict().ok());
        let color = |key: &[u8], fill: bool| {
            characteristics
                .and_then(|mk| mk.get(key).ok())
                .and_then(|color| resolve(document, color).as_array().ok())
                .and_then(|components| color_operator(document, components, fill))
        };
        let border_style = widget
            .get(b"BS")
            .ok()
            .and_then(|bs| resolve(document, bs).as_dict().ok());
        let border_width = border_style
            .and_then(|bs| bs.get(b"W").ok())
            .and_then(|width| number(document, width))
            .or_else(|| {
                widget
                    .get(b"Border")
                    .ok()
                    .and_then(|border| resolve(document, border).as_array().ok())
                    .and_then(|border| border.get(2))
                    .and_then(|width| number(document, width))
            })
            .unwrap_or(1.);
        let style = match border_style
            .and_then(|bs| bs.get(b"S").ok())
            .and_then(|style| resolve(document, style).as_name().ok())
        {
            Some(b"D") => BorderStyle::Dashed(
                border_style
                    .and_then(|bs| bs.get(b"D").ok())
                    .and_then(|dash| resolve(document, dash).as_array().ok())
                    .map(|dash| {
                        dash.iter()
                            .filter_map(|value| number(document, value))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![3.]),
            ),
            Some(b"U") => BorderStyle::Underline,
            _ => BorderStyle::Solid,
        };
        let border = color(b"BC", false)
            .filter(|_| border_width > 0.)
            .map(|color| (color, border_width, style));
        let rotation = characteristics
            .and_then(|mk| mk.get(b"R").ok())
            .and_then(|rotation| resolve(document, rotation).as_i64().ok())
            .unwrap_or(0)
            .rem_euclid(360);
        let quadding = widget
            .get(b"Q")
            .ok()
            .and_then(|q| resolve(document, q).as_i64().ok())
            .unwrap_or(raw.inherited.quadding);
        Self {
            family,
            bold,
            italic,
            font_size: parsed.size.max(0.),
            text_color: parsed.color.unwrap_or_else(|| "0 g".into()),
            background: color(b"BG", true),
            border,
            rotation: rotation - rotation % 90,
            quadding,
            caption: characteristics
                .and_then(|mk| mk.get(b"CA").ok())
                .and_then(|caption| text_of(document, caption))
                .filter(|caption| !caption.is_empty()),
        }
    }

    fn border_width(&self) -> f64 {
        self.border.as_ref().map_or(0., |(_, width, _)| *width)
    }

    /// The width and height the appearance is laid out in, before rotation.
    fn content_size(&self, rect: [f64; 4]) -> (f64, f64) {
        let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
        if self.rotation % 180 == 90 {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// The background and border.
    fn frame(&self, width: f64, height: f64) -> String {
        let mut content = String::new();
        if let Some(fill) = &self.background {
            content.push_str(&format!("{fill}\n0 0 {width:.4} {height:.4} re f\n"));
        }
        if let Some((color, line, style)) = &self.border {
            let half = line / 2.;
            content.push_str(&format!("{color}\n{line:.4} w\n"));
            match style {
                BorderStyle::Underline => {
                    content.push_str(&format!("0 {half:.4} m {width:.4} {half:.4} l S\n"))
                }
                BorderStyle::Dashed(dash) => {
                    let dash = dash
                        .iter()
                        .map(|value| format!("{value:.4}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    content.push_str(&format!(
                        "[{dash}] 0 d\n{half:.4} {half:.4} {:.4} {:.4} re S\n[] 0 d\n",
                        width - line,
                        height - line
                    ));
                }
                BorderStyle::Solid => content.push_str(&format!(
                    "{half:.4} {half:.4} {:.4} {:.4} re S\n",
                    width - line,
                    height - line
                )),
            }
        }
        content
    }

    fn appearance(&self, rect: [f64; 4], content: Vec<u8>, resources: Dictionary) -> Stream {
        let (width, height) = self.content_size(rect);
        let mut dictionary = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), Object::Real(width as f32), Object::Real(height as f32)],
            "Resources" => resources,
        };
        let (rect_width, rect_height) = (rect[2] - rect[0], rect[3] - rect[1]);
        let matrix = match self.rotation {
            90 => Some([0., 1., -1., 0., rect_width, 0.]),
            180 => Some([-1., 0., 0., -1., width, height]),
            270 => Some([0., -1., 1., 0., 0., rect_height]),
            _ => None,
        };
        if let Some(matrix) = matrix {
            dictionary.set(
                "Matrix",
                matrix
                    .iter()
                    .map(|value| Object::Real(*value as f32))
                    .collect::<Vec<_>>(),
            );
        }
        Stream::new(dictionary, content)
    }
}

/// A field's default appearance string: its font, size and colour.
#[derive(Debug, Default, PartialEq)]
struct DefaultAppearance {
    font: Option<Vec<u8>>,
    size: f64,
    color: Option<String>,
}

impl DefaultAppearance {
    fn parse(appearance: &[u8]) -> Self {
        let text = String::from_utf8_lossy(appearance);
        let mut parsed = Self::default();
        let mut operands: Vec<&str> = Vec::new();
        for token in text.split_whitespace() {
            match token {
                "Tf" if operands.len() >= 2 => {
                    let name = operands[operands.len() - 2];
                    parsed.font = name.strip_prefix('/').map(|name| name.as_bytes().to_vec());
                    parsed.size = operands[operands.len() - 1].parse().unwrap_or(0.);
                    operands.clear();
                }
                "g" | "rg" | "k" => {
                    let count = match token {
                        "g" => 1,
                        "rg" => 3,
                        _ => 4,
                    };
                    if operands.len() >= count
                        && operands[operands.len() - count..]
                            .iter()
                            .all(|value| value.parse::<f64>().is_ok())
                    {
                        parsed.color = Some(format!(
                            "{} {token}",
                            operands[operands.len() - count..].join(" ")
                        ));
                    }
                    operands.clear();
                }
                token
                    if token
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == '*' || c == '\'' || c == '"') =>
                {
                    operands.clear();
                }
                operand => operands.push(operand),
            }
        }
        parsed
    }
}

/// The bundled family that stands in for a field's font, and its style.
fn font_family(base: &str) -> (&'static str, bool, bool) {
    let lower = base.to_ascii_lowercase();
    let bold = lower.contains("bold") || lower.ends_with("bo") || lower.ends_with("bi");
    let italic = lower.contains("italic")
        || lower.contains("oblique")
        || lower.ends_with("it")
        || lower.ends_with("bi");
    let family = if lower.contains("times") || lower.starts_with("tiro") || lower.contains("serif")
    {
        if lower.contains("sans") {
            "Helvetica"
        } else {
            "Tinos"
        }
    } else if lower.contains("courier") || lower.starts_with("cour") || lower.contains("mono") {
        "Roboto Mono"
    } else {
        "Helvetica"
    };
    (family, bold, italic)
}

fn color_operator(document: &Document, components: &[Object], fill: bool) -> Option<String> {
    let values = components
        .iter()
        .map(|value| number(document, value))
        .collect::<Option<Vec<_>>>()?;
    let operator = match (values.len(), fill) {
        (1, true) => "g",
        (1, false) => "G",
        (3, true) => "rg",
        (3, false) => "RG",
        (4, true) => "k",
        (4, false) => "K",
        _ => return None,
    };
    let values = values
        .iter()
        .map(|value| format!("{value:.4}"))
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("{values} {operator}"))
}

#[derive(Clone, Copy, Debug, Default)]
struct TextLayout {
    multiline: bool,
    password: bool,
    /// The number of cells of a comb field.
    comb: Option<usize>,
}

fn text_width(text: &str, look: &WidgetLook, size: f64) -> Result<f64, PdfPersistenceError> {
    field_text_width_em(text, look.family, look.bold, look.italic)
        .map(|width| width * size)
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "no available font can draw a character of this field's text".into(),
            )
        })
}

/// A text field's or combo box's appearance.
fn text_field_content(
    document: &mut Document,
    look: &WidgetLook,
    rect: [f64; 4],
    text: &str,
    layout: TextLayout,
) -> Result<Stream, PdfPersistenceError> {
    let (width, height) = look.content_size(rect);
    let border = look.border_width();
    let padding = border + 2.;
    let inner_height = (height - border * 2.).max(0.);
    let (ascent, descent) = field_text_vertical_metrics(look.family, look.bold, look.italic);
    let text = if layout.password {
        "*".repeat(text.chars().count())
    } else {
        text.to_owned()
    };
    let mut lines: Vec<(String, f64, f64)> = Vec::new();
    let size;
    if let Some(cells) = layout.comb {
        let cell = width / cells as f64;
        size = if look.font_size > 0. {
            look.font_size
        } else {
            ((inner_height - 2.) / (ascent + descent))
                .min(cell / 0.75)
                .max(MIN_AUTO_FONT_SIZE)
        };
        let baseline = border + (inner_height - (ascent + descent) * size) / 2. + descent * size;
        for (index, character) in text.chars().take(cells).enumerate() {
            let character = character.to_string();
            let advance = text_width(&character, look, size)?;
            lines.push((
                character,
                cell * index as f64 + (cell - advance) / 2.,
                baseline,
            ));
        }
    } else if layout.multiline {
        let available = (width - padding * 2.).max(1.);
        let fits = |size: f64| -> Result<Vec<String>, PdfPersistenceError> {
            wrap_text(&text, available, |line| text_width(line, look, size))
        };
        size = if look.font_size > 0. {
            look.font_size
        } else {
            let mut candidate = AUTO_FONT_SIZE;
            loop {
                let wrapped = fits(candidate)?;
                if candidate <= MIN_AUTO_FONT_SIZE
                    || wrapped.len() as f64 * candidate * LINE_SPACING <= inner_height - 2.
                {
                    break candidate;
                }
                candidate -= 0.5;
            }
        };
        let mut baseline = height - border - 2. - ascent * size;
        for line in fits(size)? {
            let line_width = text_width(&line, look, size)?;
            lines.push((
                line,
                aligned_x(look.quadding, width, padding, line_width),
                baseline,
            ));
            baseline -= size * LINE_SPACING;
        }
    } else {
        let line = text.replace('\n', " ");
        size = if look.font_size > 0. {
            look.font_size
        } else {
            let fitted = ((inner_height - 2.) / (ascent + descent)).max(MIN_AUTO_FONT_SIZE);
            let line_width = text_width(&line, look, fitted)?;
            let available = width - padding * 2.;
            if line_width > available && line_width > 0. {
                (fitted * available / line_width).max(MIN_AUTO_FONT_SIZE)
            } else {
                fitted
            }
        };
        let line_width = text_width(&line, look, size)?;
        let baseline = border + (inner_height - (ascent + descent) * size) / 2. + descent * size;
        lines.push((
            line,
            aligned_x(look.quadding, width, padding, line_width),
            baseline,
        ));
    }
    let mut content = look.frame(width, height).into_bytes();
    if let (Some(cells), Some((color, line, _))) = (layout.comb, &look.border) {
        // A comb's cells are divided as its border is drawn.
        let cell = width / cells as f64;
        content.extend_from_slice(format!("{color}\n{line:.4} w\n").as_bytes());
        for index in 1..cells {
            let x = cell * index as f64;
            content.extend_from_slice(format!("{x:.4} 0 m {x:.4} {height:.4} l S\n").as_bytes());
        }
    }
    content.extend_from_slice(b"/Tx BMC\nq\n");
    content.extend_from_slice(
        format!(
            "{border:.4} {border:.4} {:.4} {:.4} re W n\n{}\n",
            (width - border * 2.).max(0.),
            inner_height,
            look.text_color
        )
        .as_bytes(),
    );
    let lines = lines
        .into_iter()
        .filter(|(line, _, _)| !line.is_empty())
        .collect::<Vec<_>>();
    let (text_content, fonts) =
        field_text_content(document, &lines, look.family, look.bold, look.italic, size)?;
    content.extend_from_slice(&text_content);
    content.extend_from_slice(b"Q\nEMC\n");
    Ok(look.appearance(rect, content, dictionary! { "Font" => fonts }))
}

fn aligned_x(quadding: i64, width: f64, padding: f64, line_width: f64) -> f64 {
    match quadding {
        1 => (width - line_width) / 2.,
        2 => width - padding - line_width,
        _ => padding,
    }
}

/// Breaks text into lines no wider than `available`: at its line breaks,
/// between words, and within a word too long for a line.
fn wrap_text(
    text: &str,
    available: f64,
    mut measure: impl FnMut(&str) -> Result<f64, PdfPersistenceError>,
) -> Result<Vec<String>, PdfPersistenceError> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_inclusive(' ') {
            let candidate = format!("{line}{word}");
            if measure(candidate.trim_end())? <= available || line.is_empty() {
                line = candidate;
            } else {
                lines.push(line.trim_end().to_owned());
                line = word.to_owned();
            }
            // A word wider than the line is broken between characters.
            while measure(line.trim_end())? > available && line.chars().count() > 1 {
                let mut head = String::new();
                for character in line.chars() {
                    let next = format!("{head}{character}");
                    if !head.is_empty() && measure(&next)? > available {
                        break;
                    }
                    head = next;
                }
                let rest = line[head.len()..].to_owned();
                lines.push(head);
                line = rest;
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    Ok(lines)
}

/// A list box's appearance: its options from the top one shown, the chosen
/// ones highlighted.
fn list_box_content(
    document: &mut Document,
    look: &WidgetLook,
    rect: [f64; 4],
    options: &[ChoiceOption],
    chosen: &[String],
    top_index: usize,
) -> Result<Stream, PdfPersistenceError> {
    let (width, height) = look.content_size(rect);
    let border = look.border_width();
    let padding = border + 2.;
    let size = if look.font_size > 0. {
        look.font_size
    } else {
        AUTO_FONT_SIZE
    };
    let (ascent, descent) = field_text_vertical_metrics(look.family, look.bold, look.italic);
    let row = size * LINE_SPACING;
    let mut content = look.frame(width, height);
    content.push_str("/Tx BMC\nq\n");
    content.push_str(&format!(
        "{border:.4} {border:.4} {:.4} {:.4} re W n\n",
        (width - border * 2.).max(0.),
        (height - border * 2.).max(0.)
    ));
    // The first chosen option shows when the top index would hide it.
    let first_chosen = options
        .iter()
        .position(|option| chosen.contains(&option.export));
    let visible_rows = (((height - border * 2.) / row).floor() as usize).max(1);
    let top = match first_chosen {
        Some(index) if index < top_index || index >= top_index + visible_rows => index,
        _ => top_index,
    }
    .min(options.len().saturating_sub(1));
    let mut lines = Vec::new();
    let mut top_y = height - border;
    for option in options.iter().skip(top) {
        if top_y <= border {
            break;
        }
        if chosen.contains(&option.export) {
            content.push_str(&format!(
                "{LIST_SELECTION_FILL}\n{border:.4} {:.4} {:.4} {row:.4} re f\n",
                top_y - row,
                (width - border * 2.).max(0.)
            ));
        }
        let baseline = top_y - (row - (ascent + descent) * size) / 2. - ascent * size;
        lines.push((option.display.clone(), padding, baseline));
        top_y -= row;
    }
    content.push_str(&format!("{}\n", look.text_color));
    let mut content = content.into_bytes();
    let lines = lines
        .into_iter()
        .filter(|(line, _, _)| !line.is_empty())
        .collect::<Vec<_>>();
    let (text_content, fonts) =
        field_text_content(document, &lines, look.family, look.bold, look.italic, size)?;
    content.extend_from_slice(&text_content);
    content.extend_from_slice(b"Q\nEMC\n");
    Ok(look.appearance(rect, content, dictionary! { "Font" => fonts }))
}

/// On and off appearances for a check box or radio button that has none.
fn button_appearances(
    document: &mut Document,
    look: &WidgetLook,
    rect: [f64; 4],
    on: &str,
    radio: bool,
) -> Result<Dictionary, PdfPersistenceError> {
    let (width, height) = look.content_size(rect);
    let frame = look.frame(width, height);
    // ZapfDingbats: a check mark, or a filled circle for a radio button.
    let mark = look
        .caption
        .clone()
        .unwrap_or_else(|| if radio { "l".into() } else { "4".into() });
    let size = if look.font_size > 0. {
        look.font_size
    } else {
        (width.min(height) - look.border_width() * 2.) * 0.8
    };
    let mark_width = size * 0.75;
    let on_content = format!(
        "{frame}q\n{}\nBT\n/ZaDb {size:.4} Tf\n1 0 0 1 {:.4} {:.4} Tm\n({}) Tj\nET\nQ\n",
        look.text_color,
        (width - mark_width) / 2.,
        (height - size * 0.7) / 2.,
        mark.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)"),
    );
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "ZapfDingbats",
    });
    let on_stream = document.add_object(look.appearance(
        rect,
        on_content.into_bytes(),
        dictionary! { "Font" => dictionary! { "ZaDb" => font } },
    ));
    let off_stream =
        document.add_object(look.appearance(rect, frame.into_bytes(), Dictionary::new()));
    let mut states = Dictionary::new();
    states.set(on.as_bytes().to_vec(), on_stream);
    states.set("Off", off_stream);
    Ok(dictionary! { "N" => states })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::Content;

    /// A one-page form with one field of each kind.
    fn sample_form() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let mut annots = Vec::new();
        let mut fields = Vec::new();
        let mut widget = |document: &mut Document, extra: Dictionary, rect: [i64; 4]| {
            let mut dictionary = dictionary! {
                "Type" => "Annot",
                "Subtype" => "Widget",
                "Rect" => rect.iter().map(|value| Object::Integer(*value)).collect::<Vec<_>>(),
                "P" => page_id,
                "MK" => dictionary! { "BC" => vec![0.into()], "BG" => vec![1.into()] },
            };
            dictionary.extend(&extra);
            let id = document.add_object(dictionary);
            annots.push(Object::Reference(id));
            id
        };
        let name = widget(
            &mut document,
            dictionary! { "FT" => "Tx", "T" => lopdf::text_string("Name"), "V" => lopdf::text_string("Ada"), "DA" => lopdf::text_string("/Helv 0 Tf 0 g") },
            [50, 700, 250, 720],
        );
        fields.push(Object::Reference(name));
        let notes = widget(
            &mut document,
            dictionary! { "FT" => "Tx", "T" => lopdf::text_string("Notes"), "Ff" => TEXT_MULTILINE, "DA" => lopdf::text_string("/TiRo 10 Tf 0 0 1 rg") },
            [50, 600, 250, 680],
        );
        fields.push(Object::Reference(notes));
        let code = widget(
            &mut document,
            dictionary! { "FT" => "Tx", "T" => lopdf::text_string("Code"), "Ff" => TEXT_COMB, "MaxLen" => 4, "Q" => 1 },
            [50, 560, 130, 580],
        );
        fields.push(Object::Reference(code));
        let agree_on = document.add_object(Stream::new(Dictionary::new(), b"0 g".to_vec()));
        let agree_off = document.add_object(Stream::new(Dictionary::new(), Vec::new()));
        let agree = widget(
            &mut document,
            dictionary! { "FT" => "Btn", "T" => lopdf::text_string("Agree"), "V" => "Off", "AS" => "Off",
            "AP" => dictionary! { "N" => dictionary! { "Yes" => agree_on, "Off" => agree_off } } },
            [50, 520, 62, 532],
        );
        fields.push(Object::Reference(agree));
        // A radio group whose buttons have no appearances.
        let size_id = document.new_object_id();
        let small = widget(
            &mut document,
            dictionary! { "Parent" => size_id },
            [50, 480, 62, 492],
        );
        let large = widget(
            &mut document,
            dictionary! { "Parent" => size_id },
            [80, 480, 92, 492],
        );
        for (id, state) in [(small, "S"), (large, "L")] {
            let off = document.add_object(Stream::new(Dictionary::new(), Vec::new()));
            let on = document.add_object(Stream::new(Dictionary::new(), b"0 g".to_vec()));
            let mut states = Dictionary::new();
            states.set(state, on);
            states.set("Off", off);
            document
                .get_object_mut(id)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("AP", dictionary! { "N" => states });
        }
        document.objects.insert(
            size_id,
            Object::Dictionary(dictionary! {
                "FT" => "Btn", "T" => lopdf::text_string("Size"),
                "Ff" => BUTTON_RADIO | BUTTON_NO_TOGGLE_TO_OFF, "V" => "S",
                "Kids" => vec![small.into(), large.into()],
            }),
        );
        fields.push(Object::Reference(size_id));
        let colour = widget(
            &mut document,
            dictionary! { "FT" => "Ch", "T" => lopdf::text_string("Colour"), "Ff" => CHOICE_COMBO,
            "Opt" => vec![
                Object::Array(vec![lopdf::text_string("r"), lopdf::text_string("Red")]),
                Object::Array(vec![lopdf::text_string("g"), lopdf::text_string("Green")]),
            ], "V" => lopdf::text_string("r"), "DA" => lopdf::text_string("/Helv 9 Tf 0 g") },
            [50, 440, 150, 456],
        );
        fields.push(Object::Reference(colour));
        let fruit = widget(
            &mut document,
            dictionary! { "FT" => "Ch", "T" => lopdf::text_string("Fruit"), "Ff" => CHOICE_MULTI_SELECT,
            "Opt" => vec![lopdf::text_string("Apple"), lopdf::text_string("Pear"), lopdf::text_string("Plum")],
            "DA" => lopdf::text_string("/Helv 0 Tf 0 g") },
            [50, 360, 150, 420],
        );
        fields.push(Object::Reference(fruit));
        // A read-only field nested under a parent.
        let address_id = document.new_object_id();
        let street = widget(
            &mut document,
            dictionary! { "FT" => "Tx", "T" => lopdf::text_string("Street"), "Parent" => address_id, "Ff" => FIELD_READ_ONLY, "V" => lopdf::text_string("1 Main St") },
            [50, 320, 250, 340],
        );
        document.objects.insert(
            address_id,
            Object::Dictionary(
                dictionary! { "T" => lopdf::text_string("Address"), "Kids" => vec![street.into()] },
            ),
        );
        fields.push(Object::Reference(address_id));
        let helv = document.add_object(
            dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
        );
        let times = document.add_object(
            dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Times-Roman" },
        );
        let form = document.add_object(dictionary! {
            "Fields" => fields,
            "DA" => lopdf::text_string("/Helv 0 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => helv, "TiRo" => times } },
            "XFA" => lopdf::text_string("<xdp/>"),
        });
        let contents = document.add_object(Stream::new(Dictionary::new(), Vec::new()));
        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page", "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => contents, "Annots" => annots,
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
            ),
        );
        let catalog = document.add_object(
            dictionary! { "Type" => "Catalog", "Pages" => pages_id, "AcroForm" => form },
        );
        document.trailer.set("Root", catalog);
        document
    }

    fn roundtrip(document: &mut Document) -> Document {
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        Document::load_mem(&bytes).unwrap()
    }

    fn appearance_text(document: &Document, field: &str) -> String {
        let raw = collect_fields(document)
            .into_iter()
            .find(|raw| raw.name == field)
            .unwrap();
        let widget = document.get_dictionary(raw.widgets[0]).unwrap();
        let normal = widget
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap()
            .as_reference()
            .unwrap();
        let stream = document.get_object(normal).unwrap().as_stream().unwrap();
        String::from_utf8_lossy(
            &stream
                .decompressed_content()
                .unwrap_or(stream.content.clone()),
        )
        .into_owned()
    }

    #[test]
    fn reads_each_kind_of_field_with_its_value() {
        let layer = extract_form_layer(&sample_form());
        assert_eq!(layer.xfa, XfaForm::Hybrid);
        let names = layer
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "Name",
                "Notes",
                "Code",
                "Agree",
                "Size",
                "Colour",
                "Fruit",
                "Address.Street"
            ]
        );
        let name = layer.field("Name").unwrap();
        assert_eq!(name.value, FormValue::Text("Ada".into()));
        assert!(name.fillable());
        assert_eq!(name.widgets[0].rect, [50., 700., 250., 720.]);
        assert!(matches!(
            layer.field("Notes").unwrap().kind,
            FormFieldKind::Text {
                multiline: true,
                ..
            }
        ));
        assert!(matches!(
            layer.field("Code").unwrap().kind,
            FormFieldKind::Text {
                comb: true,
                max_len: Some(4),
                ..
            }
        ));
        assert_eq!(layer.field("Agree").unwrap().value, FormValue::Button(None));
        let size = layer.field("Size").unwrap();
        assert_eq!(size.value, FormValue::Button(Some("S".into())));
        assert_eq!(
            size.widgets
                .iter()
                .map(|widget| widget.on_state.clone().unwrap())
                .collect::<Vec<_>>(),
            ["S", "L"]
        );
        let colour = layer.field("Colour").unwrap();
        assert_eq!(colour.display_text(&colour.value), "Red");
        assert!(matches!(
            layer.field("Fruit").unwrap().kind,
            FormFieldKind::List {
                multi_select: true,
                ..
            }
        ));
        let street = layer.field("Address.Street").unwrap();
        assert!(street.read_only && !street.fillable());
        assert_eq!(layer.tab_order.len(), 9);
        assert_eq!(layer.tab_order[0], (0, 0));
    }

    #[test]
    fn check_boxes_toggle_and_radios_stay_chosen() {
        let layer = extract_form_layer(&sample_form());
        let agree = layer.field("Agree").unwrap();
        let on = agree.toggled(&agree.value, 0).unwrap();
        assert_eq!(on, FormValue::Button(Some("Yes".into())));
        assert_eq!(agree.toggled(&on, 0).unwrap(), FormValue::Button(None));
        let size = layer.field("Size").unwrap();
        assert_eq!(
            size.toggled(&size.value, 1).unwrap(),
            FormValue::Button(Some("L".into()))
        );
        // No toggle to off: clicking the chosen button keeps it chosen.
        assert_eq!(size.toggled(&size.value, 0).unwrap(), size.value);
    }

    #[test]
    fn writes_values_and_appearances_that_read_back() {
        let mut document = sample_form();
        let edits = [
            FormFieldEdit {
                name: "Name".into(),
                original: FormValue::Text("Ada".into()),
                value: FormValue::Text("Grace Hopper".into()),
            },
            FormFieldEdit {
                name: "Notes".into(),
                original: FormValue::Text(String::new()),
                value: FormValue::Text(
                    "First line\nSecond line that is long enough to wrap in the box".into(),
                ),
            },
            FormFieldEdit {
                name: "Code".into(),
                original: FormValue::Text(String::new()),
                value: FormValue::Text("AB12".into()),
            },
            FormFieldEdit {
                name: "Agree".into(),
                original: FormValue::Button(None),
                value: FormValue::Button(Some("Yes".into())),
            },
            FormFieldEdit {
                name: "Size".into(),
                original: FormValue::Button(Some("S".into())),
                value: FormValue::Button(Some("L".into())),
            },
            FormFieldEdit {
                name: "Colour".into(),
                original: FormValue::Choices(vec!["r".into()]),
                value: FormValue::Choices(vec!["g".into()]),
            },
            FormFieldEdit {
                name: "Fruit".into(),
                original: FormValue::Choices(Vec::new()),
                value: FormValue::Choices(vec!["Apple".into(), "Plum".into()]),
            },
        ];
        apply_form_edits(&mut document, &edits).unwrap();
        let reopened = roundtrip(&mut document);
        let layer = extract_form_layer(&reopened);
        for edit in &edits {
            assert_eq!(
                layer.field(&edit.name).unwrap().value,
                edit.value,
                "{}",
                edit.name
            );
        }
        // The XFA copy of the values is dropped.
        assert!(!acro_form(&reopened).unwrap().has(b"XFA"));
        assert_eq!(layer.xfa, XfaForm::None);
        let name = appearance_text(&reopened, "Name");
        assert!(name.contains("/Tx BMC"), "{name}");
        assert!(name.contains("/ActualText <FEFF0047"), "{name}");
        let notes = appearance_text(&reopened, "Notes");
        assert!(
            notes.contains("0 0 1 rg"),
            "the default appearance's colour: {notes}"
        );
        assert!(
            notes.contains("/Tinos"),
            "Times stands in as Tinos: {notes}"
        );
        assert!(
            Content::decode(notes.as_bytes())
                .unwrap()
                .operations
                .iter()
                .filter(|operation| operation.operator == "BDC")
                .count()
                >= 3,
            "the notes wrap onto at least three lines"
        );
        let code = appearance_text(&reopened, "Code");
        assert_eq!(
            code.matches("BDC").count(),
            4,
            "a comb draws a span per cell"
        );
        // The radio buttons show the chosen state.
        let raw = collect_fields(&reopened)
            .into_iter()
            .find(|raw| raw.name == "Size")
            .unwrap();
        let states = raw
            .widgets
            .iter()
            .map(|widget| {
                reopened
                    .get_dictionary(*widget)
                    .unwrap()
                    .get(b"AS")
                    .unwrap()
                    .as_name()
                    .unwrap()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        assert_eq!(states, [b"Off".to_vec(), b"L".to_vec()]);
        let fruit = collect_fields(&reopened)
            .into_iter()
            .find(|raw| raw.name == "Fruit")
            .unwrap();
        let indices = reopened
            .get_dictionary(fruit.node)
            .unwrap()
            .get(b"I")
            .unwrap();
        assert_eq!(indices, &Object::Array(vec![0.into(), 2.into()]));
        let list = appearance_text(&reopened, "Fruit");
        assert_eq!(list.matches(LIST_SELECTION_FILL).count(), 2, "{list}");
    }

    #[test]
    fn unicode_values_draw_with_the_bundled_fonts() {
        let mut document = sample_form();
        apply_form_edits(
            &mut document,
            &[FormFieldEdit {
                name: "Name".into(),
                original: FormValue::Text("Ada".into()),
                value: FormValue::Text("Zoë 王".into()),
            }],
        )
        .unwrap();
        let reopened = roundtrip(&mut document);
        assert_eq!(
            extract_form_layer(&reopened).field("Name").unwrap().value,
            FormValue::Text("Zoë 王".into())
        );
        let name = appearance_text(&reopened, "Name");
        assert!(name.contains("/NotoSansSC"), "{name}");
        assert!(text_is_drawable("Zoë 王"));
    }

    #[test]
    fn check_boxes_without_appearances_get_them() {
        let mut document = sample_form();
        let agree = collect_fields(&document)
            .into_iter()
            .find(|raw| raw.name == "Agree")
            .unwrap();
        document
            .get_object_mut(agree.widgets[0])
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .remove(b"AP");
        apply_form_edits(
            &mut document,
            &[FormFieldEdit {
                name: "Agree".into(),
                original: FormValue::Button(None),
                value: FormValue::Button(Some("Yes".into())),
            }],
        )
        .unwrap();
        let reopened = roundtrip(&mut document);
        let widget = reopened.get_dictionary(agree.widgets[0]).unwrap();
        let states = widget
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap();
        let states = states.as_dict().unwrap();
        assert!(states.has(b"Yes") && states.has(b"Off"));
        assert_eq!(widget.get(b"AS").unwrap().as_name().unwrap(), b"Yes");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut document = sample_form();
        assert!(
            apply_form_edits(
                &mut document,
                &[FormFieldEdit {
                    name: "Missing".into(),
                    original: FormValue::Text(String::new()),
                    value: FormValue::Text("x".into()),
                }],
            )
            .is_err()
        );
    }

    #[test]
    fn saved_edits_become_the_base() {
        let edit = |name: &str, original: &str, value: &str| FormFieldEdit {
            name: name.into(),
            original: FormValue::Text(original.into()),
            value: FormValue::Text(value.into()),
        };
        let saved = [edit("A", "", "one"), edit("B", "", "two")];
        // Undone back to before the save: both return to the old values.
        assert_eq!(
            rebase_form_edits(&[], &saved),
            [edit("A", "one", ""), edit("B", "two", "")]
        );
        // The saved state itself needs no edits.
        assert!(rebase_form_edits(&saved, &saved).is_empty());
        // A later state that changed A again, and C.
        assert_eq!(
            rebase_form_edits(&[edit("A", "", "three"), edit("C", "", "x")], &saved),
            [
                edit("A", "one", "three"),
                edit("B", "two", ""),
                edit("C", "", "x")
            ]
        );
    }

    #[test]
    fn default_appearances_parse() {
        assert_eq!(
            DefaultAppearance::parse(b"/Helv 0 Tf 0 g"),
            DefaultAppearance {
                font: Some(b"Helv".to_vec()),
                size: 0.,
                color: Some("0 g".into())
            }
        );
        assert_eq!(
            DefaultAppearance::parse(b"0.2 0.4 0.6 rg /F1 11.5 Tf"),
            DefaultAppearance {
                font: Some(b"F1".to_vec()),
                size: 11.5,
                color: Some("0.2 0.4 0.6 rg".into())
            }
        );
        assert_eq!(font_family("Times-BoldItalic"), ("Tinos", true, true));
        assert_eq!(font_family("CourierNewPSMT"), ("Roboto Mono", false, false));
        assert_eq!(font_family("ArialMT"), ("Helvetica", false, false));
    }

    fn libreoffice_form() -> Document {
        Document::load_mem(include_bytes!("../tests/fixtures/forms/libreoffice.pdf")).unwrap()
    }

    #[test]
    fn reads_a_libreoffice_form() {
        let layer = extract_form_layer(&libreoffice_form());
        let fields = layer
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.widgets.len()))
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            [
                ("ApplicantName", 1),
                ("Email", 1),
                ("Postcode", 1),
                ("AccessNotes", 1),
                ("InspectionType", 3),
                ("SiteReady", 1),
                ("BuildingClass", 1),
                ("Trades", 1),
                ("Reference", 1),
            ]
        );
        let postcode = layer.field("Postcode").unwrap();
        assert!(matches!(
            postcode.kind,
            FormFieldKind::Text {
                max_len: Some(4),
                multiline: false,
                ..
            }
        ));
        let inspection = layer.field("InspectionType").unwrap();
        assert_eq!(inspection.value, FormValue::Button(Some("Frame".into())));
        assert!(matches!(
            inspection.kind,
            FormFieldKind::Radio { toggles_off: false }
        ));
        assert_eq!(
            layer.field("Reference").unwrap().value,
            FormValue::Text("REF-2041".into())
        );
        let class = layer.field("BuildingClass").unwrap();
        assert_eq!(class.value, FormValue::Choices(vec!["Class 1a".into()]));
        assert!(matches!(
            &class.kind,
            FormFieldKind::Combo { options, editable: false } if options.len() == 4
        ));
        // Tab follows the page's annotation order: down the form.
        let order = layer
            .tab_order
            .iter()
            .map(|(field, _)| layer.fields[*field].name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(order.first(), Some(&"ApplicantName"));
        assert_eq!(order.last(), Some(&"Reference"));
        assert!(needs_appearances(&libreoffice_form()));
    }

    #[test]
    fn fills_and_saves_a_libreoffice_form() {
        let mut document = libreoffice_form();
        let layer = extract_form_layer(&document);
        let edit = |name: &str, value: FormValue| FormFieldEdit {
            name: name.into(),
            original: layer.field(name).unwrap().value.clone(),
            value,
        };
        let edits = [
            edit("ApplicantName", FormValue::Text("Zoë Ng 吴".into())),
            edit(
                "AccessNotes",
                FormValue::Text("Gate code 4410.\nDog in the yard.".into()),
            ),
            edit("Postcode", FormValue::Text("3051".into())),
            edit("InspectionType", FormValue::Button(Some("Final".into()))),
            edit(
                "SiteReady",
                FormValue::Button(Some(
                    layer.field("SiteReady").unwrap().widgets[0]
                        .on_state
                        .clone()
                        .unwrap(),
                )),
            ),
            edit("BuildingClass", FormValue::Choices(vec!["Class 5".into()])),
            edit(
                "Trades",
                FormValue::Choices(vec!["Electrician".into(), "Roofer".into()]),
            ),
        ];
        apply_form_edits(&mut document, &edits).unwrap();
        let reopened = roundtrip(&mut document);
        let filled = extract_form_layer(&reopened);
        for edit in &edits {
            assert_eq!(
                filled.field(&edit.name).unwrap().value,
                edit.value,
                "{}",
                edit.name
            );
        }
        let name = appearance_text(&reopened, "ApplicantName");
        assert!(
            name.contains("/NotoSansSC"),
            "吴 falls back to the CJK font: {name}"
        );
        // Helvetica cannot encode the line: Arimo, its metric twin, sets it.
        assert!(name.contains("/Arimo "), "{name}");
        let notes = appearance_text(&reopened, "AccessNotes");
        assert!(
            notes.contains("/Helv "),
            "plain text keeps Helvetica: {notes}"
        );
    }

    #[test]
    fn needs_appearances_finds_unfilled_forms() {
        let mut document = sample_form();
        assert!(
            needs_appearances(&document),
            "the comb field has no appearance"
        );
        regenerate_appearances(&mut document).unwrap();
        assert!(!needs_appearances(&document));
    }
}
