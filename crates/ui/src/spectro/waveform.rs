//! Waveform strip below a spectrogram. Peaks come from a closure so the widget does not depend on
//! the analysis crate.

use egui::{Color32, Rect, Response, Sense, Shape, Stroke, Ui};

use crate::overview::{build_mesh, WaveformPalette};
use crate::view::TimelineView;

pub type Peak = (f32, f32, f32);

/// Coloured like the overview waveform: through the global LUT (the built-in palette by default).
pub struct WaveformView<'a> {
    pub height: f32,
    pub playhead_sec: Option<f64>,
    pub audio_offset_sec: f64,
    palette: Option<&'a WaveformPalette>,
}

impl<'a> WaveformView<'a> {
    pub fn new(height: f32) -> Self {
        Self { height, playhead_sec: None, audio_offset_sec: 0.0, palette: None }
    }

    pub fn palette(mut self, palette: &'a WaveformPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn playhead(mut self, sec: f64) -> Self {
        self.playhead_sec = Some(sec);
        self
    }

    /// Shifts the audio (project time = audio time + offset); the playhead stays in project time.
    pub fn audio_offset(mut self, sec: f64) -> Self {
        self.audio_offset_sec = sec;
        self
    }

    /// `peaks((start_sec, end_sec), columns)` returns exactly one `(min, max, rms)` per physical pixel
    /// column of the visible window, amplitudes in -1..1.
    pub fn show(self, ui: &mut Ui, view: &TimelineView, peaks: impl FnOnce((f64, f64), usize) -> Vec<Peak>) -> Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), self.height), Sense::click_and_drag());
        self.paint(ui, rect, view, peaks);
        response
    }

    /// Draws into `rect` without allocating, for callers that handle the interaction themselves.
    pub fn paint(&self, ui: &Ui, rect: Rect, view: &TimelineView, peaks: impl FnOnce((f64, f64), usize) -> Vec<Peak>) {
        let painter = ui.painter_at(rect);
        let default = WaveformPalette::default();
        let palette = self.palette.unwrap_or(&default);
        painter.rect_filled(rect, 0.0, palette.background());
        let ppp = ui.ctx().pixels_per_point();
        let columns = (rect.width() * ppp).round().max(1.0) as usize;
        let range = view.visible_range(rect.width());
        let peaks = peaks((range.start - self.audio_offset_sec, range.end - self.audio_offset_sec), columns);
        painter.add(Shape::mesh(build_mesh(rect, ppp, &peaks, palette)));
        if let Some(sec) = self.playhead_sec {
            painter.vline(rect.left() + view.sec_to_x(sec), rect.y_range(), Stroke::new(1.5_f32, Color32::from_rgb(255, 90, 80)));
        }
    }
}
