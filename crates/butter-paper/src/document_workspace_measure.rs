//! Measuring with the drawing tools: Dimension, Polyline and Polygon each
//! have a Measure switch. With it on they draw Revu's measurements (a
//! Length, a Polylength, an Area); off, the plain markup. A Dimension
//! measures by default wherever its page has a scale.

use super::*;

/// A drawing tool with a Measure switch, and the two markups it draws.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MeasureFamily {
    Dimension,
    Polyline,
    Polygon,
}

impl MeasureFamily {
    pub const ALL: [Self; 3] = [Self::Dimension, Self::Polyline, Self::Polygon];

    /// The family a tool belongs to.
    pub fn of(tool: AnnotationTool) -> Option<Self> {
        match tool {
            AnnotationTool::Dimension | AnnotationTool::Length => Some(Self::Dimension),
            AnnotationTool::Polyline | AnnotationTool::Polylength => Some(Self::Polyline),
            AnnotationTool::Polygon | AnnotationTool::Area => Some(Self::Polygon),
            _ => None,
        }
    }

    /// The tool that draws the family's markup, measured or not.
    pub fn tool(self, measure: bool) -> AnnotationTool {
        match (self, measure) {
            (Self::Dimension, false) => AnnotationTool::Dimension,
            (Self::Dimension, true) => AnnotationTool::Length,
            (Self::Polyline, false) => AnnotationTool::Polyline,
            (Self::Polyline, true) => AnnotationTool::Polylength,
            (Self::Polygon, false) => AnnotationTool::Polygon,
            (Self::Polygon, true) => AnnotationTool::Area,
        }
    }

    /// Whether `tool` draws the family's measurement.
    pub fn measures(tool: AnnotationTool) -> bool {
        matches!(
            tool,
            AnnotationTool::Length | AnnotationTool::Polylength | AnnotationTool::Area
        )
    }

    /// The tool's name as the rail and its properties show it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Dimension => "Dimension",
            Self::Polyline => "Polyline",
            Self::Polygon => "Polygon",
        }
    }

    /// What Measure adds, for its switch's tooltip.
    pub fn measure_hint(self) -> &'static str {
        match self {
            Self::Dimension => "Measures the line with the page scale (Length)",
            Self::Polyline => "Measures the total length with the page scale (Polylength)",
            Self::Polygon => "Measures the area with the page scale (Area)",
        }
    }

    /// Off by default but for Dimension, which measures wherever the page
    /// has a scale.
    pub const fn default_measure(self) -> bool {
        matches!(self, Self::Dimension)
    }
}

/// Each family's Measure switch, as last set while the app is open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasurePreferences {
    dimension: bool,
    polyline: bool,
    polygon: bool,
}

impl Default for MeasurePreferences {
    fn default() -> Self {
        Self {
            dimension: MeasureFamily::Dimension.default_measure(),
            polyline: MeasureFamily::Polyline.default_measure(),
            polygon: MeasureFamily::Polygon.default_measure(),
        }
    }
}

impl MeasurePreferences {
    pub fn get(self, family: MeasureFamily) -> bool {
        match family {
            MeasureFamily::Dimension => self.dimension,
            MeasureFamily::Polyline => self.polyline,
            MeasureFamily::Polygon => self.polygon,
        }
    }

    pub fn set(&mut self, family: MeasureFamily, measure: bool) {
        match family {
            MeasureFamily::Dimension => self.dimension = measure,
            MeasureFamily::Polyline => self.polyline = measure,
            MeasureFamily::Polygon => self.polygon = measure,
        }
    }
}

impl DocumentWorkspace {
    /// Sets a family's Measure switch without choosing its tool.
    pub fn set_measure_preference_for_test(&mut self, family: MeasureFamily, measure: bool) {
        self.measure_preferences.set(family, measure);
    }

    /// Whether a family's Measure switch is on.
    pub fn measure_preference(&self, family: MeasureFamily) -> bool {
        self.measure_preferences.get(family)
    }

    /// The tool a family draws on `page_index`: a Dimension chosen from the
    /// rail measures only where the page has a scale; one chosen measuring
    /// (Shift+Alt+L, or its switch turned on), a Polyline or a Polygon
    /// measures whenever its switch is on, asking for a scale on its first
    /// click if need be.
    fn measure_family_tool(
        &self,
        document_id: DocumentId,
        page_index: u32,
        family: MeasureFamily,
        cx: &App,
    ) -> AnnotationTool {
        let measure = self.measure_preference(family)
            && (family != MeasureFamily::Dimension
                || self.dimension_measures_unscaled.contains(&document_id)
                || self.page_scale(document_id, page_index, cx).is_some());
        family.tool(measure)
    }

    /// Chooses a family's tool, from the rail or a shortcut: `measure` sets
    /// its switch first (the Length, Polylength and Area shortcuts), and
    /// `None` keeps it as last set.
    pub fn choose_measure_family(
        &mut self,
        document_id: DocumentId,
        family: MeasureFamily,
        measure: Option<bool>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(measure) = measure {
            self.measure_preferences.set(family, measure);
        }
        if family == MeasureFamily::Dimension {
            // The rail button keeps Dimension's default: typed text where a
            // page has no scale. Asking to measure measures everywhere.
            if measure == Some(true) {
                self.dimension_measures_unscaled.insert(document_id);
            } else {
                self.dimension_measures_unscaled.remove(&document_id);
            }
        }
        let page_index = self
            .session(document_id, cx)
            .map(|session| session.read(cx).current_page)
            .unwrap_or_default();
        let tool = self.measure_family_tool(document_id, page_index, family, cx);
        self.set_annotation_tool(document_id, tool, cx)
    }

    /// The Measure switch of the chosen tool's properties. On, for a page
    /// without a scale, the Scale panel opens to choose one.
    pub fn set_tool_measure(
        &mut self,
        document_id: DocumentId,
        family: MeasureFamily,
        measure: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.choose_measure_family(document_id, family, Some(measure), cx)?;
        if measure
            && let Some((target_document, page_index)) = self.scale_target_page(cx)
            && target_document == document_id
            && self.page_scale(document_id, page_index, cx).is_none()
        {
            self.page_scale_switching_on = Some((document_id, page_index));
            self.open_page_scale_dialog(document_id, page_index, window, cx);
        }
        cx.notify();
        Ok(())
    }

    /// Before a press on a page: a Dimension draws a Length where the page
    /// has a scale and its switch is on, and a plain Dimension elsewhere. A
    /// line being placed keeps its tool.
    pub(super) fn resolve_dimension_tool(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let annotations = &session.read(cx).annotations;
        let tool = annotations.tool();
        if MeasureFamily::of(tool) != Some(MeasureFamily::Dimension)
            || annotations.length_placement_pending(document_id.value())
            || annotations.dimension_placement_pending(document_id.value())
        {
            return;
        }
        let resolved =
            self.measure_family_tool(document_id, page_index, MeasureFamily::Dimension, cx);
        if resolved != tool {
            let _ = self.set_annotation_tool(document_id, resolved, cx);
        }
    }

    /// The Measure switch of the selected markups' properties: each
    /// Dimension, Polyline or Polygon selected becomes a Length, Polylength
    /// or Area, or back, as one undoable change. Measured on a page without
    /// a scale, the Scale panel then opens to choose one.
    pub fn set_selected_measure(
        &mut self,
        document_id: DocumentId,
        measure: bool,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.active_document_id != Some(document_id)
            || self.pending_close_document_id == Some(document_id)
            || self.pending_save_prompt_document_id() == Some(document_id)
        {
            return Ok(false);
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Ok(false);
        };
        let (ids, pages) = {
            let annotations = &session.read(cx).annotations;
            let ids = annotations.selected_ids(document_id.value()).to_vec();
            let candidates = annotations.measure_candidates(document_id.value(), &ids);
            let pages = candidates
                .iter()
                .filter(|(_, _, measured)| *measured != measure)
                .map(|(_, page_index, _)| *page_index)
                .collect::<std::collections::BTreeSet<_>>();
            (ids, pages)
        };
        if pages.is_empty() {
            return Ok(false);
        }
        let changed =
            self.update_native_session_history_with_result(document_id, cx, move |session| {
                session
                    .annotations
                    .set_measure(document_id.value(), &ids, measure)
                    .map_err(|error| error.to_string())
            })?;
        if measure
            && let Some(page_index) = pages
                .into_iter()
                .find(|page_index| self.page_scale(document_id, *page_index, cx).is_none())
        {
            self.scale_panel_requested = Some((document_id, page_index));
        }
        cx.notify();
        Ok(changed > 0)
    }

    /// Opens the Scale panel a Measure switch asked for.
    pub(super) fn open_requested_scale_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((document_id, page_index)) = self.scale_panel_requested.take() else {
            return;
        };
        if self.active_document_id == Some(document_id) {
            self.page_scale_switching_on = Some((document_id, page_index));
            self.open_page_scale_dialog(document_id, page_index, window, cx);
        }
    }
}
