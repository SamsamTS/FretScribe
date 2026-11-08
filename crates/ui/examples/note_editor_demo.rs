//! Note editing over a synthetic spectrogram: draw, slide, chain (Shift), legato (right drag),
//! marquee, move/resize, link (L), copy/paste, undo (Ctrl+Z / Ctrl+Y), double-click for exact edit.
//! Display controls as in the app toolbar. Ctrl+wheel zooms, Shift+wheel scrolls, Alt+wheel zooms
//! pitch, middle drag pans.

use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke};
use fretscribe_analysis::{clamp_gamma, discover_luts, Levels, Lut, Spectrogram, SpectrogramParams, APP_GAMMA, APP_WHITE, GAMMA_DEFAULT, GAMMA_MAX, GAMMA_MIN, MIN_LEVELS_GAP};
use fretscribe_analysis::notes::refine_pitch;
use fretscribe_core::{Legato, LegatoLink, Note, NoteTimes, PitchRef, Project, SnapGrid, TempoMap, TimeSignature, Technique, Track, TrackRole, UndoStack};
use fretscribe_ui::note_editor::{self as ne, Effect, Env, NoteEditor};
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

type Spec = (u8, f64, f64, Option<(u64, Technique)>, bool);

/// Lone note, slide chain with a long last note, bend and release, hammer-on, pull-off; `squeeze`
/// scales every time to show the minimum width, the run rule and the corner radius.
fn scene(map: &TempoMap, t0: f64, squeeze: f64) -> Vec<Note> {
    let specs: [Spec; 13] = [
        (52, 0.0, 1.0, None, false),
        (45, 1.5, 2.0, Some((3, Technique::Slide)), false),
        (50, 2.5, 3.0, Some((4, Technique::Slide)), false),
        (47, 3.5, 6.0, None, false),
        (52, 7.0, 7.5, Some((6, Technique::Bend)), false),
        (54, 8.0, 8.5, Some((7, Technique::Bend)), false),
        (52, 9.0, 9.5, None, false),
        (48, 10.0, 10.5, None, false),
        (52, 10.5, 11.0, None, true),
        (55, 11.5, 12.0, None, false),
        (50, 12.0, 12.5, None, true),
        (40, 13.0, 13.2, Some((13, Technique::Slide)), false),
        (60, 13.4, 13.6, None, false),
    ];
    let base = (t0 * 100.0) as u64;
    specs
        .iter()
        .enumerate()
        .map(|(i, &(midi, start, end, link, legato))| {
            let at = |t: f64| map.sec_to_pos(t0 + t * squeeze);
            let mut n = Note::new(base + i as u64 + 1, midi, at(start), at(end));
            if let Some((to, technique)) = link {
                (n.link_to, n.technique) = (Some(base + to), Some(technique));
            }
            if legato {
                n.legato = Some(LegatoLink { kind: Legato::HammerOn, from: base + i as u64 });
            }
            n
        })
        .collect()
}

struct Demo {
    ctl: ViewController,
    project: Project,
    undo: UndoStack,
    editor: NoteEditor,
    grid: SnapGrid,
    exact: Option<ne::ExactFields>,
    exact_error: String,
    revision: u64,
    spectrogram: Spectrogram,
    luts: Vec<Arc<Lut>>,
    style: SpectroStyle,
    cache: SpectroCache,
    times: NoteTimes,
    pitch_ref: PitchRef,
    a4_hz: f32,
    playhead: f64,
    last_event: String,
}

impl Demo {
    fn new() -> Self {
        let mut project = Project::new(Track::new(1, "Guitar", TrackRole::Guitar), AUDIO_LEN_SEC, 110.0, TimeSignature::COMMON);
        let map = TempoMap::new(&project.measures);
        let mut luts: Vec<Arc<Lut>> = discover_luts(Path::new("LUT")).into_iter().map(Arc::new).collect();
        if luts.is_empty() {
            luts.push(Arc::new(Lut::builtin()));
        }
        let spectrogram = synthetic();
        project.notes = scene(&map, 0.0, 1.0).into_iter().chain(scene(&map, 15.0, 0.12)).collect();
        let times = NoteTimes::build(&project.notes, &map);
        let style = SpectroStyle { levels: Levels::new(0.0, APP_WHITE), gamma: APP_GAMMA, lut: luts[0].clone() };
        let state = ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 120.0 };
        Self {
            ctl: ViewController::new(state, &ViewTuning::DEFAULT, PitchRange { lo: 35.5, hi: 84.5 }),
            project,
            undo: UndoStack::new(),
            editor: NoteEditor::default(),
            grid: SnapGrid::new(&[4, 3]),
            exact: None,
            exact_error: String::new(),
            revision: 0,
            spectrogram,
            luts,
            style,
            cache: SpectroCache::default(),
            times,
            pitch_ref: PitchRef::default(),
            a4_hz: 440.0,
            playhead: 2.5,
            last_event: String::new(),
        }
    }

    fn changed(&mut self) {
        self.times = NoteTimes::build(&self.project.notes, &TempoMap::new(&self.project.measures));
        self.revision += 1;
    }

    fn run(&mut self, ctx: &egui::Context, inputs: Vec<ne::Input>, px_per_sec: f64, hovered: Option<u64>) {
        for input in inputs {
            let mut env = Env { project: &mut self.project, grid: &self.grid, px_per_sec, threshold_px: 12.0, onsets: None, now: ctx.input(|i| i.time), hovered };
            for effect in self.editor.handle(&mut env, input.clone()) {
                match effect {
                    Effect::Seek(sec) => self.playhead = sec,
                    Effect::Edit { edit, merge, .. } => {
                        if merge { self.undo.push_merged(*edit) } else { self.undo.push(*edit) }
                        self.changed();
                    }
                    Effect::Message(m) => self.last_event = m,
                    Effect::OpenExact(id) => self.exact = ne::fields_for(&self.project, id),
                }
            }
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
        if !ctx.wants_keyboard_input() {
            let (undo, redo) = ctx.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::Z) && !i.modifiers.shift, i.modifiers.command && (i.key_pressed(egui::Key::Y) || i.key_pressed(egui::Key::Z) && i.modifiers.shift)));
            if undo { self.undo.undo(&mut self.project); } else if redo { self.undo.redo(&mut self.project); }
            if undo || redo {
                self.changed();
                let alive = |id: &u64| self.project.note(*id).is_some();
                let ids: Vec<u64> = self.editor.selection.ids.iter().copied().filter(alive).collect();
                self.editor.selection.set(ids);
            }
        }
        egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
            let buttons = ne::action_buttons(&self.project, &self.editor.selection);
            if let Some(action) = ne::show_action_row(ui, &buttons, &self.editor.selection) {
                self.run(ctx, vec![ne::Input::Action(action)], self.ctl.current.pixels_per_sec, None);
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let area = ui.available_rect_before_wrap();
            let ppp = ctx.pixels_per_point();
            let spec_h = (area.height() - RULER_H - WAVE_H).max(40.0);
            let tempo = TempoMap::new(&self.project.measures);
            let nav = NavContext {
                regions: NavRegions { grid: area, lanes: area },
                map: &tempo,
                audio_len_sec: AUDIO_LEN_SEC,
                content_px: (area.height() * ppp) as u32,
                strip_pitch_px: (WAVE_H * ppp) as u32,
            };
            self.ctl.run(ctx, &nav);
            let view = self.ctl.current.timeline();
            let pitch = PitchView::from(self.ctl.pitch.current);

            let spectrogram = &self.spectrogram;
            let tuning = self.project.tuning.clone();
            let (spec_out, seek) = ui
                .scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let seek = show_time_ruler(ui, &view, RULER_H, self.playhead);
                    let out = SpectrogramView::new(spectrogram, &self.style, self.pitch_ref, &tuning)
                        .notes(&self.project.notes, &self.times, &self.editor.selection.ids)
                        .ghost(self.editor.ghost.as_ref())
                        .revision(self.revision)
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
                    (out, seek)
                })
                .inner;
            let events: Vec<SpectroEvent> = spec_out.events.iter().cloned().chain(seek).collect();
            let refine = |sec: f64, midi: f32| refine_pitch(&self.spectrogram, &self.pitch_ref, sec, midi);
            let source = ne::SourcePointer { geometry: spec_out.geometry, hovered_note: spec_out.hovered_note, pitch_ruler: spec_out.pitch_ruler, notes: &self.project.notes, times: &self.times };
            let collected = ne::collect(ctx, &source, &events, &self.editor, &refine);
            let hovered = spec_out.hovered_note.map(|h| h.0);
            self.run(ctx, collected.inputs, view.pixels_per_sec, hovered);
            if let Some(id) = collected.open_exact {
                self.exact = ne::fields_for(&self.project, id);
            }
            let painter = ui.painter_at(spec_out.geometry.rect);
            ne::paint_overlay(&painter, &spec_out.geometry, &self.editor, ctx.input(|i| i.pointer.latest_pos()), None);
            self.exact_window(ctx);
        });
    }
}

impl Demo {
    fn exact_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.exact.take() else { return };
        let (mut keep, mut apply) = (true, false);
        egui::Window::new("Exact edit").collapsible(false).open(&mut keep).show(ctx, |ui| {
            egui::Grid::new("exact").show(ui, |ui| {
                for (label, (bar, beat)) in [("Start bar / beat", &mut f.start), ("End bar / beat", &mut f.end)] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(bar).desired_width(40.0));
                    ui.add(egui::TextEdit::singleline(beat).desired_width(60.0));
                    ui.end_row();
                }
                ui.label("Pitch");
                ui.text_edit_singleline(&mut f.pitch);
                ui.end_row();
                ui.label("String");
                let mut locked = f.string.is_some();
                ui.checkbox(&mut locked, "lock");
                f.string = locked.then(|| f.string.unwrap_or(0));
                if let Some(s) = f.string.as_mut() {
                    ui.add(egui::DragValue::new(s).range(0..=5));
                }
                ui.end_row();
            });
            apply = ui.button("Apply").clicked();
            ui.colored_label(Color32::LIGHT_RED, &self.exact_error);
        });
        if apply {
            match ne::apply_exact(&mut self.project, &f) {
                Ok(edit) => {
                    self.undo.push(edit);
                    self.changed();
                    self.exact_error.clear();
                    return;
                }
                Err(e) => self.exact_error = e.to_string(),
            }
        }
        if keep {
            self.exact = Some(f);
        }
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "FretScribe note editor demo",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(Demo::new()))
        }),
    )
}
