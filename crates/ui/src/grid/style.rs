use egui::Color32;

/// Colours for the dark theme, the only one the application has.
#[derive(Clone, Debug, PartialEq)]
pub struct GridStyle {
    pub measure: Color32,
    pub measure_active: Color32,
    pub main_beat: Color32,
    pub straight: Color32,
    pub triplet: Color32,
    pub ghost: Color32,
    pub label_text: Color32,
    pub label_background: Color32,
    pub label_background_hover: Color32,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            measure: Color32::from_rgb(222, 226, 238),
            measure_active: Color32::from_rgb(255, 255, 255),
            main_beat: Color32::from_rgba_unmultiplied(190, 198, 220, 96),
            straight: Color32::from_rgba_unmultiplied(140, 165, 205, 48),
            triplet: Color32::from_rgba_unmultiplied(235, 160, 90, 64),
            ghost: Color32::from_rgb(255, 205, 90),
            label_text: Color32::from_rgb(235, 238, 246),
            label_background: Color32::from_rgba_unmultiplied(18, 20, 28, 190),
            label_background_hover: Color32::from_rgba_unmultiplied(60, 66, 90, 230),
        }
    }
}
