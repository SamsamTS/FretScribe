//! Every tunable of the navigation feel lives here.

use crate::view::{MAX_PIXELS_PER_SEC, MIN_PIXELS_PER_SEC};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTuning {
    pub zoom_per_notch: f64,
    pub min_pps: f64,
    pub max_pps: f64,
    pub zoom_tau_ms: f64,
    pub scroll_x_tau_ms: f64,
    pub scroll_y_tau_ms: f64,
    pub snap_epsilon_px: f64,
    pub snap_epsilon_log2: f64,
    pub edge_margin_px: f64,
    /// Fraction of the view width that may be shown beyond either end of the audio.
    pub overscroll_fraction: f64,
    /// Extra slide of a too-wide measure towards its flush side, as a fraction of the view width per
    /// notch (on top of the drift that the zoom itself causes).
    pub max_slide_fraction: f64,
    /// Beyond this many usable view widths the measure is ignored and the zoom anchors on the cursor.
    pub cursor_zoom_ratio: f64,
    pub points_per_notch: f64,
    pub quarter_notes_per_notch: f64,
    /// `false` makes every change jump straight to its target.
    pub animate: bool,
}

impl ViewTuning {
    pub const DEFAULT: Self = Self {
        zoom_per_notch: 1.25,
        min_pps: MIN_PIXELS_PER_SEC,
        max_pps: MAX_PIXELS_PER_SEC,
        zoom_tau_ms: 55.0,
        scroll_x_tau_ms: 45.0,
        scroll_y_tau_ms: 60.0,
        snap_epsilon_px: 0.05,
        snap_epsilon_log2: 1e-4,
        edge_margin_px: 16.0,
        overscroll_fraction: 0.1,
        max_slide_fraction: 0.25,
        cursor_zoom_ratio: 3.0,
        points_per_notch: 40.0,
        quarter_notes_per_notch: 1.0,
        animate: true,
    };
}

impl Default for ViewTuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}
