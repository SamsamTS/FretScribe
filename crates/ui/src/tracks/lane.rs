//! One lane: header, spectrogram strip, waveform strip and the offset drag.

use std::collections::HashSet;

use egui::{pos2, vec2, Color32, Context, Event, Id, PointerButton, Rect, Response, Sense, Stroke, Ui, UiBuilder};
use fretscribe_analysis::Spectrogram;
use fretscribe_core::{NoteTimes, Project, Track};

use super::events::TrackEvent;
use super::geometry::{clamp_rows, BORDER_PX, STRIP_PITCH_PX, WAVE_H_PX};
use super::header::{HeaderParams, TrackHeader};
use super::offset::{clamp_offset, drag_delta_sec, snap_offset, SNAP_TOLERANCE_PX};
use crate::nav::{notches, ViewTuning};
use crate::spectro::{place_readout, white, GhostShape, Peak, SpectroCache, SpectroEvent, SpectroStyle, SpectrogramView, WaveformView};
use crate::view::{PitchView, TimelineView, LANE_HEADER_WIDTH};

const BORDER_COLOR: Color32 = Color32::from_gray(60);
const GUIDE_SNAPPED: Color32 = Color32::from_rgb(245, 225, 60);
const NOTE_TICK: Color32 = Color32::from_rgb(130, 190, 255);
const ONSET_TICK: Color32 = Color32::from_rgb(255, 170, 90);
const FAINT_NOTES: f32 = 0.3;
const RESIZE_HANDLE_PT: f32 = 5.0;
const TICK_PT: f32 = 10.0;

/// What the lanes need from the audio side. Offsets are applied by the widgets, so everything here
/// is in the track's own audio time.
pub trait LaneSource {
    fn spectrogram(&self, track_id: u64) -> Option<&Spectrogram>;
    /// Bump when a spectrogram changes in place (streamed chunks).
    fn spectrogram_revision(&self, _track_id: u64) -> u64 {
        0
    }
    /// One `(min, max, rms)` per column over `audio_range_sec`.
    fn peaks(&self, track_id: u64, audio_range_sec: (f64, f64), columns: usize) -> Vec<Peak>;
    fn duration_sec(&self, track_id: u64) -> f64;
    fn onsets(&self, track_id: u64) -> &[f64];
    /// Offsets within `max_dist_sec` of `raw_offset_sec` that align the track's onsets with the
    /// beat lines or the reference track's onsets (backed by `analysis::sync::snap_offsets`).
    fn snap_candidates(&self, track_id: u64, raw_offset_sec: f64, max_dist_sec: f64) -> Vec<f64>;
}

/// Display preferences, owned by the app next to the lanes; none of them is project data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneOptions {
    pub show_notes_on_all: bool,
    pub show_onsets: bool,
    pub move_notes: bool,
}

impl Default for LaneOptions {
    fn default() -> Self {
        Self { show_notes_on_all: false, show_onsets: false, move_notes: true }
    }
}

#[derive(Default)]
pub struct LaneState {
    pub cache: SpectroCache,
}

pub struct LaneShared<'a> {
    pub project: &'a Project,
    pub source: &'a dyn LaneSource,
    pub style: &'a SpectroStyle,
    pub times: &'a NoteTimes,
    pub selected: &'a [u64],
    pub ghost: Option<&'a GhostShape>,
    pub playhead_sec: f64,
    pub view: TimelineView,
    pub pitch: PitchView,
    pub reference_track: u64,
    pub audible: &'a HashSet<u64>,
}

pub struct TrackLane<'a> {
    pub shared: &'a LaneShared<'a>,
    pub track: &'a Track,
    /// The whole lane, header column included.
    pub rect: Rect,
    pub shown_rows: u32,
    pub configured_rows: u32,
}

pub struct LaneOutput {
    pub events: Vec<TrackEvent>,
    /// Only the source lane can produce `AddNoteRequested`; clicks elsewhere become seeks.
    pub spectro: Vec<SpectroEvent>,
    pub grip: Response,
    pub spectro_rect: Option<Rect>,
    /// New configured rows, and whether the height should animate to them (wheel) or follow at once (drag).
    pub resized_rows: Option<(u32, bool)>,
}

#[derive(Clone, Copy)]
struct OffsetDrag {
    start: f64,
    raw: f64,
    shown: f64,
    snapped: bool,
}

fn drag_key(track_id: u64) -> Id {
    Id::new(("offset-drag", track_id))
}

impl TrackLane<'_> {
    fn drive_drag(&self, ctx: &Context, response: &Response, move_notes: bool, events: &mut Vec<TrackEvent>) {
        let (id, key, pps) = (self.track.id, drag_key(self.track.id), self.shared.view.pixels_per_sec);
        if response.drag_started_by(PointerButton::Primary) {
            let o = self.track.offset_sec;
            ctx.data_mut(|d| d.insert_temp(key, OffsetDrag { start: o, raw: o, shown: o, snapped: false }));
            events.push(TrackEvent::OffsetDragBegan(id));
        }
        if response.dragged_by(PointerButton::Primary) {
            if let Some(mut drag) = ctx.data(|d| d.get_temp::<OffsetDrag>(key)) {
                let (alt, fine) = ctx.input(|i| (i.modifiers.alt, i.modifiers.shift));
                drag.raw = clamp_offset(drag.raw + drag_delta_sec(response.drag_delta().x, pps, fine));
                let candidates = if alt { Vec::new() } else { self.shared.source.snap_candidates(id, drag.raw, SNAP_TOLERANCE_PX / pps) };
                let (shown, snapped) = snap_offset(drag.raw, &candidates, pps, alt);
                (drag.shown, drag.snapped) = (clamp_offset(shown), snapped);
                ctx.data_mut(|d| d.insert_temp(key, drag));
                events.push(TrackEvent::OffsetChanged { id, offset_sec: drag.shown, move_notes });
            }
        }
        if response.drag_stopped() {
            ctx.data_mut(|d| d.remove::<OffsetDrag>(key));
            events.push(TrackEvent::OffsetDragEnded(id));
        }
    }

    pub fn show(self, ui: &mut Ui, state: &mut LaneState, options: &mut LaneOptions) -> LaneOutput {
        let ctx = ui.ctx().clone();
        let (shared, track) = (self.shared, self.track);
        let id = track.id;
        let ppp = ctx.pixels_per_point();
        let px = |n: u32| n as f32 / ppp;
        let is_source = shared.project.source_track == id;
        let (view, rect) = (shared.view, self.rect);
        let timeline_left = rect.left() + LANE_HEADER_WIDTH;
        let spec_rect = Rect::from_min_size(pos2(timeline_left, rect.top()), vec2(rect.right() - timeline_left, px(self.shown_rows * STRIP_PITCH_PX)));
        let wave_rect = Rect::from_min_size(pos2(timeline_left, spec_rect.bottom()), vec2(spec_rect.width(), px(STRIP_PITCH_PX)));
        let mut events = Vec::new();

        let move_notes = is_source && options.move_notes;
        let wave_response = ui.interact(wave_rect, ui.id().with(("wave", id)), Sense::click_and_drag());
        self.drive_drag(&ctx, &wave_response, move_notes, &mut events);

        let header = TrackHeader::show(
            ui,
            Rect::from_min_size(rect.min, vec2(LANE_HEADER_WIDTH, rect.height())),
            HeaderParams {
                track,
                offset_sec: ctx.data(|d| d.get_temp::<OffsetDrag>(drag_key(id))).map_or(track.offset_sec, |d| d.shown),
                is_source,
                is_reference: shared.reference_track == id,
                audible: shared.audible.contains(&id),
                can_remove: shared.project.tracks.len() > 1,
                move_notes: &mut options.move_notes,
            },
        );
        events.extend(header.events);
        self.drive_drag(&ctx, &header.offset_handle, move_notes, &mut events);

        let drag = ctx.data(|d| d.get_temp::<OffsetDrag>(drag_key(id)));
        let offset = drag.map_or(track.offset_sec, |d| d.shown);
        let mut spectro_events = Vec::new();
        let mut resized_rows = None;

        if self.shown_rows > 0 {
            let height = spec_rect.height() - px(BORDER_PX);
            let mut sui = ui.new_child(UiBuilder::new().max_rect(spec_rect));
            sui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let pitch = shared.pitch;
            match shared.source.spectrogram(id) {
                Some(spectrogram) => {
                    let notes_here = is_source || options.show_notes_on_all;
                    let mut widget = SpectrogramView::new(spectrogram, shared.style, shared.project.pitch_ref(), &shared.project.tuning)
                        .capo(shared.project.capo)
                        .audio_offset(offset)
                        .revision(shared.source.spectrogram_revision(id))
                        .playhead(shared.playhead_sec)
                        .height(height);
                    if notes_here {
                        widget = widget.notes(&shared.project.notes, shared.times, if is_source { shared.selected } else { &[] });
                    }
                    if is_source {
                        widget = widget.ghost(shared.ghost);
                    } else {
                        widget = widget.note_opacity(FAINT_NOTES);
                    }
                    let out = widget.show(&mut sui, &view, &pitch, &mut state.cache);
                    spectro_events.extend(out.events.into_iter().map(|e| match e {
                        SpectroEvent::AddNoteRequested { time_sec, .. } if !is_source => SpectroEvent::SeekRequested(time_sec),
                        other => other,
                    }));
                }
                None => {
                    ui.painter_at(spec_rect).rect_filled(spec_rect, 0.0, Color32::from_gray(12));
                }
            }
            let painter = ui.painter_at(spec_rect);
            painter.rect_filled(Rect::from_min_max(pos2(spec_rect.left(), spec_rect.bottom() - px(BORDER_PX)), spec_rect.max), 0.0, BORDER_COLOR);
        }

        if self.shown_rows > 0 && ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p)) {
            let key = Id::new(("wheel-rows", id));
            let added: f64 = ctx.input(|i| {
                let tuning = ViewTuning::DEFAULT;
                i.events.iter().map(|e| match e {
                    Event::MouseWheel { unit, delta, modifiers } if modifiers.alt => notches(*unit, *delta, &tuning).1,
                    _ => 0.0,
                }).sum()
            });
            let acc = ctx.data(|d| d.get_temp::<f64>(key).unwrap_or(0.0)) + added;
            let whole = acc.trunc();
            ctx.data_mut(|d| d.insert_temp(key, acc - whole));
            if whole != 0.0 {
                resized_rows = Some((clamp_rows(i64::from(self.configured_rows) + whole as i64), true));
            }
        }

        if self.shown_rows > 0 && self.shown_rows == self.configured_rows {
            let handle = Rect::from_min_max(pos2(wave_rect.left(), wave_rect.bottom() - RESIZE_HANDLE_PT), wave_rect.max);
            let response = ui.interact(handle, ui.id().with(("resize", id)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeVertical);
            let key = Id::new(("resize-drag", id));
            if response.drag_started() {
                ctx.data_mut(|d| d.insert_temp(key, (self.configured_rows, 0.0_f32)));
            }
            if let (true, Some((start, acc))) = (response.dragged(), ctx.data(|d| d.get_temp::<(u32, f32)>(key))) {
                let acc = acc + response.drag_delta().y;
                ctx.data_mut(|d| d.insert_temp(key, (start, acc)));
                resized_rows = Some((clamp_rows(i64::from(start) + (acc * ppp / STRIP_PITCH_PX as f32).round() as i64), false));
            }
        }

        let wave = WaveformView::new(px(WAVE_H_PX)).playhead(shared.playhead_sec).audio_offset(offset);
        wave.paint(ui, Rect::from_min_size(wave_rect.min, vec2(wave_rect.width(), px(WAVE_H_PX))), &view, |range, n| shared.source.peaks(id, range, n));
        let painter = ui.painter_at(wave_rect);
        painter.rect_filled(Rect::from_min_max(pos2(wave_rect.left(), wave_rect.bottom() - px(BORDER_PX)), wave_rect.max), 0.0, BORDER_COLOR);
        let x_of = |sec: f64| timeline_left + view.sec_to_x(sec);
        if self.shown_rows == 0 && is_source {
            for note in &shared.project.notes {
                if let Some(start) = shared.times.start_sec(note.id) {
                    painter.vline(x_of(start), wave_rect.top()..=wave_rect.top() + TICK_PT, Stroke::new(1.5_f32, NOTE_TICK));
                }
            }
        }
        if options.show_onsets && !is_source {
            for &onset in shared.source.onsets(id) {
                painter.vline(x_of(onset + offset), wave_rect.bottom() - TICK_PT..=wave_rect.bottom(), Stroke::new(1.0_f32, ONSET_TICK));
            }
        }
        ui.painter_at(rect).vline(timeline_left, rect.y_range(), Stroke::new(1.0_f32, BORDER_COLOR));

        if let Some(drag) = drag {
            let lane_painter = ui.painter_at(Rect::from_min_max(pos2(timeline_left, rect.top()), rect.max));
            let duration = shared.source.duration_sec(id);
            let outline = Rect::from_min_max(pos2(x_of(drag.start), rect.top()), pos2(x_of(drag.start + duration), wave_rect.bottom()));
            lane_painter.rect_stroke(outline, 0.0, Stroke::new(1.0_f32, white(120)), egui::StrokeKind::Inside);
            lane_painter.vline(x_of(offset), rect.y_range(), Stroke::new(1.5_f32, if drag.snapped { GUIDE_SNAPPED } else { white(200) }));
            if let Some(pointer) = ctx.pointer_latest_pos() {
                let galley = lane_painter.layout_no_wrap(super::offset::format_offset(offset), egui::FontId::monospace(12.0), Color32::WHITE);
                let size = galley.size() + vec2(10.0, 6.0);
                let at = place_readout(wave_rect, pointer, size);
                lane_painter.rect_filled(Rect::from_min_size(at, size), 3.0, Color32::from_black_alpha(210));
                lane_painter.galley(at + vec2(5.0, 3.0), galley, Color32::WHITE);
            }
        }

        let spectro_rect = (self.shown_rows > 0).then_some(spec_rect);
        LaneOutput { events, spectro: spectro_events, grip: header.grip, spectro_rect, resized_rows }
    }
}
