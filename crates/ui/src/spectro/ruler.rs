//! Time ruler strip: second ticks with labels; a click seeks.

use egui::{pos2, Align2, Color32, FontId, Sense, Stroke, Ui};

use super::SpectroEvent;
use crate::view::TimelineView;

const MIN_TICK_SPACING_PX: f64 = 80.0;
const STEPS_SEC: [f64; 17] = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0];

pub fn ruler_step(pixels_per_sec: f64) -> f64 {
    STEPS_SEC.iter().copied().find(|s| s * pixels_per_sec >= MIN_TICK_SPACING_PX).unwrap_or(STEPS_SEC[STEPS_SEC.len() - 1])
}

pub fn format_time(sec: f64, step: f64) -> String {
    let decimals = if step >= 1.0 { 0 } else if step >= 0.1 { 1 } else { 2 };
    let total = (sec.max(0.0) * 10f64.powi(decimals)).round() / 10f64.powi(decimals);
    let (minutes, rest) = ((total / 60.0).floor(), total % 60.0);
    let width = if decimals == 0 { 2 } else { 3 + decimals as usize };
    format!("{minutes:.0}:{rest:0width$.prec$}", prec = decimals as usize)
}

/// `playhead_sec` is drawn as a marker; a click or drag on the strip emits `SeekRequested`.
pub fn show_time_ruler(ui: &mut Ui, view: &TimelineView, height: f32, playhead_sec: f64) -> Option<SpectroEvent> {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(34));
    let step = ruler_step(view.pixels_per_sec);
    let range = view.visible_range(rect.width());
    let first = (range.start / step).floor() as i64;
    let last = (range.end / step).ceil() as i64;
    for i in first..=last {
        let sec = i as f64 * step;
        let x = rect.left() + view.sec_to_x(sec);
        painter.vline(x, rect.bottom() - 6.0..=rect.bottom(), Stroke::new(1.0, Color32::from_white_alpha(110)));
        painter.text(pos2(x + 3.0, rect.top() + 2.0), Align2::LEFT_TOP, format_time(sec, step), FontId::monospace(10.0), Color32::from_white_alpha(160));
    }
    let x = rect.left() + view.sec_to_x(playhead_sec);
    painter.vline(x, rect.y_range(), Stroke::new(2.0, Color32::from_rgb(255, 90, 80)));
    let pos = response.interact_pointer_pos().filter(|_| response.clicked() || response.dragged())?;
    Some(SpectroEvent::SeekRequested(view.x_to_sec(pos.x - rect.left()).max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_keeps_ticks_apart_at_every_zoom() {
        for pps in [0.5, 3.0, 25.0, 120.0, 900.0, 5000.0] {
            let step = ruler_step(pps);
            assert!(step * pps >= MIN_TICK_SPACING_PX || step == 1800.0, "{pps}");
        }
        assert_eq!(ruler_step(100.0), 1.0);
    }

    #[test]
    fn times_are_minutes_and_seconds() {
        assert_eq!(format_time(0.0, 1.0), "0:00");
        assert_eq!(format_time(75.0, 5.0), "1:15");
        assert_eq!(format_time(3.5, 0.5), "0:03.5");
        assert_eq!(format_time(61.25, 0.05), "1:01.25");
    }
}
