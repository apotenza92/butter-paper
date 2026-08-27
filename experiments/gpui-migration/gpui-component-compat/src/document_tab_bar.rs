use std::{cell::Cell, collections::HashMap, rc::Rc};

use gpui::{
    Anchor, AppContext as _, Bounds, ClickEvent, Context, DispatchPhase, Entity, EventEmitter,
    FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Render, Role, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, accesskit::Live, canvas,
    linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    dialog::Cancel,
    h_flex,
    popover::Popover,
    tab::{Tab, TabBar},
    v_flex,
};

pub const DOCUMENT_TAB_BAR_ID: &str = "document-tab-bar";
pub const DOCUMENT_TAB_SURFACE_ID: &str = "document-tab-surface";
pub const DOCUMENT_TAB_LIST_ID: &str = "document-tab-list";
pub const DOCUMENT_TAB_CONTENT_ID: &str = "document-tab-content";
pub const DOCUMENT_TAB_ACTIONS_ID: &str = "document-tab-actions";
pub const DOCUMENT_TAB_OPEN_ID: &str = "document-tab-open";
pub const TEMPLATE_CONTROL_GROUP_ID: &str = "document-tab-template-controls";
pub const TEMPLATE_PRIMARY_ID: &str = "document-tab-new-pdf";
pub const TEMPLATE_PICKER_ID: &str = "document-tab-template-picker";
pub const TEMPLATE_PICKER_POPOVER_ID: &str = "template-picker";
pub const TEMPLATE_MANAGE_ID: &str = "template-picker-manage";
pub const TEMPLATE_CREATE_ID: &str = "template-picker-create";
pub const DOCUMENT_TAB_LIST_ACCESSIBLE_NAME: &str = "Open documents";
pub const DOCUMENT_TAB_REORDER_STATUS_ID: &str = "document-tab-reorder-status";
pub const DOCUMENT_TAB_REORDER_KEYSHORTCUTS: &str = "Alt+Shift+ArrowLeft Alt+Shift+ArrowRight";
pub const DOCUMENT_TAB_REORDER_DESCRIPTION: &str =
    "Drag to reorder. Press Alt+Shift+Left or Alt+Shift+Right to move this tab.";
pub const DOCUMENT_TAB_POINTER_DRAG_THRESHOLD: f64 = 6.;
pub const DOCUMENT_TAB_POINTER_PRIMARY_GAP: &str =
    "GPUI exposes one native mouse pointer and no browser-style isPrimary flag";
pub const DOCUMENT_TAB_POINTER_CANCEL_GAP: &str = "GPUI exposes mouse up and Escape but no public pointer-cancel, resize, or visibility event parity seam";
pub const DOCUMENT_TAB_POINTER_AUTOSCROLL_GAP: &str = "Electron leaves dnd-kit ancestor auto-scroll enabled; the GPUI edge-scroll surface is deferred";
pub const DIRTY_CLOSE_CONFIRMATION_ID: &str = "confirmation-popover";
pub const DIRTY_CLOSE_TITLE_ID: &str = "dirty-close-confirmation-title";
pub const DIRTY_CLOSE_DESCRIPTION_ID: &str = "dirty-close-confirmation-description";
pub const DIRTY_CLOSE_CANCEL_ID: &str = "dirty-close-confirmation-cancel";
pub const DIRTY_CLOSE_DISCARD_ID: &str = "dirty-close-confirmation-discard";
pub const DIRTY_CLOSE_SAVE_ID: &str = "dirty-close-confirmation-save";
pub const DIRTY_CLOSE_DESCRIPTION: &str =
    "Your changes will be lost if you close this tab without saving.";
/// The smallest cap that preserves the frozen 380 px strip with its existing
/// dirty `structural-details` label. Electron requests truncation but declares
/// no per-tab pixel cap, so this is an explicit reversible native mapping.
pub const DOCUMENT_TAB_MAX_WIDTH: f32 = 190.;
pub const DOCUMENT_TAB_HOVER_MASK_WIDTH: f32 = 34.;
pub const DOCUMENT_TAB_HOVER_MASK_SOLID_TAIL: f32 = 14.;
pub const DOCUMENT_TAB_CLOSE_FOCUS_MASK_GAP: &str =
    "Pinned GPUI Component Button does not expose its internal FocusHandle to sibling content";

pub fn dirty_close_title(document_name: &str) -> String {
    format!("Save changes to {document_name}?")
}

pub fn document_tab_id(tab_id: &str) -> String {
    format!("document-tab-{tab_id}")
}

pub fn document_tab_close_id(tab_id: &str) -> String {
    format!("document-tab-close-{tab_id}")
}

pub fn document_tab_label_id(tab_id: &str) -> String {
    format!("document-tab-label-{tab_id}")
}

pub fn document_tab_hover_mask_id(tab_id: &str) -> String {
    format!("document-tab-hover-mask-{tab_id}")
}

pub fn document_tab_drag_id(tab_id: &str) -> String {
    format!("document-tab-drag-{tab_id}")
}

pub fn document_tab_drop_target_id(tab_id: &str) -> String {
    format!("document-tab-drop-target-{tab_id}")
}

pub fn document_tab_close_accessible_label(document_name: &str) -> String {
    format!("Close {document_name}")
}

fn document_tab_group_id(tab_id: &str) -> String {
    format!("document-tab-group-{tab_id}")
}

pub fn format_document_tab_label(document_name: &str) -> &str {
    let Some(suffix) = document_name.get(document_name.len().saturating_sub(4)..) else {
        return document_name;
    };
    if suffix.eq_ignore_ascii_case(".pdf") {
        let label = &document_name[..document_name.len() - 4];
        if !label.is_empty() {
            return label;
        }
    }
    document_name
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateDefinition {
    pub id: &'static str,
    pub name: &'static str,
}

pub const BUILT_IN_TEMPLATES: [TemplateDefinition; 6] = [
    TemplateDefinition {
        id: "built-in-blank",
        name: "Blank Paper",
    },
    TemplateDefinition {
        id: "built-in-dots",
        name: "Dot Grid",
    },
    TemplateDefinition {
        id: "built-in-grid",
        name: "Square Grid",
    },
    TemplateDefinition {
        id: "built-in-lined",
        name: "Ruled Paper",
    },
    TemplateDefinition {
        id: "built-in-isometric",
        name: "Isometric Grid",
    },
    TemplateDefinition {
        id: "built-in-triangle",
        name: "Triangle Grid",
    },
];

pub const TEMPLATE_ITEM_IDS: [&str; 6] = [
    "template-picker-item-built-in-blank",
    "template-picker-item-built-in-dots",
    "template-picker-item-built-in-grid",
    "template-picker-item-built-in-lined",
    "template-picker-item-built-in-isometric",
    "template-picker-item-built-in-triangle",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentDocumentTab {
    pub id: String,
    pub name: String,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateCreationOrigin {
    Primary,
    Create,
    RowDoubleClick,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateCreationEvent {
    pub template_id: &'static str,
    pub origin: TemplateCreationOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplateSplitEvent {
    OpenChanged(bool),
    SelectionChanged(&'static str),
    CreateRequested {
        template_id: &'static str,
        origin: TemplateCreationOrigin,
    },
    ManageRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentTabActivationOrigin {
    Pointer,
    Keyboard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentTabEvent {
    Selected {
        tab_id: String,
        origin: DocumentTabActivationOrigin,
    },
    CleanClosed {
        tab_id: String,
        was_active: bool,
        post_close_active_tab_id: Option<String>,
    },
    DirtyCloseDeferred {
        tab_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentTabReorderEvent {
    pub tab_id: String,
    pub from_ix: usize,
    pub to_ix: usize,
    pub origin: DocumentTabReorderOrigin,
    pub announcement: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentTabReorderOrigin {
    Keyboard,
    Pointer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentTabPointerDragState {
    pub tab_id: String,
    pub start: Point<Pixels>,
    pub current: Point<Pixels>,
    pub activated: bool,
    pub over_tab_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyCloseConfirmationIntent {
    Cancel,
    Discard,
    Save,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirtyCloseConfirmationEvent {
    pub tab_id: String,
    pub intent: DirtyCloseConfirmationIntent,
}

fn template_definition(template_id: &str) -> TemplateDefinition {
    BUILT_IN_TEMPLATES
        .iter()
        .copied()
        .find(|template| template.id == template_id)
        .unwrap_or(BUILT_IN_TEMPLATES[0])
}

/// Owns the retained picker state and emits semantic template intents without
/// creating, naming, selecting, or closing document tabs.
pub struct TemplateSplitControl {
    picker_open: bool,
    selected_template_id: &'static str,
    last_template_id: &'static str,
    creating: bool,
    picker_focus: FocusHandle,
}

impl TemplateSplitControl {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            picker_open: false,
            selected_template_id: BUILT_IN_TEMPLATES[0].id,
            last_template_id: BUILT_IN_TEMPLATES[0].id,
            creating: false,
            picker_focus: cx.focus_handle(),
        }
    }

    pub fn is_picker_open(&self) -> bool {
        self.picker_open
    }

    pub fn selected_template_id(&self) -> &'static str {
        self.selected_template_id
    }

    pub fn last_template_id(&self) -> &'static str {
        self.last_template_id
    }

    pub fn is_creating(&self) -> bool {
        self.creating
    }

    pub fn picker_focus_handle(&self) -> FocusHandle {
        self.picker_focus.clone()
    }

    pub fn set_creating(&mut self, creating: bool, cx: &mut Context<Self>) {
        if self.creating == creating {
            return;
        }
        self.creating = creating;
        cx.notify();
    }

    fn request_create(
        &mut self,
        template_id: &'static str,
        origin: TemplateCreationOrigin,
        cx: &mut Context<Self>,
    ) {
        if self.creating && origin == TemplateCreationOrigin::Create {
            return;
        }
        let template = template_definition(template_id);
        self.last_template_id = template.id;
        self.selected_template_id = template.id;
        self.picker_open = false;
        cx.emit(TemplateSplitEvent::CreateRequested {
            template_id: template.id,
            origin,
        });
        cx.notify();
    }
}

impl EventEmitter<TemplateSplitEvent> for TemplateSplitControl {}

impl Render for TemplateSplitControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_template = template_definition(self.last_template_id);
        let primary_control = cx.entity().downgrade();
        let primary = Button::new(TEMPLATE_PRIMARY_ID)
            .small()
            .debug_selector(|| TEMPLATE_PRIMARY_ID.into())
            .accessibility_id(TEMPLATE_PRIMARY_ID)
            .icon(IconName::Plus)
            .tooltip(format!(
                "New from {}. Click to create.",
                current_template.name
            ))
            .on_click(move |_: &ClickEvent, _, cx| {
                let _ = primary_control.update(cx, |control, cx| {
                    control.request_create(
                        current_template.id,
                        TemplateCreationOrigin::Primary,
                        cx,
                    );
                });
            });

        let picker = Button::new(TEMPLATE_PICKER_ID)
            .small()
            .debug_selector(|| TEMPLATE_PICKER_ID.into())
            .accessibility_id(TEMPLATE_PICKER_ID)
            .icon(IconName::ChevronDown)
            .tooltip("New from template…");

        let picker_control = cx.entity().downgrade();
        let picker_content_control = cx.entity().downgrade();
        let picker_open_focus = self.picker_focus.clone();
        let picker_content_focus = self.picker_focus.clone();
        let selected_template_id = self.selected_template_id;
        let creating = self.creating;
        let picker = Popover::new("document-tab-template-picker-popover")
            .anchor(Anchor::TopLeft)
            .open(self.picker_open)
            .track_focus(&self.picker_focus)
            .on_open_change(move |open, window, cx| {
                let picker_focus = picker_open_focus.clone();
                let _ = picker_control.update(cx, |control, cx| {
                    control.picker_open = *open;
                    if *open {
                        control.selected_template_id = control.last_template_id;
                        cx.emit(TemplateSplitEvent::SelectionChanged(
                            control.selected_template_id,
                        ));
                        picker_focus.focus(window, cx);
                    }
                    cx.emit(TemplateSplitEvent::OpenChanged(*open));
                    cx.notify();
                });
            })
            .trigger(picker)
            .content(move |_, _, cx| {
                let template_rows = BUILT_IN_TEMPLATES.into_iter().zip(TEMPLATE_ITEM_IDS).map(
                    |(template, stable_id)| {
                        let control = picker_content_control.clone();
                        Button::new(stable_id)
                            .debug_selector(move || stable_id.into())
                            .accessibility_id(stable_id)
                            .label(template.name)
                            .selected(selected_template_id == template.id)
                            .toggled(selected_template_id == template.id)
                            .on_click(move |event: &ClickEvent, _, cx| {
                                let _ = control.update(cx, |control, cx| {
                                    control.selected_template_id = template.id;
                                    if event.click_count() == 2 {
                                        control.request_create(
                                            template.id,
                                            TemplateCreationOrigin::RowDoubleClick,
                                            cx,
                                        );
                                    } else {
                                        cx.emit(TemplateSplitEvent::SelectionChanged(template.id));
                                        cx.notify();
                                    }
                                });
                            })
                    },
                );

                let manage_control = picker_content_control.clone();
                let create_control = picker_content_control.clone();
                let create = Button::new(TEMPLATE_CREATE_ID)
                    .debug_selector(|| TEMPLATE_CREATE_ID.into())
                    .accessibility_id(TEMPLATE_CREATE_ID)
                    .label(if creating { "Creating…" } else { "Create" })
                    .disabled(creating)
                    .on_click(move |_: &ClickEvent, _, cx| {
                        let _ = create_control.update(cx, |control, cx| {
                            if !control.creating {
                                let selected = control.selected_template_id;
                                control.request_create(
                                    selected,
                                    TemplateCreationOrigin::Create,
                                    cx,
                                );
                            }
                        });
                    });

                v_flex()
                    .id(TEMPLATE_PICKER_POPOVER_ID)
                    .debug_selector(|| TEMPLATE_PICKER_POPOVER_ID.into())
                    .track_focus(&picker_content_focus)
                    .tab_group()
                    .w_80()
                    .gap_2()
                    .p_3()
                    .bg(cx.theme().popover)
                    .child(gpui::div().font_semibold().child("New from template"))
                    .children(template_rows)
                    .child(
                        h_flex()
                            .justify_between()
                            .gap_2()
                            .child(
                                Button::new(TEMPLATE_MANAGE_ID)
                                    .debug_selector(|| TEMPLATE_MANAGE_ID.into())
                                    .accessibility_id(TEMPLATE_MANAGE_ID)
                                    .label("Manage templates…")
                                    .on_click(move |_: &ClickEvent, _, cx| {
                                        let _ = manage_control.update(cx, |control, cx| {
                                            control.picker_open = false;
                                            cx.emit(TemplateSplitEvent::OpenChanged(false));
                                            cx.emit(TemplateSplitEvent::ManageRequested);
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(create),
                    )
            });

        // The pinned ButtonGroup only accepts concrete Button children and
        // cannot host the Popover-wrapped picker. Keep this wrapper shallow.
        h_flex()
            .id(TEMPLATE_CONTROL_GROUP_ID)
            .debug_selector(|| TEMPLATE_CONTROL_GROUP_ID.into())
            .flex_shrink_0()
            .gap_1()
            .child(primary)
            .child(picker)
    }
}

/// Owns the isolated Document Tab Bar composition and retained template state.
/// It emits deterministic experiment events and never touches production
/// document or template storage.
pub struct DocumentTabBarTemplateSeam {
    tabs: Vec<ExperimentDocumentTab>,
    active_tab_ix: usize,
    template_control: Entity<TemplateSplitControl>,
    _template_subscription: Subscription,
    picker_open: bool,
    selected_template_id: &'static str,
    last_template_id: &'static str,
    creating: bool,
    creation_events: Vec<TemplateCreationEvent>,
    manage_requests: usize,
    tab_events: Vec<DocumentTabEvent>,
    reorder_events: Vec<DocumentTabReorderEvent>,
    reorder_announcement: String,
    pointer_drag: Option<DocumentTabPointerDragState>,
    suppress_pointer_click_tab_id: Option<String>,
    pending_dirty_close_id: Option<String>,
    confirmation_busy: bool,
    confirmation_events: Vec<DirtyCloseConfirmationEvent>,
    confirmation_return_focus: Option<FocusHandle>,
    confirmation_needs_initial_focus: bool,
    hovered_tab_id: Option<String>,
    hovered_close_tab_id: Option<String>,
    tab_focus_handles: HashMap<String, FocusHandle>,
    tab_bounds: HashMap<String, Rc<Cell<Bounds<Pixels>>>>,
    focus_handle: FocusHandle,
    picker_focus: FocusHandle,
    confirmation_focus: FocusHandle,
    scroll_handle: ScrollHandle,
}

impl DocumentTabBarTemplateSeam {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let tabs = vec![
            ExperimentDocumentTab {
                id: "site-plan".into(),
                name: "site-plan".into(),
                dirty: false,
            },
            ExperimentDocumentTab {
                id: "structural-details".into(),
                name: "structural-details".into(),
                dirty: true,
            },
        ];
        let tab_focus_handles = tabs
            .iter()
            .map(|tab| (tab.id.clone(), cx.focus_handle()))
            .collect();
        let tab_bounds = tabs
            .iter()
            .map(|tab| (tab.id.clone(), Rc::new(Cell::new(Bounds::default()))))
            .collect();
        let template_control = cx.new(TemplateSplitControl::new);
        let picker_focus = template_control.read(cx).picker_focus_handle();
        let template_subscription = cx.subscribe(
            &template_control,
            |control, _, event: &TemplateSplitEvent, cx| {
                match event {
                    TemplateSplitEvent::OpenChanged(open) => control.picker_open = *open,
                    TemplateSplitEvent::SelectionChanged(template_id) => {
                        control.selected_template_id = *template_id;
                    }
                    TemplateSplitEvent::CreateRequested {
                        template_id,
                        origin,
                    } => {
                        control.create_mock_from_template(*template_id, *origin, cx);
                        return;
                    }
                    TemplateSplitEvent::ManageRequested => {
                        control.manage_requests += 1;
                    }
                }
                cx.notify();
            },
        );
        Self {
            tabs,
            active_tab_ix: 0,
            template_control,
            _template_subscription: template_subscription,
            picker_open: false,
            selected_template_id: BUILT_IN_TEMPLATES[0].id,
            last_template_id: BUILT_IN_TEMPLATES[0].id,
            creating: false,
            creation_events: Vec::new(),
            manage_requests: 0,
            tab_events: Vec::new(),
            reorder_events: Vec::new(),
            reorder_announcement: String::new(),
            pointer_drag: None,
            suppress_pointer_click_tab_id: None,
            pending_dirty_close_id: None,
            confirmation_busy: false,
            confirmation_events: Vec::new(),
            confirmation_return_focus: None,
            confirmation_needs_initial_focus: false,
            hovered_tab_id: None,
            hovered_close_tab_id: None,
            tab_focus_handles,
            tab_bounds,
            focus_handle: cx.focus_handle(),
            picker_focus,
            confirmation_focus: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn tabs(&self) -> &[ExperimentDocumentTab] {
        &self.tabs
    }

    pub fn active_tab_index(&self) -> usize {
        self.active_tab_ix
    }

    pub fn active_tab_id(&self) -> Option<&str> {
        self.tabs.get(self.active_tab_ix).map(|tab| tab.id.as_str())
    }

    pub fn tab_events(&self) -> &[DocumentTabEvent] {
        &self.tab_events
    }

    pub fn reorder_events(&self) -> &[DocumentTabReorderEvent] {
        &self.reorder_events
    }

    pub fn reorder_announcement(&self) -> &str {
        &self.reorder_announcement
    }

    pub fn pointer_drag(&self) -> Option<&DocumentTabPointerDragState> {
        self.pointer_drag.as_ref()
    }

    pub fn pending_dirty_close_id(&self) -> Option<&str> {
        self.pending_dirty_close_id.as_deref()
    }

    pub fn is_confirmation_busy(&self) -> bool {
        self.confirmation_busy
    }

    pub fn confirmation_events(&self) -> &[DirtyCloseConfirmationEvent] {
        &self.confirmation_events
    }

    pub fn confirmation_focus_handle(&self) -> FocusHandle {
        self.confirmation_focus.clone()
    }

    pub fn hovered_tab_id(&self) -> Option<&str> {
        self.hovered_tab_id.as_deref()
    }

    pub fn hovered_close_tab_id(&self) -> Option<&str> {
        self.hovered_close_tab_id.as_deref()
    }

    pub fn tab_focus_handle(&self, tab_id: &str) -> Option<FocusHandle> {
        self.tab_focus_handles.get(tab_id).cloned()
    }

    pub fn is_picker_open(&self) -> bool {
        self.picker_open
    }

    pub fn selected_template_id(&self) -> &'static str {
        self.selected_template_id
    }

    pub fn last_template_id(&self) -> &'static str {
        self.last_template_id
    }

    pub fn is_creating(&self) -> bool {
        self.creating
    }

    pub fn creation_events(&self) -> &[TemplateCreationEvent] {
        &self.creation_events
    }

    pub fn manage_requests(&self) -> usize {
        self.manage_requests
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn picker_focus_handle(&self) -> FocusHandle {
        self.picker_focus.clone()
    }

    pub fn template_control(&self) -> Entity<TemplateSplitControl> {
        self.template_control.clone()
    }

    pub fn set_creating(&mut self, creating: bool, cx: &mut Context<Self>) {
        self.creating = creating;
        self.template_control.update(cx, |control, cx| {
            control.set_creating(creating, cx);
        });
        cx.notify();
    }

    pub fn set_tab_dirty(&mut self, tab_id: &str, dirty: bool, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.dirty = dirty;
            cx.notify();
        }
    }

    pub fn set_tab_name(&mut self, tab_id: &str, name: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.name = name.into();
            cx.notify();
        }
    }

    pub fn set_confirmation_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.pending_dirty_close_id.is_some() && self.confirmation_busy != busy {
            self.confirmation_busy = busy;
            cx.notify();
        }
    }

    fn record_confirmation_intent(
        &mut self,
        intent: DirtyCloseConfirmationIntent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.confirmation_busy {
            return false;
        }
        let Some(tab_id) = self.pending_dirty_close_id.clone() else {
            return false;
        };
        self.confirmation_events
            .push(DirtyCloseConfirmationEvent { tab_id, intent });
        match intent {
            DirtyCloseConfirmationIntent::Cancel | DirtyCloseConfirmationIntent::Discard => {
                self.pending_dirty_close_id = None;
                self.confirmation_needs_initial_focus = false;
            }
            DirtyCloseConfirmationIntent::Save => {
                self.confirmation_busy = true;
            }
        }
        cx.notify();
        true
    }

    fn cancel_confirmation(&mut self, cx: &mut Context<Self>) -> Option<FocusHandle> {
        if !self.record_confirmation_intent(DirtyCloseConfirmationIntent::Cancel, cx) {
            return None;
        }
        self.confirmation_return_focus.take()
    }

    fn select_tab(
        &mut self,
        tab_id: &str,
        origin: DocumentTabActivationOrigin,
        cx: &mut Context<Self>,
    ) -> Option<FocusHandle> {
        let selected = self.tabs.iter().position(|tab| tab.id == tab_id)?;
        if selected != self.active_tab_ix {
            self.active_tab_ix = selected;
            self.tab_events.push(DocumentTabEvent::Selected {
                tab_id: tab_id.to_owned(),
                origin,
            });
            cx.notify();
        }
        self.tab_focus_handles.get(tab_id).cloned()
    }

    /// Applies the experiment-owned keyboard reorder command by stable tab ID.
    /// This mutates retained compatibility state only; it never persists order
    /// or touches a production document session.
    pub fn move_tab_by_keyboard(
        &mut self,
        tab_id: &str,
        direction: isize,
        cx: &mut Context<Self>,
    ) -> Option<FocusHandle> {
        let from_ix = self.tabs.iter().position(|tab| tab.id == tab_id)?;
        let to_ix = from_ix.checked_add_signed(direction)?;
        if to_ix >= self.tabs.len() {
            return None;
        }
        let target_tab_id = self.tabs[to_ix].id.clone();
        self.commit_tab_reorder(
            tab_id,
            &target_tab_id,
            DocumentTabReorderOrigin::Keyboard,
            cx,
        );
        self.tab_focus_handles.get(tab_id).cloned()
    }

    fn commit_tab_reorder(
        &mut self,
        tab_id: &str,
        target_tab_id: &str,
        origin: DocumentTabReorderOrigin,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(from_ix) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let Some(to_ix) = self.tabs.iter().position(|tab| tab.id == target_tab_id) else {
            return false;
        };
        if from_ix == to_ix {
            return false;
        }

        let active_tab_id = self.active_tab_id().map(str::to_owned);
        let moved_tab = self.tabs.remove(from_ix);
        let moved_name = moved_tab.name.clone();
        self.tabs.insert(to_ix, moved_tab);
        if let Some(active_tab_id) = active_tab_id {
            self.active_tab_ix = self
                .tabs
                .iter()
                .position(|tab| tab.id == active_tab_id)
                .expect("reordering must preserve the active tab identity");
        }

        let announcement = format!(
            "Moved {moved_name} to position {} of {}.",
            to_ix + 1,
            self.tabs.len()
        );
        self.reorder_announcement = announcement.clone();
        self.reorder_events.push(DocumentTabReorderEvent {
            tab_id: tab_id.to_owned(),
            from_ix,
            to_ix,
            origin,
            announcement,
        });
        cx.notify();
        true
    }

    fn begin_pointer_drag(
        &mut self,
        tab_id: &str,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.tabs.iter().any(|tab| tab.id == tab_id)
            || self.hovered_close_tab_id.as_deref() == Some(tab_id)
        {
            return false;
        }
        self.suppress_pointer_click_tab_id = None;
        self.pointer_drag = Some(DocumentTabPointerDragState {
            tab_id: tab_id.to_owned(),
            start: position,
            current: position,
            activated: false,
            over_tab_id: tab_id.to_owned(),
        });
        cx.notify();
        true
    }

    fn closest_tab_to_dragged_center(
        &self,
        tab_id: &str,
        delta: gpui::Point<Pixels>,
    ) -> Option<String> {
        let source = self.tab_bounds.get(tab_id)?.get();
        let dragged_center = source.center() + delta;
        self.tabs
            .iter()
            .filter_map(|tab| {
                let bounds = self.tab_bounds.get(&tab.id)?.get();
                let offset = bounds.center() - dragged_center;
                Some((tab.id.clone(), offset.magnitude()))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(tab_id, _)| tab_id)
    }

    /// Mirrors the exact installed dnd-kit distance sensor. The activating
    /// move starts the drag but is not applied until a later pointer move.
    fn update_pointer_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(snapshot) = self.pointer_drag.clone() else {
            return false;
        };
        let delta = position - snapshot.start;
        if !snapshot.activated {
            if delta.magnitude() <= DOCUMENT_TAB_POINTER_DRAG_THRESHOLD {
                return false;
            }
            if let Some(drag) = self.pointer_drag.as_mut() {
                drag.activated = true;
            }
            cx.notify();
            return true;
        }

        let over_tab_id = self
            .closest_tab_to_dragged_center(&snapshot.tab_id, delta)
            .unwrap_or_else(|| snapshot.tab_id.clone());
        if let Some(drag) = self.pointer_drag.as_mut() {
            drag.current = position;
            drag.over_tab_id = over_tab_id;
        }
        cx.notify();
        true
    }

    fn finish_pointer_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.pointer_drag.take() else {
            return false;
        };
        if !drag.activated {
            // Popover outside-dismiss runs during capture, before an ordinary
            // tab click completes. Keep the dirty target pending while the
            // six-pixel sensor is armed, then apply the frozen outside-click
            // cancellation if the pointer releases without activating.
            let _ = self.cancel_confirmation(cx);
            return false;
        }
        self.suppress_pointer_click_tab_id = Some(drag.tab_id.clone());
        let reordered = self.commit_tab_reorder(
            &drag.tab_id,
            &drag.over_tab_id,
            DocumentTabReorderOrigin::Pointer,
            cx,
        );
        if !reordered {
            cx.notify();
        }
        true
    }

    pub fn cancel_pointer_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.pointer_drag.take() else {
            return false;
        };
        if drag.activated {
            self.suppress_pointer_click_tab_id = Some(drag.tab_id);
        }
        cx.notify();
        true
    }

    fn take_suppressed_pointer_click(&mut self, tab_id: &str) -> bool {
        if self.suppress_pointer_click_tab_id.as_deref() != Some(tab_id) {
            return false;
        }
        self.suppress_pointer_click_tab_id = None;
        true
    }

    fn drag_shift_for_tab(&self, tab_id: &str) -> Pixels {
        let Some(drag) = self.pointer_drag.as_ref().filter(|drag| drag.activated) else {
            return px(0.);
        };
        let Some(source_ix) = self.tabs.iter().position(|tab| tab.id == drag.tab_id) else {
            return px(0.);
        };
        let Some(over_ix) = self.tabs.iter().position(|tab| tab.id == drag.over_tab_id) else {
            return px(0.);
        };
        let Some(ix) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return px(0.);
        };
        let Some(source_bounds) = self.tab_bounds.get(&drag.tab_id).map(|bounds| bounds.get())
        else {
            return px(0.);
        };

        if ix == source_ix {
            let Some(target_bounds) = self
                .tab_bounds
                .get(&drag.over_tab_id)
                .map(|bounds| bounds.get())
            else {
                return px(0.);
            };
            return if source_ix < over_ix {
                target_bounds.right() - source_bounds.right()
            } else {
                target_bounds.left() - source_bounds.left()
            };
        }

        if ix > source_ix && ix <= over_ix {
            let current = self.tab_bounds[tab_id].get();
            let previous = self.tab_bounds[&self.tabs[ix - 1].id].get();
            return -(source_bounds.size.width + (current.left() - previous.right()));
        }

        if ix < source_ix && ix >= over_ix {
            let current = self.tab_bounds[tab_id].get();
            let next = self.tab_bounds[&self.tabs[ix + 1].id].get();
            return source_bounds.size.width + (next.left() - current.right());
        }

        px(0.)
    }

    fn request_close_tab(&mut self, tab_id: &str, cx: &mut Context<Self>) -> Option<FocusHandle> {
        let closing_ix = self.tabs.iter().position(|tab| tab.id == tab_id)?;
        if self.tabs[closing_ix].dirty {
            if self.confirmation_busy || self.pending_dirty_close_id.as_deref() == Some(tab_id) {
                return None;
            }
            self.pending_dirty_close_id = Some(tab_id.to_owned());
            self.confirmation_needs_initial_focus = true;
            self.tab_events.push(DocumentTabEvent::DirtyCloseDeferred {
                tab_id: tab_id.to_owned(),
            });
            cx.notify();
            return None;
        }

        let was_active = closing_ix == self.active_tab_ix;
        self.tabs.remove(closing_ix);
        self.tab_focus_handles.remove(tab_id);
        self.tab_bounds.remove(tab_id);
        if self.hovered_tab_id.as_deref() == Some(tab_id) {
            self.hovered_tab_id = None;
        }
        if self.hovered_close_tab_id.as_deref() == Some(tab_id) {
            self.hovered_close_tab_id = None;
        }
        if self.tabs.is_empty() {
            self.active_tab_ix = 0;
        } else if was_active {
            self.active_tab_ix = closing_ix.min(self.tabs.len() - 1);
        } else if closing_ix < self.active_tab_ix {
            self.active_tab_ix -= 1;
        }

        let post_close_active_tab_id = self.active_tab_id().map(str::to_owned);
        self.tab_events.push(DocumentTabEvent::CleanClosed {
            tab_id: tab_id.to_owned(),
            was_active,
            post_close_active_tab_id: post_close_active_tab_id.clone(),
        });
        cx.notify();

        if was_active {
            post_close_active_tab_id
                .as_deref()
                .and_then(|id| self.tab_focus_handles.get(id).cloned())
        } else {
            None
        }
    }

    fn create_mock_from_template(
        &mut self,
        template_id: &'static str,
        origin: TemplateCreationOrigin,
        cx: &mut Context<Self>,
    ) {
        let template = template_definition(template_id);
        let sequence = self.creation_events.len() + 1;
        let tab_id = format!("template-document-{sequence}");
        self.tab_focus_handles
            .insert(tab_id.clone(), cx.focus_handle());
        self.tab_bounds
            .insert(tab_id.clone(), Rc::new(Cell::new(Bounds::default())));
        self.tabs.push(ExperimentDocumentTab {
            id: tab_id,
            name: template.name.into(),
            dirty: true,
        });
        self.active_tab_ix = self.tabs.len() - 1;
        self.last_template_id = template.id;
        self.selected_template_id = template.id;
        self.picker_open = false;
        self.creation_events.push(TemplateCreationEvent {
            template_id: template.id,
            origin,
        });
        cx.notify();
    }
}

impl Render for DocumentTabBarTemplateSeam {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_ids: Vec<String> = self.tabs.iter().map(|tab| tab.id.clone()).collect();
        let tab_count = self.tabs.len();
        let mut rendered_tabs = Vec::with_capacity(tab_count);
        for (ix, tab) in self.tabs.clone().into_iter().enumerate() {
            let selector = document_tab_id(&tab.id);
            let accessibility_id = selector.clone();
            let tab_debug_selector = selector.clone();
            let close_selector = document_tab_close_id(&tab.id);
            let label_selector = document_tab_label_id(&tab.id);
            let hover_mask_selector = document_tab_hover_mask_id(&tab.id);
            let drag_selector = document_tab_drag_id(&tab.id);
            let drop_target_selector = document_tab_drop_target_id(&tab.id);
            let group_id = document_tab_group_id(&tab.id);
            let is_active = ix == self.active_tab_ix;
            let is_drag_activated = self
                .pointer_drag
                .as_ref()
                .is_some_and(|drag| drag.activated);
            let is_dragged = self
                .pointer_drag
                .as_ref()
                .is_some_and(|drag| drag.activated && drag.tab_id == tab.id);
            let is_drop_target = self
                .pointer_drag
                .as_ref()
                .is_some_and(|drag| drag.activated && drag.over_tab_id == tab.id);
            let drag_shift = self.drag_shift_for_tab(&tab.id);
            let tab_bounds_cell = self
                .tab_bounds
                .get(&tab.id)
                .expect("every retained document tab must own a bounds cell")
                .clone();
            let tab_focus = self
                .tab_focus_handles
                .get(&tab.id)
                .expect("every retained document tab must own a focus handle")
                .clone();
            let close_control = cx.entity().downgrade();
            let close_tab_id = tab.id.clone();
            let close_debug_selector = close_selector.clone();
            let close_accessibility_id = close_selector.clone();
            let close_hover_control = cx.entity().downgrade();
            let close_hover_tab_id = tab.id.clone();
            let dirty = tab.dirty;
            let close = Button::new(close_selector)
                .small()
                .ghost()
                .debug_selector(move || close_debug_selector.clone().into())
                .accessibility_id(close_accessibility_id)
                .icon(IconName::Close)
                .tab_stop(is_active)
                .opacity(0.)
                .group_hover(group_id.clone(), |style| style.opacity(1.))
                .focus(|style| style.opacity(1.))
                .on_hover(move |hovered, _, cx| {
                    let _ = close_hover_control.update(cx, |control, cx| {
                        if *hovered {
                            control.hovered_close_tab_id = Some(close_hover_tab_id.clone());
                        } else if control.hovered_close_tab_id.as_deref()
                            == Some(close_hover_tab_id.as_str())
                        {
                            control.hovered_close_tab_id = None;
                        }
                        cx.notify();
                    });
                })
                .on_click(move |event: &ClickEvent, window: &mut Window, cx| {
                    let keyboard_activation = matches!(event, ClickEvent::Keyboard(_));
                    if dirty && !keyboard_activation {
                        // The real Popover trigger owns dirty-close pointer
                        // activation on mouse down. Do not duplicate that
                        // state transition when the Button emits its click.
                        cx.stop_propagation();
                        return;
                    }
                    let return_focus = keyboard_activation.then(|| window.focused(cx)).flatten();
                    let successor_focus = close_control
                        .update(cx, |control, cx| {
                            let pointer_is_over_close = control.hovered_close_tab_id.as_deref()
                                == Some(close_tab_id.as_str());
                            if !pointer_is_over_close && !keyboard_activation {
                                return None;
                            }
                            if dirty {
                                if control.confirmation_busy
                                    || control.pending_dirty_close_id.as_deref()
                                        == Some(close_tab_id.as_str())
                                {
                                    return None;
                                }
                                control.confirmation_return_focus = return_focus.clone();
                                control.request_close_tab(&close_tab_id, cx);
                                return Some(control.confirmation_focus.clone());
                            }
                            control.request_close_tab(&close_tab_id, cx)
                        })
                        .ok()
                        .flatten();
                    if let Some(successor_focus) = successor_focus {
                        successor_focus.focus(window, cx);
                    }
                    cx.stop_propagation();
                });

            let close = if tab.dirty {
                let document_name = tab.name.clone();
                let confirmation_open = self.pending_dirty_close_id.as_deref() == Some(&tab.id);
                let confirmation_busy = self.confirmation_busy && confirmation_open;
                let confirmation_needs_initial_focus =
                    self.confirmation_needs_initial_focus && confirmation_open;
                if confirmation_needs_initial_focus {
                    self.confirmation_needs_initial_focus = false;
                }
                let confirmation_focus = self.confirmation_focus.clone();
                let open_confirmation_focus = confirmation_focus.clone();
                let content_confirmation_focus = confirmation_focus.clone();
                let confirmation_tab_id = tab.id.clone();
                let open_control = cx.entity().downgrade();
                let content_control = cx.entity().downgrade();
                let trigger_mouse_button = if self.hovered_close_tab_id.as_deref() == Some(&tab.id)
                    && !confirmation_busy
                {
                    MouseButton::Left
                } else {
                    MouseButton::Right
                };
                Popover::new(format!("document-tab-dirty-close-popover-{}", tab.id))
                    .anchor(Anchor::TopRight)
                    .open(confirmation_open)
                    .mouse_button(trigger_mouse_button)
                    .overlay_closable(!confirmation_busy)
                    .track_focus(&confirmation_focus)
                    .on_open_change(move |open, window, cx| {
                        let confirmation_focus = open_confirmation_focus.clone();
                        if *open {
                            let _ = open_control.update(cx, |control, cx| {
                                control.request_close_tab(&confirmation_tab_id, cx);
                                confirmation_focus.focus(window, cx);
                            });
                            return;
                        }

                        let return_focus = open_control
                            .update(cx, |control, cx| {
                                if control.pointer_drag.is_some() {
                                    // The owner-document bridge has already
                                    // armed this pointer. Activation decides
                                    // whether this remains an outside click or
                                    // becomes a drag that preserves the target.
                                    return None;
                                }
                                control.cancel_confirmation(cx)
                            })
                            .ok()
                            .flatten();
                        if let Some(return_focus) = return_focus {
                            return_focus.focus(window, cx);
                        }
                    })
                    .w_80()
                    .trigger(close)
                    .content(move |_, window, cx| {
                        if confirmation_needs_initial_focus {
                            let confirmation_focus = content_confirmation_focus.clone();
                            window.defer(cx, move |window, cx| {
                                confirmation_focus.focus(window, cx);
                                window.focus_next(cx);
                            });
                        }
                        let popover = cx.entity();
                        let cancel_popover = popover.clone();
                        let discard_popover = popover.clone();
                        let cancel_control = content_control.clone();
                        let discard_control = content_control.clone();
                        let save_control = content_control.clone();
                        v_flex()
                            .id(DIRTY_CLOSE_CONFIRMATION_ID)
                            .debug_selector(|| DIRTY_CLOSE_CONFIRMATION_ID.into())
                            .track_focus(&confirmation_focus)
                            .tab_group()
                            .when(confirmation_busy, |this| {
                                this.on_action(|_: &Cancel, _, cx| cx.stop_propagation())
                            })
                            .w_full()
                            .gap_3()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        gpui::div()
                                            .id(DIRTY_CLOSE_TITLE_ID)
                                            .debug_selector(|| DIRTY_CLOSE_TITLE_ID.into())
                                            .font_semibold()
                                            .child(dirty_close_title(&document_name)),
                                    )
                                    .child(
                                        gpui::div()
                                            .id(DIRTY_CLOSE_DESCRIPTION_ID)
                                            .debug_selector(|| DIRTY_CLOSE_DESCRIPTION_ID.into())
                                            .text_color(cx.theme().muted_foreground)
                                            .child(DIRTY_CLOSE_DESCRIPTION),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new(DIRTY_CLOSE_CANCEL_ID)
                                            .debug_selector(|| DIRTY_CLOSE_CANCEL_ID.into())
                                            .accessibility_id(DIRTY_CLOSE_CANCEL_ID)
                                            .outline()
                                            .label("Cancel")
                                            .disabled(confirmation_busy)
                                            .on_click(move |_: &ClickEvent, window, cx| {
                                                if cancel_control
                                                    .read_with(cx, |control, _| {
                                                        control.confirmation_busy
                                                    })
                                                    .unwrap_or(true)
                                                {
                                                    return;
                                                }
                                                cancel_popover.update(cx, |popover, cx| {
                                                    popover.dismiss(window, cx)
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new(DIRTY_CLOSE_DISCARD_ID)
                                            .debug_selector(|| DIRTY_CLOSE_DISCARD_ID.into())
                                            .accessibility_id(DIRTY_CLOSE_DISCARD_ID)
                                            .danger()
                                            .label("Discard")
                                            .disabled(confirmation_busy)
                                            .on_click(move |_: &ClickEvent, window, cx| {
                                                let return_focus = discard_control
                                                    .update(cx, |control, cx| {
                                                        if control.record_confirmation_intent(
                                                            DirtyCloseConfirmationIntent::Discard,
                                                            cx,
                                                        ) {
                                                            control.confirmation_return_focus.take()
                                                        } else {
                                                            None
                                                        }
                                                    })
                                                    .ok()
                                                    .flatten();
                                                discard_popover.update(cx, |popover, cx| {
                                                    popover.dismiss(window, cx)
                                                });
                                                if let Some(return_focus) = return_focus {
                                                    return_focus.focus(window, cx);
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new(DIRTY_CLOSE_SAVE_ID)
                                            .debug_selector(|| DIRTY_CLOSE_SAVE_ID.into())
                                            .accessibility_id(DIRTY_CLOSE_SAVE_ID)
                                            .primary()
                                            .label(if confirmation_busy {
                                                "Saving…"
                                            } else {
                                                "Save"
                                            })
                                            .disabled(confirmation_busy)
                                            .on_click(move |_: &ClickEvent, _, cx| {
                                                let _ = save_control.update(cx, |control, cx| {
                                                    control.record_confirmation_intent(
                                                        DirtyCloseConfirmationIntent::Save,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    ),
                            )
                    })
                    .into_any_element()
            } else {
                close.into_any_element()
            };

            let pointer_control = cx.entity().downgrade();
            let pointer_tab_id = tab.id.clone();
            let pointer_focus = tab_focus.clone();
            let hover_control = cx.entity().downgrade();
            let hover_tab_id = tab.id.clone();
            let keyboard_control = cx.entity().downgrade();
            let keyboard_tab_id = tab.id.clone();
            let keyboard_ids = tab_ids.clone();
            let keyboard_focus_handles = self.tab_focus_handles.clone();
            let visual_label = format_document_tab_label(&tab.name);
            let accessibility_label = if tab.dirty {
                format!("{visual_label}, Unsaved changes")
            } else {
                visual_label.to_owned()
            };
            let visual_label = if tab.dirty {
                format!("* {visual_label}")
            } else {
                visual_label.to_owned()
            };
            let label_debug_selector = label_selector.clone();
            // The pinned Tab does not expose an ID for its internal text box.
            // This zero-impact tracer follows the allocated label region while
            // the real Tab label keeps ownership of shaping and ellipsis.
            let label_trace = gpui::div()
                .id(label_selector)
                .debug_selector(move || label_debug_selector.clone().into())
                .absolute()
                .left(px(10.))
                .right(px(10.))
                .top_0()
                .bottom_0();
            let bounds_trace = canvas(
                move |bounds, _, _| {
                    if !is_drag_activated {
                        tab_bounds_cell.set(bounds);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .bottom_0();
            let drag_debug_selector = drag_selector.clone();
            let drag_trace = is_dragged.then(|| {
                gpui::div()
                    .id(drag_selector)
                    .debug_selector(move || drag_debug_selector.clone().into())
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .bottom_0()
            });
            let drop_debug_selector = drop_target_selector.clone();
            let drop_target_trace = is_drop_target.then(|| {
                gpui::div()
                    .id(drop_target_selector)
                    .debug_selector(move || drop_debug_selector.clone().into())
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .bottom_0()
            });
            let show_hover_mask = self.hovered_tab_id.as_deref() == Some(tab.id.as_str());
            let hover_mask_debug_selector = hover_mask_selector.clone();
            let mask_background = if is_active {
                cx.theme().tokens.tab_active.color
            } else {
                cx.theme().background
            };
            let hover_mask = show_hover_mask.then(|| {
                let fade_end = (DOCUMENT_TAB_HOVER_MASK_WIDTH - DOCUMENT_TAB_HOVER_MASK_SOLID_TAIL)
                    / DOCUMENT_TAB_HOVER_MASK_WIDTH;
                gpui::div()
                    .id(hover_mask_selector)
                    .debug_selector(move || hover_mask_debug_selector.clone().into())
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(DOCUMENT_TAB_HOVER_MASK_WIDTH))
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(mask_background.opacity(0.), 0.),
                        linear_color_stop(mask_background, fade_end),
                    ))
            });

            let rendered_tab = Tab::new()
                .debug_selector(move || tab_debug_selector.clone().into())
                .accessibility_id(accessibility_id)
                .aria_description(DOCUMENT_TAB_REORDER_DESCRIPTION)
                .aria_keyshortcuts(DOCUMENT_TAB_REORDER_KEYSHORTCUTS)
                .label(visual_label)
                .aria_label(accessibility_label)
                .child(label_trace)
                .child(bounds_trace)
                .children(drag_trace)
                .children(drop_target_trace)
                .children(hover_mask)
                .child(
                    gpui::div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(close),
                )
                .group(group_id)
                .when(drag_shift != px(0.), |this| {
                    this.relative().left(drag_shift)
                })
                .track_focus(&tab_focus.tab_stop(is_active))
                .on_hover(move |hovered, _, cx| {
                    let _ = hover_control.update(cx, |control, cx| {
                        if *hovered {
                            control.hovered_tab_id = Some(hover_tab_id.clone());
                        } else if control.hovered_tab_id.as_deref() == Some(hover_tab_id.as_str()) {
                            control.hovered_tab_id = None;
                        }
                        cx.notify();
                    });
                })
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    let modifiers = event.keystroke.modifiers;
                    if modifiers.alt && modifiers.shift {
                        let direction = match event.keystroke.key.as_str() {
                            "left" => Some(-1),
                            "right" => Some(1),
                            _ => None,
                        };
                        let Some(direction) = direction else {
                            return;
                        };
                        let target_focus = keyboard_control
                            .update(cx, |control, cx| {
                                control.move_tab_by_keyboard(&keyboard_tab_id, direction, cx)
                            })
                            .ok()
                            .flatten();
                        if let Some(target_focus) = target_focus {
                            target_focus.focus(window, cx);
                        }
                        cx.stop_propagation();
                        return;
                    }
                    if modifiers.alt || modifiers.control || modifiers.platform || tab_count == 0 {
                        return;
                    }
                    let target_ix = match event.keystroke.key.as_str() {
                        "left" => Some((ix + tab_count - 1) % tab_count),
                        "right" => Some((ix + 1) % tab_count),
                        "home" => Some(0),
                        "end" => Some(tab_count - 1),
                        _ => None,
                    };
                    let Some(target_ix) = target_ix else {
                        return;
                    };
                    let target_id = keyboard_ids[target_ix].clone();
                    let target_focus = keyboard_focus_handles.get(&target_id).cloned();
                    let _ = keyboard_control.update(cx, |control, cx| {
                        control.select_tab(&target_id, DocumentTabActivationOrigin::Keyboard, cx);
                    });
                    if let Some(target_focus) = target_focus {
                        target_focus.focus(window, cx);
                    }
                    cx.stop_propagation();
                })
                .on_click(move |_: &ClickEvent, window, cx| {
                    let suppressed = pointer_control
                        .update(cx, |control, _| {
                            let active_drag = control.pointer_drag.as_ref().is_some_and(|drag| {
                                drag.activated && drag.tab_id == pointer_tab_id
                            });
                            active_drag || control.take_suppressed_pointer_click(&pointer_tab_id)
                        })
                        .unwrap_or(false);
                    if suppressed {
                        cx.stop_propagation();
                        return;
                    }
                    let _ = pointer_control.update(cx, |control, cx| {
                        control.select_tab(
                            &pointer_tab_id,
                            DocumentTabActivationOrigin::Pointer,
                            cx,
                        );
                    });
                    pointer_focus.focus(window, cx);
                });
            rendered_tabs.push(rendered_tab);
        }

        let tabs = TabBar::new("document-tabs")
            .small()
            .max_width(px(DOCUMENT_TAB_MAX_WIDTH))
            .selected_index(self.active_tab_ix)
            .children(rendered_tabs);

        let actions = h_flex()
            .id(DOCUMENT_TAB_ACTIONS_ID)
            .debug_selector(|| DOCUMENT_TAB_ACTIONS_ID.into())
            .flex_shrink_0()
            .gap_2()
            .child(
                Button::new(DOCUMENT_TAB_OPEN_ID)
                    .small()
                    .debug_selector(|| DOCUMENT_TAB_OPEN_ID.into())
                    .accessibility_id(DOCUMENT_TAB_OPEN_ID)
                    .icon(IconName::Plus)
                    .tooltip("Open PDF"),
            )
            .child(self.template_control.clone());

        let reorder_announcement = self.reorder_announcement.clone();
        let reorder_status = gpui::div()
            .id(DOCUMENT_TAB_REORDER_STATUS_ID)
            .debug_selector(|| DOCUMENT_TAB_REORDER_STATUS_ID.into())
            .accessibility_id(DOCUMENT_TAB_REORDER_STATUS_ID)
            .role(Role::Status)
            .aria_label(reorder_announcement)
            .a11y_synthetic_children(|builder| builder.parent_node().set_live(Live::Polite))
            .absolute()
            .left_0()
            .bottom_0()
            .size(px(1.))
            .overflow_hidden()
            .opacity(0.);

        // dnd-kit observes the owner document instead of calling DOM pointer
        // capture. This zero-size paint bridge is the pinned GPUI equivalent:
        // stable rendered tab bounds arm pointer down before a Popover's
        // outside-dismiss listener, then the bridge observes the window stream
        // until release or cancellation.
        let down_control = cx.entity().downgrade();
        let move_control = cx.entity().downgrade();
        let up_control = cx.entity().downgrade();
        let pointer_event_bridge = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let focus = down_control
                        .update(cx, |control, cx| {
                            let tab_id = control.tabs.iter().find_map(|tab| {
                                let bounds = control.tab_bounds.get(&tab.id)?.get();
                                (bounds.contains(&event.position)
                                    && control.hovered_close_tab_id.as_deref()
                                        != Some(tab.id.as_str()))
                                .then(|| tab.id.clone())
                            });
                            let tab_id = tab_id?;
                            let focus = control.tab_focus_handles.get(&tab_id).cloned();
                            control
                                .begin_pointer_drag(&tab_id, event.position, cx)
                                .then_some(focus)
                                .flatten()
                        })
                        .ok()
                        .flatten();
                    if let Some(focus) = focus {
                        // Browser pointer down focuses the sortable trigger.
                        // Preserve that activator identity without adding the
                        // keyboard command's post-commit restoration step.
                        focus.focus(window, cx);
                    }
                });
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    if event.pressed_button != Some(MouseButton::Left) {
                        let _ = move_control.update(cx, |control, cx| {
                            control.cancel_pointer_drag(cx);
                        });
                        return;
                    }
                    let activated = move_control
                        .update(cx, |control, cx| {
                            control.update_pointer_drag(event.position, cx)
                        })
                        .unwrap_or(false);
                    if activated {
                        window.prevent_default();
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        let _ = up_control.update(cx, |control, cx| {
                            control.finish_pointer_drag(cx);
                        });
                    }
                });
            },
        )
        .absolute()
        .left_0()
        .bottom_0()
        .size(px(1.));
        let root_cancel_control = cx.entity().downgrade();

        h_flex()
            .id(DOCUMENT_TAB_BAR_ID)
            .debug_selector(|| DOCUMENT_TAB_BAR_ID.into())
            .tab_group()
            .w_full()
            .min_w_0()
            .relative()
            .track_focus(&self.focus_handle)
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if event.keystroke.key != "escape" {
                    return;
                }
                let canceled = root_cancel_control
                    .update(cx, |control, cx| control.cancel_pointer_drag(cx))
                    .unwrap_or(false);
                if canceled {
                    cx.stop_propagation();
                }
            })
            .p_2()
            .child(
                h_flex()
                    .id(DOCUMENT_TAB_SURFACE_ID)
                    .debug_selector(|| DOCUMENT_TAB_SURFACE_ID.into())
                    .w_full()
                    .min_w_0()
                    .child(
                        h_flex()
                            .id(DOCUMENT_TAB_LIST_ID)
                            .debug_selector(|| DOCUMENT_TAB_LIST_ID.into())
                            .w_full()
                            .min_w_0()
                            .overflow_x_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(
                                h_flex()
                                    .id(DOCUMENT_TAB_CONTENT_ID)
                                    .debug_selector(|| DOCUMENT_TAB_CONTENT_ID.into())
                                    .w_auto()
                                    .min_w_full()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap_2()
                                    .child(h_flex().flex_shrink_0().child(tabs))
                                    .child(actions),
                            ),
                    ),
            )
            .child(reorder_status)
            .child(pointer_event_bridge)
    }
}
