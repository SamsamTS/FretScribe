//! Multi-track lanes over synthetic tracks (guitar = transcription source, drums and bass offset on
//! purpose). Drag a waveform (Alt: no snapping, Shift: fine), type or nudge the offset field, drag
//! the header grip to reorder, collapse with the chevron, resize a spectrogram from its bottom edge.
//! Ctrl+wheel zooms, Shift+wheel scrolls, wheel scrolls the lanes, Alt+wheel over the source zooms pitch.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, pos2, vec2, Rect, UiBuilder};
use fretscribe_analysis::{sync::snap_offsets, Levels, Lut, Spectrogram, APP_GAMMA, APP_WHITE};
use fretscribe_core::{shift_track, Note, NoteTimes, Project, TempoMap, TimeSignature, Track, TrackRole};
use fretscribe_ui::nav::{PitchRange, ViewState};
use fretscribe_ui::spectro::{show_time_ruler, Peak};
use fretscribe_ui::tracks::{TrackEvent, STRIP_PITCH_PX};
use fretscribe_ui::view::LANE_HEADER_WIDTH;
use fretscribe_ui::{NavContext, NavRegions, PitchView, SpectroEvent, SpectroStyle, TrackLanes, TrackLanesState, ViewController, ViewTuning};
use fretscribe_ui::LaneSource;

const LEN_SEC: f64 = 30.0;
const RULER_H: f32 = 22.0;

fn onsets_for(role: TrackRole) -> Vec<f64> {
    let step = match role {
        TrackRole::Drums => 0.5,
        TrackRole::Bass => 1.0,
        _ => 0.9,
    };
    (0..(LEN_SEC / step) as usize).map(|k| k as f64 * step).collect()
}

fn synth(onsets: &[f64], role: TrackRole) -> Spectrogram {
    let (hop, bins, min, max) = (0.01_f64, 4u8, 28.0_f32, 88.0_f32);
    let n_rows = ((max - min) * f32::from(bins)) as usize;
    let n_frames = (LEN_SEC / hop) as usize;
    let mut data = vec![0.0_f32; n_rows * n_frames];
    for (k, &start) in onsets.iter().enumerate() {
        let midi = match role {
            TrackRole::Bass => 33.0 + (k % 5) as f32 * 2.0,
            _ => 40.0 + (k * 5 % 24) as f32,
        };
        let len = (onsets.get(1).map_or(0.5, |n| n - onsets[0]) * 0.9).max(0.1);
        for h in 1..=if role == TrackRole::Drums { 1 } else { 4 } {
            let rows: Vec<usize> = if role == TrackRole::Drums {
                (0..n_rows).collect()
            } else {
                vec![((midi + 12.0 * (h as f32).log2() - min) * f32::from(bins)).round() as usize]
            };
            for f in (start / hop) as usize..(((start + len) / hop) as usize).min(n_frames) {
                let decay = (-(f as f64 * hop - start) * if role == TrackRole::Drums { 12.0 } else { 2.0 }).exp() as f32 / h as f32;
                for &row in &rows {
                    if let Some(v) = data.get_mut(row.min(n_rows - 1) * n_frames + f) {
                        *v = v.max(decay * if role == TrackRole::Drums { 0.5 } else { 1.0 });
                    }
                }
            }
        }
    }
    Spectrogram { data, n_frames, n_rows, hop_sec: hop, min_midi: min, max_midi: max, bins_per_semitone: bins }
}

struct Audio {
    spectrograms: HashMap<u64, Spectrogram>,
    onsets: HashMap<u64, Vec<f64>>,
    beat_times: Vec<f64>,
    reference: (u64, f64),
}

impl LaneSource for Audio {
    fn spectrogram(&self, id: u64) -> Option<&Spectrogram> {
        self.spectrograms.get(&id)
    }

    fn peaks(&self, id: u64, (start, end): (f64, f64), columns: usize) -> Vec<Peak> {
        let onsets = self.onsets.get(&id).map_or(&[][..], Vec::as_slice);
        (0..columns)
            .map(|i| {
                let t = start + (end - start) * (i as f64 + 0.5) / columns as f64;
                let k = onsets.partition_point(|&o| o <= t);
                let a = if !(0.0..=LEN_SEC).contains(&t) || k == 0 { 0.0 } else { (-(t - onsets[k - 1]) * 4.0).exp() as f32 * 0.9 };
                (-a, a, a * 0.5)
            })
            .collect()
    }

    fn duration_sec(&self, _: u64) -> f64 {
        LEN_SEC
    }

    fn onsets(&self, id: u64) -> &[f64] {
        self.onsets.get(&id).map_or(&[], Vec::as_slice)
    }

    fn snap_candidates(&self, id: u64, raw_offset_sec: f64, max_dist_sec: f64) -> Vec<f64> {
        let mut targets = self.beat_times.clone();
        if id != self.reference.0 {
            targets.extend(self.onsets(self.reference.0).iter().map(|o| o + self.reference.1));
        }
        snap_offsets(self.onsets(id), &targets, raw_offset_sec, max_dist_sec)
    }
}

struct Demo {
    ctl: ViewController,
    map: TempoMap,
    project: Project,
    audio: Audio,
    lanes: TrackLanesState,
    style: SpectroStyle,
    times: NoteTimes,
    reference: u64,
    log: String,
}

impl Demo {
    fn new() -> Self {
        let mut project = Project::new(Track::new(1, "Guitar", TrackRole::Guitar), LEN_SEC, 120.0, TimeSignature::COMMON);
        let mut drums = Track::new(0, "Drums", TrackRole::Drums);
        drums.offset_sec = 0.12;
        let mut bass = Track::new(0, "Bass", TrackRole::Bass);
        bass.offset_sec = -0.035;
        project.add_track(drums);
        project.add_track(bass);
        let map = TempoMap::new(&project.measures);
        project.notes = (0..12).map(|k| Note::new(k + 1, 45 + (k * 3 % 12) as u8, map.sec_to_pos(k as f64 * 0.9), map.sec_to_pos(k as f64 * 0.9 + 0.6))).collect();
        let times = NoteTimes::build(&project.notes, &map);
        let onsets: HashMap<u64, Vec<f64>> = project.tracks.iter().map(|t| (t.id, onsets_for(t.role))).collect();
        let spectrograms = project.tracks.iter().map(|t| (t.id, synth(&onsets[&t.id], t.role))).collect();
        let beat_times = (0..map.len()).flat_map(|i| map.beat_lines(i)).map(|l| l.time_sec).collect();
        Self {
            ctl: ViewController::new(ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 120.0 }, &ViewTuning::DEFAULT, PitchRange { lo: 35.5, hi: 84.5 }),
            map,
            audio: Audio { spectrograms, onsets, beat_times, reference: (1, 0.0) },
            project,
            lanes: TrackLanesState::default(),
            style: SpectroStyle { levels: Levels::new(0.0, APP_WHITE), gamma: APP_GAMMA, lut: Arc::new(Lut::builtin()) },
            times,
            reference: 1,
            log: String::new(),
        }
    }

    fn apply(&mut self, event: TrackEvent) {
        let p = &mut self.project;
        self.log = format!("{event:?}");
        match event {
            TrackEvent::VolumeChanged { id, volume } => p.tracks.iter_mut().filter(|t| t.id == id).for_each(|t| t.set_volume(volume)),
            TrackEvent::MuteToggled(id) => p.tracks.iter_mut().filter(|t| t.id == id).for_each(|t| t.muted = !t.muted),
            TrackEvent::SoloToggled { id, exclusive } => {
                let on = p.track(id).is_some_and(|t| !t.solo);
                for t in &mut p.tracks {
                    t.solo = if t.id == id { on } else { t.solo && !exclusive };
                }
            }
            TrackEvent::CollapseToggled(id) => p.tracks.iter_mut().filter(|t| t.id == id).for_each(|t| t.spectrogram_collapsed = !t.spectrogram_collapsed),
            TrackEvent::OffsetChanged { id, offset_sec, move_notes } => {
                let _ = shift_track(p, id, offset_sec, move_notes);
                self.times = NoteTimes::build(&p.notes, &self.map);
            }
            TrackEvent::RenameRequested { id, name } => p.tracks.iter_mut().filter(|t| t.id == id).for_each(|t| t.name = name.clone()),
            TrackEvent::RemoveRequested(id) => {
                let _ = p.remove_track(id);
            }
            TrackEvent::ReorderRequested { id, new_index } => {
                let _ = p.move_track(id, new_index);
            }
            TrackEvent::SetReference(id) => self.reference = id,
            TrackEvent::SetSource(id) => p.source_track = id,
            TrackEvent::RoleChanged { id, role } => p.tracks.iter_mut().filter(|t| t.id == id).for_each(|t| t.role = role),
            TrackEvent::UseForBeatsChanged { id, on } => p.tracks.iter_mut().filter(|t| t.id == id).for_each(|t| t.use_for_beats = on),
            TrackEvent::AddTrackRequested => {
                let id = p.add_track(Track::new(0, "New track", TrackRole::Other));
                let onsets = onsets_for(TrackRole::Other);
                self.audio.spectrograms.insert(id, synth(&onsets, TrackRole::Other));
                self.audio.onsets.insert(id, onsets);
            }
            TrackEvent::SyncRequested(_) | TrackEvent::OffsetDragBegan(_) | TrackEvent::OffsetDragEnded(_) => {}
        }
    }
}

impl eframe::App for Demo {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let o = &mut self.lanes.options;
                ui.checkbox(&mut o.show_notes_on_all, "Show notes on all lanes");
                ui.checkbox(&mut o.show_onsets, "Show onsets");
                ui.label(&self.log);
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let area = ui.available_rect_before_wrap();
            let ruler = Rect::from_min_size(pos2(area.left() + LANE_HEADER_WIDTH, area.top()), vec2(area.width() - LANE_HEADER_WIDTH, RULER_H));
            let lanes = Rect::from_min_max(pos2(area.left(), ruler.bottom()), area.max);
            self.audio.reference = (self.reference, self.project.track(self.reference).map_or(0.0, |t| t.offset_sec));

            let (_, content_px) = self.lanes.strips(ctx, &self.project);
            let nav = NavContext {
                regions: NavRegions { grid: Rect::from_min_max(ruler.left_top(), area.max), lanes },
                map: &self.map,
                audio_len_sec: LEN_SEC,
                content_px,
                strip_pitch_px: STRIP_PITCH_PX,
            };
            self.ctl.run(ctx, &nav);
            let view = self.ctl.current.timeline();
            let pitch = PitchView::from(self.ctl.pitch.current);

            let seek = ui.scope_builder(UiBuilder::new().max_rect(ruler), |ui| show_time_ruler(ui, &view, RULER_H, 0.0)).inner;
            let out = TrackLanes { project: &self.project, source: &self.audio, style: &self.style, times: &self.times, selected: &[], ghost: None, playhead_sec: 0.0, reference_track: self.reference }
                .show(ui, lanes, &view, &pitch, self.ctl.offset_y(ctx.pixels_per_point()), &mut self.lanes);
            for event in out.events {
                self.apply(event);
            }
            for (id, event) in out.spectro.into_iter().chain(seek.map(|e| (0, e))) {
                self.log = match event {
                    SpectroEvent::SeekRequested(sec) => format!("seek {sec:.2}s (lane {id})"),
                    other => format!("{other:?} (lane {id})"),
                };
            }
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "FretScribe track lanes demo",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(Demo::new()))
        }),
    )
}
