//! Waveform strip below a spectrogram. Peaks come from a closure so the widget does not depend on
//! the analysis crate.

use egui::{pos2, Color32, Mesh, Rect, Response, Sense, Shape, Stroke, Ui};

use crate::view::TimelineView;

pub type Peak = (f32, f32, f32);

const BACKGROUND: Color32 = Color32::from_rgb(12, 20, 14);
const ENVELOPE: Color32 = Color32::from_rgba_premultiplied(60, 120, 70, 140);
const BODY: Color32 = Color32::from_rgb(150, 235, 160);
const HEADROOM: f32 = 0.95;

pub struct WaveformView {
    pub height: f32,
    pub playhead_sec: Option<f64>,
}

impl WaveformView {
    pub fn new(height: f32) -> Self {
        Self { height, playhead_sec: None }
    }

    pub fn playhead(mut self, sec: f64) -> Self {
        self.playhead_sec = Some(sec);
        self
    }

    /// `peaks((start_sec, end_sec), columns)` returns exactly one `(min, max, rms)` per physical pixel
    /// column of the visible window, amplitudes in -1..1.
    pub fn show(self, ui: &mut Ui, view: &TimelineView, peaks: impl FnOnce((f64, f64), usize) -> Vec<Peak>) -> Response {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), self.height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BACKGROUND);
        let ppp = ui.ctx().pixels_per_point();
        let columns = (rect.width() * ppp).round().max(1.0) as usize;
        let range = view.visible_range(rect.width());
        painter.add(Shape::mesh(columns_mesh(rect, ppp, &peaks((range.start, range.end), columns))));
        if let Some(sec) = self.playhead_sec {
            painter.vline(rect.left() + view.sec_to_x(sec), rect.y_range(), Stroke::new(1.5_f32, Color32::from_rgb(255, 90, 80)));
        }
        response
    }
}

fn columns_mesh(rect: Rect, ppp: f32, peaks: &[Peak]) -> Mesh {
    let mut mesh = Mesh::default();
    let (mid, half, pixel) = (rect.center().y, rect.height() / 2.0 * HEADROOM, 1.0 / ppp);
    let centre = |amp: f32| mid - amp.clamp(-1.0, 1.0) * half;
    for (i, &(min, max, rms)) in peaks.iter().enumerate() {
        let x = rect.left() + i as f32 * pixel;
        let column = |top: f32, bottom: f32| Rect::from_min_max(pos2(x, top.min(bottom - pixel)), pos2(x + pixel, bottom));
        mesh.add_colored_rect(column(centre(max), centre(min)), ENVELOPE);
        let rms = rms.abs().min(1.0);
        mesh.add_colored_rect(column(centre(rms), centre(-rms)), BODY);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pair_of_quads_per_column_inside_the_strip() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(4.0, 40.0));
        let mesh = columns_mesh(rect, 1.0, &[(-1.0, 1.0, 0.5), (0.0, 0.0, 0.0), (-3.0, 3.0, 9.0), (-0.2, 0.4, 0.1)]);
        assert_eq!(mesh.vertices.len(), 4 * 2 * 4);
        let bounds = mesh.calc_bounds();
        assert!(rect.contains_rect(bounds), "{bounds:?}");
    }

    #[test]
    fn silent_columns_keep_a_one_pixel_line() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1.0, 40.0));
        let mesh = columns_mesh(rect, 2.0, &[(0.0, 0.0, 0.0)]);
        assert!(mesh.calc_bounds().height() >= 0.5);
    }
}
