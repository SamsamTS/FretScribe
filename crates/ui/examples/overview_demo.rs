//! Demo of the overview timeline over a synthetic waveform, with a plain rectangle standing in for
//! the main view. The wheel handling below is demo-only: in the application the navigation layer
//! owns it.

use std::ops::Range;

use eframe::egui::{self, Sense};
use fretscribe_ui::grid::format_time;
use fretscribe_ui::{OverviewEvent, OverviewTimeline, RegionSelection, TimelineView};

const AUDIO_LEN_SEC: f64 = 380.0;
const BEAT_SEC: f64 = 60.0 / 96.0;

struct OverviewDemo {
    view: TimelineView,
    region: Option<RegionSelection>,
    playhead_sec: f64,
    looping: bool,
    snap_always: bool,
    measures: Vec<f64>,
    status: String,
}

impl OverviewDemo {
    fn new() -> Self {
        let measure_sec = BEAT_SEC * 4.0;
        Self {
            view: TimelineView::new(20.0, 120.0),
            region: None,
            playhead_sec: 0.0,
            looping: false,
            snap_always: false,
            measures: (0..)
                .map(|k| f64::from(k) * measure_sec)
                .take_while(|&t| t < AUDIO_LEN_SEC)
                .collect(),
            status: "Click = seek, drag = select, drag the box = pan, drag its edges = zoom, double-click = clear. Ctrl snaps."
                .to_owned(),
        }
    }

    fn handle(&mut self, event: OverviewEvent) {
        match event {
            OverviewEvent::SeekRequested(sec) => {
                self.playhead_sec = sec;
                self.status = format!("Seek to {}", format_time(sec));
            }
            OverviewEvent::RegionChanged(region) => {
                self.status = region.map_or_else(
                    || "Selection cleared".to_owned(),
                    |r| format!("Selection {} - {}", format_time(r.start_sec), format_time(r.end_sec)),
                );
            }
            OverviewEvent::ViewportChanged { scroll_sec, pixels_per_sec } => {
                self.view = TimelineView::new(scroll_sec, pixels_per_sec);
            }
            OverviewEvent::PlayRegionRequested => {
                if let Some(r) = self.region {
                    self.playhead_sec = r.start_sec;
                    self.status = format!("Play selection from {}", format_time(r.start_sec));
                }
            }
            OverviewEvent::LoopToggled(on) => {
                self.looping = on;
                self.status = format!("Loop {}", if on { "on" } else { "off" });
            }
        }
    }
}

/// Synthetic song: quiet intro, louder verses and choruses, with a pulse on every beat.
fn peaks(range: Range<f64>, columns: usize) -> Vec<(f32, f32, f32)> {
    (0..columns)
        .map(|i| {
            let t = range.start + (range.end - range.start) * (i as f64 + 0.5) / columns as f64;
            let section = match (t / 38.0) as u32 % 4 {
                0 => 0.35,
                1 => 0.6,
                2 => 0.45,
                _ => 0.85,
            };
            let pulse = 0.65 + 0.35 * (t / BEAT_SEC * std::f64::consts::TAU).cos().abs();
            let a = (section * pulse * (0.8 + 0.2 * (t * 3.1).sin())) as f32;
            (-a, a * 0.9, a * 0.5)
        })
        .collect()
}

impl eframe::App for OverviewDemo {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();
        egui::TopBottomPanel::top("overview").show(ctx, |ui| {
            let main_width = ui.ctx().screen_rect().width();
            let output = OverviewTimeline::new(AUDIO_LEN_SEC, &self.view, main_width, peaks)
                .measure_starts(&self.measures)
                .playhead(self.playhead_sec)
                .looping(self.looping)
                .snap_region(self.snap_always)
                .show(ui, &mut self.region);
            events = output.events;
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.checkbox(&mut self.snap_always, "Always snap the selection to measures");
            ui.label(&self.status);
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(24, 38, 58));
            if let Some(r) = self.region {
                let (left, right) = r.x_range(&self.view);
                let band = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + left, rect.top()),
                    egui::pos2(rect.left() + right, rect.bottom()),
                )
                .intersect(rect);
                ui.painter().rect_filled(band, 0.0, egui::Color32::from_rgba_unmultiplied(255, 190, 70, 50));
            }
            if response.hovered() {
                let (scroll, zoom, pointer) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
                if let Some(pointer) = pointer.filter(|_| zoom != 1.0) {
                    self.view.zoom_around(pointer.x - rect.left(), f64::from(zoom));
                }
                self.view.scroll_sec -= f64::from(scroll.x + scroll.y) / self.view.pixels_per_sec;
                self.view.clamp(AUDIO_LEN_SEC);
            }
        });
        for event in events {
            self.handle(event);
        }
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "FretScribe overview demo",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(OverviewDemo::new()))
        }),
    )
}
