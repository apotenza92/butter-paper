//! Application-owned retained state for the native template manager.
//!
//! GPUI Component renders snapshots of this model and dispatches commands. The
//! durable template library and document workspace remain the storage and
//! document authorities.

use butter_paper_gpui_gallery::generated_document::{
    GeneratedDocumentRequest, GeneratedDocumentStore, OwnedGeneratedDocument,
};
use butter_paper_gpui_gallery::template_library::{
    TemplateLibrary, TemplateRecord, built_in_request,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, Render, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, WeakEntity, Window, div, prelude::FluentBuilder as _, px, rgb,
    svg,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IndexPath, Selectable as _, StyledExt as _, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonGroup, ButtonVariants as _},
    dialog::{DialogDescription, DialogFooter, DialogHeader, DialogTitle},
    form::Field,
    input::{Input, InputEvent, InputState},
    list::{List, ListDelegate, ListItem, ListState},
    scroll::ScrollableElement as _,
    v_flex,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::document_workspace::{DocumentWorkspace, DocumentWorkspaceTemplateCommand};

pub const TEMPLATE_MANAGER_ID: &str = "template-manager-dialog";
pub const TEMPLATE_MANAGER_LIST_ID: &str = "template-manager-list";
pub const TEMPLATE_MANAGER_PREVIEW_ID: &str = "template-manager-preview";
pub const TEMPLATE_MANAGER_PREVIEW_PAGE_ID: &str = "template-manager-preview-page";
pub const TEMPLATE_MANAGER_BROWSE_PAGE_ID: &str = "template-manager-browse-preview-page";
pub const TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID: &str = "template-manager-imported-pdf-preview";
pub const TEMPLATE_MANAGER_SCROLL_ID: &str = "template-manager-scroll";
pub const TEMPLATE_MANAGER_STATUS_ID: &str = "template-manager-status";
pub const TEMPLATE_MANAGER_NAME_FIELD_ID: &str = "template-manager-name-field";
pub const TEMPLATE_MANAGER_NAME_INPUT_ID: &str = "template-manager-name-input";
pub const TEMPLATE_MANAGER_CREATE_ID: &str = "template-manager-create";
pub const TEMPLATE_MANAGER_IMPORT_ID: &str = "template-manager-import";
pub const TEMPLATE_MANAGER_DONE_ID: &str = "template-manager-done";
pub const TEMPLATE_MANAGER_CREATE_DOCUMENT_ID: &str = "template-manager-create-document";
pub const TEMPLATE_MANAGER_SAVE_ID: &str = "template-manager-save";
pub const TEMPLATE_MANAGER_CANCEL_ID: &str = "template-manager-cancel";
pub const TEMPLATE_MANAGER_PATTERN_GROUP_ID: &str = "template-manager-pattern-group";
pub const TEMPLATE_MANAGER_PATTERN_IDS: [&str; 6] = [
    "template-manager-pattern-blank",
    "template-manager-pattern-dots",
    "template-manager-pattern-grid",
    "template-manager-pattern-lined",
    "template-manager-pattern-isometric",
    "template-manager-pattern-triangle",
];
pub const TEMPLATE_MANAGER_PAPER_IDS: [&str; 7] = [
    "template-manager-paper-a5",
    "template-manager-paper-a4",
    "template-manager-paper-a3",
    "template-manager-paper-a2",
    "template-manager-paper-a1",
    "template-manager-paper-a0",
    "template-manager-paper-custom",
];
pub const TEMPLATE_MANAGER_ORIENTATION_IDS: [&str; 2] = [
    "template-manager-orientation-portrait",
    "template-manager-orientation-landscape",
];
pub const TEMPLATE_MANAGER_SPACING_IDS: [&str; 4] = [
    "template-manager-spacing-5",
    "template-manager-spacing-10",
    "template-manager-spacing-25",
    "template-manager-spacing-custom",
];
pub const TEMPLATE_MANAGER_COLOR_IDS: [&str; 4] = [
    "template-manager-color-black",
    "template-manager-color-grey",
    "template-manager-color-blue",
    "template-manager-color-custom",
];
pub const TEMPLATE_MANAGER_WIDTH_INPUT_ID: &str = "template-manager-width-input";
pub const TEMPLATE_MANAGER_HEIGHT_INPUT_ID: &str = "template-manager-height-input";
pub const TEMPLATE_MANAGER_SPACING_INPUT_ID: &str = "template-manager-spacing-input";
pub const TEMPLATE_MANAGER_COLOR_INPUT_ID: &str = "template-manager-color-input";

pub fn template_manager_dialog_width(viewport_width: f32) -> f32 {
    (viewport_width - 32.).clamp(288., 880.)
}

pub fn template_manager_uses_stacked_layout(viewport_width: f32) -> bool {
    viewport_width < 720.
}

struct TemplateListDelegate {
    owner: WeakEntity<TemplateManagerView>,
    records: Vec<TemplateManagerRecord>,
    selected_index: Option<IndexPath>,
    syncing_snapshot: bool,
    storage_busy: bool,
}

impl ListDelegate for TemplateListDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.records.len()
    }

    fn set_selected_index(
        &mut self,
        index: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if self.storage_busy && !self.syncing_snapshot {
            cx.notify();
            return;
        }
        self.selected_index = index;
        if !self.syncing_snapshot
            && let (Some(index), Some(owner)) = (index, self.owner.upgrade())
        {
            let selected_id = self
                .records
                .get(index.row)
                .map(|record| record.id().to_owned());
            let _ = owner.update(cx, |manager, cx| {
                if let Some(id) = selected_id {
                    let _ = manager.model.select(&id);
                    cx.notify();
                }
            });
        }
        cx.notify();
    }

    fn render_item(
        &mut self,
        index: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let record = self.records.get(index.row)?.clone();
        let selected = self.selected_index == Some(index);
        let stable_id = format!("template-manager-item-{}", record.id());
        let removable = record.removable();
        let remove_id = record.id().to_owned();
        let remove_owner = self.owner.clone();
        let storage_busy = self.storage_busy;
        let group_id = format!("template-manager-row-{}", record.id());
        Some(
            ListItem::new(stable_id.clone())
                .debug_selector(move || stable_id.clone().into())
                .selected(selected)
                .group(group_id.clone())
                .child(
                    v_flex()
                        .min_w_0()
                        .child(div().font_semibold().child(record.name().to_owned()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(record.summary()),
                        ),
                )
                .when(removable, |row| {
                    let button_id = format!("template-manager-remove-{remove_id}");
                    row.suffix(move |_, _| {
                        let remove_id = remove_id.clone();
                        let remove_owner = remove_owner.clone();
                        Button::new(button_id.clone())
                            .debug_selector({
                                let button_id = button_id.clone();
                                move || button_id.clone().into()
                            })
                            .label(format!("Remove {}", record.name()))
                            .danger()
                            .disabled(storage_busy)
                            .opacity(0.)
                            .group_hover(group_id.clone(), |style| style.opacity(1.))
                            .focus(|style| style.opacity(1.))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                let _ = remove_owner.update(cx, |manager, cx| {
                                    manager.remove_template(&remove_id, window, cx);
                                    cx.notify();
                                });
                            })
                    })
                }),
        )
    }

    fn perform_search(
        &mut self,
        _: &str,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        Task::ready(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateManagerMode {
    Browse,
    Create,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftInputTarget {
    Width,
    Height,
    Spacing,
    Color,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateManagerRecord {
    BuiltIn {
        id: String,
        name: String,
        summary: String,
    },
    Generated {
        id: String,
        name: String,
        summary: String,
        request: GeneratedDocumentRequest,
    },
    ImportedPdf {
        id: String,
        name: String,
        page_count: usize,
    },
}

impl TemplateManagerRecord {
    pub fn built_in(
        id: impl Into<String>,
        name: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self::BuiltIn {
            id: id.into(),
            name: name.into(),
            summary: summary.into(),
        }
    }

    pub fn generated(
        id: impl Into<String>,
        name: impl Into<String>,
        summary: impl Into<String>,
        request: GeneratedDocumentRequest,
    ) -> Self {
        Self::Generated {
            id: id.into(),
            name: name.into(),
            summary: summary.into(),
            request,
        }
    }

    pub fn imported(id: impl Into<String>, name: impl Into<String>, page_count: usize) -> Self {
        Self::ImportedPdf {
            id: id.into(),
            name: name.into(),
            page_count,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::BuiltIn { id, .. }
            | Self::Generated { id, .. }
            | Self::ImportedPdf { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::BuiltIn { name, .. }
            | Self::Generated { name, .. }
            | Self::ImportedPdf { name, .. } => name,
        }
    }

    pub const fn removable(&self) -> bool {
        !matches!(self, Self::BuiltIn { .. })
    }

    pub fn summary(&self) -> String {
        match self {
            Self::BuiltIn { summary, .. } | Self::Generated { summary, .. } => summary.clone(),
            Self::ImportedPdf { page_count, .. } => format!(
                "{} {} · Imported PDF · Page grid not defined",
                page_count,
                if *page_count == 1 { "page" } else { "pages" }
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateManagerEvent {
    GeneratedSaved { template_id: String },
    ImportRequested { generation: u64 },
    ImportCancelled { generation: u64 },
    Imported { template_id: String },
    Removed { template_id: String },
    CreateRequested { template_id: String },
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateManagerError(String);

impl std::fmt::Display for TemplateManagerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TemplateManagerError {}

#[derive(Clone, Debug)]
pub struct TemplateManagerModel {
    records: Vec<TemplateManagerRecord>,
    selected_id: String,
    last_used_id: String,
    mode: TemplateManagerMode,
    draft_request: GeneratedDocumentRequest,
    draft_paper_preset: &'static str,
    draft_orientation: &'static str,
    draft_spacing_preset: &'static str,
    draft_color_preset: &'static str,
    draft_spacing_mm: f64,
    draft_color: String,
    error: Option<String>,
    import_generation: u64,
    pending_import: Option<u64>,
    events: Vec<TemplateManagerEvent>,
}

impl TemplateManagerModel {
    pub fn new(
        records: Vec<TemplateManagerRecord>,
        last_used_id: impl Into<String>,
    ) -> Result<Self, TemplateManagerError> {
        if records.is_empty() {
            return Err(TemplateManagerError("the template library is empty".into()));
        }
        for (index, record) in records.iter().enumerate() {
            if record.id().is_empty()
                || records[..index]
                    .iter()
                    .any(|existing| existing.id() == record.id())
            {
                return Err(TemplateManagerError(
                    "template identifiers must be non-empty and unique".into(),
                ));
            }
        }
        let requested = last_used_id.into();
        let last_used_id = records
            .iter()
            .find(|record| record.id() == requested)
            .map(|record| record.id().to_owned())
            .unwrap_or_else(|| records[0].id().to_owned());
        Ok(Self {
            records,
            selected_id: last_used_id.clone(),
            last_used_id,
            mode: TemplateManagerMode::Browse,
            draft_request: built_in_request("built-in-blank").unwrap(),
            draft_paper_preset: "a3",
            draft_orientation: "landscape",
            draft_spacing_preset: "10",
            draft_color_preset: "grey",
            draft_spacing_mm: 10.,
            draft_color: "#d1d5db".into(),
            error: None,
            import_generation: 0,
            pending_import: None,
            events: Vec::new(),
        })
    }

    pub fn records(&self) -> &[TemplateManagerRecord] {
        &self.records
    }

    pub fn selected_id(&self) -> &str {
        &self.selected_id
    }

    pub fn last_used_id(&self) -> &str {
        &self.last_used_id
    }

    pub const fn mode(&self) -> TemplateManagerMode {
        self.mode
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn draft_request(&self) -> &GeneratedDocumentRequest {
        &self.draft_request
    }

    pub const fn draft_paper_preset(&self) -> &'static str {
        self.draft_paper_preset
    }

    pub const fn draft_orientation(&self) -> &'static str {
        self.draft_orientation
    }

    pub const fn draft_spacing_preset(&self) -> &'static str {
        self.draft_spacing_preset
    }

    pub const fn draft_color_preset(&self) -> &'static str {
        self.draft_color_preset
    }

    pub fn events(&self) -> &[TemplateManagerEvent] {
        &self.events
    }

    pub fn select(&mut self, template_id: &str) -> Result<(), TemplateManagerError> {
        self.require_record(template_id)?;
        self.selected_id = template_id.into();
        self.error = None;
        Ok(())
    }

    pub fn begin_create(&mut self) {
        self.mode = TemplateManagerMode::Create;
        self.draft_request = built_in_request("built-in-blank").unwrap();
        self.draft_paper_preset = "a3";
        self.draft_orientation = "landscape";
        self.draft_spacing_preset = "10";
        self.draft_color_preset = "grey";
        self.draft_spacing_mm = 10.;
        self.draft_color = "#d1d5db".into();
        self.error = None;
    }

    pub fn reset_for_open(&mut self) {
        self.mode = TemplateManagerMode::Browse;
        self.selected_id = self.last_used_id.clone();
        self.error = None;
    }

    pub fn set_draft_pattern(&mut self, built_in_id: &str) -> Result<(), TemplateManagerError> {
        let request = built_in_request(built_in_id)
            .ok_or_else(|| TemplateManagerError("unknown paper pattern".into()))?;
        self.draft_request.pattern = request.pattern.map(|pattern| {
            pattern_with_style(pattern, self.draft_spacing_mm, self.draft_color.clone())
        });
        self.error = None;
        Ok(())
    }

    pub fn set_draft_paper_preset(
        &mut self,
        preset: &'static str,
    ) -> Result<(), TemplateManagerError> {
        let (width, height) = match preset {
            "a5" => (148., 210.),
            "a4" => (210., 297.),
            "a3" => (297., 420.),
            "a2" => (420., 594.),
            "a1" => (594., 841.),
            "a0" => (841., 1189.),
            "custom" => {
                self.draft_paper_preset = preset;
                self.error = None;
                return Ok(());
            }
            _ => return Err(TemplateManagerError("unknown paper size".into())),
        };
        self.draft_paper_preset = preset;
        (self.draft_request.width_mm, self.draft_request.height_mm) =
            if self.draft_orientation == "landscape" {
                (height, width)
            } else {
                (width, height)
            };
        self.error = None;
        Ok(())
    }

    pub fn set_draft_orientation(
        &mut self,
        orientation: &'static str,
    ) -> Result<(), TemplateManagerError> {
        if !matches!(orientation, "portrait" | "landscape") {
            return Err(TemplateManagerError("unknown paper orientation".into()));
        }
        if self.draft_paper_preset != "custom" && self.draft_orientation != orientation {
            std::mem::swap(
                &mut self.draft_request.width_mm,
                &mut self.draft_request.height_mm,
            );
        }
        self.draft_orientation = orientation;
        self.error = None;
        Ok(())
    }

    pub fn set_draft_custom_dimensions(
        &mut self,
        width: &str,
        height: &str,
    ) -> Result<(), TemplateManagerError> {
        let width = parse_bounded_number(width, 10., 5_000., "Width")?;
        let height = parse_bounded_number(height, 10., 5_000., "Height")?;
        self.draft_paper_preset = "custom";
        self.draft_request.width_mm = width;
        self.draft_request.height_mm = height;
        self.error = None;
        Ok(())
    }

    pub fn set_draft_spacing(
        &mut self,
        preset: &'static str,
        custom: Option<&str>,
    ) -> Result<(), TemplateManagerError> {
        let spacing = match preset {
            "5" => 5.,
            "10" => 10.,
            "25" => 25.,
            "custom" => {
                parse_bounded_number(custom.unwrap_or_default(), 1., 500., "Pattern spacing")?
            }
            _ => return Err(TemplateManagerError("unknown pattern spacing".into())),
        };
        self.draft_spacing_preset = preset;
        self.draft_spacing_mm = spacing;
        self.refresh_draft_pattern_style();
        self.error = None;
        Ok(())
    }

    pub fn set_draft_color(
        &mut self,
        preset: &'static str,
        custom: Option<&str>,
    ) -> Result<(), TemplateManagerError> {
        let color = match preset {
            "black" => "#000000".into(),
            "grey" => "#d1d5db".into(),
            "blue" => "#4e95cc".into(),
            "custom" => normalize_hex_color(custom.unwrap_or_default())?,
            _ => return Err(TemplateManagerError("unknown pattern colour".into())),
        };
        self.draft_color_preset = preset;
        self.draft_color = color;
        self.refresh_draft_pattern_style();
        self.error = None;
        Ok(())
    }

    fn refresh_draft_pattern_style(&mut self) {
        self.draft_request.pattern = self.draft_request.pattern.take().map(|pattern| {
            pattern_with_style(pattern, self.draft_spacing_mm, self.draft_color.clone())
        });
    }

    pub fn cancel_create(&mut self) {
        self.mode = TemplateManagerMode::Browse;
        self.selected_id = self.last_used_id.clone();
        self.error = None;
    }

    pub fn save_generated(
        &mut self,
        template_id: &str,
        name: &str,
        request: GeneratedDocumentRequest,
    ) -> Result<(), TemplateManagerError> {
        if !template_id.starts_with("custom-") || self.record(template_id).is_some() {
            return Err(TemplateManagerError(
                "custom template identifier is invalid".into(),
            ));
        }
        let name = normalize_name(name)?;
        request
            .to_pdf_bytes()
            .map_err(|error| TemplateManagerError(error.to_string()))?;
        let orientation = if request.width_mm >= request.height_mm {
            "Landscape"
        } else {
            "Portrait"
        };
        let summary = format!(
            "{} × {} mm · {orientation}",
            request.width_mm, request.height_mm
        );
        self.records.push(TemplateManagerRecord::generated(
            template_id,
            name,
            summary,
            request,
        ));
        self.selected_id = template_id.into();
        self.last_used_id = template_id.into();
        self.mode = TemplateManagerMode::Browse;
        self.error = None;
        self.events.push(TemplateManagerEvent::GeneratedSaved {
            template_id: template_id.into(),
        });
        Ok(())
    }

    pub fn begin_import(&mut self) -> Option<u64> {
        if self.pending_import.is_some() {
            return None;
        }
        self.import_generation = self.import_generation.saturating_add(1);
        self.pending_import = Some(self.import_generation);
        self.error = None;
        self.events.push(TemplateManagerEvent::ImportRequested {
            generation: self.import_generation,
        });
        Some(self.import_generation)
    }

    pub fn cancel_import(&mut self, generation: u64) -> bool {
        if self.pending_import != Some(generation) {
            return false;
        }
        self.pending_import = None;
        self.events
            .push(TemplateManagerEvent::ImportCancelled { generation });
        true
    }

    pub fn complete_import(&mut self, generation: u64, record: TemplateManagerRecord) -> bool {
        if self.pending_import != Some(generation)
            || !matches!(record, TemplateManagerRecord::ImportedPdf { .. })
            || self.record(record.id()).is_some()
        {
            return false;
        }
        self.pending_import = None;
        self.selected_id = record.id().into();
        self.last_used_id = record.id().into();
        self.events.push(TemplateManagerEvent::Imported {
            template_id: record.id().into(),
        });
        self.records.push(record);
        self.error = None;
        true
    }

    pub fn request_create_selected(&mut self) -> Result<(), TemplateManagerError> {
        self.require_record(&self.selected_id)?;
        self.events.push(TemplateManagerEvent::CreateRequested {
            template_id: self.selected_id.clone(),
        });
        Ok(())
    }

    pub fn mark_create_succeeded(&mut self, template_id: &str) -> Result<(), TemplateManagerError> {
        self.require_record(template_id)?;
        self.last_used_id = template_id.into();
        self.selected_id = template_id.into();
        self.error = None;
        Ok(())
    }

    pub fn record_failure(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.pending_import = None;
        self.error = Some(message.clone());
        self.events.push(TemplateManagerEvent::Failed { message });
    }

    pub fn remove(&mut self, template_id: &str) -> Result<(), TemplateManagerError> {
        let record = self.require_record(template_id)?;
        if !record.removable() {
            return Err(TemplateManagerError(
                "built-in templates cannot be removed".into(),
            ));
        }
        self.records.retain(|record| record.id() != template_id);
        if self.last_used_id == template_id {
            self.last_used_id = self.records[0].id().into();
        }
        if self.selected_id == template_id {
            self.selected_id = self.last_used_id.clone();
        }
        self.events.push(TemplateManagerEvent::Removed {
            template_id: template_id.into(),
        });
        self.error = None;
        Ok(())
    }

    fn record(&self, template_id: &str) -> Option<&TemplateManagerRecord> {
        self.records
            .iter()
            .find(|record| record.id() == template_id)
    }

    fn require_record(
        &self,
        template_id: &str,
    ) -> Result<&TemplateManagerRecord, TemplateManagerError> {
        self.record(template_id)
            .ok_or_else(|| TemplateManagerError("the template does not exist".into()))
    }
}

fn normalize_name(name: &str) -> Result<String, TemplateManagerError> {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(TemplateManagerError("Template name is required.".into()));
    }
    if normalized.encode_utf16().count() > 80 {
        return Err(TemplateManagerError(
            "Template name must be 80 characters or fewer.".into(),
        ));
    }
    Ok(normalized)
}

pub fn legacy_blank_request_from_json(
    json: &str,
) -> Result<GeneratedDocumentRequest, TemplateManagerError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| TemplateManagerError(format!("invalid legacy settings: {error}")))?;
    let string = |key: &str, fallback: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or(fallback)
            .to_owned()
    };
    let mut model = TemplateManagerModel::new(built_in_manager_records(), "built-in-blank")?;
    model.begin_create();
    let preset = string("preset", "a3");
    let preset_key = match preset.as_str() {
        "a5" => "a5",
        "a4" => "a4",
        "a3" => "a3",
        "a2" => "a2",
        "a1" => "a1",
        "a0" => "a0",
        "custom" => "custom",
        _ => return Err(TemplateManagerError("invalid legacy paper preset".into())),
    };
    let orientation = string("orientation", "landscape");
    let orientation_key = match orientation.as_str() {
        "portrait" => "portrait",
        "landscape" => "landscape",
        _ => return Err(TemplateManagerError("invalid legacy orientation".into())),
    };
    model.set_draft_paper_preset(preset_key)?;
    model.set_draft_orientation(orientation_key)?;
    if preset == "custom" {
        model.set_draft_custom_dimensions(
            &string("customWidth", "420"),
            &string("customHeight", "297"),
        )?;
    }
    let pattern = string("patternType", "blank");
    let pattern_id = format!("built-in-{pattern}");
    model.set_draft_pattern(&pattern_id)?;
    if pattern != "blank" {
        let spacing = string("patternSpacingPreset", "10");
        let spacing_key = match spacing.as_str() {
            "5" => "5",
            "10" => "10",
            "25" => "25",
            "custom" => "custom",
            _ => return Err(TemplateManagerError("invalid legacy spacing preset".into())),
        };
        let custom_spacing = string("customPatternSpacing", "10");
        model.set_draft_spacing(
            spacing_key,
            (spacing == "custom").then_some(custom_spacing.as_str()),
        )?;
        let color = string("patternColorPreset", "grey");
        let color_key = match color.as_str() {
            "grey" => "grey",
            "black" => "black",
            "blue" => "blue",
            "custom" => "custom",
            _ => return Err(TemplateManagerError("invalid legacy colour preset".into())),
        };
        let custom_color = string("customPatternColor", "#808080");
        model.set_draft_color(
            color_key,
            (color == "custom").then_some(custom_color.as_str()),
        )?;
    }
    Ok(model.draft_request().clone())
}

pub fn next_custom_template_id(records: &[TemplateManagerRecord]) -> String {
    let next = records
        .iter()
        .filter_map(|record| record.id().strip_prefix("custom-native-"))
        .filter_map(|suffix| suffix.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    format!("custom-native-{next}")
}

pub fn next_imported_template_id(records: &[TemplateManagerRecord]) -> String {
    let next = records
        .iter()
        .filter_map(|record| {
            record
                .id()
                .strip_prefix("imported-00000000-0000-4000-8000-")
        })
        .filter_map(|suffix| u64::from_str_radix(suffix, 16).ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    format!("imported-00000000-0000-4000-8000-{next:012x}")
}

fn parse_bounded_number(
    value: &str,
    minimum: f64,
    maximum: f64,
    label: &str,
) -> Result<f64, TemplateManagerError> {
    let parsed = value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= minimum && *value <= maximum)
        .ok_or_else(|| {
            TemplateManagerError(format!(
                "{label} must be between {minimum:.0} and {maximum:.0} mm."
            ))
        })?;
    Ok(parsed)
}

fn normalize_hex_color(value: &str) -> Result<String, TemplateManagerError> {
    let bytes = value.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        return Err(TemplateManagerError(
            "Pattern colour must be a six-digit hexadecimal value.".into(),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn pattern_with_style(
    pattern: butter_paper_gpui_gallery::generated_document::GeneratedPattern,
    spacing_mm: f64,
    color: String,
) -> butter_paper_gpui_gallery::generated_document::GeneratedPattern {
    use butter_paper_gpui_gallery::generated_document::GeneratedPattern;
    match pattern {
        GeneratedPattern::Dots { .. } => GeneratedPattern::Dots { spacing_mm, color },
        GeneratedPattern::SquareGrid { .. } => GeneratedPattern::SquareGrid { spacing_mm, color },
        GeneratedPattern::Ruled { .. } => GeneratedPattern::Ruled { spacing_mm, color },
        GeneratedPattern::Isometric { .. } => GeneratedPattern::Isometric { spacing_mm, color },
        GeneratedPattern::Triangle { .. } => GeneratedPattern::Triangle { spacing_mm, color },
    }
}

pub fn draft_preview_svg(request: &GeneratedDocumentRequest) -> String {
    use butter_paper_gpui_gallery::generated_document::GeneratedPattern;
    let Some(pattern) = request.pattern.as_ref() else {
        return "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" data-pattern=\"blank\"/>".into();
    };
    let (kind, spacing_mm, color) = match pattern {
        GeneratedPattern::Dots { spacing_mm, color } => ("dots", *spacing_mm, color),
        GeneratedPattern::SquareGrid { spacing_mm, color } => ("grid", *spacing_mm, color),
        GeneratedPattern::Ruled { spacing_mm, color } => ("lined", *spacing_mm, color),
        GeneratedPattern::Isometric { spacing_mm, color } => ("isometric", *spacing_mm, color),
        GeneratedPattern::Triangle { spacing_mm, color } => ("triangle", *spacing_mm, color),
    };
    let step = (spacing_mm / request.width_mm.min(request.height_mm) * 100.).clamp(4., 40.);
    let mut marks = String::new();
    let mut position = step;
    while position < 100. {
        match kind {
            "dots" => {
                let mut x = step;
                while x < 100. {
                    marks.push_str(&format!("<circle cx=\"{x:.2}\" cy=\"{position:.2}\" r=\"0.7\"/>"));
                    x += step;
                }
            }
            "lined" => marks.push_str(&format!("<path d=\"M0 {position:.2}H100\"/>")),
            "grid" => marks.push_str(&format!("<path d=\"M0 {position:.2}H100 M{position:.2} 0V100\"/>")),
            "isometric" => marks.push_str(&format!("<path d=\"M0 {position:.2}L100 {:.2} M0 {position:.2}L100 {:.2}\"/>", position + 57.74, position - 57.74)),
            "triangle" => marks.push_str(&format!("<path d=\"M0 {position:.2}H100 M0 {position:.2}L100 {:.2} M0 {position:.2}L100 {:.2}\"/>", position + 57.74, position - 57.74)),
            _ => {}
        }
        position += step;
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\" data-pattern=\"{kind}\" data-spacing-mm=\"{spacing_mm}\"><g fill=\"{color}\" stroke=\"{color}\" stroke-width=\"0.55\">{marks}</g></svg>"
    )
}

/// Sole durable template-library authority for the native manager seam.
///
/// The returned [`TemplateManagerModel`] is a presentation snapshot. Mutations
/// must return through this facade before the snapshot is refreshed.
pub struct PersistentTemplateManager {
    library: TemplateLibrary,
}

impl PersistentTemplateManager {
    pub fn open(root: PathBuf) -> Result<Self, TemplateManagerError> {
        Ok(Self {
            library: TemplateLibrary::open(root)
                .map_err(|error| TemplateManagerError(error.to_string()))?,
        })
    }

    pub fn snapshot(&self) -> Result<TemplateManagerModel, TemplateManagerError> {
        let mut records = built_in_manager_records();
        records.extend(
            self.library
                .records()
                .iter()
                .filter(|record| matches!(record, TemplateRecord::Generated { .. }))
                .map(manager_record_from_library),
        );
        records.extend(
            self.library
                .records()
                .iter()
                .filter(|record| matches!(record, TemplateRecord::ImportedPdf { .. }))
                .map(manager_record_from_library),
        );
        TemplateManagerModel::new(records, self.library.last_template_id())
    }

    pub fn migrate_legacy_blank_request(
        &mut self,
        request: Option<GeneratedDocumentRequest>,
    ) -> Result<TemplateManagerModel, TemplateManagerError> {
        self.library
            .migrate_legacy_blank_request(request)
            .map_err(|error| TemplateManagerError(error.to_string()))?;
        self.snapshot()
    }

    pub fn save_generated(
        &mut self,
        template_id: &str,
        name: &str,
        request: GeneratedDocumentRequest,
    ) -> Result<TemplateManagerModel, TemplateManagerError> {
        self.library
            .add_generated(template_id, name, request)
            .map_err(|error| TemplateManagerError(error.to_string()))?;
        self.snapshot()
    }

    pub fn import_pdf(
        &mut self,
        template_id: &str,
        name: &str,
        created_at: &str,
        source: &Path,
    ) -> Result<TemplateManagerModel, TemplateManagerError> {
        self.library
            .import_pdf(template_id, name, created_at, source)
            .map_err(|error| TemplateManagerError(error.to_string()))?;
        self.snapshot()
    }

    pub fn save_document_as_template(
        &mut self,
        template_id: &str,
        document_name: &str,
        created_at: &str,
        authorized_source: &Path,
    ) -> Result<TemplateManagerModel, TemplateManagerError> {
        // Electron imports the authorized source bytes, not unsaved in-memory
        // edits. Keep that compatibility boundary explicit.
        self.import_pdf(template_id, document_name, created_at, authorized_source)
    }

    pub fn materialize(
        &self,
        template_id: &str,
        document_key: &str,
        store: &GeneratedDocumentStore,
    ) -> Result<OwnedGeneratedDocument, TemplateManagerError> {
        if let Some(request) = built_in_request(template_id) {
            return store
                .create(document_key, &request)
                .map_err(|error| TemplateManagerError(error.to_string()));
        }
        self.library
            .materialize(template_id, document_key, store)
            .map_err(|error| TemplateManagerError(error.to_string()))
    }

    pub fn managed_source_path(&self, template_id: &str) -> Result<PathBuf, TemplateManagerError> {
        self.library
            .managed_source_path(template_id)
            .map_err(|error| TemplateManagerError(error.to_string()))
    }

    pub fn select(
        &mut self,
        template_id: &str,
    ) -> Result<TemplateManagerModel, TemplateManagerError> {
        self.library
            .select(template_id)
            .map_err(|error| TemplateManagerError(error.to_string()))?;
        self.snapshot()
    }

    pub fn remove(
        &mut self,
        template_id: &str,
    ) -> Result<TemplateManagerModel, TemplateManagerError> {
        if template_id.starts_with("built-in-") {
            return Err(TemplateManagerError(
                "built-in templates cannot be removed".into(),
            ));
        }
        self.library
            .remove(template_id)
            .map_err(|error| TemplateManagerError(error.to_string()))?;
        self.snapshot()
    }
}

fn manager_record_from_library(record: &TemplateRecord) -> TemplateManagerRecord {
    match record {
        TemplateRecord::Generated { id, name, request } => {
            let orientation = if request.width_mm >= request.height_mm {
                "Landscape"
            } else {
                "Portrait"
            };
            TemplateManagerRecord::generated(
                id,
                name,
                format!(
                    "{} × {} mm · {orientation}",
                    request.width_mm, request.height_mm
                ),
                request.clone(),
            )
        }
        TemplateRecord::ImportedPdf {
            id,
            name,
            page_count,
            ..
        } => TemplateManagerRecord::imported(id, name, *page_count),
    }
}

fn built_in_manager_records() -> Vec<TemplateManagerRecord> {
    [
        ("built-in-blank", "Blank Paper"),
        ("built-in-dots", "Dot Grid"),
        ("built-in-grid", "Square Grid"),
        ("built-in-lined", "Ruled Paper"),
        ("built-in-isometric", "Isometric Grid"),
        ("built-in-triangle", "Triangle Grid"),
    ]
    .into_iter()
    .map(|(id, name)| TemplateManagerRecord::built_in(id, name, "A3 · Landscape"))
    .collect()
}

/// Real GPUI Component presentation around application-owned manager state.
pub struct TemplateManagerView {
    model: TemplateManagerModel,
    persistent: Option<Arc<Mutex<PersistentTemplateManager>>>,
    name_input: Entity<InputState>,
    width_input: Entity<InputState>,
    height_input: Entity<InputState>,
    spacing_input: Entity<InputState>,
    color_input: Entity<InputState>,
    template_list: Entity<ListState<TemplateListDelegate>>,
    _input_subscriptions: Vec<Subscription>,
    document_workspace: Option<WeakEntity<DocumentWorkspace>>,
    generated_store: Option<GeneratedDocumentStore>,
    document_sequence: u64,
    import_generation_token: Arc<AtomicU64>,
    document_generation: u64,
    pending_document_generation: Option<u64>,
    draft_error_target: Option<DraftInputTarget>,
    scroll_handle: ScrollHandle,
}

impl TemplateManagerView {
    fn storage_busy(&self) -> bool {
        self.model.pending_import.is_some() || self.pending_document_generation.is_some()
    }

    pub fn is_storage_busy(&self) -> bool {
        self.storage_busy()
    }

    pub fn new(model: TemplateManagerModel, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Template name"));
        let width_input = cx.new(|cx| InputState::new(window, cx));
        let height_input = cx.new(|cx| InputState::new(window, cx));
        let spacing_input = cx.new(|cx| InputState::new(window, cx));
        let color_input = cx.new(|cx| InputState::new(window, cx));
        let selected_index = model
            .records()
            .iter()
            .position(|record| record.id() == model.selected_id())
            .map(IndexPath::new);
        let list_owner = cx.weak_entity();
        let template_list = cx.new(|cx| {
            ListState::new(
                TemplateListDelegate {
                    owner: list_owner,
                    records: model.records().to_vec(),
                    selected_index,
                    syncing_snapshot: false,
                    storage_busy: false,
                },
                window,
                cx,
            )
        });
        if let Some(selected_index) = selected_index {
            template_list.update(cx, |list, cx| {
                list.delegate_mut().syncing_snapshot = true;
                list.set_selected_index(Some(selected_index), window, cx);
                list.delegate_mut().syncing_snapshot = false;
            });
        }
        width_input.update(cx, |input, cx| input.set_value("420", window, cx));
        height_input.update(cx, |input, cx| input.set_value("297", window, cx));
        spacing_input.update(cx, |input, cx| input.set_value("10", window, cx));
        color_input.update(cx, |input, cx| input.set_value("#808080", window, cx));
        let input_subscriptions = [
            &name_input,
            &width_input,
            &height_input,
            &spacing_input,
            &color_input,
        ]
        .into_iter()
        .map(|input| {
            cx.subscribe(input, |manager, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    manager.model.error = None;
                    manager.draft_error_target = None;
                    let _ = manager.sync_draft_inputs(cx);
                    cx.notify();
                }
            })
        })
        .collect();
        Self {
            model,
            persistent: None,
            name_input,
            width_input,
            height_input,
            spacing_input,
            color_input,
            template_list,
            _input_subscriptions: input_subscriptions,
            document_workspace: None,
            generated_store: None,
            document_sequence: 0,
            import_generation_token: Arc::new(AtomicU64::new(0)),
            document_generation: 0,
            pending_document_generation: None,
            draft_error_target: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn open_persistent(
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, TemplateManagerError> {
        Self::open_persistent_with_legacy(root, None, window, cx)
    }

    pub fn open_persistent_with_legacy(
        root: PathBuf,
        legacy_request: Option<GeneratedDocumentRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, TemplateManagerError> {
        let mut persistent = PersistentTemplateManager::open(root)?;
        let model = persistent.migrate_legacy_blank_request(legacy_request)?;
        let mut view = Self::new(model, window, cx);
        view.persistent = Some(Arc::new(Mutex::new(persistent)));
        Ok(view)
    }

    pub fn model(&self) -> &TemplateManagerModel {
        &self.model
    }

    pub fn name_input(&self) -> Entity<InputState> {
        self.name_input.clone()
    }

    pub fn draft_inputs(&self) -> [Entity<InputState>; 4] {
        [
            self.width_input.clone(),
            self.height_input.clone(),
            self.spacing_input.clone(),
            self.color_input.clone(),
        ]
    }

    /// Deterministic seam used by native input events and rendered tests to
    /// refresh the preview from the current composition-safe input values.
    pub fn refresh_draft_preview(&mut self, cx: &App) -> bool {
        self.sync_draft_inputs(cx).is_ok()
    }

    pub fn focus_template_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.template_list
            .update(cx, |list, cx| list.focus(window, cx));
    }

    pub fn bind_document_workspace(
        &mut self,
        workspace: WeakEntity<DocumentWorkspace>,
        store: GeneratedDocumentStore,
    ) {
        self.document_workspace = Some(workspace);
        self.generated_store = Some(store);
    }

    pub fn request_create_template(
        &mut self,
        template_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.model.select(template_id) {
            self.model.record_failure(error.to_string());
            cx.notify();
            return;
        }
        self.sync_template_list(window, cx);
        self.create_selected_document(window, cx);
    }

    pub fn save_authorized_document_as_template(
        &mut self,
        document_name: String,
        authorized_source: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.begin_import_request() else {
            return false;
        };
        self.sync_template_list(window, cx);
        cx.notify();
        self.dispatch_native_import_named(
            generation,
            authorized_source,
            document_name,
            "2026-08-27T00:00:00Z",
            window,
            cx,
        )
    }

    pub fn open_dialog(owner: &Entity<Self>, window: &mut Window, cx: &mut App) {
        owner.update(cx, |manager, cx| {
            manager.model.reset_for_open();
            manager.sync_template_list(window, cx);
        });
        let dialog_width = px(template_manager_dialog_width(f32::from(
            window.viewport_size().width,
        )));
        let weak = owner.downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            dialog
                .w(dialog_width)
                .max_w(dialog_width)
                .overlay_closable(true)
                .keyboard(true)
                .content(move |content, _, _| {
                    let Some(owner) = weak.upgrade() else {
                        return content;
                    };
                    content.child(owner)
                })
        });
        let weak = owner.downgrade();
        window.on_next_frame(move |window, cx| {
            let Some(owner) = weak.upgrade() else {
                return;
            };
            owner.update(cx, |manager, cx| {
                manager.focus_template_list(window, cx);
            });
        });
    }

    fn begin_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model.begin_create();
        self.draft_error_target = None;
        self.name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.width_input
            .update(cx, |input, cx| input.set_value("420", window, cx));
        self.height_input
            .update(cx, |input, cx| input.set_value("297", window, cx));
        self.spacing_input
            .update(cx, |input, cx| input.set_value("10", window, cx));
        self.color_input
            .update(cx, |input, cx| input.set_value("#808080", window, cx));
        self.name_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy() {
            self.model
                .record_failure("Wait for the current template operation to finish.");
            cx.notify();
            return;
        }
        let name = self.name_input.read(cx).value().to_string();
        if let Err((error, target)) = self.sync_draft_inputs(cx) {
            self.model.error = Some(error.to_string());
            self.draft_error_target = Some(target);
            let input = match target {
                DraftInputTarget::Width => &self.width_input,
                DraftInputTarget::Height => &self.height_input,
                DraftInputTarget::Spacing => &self.spacing_input,
                DraftInputTarget::Color => &self.color_input,
            };
            input.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return;
        }
        let id = next_custom_template_id(self.model.records());
        let request = self.model.draft_request.clone();
        let result = if let Some(persistent) = self.persistent.as_ref() {
            persistent
                .lock()
                .map_err(|_| TemplateManagerError("template storage lock was poisoned".into()))
                .and_then(|mut persistent| persistent.save_generated(&id, &name, request))
        } else {
            self.model
                .save_generated(&id, &name, request)
                .map(|()| self.model.clone())
        };
        match result {
            Ok(model) => {
                self.model = model;
                self.draft_error_target = None;
            }
            Err(error) => {
                self.model.error = Some(error.to_string());
                self.draft_error_target = None;
                self.name_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
        }
        self.sync_template_list(window, cx);
        cx.notify();
    }

    fn remove_template(&mut self, template_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy() {
            self.model
                .record_failure("Wait for the current template operation to finish.");
            cx.notify();
            return;
        }
        let result = if let Some(persistent) = self.persistent.as_ref() {
            persistent
                .lock()
                .map_err(|_| TemplateManagerError("template storage lock was poisoned".into()))
                .and_then(|mut persistent| persistent.remove(template_id))
        } else {
            self.model.remove(template_id).map(|()| self.model.clone())
        };
        match result {
            Ok(model) => self.model = model,
            Err(error) => self.model.record_failure(error.to_string()),
        }
        self.sync_template_list(window, cx);
    }

    fn create_selected_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy() {
            return;
        }
        let template_id = self.model.selected_id().to_owned();
        if let Err(error) = self.model.request_create_selected() {
            self.model.record_failure(error.to_string());
            cx.notify();
            return;
        }
        let (Some(persistent), Some(store), Some(workspace)) = (
            self.persistent.clone(),
            self.generated_store.clone(),
            self.document_workspace
                .as_ref()
                .and_then(WeakEntity::upgrade),
        ) else {
            self.model
                .record_failure("Document creation is not configured for this manager.");
            cx.notify();
            return;
        };
        self.document_sequence = self.document_sequence.saturating_add(1);
        let document_key = format!("template-manager-document-{}", self.document_sequence);
        self.document_generation = self.document_generation.saturating_add(1);
        let generation = self.document_generation;
        self.pending_document_generation = Some(generation);
        self.sync_template_list(window, cx);
        cx.notify();
        let background_store = store.clone();
        let background_template_id = template_id.clone();
        let task = cx.background_executor().spawn(async move {
            persistent
                .lock()
                .map_err(|_| "template storage lock was poisoned".to_owned())
                .and_then(|persistent| {
                    persistent
                        .materialize(&background_template_id, &document_key, &background_store)
                        .map_err(|error| error.to_string())
                })
        });
        cx.spawn_in(window, async move |entity, window| {
            let result = task.await;
            let _ = window.update(|window, cx| {
                let _ = entity.update(cx, |manager, cx| {
                    if manager.pending_document_generation != Some(generation) {
                        return;
                    }
                    manager.pending_document_generation = None;
                    match result {
                        Ok(source) => {
                            match workspace.update(cx, |workspace, cx| {
                                workspace.create_owned_template_document(store, source, cx)
                            }) {
                                Ok(_) => {
                                    let selected = manager.persistent.as_ref().and_then(|persistent| {
                                        persistent.lock().ok()?.select(&template_id).ok()
                                    });
                                    if let Some(model) = selected {
                                        manager.model = model;
                                    } else {
                                        manager.model.record_failure(
                                            "The document opened, but the last-used template could not be saved.",
                                        );
                                    }
                                }
                                Err(error) => manager.model.record_failure(error.to_string()),
                            }
                        }
                        Err(error) => manager.model.record_failure(error),
                    }
                    manager.sync_template_list(window, cx);
                    cx.notify();
                });
            });
        })
        .detach();
        cx.notify();
    }

    fn begin_native_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(generation) = self.begin_import_request() else {
            return;
        };
        self.sync_template_list(window, cx);
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a PDF template".into()),
        });
        let owner = cx.entity().downgrade();
        cx.spawn_in(window, async move |_, window| {
            let selection = selection.await;
            window
                .update(|window, cx| {
                    let Some(owner) = owner.upgrade() else {
                        return;
                    };
                    owner.update(cx, |manager, cx| {
                        match selection {
                            Ok(Ok(Some(paths))) => {
                                if let Some(path) = paths.into_iter().next() {
                                    manager.dispatch_native_import(generation, path, window, cx);
                                } else {
                                    manager.cancel_import_request(generation);
                                }
                            }
                            Ok(Ok(None)) => {
                                manager.cancel_import_request(generation);
                            }
                            Ok(Err(error)) => {
                                manager.import_generation_token.store(0, Ordering::Release);
                                manager.model.record_failure(format!(
                                    "Unable to import the PDF template: {error}"
                                ));
                            }
                            Err(_) => {
                                manager.import_generation_token.store(0, Ordering::Release);
                                manager.model.record_failure(
                                    "The PDF template picker stopped before it returned a result.",
                                );
                            }
                        }
                        manager.sync_template_list(window, cx);
                        cx.notify();
                    });
                })
                .ok()
        })
        .detach();
        cx.notify();
    }

    /// Starts the platform-picker transaction for an injected native adapter.
    /// The generation is the only completion token accepted by
    /// [`Self::complete_native_import`].
    pub fn begin_import_request(&mut self) -> Option<u64> {
        let generation = self.model.begin_import()?;
        self.import_generation_token
            .store(generation, Ordering::Release);
        Some(generation)
    }

    pub fn cancel_import_request(&mut self, generation: u64) -> bool {
        if !self.model.cancel_import(generation) {
            return false;
        }
        self.import_generation_token.store(0, Ordering::Release);
        true
    }

    pub fn dispatch_native_import(
        &mut self,
        generation: u64,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported PDF")
            .to_owned();
        self.dispatch_native_import_named(
            generation,
            path,
            name,
            "2026-08-27T00:00:00Z",
            window,
            cx,
        )
    }

    fn dispatch_native_import_named(
        &mut self,
        generation: u64,
        path: PathBuf,
        name: String,
        created_at: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.model.pending_import != Some(generation) {
            return false;
        }
        let Some(persistent) = self.persistent.clone() else {
            self.model
                .record_failure("Template storage is not configured for this manager.");
            self.sync_template_list(window, cx);
            cx.notify();
            return false;
        };
        let id = next_imported_template_id(self.model.records());
        let previous_last_used_id = self.model.last_used_id().to_owned();
        let token = self.import_generation_token.clone();
        let cleanup_last_used_id = previous_last_used_id.clone();
        let task = cx.background_executor().spawn(async move {
            if token.load(Ordering::Acquire) != generation {
                return (generation, Ok(None));
            }
            let mut result = persistent
                .lock()
                .map_err(|_| TemplateManagerError("template storage lock was poisoned".into()))
                .and_then(|mut persistent| {
                    persistent
                        .import_pdf(&id, &name, created_at, &path)
                        .map(Some)
                });
            if token.load(Ordering::Acquire) != generation {
                if result.is_ok() {
                    result = persistent
                        .lock()
                        .map_err(|_| {
                            TemplateManagerError("template storage lock was poisoned".into())
                        })
                        .and_then(|mut persistent| {
                            persistent
                                .remove(&id)
                                .and_then(|_| persistent.select(&cleanup_last_used_id))
                                .map(|_| None)
                        });
                }
            }
            (generation, result)
        });
        cx.spawn_in(window, async move |entity, window| {
            let (generation, result) = task.await;
            let _ = window.update(|window, cx| {
                let _ = entity.update(cx, |manager, cx| {
                    if manager.model.pending_import != Some(generation)
                        || manager.import_generation_token.load(Ordering::Acquire) != generation
                    {
                        if let Ok(Some(model)) = result {
                            let stale_id = model.last_used_id().to_owned();
                            let previous_last_used_id = previous_last_used_id.clone();
                            if let Some(persistent) = manager.persistent.clone() {
                                cx.background_executor()
                                    .spawn(async move {
                                        if let Ok(mut persistent) = persistent.lock() {
                                            let _ = persistent.remove(&stale_id).and_then(|_| {
                                                persistent.select(&previous_last_used_id)
                                            });
                                        }
                                    })
                                    .detach();
                            }
                        }
                        return;
                    }
                    manager.import_generation_token.store(0, Ordering::Release);
                    match result {
                        Ok(Some(model)) => manager.model = model,
                        Ok(None) => {
                            manager.model.cancel_import(generation);
                        }
                        Err(error) => manager.model.record_failure(error.to_string()),
                    }
                    manager.sync_template_list(window, cx);
                    cx.notify();
                });
            });
        })
        .detach();
        true
    }

    fn sync_template_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let records = self.model.records().to_vec();
        let storage_busy = self.storage_busy();
        let selected = records
            .iter()
            .position(|record| record.id() == self.model.selected_id())
            .map(IndexPath::new);
        self.template_list.update(cx, |list, cx| {
            list.delegate_mut().records = records;
            list.delegate_mut().storage_busy = storage_busy;
            list.delegate_mut().syncing_snapshot = true;
            list.set_selected_index(selected, window, cx);
            list.delegate_mut().syncing_snapshot = false;
        });
    }

    fn set_draft_pattern(&mut self, built_in_id: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.model.set_draft_pattern(built_in_id) {
            self.model.record_failure(error.to_string());
        }
        cx.notify();
    }

    fn sync_draft_inputs(
        &mut self,
        cx: &App,
    ) -> Result<(), (TemplateManagerError, DraftInputTarget)> {
        if self.model.draft_paper_preset == "custom" {
            let width = self.width_input.read(cx).value();
            let height = self.height_input.read(cx).value();
            parse_bounded_number(&width, 10., 5_000., "Width")
                .map_err(|error| (error, DraftInputTarget::Width))?;
            parse_bounded_number(&height, 10., 5_000., "Height")
                .map_err(|error| (error, DraftInputTarget::Height))?;
            self.model
                .set_draft_custom_dimensions(&width, &height)
                .map_err(|error| (error, DraftInputTarget::Width))?;
        }
        if self.model.draft_spacing_preset == "custom" {
            let spacing = self.spacing_input.read(cx).value();
            self.model
                .set_draft_spacing("custom", Some(&spacing))
                .map_err(|error| (error, DraftInputTarget::Spacing))?;
        }
        if self.model.draft_color_preset == "custom" {
            let color = self.color_input.read(cx).value();
            self.model
                .set_draft_color("custom", Some(&color))
                .map_err(|error| (error, DraftInputTarget::Color))?;
        }
        Ok(())
    }

    fn set_draft_paper(&mut self, preset: &'static str, cx: &mut Context<Self>) {
        if let Err(error) = self.model.set_draft_paper_preset(preset) {
            self.model.record_failure(error.to_string());
        }
        cx.notify();
    }

    fn set_draft_orientation(&mut self, orientation: &'static str, cx: &mut Context<Self>) {
        if let Err(error) = self.model.set_draft_orientation(orientation) {
            self.model.record_failure(error.to_string());
        }
        cx.notify();
    }

    fn set_draft_spacing(&mut self, preset: &'static str, cx: &mut Context<Self>) {
        let custom = (preset == "custom").then(|| self.spacing_input.read(cx).value());
        if let Err(error) = self.model.set_draft_spacing(preset, custom.as_deref()) {
            self.model.record_failure(error.to_string());
        }
        cx.notify();
    }

    fn set_draft_color(&mut self, preset: &'static str, cx: &mut Context<Self>) {
        let custom = (preset == "custom").then(|| self.color_input.read(cx).value());
        if let Err(error) = self.model.set_draft_color(preset, custom.as_deref()) {
            self.model.record_failure(error.to_string());
        }
        cx.notify();
    }
}

pub fn route_workspace_template_command(
    manager: &Entity<TemplateManagerView>,
    event: &DocumentWorkspaceTemplateCommand,
    window: &mut Window,
    cx: &mut App,
) {
    match event {
        DocumentWorkspaceTemplateCommand::Create(request) => {
            manager.update(cx, |manager, cx| {
                manager.request_create_template(&request.template_id, window, cx);
            });
        }
        DocumentWorkspaceTemplateCommand::Manage => {
            TemplateManagerView::open_dialog(manager, window, cx);
        }
        DocumentWorkspaceTemplateCommand::SaveDocumentAsTemplate {
            document_name,
            authorized_source,
            ..
        } => {
            manager.update(cx, |manager, cx| {
                manager.save_authorized_document_as_template(
                    document_name.clone(),
                    authorized_source.clone(),
                    window,
                    cx,
                );
            });
        }
    }
}

impl Render for TemplateManagerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let constrained =
            template_manager_uses_stacked_layout(f32::from(window.viewport_size().width));
        let owner = cx.entity().downgrade();
        let selected_id = self.model.selected_id.clone();
        let selected = self
            .model
            .record(&selected_id)
            .cloned()
            .unwrap_or_else(|| self.model.records[0].clone());
        let error = self.model.error.clone();
        let storage_busy = self.storage_busy();
        let draft_error_target = self.draft_error_target;
        let draft_error = error.clone().unwrap_or_default();

        let header = DialogHeader::new()
            .child(DialogTitle::new().child("Template library"))
            .child(DialogDescription::new().child(
                "Create reusable paper or import a PDF that you already use as a template.",
            ));

        let browse_preview: AnyElement = match &selected {
            TemplateManagerRecord::BuiltIn { id, .. } => {
                let request = built_in_request(id)
                    .unwrap_or_else(GeneratedDocumentRequest::a3_landscape_blank);
                let data = draft_preview_svg(&request);
                div()
                    .id(TEMPLATE_MANAGER_BROWSE_PAGE_ID)
                    .debug_selector(|| TEMPLATE_MANAGER_BROWSE_PAGE_ID.into())
                    .w(px(160.))
                    .h(px((160. * request.height_mm / request.width_mm) as f32))
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(rgb(0xffffff))
                    .child(svg().data(data.as_bytes()).size_full())
                    .into_any_element()
            }
            TemplateManagerRecord::Generated { request, .. } => {
                let data = draft_preview_svg(request);
                div()
                    .id(TEMPLATE_MANAGER_BROWSE_PAGE_ID)
                    .debug_selector(|| TEMPLATE_MANAGER_BROWSE_PAGE_ID.into())
                    .w(px(160.))
                    .h(px((160. * request.height_mm / request.width_mm) as f32))
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(rgb(0xffffff))
                    .child(svg().data(data.as_bytes()).size_full())
                    .into_any_element()
            }
            TemplateManagerRecord::ImportedPdf { .. } => div()
                .id(TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID)
                .debug_selector(|| TEMPLATE_MANAGER_IMPORTED_PREVIEW_ID.into())
                .w(px(120.))
                .h(px(160.))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(cx.theme().border)
                .bg(rgb(0xffffff))
                .font_semibold()
                .child("PDF")
                .into_any_element(),
        };

        let body = match self.model.mode {
            TemplateManagerMode::Browse => {
                let list = div()
                    .id(TEMPLATE_MANAGER_LIST_ID)
                    .debug_selector(|| TEMPLATE_MANAGER_LIST_ID.into())
                    .min_w_0()
                    .h(if constrained { px(260.) } else { px(430.) })
                    .flex_1()
                    .child(List::new(&self.template_list));
                let preview = v_flex()
                    .id(TEMPLATE_MANAGER_PREVIEW_ID)
                    .debug_selector(|| TEMPLATE_MANAGER_PREVIEW_ID.into())
                    .when(!constrained, |preview| preview.w(px(280.)).flex_none())
                    .when(constrained, |preview| preview.w_full())
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .p_4()
                    .child(div().font_semibold().child(selected.name().to_owned()))
                    .child(browse_preview)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(selected.summary()),
                    );
                let library_content = if constrained {
                    v_flex()
                        .min_h_0()
                        .gap_4()
                        .child(list)
                        .child(preview)
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .min_h_0()
                        .gap_4()
                        .child(list)
                        .child(preview)
                        .into_any_element()
                };

                let create_owner = owner.clone();
                let import_owner = owner.clone();
                let create_document_owner = owner.clone();
                let footer = DialogFooter::new()
                    .when(constrained, |footer| footer.flex_col().items_stretch())
                    .child(
                        Button::new(TEMPLATE_MANAGER_CREATE_DOCUMENT_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_CREATE_DOCUMENT_ID.into())
                            .label("Create document")
                            .primary()
                            .disabled(
                                self.document_workspace.is_none()
                                    || self.generated_store.is_none()
                                    || self.storage_busy(),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = create_document_owner.update(cx, |manager, cx| {
                                    manager.create_selected_document(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(TEMPLATE_MANAGER_CREATE_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_CREATE_ID.into())
                            .label("Create paper template…")
                            .outline()
                            .disabled(self.storage_busy())
                            .on_click(move |_, window, cx| {
                                let _ = create_owner.update(cx, |manager, cx| {
                                    manager.begin_create(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(TEMPLATE_MANAGER_IMPORT_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_IMPORT_ID.into())
                            .label("Import PDF as template…")
                            .outline()
                            .disabled(self.storage_busy())
                            .on_click(move |_, window, cx| {
                                let _ = import_owner.update(cx, |manager, cx| {
                                    manager.begin_native_import(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(TEMPLATE_MANAGER_DONE_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_DONE_ID.into())
                            .label("Done")
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    );

                v_flex()
                    .gap_3()
                    .child(library_content)
                    .children(error.map(|message| {
                        div()
                            .id(TEMPLATE_MANAGER_STATUS_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_STATUS_ID.into())
                            .child(Alert::error("template-manager-status-alert", message))
                    }))
                    .child(footer)
                    .into_any_element()
            }
            TemplateManagerMode::Create => {
                let cancel_owner = owner.clone();
                let save_owner = owner.clone();
                let selected_paper = self.model.draft_paper_preset;
                let paper_buttons = ["a5", "a4", "a3", "a2", "a1", "a0", "custom"]
                    .into_iter()
                    .zip(["A5", "A4", "A3", "A2", "A1", "A0", "Custom"])
                    .zip(TEMPLATE_MANAGER_PAPER_IDS)
                    .map(|((preset, label), stable_id)| {
                        let paper_owner = owner.clone();
                        Button::new(stable_id)
                            .debug_selector(move || stable_id.into())
                            .label(label)
                            .selected(selected_paper == preset)
                            .on_click(move |_, _, cx| {
                                let _ = paper_owner.update(cx, |manager, cx| {
                                    manager.set_draft_paper(preset, cx);
                                });
                            })
                    })
                    .collect::<Vec<_>>();
                let paper_controls = if constrained {
                    v_flex().gap_1().children(paper_buttons).into_any_element()
                } else {
                    ButtonGroup::new("template-manager-paper-group")
                        .children(paper_buttons)
                        .into_any_element()
                };
                let selected_orientation = self.model.draft_orientation;
                let orientation_buttons = ["portrait", "landscape"]
                    .into_iter()
                    .zip(["Portrait", "Landscape"])
                    .zip(TEMPLATE_MANAGER_ORIENTATION_IDS)
                    .map(|((orientation, label), stable_id)| {
                        let orientation_owner = owner.clone();
                        Button::new(stable_id)
                            .debug_selector(move || stable_id.into())
                            .label(label)
                            .selected(selected_orientation == orientation)
                            .disabled(selected_paper == "custom")
                            .on_click(move |_, _, cx| {
                                let _ = orientation_owner.update(cx, |manager, cx| {
                                    manager.set_draft_orientation(orientation, cx);
                                });
                            })
                    });
                let selected_pattern = match self.model.draft_request.pattern {
                    None => "built-in-blank",
                    Some(butter_paper_gpui_gallery::generated_document::GeneratedPattern::Dots {
                        ..
                    }) => "built-in-dots",
                    Some(
                        butter_paper_gpui_gallery::generated_document::GeneratedPattern::SquareGrid {
                            ..
                        },
                    ) => "built-in-grid",
                    Some(butter_paper_gpui_gallery::generated_document::GeneratedPattern::Ruled {
                        ..
                    }) => "built-in-lined",
                    Some(
                        butter_paper_gpui_gallery::generated_document::GeneratedPattern::Isometric {
                            ..
                        },
                    ) => "built-in-isometric",
                    Some(
                        butter_paper_gpui_gallery::generated_document::GeneratedPattern::Triangle {
                            ..
                        },
                    ) => "built-in-triangle",
                };
                let pattern_buttons = [
                    ("built-in-blank", "Blank"),
                    ("built-in-dots", "Dots"),
                    ("built-in-grid", "Grid"),
                    ("built-in-lined", "Lined"),
                    ("built-in-isometric", "Isometric"),
                    ("built-in-triangle", "Triangle"),
                ]
                .into_iter()
                .zip(TEMPLATE_MANAGER_PATTERN_IDS)
                .map(|((pattern_id, label), stable_id)| {
                    let pattern_owner = owner.clone();
                    Button::new(stable_id)
                        .debug_selector(move || stable_id.into())
                        .label(label)
                        .selected(selected_pattern == pattern_id)
                        .on_click(move |_, _, cx| {
                            let _ = pattern_owner.update(cx, |manager, cx| {
                                manager.set_draft_pattern(pattern_id, cx);
                            });
                        })
                })
                .collect::<Vec<_>>();
                let pattern_controls = if constrained {
                    v_flex()
                        .gap_1()
                        .children(pattern_buttons)
                        .into_any_element()
                } else {
                    ButtonGroup::new(TEMPLATE_MANAGER_PATTERN_GROUP_ID)
                        .children(pattern_buttons)
                        .into_any_element()
                };
                let selected_spacing = self.model.draft_spacing_preset;
                let spacing_buttons = ["5", "10", "25", "custom"]
                    .into_iter()
                    .zip(["5 mm", "10 mm", "25 mm", "Custom"])
                    .zip(TEMPLATE_MANAGER_SPACING_IDS)
                    .map(|((preset, label), stable_id)| {
                        let spacing_owner = owner.clone();
                        Button::new(stable_id)
                            .debug_selector(move || stable_id.into())
                            .label(label)
                            .selected(selected_spacing == preset)
                            .on_click(move |_, _, cx| {
                                let _ = spacing_owner.update(cx, |manager, cx| {
                                    manager.set_draft_spacing(preset, cx);
                                });
                            })
                    })
                    .collect::<Vec<_>>();
                let spacing_controls = if constrained {
                    v_flex()
                        .gap_1()
                        .children(spacing_buttons)
                        .into_any_element()
                } else {
                    ButtonGroup::new("template-manager-spacing-group")
                        .children(spacing_buttons)
                        .into_any_element()
                };
                let selected_color = self.model.draft_color_preset;
                let color_buttons = ["black", "grey", "blue", "custom"]
                    .into_iter()
                    .zip(["Black", "Grey", "Light blue", "Custom"])
                    .zip(TEMPLATE_MANAGER_COLOR_IDS)
                    .map(|((preset, label), stable_id)| {
                        let color_owner = owner.clone();
                        Button::new(stable_id)
                            .debug_selector(move || stable_id.into())
                            .label(label)
                            .selected(selected_color == preset)
                            .on_click(move |_, _, cx| {
                                let _ = color_owner.update(cx, |manager, cx| {
                                    manager.set_draft_color(preset, cx);
                                });
                            })
                    })
                    .collect::<Vec<_>>();
                let color_controls = if constrained {
                    v_flex().gap_1().children(color_buttons).into_any_element()
                } else {
                    ButtonGroup::new("template-manager-color-group")
                        .children(color_buttons)
                        .into_any_element()
                };
                let draft_summary = self.model.draft_request.to_pdf_bytes().map_or_else(
                    |error| error.to_string(),
                    |_| {
                        let pattern = match selected_pattern {
                            "built-in-blank" => "No page grid".to_owned(),
                            "built-in-dots" => {
                                format!("Dot grid · {} mm", self.model.draft_spacing_mm)
                            }
                            "built-in-grid" => {
                                format!("Square grid · {} mm", self.model.draft_spacing_mm)
                            }
                            "built-in-lined" => {
                                format!("Ruled · {} mm", self.model.draft_spacing_mm)
                            }
                            "built-in-isometric" => {
                                format!("Isometric grid · {} mm", self.model.draft_spacing_mm)
                            }
                            _ => format!("Triangle grid · {} mm", self.model.draft_spacing_mm),
                        };
                        format!(
                            "{} × {} mm · {} · {pattern}",
                            self.model.draft_request.width_mm,
                            self.model.draft_request.height_mm,
                            if self.model.draft_orientation == "landscape" {
                                "Landscape"
                            } else {
                                "Portrait"
                            }
                        )
                    },
                );
                let aspect = (self.model.draft_request.width_mm
                    / self.model.draft_request.height_mm)
                    .clamp(0.2, 5.0) as f32;
                let (preview_width, preview_height) = if aspect >= 1. {
                    (220., 220. / aspect)
                } else {
                    (160. * aspect, 160.)
                };
                let preview_svg = draft_preview_svg(&self.model.draft_request);
                v_flex()
                    .gap_4()
                    .child(
                        div()
                            .id(TEMPLATE_MANAGER_NAME_FIELD_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_NAME_FIELD_ID.into())
                            .child(
                                Field::new().label("Template name").required(true).child(
                                    div()
                                        .id(TEMPLATE_MANAGER_NAME_INPUT_ID)
                                        .debug_selector(|| TEMPLATE_MANAGER_NAME_INPUT_ID.into())
                                        .child(
                                            Input::new(&self.name_input)
                                                .accessibility_id(TEMPLATE_MANAGER_NAME_INPUT_ID)
                                                .aria_label("Template name"),
                                        ),
                                ),
                            ),
                    )
                    .child(Field::new().label("Paper size").child(paper_controls))
                    .when(selected_paper == "custom", |form| {
                        form.child(
                            div()
                                .flex()
                                .gap_3()
                                .child(
                                    Field::new()
                                        .label("Width (mm)")
                                        .when(
                                            draft_error_target == Some(DraftInputTarget::Width),
                                            |field| field.description(draft_error.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(TEMPLATE_MANAGER_WIDTH_INPUT_ID)
                                                .debug_selector(|| {
                                                    TEMPLATE_MANAGER_WIDTH_INPUT_ID.into()
                                                })
                                                .child(
                                                    Input::new(&self.width_input)
                                                        .accessibility_id(
                                                            TEMPLATE_MANAGER_WIDTH_INPUT_ID,
                                                        )
                                                        .aria_label(
                                                            "Custom paper width in millimetres",
                                                        ),
                                                ),
                                        ),
                                )
                                .child(
                                    Field::new()
                                        .label("Height (mm)")
                                        .when(
                                            draft_error_target == Some(DraftInputTarget::Height),
                                            |field| field.description(draft_error.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(TEMPLATE_MANAGER_HEIGHT_INPUT_ID)
                                                .debug_selector(|| {
                                                    TEMPLATE_MANAGER_HEIGHT_INPUT_ID.into()
                                                })
                                                .child(
                                                    Input::new(&self.height_input)
                                                        .accessibility_id(
                                                            TEMPLATE_MANAGER_HEIGHT_INPUT_ID,
                                                        )
                                                        .aria_label(
                                                            "Custom paper height in millimetres",
                                                        ),
                                                ),
                                        ),
                                ),
                        )
                    })
                    .child(
                        Field::new().label("Orientation").child(
                            ButtonGroup::new("template-manager-orientation-group")
                                .children(orientation_buttons),
                        ),
                    )
                    .child(Field::new().label("Page pattern").child(pattern_controls))
                    .when(selected_pattern != "built-in-blank", |form| {
                        form.child(Field::new().label("Spacing").child(spacing_controls))
                            .when(selected_spacing == "custom", |form| {
                                form.child(
                                    Field::new()
                                        .label("Custom spacing (mm)")
                                        .when(
                                            draft_error_target == Some(DraftInputTarget::Spacing),
                                            |field| field.description(draft_error.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(TEMPLATE_MANAGER_SPACING_INPUT_ID)
                                                .debug_selector(|| {
                                                    TEMPLATE_MANAGER_SPACING_INPUT_ID.into()
                                                })
                                                .child(
                                                    Input::new(&self.spacing_input)
                                                        .accessibility_id(
                                                            TEMPLATE_MANAGER_SPACING_INPUT_ID,
                                                        )
                                                        .aria_label(
                                                            "Custom pattern spacing in millimetres",
                                                        ),
                                                ),
                                        ),
                                )
                            })
                            .child(Field::new().label("Colour").child(color_controls))
                            .when(selected_color == "custom", |form| {
                                form.child(
                                    Field::new()
                                        .label("Custom colour")
                                        .when(
                                            draft_error_target == Some(DraftInputTarget::Color),
                                            |field| field.description(draft_error.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(TEMPLATE_MANAGER_COLOR_INPUT_ID)
                                                .debug_selector(|| {
                                                    TEMPLATE_MANAGER_COLOR_INPUT_ID.into()
                                                })
                                                .child(
                                                    Input::new(&self.color_input)
                                                        .accessibility_id(
                                                            TEMPLATE_MANAGER_COLOR_INPUT_ID,
                                                        )
                                                        .aria_label(
                                                            "Custom pattern colour as hexadecimal",
                                                        ),
                                                ),
                                        ),
                                )
                            })
                    })
                    .child(
                        v_flex()
                            .id(TEMPLATE_MANAGER_PREVIEW_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_PREVIEW_ID.into())
                            .items_center()
                            .gap_3()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .p_4()
                            .child(
                                div()
                                    .id(TEMPLATE_MANAGER_PREVIEW_PAGE_ID)
                                    .debug_selector(|| TEMPLATE_MANAGER_PREVIEW_PAGE_ID.into())
                                    .relative()
                                    .flex_none()
                                    .w(px(preview_width))
                                    .h(px(preview_height))
                                    .overflow_hidden()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .bg(rgb(0xffffff))
                                    .child(svg().data(preview_svg.as_bytes()).size_full()),
                            )
                            .child(draft_summary),
                    )
                    .children(error.map(|message| {
                        div()
                            .id(TEMPLATE_MANAGER_STATUS_ID)
                            .debug_selector(|| TEMPLATE_MANAGER_STATUS_ID.into())
                            .child(Alert::error("template-manager-status-alert", message))
                    }))
                    .child(
                        DialogFooter::new()
                            .when(constrained, |footer| footer.flex_col().items_stretch())
                            .child(
                                Button::new(TEMPLATE_MANAGER_CANCEL_ID)
                                    .debug_selector(|| TEMPLATE_MANAGER_CANCEL_ID.into())
                                    .label("Cancel")
                                    .outline()
                                    .on_click(move |_, _, cx| {
                                        let _ = cancel_owner.update(cx, |manager, cx| {
                                            manager.model.cancel_create();
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new(TEMPLATE_MANAGER_SAVE_ID)
                                    .debug_selector(|| TEMPLATE_MANAGER_SAVE_ID.into())
                                    .label("Save template")
                                    .primary()
                                    .disabled(storage_busy)
                                    .on_click(move |_, window, cx| {
                                        let _ = save_owner.update(cx, |manager, cx| {
                                            manager.save_draft(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .into_any_element()
            }
        };

        let content = v_flex()
            .id(TEMPLATE_MANAGER_ID)
            .debug_selector(|| TEMPLATE_MANAGER_ID.into())
            .w_full()
            .when(!constrained, |content| content.max_h(px(720.)))
            .min_h_0()
            .gap_4()
            .child(header)
            .child(body);
        if constrained {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id(TEMPLATE_MANAGER_SCROLL_ID)
                        .debug_selector(|| TEMPLATE_MANAGER_SCROLL_ID.into())
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle)
                        .child(content),
                )
                .vertical_scrollbar(&self.scroll_handle)
                .into_any_element()
        } else {
            content.into_any_element()
        }
    }
}
