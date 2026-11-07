//! Spectrogram widget over a synthetic spectrogram, with the three global display controls of the
//! app toolbar: a two-handle levels slider, a logarithmic gamma slider and a LUT dropdown.
//! Ctrl+wheel zooms, Shift+wheel scrolls, Alt+wheel zooms pitch, middle drag pans.

use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke};
use fretscribe_analysis::{clamp_gamma, discover_luts, Levels, Lut, Spectrogram, SpectrogramParams, APP_GAMMA, APP_WHITE, GAMMA_DEFAULT, GAMMA_MAX, GAMMA_MIN, MIN_LEVELS_GAP};
use fretscribe_core::{generate_measures, Note, NoteTimes, PitchRef, TempoMap, TimeSignature, Technique, Tuning};
use fretscribe_ui::nav::{PitchRange, ViewState};
use fretscribe_ui::spectro::show_time_ruler;
use fretscribe_ui::{NavContext, NavRegions, PitchView, SpectroCache, SpectroEvent, SpectroStyle, SpectrogramView, ViewController, ViewTuning, WaveformView};

const AUDIO_LEN_SEC: f64 = 30.0;
const WAVE_H: f32 = 48.0;
const RULER_H: f32 = 22.0;

fn synthetic() -> Spectrogram {
    let (hop, bins, min, max) = (0.01_f64, 4u8, 28.0_f32, 88.0_f32);
    let n_rows = ((max - min) * f32::from(bins)) as usize;
    let n_frames = (AUDIO_LEN_SEC / hop) as usize;
    let mut data = vec![0.0_f32; n_rows * n_frames];
    for k in 0..30 {
        let (start, len, midi) = (f64::from(k) * 0.9, 0.8, 40.0 + (k * 5 % 24) as f32);
        for h in 1..=4 {
            let pitch = midi + 12.0 * (h as f32).log2();
            let row = ((pitch - min) * f32::from(bins)).round() as usize;
            for f in (start / hop) as usize..((start + len) / hop) as usize {
                let decay = (-(f as f64 * hop - start) * 2.0).exp() as f32 / h as f32;
                for dr in 0..3usize {
                    if let Some(v) = data.get_mut((row + dr).saturating_sub(1).min(n_rows - 1) * n_frames + f) {
                        *v = v.max(decay * if dr == 1 { 1.0 } else { 0.5 });
                    }
                }
            }
        }
    }
    Spectrogram { data, n_frames, n_rows, hop_sec: hop, min_midi: min, max_midi: max, bins_per_semitone: bins }
}

fn levels_slider(ui: &mut egui::Ui, levels: &mut Levels) {
    let (rect, response) = ui.allocate_exact_size(vec2(180.0, 18.0), Sense::click_and_drag());
    let x_of = |v: f32| rect.left() + 6.0 + v * (rect.width() - 12.0);
    let value_at = |x: f32| ((x - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0);
    let id = response.id.with("handle");
    if response.drag_started() {
        let x = response.interact_pointer_pos().map_or(0.0, |p| p.x);
        let black_closer = (x - x_of(levels.black)).abs() <= (x - x_of(levels.white)).abs();
        ui.data_mut(|d| d.insert_temp(id, black_closer));
    }
    if let Some(pos) = response.interact_pointer_pos().filter(|_| response.dragged()) {
        let black = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
        let v = value_at(pos.x);
        if black {
            levels.black = v.min(levels.white - MIN_LEVELS_GAP).max(0.0);
        } else {
            levels.white = v.max(levels.black + MIN_LEVELS_GAP).min(1.0);
        }
    }
    if response.double_clicked() {
        *levels = Levels::default();
    }
    let painter = ui.painter_at(rect);
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 12.0, 4.0));
    painter.rect_filled(track, 2.0, Color32::from_gray(60));
    painter.rect_filled(Rect::from_min_max(pos2(x_of(levels.black), track.top()), pos2(x_of(levels.white), track.bottom())), 2.0, Color32::from_rgb(90, 150, 230));
    for v in [levels.black, levels.white] {
        painter.circle(pos2(x_of(v), rect.center().y), 6.0, Color32::from_gray(220), Stroke::new(1.0_f32, Color32::BLACK));
    }
    let (black_db, white_db) = levels.to_db(&SpectrogramParams::default());
    response.on_hover_text(format!("black {black_db:.0} dB, white {white_db:.0} dB"));
}

struct Demo {
    ctl: ViewController,
    map: TempoMap,
    spectrogram: Spectrogram,
    luts: Vec<Arc<Lut>>,
    style: SpectroStyle,
    cache: SpectroCache,
    notes: Vec<Note>,
    times: NoteTimes,
    selected: Vec<u64>,
    tuning: Tuning,
    pitch_ref: PitchRef,
    a4_hz: f32,
    playhead: f64,
    last_event: String,
}

impl Demo {
    fn new() -> Self {
        let mut next = 1;
        let map = TempoMap::new(&generate_measures(0.0, AUDIO_LEN_SEC, 110.0, TimeSignature::COMMON, &mut next));
        let mut luts: Vec<Arc<Lut>> = discover_luts(Path::new("LUT")).into_iter().map(Arc::new).collect();
        if luts.is_empty() {
            luts.push(Arc::new(Lut::builtin()));
        }
        let spectrogram = synthetic();
        let mut notes = Vec::new();
        for (id, (start, end, midi, technique)) in [(1.0, 2.0, 52, Some(Technique::Bend)), (2.0, 3.0, 54, Some(Technique::Bend)), (3.0, 4.0, 52, None), (5.0, 6.0, 45, Some(Technique::Slide)), (6.5, 7.5, 50, None)].into_iter().enumerate() {
            let mut n = Note::new(id as u64 + 1, midi, map.sec_to_pos(start), map.sec_to_pos(end));
            n.technique = technique;
            n.link_to = [Some(2), Some(3), None, Some(5), None][id];
            notes.push(n);
        }
        let times = NoteTimes::build(&notes, &map);
        let style = SpectroStyle { levels: Levels::new(0.0, APP_WHITE), gamma: APP_GAMMA, lut: luts[0].clone() };
        let state = ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 120.0 };
        Self {
            ctl: ViewController::new(state, &ViewTuning::DEFAULT, PitchRange { lo: 35.5, hi: 84.5 }),
            map,
            spectrogram,
            luts,
            style,
            cache: SpectroCache::default(),
            notes,
            times,
            selected: vec![3],
            tuning: Tuning::standard(),
            pitch_ref: PitchRef::default(),
            a4_hz: 440.0,
            playhead: 2.5,
            last_event: String::new(),
        }
    }
}

impl eframe::App for Demo {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Levels");
                levels_slider(ui, &mut self.style.levels);
                ui.label("Gamma");
                let response = ui.add(egui::Slider::new(&mut self.style.gamma, GAMMA_MIN..=GAMMA_MAX).logarithmic(true));
                if response.double_clicked() {
                    self.style.gamma = GAMMA_DEFAULT;
                }
                self.style.gamma = clamp_gamma(self.style.gamma);
                egui::ComboBox::from_id_salt("lut").selected_text(&self.style.lut.name).show_ui(ui, |ui| {
                    for lut in &self.luts {
                        if ui.selectable_label(lut.name == self.style.lut.name, &lut.name).clicked() {
                            self.style.lut = lut.clone();
                        }
                    }
                });
                ui.label("A4");
                if ui.add(egui::DragValue::new(&mut self.a4_hz).range(400.0..=480.0).speed(0.1)).changed() {
                    self.pitch_ref = PitchRef::new(self.a4_hz).unwrap_or_default();
                }
                ui.label(&self.last_event);
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let area = ui.available_rect_before_wrap();
            let ppp = ctx.pixels_per_point();
            let spec_h = (area.height() - RULER_H - WAVE_H).max(40.0);
            let spec_rect = Rect::from_min_size(pos2(area.left(), area.top() + RULER_H), vec2(area.width(), spec_h));
            let nav = NavContext {
                regions: NavRegions { grid: area, lanes: area, source_spec: Some(spec_rect) },
                map: &self.map,
                audio_len_sec: AUDIO_LEN_SEC,
                content_px: (area.height() * ppp) as u32,
                strip_pitch_px: (WAVE_H * ppp) as u32,
            };
            self.ctl.run(ctx, &nav);
            let view = self.ctl.current.timeline();
            let pitch = PitchView::from(self.ctl.pitch.current);

            let spectrogram = &self.spectrogram;
            let out = ui
                .scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let seek = show_time_ruler(ui, &view, RULER_H, self.playhead);
                    let out = SpectrogramView::new(spectrogram, &self.style, self.pitch_ref, &self.tuning)
                        .notes(&self.notes, &self.times, &self.selected)
                        .playhead(self.playhead)
                        .height(spec_h)
                        .show(ui, &view, &pitch, &mut self.cache);
                    let wave = |range: (f64, f64), n: usize| {
                        (0..n)
                            .map(|i| {
                                let t = range.0 + (range.1 - range.0) * i as f64 / n as f64;
                                let a = ((t * 3.1).sin().abs() * (t * 0.4).cos().abs()) as f32;
                                (-a, a, a * 0.5)
                            })
                            .collect()
                    };
                    WaveformView::new(WAVE_H).playhead(self.playhead).show(ui, &view, wave);
                    out.events.into_iter().chain(seek).collect::<Vec<_>>()
                })
                .inner;
            for event in out {
                match event {
                    SpectroEvent::SeekRequested(sec) => {
                        self.playhead = sec;
                        self.last_event = format!("seek {sec:.2}s");
                    }
                    SpectroEvent::AddNoteRequested { midi, time_sec, exact_midi_float } => {
                        self.last_event = format!("add note {midi} ({exact_midi_float:.2}) at {time_sec:.2}s");
                    }
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "FretScribe spectrogram demo",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(Demo::new()))
        }),
    )
}
