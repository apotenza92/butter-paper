pub const MIN_ZOOM_PERCENT: f32 = 6.25;
pub const MAX_ZOOM_PERCENT: f32 = 6400.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FitMode {
    Width,
    Page,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellModel {
    pub active_document: usize,
    pub active_tool: usize,
    pub fit_mode: FitMode,
    pub continuous: bool,
    pub zoom_percent: f32,
}

impl Default for ShellModel {
    fn default() -> Self {
        Self {
            active_document: 0,
            active_tool: 0,
            fit_mode: FitMode::Width,
            continuous: true,
            zoom_percent: 100.0,
        }
    }
}

impl ShellModel {
    pub fn change_zoom(&mut self, direction: i8) {
        let next = if direction.is_positive() {
            self.zoom_percent * 1.1
        } else {
            self.zoom_percent / 1.1
        };
        self.zoom_percent = next.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);
    }

    pub fn select_fit_mode(&mut self, index: usize) {
        self.fit_mode = if index == 0 {
            FitMode::Width
        } else {
            FitMode::Page
        };
    }

    pub fn select_scroll_mode(&mut self, index: usize) {
        self.continuous = index == 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_uses_product_step_and_limits() {
        let mut model = ShellModel::default();
        model.change_zoom(1);
        assert!((model.zoom_percent - 110.0).abs() < f32::EPSILON);
        model.zoom_percent = MAX_ZOOM_PERCENT;
        model.change_zoom(1);
        assert_eq!(model.zoom_percent, MAX_ZOOM_PERCENT);
        model.zoom_percent = MIN_ZOOM_PERCENT;
        model.change_zoom(-1);
        assert_eq!(model.zoom_percent, MIN_ZOOM_PERCENT);
    }

    #[test]
    fn fit_mode_selection_is_exclusive() {
        let mut model = ShellModel::default();
        model.select_fit_mode(1);
        assert_eq!(model.fit_mode, FitMode::Page);
        model.select_fit_mode(0);
        assert_eq!(model.fit_mode, FitMode::Width);
    }

    #[test]
    fn scroll_mode_selection_is_exclusive() {
        let mut model = ShellModel::default();
        model.select_scroll_mode(1);
        assert!(!model.continuous);
        model.select_scroll_mode(0);
        assert!(model.continuous);
    }
}
