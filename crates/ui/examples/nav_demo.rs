//! Demo of the navigation layer over placeholder strips: scrolling lanes (a spectrogram strip of
//! k*P and a waveform strip of P, k = 0 meaning collapsed), a pinned ruler and notation panel, and
//! the measure grid across everything. Ctrl+wheel zooms, Shift+wheel or tilt scrolls by quarter
//! notes, wheel scrolls the lanes by strips, Alt+wheel over the first spectrogram zooms pitch,
//! middle drag pans, a click on the ruler jumps there.

use eframe::egui::{self, pos2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind};
use fretscribe_core::{generate_measures, set_tempo, TempoMap, TimeSignature};
use fretscribe_ui::nav::{PitchRange, ViewState};
use fretscribe_ui::{NavContext, NavRegions, ViewController, ViewTuning};

const AUDIO_LEN_SEC: f64 = 240.0;
const PITCH_PT: f32 = 18.0;
const RULER_PT: f32 = 24.0;
const NOTATION_PT: f32 = 110.0;
const LANE_SPEC_K: [u32; 7] = [3, 0, 2, 4, 1, 0, 3];
const FULL_PITCH: PitchRange = PitchRange { lo: 28.0, hi: 88.0 };

struct NavDemo {
    ctl: ViewController,
    map: TempoMap,
}

impl NavDemo {
    fn new() -> Self {
        let mut next = 1;
        let mut measures = generate_measures(0.0, AUDIO_LEN_SEC, 100.0, TimeSignature::COMMON, &mut next);
        for (index, bpm) in [(30, 70.0), (60, 140.0)] {
            let _ = set_tempo(&mut measures, index, bpm);
        }
        let state = ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 120.0 };
        Self {
            ctl: ViewController::new(state, &ViewTuning::DEFAULT, FULL_PITCH),
            map: TempoMap::new(&measures),
        }
    }
}

fn dim(a: u8) -> Color32 {
    Color32::from_white_alpha(a)
}

fn strip(painter: &egui::Painter, rect: Rect, fill: Color32) {
    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, dim(70)), StrokeKind::Inside);
}

fn draw_grid(painter: &egui::Painter, map: &TempoMap, grid: Rect, x_of: &dyn Fn(f64) -> f32) {
    for i in 0..map.len() {
        let x0 = x_of(map.measure_start_sec(i));
        if x0 > grid.right() {
            break;
        }
        if x_of(map.measure_start_sec(i) + map.measure_duration_sec(i)) < grid.left() {
            continue;
        }
        let long = x_of(map.measure_start_sec(i) + map.measure_duration_sec(i)) - x0 > 40.0;
        for line in map.beat_lines(i).iter().filter(|l| long || l.kind == fretscribe_core::BeatKind::Measure) {
            let x = x_of(line.time_sec);
            let alpha = match line.kind {
                fretscribe_core::BeatKind::Measure => 110,
                fretscribe_core::BeatKind::MainBeat => 45,
                fretscribe_core::BeatKind::Subdivision => 18,
            };
            painter.vline(x, grid.y_range(), Stroke::new(1.0_f32, dim(alpha)));
        }
        if long {
            painter.text(pos2(x0 + 3.0, grid.top() + 2.0), Align2::LEFT_TOP, format!("{}", i + 1), FontId::monospace(10.0), dim(160));
        }
    }
}

impl eframe::App for NavDemo {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let ppp = ctx.pixels_per_point();
            let area = ui.available_rect_before_wrap();
            let ruler = Rect::from_min_size(area.min, egui::vec2(area.width(), RULER_PT));
            let notation = Rect::from_min_max(pos2(area.left(), area.bottom() - NOTATION_PT), area.max);
            let lanes = Rect::from_min_max(ruler.left_bottom(), notation.left_top());
            let pitch_px = (PITCH_PT * ppp).round() as u32;
            let content_px: u32 = LANE_SPEC_K.iter().map(|k| (k + 1) * pitch_px).sum();
            let spec_top = |scroll_px: f32| lanes.top() + (-scroll_px) / ppp;
            let source_spec = {
                let r = Rect::from_min_size(
                    pos2(lanes.left(), spec_top((self.ctl.current.scroll_y as f32 * ppp).round())),
                    egui::vec2(lanes.width(), (LANE_SPEC_K[0] * pitch_px) as f32 / ppp),
                );
                Some(r.intersect(lanes)).filter(|r| r.height() > 0.0)
            };

            let nav = NavContext {
                regions: NavRegions { grid: area, lanes, source_spec },
                map: &self.map,
                audio_len_sec: AUDIO_LEN_SEC,
                content_px,
                strip_pitch_px: pitch_px,
            };
            self.ctl.run(ctx, &nav);

            let response = ui.allocate_rect(ruler, Sense::click());
            let view = self.ctl.current.timeline();
            let x_of = |sec: f64| area.left() + view.sec_to_x(sec);
            if let Some(pos) = response.interact_pointer_pos().filter(|_| response.clicked()) {
                self.ctl.jump_to(view.x_to_sec(pos.x - area.left()));
            }

            let painter = ui.painter();
            painter.rect_filled(area, 0.0, Color32::from_gray(20));
            painter.rect_filled(ruler, 0.0, Color32::from_gray(34));
            painter.rect_filled(notation, 0.0, Color32::from_gray(28));

            let lane_painter = painter.with_clip_rect(lanes);
            let scroll_px = (self.ctl.current.scroll_y as f32 * ppp).round();
            let mut y_px = 0u32;
            for (lane, &k) in LANE_SPEC_K.iter().enumerate() {
                let strip_rect = |top: u32, rows: u32| {
                    Rect::from_min_size(
                        pos2(lanes.left(), lanes.top() + (top as f32 - scroll_px) / ppp),
                        egui::vec2(lanes.width(), (rows * pitch_px) as f32 / ppp),
                    )
                };
                if k > 0 {
                    let rect = strip_rect(y_px, k);
                    strip(&lane_painter, rect, Color32::from_rgb(14, 30, 40));
                    let p = self.ctl.pitch.current;
                    for midi in (p.lo.ceil() as i32)..=(p.hi.floor() as i32) {
                        let y = rect.bottom() - ((f64::from(midi) - p.lo) / p.span()) as f32 * rect.height();
                        let octave = midi % 12 == 0;
                        if octave || rect.height() / p.span() as f32 > 6.0 {
                            lane_painter.hline(rect.x_range(), y, Stroke::new(1.0_f32, dim(if octave { 60 } else { 16 })));
                        }
                    }
                    lane_painter.text(rect.left_top() + egui::vec2(4.0, 3.0), Align2::LEFT_TOP, format!("lane {lane} spectrogram k={k}"), FontId::monospace(10.0), dim(150));
                }
                let wave = strip_rect(y_px + k * pitch_px, 1);
                strip(&lane_painter, wave, Color32::from_rgb(34, 24, 18));
                let mid = wave.center().y;
                let mut x = lanes.left();
                while x < lanes.right() {
                    let sec = view.x_to_sec(x - lanes.left());
                    let amp = (sec * 2.3 + lane as f64).sin().abs() * (sec * 0.37).cos().abs();
                    lane_painter.vline(x, (mid - amp as f32 * wave.height() * 0.45)..=(mid + amp as f32 * wave.height() * 0.45), Stroke::new(1.0_f32, Color32::from_rgb(240, 170, 60)));
                    x += 3.0;
                }
                y_px += (k + 1) * pitch_px;
            }

            draw_grid(painter, &self.map, area, &x_of);
            let readout = format!(
                "pps {:.1}  scroll {:.3}s  y {}px (P={pitch_px}, y%P={})  {}",
                view.pixels_per_sec,
                view.scroll_sec,
                scroll_px,
                scroll_px as u32 % pitch_px,
                if self.ctl.is_animating() { "animating" } else { "idle" }
            );
            painter.text(notation.left_top() + egui::vec2(6.0, 6.0), Align2::LEFT_TOP, "notation panel (pinned)", FontId::proportional(13.0), dim(170));
            painter.text(notation.left_top() + egui::vec2(6.0, 26.0), Align2::LEFT_TOP, readout, FontId::monospace(11.0), dim(140));
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "FretScribe navigation demo",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(NavDemo::new()))
        }),
    )
}
